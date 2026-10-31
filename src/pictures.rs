use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, SystemTime};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use flate2::Compression;
use flate2::write::ZlibEncoder;
use image::imageops::{self, FilterType};
use image::{DynamicImage, ImageReader, Rgba, RgbaImage};
use ratatui::Frame;
use ratatui::layout::{Rect, Size};
use ratatui::style::Color;
use ratatui::text::Span;
use ratatui::widgets::{Clear, Paragraph};
use ratatui_image::picker::{Picker, ProtocolType};
use ratatui_image::sliced::{SignedPosition, SlicedImage, SlicedProtocol};
use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg;

use crate::animation::{Animation, Playback};
use crate::picture_view::{PictureView, Window};
use crate::theme::Palette;
use crate::video;

/// A picture sized to fit the text, in cells.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Placed {
    pub source: Source,
    pub columns: u16,
    pub rows: u16,
}

/// Where the pixels of a picture come from.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Source {
    File(PathBuf),
    Svg(Arc<str>),
}

/// The raster picture the terminal holds for the view.
struct Held {
    path: PathBuf,
    /// The number of the frame the terminal holds, or `None` when it may have dropped the data.
    frame: Option<u64>,
    /// The Kitty image id of the picture.
    image: u32,
    /// The size of the picture in pixels, or why it cannot be sent.
    sent: Result<(u32, u32), String>,
}

/// The size of each video by its path and the time it was last written, because the page is laid
/// out again on each fold, resize and panel, and each answer costs a run of ffmpeg.
type VideoSizes = HashMap<(PathBuf, SystemTime), Result<(u32, u32), String>>;

/// Where the pictures of a page come from, and how the terminal draws them.
pub struct Pictures {
    base: PathBuf,
    picker: Picker,
    loaded: HashMap<Placed, Result<SlicedProtocol, String>>,
    /// The pixels of each picture in `loaded` at its size on the page, so that a picture can be
    /// sent to the terminal again without a second decode.
    pixels: HashMap<Placed, DynamicImage>,
    /// The last drawing of the picture view, with the number of its frame, drawn again only when
    /// the part shown or the frame changes.
    view: Option<(Source, Window, u64, Result<SlicedProtocol, String>)>,
    /// The pixels of the raster picture in the view, decoded once for every frame of it.
    decoded: Option<(PathBuf, DynamicImage)>,
    animation: Option<Animation>,
    held: Option<Held>,
    /// The Kitty commands for the picture the terminal holds, which go out after the cells of the
    /// next frame.
    commands: String,
    ffmpeg: Option<PathBuf>,
    video_sizes: RefCell<VideoSizes>,
}

impl fmt::Debug for Pictures {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Pictures")
            .field("base", &self.base)
            .field("protocol", &self.picker.protocol_type())
            .field("loaded", &self.loaded.len())
            .finish()
    }
}

impl Pictures {
    /// Pictures at relative paths are read from `base`, the folder of the Markdown file.
    pub fn new(base: &Path, picker: Picker) -> Self {
        let mut pictures = Self {
            base: PathBuf::new(),
            picker: Picker::halfblocks(),
            loaded: HashMap::new(),
            pixels: HashMap::new(),
            view: None,
            decoded: None,
            animation: None,
            held: None,
            commands: String::new(),
            ffmpeg: video::find_ffmpeg(),
            video_sizes: RefCell::new(HashMap::new()),
        };
        pictures.set_base(base);
        pictures.set_picker(picker);
        pictures
    }

    /// Draws with `picker` from now on. The loaded pictures go, because their key holds no cell
    /// size.
    pub fn set_picker(&mut self, picker: Picker) {
        self.picker = picker;
        self.loaded.clear();
        self.pixels.clear();
        self.view = None;
    }

    pub fn set_ffmpeg(&mut self, ffmpeg: Option<PathBuf>) {
        self.ffmpeg = ffmpeg;
    }

    /// Reads relative paths from `base` from now on.
    pub fn set_base(&mut self, base: &Path) {
        self.base = base.canonicalize().unwrap_or_else(|_| base.to_path_buf());
    }

