use std::ffi::OsString;
use std::io::Cursor;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use image::{DynamicImage, ImageFormat, ImageReader};

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
    let ffmpeg = ffmpeg.ok_or("ffmpeg not found")?;
    // The `file:` protocol keeps ffmpeg from reading a path as another of its protocols.
    let mut input = OsString::from("file:");
    input.push(path);
    let output = Command::new(ffmpeg)
        .args(["-nostdin", "-v", "error", "-i"])
        .arg(input)
        .args(["-frames:v", "1", "-f", "image2pipe", "-c:v", "png", "-"])
        .stdin(Stdio::null())
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
