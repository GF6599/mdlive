use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::time::Duration;

use image::codecs::gif::GifDecoder;
use image::{
    AnimationDecoder, DynamicImage, Frame, Frames, ImageDecoder, ImageFormat, ImageReader, Limits,
};

/// Browsers show a GIF frame that asks for less than 20 ms for 100 ms, and GIF makers write such
/// delays for that, so mdlive does the same.
const SHORTEST_DELAY: Duration = Duration::from_millis(20);
const DELAY_OF_A_SHORTER_FRAME: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Playback {
    Still,
    Playing,
    Paused,
}

/// An animated GIF, decoded one frame at a time as each frame comes due, so that a long GIF never
/// sits in memory whole.
pub struct Animation {
    path: PathBuf,
    frames: Frames<'static>,
    shown: DynamicImage,
    /// Counts the frames shown, so that a drawing of the view knows its frame is out of date.
    number: u64,
    time_left: Duration,
    paused: bool,
}

impl Animation {
    /// The animation of the picture at `path`, or `None` for a picture that is not a GIF of two
    /// frames or more.
    pub fn open(path: &Path) -> Result<Option<Self>, String> {
        let format = ImageReader::open(path)
            .and_then(ImageReader::with_guessed_format)
            .map_err(|error| format!("cannot read: {error}"))?
            .format();
        if format != Some(ImageFormat::Gif) || frames(path)?.take(2).count() < 2 {
            return Ok(None);
        }
        let mut frames = frames(path)?;
        let first = next_frame(&mut frames)?.ok_or("the GIF has no frames")?;
        let mut animation = Self {
            path: path.to_path_buf(),
            frames,
            shown: DynamicImage::new_rgba8(1, 1),
            number: 0,
            time_left: Duration::ZERO,
            paused: false,
        };
        animation.show(first);
        Ok(Some(animation))
    }

    pub fn frame(&self) -> (u64, &DynamicImage) {
        (self.number, &self.shown)
    }

    pub fn playback(&self) -> Playback {
        if self.paused {
            Playback::Paused
        } else {
            Playback::Playing
        }
    }

    pub fn until_next_frame(&self) -> Option<Duration> {
        (!self.paused).then_some(self.time_left)
    }

    pub fn toggle_pause(&mut self) {
        self.paused = !self.paused;
    }

    /// Lets `elapsed` pass, and says whether the next frame came due. A frame that comes due late
    /// shows late, and the frames after it keep their own delays, so a slow terminal slows the GIF
    /// down and skips no frame. A frame that cannot be read pauses the GIF, so that the reader
    /// sees the reason once and not at each frame.
    pub fn play(&mut self, elapsed: Duration) -> Result<bool, String> {
        if self.paused {
            return Ok(false);
        }
        if elapsed < self.time_left {
            self.time_left -= elapsed;
            return Ok(false);
        }
        match self.next_frame() {
            Ok(frame) => {
                self.show(frame);
                Ok(true)
            }
            Err(reason) => {
                self.paused = true;
                Err(reason)
            }
        }
    }

    fn next_frame(&mut self) -> Result<Frame, String> {
        if let Some(frame) = next_frame(&mut self.frames)? {
            return Ok(frame);
        }
        self.frames = frames(&self.path)?;
        next_frame(&mut self.frames)?.ok_or_else(|| "the GIF has no frames".to_string())
    }

    fn show(&mut self, frame: Frame) {
        let delay = Duration::from(frame.delay());
        self.time_left = if delay < SHORTEST_DELAY {
            DELAY_OF_A_SHORTER_FRAME
        } else {
            delay
        };
        self.shown = DynamicImage::ImageRgba8(frame.into_buffer());
        self.number += 1;
    }
}

/// The decoder takes the limits that `ImageReader` gives every other picture, because `GifDecoder`
/// on its own has none, and a GIF header can name a canvas of 17 GB a frame in a few bytes.
fn frames(path: &Path) -> Result<Frames<'static>, String> {
    let file = File::open(path).map_err(|error| format!("cannot read: {error}"))?;
    let mut decoder =
        GifDecoder::new(BufReader::new(file)).map_err(|error| format!("cannot read: {error}"))?;
    decoder
        .set_limits(Limits::default())
        .map_err(|error| format!("cannot read: {error}"))?;
    Ok(decoder.into_frames())
}

fn next_frame(frames: &mut Frames<'static>) -> Result<Option<Frame>, String> {
    frames
        .next()
        .transpose()
        .map_err(|error| format!("cannot read the next frame: {error}"))
}