    /// Sizes the picture at `url` to fit `width` columns, or says why it cannot be drawn. Only the
    /// header of the file is read, and a remote picture is never fetched, so laying out a page
    /// stays fast and makes no network call.
    pub fn place(&self, url: &str, width: usize) -> Result<Placed, String> {
        if has_scheme(url) {
            return Err("remote image, not fetched".to_string());
        }
        let path = self
            .resolve(url)
            .ok_or_else(|| format!("not found: {url}"))?;
        let source = Source::File(path);
        let (pixels_wide, pixels_high) = self.pixel_size(&source)?;
        Ok(self.fit(source, pixels_wide, pixels_high, width))
    }

    /// Sizes an SVG held in memory to fit `width` columns.
    pub fn place_svg(&self, svg: Arc<str>, width: usize) -> Result<Placed, String> {
        let source = Source::Svg(svg);
        let (pixels_wide, pixels_high) = self.pixel_size(&source)?;
        Ok(self.fit(source, pixels_wide, pixels_high, width))
    }

    /// Whether the terminal draws real pixels, rather than half blocks that blur small text.
    pub fn has_graphics(&self) -> bool {
        self.picker.protocol_type() != ProtocolType::Halfblocks
    }

    /// The cells a picture takes at the size a docs site shows it, shrunk to `width` columns when
    /// it is wider, and shrunk further when it would hold more than the pixel budget.
    fn fit(&self, source: Source, pixels_wide: u32, pixels_high: u32, width: usize) -> Placed {
        let font = self.picker.font_size();
        let (cell_wide, cell_high) = (u64::from(font.width.max(1)), u64::from(font.height.max(1)));
        let on_screen = |pixels: u32| {
            (u64::from(pixels.max(1)) * cell_wide).div_ceil(PICTURE_PIXELS_PER_COLUMN)
        };
        let (wide, high) = (on_screen(pixels_wide), on_screen(pixels_high));
        let room = u64::try_from(width.max(1))
            .unwrap_or(u64::MAX)
            .saturating_mul(cell_wide);
        let (wide, high) = if wide > room {
            (room, high.saturating_mul(room).div_ceil(wide))
        } else {
            (wide, high)
        };
        let (wide, high) = if wide.saturating_mul(high) > MAX_PICTURE_PIXELS {
            let narrow = (MAX_PICTURE_PIXELS.saturating_mul(wide) / high)
                .isqrt()
                .max(1);
            (narrow, (MAX_PICTURE_PIXELS / narrow).max(1))
        } else {
            (wide, high)
        };
        let cells = |pixels: u64, cell: u64| {
            u16::try_from(pixels.div_ceil(cell).clamp(1, MAX_ROWS)).unwrap_or(1)
        };
        Placed {
            source,
            columns: cells(wide, cell_wide),
            rows: cells(high, cell_high),
        }
    }

    /// The file a picture address names. A site-root address such as `/diagrams/a.svg` is found
    /// the way a docs site serves it, in `public/` or `static/` or at the root of a folder above the
    /// page, and the search stops at the repository so it never reaches an unrelated project.
    fn resolve(&self, url: &str) -> Option<PathBuf> {
        let Some(rooted) = url.strip_prefix('/') else {
            let path = self.base.join(url);
            return path.exists().then_some(path);
        };
        let absolute = PathBuf::from(url);
        if absolute.exists() {
            return Some(absolute);
        }
        for folder in self.base.ancestors() {
            let found = SITE_ROOTS
                .iter()
                .map(|root| folder.join(root).join(rooted))
                .find(|candidate| candidate.exists());
            if found.is_some() {
                return found;
            }
            if folder.join(".git").exists() {
                break;
            }
        }
        None
    }

