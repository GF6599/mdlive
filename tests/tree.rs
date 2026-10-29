#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use mdlive::{Viewer, theme};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;

mod common;

use common::{HEADER_ROWS, below_header};

const WIDTH: u16 = 80;
const HEIGHT: u16 = 14;

fn draw(viewer: &mut Viewer) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(WIDTH, HEIGHT + HEADER_ROWS)).unwrap();
    terminal.draw(|frame| viewer.draw(frame)).unwrap();
    below_header(terminal.backend().buffer())
}

fn cells(buffer: &Buffer, y: u16, from: u16, to: u16) -> String {
    (from..to).map(|x| buffer[(x, y)].symbol()).collect()
}

/// The rows inside the panel border, trimmed, down to the first empty one.
fn panel(buffer: &Buffer) -> Vec<String> {
    (1..HEIGHT - 3)
        .map(|y| cells(buffer, y, 1, 29).trim_end().to_string())
        .take_while(|row| !row.is_empty())
        .collect()
}

fn panel_title(buffer: &Buffer) -> String {
    cells(buffer, 0, 0, 30)
}

/// The row of the panel whose text starts with `text`, and whether the current file marker ends it.
fn marked(buffer: &Buffer, text: &str) -> bool {
    let y = (1..HEIGHT - 3)
        .find(|&y| cells(buffer, y, 1, 29).trim().starts_with(text))
        .unwrap();
    buffer[(28, y)].symbol() == "◀"
}

/// The text of the row the tree cursor is on, without the marker column, while the tree has the
/// focus.
fn cursor(buffer: &Buffer) -> Option<String> {
    let colour = theme::ROSE_PINE.tree_cursor().bg;
    (1..HEIGHT - 3)
        .find(|&y| Some(buffer[(1, y)].bg) == colour)
        .map(|y| cells(buffer, y, 1, 28).trim().to_string())
}

/// A panel row: 27 columns of entry text, then the current file marker.
fn entry(text: &str, current: bool) -> String {
    format!("{text:<27}{}", if current { "◀" } else { " " })
        .trim_end()
        .to_string()
}

fn page_top(buffer: &Buffer) -> String {
    cells(buffer, 0, 31, WIDTH).trim().to_string()
}

fn hint_bar(buffer: &Buffer) -> String {
    cells(buffer, HEIGHT - 2, 0, WIDTH)
}

fn status(buffer: &Buffer) -> String {
    cells(buffer, HEIGHT - 1, 0, WIDTH)
}

fn press(viewer: &mut Viewer, code: KeyCode) {
    viewer.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn type_keys(viewer: &mut Viewer, keys: &str) {
    for key in keys.chars() {
        press(viewer, KeyCode::Char(key));
    }
}

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

/// A repository with Markdown in numbered folders, a folder with no Markdown, and an ignored
/// folder that holds Markdown.
fn repository() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir(root.join(".git")).unwrap();
    write(&root.join(".gitignore"), "node_modules\n");
    write(&root.join("README.md"), "# Readme\n");
    write(&root.join("docs/001-intro/one.md"), "# One\n");
    write(&root.join("docs/001-intro/two.md"), "# Two\n");
    write(&root.join("docs/002-guide/three.md"), "# Three\n");
    write(&root.join("docs/images/pic.png"), "png");
    write(&root.join("node_modules/pkg/README.md"), "# Package\n");
    let two = root.join("docs/001-intro/two.md");
    (dir, two)
}

fn viewer_of(path: &Path) -> Viewer {
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    Viewer::new(&name, &fs::read_to_string(path).unwrap()).with_path(path)
}

