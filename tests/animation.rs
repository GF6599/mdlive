#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::fs::File;
use std::path::Path;
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use image::codecs::gif::{GifEncoder, Repeat};
use image::{Delay, Frame, Rgba, RgbaImage};
use mdlive::{Viewer, draw_frame};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::Color;
use ratatui_image::picker::{Picker, ProtocolType};

mod common;

use common::{HEADER_ROWS, below_header};

const RED: Rgba<u8> = Rgba([255, 0, 0, 255]);
const BLUE: Rgba<u8> = Rgba([0, 0, 255, 255]);
const CLEAR: Rgba<u8> = Rgba([0, 0, 0, 0]);
const DELAY: Duration = Duration::from_millis(100);
/// The screen under the breadcrumb: a page of 40 by 10 cells, the hint bar and the status line.
const WIDTH: u16 = 40;
const HEIGHT: u16 = 12;
const HINT_BAR: u16 = HEIGHT - 2;
/// A cell in the middle of the page, which the picture covers.
const MIDDLE: (u16, u16) = (20, 5);

/// A 40 by 20 pixel GIF of one solid colour per frame, each frame shown for `DELAY`. On the 40 by
/// 10 page of 10 by 20 pixel cells it fills the page.
fn gif(dir: &Path, name: &str, colours: &[Rgba<u8>]) {
    let mut encoder = GifEncoder::new(File::create(dir.join(name)).unwrap());
    encoder.set_repeat(Repeat::Infinite).unwrap();
    let delay = Delay::from_saturating_duration(DELAY);
    encoder
        .encode_frames(
            colours.iter().map(|colour| {
                Frame::from_parts(RgbaImage::from_pixel(40, 20, *colour), 0, 0, delay)
            }),
        )
        .unwrap();
}

/// `from_fontsize` is the only constructor that takes a cell size, because `from_query_stdio` asks
/// a real terminal.
#[allow(deprecated)]
fn viewer(dir: &Path, name: &str, protocol: ProtocolType) -> Viewer {
    let mut picker = Picker::from_fontsize((10, 20).into());
    picker.set_protocol_type(protocol);
    Viewer::new("doc.md", &format!("intro\n\n![clip]({name})\n\nafter\n")).with_images(dir, picker)
}

/// A viewer with the red and blue GIF open in the picture view, in half blocks.
fn playing(dir: &Path) -> Viewer {
    gif(dir, "clip.gif", &[RED, BLUE]);
    let mut viewer = viewer(dir, "clip.gif", ProtocolType::Halfblocks);
    open_first_picture(&mut viewer);
    viewer
}

fn open_first_picture(viewer: &mut Viewer) {
    draw(viewer);
    press(viewer, "]p");
    viewer.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
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

/// The colour of the lower half of the middle cell. The GIF encoder may move a colour by a step or
/// two, so the cell names the nearest of red and blue.
fn middle_colour(buffer: &Buffer) -> &'static str {
    match buffer[MIDDLE].bg {
        Color::Rgb(r, _, b) if r > 200 && b < 50 => "red",
        Color::Rgb(r, _, b) if b > 200 && r < 50 => "blue",
        other => panic!("the middle cell is {other:?}"),
    }
}

fn row(buffer: &Buffer, y: u16) -> String {
    (0..buffer.area.width)
        .map(|x| buffer[(x, y)].symbol())
        .collect::<String>()
}

/// The picture view plays an animated GIF frame by frame with its own delays, and starts again
/// after the last frame.
///
/// Why it matters: the view showed only the first frame, so an animated GIF, such as a recorded
/// demo, could not be watched in mdlive.
#[test]
fn plays_the_frames_of_a_gif_in_turn_and_starts_again_after_the_last() {
    // Given: the red and blue GIF open in the view, on its first frame
    let dir = tempfile::tempdir().unwrap();
    let mut viewer = playing(dir.path());
    assert_eq!(middle_colour(&draw(&mut viewer)), "red");
    assert_eq!(viewer.until_next_frame(), Some(DELAY));

    // When: less than the delay of the frame passes
    // Then: the frame stays
    assert!(!viewer.play(DELAY / 2));
    assert_eq!(middle_colour(&draw(&mut viewer)), "red");

    // When: the rest of the delay passes
    // Then: the second frame shows
    assert!(viewer.play(DELAY / 2));
    assert_eq!(middle_colour(&draw(&mut viewer)), "blue");

    // When: the delay of the last frame passes
    // Then: the first frame shows again
    assert!(viewer.play(DELAY));
    assert_eq!(middle_colour(&draw(&mut viewer)), "red");
}