    /// Draws a placed picture into `area` with its top left corner at `position` relative to
    /// `area`, which is above the area when the picture is scrolled partly out of view.
    pub fn draw(
        &mut self,
        frame: &mut Frame,
        placed: &Placed,
        area: Rect,
        position: SignedPosition,
        palette: &Palette,
    ) {
        let picker = &self.picker;
        let ffmpeg = self.ffmpeg.as_deref();
        let pixels = &mut self.pixels;
        let loaded = self
            .loaded
            .entry(placed.clone())
            .or_insert_with(|| load(picker, ffmpeg, pixels, placed, palette));
        match loaded {
            Ok(protocol) => frame.render_widget(SlicedImage::new(protocol, position), area),
            Err(reason) => {
                let (Ok(x), Ok(y)) = (u16::try_from(position.x), u16::try_from(position.y.max(0)))
                else {
                    return;
                };
                let line = Rect {
                    x: area.x.saturating_add(x),
                    y: area.y.saturating_add(y),
                    height: 1,
                    ..area
                }
                .intersection(area);
                frame.render_widget(
                    Paragraph::new(Span::styled(reason.as_str(), palette.picture_note())),
                    line,
                );
            }
        }
    }

    /// Draws the part of the picture `view` that fits `area`, in the middle of it. An SVG is drawn
    /// again at each zoom so that it stays sharp.
    pub fn draw_view(
        &mut self,
        frame: &mut Frame,
        view: &mut PictureView,
        area: Rect,
        palette: &Palette,
    ) {
        let font = self.picker.font_size();
        let window = view.frame(area.width, area.height, (font.width, font.height));
        let shown = Rect {
            x: area.x + area.width.saturating_sub(window.columns) / 2,
            y: area.y + area.height.saturating_sub(window.rows) / 2,
            width: window.columns,
            height: window.rows,
        }
        .intersection(area);
        // Kitty reads a placement of no cells as the natural size of the picture.
        if shown.is_empty() {
            return;
        }
        if let Some(path) = self.placed_directly(&view.source) {
            let replaced = self.hold(&path);
            let Some(held) = &self.held else {
                return;
            };
            match &held.sent {
                Ok(size) => {
                    frame.render_widget(Clear, shown);
                    self.commands
                        .push_str(&place(held.image, *size, &window, shown));
                    if let Some(replaced) = replaced {
                        self.commands.push_str(&delete(replaced));
                    }
                }
                Err(reason) => frame.render_widget(
                    Paragraph::new(Span::styled(reason.as_str(), palette.picture_note())),
                    area,
                ),
            }
            return;
        }
        let number = self.frame_number();
        let current = self.view.as_ref().is_some_and(|(source, shown, drawn, _)| {
            *source == view.source && *shown == window && *drawn == number
        });
        if !current {
            let drawn = self.draw_window(&view.source, &window, palette);
            self.view = Some((view.source.clone(), window, number, drawn));
        }
        let Some((_, _, _, drawn)) = &mut self.view else {
            return;
        };
        match drawn {
            Ok(protocol) => frame.render_widget(
                SlicedImage::new(protocol, SignedPosition { x: 0, y: 0 }),
                shown,
            ),
            Err(reason) => frame.render_widget(
                Paragraph::new(Span::styled(reason.as_str(), palette.picture_note())),
                area,
            ),
        }
    }

    /// The terminal may drop the data of the page's pictures while the view covers them, and a
    /// picture is sent only once, so the page sends each one again.
    pub fn close_view(&mut self) {
        self.view = None;
        self.decoded = None;
        self.animation = None;
        self.loaded.clear();
        if let Some(Held {
            image, sent: Ok(_), ..
        }) = self.held.take()
        {
            self.commands.push_str(&delete(image));
        }
    }

    /// Starts to play the picture at `source` in the view, when it is an animated GIF or a video.
    pub fn open_view(&mut self, source: &Source) -> Result<(), String> {
        self.animation = match source {
            Source::File(path) if video::is_video(path) => Some(Animation::video(
                self.ffmpeg.as_deref(),
                path,
                self.pixel_size(source)?,
            )?),
            Source::File(path) if !is_svg(path) => Animation::open(path)?,
            _ => None,
        };
        Ok(())
    }

