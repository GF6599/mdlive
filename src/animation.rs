use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::time::Duration;

use image::codecs::gif::GifDecoder;
use image::{
    AnimationDecoder, DynamicImage, Frame, Frames, ImageDecoder, ImageFormat, ImageReader, Limits,
};

use crate::video::{self, Stream};

/// The largest frame a video is played at. The terminal scales it to the view, and a larger frame
/// costs more bytes on each frame than the terminal reads in time: a 1280 by 720 frame of a busy
/// test video is 216 KB after zlib, 7 MB/s at 25 frames a second.
const LARGEST_VIDEO_FRAME: (u32, u32) = (1280, 720);
const VIDEO_FRAME: Duration = Duration::from_millis(1000 / video::FRAMES_PER_SECOND as u64);

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

/// An animated GIF or a video, decoded one frame at a time as each frame comes due, so that a long
/// one never sits in memory whole.
pub struct Animation {
    reader: Reader,
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
        Self::start(Reader::Gif {
            path: path.to_path_buf(),
            frames: frames(path)?,
        })
        .map(Some)
    }

    /// The animation of the video at `path`, whose own frames are `size` pixels.
    pub fn video(ffmpeg: Option<&Path>, path: &Path, size: (u32, u32)) -> Result<Self, String> {
        let stream = Stream::start(ffmpeg, path, fit(size, LARGEST_VIDEO_FRAME))?;
        Self::start(Reader::Video(stream))
    }

    fn start(reader: Reader) -> Result<Self, String> {
        let mut animation = Self {
            reader,
            shown: DynamicImage::new_rgba8(1, 1),
            number: 0,
            time_left: Duration::ZERO,
            paused: false,
        };
        let (first, delay) = animation.reader.next_frame()?;
        animation.show(first, delay);
        Ok(animation)
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
    /// shows late, and the frames after it keep their own delays, so a slow terminal slows the
    /// animation down and skips no frame. A frame that cannot be read pauses the animation, so
    /// that the reader sees the reason once and not at each frame.
    pub fn play(&mut self, elapsed: Duration) -> Result<bool, String> {
        if self.paused {
            return Ok(false);
        }
        if elapsed < self.time_left {
            self.time_left -= elapsed;
            return Ok(false);
        }
        match self.reader.next_frame() {
            Ok((frame, delay)) => {
                self.show(frame, delay);
                Ok(true)
            }
            Err(reason) => {
                self.paused = true;
                Err(reason)
            }
        }
    }

    fn show(&mut self, frame: DynamicImage, delay: Duration) {
        self.time_left = delay;
        self.shown = frame;
        self.number += 1;
    }
}

enum Reader {
    Gif {
        path: PathBuf,
        frames: Frames<'static>,
    },
    Video(Stream),
}

impl Reader {
    /// The next frame and the time it shows, from the first frame again after the last.
    fn next_frame(&mut self) -> Result<(DynamicImage, Duration), String> {
        match self {
            Self::Gif { path, frames } => {
                let frame = match next_gif_frame(frames)? {
                    Some(frame) => frame,
                    None => {
                        *frames = self::frames(path)?;
                        next_gif_frame(frames)?.ok_or("the GIF has no frames")?
                    }
                };
                let delay = Duration::from(frame.delay());
                let delay = if delay < SHORTEST_DELAY {
                    DELAY_OF_A_SHORTER_FRAME
                } else {
                    delay
                };
                Ok((DynamicImage::ImageRgba8(frame.into_buffer()), delay))
            }
            Self::Video(stream) => {
                let frame = match stream.next_frame()? {
                    Some(frame) => frame,
                    None => {
                        stream.restart()?;
                        stream.next_frame()?.ok_or("the video has no frames")?
                    }
                };
                Ok((frame, VIDEO_FRAME))
            }
        }
    }
}

/// `size` shrunk to fit inside `largest` with its own shape, and never grown.
fn fit((wide, high): (u32, u32), largest: (u32, u32)) -> (u32, u32) {
    let scale = (f64::from(largest.0) / f64::from(wide.max(1)))
        .min(f64::from(largest.1) / f64::from(high.max(1)))
        .min(1.0);
    let shrink = |side: u32| ((f64::from(side) * scale).round() as u32).max(1);
    (shrink(wide), shrink(high))
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

fn next_gif_frame(frames: &mut Frames<'static>) -> Result<Option<Frame>, String> {
    frames
        .next()
        .transpose()
        .map_err(|error| format!("cannot read the next frame: {error}"))
}
