use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};

use image::imageops::FilterType;
use image::{DynamicImage, ImageReader, Rgba};
use ratatui::Frame;
use ratatui::layout::{Rect, Size};
use ratatui::style::Color;
use ratatui::text::Span;
use ratatui::widgets::Paragraph;
use ratatui_image::picker::Picker;
use ratatui_image::sliced::{SignedPosition, SlicedImage, SlicedProtocol};

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
            base: base.to_path_buf(),
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
        let path = self.base.join(url);
        if !path.exists() {
            return Err(format!("not found: {url}"));
        }
        let (pixels_wide, pixels_high) = ImageReader::open(&path)
            .and_then(ImageReader::with_guessed_format)
            .map_err(|error| error.to_string())
            .and_then(|reader| reader.into_dimensions().map_err(|error| error.to_string()))
            .map_err(|error| format!("cannot read: {error}"))?;
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

/// A picture taller than this many rows is cut, because the sliced drawing positions rows with a
/// signed 16-bit offset.
const MAX_ROWS: u64 = 10_000;

fn load(picker: &Picker, placed: &Placed) -> Result<SlicedProtocol, String> {
    let image = ImageReader::open(&placed.path)
        .and_then(ImageReader::with_guessed_format)
        .map_err(|error| error.to_string())
        .and_then(|reader| reader.decode().map_err(|error| error.to_string()))
        .map_err(|error| format!("cannot read: {error}"))?;
    let font = picker.font_size();
    let wide = u32::from(placed.columns) * u32::from(font.width);
    let high = u32::from(placed.rows) * u32::from(font.height);
    let image: DynamicImage = if image.width() > wide {
        image.resize(wide, high, FilterType::Triangle)
    } else {
        image
    };
    let size = Size::new(placed.columns, placed.rows);
    SlicedProtocol::new(picker, image, Some(size)).map_err(|error| format!("cannot draw: {error}"))
}

fn has_scheme(url: &str) -> bool {
    url.split_once(':').is_some_and(|(scheme, _)| {
        let mut chars = scheme.chars();
        scheme.len() > 1
            && chars.next().is_some_and(|c| c.is_ascii_alphabetic())
            && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    })
}