    pub fn playback(&self) -> Playback {
        self.animation
            .as_ref()
            .map_or(Playback::Still, Animation::playback)
    }

    pub fn until_next_frame(&self) -> Option<Duration> {
        self.animation.as_ref()?.until_next_frame()
    }

    pub fn toggle_pause(&mut self) {
        if let Some(animation) = &mut self.animation {
            animation.toggle_pause();
        }
    }

    pub fn play(&mut self, elapsed: Duration) -> Result<bool, String> {
        self.animation
            .as_mut()
            .map_or(Ok(false), |animation| animation.play(elapsed))
    }

    /// A cleared screen loses the placement of the view, and the terminal may then drop the data
    /// of its picture, so the next frame sends the picture again.
    pub fn screen_cleared(&mut self) {
        if let Some(held) = &mut self.held {
            held.frame = None;
        }
    }

    fn frame_number(&self) -> u64 {
        self.animation
            .as_ref()
            .map_or(0, |animation| animation.frame().0)
    }

    pub fn take_commands(&mut self) -> String {
        std::mem::take(&mut self.commands)
    }

    /// The file of a raster picture that the view places directly from the data the terminal
    /// holds. An SVG is drawn again at each zoom so that it stays sharp. tmux cannot put a direct
    /// placement at its own cursor, so inside it the view sends each frame as a new picture.
    fn placed_directly(&self, source: &Source) -> Option<PathBuf> {
        let kitty = self.picker.protocol_type() == ProtocolType::Kitty;
        match source {
            Source::File(path) if kitty && !self.picker.tmux_detected() && !is_svg(path) => {
                Some(path.clone())
            }
            _ => None,
        }
    }

    /// Sends the frame on show of the picture at `path` to the terminal, unless the terminal holds
    /// it already. A new frame goes under the other of the two view ids, and the id of the frame
    /// it replaces comes back, to delete once the new frame is placed.
    fn hold(&mut self, path: &Path) -> Option<u32> {
        let number = self.frame_number();
        let replaced = match &self.held {
            Some(held) if held.path == path && held.frame == Some(number) => return None,
            Some(Held {
                image, sent: Ok(_), ..
            }) => Some(*image),
            _ => None,
        };
        let image = match replaced {
            Some(VIEW_IMAGE) => OTHER_VIEW_IMAGE,
            _ => VIEW_IMAGE,
        };
        let decoded;
        let pixels = match &self.animation {
            Some(animation) => Ok(animation.frame().1),
            None => {
                decoded = decode(path, self.ffmpeg.as_deref());
                decoded.as_ref().map_err(String::clone)
            }
        };
        let sent = pixels.map(|pixels| {
            self.commands.push_str(&transmit(pixels, image));
            (pixels.width(), pixels.height())
        });
        self.held = Some(Held {
            path: path.to_path_buf(),
            frame: Some(number),
            image,
            sent,
        });
        replaced
    }

    fn draw_window(
        &mut self,
        source: &Source,
        window: &Window,
        palette: &Palette,
    ) -> Result<SlicedProtocol, String> {
        let image = match source {
            Source::Svg(svg) => draw_svg_window(svg.as_bytes(), None, window)?,
            Source::File(path) if is_svg(path) => {
                let data = std::fs::read(path).map_err(|error| format!("cannot read: {error}"))?;
                draw_svg_window(&data, path.parent(), window)?
            }
            Source::File(_) if let Some(animation) = &self.animation => {
                crop_window(animation.frame().1, window)
            }
            Source::File(path) => {
                if self
                    .decoded
                    .as_ref()
                    .is_none_or(|(decoded, _)| decoded != path)
                {
                    self.decoded = Some((path.clone(), decode(path, self.ffmpeg.as_deref())?));
                }
                let Some((_, image)) = &self.decoded else {
                    return Err("cannot read the picture".to_string());
                };
                crop_window(image, window)
            }
        };
        encode(
            &self.picker,
            image,
            Size::new(window.columns, window.rows),
            palette,
        )
    }