/// Pressing - opens a tree of the repository's Markdown files beside the page, unfolded to the open file,
/// and the tree keys browse it and read a file into the page.
///
/// Why it matters: the tree is how a reader finds the next page of a docs folder, so a tree that
/// lists ignored or empty folders buries the pages, and one that loses the open file makes the
/// reader hunt for where they are.
#[test]
fn browses_the_markdown_files_of_the_repository_in_a_tree() {
    // Given: the second page of a repository, read on a screen too narrow to open the contents
    let (dir, two) = repository();
    let mut viewer = viewer_of(&two);
    let buffer = draw(&mut viewer);
    // Then: the bar names the key of the tree
    assert!(
        hint_bar(&buffer).contains("  - files  "),
        "{}",
        hint_bar(&buffer)
    );

    // When: the reader presses -
    type_keys(&mut viewer, "-");
    let buffer = draw(&mut viewer);
    // Then: the tree is titled with the repository folder, and lists only folders that hold
    // Markdown, folders first, unfolded down to the open file
    let name = dir.path().file_name().unwrap().to_string_lossy();
    assert!(panel_title(&buffer).starts_with(&format!("┌ {name}")));
    assert_eq!(
        panel(&buffer),
        [
            "▾ docs",
            "  ▾ 001-intro",
            "      one.md",
            &entry("      two.md", true),
            "  ▸ 002-guide",
            "  README.md",
        ]
    );
    // And: the cursor is on the open file, and the bar names the keys of the tree
    assert_eq!(cursor(&buffer).as_deref(), Some("two.md"));
    assert!(hint_bar(&buffer).contains("j/k move  l open  h close  / filter  Esc page"));

    // When: the reader presses Enter on the open file
    press(&mut viewer, KeyCode::Enter);
    // Then: nothing was opened, so there is no way back
    assert!(!hint_bar(&draw(&mut viewer)).contains("C-o back"));
    type_keys(&mut viewer, "-");

    // When: the reader moves up and presses Enter
    type_keys(&mut viewer, "k");
    press(&mut viewer, KeyCode::Enter);
    let buffer = draw(&mut viewer);
    // Then: that page is read into the page, the tree stays open, and marks the new file
    assert_eq!(page_top(&buffer), "One");
    assert!(status(&buffer).starts_with(" one.md"));
    assert!(marked(&buffer, "one.md"));
    assert!(!marked(&buffer, "two.md"));
    // And: the page has the keys, so the reader can read the file they opened
    assert_eq!(cursor(&buffer), None);
    assert!(hint_bar(&buffer).contains("j/k scroll"));

    // When: the reader gives the tree the keys, then presses h on the file and h on its folder
    type_keys(&mut viewer, "-hh");
    // Then: the cursor goes up to the folder, and the folder closes
    let buffer = draw(&mut viewer);
    assert_eq!(cursor(&buffer).as_deref(), Some("▸ 001-intro"));
    assert_eq!(
        panel(&buffer),
        ["▾ docs", "  ▸ 001-intro", "  ▸ 002-guide", "  README.md"]
    );

    // When: the reader moves down, opens the folder with l, and reads its file with l
    type_keys(&mut viewer, "jljl");
    let buffer = draw(&mut viewer);
    // Then: the file is read, the tree stays open, and the page has the keys
    assert_eq!(page_top(&buffer), "Three");
    assert!(panel_title(&buffer).starts_with(&format!("┌ {name}")));
    assert_eq!(cursor(&buffer), None);
    assert!(hint_bar(&buffer).contains("j/k scroll"));

    // When: the reader presses -, then - again
    type_keys(&mut viewer, "-");
    // Then: the first gives the tree the keys, and the second closes it
    assert!(cursor(&draw(&mut viewer)).is_some());
    type_keys(&mut viewer, "-");
    assert!(!panel_title(&draw(&mut viewer)).starts_with("┌"));

    // When: the reader opens the contents, then the tree, and presses h
    type_keys(&mut viewer, "t-h");
    let buffer = draw(&mut viewer);
    // Then: the tree takes the place of the contents, and h moves its cursor instead of acting as
    // a contents hint letter
    assert!(panel_title(&buffer).starts_with(&format!("┌ {name}")));
    assert_eq!(cursor(&buffer).as_deref(), Some("▾ 002-guide"));

    // When: the reader closes the tree
    type_keys(&mut viewer, "-");
    // Then: the contents comes back
    assert!(panel_title(&draw(&mut viewer)).starts_with("┌ Contents"));
}

/// A page outside any repository roots the tree at its own folder.
///
/// Why it matters: without a repository there is no root to climb to, and a walk up to the home
/// folder would list every Markdown file the reader owns.
#[test]
fn roots_the_tree_at_the_folder_of_a_page_outside_a_repository() {
    // Given: a page in a folder with a sibling, inside a parent folder with another page
    let dir = tempfile::tempdir().unwrap();
    write(&dir.path().join("parent.md"), "# Parent\n");
    write(&dir.path().join("notes/sibling.md"), "# Sibling\n");
    let page = dir.path().join("notes/page.md");
    write(&page, "# Page\n");
    let mut viewer = viewer_of(&page);

    // When: the reader opens the tree
    type_keys(&mut viewer, "-");
    let buffer = draw(&mut viewer);

    // Then: it holds the page's folder and nothing above it
    assert!(panel_title(&buffer).starts_with("┌ notes"));
    assert_eq!(
        panel(&buffer),
        [entry("  page.md", true), entry("  sibling.md", false)]
    );
}

