use crate::pictures::Source;

/// One picture shown on the whole page, fitted to it.
#[derive(Debug, Clone)]
pub struct PictureView {
    pub source: Source,
    /// The size of the picture in its own pixels.
    size: (f64, f64),
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
        Self { source, size }
    }

    /// The part of the picture a page of `columns` by `rows` cells shows, on cells of `cell`
    /// pixels.
    pub fn frame(&self, columns: u16, rows: u16, cell: (u16, u16)) -> Window {
        let cell = (cell.0.max(1), cell.1.max(1));
        let page = (
            f64::from(columns.max(1)) * f64::from(cell.0),
            f64::from(rows.max(1)) * f64::from(cell.1),
        );
        self.window(page, cell)
    }

    fn scale(&self, page: (f64, f64)) -> f64 {
        (page.0 / self.size.0).min(page.1 / self.size.1)
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
        Window {
            x: 0.0,
            y: 0.0,
            wide: (f64::from(pixels_wide) / scale).min(self.size.0),
            high: (f64::from(pixels_high) / scale).min(self.size.1),
            columns,
            rows,
            pixels_wide,
            pixels_high,
        }
    }
}