    /// The size of a picture in its own pixels. A video has no header that image readers know, so
    /// its size is the size of its first frame.
    pub fn pixel_size(&self, source: &Source) -> Result<(u32, u32), String> {
        let Source::File(path) = source else {
            return stated_size(source);
        };
        if !video::is_video(path) {
            return stated_size(source);
        }
        let written = std::fs::metadata(path)
            .and_then(|file| file.modified())
            .map_err(|error| format!("cannot read: {error}"))?;
        let key = (path.clone(), written);
        if let Some(size) = self.video_sizes.borrow().get(&key) {
            return size.clone();
        }
        let size = video::first_frame(self.ffmpeg.as_deref(), path)
            .map(|frame| (frame.width(), frame.height()));
        self.video_sizes.borrow_mut().insert(key, size.clone());
        size
    }

    /// Forgets the pictures a page no longer shows at these sizes.
    pub fn keep_only<'a>(&mut self, shown: impl Iterator<Item = &'a Placed>) {
        let shown: HashSet<&Placed> = shown.collect();
        self.loaded.retain(|placed, _| shown.contains(placed));
        self.pixels.retain(|placed, _| shown.contains(placed));
    }
}

/// The folders a docs site serves from its root: Next.js and Vite use `public`, Docusaurus uses
/// `static`, and the VS Code preview uses the workspace folder itself.
const SITE_ROOTS: [&str; 3] = ["public", "static", ""];

/// A browser lays out text about 8 CSS pixels to a monospace column, and a picture pixel is a CSS
/// pixel, so this keeps a picture as wide against the text as the docs site shows it, whatever
/// the screen density and font size.
const PICTURE_PIXELS_PER_COLUMN: u64 = 8;

/// The most pixels the cells of one picture may hold, 2048 squared, which is 16 MiB as RGBA. That
/// still holds a picture across a full 100-column text on a Retina screen and 50 to 80 rows tall,
/// by the font size.
/// Drawing renders every pixel of those cells, so the budget, not the size a file declares, is
/// what bounds the memory one picture takes.
const MAX_PICTURE_PIXELS: u64 = 2048 * 2048;

/// A picture taller than this many rows is cut, because the sliced drawing positions rows with a
/// signed 16-bit offset.
const MAX_ROWS: u64 = 10_000;

fn load(
    picker: &Picker,
    ffmpeg: Option<&Path>,
    pixels: &mut HashMap<Placed, DynamicImage>,
    placed: &Placed,
    palette: &Palette,
) -> Result<SlicedProtocol, String> {
    let image = match pixels.get(placed) {
        Some(image) => image.clone(),
        None => {
            let image = page_pixels(picker, ffmpeg, placed)?;
            pixels.insert(placed.clone(), image.clone());
            image
        }
    };
    encode(
        picker,
        image,
        Size::new(placed.columns, placed.rows),
        palette,
    )
}

/// Half blocks and sixel have no clear pixel, so the transparent parts of a picture take the page
/// colour there. Kitty and iTerm2 keep them clear, and the terminal background shows through.
fn encode(
    picker: &Picker,
    image: DynamicImage,
    size: Size,
    palette: &Palette,
) -> Result<SlicedProtocol, String> {
    let image = match picker.protocol_type() {
        ProtocolType::Halfblocks | ProtocolType::Sixel if image.color().has_alpha() => {
            on_page_colour(&image, palette.base)
        }
        _ => image,
    };
    SlicedProtocol::new(picker, image, Some(size)).map_err(|error| format!("cannot draw: {error}"))
}

fn on_page_colour(image: &DynamicImage, colour: Color) -> DynamicImage {
    let Color::Rgb(r, g, b) = colour else {
        return image.clone();
    };
    let mut ground = RgbaImage::from_pixel(image.width(), image.height(), Rgba([r, g, b, u8::MAX]));
    imageops::overlay(&mut ground, image, 0, 0);
    DynamicImage::ImageRgba8(ground)
}

