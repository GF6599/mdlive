#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::fs;
use std::path::Path;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use mdlive::{Flow, Viewer, theme};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;

const WIDTH: u16 = 60;
const HEIGHT: u16 = 10;

fn draw(viewer: &mut Viewer) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(WIDTH, HEIGHT)).unwrap();
    terminal.draw(|frame| viewer.draw(frame)).unwrap();
    terminal.backend().buffer().clone()
}

fn row(buffer: &Buffer, y: u16) -> String {
    (0..buffer.area.width)
        .map(|x| buffer[(x, y)].symbol())
        .collect::<String>()
        .trim_end()
        .to_string()
}

fn first_row(viewer: &mut Viewer) -> String {
    row(&draw(viewer), 0)
}

fn status(viewer: &mut Viewer) -> String {
    row(&draw(viewer), HEIGHT - 1)
}

fn hint_bar(viewer: &mut Viewer) -> String {
    row(&draw(viewer), HEIGHT - 2)
}

/// Whether any cell carries the colour of a search match.
fn marks_a_match(buffer: &Buffer) -> bool {
    let colours = [theme::search_match().bg, theme::search_current().bg];
    buffer
        .content()
        .iter()
        .any(|cell| colours.contains(&Some(cell.bg)))
}

fn key(viewer: &mut Viewer, code: KeyCode, modifiers: KeyModifiers) {
    viewer.handle_key(KeyEvent::new(code, modifiers));
}

fn page(dir: &Path, name: &str, title: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let lines: String = (1..=40).map(|n| format!("{title} line {n}\n\n")).collect();
    fs::write(&path, format!("# {title}\n\n{lines}")).unwrap();
    path
}

fn viewer_of(path: &Path) -> Viewer {
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    Viewer::new(&name, &fs::read_to_string(path).unwrap()).with_path(path)
}

/// Opening a file shows it in place of the page, Ctrl-o goes back to the last page at the place
/// the reader left, and Ctrl-i or Tab goes forward again. A file that cannot be read keeps the page.
///
/// Why it matters: the file tree and links open pages through this path, so a reader who follows
/// one and comes back must land where they were, not at the top of the page.
#[test]
fn opens_another_file_and_goes_back_and_forward() {
    // Given: two pages, and the reader eleven lines into the first, on a line of text
    let dir = tempfile::tempdir().unwrap();
    let alpha = page(dir.path(), "alpha.md", "Alpha");
    let beta = page(dir.path(), "sub/beta.md", "Beta");
    let mut viewer = viewer_of(&alpha);
    draw(&mut viewer);
    for _ in 0..11 {
        key(&mut viewer, KeyCode::Char('j'), KeyModifiers::NONE);
    }
    for typed in "/line".chars() {
        key(&mut viewer, KeyCode::Char(typed), KeyModifiers::NONE);
    }
    key(&mut viewer, KeyCode::Enter, KeyModifiers::NONE);
    assert!(marks_a_match(&draw(&mut viewer)));
    let left_at = first_row(&mut viewer);
    assert!(left_at.contains("Alpha line"), "{left_at}");

    // When: the reader opens the second page
    viewer.open(&beta);
    // Then: it shows from the top, under its own name, and the bar names the way back
    assert_eq!(first_row(&mut viewer), " Beta");
    assert!(status(&mut viewer).starts_with(" beta.md"));
    assert!(
        hint_bar(&mut viewer).contains("C-o back"),
        "{}",
        hint_bar(&mut viewer)
    );

    // And: no match is marked, because the search of the first page does not carry over
    assert!(!marks_a_match(&draw(&mut viewer)));

    // When: the reader presses Ctrl-o
    key(&mut viewer, KeyCode::Char('o'), KeyModifiers::CONTROL);
    // Then: the first page is back where the reader left it
    assert_eq!(first_row(&mut viewer), left_at);
    assert!(status(&mut viewer).starts_with(" alpha.md"));

    // When: the reader presses Tab, then Ctrl-o, then Ctrl-i
    key(&mut viewer, KeyCode::Tab, KeyModifiers::NONE);
    // Then: Tab goes forward to the second page
    assert_eq!(first_row(&mut viewer), " Beta");
    key(&mut viewer, KeyCode::Char('o'), KeyModifiers::CONTROL);
    key(&mut viewer, KeyCode::Char('i'), KeyModifiers::CONTROL);
    // And: Ctrl-i goes forward too
    assert_eq!(first_row(&mut viewer), " Beta");

    // When: the reader opens a file that does not exist
    viewer.open(&dir.path().join("missing.md"));
    // Then: the page stays, and the status line names the failure
    assert_eq!(first_row(&mut viewer), " Beta");
    assert!(status(&mut viewer).contains("cannot open"));

    // When: the reader goes back, opens a third page, and presses Tab
    key(&mut viewer, KeyCode::Char('o'), KeyModifiers::CONTROL);
    let gamma = page(dir.path(), "gamma.md", "Gamma");
    viewer.open(&gamma);
    key(&mut viewer, KeyCode::Tab, KeyModifiers::NONE);
    // Then: Tab does nothing, because opening a page starts a new way forward
    assert_eq!(first_row(&mut viewer), " Gamma");

    // When: the page to go back to is deleted, and the reader presses Ctrl-o
    fs::remove_file(&alpha).unwrap();
    key(&mut viewer, KeyCode::Char('o'), KeyModifiers::CONTROL);
    // Then: the page stays, and the status line names the failure
    assert_eq!(first_row(&mut viewer), " Gamma");
    assert!(status(&mut viewer).contains("cannot open"));

    // When: the page comes back, and the reader presses Ctrl-o again
    page(dir.path(), "alpha.md", "Alpha");
    key(&mut viewer, KeyCode::Char('o'), KeyModifiers::CONTROL);
    // Then: the way back still leads to it
    assert!(status(&mut viewer).starts_with(" alpha.md"));
}

/// E asks to open the editor on the file at the source line at the top of the page, and a viewer
/// with no file says there is nothing to edit.
///
/// Why it matters: the reader presses E to fix what they are reading, so an editor that opens at
/// another line, or on another file, makes them hunt for the place.
#[test]
fn asks_to_edit_the_file_at_the_line_at_the_top() {
    // Given: a page scrolled until its fifth line of text is at the top, which is source line 11
    let dir = tempfile::tempdir().unwrap();
    let alpha = page(dir.path(), "alpha.md", "Alpha");
    let mut viewer = viewer_of(&alpha);
    draw(&mut viewer);
    for _ in 0..11 {
        key(&mut viewer, KeyCode::Char('j'), KeyModifiers::NONE);
    }
    assert_eq!(first_row(&mut viewer), " Alpha line 5");

    // When: the reader presses E
    let flow = viewer.handle_key(KeyEvent::new(KeyCode::Char('E'), KeyModifiers::SHIFT));
    // Then: the viewer asks for the editor on that file at that line
    assert_eq!(flow, Flow::Edit(alpha, 11));

    // When: a viewer with no file gets E
    let mut empty = Viewer::new("notes", "");
    let flow = empty.handle_key(KeyEvent::new(KeyCode::Char('E'), KeyModifiers::SHIFT));
    // Then: it goes on, and says there is no file to edit
    assert_eq!(flow, Flow::Continue);
    assert!(status(&mut empty).contains("no file to edit"));
}
