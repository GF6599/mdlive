#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::cell::RefCell;
use std::fs;
use std::path::Path;
use std::rc::Rc;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use mdlive::{Flow, Viewer, theme};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;

const WIDTH: u16 = 60;
const HEIGHT: u16 = 12;

fn draw(viewer: &mut Viewer) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(WIDTH, HEIGHT)).unwrap();
    terminal.draw(|frame| viewer.draw(frame)).unwrap();
    terminal.backend().buffer().clone()
}

fn row(buffer: &Buffer, y: u16) -> String {
    (0..WIDTH)
        .map(|x| buffer[(x, y)].symbol())
        .collect::<String>()
        .trim_end()
        .to_string()
}

/// The text of the cells marked as the selected link.
fn selected(buffer: &Buffer) -> String {
    let colour = theme::link_selected().bg;
    buffer
        .content()
        .iter()
        .filter(|cell| Some(cell.bg) == colour)
        .map(|cell| cell.symbol())
        .collect()
}

fn type_keys(viewer: &mut Viewer, keys: &str) -> Flow {
    keys.chars()
        .map(|key| viewer.handle_key(KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE)))
        .last()
        .unwrap_or(Flow::Continue)
}

fn press(viewer: &mut Viewer, code: KeyCode) -> Flow {
    viewer.handle_key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

/// ]l and [l mark each link in turn, and Enter follows the marked one: a web link goes to the
/// system opener, a page link opens the page at its heading, a heading link jumps on the page, and
/// any other address is refused.
///
/// Why it matters: the keyboard is the only way to follow a link where the terminal does not open
/// clicks, and an address with another scheme, such as a file or an app, must never reach the
/// opener, because link text hides it.
#[test]
fn steps_through_the_links_and_follows_the_marked_one() {
    // Given: a page with a web link, a page link to a heading, a heading link, a missing page, a
    // page link without its extension, and a file link, and an opener that records what it gets
    let dir = tempfile::tempdir().unwrap();
    let filler: String = (1..=12).map(|n| format!("filler {n}\n\n")).collect();
    write(
        &dir.path().join("sub/other.md"),
        &format!("# Other\n\n{filler}## Second part\n\n{filler}"),
    );
    write(&dir.path().join("sub/plain.md"), "# Plain\n");
    let page = dir.path().join("page.md");
    write(
        &page,
        &format!(
            "# Page\n\n[web](https://example.com/a) [other](sub/other.md#second-part) \
             [later](#later-on) [gone](missing.md) [plain](sub/plain) [file](file:///etc/hosts)\n\n\
             {filler}## Later on\n\n{filler}[far](#page)\n"
        ),
    );
    let opened = Rc::new(RefCell::new(Vec::new()));
    let record = Rc::clone(&opened);
    let mut viewer = Viewer::new("page.md", &fs::read_to_string(&page).unwrap())
        .with_path(&page)
        .with_opener(move |url| {
            record.borrow_mut().push(url.to_string());
            Ok(())
        });
    draw(&mut viewer);

    // When: the reader presses ]l
    type_keys(&mut viewer, "]l");
    // Then: the first link is marked, and the bar names the key that follows it
    let buffer = draw(&mut viewer);
    assert_eq!(selected(&buffer), "web");
    assert!(row(&buffer, HEIGHT - 2).contains("Enter follow"));

    // When: the reader presses Enter
    press(&mut viewer, KeyCode::Enter);
    // Then: the opener gets the web address
    assert_eq!(*opened.borrow(), ["https://example.com/a"]);

    // When: the reader marks the next link and follows it
    type_keys(&mut viewer, "]l");
    press(&mut viewer, KeyCode::Enter);
    // Then: the other page opens with its linked heading at the top, and Ctrl-o comes back
    let buffer = draw(&mut viewer);
    assert_eq!(row(&buffer, 0), " Second part");
    assert!(row(&buffer, HEIGHT - 1).starts_with(" other.md"));
    viewer.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL));
    assert!(row(&draw(&mut viewer), HEIGHT - 1).starts_with(" page.md"));

    // When: the reader marks the third link and follows it
    type_keys(&mut viewer, "]l]l]l");
    assert_eq!(selected(&draw(&mut viewer)), "later");
    press(&mut viewer, KeyCode::Enter);
    // Then: the page jumps to the heading
    assert_eq!(row(&draw(&mut viewer), 0), " Later on");

    // When: the reader goes back to the top, marks the missing page, and follows it
    type_keys(&mut viewer, "gg]l]l]l]l");
    assert_eq!(selected(&draw(&mut viewer)), "gone");
    press(&mut viewer, KeyCode::Enter);
    // Then: the status line names the path, and the page stays
    let buffer = draw(&mut viewer);
    assert!(row(&buffer, HEIGHT - 1).contains("not found"));
    assert_eq!(row(&buffer, 0), " Page");

    // When: the reader marks the link without an extension and follows it
    type_keys(&mut viewer, "]l");
    press(&mut viewer, KeyCode::Enter);
    // Then: the page with that name and the Markdown extension opens
    assert_eq!(row(&draw(&mut viewer), 0), " Plain");
    viewer.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL));

    // When: the reader marks the file link and follows it
    type_keys(&mut viewer, "gg]l]l]l]l]l]l");
    assert_eq!(selected(&draw(&mut viewer)), "file");
    press(&mut viewer, KeyCode::Enter);
    // Then: the opener does not get it, and the status line says why
    assert_eq!(*opened.borrow(), ["https://example.com/a"]);
    assert!(row(&draw(&mut viewer), HEIGHT - 1).contains("will not open file:"));

    // When: the reader presses [l
    type_keys(&mut viewer, "[l");
    // Then: the link before it is marked
    assert_eq!(selected(&draw(&mut viewer)), "plain");

    // When: the reader presses ]l twice, past the last link on the screen
    type_keys(&mut viewer, "]l]l");
    // Then: the page scrolls to the link at the end, and marks it
    assert_eq!(selected(&draw(&mut viewer)), "far");

    // When: the reader drops the mark, marks the first link, goes to the end where that link is
    // off screen, and presses ]l
    press(&mut viewer, KeyCode::Esc);
    type_keys(&mut viewer, "gg]l");
    assert_eq!(selected(&draw(&mut viewer)), "web");
    type_keys(&mut viewer, "G]l");
    // Then: the first link on the screen is marked, not the one after the old mark
    assert_eq!(selected(&draw(&mut viewer)), "far");

    // When: the reader presses Escape, then Escape again
    // Then: the first only drops the mark, and the second quits
    assert_eq!(press(&mut viewer, KeyCode::Esc), Flow::Continue);
    assert_eq!(selected(&draw(&mut viewer)), "");
    assert_eq!(press(&mut viewer, KeyCode::Esc), Flow::Quit);
}
