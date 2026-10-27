// Each test binary compiles this module and uses only part of it.
#![allow(dead_code)]

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

/// The breadcrumb row the viewer draws above the page.
pub const HEADER_ROWS: u16 = 1;

/// The screen under the breadcrumb row: the page, the hint bar and the status line, from row 0.
/// Tests about the page read it this way, so a row number names a row of the page.
pub fn below_header(screen: &Buffer) -> Buffer {
    let area = Rect {
        height: screen.area.height - HEADER_ROWS,
        ..screen.area
    };
    let mut below = Buffer::empty(area);
    for y in 0..area.height {
        for x in 0..area.width {
            below[(x, y)] = screen[(x, y + HEADER_ROWS)].clone();
        }
    }
    below
}
