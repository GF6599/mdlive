#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::cell::RefCell;
use std::rc::Rc;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use mdlive::{Viewer, theme};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;

mod common;

use common::{HEADER_ROWS, below_header};

const WIDTH: u16 = 60;
const HEIGHT: u16 = 12;
/// The rows the page takes, above the hint bar and the status line.
const PAGE_ROWS: u16 = HEIGHT - 2;

fn draw_at(viewer: &mut Viewer, width: u16) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, HEIGHT + HEADER_ROWS)).unwrap();
    terminal.draw(|frame| viewer.draw(frame)).unwrap();
    below_header(terminal.backend().buffer())
}

fn draw(viewer: &mut Viewer) -> Buffer {
    draw_at(viewer, WIDTH)
}

fn row(buffer: &Buffer, y: u16) -> String {
    (0..buffer.area.width)
        .map(|x| buffer[(x, y)].symbol())
        .collect::<String>()
        .trim_end()
        .to_string()
}

fn type_keys(viewer: &mut Viewer, keys: &str) {
    for key in keys.chars() {
        viewer.handle_key(KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE));
    }
}

fn press(viewer: &mut Viewer, code: KeyCode) {
    viewer.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}

/// The page rows with any cell in the selection colour, the cursor row included.
fn selected_rows(buffer: &Buffer) -> Vec<u16> {
    let colours = [theme::selection().bg, theme::selection_cursor().bg];
    (0..PAGE_ROWS)
        .filter(|&y| (0..buffer.area.width).any(|x| colours.contains(&Some(buffer[(x, y)].bg))))
        .collect()
}

/// The page row in the cursor colour.
fn cursor_row(buffer: &Buffer) -> Option<u16> {
    let colour = theme::selection_cursor().bg;
    (0..PAGE_ROWS).find(|&y| (0..buffer.area.width).any(|x| Some(buffer[(x, y)].bg) == colour))
}

/// A left button event at `column` and `row`, with the row counted below the breadcrumb.
fn left(viewer: &mut Viewer, kind: fn(MouseButton) -> MouseEventKind, column: u16, row: u16) {
    viewer.handle_mouse(MouseEvent {
        kind: kind(MouseButton::Left),
        column,
        row: row + HEADER_ROWS,
        modifiers: KeyModifiers::NONE,
    });
}

/// A viewer of `markdown` whose copies land in the returned list.
fn viewer(markdown: &str) -> (Viewer, Rc<RefCell<Vec<String>>>) {
    let copies = Rc::new(RefCell::new(Vec::new()));
    let record = Rc::clone(&copies);
    let viewer = Viewer::new("page.md", markdown).with_copier(move |text| {
        record.borrow_mut().push(text.to_string());
        Ok(())
    });
    (viewer, copies)
}

const PAGE: &str = "# Title\n\nFirst paragraph line one\ncontinues here.\n\n- item one\n- item two\n\n## Next\n\nafter\n";

/// V puts a cursor on the middle row, the movement keys grow the selection from there, and y copies
/// the Markdown source of the selected blocks and leaves visual mode.
///
/// Why it matters: a reader copies a passage to paste it into an editor or a chat, where the
/// source keeps the list markers and the line breaks the rendered page drops.
#[test]
fn copies_the_source_of_the_selected_lines() {
    // Given: a short page on the screen
    let (mut viewer, copies) = viewer(PAGE);
    let before = draw(&mut viewer);
    assert_eq!(selected_rows(&before), Vec::<u16>::new());
    let middle = PAGE_ROWS / 2;

    // When: the reader presses V
    type_keys(&mut viewer, "V");
    // Then: the middle row holds the cursor, and the bar names the keys of visual mode
    let buffer = draw(&mut viewer);
    assert_eq!(cursor_row(&buffer), Some(middle));
    assert_eq!(selected_rows(&buffer), [middle]);
    assert!(
        row(&buffer, HEIGHT - 2).starts_with(" j/k extend  y copy  Esc cancel"),
        "{}",
        row(&buffer, HEIGHT - 2)
    );

    // When: the reader moves the cursor up two rows
    type_keys(&mut viewer, "kk");
    // Then: the three rows from the cursor to where V started are selected
    let buffer = draw(&mut viewer);
    assert_eq!(cursor_row(&buffer), Some(middle - 2));
    assert_eq!(selected_rows(&buffer), [middle - 2, middle - 1, middle]);

    // When: the reader presses y
    type_keys(&mut viewer, "y");
    // Then: the source lines behind the selection are copied, and visual mode ends
    assert_eq!(
        *copies.borrow(),
        ["First paragraph line one\ncontinues here.\n\n- item one"]
    );
    let buffer = draw(&mut viewer);
    assert_eq!(selected_rows(&buffer), Vec::<u16>::new());
    assert!(row(&buffer, HEIGHT - 1).contains("copied 4 lines"));
    assert!(row(&buffer, HEIGHT - 2).starts_with(" j/k scroll"));
}

