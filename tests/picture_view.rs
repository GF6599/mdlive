#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::collections::VecDeque;
use std::path::Path;

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use image::{Rgb, RgbImage};
use mdlive::{Flow, Viewer, draw_frame, handle_burst};
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
    // And: the bar names the keys of the picture
    assert!(row(&buffer, HINT_BAR).starts_with(" +/- zoom"));

    // When: the reader presses q
    press(&mut viewer, "q");
    let buffer = draw(&mut viewer);

    // Then: the page shows again
    assert_eq!(row(&buffer, 0), " intro");
}

/// + zooms into the middle of the picture, - zooms out, and 0 fits it to the page again.
///
/// Why it matters: the labels of a wide diagram stay unreadable at the fit, so the reader needs to
/// make one part of it larger than the page.
#[test]
fn zooms_into_the_middle_of_the_picture_and_fits_it_again() {
    // Given: the chart open on the page
    let dir = tempfile::tempdir().unwrap();
    chart(dir.path());
    let mut viewer = viewer(dir.path());
    draw(&mut viewer);
    press(&mut viewer, "]p");
    enter(&mut viewer);

    // When: the reader presses - at the fit
    press(&mut viewer, "-");
    // Then: the chart stays fitted to the page
    assert!(is(&draw(&mut viewer), 2, 1, RED));

    // When: the reader presses + twice, which doubles the size
    press(&mut viewer, "++");
    let buffer = draw(&mut viewer);
    // Then: the blue middle fills the page
    assert!(is(&buffer, 2, 1, BLUE));
    assert!(is(&buffer, 37, 8, BLUE));

    // When: the reader presses - once
    press(&mut viewer, "-");
    let buffer = draw(&mut viewer);
    // Then: the red edge shows again at the corners, and the middle stays blue
    assert!(is(&buffer, 1, 0, RED));
    assert!(is(&buffer, 20, 5, BLUE));

    // When: the reader presses 0
    press(&mut viewer, "0");
    // Then: the chart fits the page again
    assert!(is(&draw(&mut viewer), 2, 1, RED));
}

/// h, j, k, l and the wheel pan a zoomed picture, and the pan stops at the edge of the picture.
///
/// Why it matters: a pan past the edge would show empty page where the reader expects the rest of
/// the picture, and they would have to pan back blind.
#[test]
fn pans_a_zoomed_picture_up_to_its_edges() {
    // Given: the chart open and zoomed to twice the fit, so the blue middle fills the page
    let dir = tempfile::tempdir().unwrap();
    chart(dir.path());
    let mut viewer = viewer(dir.path());
    draw(&mut viewer);
    press(&mut viewer, "]p");
    enter(&mut viewer);
    press(&mut viewer, "++");
    draw(&mut viewer);

    // When: the reader presses h far more often than the picture is wide
    press(&mut viewer, "hhhhhhhhhhhhhhhhhhhh");
    let buffer = draw(&mut viewer);
    // Then: the left edge of the chart is at the left of the page, a red band and then blue
    assert!(is(&buffer, 2, 5, RED));
    assert!(is(&buffer, 30, 5, BLUE));

    // When: the reader turns the wheel down far more often than the picture is tall
    for _ in 0..20 {
        viewer.handle_mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 20,
            row: 5 + HEADER_ROWS,
            modifiers: KeyModifiers::NONE,
        });
    }
    let buffer = draw(&mut viewer);
    // Then: the bottom edge of the chart is at the bottom of the page
    assert!(is(&buffer, 30, 8, RED));
    assert!(is(&buffer, 30, 1, BLUE));

    // When: the reader presses l and k far more often than the picture is wide and tall
    press(&mut viewer, "llllllllllllllllllllkkkkkkkkkkkkkkkkkkkk");
    let buffer = draw(&mut viewer);
    // Then: the top right corner of the chart is at the top right of the page
    assert!(is(&buffer, 37, 1, RED));
    assert!(is(&buffer, 10, 8, BLUE));
}

/// A plain click on a picture in the page opens it, and a click on the text beside it does not.
///
/// Why it matters: a reader who sees a diagram too small to read reaches for the mouse, and a click
/// on text must stay free for links and line selection.
#[test]
fn opens_a_picture_with_a_click_on_it() {
    // Given: a page with text above a chart, which takes 5 columns and 2 rows from column 1
    let dir = tempfile::tempdir().unwrap();
    chart(dir.path());
    let mut viewer = viewer(dir.path());
    draw(&mut viewer);

    // When: the reader clicks the text above the chart
    click(&mut viewer, 3, 0);
    // Then: the page stays
    assert_eq!(row(&draw(&mut viewer), 0), " intro");

    // When: the reader clicks the chart
    click(&mut viewer, 3, 2);
    let buffer = draw(&mut viewer);

    // Then: the chart fills the page
    assert!(is(&buffer, 2, 1, RED));
    assert!(is(&buffer, 20, 5, BLUE));
}