/// Space pauses a GIF on its frame, and Space again plays it on from there.
///
/// Why it matters: a reader who wants to read one frame of a demo must stop it there.
#[test]
fn pauses_and_plays_a_gif_with_space() {
    // Given: the red and blue GIF playing in the view
    let dir = tempfile::tempdir().unwrap();
    let mut viewer = playing(dir.path());
    draw(&mut viewer);

    // When: the reader presses Space and a second passes
    press(&mut viewer, " ");
    let shown = viewer.play(Duration::from_secs(1));

    // Then: no frame is due and the first frame stays
    assert!(!shown);
    assert_eq!(viewer.until_next_frame(), None);
    assert_eq!(middle_colour(&draw(&mut viewer)), "red");

    // When: the reader presses Space again and the delay passes
    press(&mut viewer, " ");
    let shown = viewer.play(DELAY);

    // Then: the second frame shows
    assert!(shown);
    assert_eq!(middle_colour(&draw(&mut viewer)), "blue");
}

/// The hint bar offers Space to pause while a GIF plays, and to play while it is paused.
///
/// Why it matters: a reader only finds the pause key from the hint bar or the key list.
#[test]
fn offers_pause_while_a_gif_plays_and_play_while_it_is_paused() {
    // Given: the red and blue GIF playing in the view
    let dir = tempfile::tempdir().unwrap();
    let mut viewer = playing(dir.path());

    // Then: the hint bar offers to pause
    let bar = row(&draw(&mut viewer), HINT_BAR);
    assert!(bar.contains("Space pause"), "{bar:?}");

    // When: the reader pauses
    press(&mut viewer, " ");

    // Then: the hint bar offers to play
    let bar = row(&draw(&mut viewer), HINT_BAR);
    assert!(bar.contains("Space play"), "{bar:?}");
}

/// A GIF of one frame and a picture of another format play nothing, so mdlive waits for the
/// reader as before.
///
/// Why it matters: a frame clock on a still picture wakes mdlive for nothing.
#[test]
fn plays_nothing_for_a_gif_of_one_frame() {
    // Given: a GIF of one frame open in the view
    let dir = tempfile::tempdir().unwrap();
    gif(dir.path(), "still.gif", &[RED]);
    let mut viewer = viewer(dir.path(), "still.gif", ProtocolType::Halfblocks);
    open_first_picture(&mut viewer);

    // Then: no frame is due, and the picture shows
    assert_eq!(viewer.until_next_frame(), None);
    assert_eq!(middle_colour(&draw(&mut viewer)), "red");
}

/// Closing the view stops the GIF.
///
/// Why it matters: a GIF that played on behind the page would keep mdlive busy decoding frames
/// that nobody sees.
#[test]
fn stops_the_gif_when_the_view_closes() {
    // Given: the red and blue GIF playing in the view
    let dir = tempfile::tempdir().unwrap();
    let mut viewer = playing(dir.path());
    draw(&mut viewer);

    // When: the reader closes the view
    press(&mut viewer, "q");

    // Then: no frame is due
    assert_eq!(viewer.until_next_frame(), None);
    assert!(!viewer.play(DELAY));
}

/// A GIF whose frames need more memory than mdlive lets one picture take is not played, and the
/// status line says why.
///
/// Why it matters: a GIF header can name a canvas of 65535 by 65535 pixels in a few bytes, and
/// each frame of it is 17 GB, so a small file could take all the memory of the machine.
#[test]
fn refuses_to_play_a_gif_whose_frames_pass_the_memory_limit() {
    // Given: a GIF of two 40 by 20 frames on a canvas of 11600 by 11600 pixels, 538 MB as RGBA,
    // just past the 512 MiB limit
    let dir = tempfile::tempdir().unwrap();
    gif(dir.path(), "large.gif", &[RED, BLUE]);
    let path = dir.path().join("large.gif");
    let mut bytes = std::fs::read(&path).unwrap();
    let side = 11_600_u16.to_le_bytes();
    bytes[6..10].copy_from_slice(&[side[0], side[1], side[0], side[1]]);
    std::fs::write(&path, bytes).unwrap();
    let mut viewer = viewer(dir.path(), "large.gif", ProtocolType::Halfblocks);

    // When: the reader opens it in the view
    open_first_picture(&mut viewer);

    // Then: no frame is due, and the status line says the GIF cannot be read
    assert_eq!(viewer.until_next_frame(), None);
    let status = row(&draw(&mut viewer), HEIGHT - 1);
    assert!(status.contains("cannot read"), "{status:?}");
}

