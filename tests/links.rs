#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use mdlive::{Viewer, hyperlink_cells};
use ratatui::Terminal;
use ratatui::backend::{Backend, CrosstermBackend, TestBackend};
use ratatui::buffer::Buffer;
use unicode_width::UnicodeWidthStr;

const URL: &str = "https://example.com/mdlive";

fn draw(viewer: &mut Viewer, width: u16, height: u16) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| viewer.draw(frame)).unwrap();
    terminal.backend().buffer().clone()
}

/// The text under each link on screen, read from the buffer, and the address it opens.
fn linked(viewer: &mut Viewer, width: u16, height: u16) -> Vec<(String, String)> {
    let buffer = draw(viewer, width, height);
    viewer
        .links()
        .iter()
        .map(|link| {
            let mut text = String::new();
            let mut x = link.area.x;
            while x < link.area.right() {
                let symbol = buffer[(x, link.area.y)].symbol();
                text.push_str(symbol);
                x += u16::try_from(symbol.width().max(1)).unwrap();
            }
            (text, link.url.clone())
        })
        .collect()
}

fn links_in(markdown: &str, width: u16, height: u16) -> Vec<(String, String)> {
    linked(&mut Viewer::new("doc.md", markdown), width, height)
}

/// What the terminal receives when the link cells of one frame are written to it.
fn written(viewer: &mut Viewer, width: u16, height: u16, before: &[mdlive::LinkArea]) -> String {
    let buffer = draw(viewer, width, height);
    let cells = hyperlink_cells(&buffer, viewer.links(), before);
    let mut output = Vec::new();
    {
        let mut backend = CrosstermBackend::new(&mut output);
        backend
            .draw(cells.iter().map(|(x, y, cell)| (*x, *y, cell)))
            .unwrap();
        Backend::flush(&mut backend).unwrap();
    }
    String::from_utf8(output).unwrap()
}

/// The text of terminal output with its cursor and colour codes taken out.
fn without_csi(output: &str) -> String {
    let mut text = String::new();
    let mut chars = output.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.next() == Some('[') {
            for c in chars.by_ref() {
                if ('@'..='~').contains(&c) {
                    break;
                }
            }
        } else {
            text.push(c);
        }
    }
    text
}

/// A link that wraps is linked on each row it covers, and only over its own text.
///
/// Why it matters: a click anywhere on the link text must open it, and a click on the words around
/// it must not.
#[test]
fn links_each_row_of_a_wrapped_link_and_nothing_around_it() {
    // Given: a link whose text wraps across two rows
    let markdown = format!("See [the mdlive project page]({URL}) for more.\n");

    // When: the page is drawn 22 columns wide
    let links = links_in(&markdown, 22, 5);

    // Then: each row of the link text is linked to its address
    assert_eq!(
        links,
        [
            ("the mdlive".to_string(), URL.to_string()),
            ("project page".to_string(), URL.to_string()),
        ]
    );
}

/// A link in a list, a quote, a heading or a table cell is linked where it is drawn.
///
/// Why it matters: each of these adds columns in front of the text, a bullet, a bar, a border, and
/// a link area that ignores them puts the click on the wrong characters.
#[test]
fn links_text_in_lists_quotes_headings_and_tables() {
    // Given: links in a list item, a quote, a heading and a table cell
    let markdown = format!(
        "- item [one]({URL}/1)\n\n> quote [two]({URL}/2)\n\n## Head [three]({URL}/3)\n\n\
         | Name | Site |\n|---|---|\n| x | [four]({URL}/4) |\n"
    );

    // When: the page is drawn
    let links = links_in(&markdown, 60, 20);

    // Then: each link covers exactly its text
    assert_eq!(
        links,
        [
            ("one".to_string(), format!("{URL}/1")),
            ("two".to_string(), format!("{URL}/2")),
            ("three".to_string(), format!("{URL}/3")),
            ("four".to_string(), format!("{URL}/4")),
        ]
    );
}

/// Only an address with a scheme becomes a link, and a link whose address holds a control
/// character is shown as plain text.
///
/// Why it matters: the address goes to the terminal inside an escape sequence, so a control
/// character from the file could end that sequence and send the terminal commands of its own.
/// A relative path or an anchor has nothing the terminal can open.
#[test]
fn links_only_safe_absolute_addresses() {
    // Given: an escape and a C1 control smuggled in through character references, a relative
    // path, an anchor, and one good link
    let markdown = format!(
        "[esc](https://a.example/&#27;]2;title&#7;) [csi](https://a.example/&#x9b;31m) \
         [rel](other.md) [anchor](#top) [good]({URL})\n"
    );

    // When: the page is drawn
    let links = links_in(&markdown, 80, 5);

    // Then: only the good link is linked
    assert_eq!(links, [("good".to_string(), URL.to_string())]);
}

/// The terminal receives each link cell inside an OSC 8 hyperlink, and a wide character once.
///
/// Why it matters: the hyperlink codes are what make the click work, and a wide character
/// written twice, or with its second cell as a space, breaks the row.
#[test]
fn writes_link_cells_inside_hyperlink_codes() {
    // Given: a link with a wide character in its text
    let mut viewer = Viewer::new("doc.md", &format!("[go 日本]({URL})\n"));

    // When: the link cells are written to the terminal
    let output = written(&mut viewer, 40, 4, &[]);

    // Then: the text sits between the open and close codes, with the wide character once
    let open = format!("\x1b]8;id=0;{URL}\x1b\\");
    let close = "\x1b]8;;\x1b\\";
    let start = output.find(&open).expect("no open code");
    let end = output.rfind(close).expect("no close code");
    let inside = &output[start + open.len()..end];
    assert_eq!(without_csi(inside), "go 日本", "{inside:?}");
    assert_eq!(output.matches('日').count(), 1);
    assert_eq!(output.matches(close).count(), 1);
}

/// A link that scrolls away has its old cells written again as plain text.
///
/// Why it matters: the terminal keeps a hyperlink on a cell until the cell is written again, and
/// a cell whose new text matches the old is not redrawn, so a stale link would stay clickable on
/// text that is no longer the link.
#[test]
fn writes_old_link_cells_again_without_the_link() {
    // Given: a link on the first row, then the reader scrolls it off screen
    let markdown = format!("[top]({URL})\n\nsecond\n\nthird\n\nfourth\n");
    let mut viewer = Viewer::new("doc.md", &markdown);
    draw(&mut viewer, 40, 4);
    let before = viewer.links().to_vec();
    viewer.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
    viewer.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));

    // When: the next frame's link cells are written
    let output = written(&mut viewer, 40, 4, &before);

    // Then: the old cells get the text now under them, with no hyperlink code
    assert_eq!(without_csi(&output), "sec", "{output:?}");
}

/// A link under the contents panel is not linked.
///
/// Why it matters: on a narrow screen the panel covers the page, and a click on a contents entry
/// must not open a link hidden under it.
#[test]
fn does_not_link_text_under_the_contents_panel() {
    // Given: a link at the left of the page, and the contents open on a narrow screen
    let markdown = format!(
        "# Title\n\n[hidden]({URL}) {}[shown]({URL}/2)\n",
        "x".repeat(30)
    );
    let mut viewer = Viewer::new("doc.md", &markdown);
    viewer.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE));

    // When: the page is drawn 60 columns wide
    let links = linked(&mut viewer, 60, 8);

    // Then: only the link beside the panel is linked
    assert_eq!(links, [("shown".to_string(), format!("{URL}/2"))]);
}
