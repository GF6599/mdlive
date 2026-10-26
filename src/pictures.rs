use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use image::imageops::FilterType;
use image::{DynamicImage, ImageReader, Rgba};
use ratatui::Frame;
use ratatui::layout::{Rect, Size};
use ratatui::style::Color;
use ratatui::text::Span;
use ratatui::widgets::Paragraph;
use ratatui_image::picker::Picker;
use ratatui_image::sliced::{SignedPosition, SlicedImage, SlicedProtocol};
use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg;

use crate::theme;

/// A picture sized to fit the text, in cells.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Placed {
    pub path: PathBuf,
    pub columns: u16,
    pub rows: u16,
}

/// Where the pictures of a page come from, and how the terminal draws them.
pub struct Pictures {
    base: PathBuf,
    picker: Picker,
    loaded: HashMap<Placed, Result<SlicedProtocol, String>>,
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
    pub fn new(base: &Path, mut picker: Picker) -> Self {
        if let Color::Rgb(r, g, b) = theme::BASE {
            picker.set_background_color(Some(Rgba([r, g, b, u8::MAX])));
        }
        Self {
            base: base.canonicalize().unwrap_or_else(|_| base.to_path_buf()),
            picker,
            loaded: HashMap::new(),
        }
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
        let (pixels_wide, pixels_high) = if is_svg(&path) {
            let size = read_svg(&path, &usvg::Options::default())?.size();
            (size.width().ceil() as u32, size.height().ceil() as u32)
        } else {
            ImageReader::open(&path)
                .and_then(ImageReader::with_guessed_format)
                .map_err(|error| error.to_string())
                .and_then(|reader| reader.into_dimensions().map_err(|error| error.to_string()))
                .map_err(|error| format!("cannot read: {error}"))?
        };
        let font = self.picker.font_size();
        let (cell_wide, cell_high) = (u64::from(font.width.max(1)), u64::from(font.height.max(1)));
        let (wide, high) = (u64::from(pixels_wide.max(1)), u64::from(pixels_high.max(1)));
        let room = u64::try_from(width.max(1))
            .unwrap_or(u64::MAX)
            .saturating_mul(cell_wide);
        let (wide, high) = if wide > room {
            (room, high.saturating_mul(room).div_ceil(wide))
        } else {
            (wide, high)
        };
        let cells = |pixels: u64, cell: u64| {
            u16::try_from(pixels.div_ceil(cell).clamp(1, MAX_ROWS)).unwrap_or(1)
        };
        Ok(Placed {
            path,
            columns: cells(wide, cell_wide),
            rows: cells(high, cell_high),
        })
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
    ) {
        let picker = &self.picker;
        let loaded = self
            .loaded
            .entry(placed.clone())
            .or_insert_with(|| load(picker, placed));
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
                    Paragraph::new(Span::styled(reason.as_str(), theme::picture_note())),
                    line,
                );
            }
        }
    }

    /// Forgets the pictures a page no longer shows at these sizes.
    pub fn keep_only<'a>(&mut self, shown: impl Iterator<Item = &'a Placed>) {
        let shown: HashSet<&Placed> = shown.collect();
        self.loaded.retain(|placed, _| shown.contains(placed));
    }
}

/// The folders a docs site serves from its root: Next.js and Vite use `public`, Docusaurus uses
/// `static`, and the VS Code preview uses the workspace folder itself.
const SITE_ROOTS: [&str; 3] = ["public", "static", ""];

/// A picture taller than this many rows is cut, because the sliced drawing positions rows with a
/// signed 16-bit offset.
const MAX_ROWS: u64 = 10_000;

fn load(picker: &Picker, placed: &Placed) -> Result<SlicedProtocol, String> {
    let font = picker.font_size();
    let wide = u32::from(placed.columns) * u32::from(font.width);
    let high = u32::from(placed.rows) * u32::from(font.height);
    let image = if is_svg(&placed.path) {
        draw_svg(&placed.path, wide, high)?
    } else {
        ImageReader::open(&placed.path)
            .and_then(ImageReader::with_guessed_format)
            .map_err(|error| error.to_string())
            .and_then(|reader| reader.decode().map_err(|error| error.to_string()))
            .map_err(|error| format!("cannot read: {error}"))?
    };
    let image: DynamicImage = if image.width() > wide {
        image.resize(wide, high, FilterType::Triangle)
    } else {
        image
    };
    let size = Size::new(placed.columns, placed.rows);
    SlicedProtocol::new(picker, image, Some(size)).map_err(|error| format!("cannot draw: {error}"))
}

fn is_svg(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("svg"))
}

fn read_svg(path: &Path, options: &usvg::Options) -> Result<usvg::Tree, String> {
    std::fs::read(path)
        .map_err(|error| error.to_string())
        .and_then(|data| usvg::Tree::from_data(&data, options).map_err(|error| error.to_string()))
        .map_err(|error| format!("cannot read: {error}"))
}

/// Draws an SVG at the size it takes on screen, so it stays sharp at any width. Its labels need
/// the system fonts, which take a moment to load, so they load once, on the first SVG drawn.
fn draw_svg(path: &Path, wide: u32, high: u32) -> Result<DynamicImage, String> {
    static FONTS: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    let fonts = FONTS.get_or_init(|| {
        let mut fonts = usvg::fontdb::Database::new();
        fonts.load_system_fonts();
        Arc::new(fonts)
    });
    let options = usvg::Options {
        resources_dir: path.parent().map(Path::to_path_buf),
        fontdb: Arc::clone(fonts),
        ..usvg::Options::default()
    };
    let tree = read_svg(path, &options)?;
    let size = tree.size();
    let scale = (wide as f32 / size.width()).min(high as f32 / size.height());
    let (pixels_wide, pixels_high) = (
        (size.width() * scale).ceil() as u32,
        (size.height() * scale).ceil() as u32,
    );
    let mut pixmap = Pixmap::new(pixels_wide.max(1), pixels_high.max(1))
        .ok_or_else(|| "cannot draw: the picture has no area".to_string())?;
    resvg::render(
        &tree,
        Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
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