/// A viewer started on a folder opens with the tree of that folder and an empty page, and the
/// tree keeps that root after a file is read.
///
/// Why it matters: `mdl <folder>` and plain `mdl` have no page yet, so the tree is the only way
/// in, and an empty page that counted as a place to go back to would make Ctrl-o blank the screen.
#[test]
fn starts_in_the_tree_of_a_folder_with_an_empty_page() {
    // Given: a viewer started on the docs folder of a repository
    let (dir, _) = repository();
    let docs = dir.path().join("docs");
    let mut viewer = Viewer::new("docs", "").with_tree(&docs);

    // When: the screen is drawn
    let buffer = draw(&mut viewer);
    // Then: the tree of the folder has the keys, with every folder closed, and no file is marked
    assert!(panel_title(&buffer).starts_with("┌ docs"));
    assert_eq!(panel(&buffer), ["▸ 001-intro", "▸ 002-guide"]);
    assert_eq!(cursor(&buffer).as_deref(), Some("▸ 001-intro"));
    assert!(status(&buffer).starts_with(" docs"));

    // When: the reader opens the first folder and reads its first file
    type_keys(&mut viewer, "ljl");
    // Then: the page shows it
    assert_eq!(page_top(&draw(&mut viewer)), "One");

    // When: the reader presses Ctrl-o, and gives the page the keys
    viewer.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL));
    press(&mut viewer, KeyCode::Esc);
    let buffer = draw(&mut viewer);
    // Then: the page stays, because the empty start is not a page to go back to
    assert_eq!(page_top(&buffer), "One");
    assert!(!hint_bar(&buffer).contains("C-o back"));

    // When: the reader closes the tree and opens it again
    type_keys(&mut viewer, "--");
    type_keys(&mut viewer, "-");
    // Then: it opens at the folder the viewer started on
    assert!(panel_title(&draw(&mut viewer)).starts_with("┌ docs"));
}

/// / in the tree filters it as the reader types to the files whose path holds every word, in any
/// case, with their folders open. Enter keeps the filter, and Escape clears it.
///
/// Why it matters: a docs tree holds hundreds of pages, so the filter is how a reader reaches one
/// they can name, and a filter that cannot be cleared hides the rest of the tree.
#[test]
fn filters_the_tree_to_the_files_whose_path_holds_every_word() {
    // Given: the tree of a repository, holding the keys
    let (_dir, two) = repository();
    let mut viewer = viewer_of(&two);
    type_keys(&mut viewer, "-");

    // When: the reader presses / and types part of a name
    type_keys(&mut viewer, "/thr");
    let buffer = draw(&mut viewer);
    // Then: the tree holds only that file and its folders, with the cursor on the file, and the
    // bar names the keys of the filter
    assert_eq!(
        panel(&buffer),
        ["▾ docs", "  ▾ 002-guide", "      three.md"]
    );
    assert_eq!(cursor(&buffer).as_deref(), Some("three.md"));
    assert!(panel_title(&buffer).contains("/thr"));
    assert!(hint_bar(&buffer).starts_with(" Enter keep  Esc clear"));

    // When: the reader presses Enter, then l
    press(&mut viewer, KeyCode::Enter);
    type_keys(&mut viewer, "l");
    // Then: the filter stays, and l reads the file instead of typing
    let buffer = draw(&mut viewer);
    assert_eq!(page_top(&buffer), "Three");
    assert!(panel_title(&buffer).contains("/thr"));

    // When: the reader gives the tree the keys again, and presses Escape
    type_keys(&mut viewer, "-");
    press(&mut viewer, KeyCode::Esc);
    // Then: the filter clears, and the tree keeps the keys
    let buffer = draw(&mut viewer);
    assert!(panel(&buffer).len() > 3);
    assert!(cursor(&buffer).is_some());

    // When: the reader filters by two words in another case
    type_keys(&mut viewer, "/INTRO one");
    // Then: only the file whose path holds both words is left
    assert_eq!(
        panel(&draw(&mut viewer)),
        ["▾ docs", "  ▾ 001-intro", "      one.md"]
    );

    // When: the reader presses Escape while typing
    press(&mut viewer, KeyCode::Esc);
    // Then: the filter clears
    assert!(panel(&draw(&mut viewer)).len() > 3);
}