/// The movement keys move the cursor and scroll the page to keep it in view, Escape leaves without
/// copying, and j scrolls the page again after that.
///
/// Why it matters: a selection longer than the screen needs the view to follow the cursor, and a
/// reader who backs out must get the plain reading keys back.
#[test]
fn moves_the_cursor_through_the_page_and_leaves_on_escape() {
    // Given: a page longer than the screen, with a second section near its end
    let filler: String = (1..=20).map(|n| format!("filler {n}\n\n")).collect();
    let (mut viewer, copies) = viewer(&format!("# Top\n\n{filler}## End part\n\nlast\n"));
    draw(&mut viewer);

    // When: the reader presses V, then G
    type_keys(&mut viewer, "VG");
    // Then: the page scrolls so that the last line holds the cursor
    let buffer = draw(&mut viewer);
    assert_eq!(row(&buffer, PAGE_ROWS - 1), " last");
    assert_eq!(cursor_row(&buffer), Some(PAGE_ROWS - 1));

    // When: the reader presses [[
    type_keys(&mut viewer, "[[");
    // Then: the cursor moves to the heading above it, and stays in view
    let buffer = draw(&mut viewer);
    let cursor = cursor_row(&buffer).expect("no cursor on screen");
    assert_eq!(row(&buffer, cursor).trim(), "End part");

    // When: the reader presses gg
    type_keys(&mut viewer, "gg");
    // Then: the page is at its top with the cursor on its first line
    let buffer = draw(&mut viewer);
    assert_eq!(row(&buffer, 0), " Top");
    assert_eq!(cursor_row(&buffer), Some(0));

    // When: the reader presses Escape
    press(&mut viewer, KeyCode::Esc);
    // Then: nothing is copied, the selection is gone, and j scrolls the page
    assert_eq!(*copies.borrow(), Vec::<String>::new());
    assert_eq!(selected_rows(&draw(&mut viewer)), Vec::<u16>::new());
    type_keys(&mut viewer, "j");
    assert_ne!(row(&draw(&mut viewer), 0), " Top");
}

/// With the contents open, y copies in visual mode instead of acting as a contents hint letter,
/// the copy leaves out the blank line after the block, and the selection colours only the page
/// text, never the panel beside it.
///
/// Why it matters: y is also a hint letter, so without this the copy key would be swallowed while
/// the contents is open, and a selection over the panel would suggest the panel gets copied.
#[test]
fn copies_with_the_contents_open_and_colours_only_the_page() {
    // Given: a page of short paragraphs with the contents open beside it
    let (mut viewer, copies) = viewer("# Title\n\nalpha\n\nbeta\n\ngamma\n\n## Next\n\nmore\n");
    type_keys(&mut viewer, "t");
    let width = 100;
    draw_at(&mut viewer, width);

    // When: the reader presses V, then y
    type_keys(&mut viewer, "V");
    let buffer = draw_at(&mut viewer, width);
    type_keys(&mut viewer, "y");

    // Then: the panel columns carry no selection colour, and the paragraph on the middle row is
    // copied without the blank line after it
    let colours = [theme::selection().bg, theme::selection_cursor().bg];
    let middle = PAGE_ROWS / 2;
    assert!(row(&buffer, middle).ends_with("beta"));
    assert!((0..30).all(|x| !colours.contains(&Some(buffer[(x, middle)].bg))));
    assert!((30..width).any(|x| colours.contains(&Some(buffer[(x, middle)].bg))));
    assert_eq!(*copies.borrow(), ["beta"]);
}

/// A selection stays on the same text when the screen narrows and the page wraps again.
///
/// Why it matters: the lines move when a paragraph wraps to more rows, so a selection kept by row
/// would mark, and copy, other text than the reader chose.
#[test]
fn keeps_the_selection_on_its_text_when_the_page_wraps_again() {
    // Given: a long paragraph and a last line, with the cursor on the last line
    let paragraph = "word ".repeat(22);
    let (mut viewer, copies) = viewer(&format!("{paragraph}\n\nlast\n"));
    draw(&mut viewer);
    type_keys(&mut viewer, "VG");
    assert_eq!(
        row(&draw(&mut viewer), cursor_row(&draw(&mut viewer)).unwrap()),
        " last"
    );

    // When: the screen narrows, so the paragraph takes more rows
    let buffer = draw_at(&mut viewer, 30);

    // Then: the cursor is still on the last line, and y still copies it
    let cursor = cursor_row(&buffer).expect("no cursor on screen");
    assert_eq!(row(&buffer, cursor), " last");
    type_keys(&mut viewer, "y");
    assert!(copies.borrow()[0].ends_with("last"));
}

