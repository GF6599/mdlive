use std::ffi::OsString;
use std::io::{self, Cursor, ErrorKind, Read};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use image::{DynamicImage, ImageFormat, ImageReader, RgbImage};

const EXTENSIONS: [&str; 4] = ["mp4", "webm", "mov", "mkv"];

/// The longest mdlive waits for ffmpeg to send the first frame or the next frame. mdlive waits on
/// its only thread, and ffmpeg can wait for ever on a named pipe or a stalled share. The limit
/// leaves room for a slow disk.
const TIME_LIMIT: Duration = Duration::from_secs(5);

pub fn is_video(path: &Path) -> bool {
    path.extension().is_some_and(|extension| {
        EXTENSIONS
            .iter()
            .any(|video| extension.eq_ignore_ascii_case(video))
    })
}

/// The ffmpeg program on `PATH`. mdlive reads video through it, and the Homebrew cask does not
/// install it, so a reader without it still reads every other page.
pub fn find_ffmpeg() -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|folder| folder.join("ffmpeg"))
        .find(|program| {
            program
                .metadata()
                .is_ok_and(|file| file.is_file() && file.permissions().mode() & 0o111 != 0)
        })
}

/// The first frame of the video at `path`. ffmpeg sends it as a PNG, and the PNG takes the same
/// memory limits as every other picture.
pub fn first_frame(ffmpeg: Option<&Path>, path: &Path) -> Result<DynamicImage, String> {
    let mut child = reader(ffmpeg, path)?
        .args(["-frames:v", "1", "-f", "image2pipe", "-c:v", "png", "-"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("cannot run ffmpeg: {error}"))?;
    let (stdout, stderr) = (child.stdout.take(), child.stderr.take());
    let (sender, output) = mpsc::channel();
    thread::spawn(move || {
        let mut png = Vec::new();
        let mut errors = String::new();
        let read = stdout
            .map_or(Ok(0), |mut pipe| pipe.read_to_end(&mut png))
            .and_then(|_| stderr.map_or(Ok(0), |mut pipe| pipe.read_to_string(&mut errors)));
        let _ = sender.send(read.map(|_| (png, errors)));
    });
    let Ok(read) = output.recv_timeout(TIME_LIMIT) else {
        stop(&mut child);
        return Err(no_answer());
    };
    let (png, errors) = read.map_err(|error| format!("cannot read ffmpeg: {error}"))?;
    let status = child
        .wait()
        .map_err(|error| format!("cannot read ffmpeg: {error}"))?;
    if !status.success() || png.is_empty() {
        let reason = errors.lines().last().unwrap_or("no frame");
        return Err(format!("ffmpeg cannot read the video: {reason}"));
    }
    ImageReader::with_format(Cursor::new(png), ImageFormat::Png)
        .decode()
        .map_err(|error| format!("cannot read the first frame: {error}"))
}

/// The frames of a video, which ffmpeg decodes and sends through a pipe. A thread reads them one
/// frame ahead of the view, so that a wait for ffmpeg can end at the time limit. A view that stops
/// taking frames leaves the thread and then ffmpeg waiting on a full pipe, so a pause costs
/// nothing.
pub struct Stream {
    ffmpeg: PathBuf,
    path: PathBuf,
    size: (u32, u32),
    bytes: usize,
    child: Child,
    frames: Frames,
}

impl Stream {
    pub fn start(ffmpeg: Option<&Path>, path: &Path, size: (u32, u32)) -> Result<Self, String> {
        let ffmpeg = ffmpeg.ok_or("ffmpeg not found")?;
        let bytes = usize::try_from(u64::from(size.0) * u64::from(size.1) * 3)
            .map_err(|_| "the video is too large")?;
        let (child, frames) = spawn(ffmpeg, path, size, bytes)?;
        Ok(Self {
            ffmpeg: ffmpeg.to_path_buf(),
            path: path.to_path_buf(),
            size,
            bytes,
            child,
            frames,
        })
    }

    /// The next frame, or `None` after the last.
    pub fn next_frame(&mut self) -> Result<Option<DynamicImage>, String> {
        let frame = match self.frames.recv_timeout(TIME_LIMIT) {
            Ok(Ok(frame)) => frame,
            Ok(Err(error)) => return Err(format!("cannot read the video: {error}")),
            Err(RecvTimeoutError::Disconnected) => return Ok(None),
            Err(RecvTimeoutError::Timeout) => return Err(no_answer()),
        };
        RgbImage::from_raw(self.size.0, self.size.1, frame)
            .map(|frame| Some(DynamicImage::ImageRgb8(frame)))
            .ok_or_else(|| "cannot read the video".to_string())
    }

    pub fn restart(&mut self) -> Result<(), String> {
        stop(&mut self.child);
        (self.child, self.frames) = spawn(&self.ffmpeg, &self.path, self.size, self.bytes)?;
        Ok(())
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        stop(&mut self.child);
    }
}

/// ffmpeg turns the frame rate of any video into this one, so every frame shows for the same time.
pub const FRAMES_PER_SECOND: u32 = 25;

type Frames = Receiver<io::Result<Vec<u8>>>;

fn spawn(
    ffmpeg: &Path,
    path: &Path,
    (wide, high): (u32, u32),
    bytes: usize,
) -> Result<(Child, Frames), String> {
    let mut child = reader(Some(ffmpeg), path)?
        .args(["-an", "-vf"])
        .arg(format!("fps={FRAMES_PER_SECOND},scale={wide}:{high}"))
        .args(["-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("cannot run ffmpeg: {error}"))?;
    let (sender, frames) = mpsc::sync_channel(1);
    if let Some(pipe) = child.stdout.take() {
        thread::spawn(move || send_frames(pipe, bytes, &sender));
    }
    Ok((child, frames))
}

/// Ends at the end of the video, when ffmpeg stops, or when the stream that takes the frames is
/// gone.
fn send_frames(
    mut pipe: ChildStdout,
    bytes: usize,
    sender: &mpsc::SyncSender<io::Result<Vec<u8>>>,
) {
    loop {
        let mut frame = vec![0; bytes];
        let read = match pipe.read_exact(&mut frame) {
            Err(error) if error.kind() == ErrorKind::UnexpectedEof => return,
            read => read.map(|()| frame),
        };
        let failed = read.is_err();
        if sender.send(read).is_err() || failed {
            return;
        }
    }
}

fn no_answer() -> String {
    format!("ffmpeg did not answer in {} s", TIME_LIMIT.as_secs())
}

/// A wait after the kill takes the process out of the process table.
fn stop(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// The ffmpeg command that opens the video at `path`, before the options for its output.
fn reader(ffmpeg: Option<&Path>, path: &Path) -> Result<Command, String> {
    let ffmpeg = ffmpeg.ok_or("ffmpeg not found")?;
    // The `file:` protocol keeps ffmpeg from reading a path as another of its protocols.
    let mut input = OsString::from("file:");
    input.push(path);
    let mut command = Command::new(ffmpeg);
    command
        .args(["-nostdin", "-v", "error"])
        // ffmpeg picks a reader from what a file holds, and a concat script or a playlist named
        // `.mp4` makes it open other files or addresses. These two limits keep it to the readers
        // of the four extensions, and to local files.
        .args(["-protocol_whitelist", "file", "-format_whitelist", "mov,matroska", "-i"])
        .arg(input)
        .stdin(Stdio::null());
    Ok(command)
}