/// The pixels of a placed picture at the size of its cells.
fn page_pixels(
    picker: &Picker,
    ffmpeg: Option<&Path>,
    placed: &Placed,
) -> Result<DynamicImage, String> {
    let font = picker.font_size();
    let wide = u32::from(placed.columns) * u32::from(font.width);
    let high = u32::from(placed.rows) * u32::from(font.height);
    let image = match &placed.source {
        Source::Svg(svg) => draw_svg(svg.as_bytes(), None, wide, high)?,
        Source::File(path) if is_svg(path) => {
            let data = std::fs::read(path).map_err(|error| format!("cannot read: {error}"))?;
            draw_svg(&data, path.parent(), wide, high)?
        }
        Source::File(path) => decode(path, ffmpeg)?,
    };
    Ok(if image.width() != wide {
        image.resize(wide, high, FilterType::Triangle)
    } else {
        image
    })
}

/// The size of a picture in its own pixels, from the header of a file or the size an SVG names.
fn stated_size(source: &Source) -> Result<(u32, u32), String> {
    match source {
        Source::Svg(svg) => Ok(svg_size(&parse_svg(
            svg.as_bytes(),
            &usvg::Options::default(),
        )?)),
        Source::File(path) if is_svg(path) => {
            let data = std::fs::read(path).map_err(|error| format!("cannot read: {error}"))?;
            Ok(svg_size(&parse_svg(&data, &usvg::Options::default())?))
        }
        Source::File(path) => ImageReader::open(path)
            .and_then(ImageReader::with_guessed_format)
            .map_err(|error| error.to_string())
            .and_then(|reader| reader.into_dimensions().map_err(|error| error.to_string()))
            .map_err(|error| format!("cannot read: {error}")),
    }
}

fn decode(path: &Path, ffmpeg: Option<&Path>) -> Result<DynamicImage, String> {
    if video::is_video(path) {
        return video::first_frame(ffmpeg, path);
    }
    ImageReader::open(path)
        .and_then(ImageReader::with_guessed_format)
        .map_err(|error| error.to_string())
        .and_then(|reader| reader.decode().map_err(|error| error.to_string()))
        .map_err(|error| format!("cannot read: {error}"))
}

/// The part of `image` that `window` shows, at the pixels the frame draws.
fn crop_window(image: &DynamicImage, window: &Window) -> DynamicImage {
    let (x, y, wide, high) = source_rect((image.width(), image.height()), window);
    image.crop_imm(x, y, wide, high).resize_exact(
        window.pixels_wide,
        window.pixels_high,
        FilterType::Triangle,
    )
}

/// The whole pixels of a picture of `size` that `window` shows: the left, the top, the width and
/// the height.
fn source_rect((width, height): (u32, u32), window: &Window) -> (u32, u32, u32, u32) {
    let x = (window.x.floor() as u32).min(width.saturating_sub(1));
    let y = (window.y.floor() as u32).min(height.saturating_sub(1));
    let wide = ((window.x + window.wide).ceil() as u32).clamp(x + 1, width) - x;
    let high = ((window.y + window.high).ceil() as u32).clamp(y + 1, height) - y;
    (x, y, wide, high)
}

/// The Kitty image id of the picture the view holds. ratatui-image picks a random id for each
/// picture of the page, so a fixed id meets one of them with a chance of one in four billion.
const VIEW_IMAGE: u32 = 0x00ED_1A0F;
/// The id of every second frame of a GIF. The terminal leaves the screen blank for a moment when
/// a picture is sent again under its own id, so a frame goes under the id the screen does not
/// show.
const OTHER_VIEW_IMAGE: u32 = VIEW_IMAGE + 1;
/// The terminal replaces a placement that has the id of an earlier one, so each frame of the view
/// takes the place of the frame before it.
const VIEW_PLACEMENT: u32 = 1;
/// The protocol takes at most 4096 bytes of base64 in one escape.
const CHUNK: usize = 4096 / 4 * 3;

fn kitty(control: &str, payload: &str) -> String {
    format!("\x1b_G{control};{payload}\x1b\\")
}