/// A drag over the page selects the lines it crosses, and the release copies their source.
///
/// Why it matters: with the mouse captured, the terminal's own selection takes whole screen rows,
/// panel and bars included, so a drag must select page lines the way V does.
#[test]
fn copies_the_lines_a_drag_crosses_on_release() {
    // Given: a short page on the screen
    let (mut viewer, copies) = viewer(PAGE);
    draw(&mut viewer);

    // When: the reader presses on the paragraph and drags down two rows
    left(&mut viewer, MouseEventKind::Down, 10, 3);
    left(&mut viewer, MouseEventKind::Drag, 12, 5);
    // Then: the three rows are selected, with the cursor where the pointer is
    let buffer = draw(&mut viewer);
    assert_eq!(selected_rows(&buffer), [3, 4, 5]);
    assert_eq!(cursor_row(&buffer), Some(5));

    // When: the reader lets go
    left(&mut viewer, MouseEventKind::Up, 12, 5);
    // Then: the source behind the rows is copied, and the selection is gone
    assert_eq!(
        *copies.borrow(),
        ["First paragraph line one\ncontinues here.\n\n- item one"]
    );
    assert_eq!(selected_rows(&draw(&mut viewer)), Vec::<u16>::new());
}

/// A drag that starts on a link selects instead of following it, and a drag past the last page row
/// scrolls the page and keeps the selection on page rows.
///
/// Why it matters: following on the press would open the link of every drag that starts on one,
/// and a selection that ran onto the hint bar would copy text that is not on the page.
#[test]
fn selects_from_a_link_and_scrolls_past_the_bottom() {
    // Given: a page longer than the screen with a link near its top, and an opener that records
    let filler: String = (1..=20).map(|n| format!("filler {n}\n\n")).collect();
    let opened = Rc::new(RefCell::new(Vec::<String>::new()));
    let record = Rc::clone(&opened);
    let (viewer, copies) = viewer(&format!("[web](https://example.com/a)\n\n{filler}"));
    let mut viewer = viewer.with_opener(move |url| {
        record.borrow_mut().push(url.to_string());
        Ok(())
    });
    draw(&mut viewer);

    // When: the reader presses on the link and drags onto the hint bar row
    left(&mut viewer, MouseEventKind::Down, 2, 0);
    left(&mut viewer, MouseEventKind::Drag, 2, PAGE_ROWS);
    // Then: the page scrolls one line, and the selection ends on the last page row
    let buffer = draw(&mut viewer);
    assert_eq!(row(&buffer, 0), "");
    assert_eq!(cursor_row(&buffer), Some(PAGE_ROWS - 1));

    // When: the reader lets go
    left(&mut viewer, MouseEventKind::Up, 2, PAGE_ROWS);
    // Then: the lines are copied, and the link is not followed
    assert!(copies.borrow()[0].starts_with("[web](https://example.com/a)"));
    assert_eq!(*opened.borrow(), Vec::<String>::new());

    // When: the reader goes back to the top and clicks the link with a shake along its row
    type_keys(&mut viewer, "gg");
    draw(&mut viewer);
    left(&mut viewer, MouseEventKind::Down, 2, 0);
    left(&mut viewer, MouseEventKind::Drag, 3, 0);
    left(&mut viewer, MouseEventKind::Up, 3, 0);
    // Then: the click follows the link, and nothing more is copied
    assert_eq!(*opened.borrow(), ["https://example.com/a"]);
    assert_eq!(copies.borrow().len(), 1);
}

/// A drag that starts below the end of a short page selects from its last line.
///
/// Why it matters: the empty rows under a page hold no text, so a selection over them would mark
/// rows that copy nothing.
#[test]
fn starts_a_drag_below_a_short_page_at_its_last_line() {
    // Given: a page of three lines on a taller screen
    let (mut viewer, copies) = viewer("alpha\n\nbeta\n");
    draw(&mut viewer);

    // When: the reader presses on an empty row under the page and drags to the first row
    left(&mut viewer, MouseEventKind::Down, 10, PAGE_ROWS - 2);
    left(&mut viewer, MouseEventKind::Drag, 10, 0);
    // Then: only the three page rows are selected
    assert_eq!(selected_rows(&draw(&mut viewer)), [0, 1, 2]);

    // When: the reader lets go
    left(&mut viewer, MouseEventKind::Up, 10, 0);
    // Then: the whole page is copied
    assert_eq!(*copies.borrow(), ["alpha\n\nbeta"]);
}