/// A press and a release of the left button at `column` and `row`, counted below the breadcrumb.
fn click(viewer: &mut Viewer, column: u16, row: u16) {
    for kind in [
        MouseEventKind::Down(MouseButton::Left),
        MouseEventKind::Up(MouseButton::Left),
    ] {
        viewer.handle_mouse(MouseEvent {
            kind,
            column,
            row: row + HEADER_ROWS,
            modifiers: KeyModifiers::NONE,
        });
    }
}

/// After the picture view closes, the page sends its pictures to the terminal again.
///
/// Why it matters: the Kitty protocol sends a picture once and then draws only placeholder cells.
/// The terminal may drop that data while the view covers the page, so a page that does not send it
/// again shows an empty placeholder where the picture was.
#[test]
fn sends_the_pictures_of_the_page_again_after_the_view_closes() {
    // Given: a chart drawn through the Kitty protocol, so its data was sent on the first draw
    let dir = tempfile::tempdir().unwrap();
    chart(dir.path());
    let mut viewer = kitty_viewer(dir.path());
    assert!(sends_a_picture(&draw(&mut viewer)));
    assert!(!sends_a_picture(&draw(&mut viewer)));

    // When: the reader opens the chart on the whole page and closes it
    press(&mut viewer, "]p");
    enter(&mut viewer);
    draw(&mut viewer);
    press(&mut viewer, "q");
    let buffer = draw(&mut viewer);

    // Then: the page sends the chart again
    assert!(sends_a_picture(&buffer));
}

/// Closing the view sends the page pictures again from the pixels mdlive already holds, without
/// reading their files again.
///
/// Why it matters: a large photo takes about 100 ms to decode, and closing the view decoded every
/// picture of the page again, so the page stalled each time the view closed.
#[test]
fn sends_the_page_pictures_again_without_reading_their_files() {
    // Given: a chart drawn through the Kitty protocol, then open on the whole page
    let dir = tempfile::tempdir().unwrap();
    chart(dir.path());
    let mut viewer = kitty_viewer(dir.path());
    draw(&mut viewer);
    press(&mut viewer, "]p");
    enter(&mut viewer);
    draw(&mut viewer);
    // And: the file of the chart is gone, so a read of it would fail
    std::fs::remove_file(dir.path().join("chart.png")).unwrap();

    // When: the reader closes the view
    press(&mut viewer, "q");
    let buffer = draw(&mut viewer);

    // Then: the page still sends the chart
    assert!(sends_a_picture(&buffer));
}

#[allow(deprecated)]
fn kitty_viewer(dir: &Path) -> Viewer {
    let mut picker = Picker::from_fontsize((10, 20).into());
    picker.set_protocol_type(ProtocolType::Kitty);
    Viewer::new("doc.md", "intro\n\n![chart](chart.png)\n\nafter\n").with_images(dir, picker)
}

/// Whether the frame carries a Kitty escape that sends picture data, as opposed to one that only
/// draws a picture the terminal already holds.
fn sends_a_picture(buffer: &Buffer) -> bool {
    buffer
        .content()
        .iter()
        .any(|cell| cell.symbol().contains("a=T"))
}

/// Whether the bytes of a frame carry a Kitty escape that sends picture data.
fn sends_picture_data(output: &str) -> bool {
    output.contains("a=t,") || output.contains("a=T,")
}

/// The Kitty command in the bytes of a frame that places a picture the terminal holds.
fn placement(output: &str) -> Option<&str> {
    let start = output.find("a=p,")?;
    let end = output[start..].find("\x1b\\")?;
    Some(&output[start..start + end])
}

/// In a Kitty terminal, a pan in the view places another part of the picture the terminal already
/// holds, and sends no picture data.
///
/// Why it matters: each pan, zoom or wheel step sent a new screen-sized picture, 32 MB for a large
/// photo on a large screen, so the view lagged behind the reader.
#[test]
fn pans_a_held_picture_with_a_placement_and_no_picture_data() {
    // Given: the chart open in a Kitty terminal and zoomed to twice the fit
    let dir = tempfile::tempdir().unwrap();
    chart(dir.path());
    let mut viewer = kitty_viewer(dir.path());
    let (mut terminal, output) = common::terminal(WIDTH, HEIGHT + HEADER_ROWS);
    let mut links = Vec::new();
    draw_frame(&mut terminal, &mut viewer, &mut links).unwrap();
    press(&mut viewer, "]p");
    enter(&mut viewer);
    press(&mut viewer, "++");
    draw_frame(&mut terminal, &mut viewer, &mut links).unwrap();
    let opened = output.take();
    assert!(sends_picture_data(&opened));

    // When: the reader pans left
    press(&mut viewer, "h");
    draw_frame(&mut terminal, &mut viewer, &mut links).unwrap();
    let panned = output.take();

    // Then: the frame places another part of the chart and sends no picture data
    assert!(!sends_picture_data(&panned), "{panned:?}");
    assert!(placement(&panned).is_some(), "{panned:?}");
    assert_ne!(placement(&panned), placement(&opened));
}