/// A picture with no clear pixel goes as RGB, a quarter fewer bytes than RGBA. Ghostty also shows
/// each frame of a GIF with a flash when the frame goes as RGBA. The pixels go through zlib, which
/// takes a GIF frame of a recorded terminal from 3.6 MB down to about 100 KB.
fn transmit(image: &DynamicImage, id: u32) -> String {
    let (format, pixels) = if has_clear_pixel(image) {
        (32, image.to_rgba8().into_raw())
    } else {
        (24, image.to_rgb8().into_raw())
    };
    let bytes = compress(&pixels);
    let chunks: Vec<&[u8]> = bytes.chunks(CHUNK).collect();
    let mut escapes = String::with_capacity(bytes.len() / 3 * 4 + chunks.len() * 16 + 64);
    for (index, chunk) in chunks.iter().enumerate() {
        let more = u8::from(index + 1 < chunks.len());
        let control = if index == 0 {
            format!(
                "a=t,f={format},o=z,s={},v={},i={id},q=2,m={more}",
                image.width(),
                image.height()
            )
        } else {
            format!("m={more}")
        };
        escapes.push_str(&kitty(&control, &STANDARD.encode(chunk)));
    }
    escapes
}

/// A GIF decoder gives every frame as RGBA, even a frame with no clear pixel.
fn has_clear_pixel(image: &DynamicImage) -> bool {
    match image {
        DynamicImage::ImageRgba8(pixels) => pixels.pixels().any(|pixel| pixel[3] < u8::MAX),
        other => other.color().has_alpha(),
    }
}

fn compress(pixels: &[u8]) -> Vec<u8> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
    // A write to a Vec cannot fail.
    let _ = encoder.write_all(pixels);
    encoder.finish().unwrap_or_default()
}

fn delete(image: u32) -> String {
    kitty(&format!("a=d,d=I,i={image},q=2"), "")
}

/// The escapes that show the part `window` of the held picture `image` of `size` on the cells of
/// `shown`. `C=1` keeps the cursor still, because a cursor moved below the last row scrolls the
/// screen.
fn place(image: u32, size: (u32, u32), window: &Window, shown: Rect) -> String {
    let (x, y, wide, high) = source_rect(size, window);
    let control = format!(
        "a=p,i={image},p={VIEW_PLACEMENT},x={x},y={y},w={wide},h={high},c={},r={},C=1,q=2",
        shown.width, shown.height
    );
    format!(
        "\x1b[{};{}H{}",
        shown.y + 1,
        shown.x + 1,
        kitty(&control, "")
    )
}

fn is_svg(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("svg"))
}

fn parse_svg(data: &[u8], options: &usvg::Options) -> Result<usvg::Tree, String> {
    usvg::Tree::from_data(data, options).map_err(|error| format!("cannot read: {error}"))
}

fn svg_size(tree: &usvg::Tree) -> (u32, u32) {
    let size = tree.size();
    (size.width().ceil() as u32, size.height().ceil() as u32)
}

/// Draws an SVG at the size it takes on screen, so it stays sharp at any width.
fn draw_svg(
    data: &[u8],
    resources: Option<&Path>,
    wide: u32,
    high: u32,
) -> Result<DynamicImage, String> {
    let tree = svg_tree(data, resources)?;
    let size = tree.size();
    let scale = (wide as f32 / size.width()).min(high as f32 / size.height());
    rasterize(
        &tree,
        Transform::from_scale(scale, scale),
        (size.width() * scale).ceil() as u32,
        (size.height() * scale).ceil() as u32,
    )
}

/// Draws the part of an SVG that `window` shows, at the pixels the frame draws.
fn draw_svg_window(
    data: &[u8],
    resources: Option<&Path>,
    window: &Window,
) -> Result<DynamicImage, String> {
    let tree = svg_tree(data, resources)?;
    let across = f64::from(window.pixels_wide) / window.wide;
    let down = f64::from(window.pixels_high) / window.high;
    let transform = Transform::from_row(
        across as f32,
        0.0,
        0.0,
        down as f32,
        (-window.x * across) as f32,
        (-window.y * down) as f32,
    );
    rasterize(&tree, transform, window.pixels_wide, window.pixels_high)
}

