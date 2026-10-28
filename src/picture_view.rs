use std::f64::consts::SQRT_2;

use crate::pictures::Source;

/// How many presses of + a picture takes from the fit, each one √2 larger, so 8 is 16 times the
/// fit. Past that a raster picture is a blur of single pixels.
const MAX_ZOOM: u32 = 8;
/// How far one pan moves, as a share of the page.
const PAN_SHARE: f64 = 1.0 / 8.0;

/// One picture shown on the whole page, at a zoom the reader picks, around a point they pan to.
#[derive(Debug, Clone)]
pub struct PictureView {
    pub source: Source,
    /// The size of the picture in its own pixels.
    size: (f64, f64),
    zoom: u32,
    /// The point of the picture at the middle of the page, in picture pixels.
    centre: (f64, f64),
    /// The pixels of the page in the last frame, which a pan moves a share of.
    page: (f64, f64),
}

/// The part of a picture that one frame shows, and the cells and pixels it takes on the page.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Window {
    /// The top left corner of the part shown, in picture pixels.
    pub x: f64,
    pub y: f64,
    /// The size of the part shown, in picture pixels.
    pub wide: f64,
    pub high: f64,
    pub columns: u16,
    pub rows: u16,
    /// The pixels the frame draws, a whole number of cells.
    pub pixels_wide: u32,
    pub pixels_high: u32,
}

impl PictureView {
    pub fn new(source: Source, (wide, high): (u32, u32)) -> Self {
        let size = (f64::from(wide.max(1)), f64::from(high.max(1)));
        Self {
            source,
            size,
            zoom: 0,
            centre: (size.0 / 2.0, size.1 / 2.0),
            page: (1.0, 1.0),
        }
    }

    pub fn zoom_in(&mut self, times: usize) {
        let times = u32::try_from(times).unwrap_or(MAX_ZOOM);
        self.zoom = self.zoom.saturating_add(times).min(MAX_ZOOM);
    }

    pub fn zoom_out(&mut self, times: usize) {
        let times = u32::try_from(times).unwrap_or(MAX_ZOOM);
        self.zoom = self.zoom.saturating_sub(times);
    }

    pub fn fit(&mut self) {
        self.zoom = 0;
        self.centre = (self.size.0 / 2.0, self.size.1 / 2.0);
    }

    /// Moves the part shown by `across` and `down` shares of the page, and no further than the
    /// edges of the picture.
    pub fn pan(&mut self, across: i32, down: i32) {
        let scale = self.scale(self.page);
        self.centre.0 += f64::from(across) * PAN_SHARE * self.page.0 / scale;
        self.centre.1 += f64::from(down) * PAN_SHARE * self.page.1 / scale;
        let window = self.window(self.page, (1, 1));
        self.centre = (window.x + window.wide / 2.0, window.y + window.high / 2.0);
    }

    /// The part of the picture a page of `columns` by `rows` cells shows, on cells of `cell`
    /// pixels. The page is kept for the next pan.
    pub fn frame(&mut self, columns: u16, rows: u16, cell: (u16, u16)) -> Window {
        let cell = (cell.0.max(1), cell.1.max(1));
        self.page = (
            f64::from(columns.max(1)) * f64::from(cell.0),
            f64::from(rows.max(1)) * f64::from(cell.1),
        );
        let window = self.window(self.page, cell);
        self.centre = (window.x + window.wide / 2.0, window.y + window.high / 2.0);
        window
    }

    fn scale(&self, page: (f64, f64)) -> f64 {
        let fit = (page.0 / self.size.0).min(page.1 / self.size.1);
        fit * SQRT_2.powi(i32::try_from(self.zoom).unwrap_or(0))
    }

    fn window(&self, page: (f64, f64), cell: (u16, u16)) -> Window {
        let scale = self.scale(page);
        let cells = |picture: f64, page: f64, cell: u16| {
            let shown = (picture * scale).min(page);
            (shown / f64::from(cell)).ceil().max(1.0) as u16
        };
        let columns = cells(self.size.0, page.0, cell.0);
        let rows = cells(self.size.1, page.1, cell.1);
        let pixels_wide = u32::from(columns) * u32::from(cell.0);
        let pixels_high = u32::from(rows) * u32::from(cell.1);
        let wide = (f64::from(pixels_wide) / scale).min(self.size.0);
        let high = (f64::from(pixels_high) / scale).min(self.size.1);
        let corner = |centre: f64, shown: f64, picture: f64| {
            (centre - shown / 2.0).clamp(0.0, picture - shown)
        };
        Window {
            x: corner(self.centre.0, wide, self.size.0),
            y: corner(self.centre.1, high, self.size.1),
            wide,
            high,
            columns,
            rows,
            pixels_wide,
            pixels_high,
        }
    }
}
