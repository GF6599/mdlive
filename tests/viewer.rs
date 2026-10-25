#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use mdview::{Flow, Viewer, theme};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::Modifier;
use unicode_width::UnicodeWidthStr;

fn draw(viewer: &mut Viewer, width: u16, height: u16) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| viewer.draw(frame)).unwrap();
    terminal.backend().buffer().clone()
}

/// Each row as a reader sees it, with a wide character counted once and trailing blanks cut.
fn rows(buffer: &Buffer) -> Vec<String> {
    (0..buffer.area.height)
        .map(|y| {
            let mut row = String::new();
            let mut x = 0;
            while x < buffer.area.width {
                let symbol = buffer[(x, y)].symbol();
                row.push_str(symbol);
                x += u16::try_from(symbol.width().max(1)).unwrap();
            }
            row.trim_end().to_string()
        })
        .collect()
}

fn body(buffer: &Buffer) -> Vec<String> {
    let mut all = rows(buffer);
    all.pop();
    all
}

fn status(buffer: &Buffer) -> String {
    rows(buffer).pop().unwrap()
}

fn shown(markdown: &str, width: u16, height: u16) -> Vec<String> {
    body(&draw(&mut Viewer::new("doc.md", markdown), width, height))
}

fn press(viewer: &mut Viewer, code: KeyCode) -> Flow {
    viewer.handle_key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn find(buffer: &Buffer, text: &str) -> (u16, u16) {
    rows(buffer)
        .iter()
        .enumerate()
        .find_map(|(y, row)| {
            row.find(text)
                .map(|byte| (row[..byte].width() as u16, y as u16))
        })
        .unwrap_or_else(|| panic!("{text:?} is not on screen"))
}

/// A heading and a paragraph render as text without their Markdown marks, wrapped at the width.
///
/// Why it matters: the preview must read like the rendered page, not like the source.
#[test]
fn renders_heading_and_paragraph_without_markup() {
    // Given: a heading and a paragraph with emphasis, wider than the screen
    let markdown = "# Title\n\nSome **bold** and *italic* text that wraps across lines.\n";

    // When: it is drawn 32 columns wide, which leaves 30 for text between the margins
    let screen = shown(markdown, 32, 8);

    // Then: the heading sits on its rule, and the paragraph wraps at a word boundary
    assert_eq!(
        screen,
        [
            " Title",
            &format!(" {}", "━".repeat(30)),
            "",
            " Some bold and italic text that",
            " wraps across lines.",
            "",
            "",
        ]
    );
}

/// Wide characters take two columns, so a line wraps before it overflows the screen.
///
/// Why it matters: an emoji counted as one column pushes text past the edge and garbles the row.
#[test]
fn wraps_wide_characters_by_their_display_width() {
    // Given: a paragraph with a joined emoji, and a run of CJK text with no spaces
    let markdown = "Text 🧑🏻‍🍳 chefs cook well\n\n日本語のテキストです\n";

    // When: it is drawn with 10 columns for text
    let screen = shown(markdown, 12, 7);

    // Then: the emoji stays whole, and the unbroken run splits between characters
    assert_eq!(
        screen,
        [
            " Text 🧑🏻‍🍳",
            " chefs cook",
            " well",
            "",
            " 日本語のテ",
            " キストです",
        ]
    );
}

/// Lists show their markers, and wrapped item text lines up under the item, not the marker.
///
/// Why it matters: a continuation line under the marker makes one item read as two.
#[test]
fn renders_nested_and_ordered_lists() {
    // Given: a bullet list with a nested list, then an ordered list
    let markdown = "- one\n- two is longer than the width\n  - nested\n\n1. first\n2. second\n";

    // When: it is drawn with 20 columns for text
    let screen = shown(markdown, 22, 9);

    // Then: the markers lead, and the nested list indents under its parent's text
    assert_eq!(
        screen,
        [
            " • one",
            " • two is longer than",
            "   the width",
            "   • nested",
            "",
            " 1. first",
            " 2. second",
            "",
        ]
    );
}

/// A block quote keeps its bar on every wrapped line.
///
/// Why it matters: without the bar, the second line of a quote reads as the author's own text.
#[test]
fn keeps_the_quote_bar_on_wrapped_lines() {
    // Given: a quote longer than the screen
    let markdown = "> A quote that wraps onto two lines\n";

    // When: it is drawn with 20 columns for text
    let screen = shown(markdown, 22, 4);

    // Then: both lines carry the bar
    assert_eq!(screen, [" ▎ A quote that wraps", " ▎ onto two lines", ""]);
}

/// A code block keeps its indentation and line breaks, and hard-wraps a line wider than the screen.
///
/// Why it matters: reflowing code like prose destroys the indentation that carries its meaning.
#[test]
fn keeps_code_block_layout() {
    // Given: a fenced code block with an indented line and a line wider than the screen
    let markdown = "```rust\nfn main() {\n    print!(\"hi\");\n}\nlet long = 1234567890;\n```\n";

    // When: it is drawn with 20 columns for text, 18 inside the block's padding
    let screen = shown(markdown, 22, 10);

    // Then: the lines keep their layout inside a padded block
    assert_eq!(
        screen,
        [
            "",
            "  fn main() {",
            "      print!(\"hi\");",
            "  }",
            "  let long = 1234567",
            "  890;",
            "",
            "",
            "",
        ]
    );
}

/// A thematic break draws a rule across the text width.
///
/// Why it matters: the break separates sections, and without it two sections run together.
#[test]
fn draws_a_thematic_break() {
    // Given: two paragraphs with a break between them
    let markdown = "above\n\n---\n\nbelow\n";

    // When: it is drawn with 20 columns for text
    let screen = shown(markdown, 22, 6);

    // Then: a rule sits between them
    assert_eq!(
        screen,
        [" above", "", &format!(" {}", "─".repeat(20)), "", " below"]
    );
}

/// Each kind of text takes its colour and weight from the Rose Pine reading palette.
///
/// Why it matters: the styles are how a reader tells a heading, a link and code apart at a glance.
#[test]
fn styles_text_with_the_reading_palette() {
    // Given: a heading, and a paragraph with bold, italic, struck, linked and code text
    let markdown =
        "# Head\n\n### Sub\n\n**bold** *soft* ~~gone~~ [link](https://example.com) `code`\n";

    // When: it is drawn
    let buffer = draw(&mut Viewer::new("doc.md", markdown), 60, 10);
    let cell = |text: &str| buffer[find(&buffer, text)].clone();

    // Then: each kind carries its own style
    assert!(cell("Head").modifier.contains(Modifier::BOLD));
    assert_eq!(cell("━").fg, theme::ROSE);
    assert_eq!(cell("Sub").fg, theme::FOAM);
    assert!(cell("bold").modifier.contains(Modifier::BOLD));
    assert!(cell("soft").modifier.contains(Modifier::ITALIC));
    assert_eq!(cell("soft").fg, theme::SUBTLE);
    assert!(cell("gone").modifier.contains(Modifier::CROSSED_OUT));
    assert_eq!(cell("link").fg, theme::FOAM);
    assert_eq!(cell("code").fg, theme::ROSE);
    assert_eq!(cell("code").bg, theme::OVERLAY);
}

/// The scroll keys move through the page, stop at both ends, and the status line shows where.
///
/// Why it matters: a scroll that runs past the end shows a blank screen and loses the reader.
#[test]
fn scrolls_with_the_keys_and_stops_at_the_ends() {
    // Given: a list of 20 items on a screen with 5 rows for text
    let markdown: String = (1..=20).map(|n| format!("- item {n}\n")).collect();
    let mut viewer = Viewer::new("doc.md", &markdown);
    let first_row = |viewer: &mut Viewer| body(&draw(viewer, 30, 6))[0].clone();
    assert_eq!(first_row(&mut viewer), " • item 1");
    assert!(status(&draw(&mut viewer, 30, 6)).ends_with("25%"));

    // When: the reader scrolls up at the top
    press(&mut viewer, KeyCode::Char('k'));
    // Then: the page stays at the top
    assert_eq!(first_row(&mut viewer), " • item 1");

    // When: the reader scrolls down one line, then one page
    press(&mut viewer, KeyCode::Char('j'));
    assert_eq!(first_row(&mut viewer), " • item 2");
    press(&mut viewer, KeyCode::PageDown);
    // Then: the page moves by the rows it shows
    assert_eq!(first_row(&mut viewer), " • item 7");

    // When: the reader jumps to the end
    press(&mut viewer, KeyCode::Char('G'));
    // Then: the last item sits on the last row, and the status line says 100%
    let buffer = draw(&mut viewer, 30, 6);
    assert_eq!(
        body(&buffer),
        [
            " • item 16",
            " • item 17",
            " • item 18",
            " • item 19",
            " • item 20"
        ]
    );
    assert!(status(&buffer).starts_with(" doc.md"));
    assert!(status(&buffer).ends_with("100%"));

    // When: the reader scrolls down at the end, then jumps to the top
    press(&mut viewer, KeyCode::Char('j'));
    assert_eq!(first_row(&mut viewer), " • item 16");
    press(&mut viewer, KeyCode::Char('g'));
    // Then: the page is back at the top
    assert_eq!(first_row(&mut viewer), " • item 1");
}

/// The quit keys end the viewer, and other keys keep it open.
///
/// Why it matters: a viewer that ignores q traps the reader in the terminal.
#[test]
fn quits_on_q() {
    // Given: an open viewer
    let mut viewer = Viewer::new("doc.md", "text\n");

    // Then: a scroll key keeps it open, and q ends it
    assert_eq!(press(&mut viewer, KeyCode::Char('j')), Flow::Continue);
    assert_eq!(press(&mut viewer, KeyCode::Char('q')), Flow::Quit);
}

/// A resize reflows the text and keeps the block the reader was on at the top.
///
/// Why it matters: a resize that jumps to another part of the page loses the reader's place.
#[test]
fn keeps_the_reading_position_across_a_resize() {
    // Given: three paragraphs, scrolled so the second one is at the top of a wide screen, where
    // the first one also wraps when narrow and so pushes the second one down
    let markdown =
        "first paragraph that also wraps\n\nsecond paragraph that wraps when narrow\n\nthird\n";
    let mut viewer = Viewer::new("doc.md", markdown);
    draw(&mut viewer, 60, 4);
    press(&mut viewer, KeyCode::Char('j'));
    press(&mut viewer, KeyCode::Char('j'));
    assert_eq!(
        body(&draw(&mut viewer, 60, 4))[0],
        " second paragraph that wraps when narrow"
    );

    // When: the screen narrows so the paragraph wraps
    let screen = body(&draw(&mut viewer, 22, 4));

    // Then: the second paragraph still starts the screen
    assert_eq!(screen, [" second paragraph", " that wraps when", " narrow"]);
}

/// Raw HTML shows as muted source text, and an HTML comment shows nothing, as in a rendered page.
///
/// Why it matters: a comment is a note to the author, and printing it puts hidden text in front of
/// the reader, while dropping other HTML would lose content without a trace.
#[test]
fn shows_html_as_source_and_hides_comments() {
    // Given: an HTML block, a paragraph with inline HTML and an inline comment, and a block comment
    let markdown = "<details>\n<summary>More</summary>\n</details>\n\nkeep <kbd>K</kbd><!-- note --> this\n\n<!--\nhidden\n-->\n\nend\n";

    // When: it is drawn
    let buffer = draw(&mut Viewer::new("doc.md", markdown), 40, 9);

    // Then: the HTML shows in the muted colour, and neither comment appears
    assert_eq!(
        body(&buffer),
        [
            " <details>",
            " <summary>More</summary>",
            " </details>",
            "",
            " keep <kbd>K</kbd> this",
            "",
            " end",
            "",
        ]
    );
    assert_eq!(buffer[find(&buffer, "<summary>")].fg, theme::MUTED);
    assert_eq!(buffer[find(&buffer, "<kbd>")].fg, theme::MUTED);
}

const SECTIONS: &str = "intro\n\n## Notes\n\nfirst section\n\n## Notes\n\nline one\n\nline two\n\nline three\n\nline four\n";

/// Scrolls `SECTIONS` so that "line two", five lines under the second Notes heading, is at the top.
fn reading_line_two() -> Viewer {
    let mut viewer = Viewer::new("doc.md", SECTIONS);
    draw(&mut viewer, 30, 4);
    for _ in 0..12 {
        press(&mut viewer, KeyCode::Char('j'));
    }
    assert_eq!(body(&draw(&mut viewer, 30, 4))[0], " line two");
    viewer
}

/// A reload keeps the reader in their section, even when the edit adds lines above it.
///
/// Why it matters: a save that throws the reader to another part of the page breaks the preview
/// beside the editor, which saves often.
#[test]
fn keeps_the_section_across_a_reload_that_adds_lines_above() {
    // Given: a reader five lines into the second of two sections that share a heading
    let mut viewer = reading_line_two();

    // When: the file is saved with two new paragraphs at the top and an edit below the reader
    let edited = SECTIONS.replace("line three", "line three edited");
    viewer.reload(&format!("new para\n\nanother\n\n{edited}"));

    // Then: the edit shows, and the same line is still at the top, under the same heading
    assert_eq!(
        body(&draw(&mut viewer, 30, 4)),
        [" line two", "", " line three edited"]
    );
}

/// A reload whose edit removed the reader's heading keeps the same source line at the top.
///
/// Why it matters: with no heading to anchor to, the source line is the best guess left, and
/// jumping to the top would lose the reader entirely.
#[test]
fn keeps_the_source_line_when_the_heading_is_gone() {
    // Given: a reader five lines into the second Notes section
    let mut viewer = reading_line_two();

    // When: the file is saved with that heading renamed and an edit below the reader
    let edited = SECTIONS
        .replacen("## Notes\n\nline one", "## Usage\n\nline one", 1)
        .replace("line three", "line three edited");
    viewer.reload(&edited);

    // Then: the edit shows, and the same line is still at the top
    assert_eq!(
        body(&draw(&mut viewer, 30, 4)),
        [" line two", "", " line three edited"]
    );
}

/// A file that cannot be read shows the error in the status line and keeps the last render.
///
/// Why it matters: an editor saves by replacing the file, and a reload that blanks the page in
/// that moment makes the preview flicker or go empty.
#[test]
fn shows_a_read_error_and_keeps_the_last_render() {
    // Given: a reader on line two
    let mut viewer = reading_line_two();

    // When: a reload fails
    viewer.show_error("doc.md: No such file or directory");

    // Then: the status line names the error, and the page stays as it was
    let buffer = draw(&mut viewer, 60, 4);
    assert!(status(&buffer).contains("No such file or directory"));
    assert_eq!(body(&buffer)[0], " line two");

    // When: the next reload succeeds
    viewer.reload(SECTIONS);

    // Then: the error is gone
    assert!(!status(&draw(&mut viewer, 60, 4)).contains("No such file"));
}

/// Code takes the colours of its language, and a wrapped line keeps the panel behind it.
///
/// Why it matters: colour is how a reader scans code for its strings, keywords and comments, and a
/// gap in the panel on a wrapped row breaks the block apart.
#[test]
fn highlights_code_by_its_language() {
    // Given: a Rust block with a line wider than the panel, and a TypeScript block
    let markdown = "```rust\nfn main() { let s = \"hi\"; } // a note that runs past the edge\n```\n\n```ts\nconst count: number = 1;\n```\n";

    // When: it is drawn with 40 columns for text
    let buffer = draw(&mut Viewer::new("doc.md", markdown), 42, 12);
    let cell = |text: &str| buffer[find(&buffer, text)].clone();

    // Then: keywords, strings and comments each take their colour
    assert_eq!(cell("fn").fg, theme::PINE);
    assert_eq!(cell("\"hi\"").fg, theme::GOLD);
    assert_eq!(cell("// a note").fg, theme::MUTED);
    assert!(cell("// a note").modifier.contains(Modifier::ITALIC));
    assert_eq!(cell("const").fg, theme::PINE);

    // And: the wrapped row of the comment keeps the panel to its last column
    let (_, wrapped) = find(&buffer, "past the edge");
    assert_eq!(buffer[(40, wrapped)].bg, theme::SURFACE);
}

/// Code in an unknown language, or with no language, shows as plain text on the panel.
///
/// Why it matters: a guessed grammar colours code wrongly, which reads worse than no colour.
#[test]
fn shows_code_in_an_unknown_language_as_plain_text() {
    // Given: a block in a language with no grammar, and a block with no language
    let markdown = "```nosuchlang\nfn unknown\n```\n\n```\nfn bare\n```\n";

    // When: it is drawn
    let buffer = draw(&mut Viewer::new("doc.md", markdown), 42, 10);

    // Then: both show in the plain text colour
    assert_eq!(buffer[find(&buffer, "fn unknown")].fg, theme::TEXT);
    assert_eq!(buffer[find(&buffer, "fn bare")].fg, theme::TEXT);
}

/// A table draws its cells in a grid, aligned the way its delimiter row says.
///
/// Why it matters: a table is the densest content in the notes, and columns that do not line up
/// cannot be read across.
#[test]
fn draws_a_table_aligned_by_its_delimiter_row() {
    // Given: a table with a left, a centred and a right column
    let markdown =
        "| Name | Kind | Size |\n|:-----|:----:|-----:|\n| alpha | x | 1 |\n| beta | yy | 22 |\n";

    // When: it is drawn with room to spare
    let buffer = draw(&mut Viewer::new("doc.md", markdown), 42, 8);

    // Then: the grid fits its content, and each column takes its alignment
    assert_eq!(
        body(&buffer)[..6],
        [
            " ┌───────┬──────┬──────┐",
            " │ Name  │ Kind │ Size │",
            " ├───────┼──────┼──────┤",
            " │ alpha │  x   │    1 │",
            " │ beta  │  yy  │   22 │",
            " └───────┴──────┴──────┘",
        ]
    );
    assert_eq!(buffer[find(&buffer, "Name")].fg, theme::SUBTLE);
    assert!(
        buffer[find(&buffer, "Name")]
            .modifier
            .contains(Modifier::BOLD)
    );
    assert_eq!(buffer[find(&buffer, "┌")].fg, theme::HL_MED);
}

/// A table wider than the screen narrows its widest column and wraps the cells in it.
///
/// Why it matters: a clipped table hides the end of every long cell, which is where the detail is.
#[test]
fn wraps_the_cells_of_a_table_wider_than_the_screen() {
    // Given: a table whose second column is wider than the screen
    let markdown = "| Key | Text |\n|-----|------|\n| a | a long text that wraps over lines |\n| b | short |\n";

    // When: it is drawn with 20 columns for text
    let screen = shown(markdown, 22, 12);

    // Then: the wide column wraps, and rule lines separate the rows that now span several lines
    assert_eq!(
        screen[..9],
        [
            " ┌─────┬────────────┐",
            " │ Key │ Text       │",
            " ├─────┼────────────┤",
            " │ a   │ a long     │",
            " │     │ text that  │",
            " │     │ wraps over │",
            " │     │ lines      │",
            " ├─────┼────────────┤",
            " │ b   │ short      │",
        ]
    );
    assert_eq!(screen[9], " └─────┴────────────┘");
}

/// A task list shows a box per item, ticked for a done item.
///
/// Why it matters: the box is the whole point of a task list, and without it done and open items
/// read alike.
#[test]
fn shows_task_list_boxes() {
    // Given: an open task and a done task
    let markdown = "- [ ] write tests\n- [x] ship\n";

    // When: it is drawn
    let screen = shown(markdown, 30, 4);

    // Then: each item shows its box in place of the bullet
    assert_eq!(screen[..2], [" ☐ write tests", " ☑ ship"]);
}

/// An alert block shows its kind as a label, and its bar and label take the kind's colour.
///
/// Why it matters: a warning that looks like a plain quote is a warning nobody notices.
#[test]
fn labels_and_colours_alert_blocks() {
    // Given: a warning alert, then a plain quote
    let markdown = "> [!WARNING]\n> Mind the gap.\n\n> plain quote\n";

    // When: it is drawn
    let buffer = draw(&mut Viewer::new("doc.md", markdown), 30, 6);

    // Then: the alert carries its label over its text, and the plain quote has none
    assert_eq!(
        body(&buffer),
        [" ▎ Warning", " ▎ Mind the gap.", "", " ▎ plain quote", ""]
    );
    assert_eq!(buffer[(1, 0)].fg, theme::GOLD);
    assert_eq!(buffer[find(&buffer, "Warning")].fg, theme::GOLD);
    assert_eq!(buffer[find(&buffer, "Mind")].fg, theme::TEXT);
    assert_eq!(buffer[(1, 3)].fg, theme::ROSE);
}

/// Footnote references show as superscript numbers in the order they are read, and the notes
/// gather at the end of the page.
///
/// Why it matters: a note shown where it is defined interrupts the text, and a number that
/// follows the source order instead of the reading order sends the reader to the wrong note.
#[test]
fn numbers_footnotes_in_reading_order_and_lists_them_at_the_end() {
    // Given: two references, with their notes defined in the other order, mid-page
    let markdown = "Text[^b] and[^a].\n\n[^a]: Ay.\n[^b]: Bee.\n\nAfter.\n";

    // When: it is drawn with 28 columns for text
    let screen = shown(markdown, 30, 11);

    // Then: the text reads on, and the notes follow a rule at the end
    assert_eq!(
        screen,
        [
            " Text¹ and².",
            "",
            " After.",
            "",
            &format!(" {}", "─".repeat(28)),
            "",
            " 1. Bee.",
            "",
            " 2. Ay.",
            "",
        ]
    );
}

/// A resize keeps the reader's place on a page whose notes are defined in the middle.
///
/// Why it matters: the notes draw at the end but come from the middle of the source, and a
/// position that maps through them lands the reader on the wrong paragraph.
#[test]
fn keeps_the_reading_position_on_a_page_with_notes_defined_mid_page() {
    // Given: a note defined mid-page, and a reader scrolled to the last paragraph
    let markdown = "Intro[^n] text\n\n[^n]: Note.\n\nmiddle\n\nend paragraph\n";
    let mut viewer = Viewer::new("doc.md", markdown);
    draw(&mut viewer, 60, 4);
    for _ in 0..4 {
        press(&mut viewer, KeyCode::Char('j'));
    }
    assert_eq!(body(&draw(&mut viewer, 60, 4))[0], " end paragraph");

    // When: the screen narrows
    let screen = body(&draw(&mut viewer, 40, 4));

    // Then: the same paragraph is still at the top
    assert_eq!(screen[0], " end paragraph");
}
