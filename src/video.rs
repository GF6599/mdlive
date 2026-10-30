use std::ffi::OsString;
use std::io::{Cursor, ErrorKind, Read};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use image::{DynamicImage, ImageFormat, ImageReader, RgbImage};

const EXTENSIONS: [&str; 4] = ["mp4", "webm", "mov", "mkv"];

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
    let output = reader(ffmpeg, path)?
        .args(["-frames:v", "1", "-f", "image2pipe", "-c:v", "png", "-"])
        .output()
        .map_err(|error| format!("cannot run ffmpeg: {error}"))?;
    if !output.status.success() || output.stdout.is_empty() {
        let reason = String::from_utf8_lossy(&output.stderr);
        let reason = reason.lines().last().unwrap_or("no frame");
        return Err(format!("ffmpeg cannot read the video: {reason}"));
    }
    ImageReader::with_format(Cursor::new(output.stdout), ImageFormat::Png)
        .decode()
        .map_err(|error| format!("cannot read the first frame: {error}"))
}

/// The frames of a video, which ffmpeg decodes and sends through a pipe as they are read. A reader
/// that stops reading leaves ffmpeg waiting on the full pipe, so a pause costs nothing.
pub struct Stream {
    ffmpeg: PathBuf,
    path: PathBuf,
    size: (u32, u32),
    child: Child,
    frame: Vec<u8>,
}

impl Stream {
    pub fn start(ffmpeg: Option<&Path>, path: &Path, size: (u32, u32)) -> Result<Self, String> {
        let ffmpeg = ffmpeg.ok_or("ffmpeg not found")?;
        let child = spawn(ffmpeg, path, size)?;
        let bytes = usize::try_from(u64::from(size.0) * u64::from(size.1) * 3)
            .map_err(|_| "the video is too large")?;
        Ok(Self {
            ffmpeg: ffmpeg.to_path_buf(),
            path: path.to_path_buf(),
            size,
            child,
            frame: vec![0; bytes],
        })
    }

    pub fn next_frame(&mut self) -> Result<Option<DynamicImage>, String> {
        let Some(pipe) = self.child.stdout.as_mut() else {
            return Ok(None);
        };
        match pipe.read_exact(&mut self.frame) {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::UnexpectedEof => return Ok(None),
            Err(error) => return Err(format!("cannot read the video: {error}")),
        }
        RgbImage::from_raw(self.size.0, self.size.1, self.frame.clone())
            .map(|frame| Some(DynamicImage::ImageRgb8(frame)))
            .ok_or_else(|| "cannot read the video".to_string())
    }

    pub fn restart(&mut self) -> Result<(), String> {
        stop(&mut self.child);
        self.child = spawn(&self.ffmpeg, &self.path, self.size)?;
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

fn spawn(ffmpeg: &Path, path: &Path, (wide, high): (u32, u32)) -> Result<Child, String> {
    reader(Some(ffmpeg), path)?
        .args(["-an", "-vf"])
        .arg(format!("fps={FRAMES_PER_SECOND},scale={wide}:{high}"))
        .args(["-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("cannot run ffmpeg: {error}"))
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