/// Closing the view in a Kitty terminal deletes the picture it held and frees its data.
///
/// Why it matters: a placed picture stays on the screen until mdlive deletes it, so the last frame
/// of the view would cover the page, and the terminal would keep the data of a large photo.
#[test]
fn deletes_the_held_picture_when_the_view_closes() {
    // Given: the chart open in a Kitty terminal
    let dir = tempfile::tempdir().unwrap();
    chart(dir.path());
    let mut viewer = kitty_viewer(dir.path());
    let (mut terminal, output) = common::terminal(WIDTH, HEIGHT + HEADER_ROWS);
    let mut links = Vec::new();
    draw_frame(&mut terminal, &mut viewer, &mut links).unwrap();
    press(&mut viewer, "]p");
    enter(&mut viewer);
    draw_frame(&mut terminal, &mut viewer, &mut links).unwrap();
    output.take();

    // When: the reader closes the view
    press(&mut viewer, "q");
    draw_frame(&mut terminal, &mut viewer, &mut links).unwrap();

    // Then: the frame deletes the picture and its data
    let closed = output.take();
    assert!(closed.contains("a=d,d=I,"), "{closed:?}");
}

/// After the screen is cleared, the view sends its picture to the terminal again.
///
/// Why it matters: ratatui clears the screen on each resize, and Ghostty then deletes each picture
/// that no placement shows, so a view that only places the picture again stays blank after the
/// reader zooms the terminal.
#[test]
fn sends_the_held_picture_again_after_the_screen_is_cleared() {
    // Given: the chart open in a Kitty terminal, which holds its data
    let dir = tempfile::tempdir().unwrap();
    chart(dir.path());
    let mut viewer = kitty_viewer(dir.path());
    let (mut terminal, output) = common::terminal(WIDTH, HEIGHT + HEADER_ROWS);
    let mut links = Vec::new();
    draw_frame(&mut terminal, &mut viewer, &mut links).unwrap();
    press(&mut viewer, "]p");
    enter(&mut viewer);
    draw_frame(&mut terminal, &mut viewer, &mut links).unwrap();
    output.take();

    // When: the terminal clears the screen
    viewer.screen_cleared();
    draw_frame(&mut terminal, &mut viewer, &mut links).unwrap();

    // Then: the frame sends the chart again
    assert!(sends_picture_data(&output.take()));
}

/// A view with no room on the screen places nothing.
///
/// Why it matters: Kitty reads a placement of zero cells as the natural size of the picture, so a
/// photo would cover the whole screen of a terminal that has no rows left for the view.
#[test]
fn places_nothing_when_the_view_has_no_room() {
    // Given: the chart open in a Kitty terminal
    let dir = tempfile::tempdir().unwrap();
    chart(dir.path());
    let mut viewer = kitty_viewer(dir.path());
    draw(&mut viewer);
    press(&mut viewer, "]p");
    enter(&mut viewer);
    let (mut terminal, output) = common::terminal(WIDTH, 3);
    let mut links = Vec::new();

    // When: mdlive draws a frame on a screen with rows only for the breadcrumb, the hint bar and
    // the status line
    draw_frame(&mut terminal, &mut viewer, &mut links).unwrap();

    // Then: the frame places no picture
    let frame = output.take();
    assert_eq!(placement(&frame), None, "{frame:?}");
}

/// A burst of wheel events that is already queued is handled in one pass, before the next frame.
///
/// Why it matters: each frame of the view builds and sends a screen-sized picture, so a frame per
/// wheel event leaves the view far behind a trackpad scroll.
#[test]
fn handles_a_queued_burst_of_wheel_events_before_the_next_frame() {
    // Given: the chart open and zoomed to twice the fit
    let dir = tempfile::tempdir().unwrap();
    chart(dir.path());
    let mut viewer = viewer(dir.path());
    draw(&mut viewer);
    press(&mut viewer, "]p");
    enter(&mut viewer);
    press(&mut viewer, "++");
    draw(&mut viewer);
    // And: far more wheel events queued than the picture is tall
    let mut queued: VecDeque<Event> = (0..20)
        .map(|_| {
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::ScrollDown,
                column: 20,
                row: 5 + HEADER_ROWS,
                modifiers: KeyModifiers::NONE,
            })
        })
        .collect();

    // When: mdlive handles the input that is ready
    let flow = handle_burst(
        || Ok(queued.pop_front()),
        |event| viewer.handle_event(event),
    )
    .unwrap();

    // Then: every queued event is handled
    assert!(matches!(flow, Some(Flow::Continue)));
    assert!(queued.is_empty());
    // And: the one frame after it shows the bottom edge of the chart
    let buffer = draw(&mut viewer);
    assert!(is(&buffer, 30, 8, RED));
    assert!(is(&buffer, 30, 1, BLUE));
}
