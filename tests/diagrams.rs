#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::path::Path;

use mdlive::{Viewer, theme};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui_image::picker::{Picker, ProtocolType};
use unicode_width::UnicodeWidthStr;

mod common;

use common::{HEADER_ROWS, below_header};

const FLOW: &str = "```mermaid\nflowchart LR\n  A[Write] --> B[Preview]\n```\n\nafter\n";
/// The character the kitty protocol puts in each cell a picture covers.
const KITTY_CELL: char = '\u{10EEEE}';

fn draw(picker: Picker, markdown: &str, width: u16, height: u16) -> Buffer {
    let mut viewer = Viewer::new("doc.md", markdown).with_images(Path::new("."), picker);
    let mut terminal = Terminal::new(TestBackend::new(width, height + HEADER_ROWS)).unwrap();
    terminal.draw(|frame| viewer.draw(frame)).unwrap();
    below_header(terminal.backend().buffer())
}

fn rows(buffer: &Buffer) -> Vec<String> {
    (0..buffer.area.height)
        .map(|y| {
            let mut text = String::new();
            let mut x = 0;
            while x < buffer.area.width {
                let symbol = buffer[(x, y)].symbol();
                text.push_str(symbol);
                // A kitty picture cell holds escape codes, so its text width says nothing.
                x += u16::try_from(symbol.width().clamp(1, 2)).unwrap();
            }
            text.trim_end().to_string()
        })
        .collect()
}

fn kitty() -> Picker {
    let mut picker = Picker::halfblocks();
    picker.set_protocol_type(ProtocolType::Kitty);
    picker
}

/// In a terminal with graphics, a Mermaid block is drawn as a picture in place of its source.
///
/// Why it matters: the preview should show the diagram the page describes, the way the VS Code
/// preview does, and not the Mermaid source.
#[test]
fn draws_a_mermaid_block_as_a_picture_in_a_terminal_with_graphics() {
    // When: a page with a flowchart is drawn in a kitty terminal
    let buffer = draw(kitty(), FLOW, 60, 20);
    let screen = rows(&buffer);

    // Then: the first rows are picture cells, the source is gone, and the text after follows
    assert!(buffer[(1, 0)].symbol().contains(KITTY_CELL));
    assert!(!screen.iter().any(|row| row.contains("flowchart")));
    let after = screen
        .iter()
        .position(|row| row == " after")
        .expect("no text after");
    assert!(
        buffer[(1, u16::try_from(after - 2).unwrap())]
            .symbol()
            .contains(KITTY_CELL)
    );
}

/// A diagram drawn as a picture has no ground of its own, so the terminal background shows
/// around its boxes.
///
/// Why it matters: a diagram on a fill shows a dark block in a terminal with opacity or a
/// background picture.
#[test]
fn draws_a_diagram_on_the_terminal_background() {
    // When: a page with a flowchart is drawn in a kitty terminal
    let pixels = common::kitty_pixels(&draw(kitty(), FLOW, 60, 20));

    // Then: the top left pixel, outside every box, is transparent
    assert_eq!(pixels[3], 0);
}

/// Without graphics, a Mermaid block is drawn as box text.
///
/// Why it matters: a picture in half blocks turns the labels into blurs, while box text keeps
/// every label readable.
#[test]
fn draws_a_mermaid_block_as_box_text_without_graphics() {
    // When: the same page is drawn with half blocks only
    let screen = rows(&draw(Picker::halfblocks(), FLOW, 60, 20));

    // Then: the labels sit in boxes, and the source is gone
    let write = screen
        .iter()
        .find(|row| row.contains("Write"))
        .expect("no Write label");
    assert!(write.contains('│'), "{screen:#?}");
    assert!(
        screen.iter().any(|row| row.contains("Preview")),
        "{screen:#?}"
    );
    assert!(
        !screen.iter().any(|row| row.contains("flowchart")),
        "{screen:#?}"
    );
}

/// A block neither renderer can draw keeps its source, with the reason under it.
///
/// Why it matters: a typo in a diagram must not make it vanish, and the reader needs to see what
/// is wrong with it.
#[test]
fn shows_the_source_and_the_reason_for_a_block_it_cannot_draw() {
    // Given: a block whose diagram type no renderer knows
    let markdown = "```mermaid\nnotADiagram\n  A --> B\n```\n";

    for picker in [kitty(), Picker::halfblocks()] {
        // When: the page is drawn
        let buffer = draw(picker, markdown, 60, 10);
        let screen = rows(&buffer);

        // Then: the source shows, and a muted note under it names the reason
        assert!(
            screen.iter().any(|row| row.contains("notADiagram")),
            "{screen:#?}"
        );
        let note = screen
            .iter()
            .position(|row| row.starts_with(" (diagram: "))
            .unwrap_or_else(|| panic!("no reason: {screen:#?}"));
        assert_eq!(buffer[(2, u16::try_from(note).unwrap())].fg, theme::MUTED);
    }
}
