#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::path::Path;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use image::{Rgb, RgbImage};
use mdlive::Viewer;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::Color;
use ratatui_image::picker::{Picker, ProtocolType};

mod common;

use common::{HEADER_ROWS, below_header};

const RED: Rgb<u8> = Rgb([255, 0, 0]);
const BLUE: Rgb<u8> = Rgb([0, 0, 255]);
/// The screen under the breadcrumb: a page of 40 by 10 cells, the hint bar and the status line.
const WIDTH: u16 = 40;
const HEIGHT: u16 = 12;
const HINT_BAR: u16 = HEIGHT - 2;

/// A 40 by 20 pixel chart, red with a blue 20 by 10 block in the middle. On the 40 by 10 page of
/// 10 by 20 pixel cells it fits at a scale of 10, so it fills the page exactly.
fn chart(dir: &Path) {
    RgbImage::from_fn(40, 20, |x, y| {
        if (10..30).contains(&x) && (5..15).contains(&y) {
            BLUE
        } else {
            RED
        }
    })
    .save(dir.join("chart.png"))
    .unwrap();
}

/// `from_fontsize` is the only constructor that takes a cell size, because `from_query_stdio` asks
/// a real terminal.
#[allow(deprecated)]
fn viewer(dir: &Path) -> Viewer {
    let mut picker = Picker::from_fontsize((10, 20).into());
    picker.set_protocol_type(ProtocolType::Halfblocks);
    Viewer::new("doc.md", "intro\n\n![chart](chart.png)\n\nafter\n").with_images(dir, picker)
}

fn draw(viewer: &mut Viewer) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(WIDTH, HEIGHT + HEADER_ROWS)).unwrap();
    terminal.draw(|frame| viewer.draw(frame)).unwrap();
    below_header(terminal.backend().buffer())
}

fn press(viewer: &mut Viewer, keys: &str) {
    for key in keys.chars() {
        viewer.handle_key(KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE));
    }
}

fn enter(viewer: &mut Viewer) {
    viewer.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
}

fn row(buffer: &Buffer, y: u16) -> String {
    (0..buffer.area.width)
        .map(|x| buffer[(x, y)].symbol())
        .collect::<String>()
        .trim_end()
        .to_string()
}

/// Whether the lower half of the cell at `x`, `y` is painted `colour`.
fn is(buffer: &Buffer, x: u16, y: u16, colour: Rgb<u8>) -> bool {
    let [r, g, b] = colour.0;
    buffer[(x, y)].bg == Color::Rgb(r, g, b)
}

/// A picture marked with `]p` opens on the whole page with Enter, fitted to it, and q closes it.
///
/// Why it matters: a picture in the text is never wider than the text, so a wide diagram stays too
/// small to read until the reader can show it on the whole screen.
#[test]
fn opens_the_marked_picture_fitted_to_the_page_and_closes_it() {
    // Given: a page with text above and below a chart
    let dir = tempfile::tempdir().unwrap();
    chart(dir.path());
    let mut viewer = viewer(dir.path());
    draw(&mut viewer);

    // When: the reader marks the next picture
    press(&mut viewer, "]p");
    let buffer = draw(&mut viewer);
    // Then: a marker shows in the margin beside the chart, and the bar names Enter to open it
    assert_eq!(buffer[(0, 2)].symbol(), "▶");
    assert!(row(&buffer, HINT_BAR).starts_with(" Enter open"));

    // When: the reader presses Enter
    enter(&mut viewer);
    let buffer = draw(&mut viewer);

    // Then: the chart fills the page, red at the corners and blue in the middle, and no text shows
    assert!(is(&buffer, 2, 1, RED));
    assert!(is(&buffer, 37, 8, RED));
    assert!(is(&buffer, 20, 5, BLUE));
    assert_ne!(row(&buffer, 0), " intro");
    // And: the bar names the key that closes the picture
    assert!(row(&buffer, HINT_BAR).starts_with(" q close"));

    // When: the reader presses q
    press(&mut viewer, "q");
    let buffer = draw(&mut viewer);

    // Then: the page shows again
    assert_eq!(row(&buffer, 0), " intro");
}
