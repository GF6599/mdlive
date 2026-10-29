#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::fs;
use std::path::Path;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use mdlive::{Viewer, theme};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;

mod common;

use common::{HEADER_ROWS, below_header};

const WIDTH: u16 = 80;
const HEIGHT: u16 = 13;

fn draw(viewer: &mut Viewer) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(WIDTH, HEIGHT + HEADER_ROWS)).unwrap();
    terminal.draw(|frame| viewer.draw(frame)).unwrap();
    below_header(terminal.backend().buffer())
}

fn cells(buffer: &Buffer, y: u16, from: u16, to: u16) -> String {
    (from..to).map(|x| buffer[(x, y)].symbol()).collect()
}

fn page_row(buffer: &Buffer, y: u16) -> String {
    cells(buffer, y, 31, WIDTH).trim().to_string()
}

fn full_row(buffer: &Buffer, y: u16) -> String {
    cells(buffer, y, 0, WIDTH).trim().to_string()
}

/// A mouse event at `row` counted below the breadcrumb, as the screen helpers count rows. The
/// terminal reports screen rows, so the event carries the breadcrumb row too.
fn mouse(viewer: &mut Viewer, kind: MouseEventKind, column: u16, row: u16) {
    viewer.handle_mouse(MouseEvent {
        kind,
        column,
        row: row + HEADER_ROWS,
        modifiers: KeyModifiers::NONE,
    });
}

fn click(viewer: &mut Viewer, column: u16, row: u16) {
    mouse(viewer, MouseEventKind::Down(MouseButton::Left), column, row);
}

fn key(viewer: &mut Viewer, letter: char) {
    viewer.handle_key(KeyEvent::new(KeyCode::Char(letter), KeyModifiers::NONE));
}

/// The wheel scrolls the page, also with the pointer over the contents, and a click on a contents
/// entry puts its section at the top.
///
/// Why it matters: a reader with a hand on the mouse expects the wheel to move what is under it,
/// and a click on an entry to go there, as in any reader with a sidebar.
#[test]
fn scrolls_with_the_wheel_and_jumps_to_a_clicked_section() {
    // Given: a page with four sections and enough text to scroll, on a screen that holds it
    let filler: String = (1..=20).map(|n| format!("filler {n}\n\n")).collect();
    let markdown = format!(
        "# Guide\n\nintro\n\n## Install\n\ntext\n\n### Flags\n\ntext\n\n## Usage\n\n{filler}"
    );
    let mut viewer = Viewer::new("doc.md", &markdown);
    let before = draw(&mut viewer);

    // When: the wheel turns down once over the page
    mouse(&mut viewer, MouseEventKind::ScrollDown, 40, 4);
    // Then: the page moves three lines, so the fourth row is now the first
    assert_eq!(full_row(&draw(&mut viewer), 0), full_row(&before, 3));

    // When: the wheel turns up once
    mouse(&mut viewer, MouseEventKind::ScrollUp, 40, 4);
    // Then: the page is back at the top
    assert_eq!(full_row(&draw(&mut viewer), 0), full_row(&before, 0));

    // When: the reader opens the contents and clicks the third entry
    key(&mut viewer, 't');
    let opened = draw(&mut viewer);
    assert!(cells(&opened, 3, 0, 30).contains("Flags"));
    click(&mut viewer, 10, 3);
    // Then: that section is at the top of the page
    assert!(page_row(&draw(&mut viewer), 0).starts_with("Flags"));

    // When: the wheel turns down over the contents
    let at = draw(&mut viewer);
    mouse(&mut viewer, MouseEventKind::ScrollDown, 10, 5);
    // Then: the page scrolls
    assert_eq!(page_row(&draw(&mut viewer), 0), page_row(&at, 3));
}

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

/// The cursor row of the tree, while the tree has the keys.
fn cursor_row(buffer: &Buffer) -> Option<u16> {
    let colour = theme::ROSE_PINE.tree_cursor().bg;
    (1..HEIGHT - 3).find(|&y| Some(buffer[(1, y)].bg) == colour)
}

/// A click on a tree folder opens or closes it, the wheel over the tree moves its cursor, a click
/// on a file reads it and gives the page the keys, and a click on the page gives the page the keys.
///
/// Why it matters: the tree is a list a reader points at, so a click that lands on another row, or
/// a page that keeps ignoring the keys after the reader clicked into it, sends them elsewhere.
#[test]
fn opens_tree_rows_by_click_and_gives_the_keys_to_what_was_clicked() {
    // Given: a repository with a folder of two pages, read from its top page, with the tree open
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join(".git")).unwrap();
    write(&dir.path().join("docs/alpha.md"), "# Alpha\n");
    write(&dir.path().join("docs/beta.md"), "# Beta\n");
    let top = dir.path().join("top.md");
    write(&top, "# Top\n");
    let mut viewer = Viewer::new("top.md", "# Top\n").with_path(&top);
    key(&mut viewer, '-');
    assert_eq!(cells(&draw(&mut viewer), 1, 1, 10).trim_end(), "▸ docs");

    // When: the reader clicks the folder
    click(&mut viewer, 5, 1);
    // Then: it opens
    let buffer = draw(&mut viewer);
    assert_eq!(cells(&buffer, 1, 1, 10).trim_end(), "▾ docs");
    assert_eq!(cells(&buffer, 3, 1, 12).trim(), "beta.md");

    // When: the wheel turns up over the tree
    mouse(&mut viewer, MouseEventKind::ScrollUp, 8, 3);
    // Then: the cursor moves up
    assert!(cursor_row(&draw(&mut viewer)).is_some_and(|row| row < 3));

    // When: the reader clicks the second file
    click(&mut viewer, 8, 3);
    // Then: it is read into the page, the page has the keys, and the tree stays open
    let buffer = draw(&mut viewer);
    assert_eq!(page_row(&buffer, 0), "Beta");
    assert!(full_row(&buffer, HEIGHT - 2).starts_with("j/k scroll"));
    assert_eq!(cursor_row(&buffer), None);
    assert_eq!(cells(&buffer, 1, 1, 10).trim_end(), "▾ docs");

    // When: the reader clicks the tree again, on the folder
    click(&mut viewer, 5, 1);
    // Then: the tree has the keys again, and the folder closes
    let buffer = draw(&mut viewer);
    assert!(full_row(&buffer, HEIGHT - 2).starts_with("j/k move"));
    assert_eq!(cells(&buffer, 1, 1, 10).trim_end(), "▸ docs");

    // When: the reader clicks the page
    click(&mut viewer, 50, 4);
    // Then: the page has the keys, and the tree stays open
    let buffer = draw(&mut viewer);
    assert!(full_row(&buffer, HEIGHT - 2).starts_with("j/k scroll"));
    assert_eq!(cursor_row(&buffer), None);
}