/// Its labels need the system fonts, which take a moment to load, so they load once, on the first
/// SVG drawn.
fn svg_tree(data: &[u8], resources: Option<&Path>) -> Result<usvg::Tree, String> {
    static FONTS: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    let fonts = FONTS.get_or_init(|| {
        let mut fonts = usvg::fontdb::Database::new();
        fonts.load_system_fonts();
        Arc::new(fonts)
    });
    let options = usvg::Options {
        resources_dir: resources.map(Path::to_path_buf),
        fontdb: Arc::clone(fonts),
        ..usvg::Options::default()
    };
    parse_svg(data, &options)
}

fn rasterize(
    tree: &usvg::Tree,
    transform: Transform,
    wide: u32,
    high: u32,
) -> Result<DynamicImage, String> {
    let mut pixmap = Pixmap::new(wide.max(1), high.max(1))
        .ok_or_else(|| "cannot draw: the picture has no area".to_string())?;
    resvg::render(tree, transform, &mut pixmap.as_mut());
    let pixels = pixmap
        .pixels()
        .iter()
        .flat_map(|pixel| {
            let color = pixel.demultiply();
            [color.red(), color.green(), color.blue(), color.alpha()]
        })
        .collect();
    image::RgbaImage::from_raw(pixmap.width(), pixmap.height(), pixels)
        .map(DynamicImage::ImageRgba8)
        .ok_or_else(|| "cannot draw: the picture has the wrong size".to_string())
}

fn has_scheme(url: &str) -> bool {
    url.split_once(':').is_some_and(|(scheme, _)| {
        let mut chars = scheme.chars();
        scheme.len() > 1
            && chars.next().is_some_and(|c| c.is_ascii_alphabetic())
            && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;

    use ratatui_image::picker::Picker;

    use super::{MAX_PICTURE_PIXELS, Pictures};

    fn svg(wide: u32, high: u32) -> Arc<str> {
        format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="{wide}" height="{high}"><rect width="{wide}" height="{high}" fill="#f00"/></svg>"##
        )
        .into()
    }

    /// A picture that would take more pixels than the budget is placed in fewer cells, with its
    /// shape kept, and a picture inside the budget keeps its size.
    ///
    /// Why it matters: drawing renders every pixel of the cells a picture takes, so a 123-byte SVG
    /// that declares 1000 by 200000 pixels would make the viewer allocate gigabytes.
    #[test]
    fn places_a_picture_within_the_pixel_budget() {
        // Given: pictures drawn in half blocks on 10 by 20 pixel cells, 100 columns of room
        let pictures = Pictures::new(Path::new("."), Picker::halfblocks());
        let pixels = |columns: u16, rows: u16| u64::from(columns) * 10 * u64::from(rows) * 20;

        // When: a picture far taller than the budget is placed
        let tall = pictures.place_svg(svg(1000, 200_000), 100).unwrap();
        // Then: its cells hold no more pixels than the budget, plus a row and a column of rounding
        // up to whole cells
        let rounding = pixels(tall.columns, 1) + pixels(1, tall.rows);
        assert!(
            pixels(tall.columns, tall.rows) <= MAX_PICTURE_PIXELS + rounding,
            "{} by {}",
            tall.columns,
            tall.rows
        );
        // And: it keeps the shape of 1 wide to 200 high, within a cell of rounding
        let shape = f64::from(tall.rows) * 20.0 / (f64::from(tall.columns) * 10.0);
        assert!((shape - 200.0).abs() < 200.0 * 0.05, "shape {shape}");

        // When: a picture inside the budget is placed
        let small = pictures.place_svg(svg(80, 40), 100).unwrap();
        // Then: it takes its docs site size, 10 columns and 3 rows
        assert_eq!((small.columns, small.rows), (10, 3));
    }
}