/// The value of `key` in the first Kitty command of `output` that holds `action`.
fn kitty_value<'a>(output: &'a str, action: &str, key: &str) -> Option<(usize, &'a str)> {
    let start = output.find(action)?;
    let end = start + output[start..].find(';')?;
    let value = output[start..end]
        .split(',')
        .find_map(|pair| pair.strip_prefix(key)?.strip_prefix('='))?;
    Some((start, value))
}

/// In a Kitty terminal, each new frame goes to the terminal under the other of two picture ids and
/// takes the place of the frame before it, and only then is the frame before it deleted.
///
/// Why it matters: a frame sent again under the same id leaves the screen blank for a moment, so
/// the probe in Ghostty and herdr saw the picture flicker on each frame. With two ids the probe
/// played without flicker.
#[test]
fn sends_each_frame_under_the_other_picture_id_and_then_deletes_the_frame_before() {
    // Given: the red and blue GIF open in the view of a Kitty terminal
    let dir = tempfile::tempdir().unwrap();
    gif(dir.path(), "clip.gif", &[RED, BLUE]);
    let mut viewer = viewer(dir.path(), "clip.gif", ProtocolType::Kitty);
    let (mut terminal, output) = common::terminal(WIDTH, HEIGHT + HEADER_ROWS);
    let mut links = Vec::new();
    draw_frame(&mut terminal, &mut viewer, &mut links).unwrap();
    press(&mut viewer, "]p");
    viewer.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    draw_frame(&mut terminal, &mut viewer, &mut links).unwrap();
    let opened = output.take();
    let (_, first) = kitty_value(&opened, "a=t,", "i").expect("the first frame is sent");

    // When: the delay of the first frame passes
    viewer.play(DELAY);
    draw_frame(&mut terminal, &mut viewer, &mut links).unwrap();
    let next = output.take();

    // Then: the second frame is sent and placed under another id, and then the first is deleted
    let (sent_at, second) = kitty_value(&next, "a=t,", "i").expect("the second frame is sent");
    let (placed_at, placed) = kitty_value(&next, "a=p,", "i").expect("the second frame is placed");
    let (deleted_at, deleted) =
        kitty_value(&next, "a=d,", "i").expect("the first frame is deleted");
    assert_ne!(second, first);
    assert_eq!(placed, second);
    assert_eq!(deleted, first);
    assert!(sent_at < placed_at && placed_at < deleted_at, "{next:?}");
}

/// In a Kitty terminal, a GIF frame with no clear pixel goes to the terminal as RGB, and a frame
/// with a clear pixel goes as RGBA.
///
/// Why it matters: Ghostty showed each opaque GIF frame sent as RGBA with a flash, while the same
/// frames sent as RGB played smoothly. A clear pixel still needs RGBA, so that the terminal
/// background shows through it.
#[test]
fn sends_an_opaque_gif_frame_as_rgb_and_a_clear_frame_as_rgba() {
    // Given: a GIF of red, blue and then clear frames, open in the view of a Kitty terminal
    let dir = tempfile::tempdir().unwrap();
    gif(dir.path(), "clip.gif", &[RED, BLUE, CLEAR]);
    let mut viewer = viewer(dir.path(), "clip.gif", ProtocolType::Kitty);
    let (mut terminal, output) = common::terminal(WIDTH, HEIGHT + HEADER_ROWS);
    let mut links = Vec::new();
    draw_frame(&mut terminal, &mut viewer, &mut links).unwrap();
    press(&mut viewer, "]p");
    viewer.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    draw_frame(&mut terminal, &mut viewer, &mut links).unwrap();
    output.take();

    // When: the delay of the red frame passes
    viewer.play(DELAY);
    draw_frame(&mut terminal, &mut viewer, &mut links).unwrap();
    let blue = output.take();

    // Then: the opaque blue frame is sent in the 24-bit RGB format
    let (_, format) = kitty_value(&blue, "a=t,", "f").expect("the blue frame is sent");
    assert_eq!(format, "24");

    // When: the delay of the blue frame passes
    viewer.play(DELAY);
    draw_frame(&mut terminal, &mut viewer, &mut links).unwrap();
    let clear = output.take();

    // Then: the clear frame is sent in the 32-bit RGBA format
    let (_, format) = kitty_value(&clear, "a=t,", "f").expect("the clear frame is sent");
    assert_eq!(format, "32");
}
