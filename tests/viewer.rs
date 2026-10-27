#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use mdlive::{Flow, Viewer, theme};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::Modifier;
use unicode_width::UnicodeWidthStr;

mod common;

use common::{HEADER_ROWS, below_header};

fn draw_screen(viewer: &mut Viewer, width: u16, height: u16) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| viewer.draw(frame)).unwrap();
    terminal.backend().buffer().clone()
}

/// The `height` rows under the breadcrumb row of a screen one row taller.
fn draw(viewer: &mut Viewer, width: u16, height: u16) -> Buffer {
    below_header(&draw_screen(viewer, width, height + HEADER_ROWS))
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

/// The rows of the page, above the hint bar and the status line.
fn body(buffer: &Buffer) -> Vec<String> {
    let mut all = rows(buffer);
    all.truncate(all.len() - 2);
    all
}

fn hint_bar(buffer: &Buffer) -> String {
    let all = rows(buffer);
    all[all.len() - 2].clone()
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

/// Presses each character as a key, the way a reader types `5j` or `gg`.
fn type_keys(viewer: &mut Viewer, keys: &str) -> Flow {
    keys.chars()
        .map(|key| press(viewer, KeyCode::Char(key)))
        .last()
        .unwrap_or(Flow::Continue)
}

fn control(viewer: &mut Viewer, key: char) -> Flow {
    viewer.handle_key(KeyEvent::new(KeyCode::Char(key), KeyModifiers::CONTROL))
}

/// The cells of each body row from column `from` up to `to`, untrimmed, for one side of a split.
fn columns(buffer: &Buffer, from: u16, to: u16) -> Vec<String> {
    (0..buffer.area.height - 2)
        .map(|y| (from..to).map(|x| buffer[(x, y)].symbol()).collect())
        .collect()
}

/// A row of the contents panel: 27 columns of entry text, then the marker column.
fn entry(text: &str, marked: bool) -> String {
    format!("│{text:<27}{}│", if marked { '◀' } else { ' ' })
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
    let screen = shown(markdown, 32, 9);

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
    let screen = shown(markdown, 12, 8);

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
    let screen = shown(markdown, 22, 10);

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
    let screen = shown(markdown, 22, 5);

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
    let screen = shown(markdown, 22, 11);

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
    let screen = shown(markdown, 22, 7);

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
    let buffer = draw(&mut Viewer::new("doc.md", markdown), 60, 11);
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
    let first_row = |viewer: &mut Viewer| body(&draw(viewer, 30, 7))[0].clone();
    assert_eq!(first_row(&mut viewer), " • item 1");
    assert!(status(&draw(&mut viewer, 30, 7)).ends_with("25%"));

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
    let buffer = draw(&mut viewer, 30, 7);
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
    type_keys(&mut viewer, "gg");
    // Then: the page is back at the top
    assert_eq!(first_row(&mut viewer), " • item 1");
}

/// The vim control keys scroll by a line, half a page, or a whole page, in both directions.
///
/// Why it matters: a reader who drives nvim all day presses these keys first, and a key that does
/// nothing breaks their flow.
#[test]
fn scrolls_by_lines_and_pages_with_the_vim_control_keys() {
    // Given: a list of 40 items on a screen with 10 rows for text
    let markdown: String = (1..=40).map(|n| format!("- item {n}\n")).collect();
    let mut viewer = Viewer::new("doc.md", &markdown);
    let first_row = |viewer: &mut Viewer, key: char| {
        control(viewer, key);
        body(&draw(viewer, 30, 12))[0].clone()
    };
    draw(&mut viewer, 30, 12);

    // Then: C-d and C-u move half a page, C-f and C-b a page, C-e and C-y a line
    assert_eq!(first_row(&mut viewer, 'd'), " • item 6");
    assert_eq!(first_row(&mut viewer, 'f'), " • item 16");
    assert_eq!(first_row(&mut viewer, 'e'), " • item 17");
    assert_eq!(first_row(&mut viewer, 'y'), " • item 16");
    assert_eq!(first_row(&mut viewer, 'b'), " • item 6");
    assert_eq!(first_row(&mut viewer, 'u'), " • item 1");
}

/// A count typed before a motion repeats it, as in vim.
///
/// Why it matters: without counts a reader has to press j twenty times to skip twenty lines.
#[test]
fn repeats_a_motion_by_the_count_typed_before_it() {
    // Given: a list of 40 items on a screen with 10 rows for text
    let markdown: String = (1..=40).map(|n| format!("- item {n}\n")).collect();
    let mut viewer = Viewer::new("doc.md", &markdown);
    let first_row = |viewer: &mut Viewer| body(&draw(viewer, 30, 12))[0].clone();
    draw(&mut viewer, 30, 12);

    // When: the reader types 5j, then 10j
    type_keys(&mut viewer, "5j");
    // Then: the page moves 5 lines, then 10 more
    assert_eq!(first_row(&mut viewer), " • item 6");
    type_keys(&mut viewer, "10j");
    assert_eq!(first_row(&mut viewer), " • item 16");

    // When: the reader types 2 and C-u
    type_keys(&mut viewer, "2");
    control(&mut viewer, 'u');
    // Then: the page moves up two half pages
    assert_eq!(first_row(&mut viewer), " • item 6");

    // When: the reader types 3k
    type_keys(&mut viewer, "3k");
    // Then: the count applies once and is gone, so a following j moves one line
    assert_eq!(first_row(&mut viewer), " • item 3");
    type_keys(&mut viewer, "j");
    assert_eq!(first_row(&mut viewer), " • item 4");
}

/// gg goes to the top, G to the end, and a count before either goes to that source line.
///
/// Why it matters: the count is a line number in the editor, so 30G must show the text that nvim
/// shows on line 30, not the 30th line of the wrapped screen.
#[test]
fn goes_to_the_top_the_end_or_a_source_line() {
    // Given: twelve paragraphs that each wrap to two screen lines, one per source line pair
    let markdown: String = (1..=12)
        .map(|n| format!("paragraph {n} wraps here\n\n"))
        .collect();
    let mut viewer = Viewer::new("doc.md", &markdown);
    let first_row = |viewer: &mut Viewer| body(&draw(viewer, 14, 6))[0].clone();
    draw(&mut viewer, 14, 6);

    // When: the reader types 9G
    type_keys(&mut viewer, "9G");
    // Then: paragraph 5, which starts on source line 9, is at the top
    assert_eq!(first_row(&mut viewer), " paragraph 5");

    // When: the reader types G, then gg
    type_keys(&mut viewer, "G");
    assert_eq!(
        body(&draw(&mut viewer, 14, 6)),
        [" wraps here", "", " paragraph 12", " wraps here"]
    );
    type_keys(&mut viewer, "gg");
    // Then: the page is at the end, then back at the top
    assert_eq!(first_row(&mut viewer), " paragraph 1");

    // When: the reader types 5gg
    type_keys(&mut viewer, "5gg");
    // Then: paragraph 3, which starts on source line 5, is at the top
    assert_eq!(first_row(&mut viewer), " paragraph 3");
    // And: the count is spent, so a following j moves one line
    type_keys(&mut viewer, "j");
    assert_eq!(first_row(&mut viewer), " wraps here");

    // And: in a list, where every item has its own source line, 7G shows item 7
    let markdown: String = (1..=40).map(|n| format!("- item {n}\n")).collect();
    let mut list = Viewer::new("doc.md", &markdown);
    draw(&mut list, 30, 12);
    type_keys(&mut list, "7G");
    assert_eq!(body(&draw(&mut list, 30, 12))[0], " • item 7");
}

/// Keys typed toward a command show in the status line, and Escape clears them before it quits.
///
/// Why it matters: a pending count that the reader cannot see changes the next motion by surprise,
/// and an Escape that quits while a count is pending closes the viewer by accident.
#[test]
fn shows_pending_keys_and_clears_them_on_escape() {
    // Given: a list of 40 items on a screen with 10 rows for text
    let markdown: String = (1..=40).map(|n| format!("- item {n}\n")).collect();
    let mut viewer = Viewer::new("doc.md", &markdown);
    draw(&mut viewer, 30, 12);

    // When: the reader types 12g
    type_keys(&mut viewer, "12g");
    // Then: the status line shows the pending keys before the percentage
    assert!(status(&draw(&mut viewer, 30, 12)).ends_with("12g  25%"));

    // When: the reader presses Escape
    // Then: the pending keys clear and the viewer stays open
    assert_eq!(press(&mut viewer, KeyCode::Esc), Flow::Continue);
    assert!(status(&draw(&mut viewer, 30, 12)).ends_with(" 25%"));
    assert!(!status(&draw(&mut viewer, 30, 12)).contains("12g"));

    // When: the reader types g and a key that completes nothing, then j
    type_keys(&mut viewer, "gxj");
    // Then: the unfinished g is dropped and j moves one line
    assert_eq!(body(&draw(&mut viewer, 30, 12))[0], " • item 2");

    // When: the reader presses Escape with nothing pending
    // Then: the viewer quits
    assert_eq!(press(&mut viewer, KeyCode::Esc), Flow::Quit);
}

/// ]] and [[ put the next or previous heading at the top, as vim's section motions do.
///
/// Why it matters: a reader skims a long page by its sections, and a jump that lands one line off
/// hides the heading it was meant to show.
#[test]
fn jumps_between_headings_with_the_section_keys() {
    // Given: three sections, then enough text that every heading can reach the top
    let filler: String = (1..=8).map(|n| format!("filler {n}\n\n")).collect();
    let markdown =
        format!("intro\n\n## Two\n\ntwo text\n\n## Three\n\nthree text\n\n## Four\n\n{filler}");
    let mut viewer = Viewer::new("doc.md", &markdown);
    let first_row = |viewer: &mut Viewer| body(&draw(viewer, 30, 7))[0].clone();
    draw(&mut viewer, 30, 7);

    // When: the reader types ]], then 2]]
    type_keys(&mut viewer, "]]");
    // Then: the next heading is at the top, then the one two further on
    assert_eq!(first_row(&mut viewer), " Two");
    type_keys(&mut viewer, "2]]");
    assert_eq!(first_row(&mut viewer), " Four");

    // When: the reader scrolls into the section, then types [[
    type_keys(&mut viewer, "3j[[");
    // Then: the heading of the section the reader is in comes back to the top
    assert_eq!(first_row(&mut viewer), " Four");

    // When: the reader types [[ again, then 5[[
    type_keys(&mut viewer, "[[");
    assert_eq!(first_row(&mut viewer), " Three");
    type_keys(&mut viewer, "5[[");
    // Then: with no heading left above, the page goes to the top, as vim does
    assert_eq!(first_row(&mut viewer), " intro");

    // When: the reader types 9]]
    type_keys(&mut viewer, "9]]");
    // Then: with no heading left below, the page goes to the end
    assert!(status(&draw(&mut viewer, 30, 7)).ends_with("100%"));
}

/// } and { move by the blocks that a blank line separates, so a tight list is one block.
///
/// Why it matters: vim's paragraph motion stops at blank lines, and a reader who skips a list
/// expects to land after it, not on its second item.
#[test]
fn jumps_between_blocks_with_the_paragraph_keys() {
    // Given: a paragraph, a tight list, two more paragraphs, then enough text to scroll
    let filler: String = (1..=8).map(|n| format!("filler {n}\n\n")).collect();
    let markdown = format!("first\n\n- one\n- two\n- three\n\nsecond\n\nthird\n\n{filler}");
    let mut viewer = Viewer::new("doc.md", &markdown);
    let first_row = |viewer: &mut Viewer| body(&draw(viewer, 30, 7))[0].clone();
    draw(&mut viewer, 30, 7);

    // When: the reader types }, then } again
    type_keys(&mut viewer, "}");
    // Then: the list is at the top, then the paragraph after the whole list
    assert_eq!(first_row(&mut viewer), " • one");
    type_keys(&mut viewer, "}");
    assert_eq!(first_row(&mut viewer), " second");

    // When: the reader types 2}, then {
    type_keys(&mut viewer, "2}");
    assert_eq!(first_row(&mut viewer), " filler 1");
    type_keys(&mut viewer, "{");
    // Then: the block before is at the top
    assert_eq!(first_row(&mut viewer), " third");

    // When: the reader scrolls into the list, then types {
    type_keys(&mut viewer, "gg3j{");
    // Then: the start of the list comes back to the top
    assert_eq!(first_row(&mut viewer), " • one");
}

/// t opens the contents beside the page, indented by level, with a marker on the section in view.
///
/// Why it matters: the contents is how a reader sees where they are in a long page, so a marker
/// that does not follow the scroll or a reload points them at the wrong section.
#[test]
fn shows_the_contents_beside_the_page_and_marks_the_section_in_view() {
    // Given: headings at three levels, then enough text to scroll, on a wide screen
    let filler: String = (1..=8).map(|n| format!("filler {n}\n\n")).collect();
    let markdown = format!(
        "# Guide\n\nintro\n\n## Install\n\ntext\n\n### Flags\n\ntext\n\n## Usage\n\n{filler}"
    );
    let mut viewer = Viewer::new("doc.md", &markdown);
    draw(&mut viewer, 80, 13);

    // When: the reader presses t
    type_keys(&mut viewer, "t");
    let buffer = draw(&mut viewer, 80, 13);
    let panel = columns(&buffer, 0, 30);

    // Then: the panel lists the headings under its title, indented by level, marking the first
    assert!(panel[0].starts_with("┌ Contents "));
    assert_eq!(
        panel[1..6],
        [
            entry(" [a] Guide", true),
            entry(" [s]   Install", false),
            entry(" [d]     Flags", false),
            entry(" [f]   Usage", false),
            entry("", false),
        ]
    );
    // And: the page moves right of the panel
    assert_eq!(find(&buffer, "intro"), (31, 3));

    // When: the reader jumps two sections on and scrolls one line into the section
    type_keys(&mut viewer, "2]]j");
    // Then: the marker follows to that section
    let panel = columns(&draw(&mut viewer, 80, 13), 0, 30);
    assert_eq!(
        panel[2..4],
        [
            entry(" [s]   Install", false),
            entry(" [d]     Flags", true)
        ]
    );

    // When: the file gains a paragraph above
    viewer.reload(&markdown.replace("intro\n\n", "intro\n\nadded\n\n"));
    // Then: the marker stays on the section
    let panel = columns(&draw(&mut viewer, 80, 13), 0, 30);
    assert_eq!(panel[3], entry(" [d]     Flags", true));

    // When: the reader presses t again
    type_keys(&mut viewer, "t");
    // Then: the panel closes and the page has the full width again
    let buffer = draw(&mut viewer, 80, 13);
    assert!(!rows(&buffer).iter().any(|row| row.contains("Contents")));
    assert_eq!(find(&buffer, "Flags").0, 1);
}

/// A long contents scrolls to keep the marked entry in view, cuts long headings, and says when a
/// page has no headings.
///
/// Why it matters: a marker scrolled out of the panel, or a heading that runs into the page, makes
/// the contents unreadable on exactly the long pages that need it.
#[test]
fn keeps_the_marked_entry_in_view_and_cuts_long_headings() {
    // Given: a long first heading, then 30 short sections, on a screen with 7 rows for entries
    let title = "A heading far too long to fit in the panel";
    let sections: String = (1..=30)
        .map(|n| format!("## Section {n}\n\ntext\n\n"))
        .collect();
    let mut viewer = Viewer::new("doc.md", &format!("## {title}\n\n{sections}"));
    draw(&mut viewer, 80, 11);
    type_keys(&mut viewer, "t");

    // Then: the long heading is cut to the panel with an ellipsis
    let panel = columns(&draw(&mut viewer, 80, 11), 0, 30);
    assert_eq!(panel[1], entry(&format!(" [aa] {}…", &title[..20]), true));

    // When: the reader goes to the end
    type_keys(&mut viewer, "G");
    // Then: the panel scrolls to its last entries, marking Section 29, whose heading is the top
    // line, and the first entry has scrolled away
    let panel = columns(&draw(&mut viewer, 80, 11), 0, 30);
    assert_eq!(
        panel[6..8],
        [
            entry(" [so] Section 29", true),
            entry(" [sp] Section 30", false)
        ]
    );
    assert!(!panel.iter().any(|row| row.contains("A heading")));

    // And: a page with no headings says so
    let mut plain = Viewer::new("doc.md", "just text\n");
    type_keys(&mut plain, "t");
    let panel = columns(&draw(&mut plain, 80, 11), 0, 30);
    assert_eq!(panel[1], entry(" No headings", false));
}

/// On a narrow screen the contents covers the left of the page instead of squeezing the text.
///
/// Why it matters: text squeezed below 40 columns wraps every few words and is hard to read, and a
/// reflow there moves the reader's place each time the panel opens.
#[test]
fn covers_the_page_on_a_narrow_screen() {
    // Given: a paragraph that wraps on a 60-column screen
    let markdown = format!("# Title\n\n{}\n", "word ".repeat(30));
    let mut viewer = Viewer::new("doc.md", &markdown);
    let before = columns(&draw(&mut viewer, 60, 9), 30, 60);

    // When: the reader presses t
    type_keys(&mut viewer, "t");
    let buffer = draw(&mut viewer, 60, 9);

    // Then: the panel shows on the left, and the page beside it is laid out as before
    assert!(columns(&buffer, 0, 30)[0].starts_with("┌ Contents "));
    assert_eq!(columns(&buffer, 30, 60), before);
}

/// The contents opens at start on a page with 3 or more headings when the screen holds it beside a
/// full-width page, and a resize never opens it after that.
///
/// Why it matters: a reader who does not know t never finds the contents, but a panel that opens on
/// a narrow screen squeezes the text, and one that reopens on a resize undoes the reader's choice.
#[test]
fn opens_the_contents_at_start_when_it_fits_beside_the_page() {
    let three = "# One\n\ntext\n\n## Two\n\ntext\n\n## Three\n\ntext\n";
    let has_panel = |buffer: &Buffer| columns(buffer, 0, 30)[0].starts_with("┌ Contents ");

    // Given: three headings on a screen of 132 columns, the panel and a full-width page
    let mut viewer = Viewer::new("doc.md", three);
    // Then: the first draw shows the contents
    assert!(has_panel(&draw(&mut viewer, 132, 9)));

    // When: the reader presses t, then the screen grows
    type_keys(&mut viewer, "t");
    // Then: the contents stays closed
    assert!(!has_panel(&draw(&mut viewer, 160, 9)));

    // Given: the same page on a screen one column narrower
    let mut narrow = Viewer::new("doc.md", three);
    // Then: the contents starts closed
    assert!(!has_panel(&draw(&mut narrow, 131, 9)));
    // When: the screen grows past the width
    // Then: the contents stays closed, because the choice is made at start
    assert!(!has_panel(&draw(&mut narrow, 160, 9)));

    // Given: two headings on the wide screen
    let mut short = Viewer::new("doc.md", "# One\n\ntext\n\n## Two\n\ntext\n");
    // Then: the contents starts closed
    assert!(!has_panel(&draw(&mut short, 132, 9)));
}

/// With the contents open, each entry's hint letter puts its heading at the top.
///
/// Why it matters: the hint is the one-key way to reach any section, so a hint that lands on the
/// wrong heading, or a letter that acts while the contents is closed, sends the reader elsewhere.
#[test]
fn jumps_to_a_section_by_its_hint_letter() {
    // Given: four headings, then enough text to scroll, with the contents open
    let filler: String = (1..=8).map(|n| format!("filler {n}\n\n")).collect();
    let markdown = format!(
        "# Guide\n\nintro\n\n## Install\n\ntext\n\n### Flags\n\ntext\n\n## Usage\n\n{filler}"
    );
    let mut viewer = Viewer::new("doc.md", &markdown);
    let page_top = |viewer: &mut Viewer| columns(&draw(viewer, 80, 13), 31, 80)[0].clone();
    draw(&mut viewer, 80, 13);
    type_keys(&mut viewer, "t");

    // When: the reader presses d, the hint of Flags
    type_keys(&mut viewer, "d");
    // Then: Flags is at the top of the page
    assert!(page_top(&mut viewer).starts_with("Flags"));

    // When: the reader presses x, a hint letter no entry has
    type_keys(&mut viewer, "x");
    // Then: the page stays where it is
    assert!(page_top(&mut viewer).starts_with("Flags"));

    // When: the reader closes the contents and presses s
    type_keys(&mut viewer, "ts");
    // Then: the letter does nothing, so the page still starts at Flags
    let buffer = draw(&mut viewer, 80, 13);
    assert_eq!(find(&buffer, "Flags"), (1, 0));
}

/// Past 19 headings every hint has two letters, the first shows as pending, and Escape drops it.
///
/// Why it matters: with mixed one- and two-letter hints, a one-letter hint would also start a
/// two-letter one, so the reader could never reach the longer one.
#[test]
fn uses_two_letter_hints_past_the_single_letters() {
    // Given: 25 sections, then enough text to scroll, with the contents open
    let sections: String = (1..=25)
        .map(|n| format!("## Section {n}\n\ntext\n\n"))
        .collect();
    let mut viewer = Viewer::new("doc.md", &sections);
    let page_top = |viewer: &mut Viewer| columns(&draw(viewer, 80, 11), 31, 80)[0].clone();
    draw(&mut viewer, 80, 11);
    type_keys(&mut viewer, "t");

    // When: the reader presses a
    type_keys(&mut viewer, "a");
    // Then: the page has not moved, and the status line shows a as pending
    assert!(page_top(&mut viewer).starts_with("Section 1 "));
    assert!(status(&draw(&mut viewer, 80, 11)).contains(" a  "));

    // When: the reader presses s, completing the hint as
    type_keys(&mut viewer, "s");
    // Then: Section 2 is at the top
    assert!(page_top(&mut viewer).starts_with("Section 2 "));

    // When: the reader presses s, then Escape, then a and f
    type_keys(&mut viewer, "s");
    press(&mut viewer, KeyCode::Esc);
    type_keys(&mut viewer, "af");
    // Then: the dropped s starts nothing, and af, the fourth hint, reaches Section 4
    assert!(page_top(&mut viewer).starts_with("Section 4 "));
}

/// The bar above the status line names the keys for what the reader is doing, and on a narrow
/// screen it keeps only the hints that fit whole.
///
/// Why it matters: the bar is how a reader learns that the contents, search and folds exist, so a
/// hint missing from its mode, or one cut in half, hides a feature again.
#[test]
fn shows_the_keys_for_what_the_reader_is_doing_in_a_bar() {
    // Given: a page with headings, a needle and a details block
    let markdown =
        "# One\n\nneedle\n\n## Two\n\n<details>\n<summary>More</summary>\n\nHidden\n\n</details>\n";
    let mut viewer = Viewer::new("doc.md", markdown);
    let buffer = draw(&mut viewer, 120, 10);
    // Then: the bar names the reading keys, the fold key because the page has a fold, and quit
    assert_eq!(
        hint_bar(&buffer),
        " j/k scroll  g? help  t contents  - files  ]]/[[ section  / search  za fold  C-d/C-u half page  q quit"
    );
    // And: each key stands out from its label
    let (x, y) = find(&buffer, "j/k scroll");
    assert_eq!(buffer[(x, y)].fg, theme::hint().fg.unwrap());
    assert_ne!(buffer[(x + 4, y)].fg, theme::hint().fg.unwrap());

    // When: the reader starts a search
    type_keys(&mut viewer, "/");
    // Then: the bar names the keys of the prompt
    assert_eq!(
        hint_bar(&draw(&mut viewer, 120, 10)),
        " Enter search  Esc cancel"
    );

    // When: the search runs
    type_keys(&mut viewer, "needle");
    press(&mut viewer, KeyCode::Enter);
    // Then: the bar adds the keys that step through the matches
    assert!(hint_bar(&draw(&mut viewer, 120, 10)).contains("  / search  n/N match  za fold"));

    // When: the reader opens the contents
    type_keys(&mut viewer, "t");
    // Then: the bar leads with the hint letters of the two headings, and t now closes
    assert!(
        hint_bar(&draw(&mut viewer, 120, 10))
            .starts_with(" a/s go to section  t close contents  j/k scroll")
    );

    // When: the screen is 40 columns wide, with the contents closed
    type_keys(&mut viewer, "t");
    // Then: the bar keeps the first hints that fit whole, and drops every hint after them, even
    // one short enough to fit
    assert_eq!(
        hint_bar(&draw(&mut viewer, 40, 10)),
        " j/k scroll  g? help  t contents"
    );
}

/// After a prefix key, a popup lists each key that can come next, and it closes when the command
/// completes or on Escape. A letter that starts a two-letter contents hint opens no popup.
///
/// Why it matters: a reader who types g or z has to know what can follow, and a popup that stays
/// open, or opens for a hint letter that the contents already explains, covers the page for nothing.
#[test]
fn lists_the_next_keys_after_a_prefix_key() {
    // Given: 25 sections, enough for two-letter contents hints
    let sections: String = (1..=25)
        .map(|n| format!("## Section {n}\n\ntext\n\n"))
        .collect();
    let mut viewer = Viewer::new("doc.md", &sections);
    let screen = |viewer: &mut Viewer| rows(&draw(viewer, 80, 14)).join("\n");
    draw(&mut viewer, 80, 14);

    // When: the reader presses z
    type_keys(&mut viewer, "z");
    // Then: a popup titled z lists the fold keys
    let shown = screen(&mut viewer);
    assert!(shown.contains("┌ z "), "{shown}");
    for entry in [
        "a  open or close fold",
        "R  open all folds",
        "M  close all folds",
    ] {
        assert!(shown.contains(entry), "{entry} missing from\n{shown}");
    }

    // When: the reader presses Escape
    press(&mut viewer, KeyCode::Esc);
    // Then: the popup closes
    assert!(!screen(&mut viewer).contains("open all folds"));

    // When: the reader types 3g
    type_keys(&mut viewer, "3g");
    // Then: the popup title shows the count, and the popup lists gg
    let shown = screen(&mut viewer);
    assert!(shown.contains("┌ 3g "), "{shown}");
    assert!(shown.contains("g  go to top or line N"), "{shown}");

    // When: the reader completes gg
    type_keys(&mut viewer, "g");
    // Then: the popup closes
    assert!(!screen(&mut viewer).contains("go to top"));

    // When: the reader opens the contents and presses a, the start of a two-letter hint
    type_keys(&mut viewer, "ta");
    // Then: no popup opens, and the status line shows a as pending
    let buffer = draw(&mut viewer, 80, 14);
    assert!(!rows(&buffer).iter().any(|row| row.contains("┌ a ")));
    assert!(status(&buffer).contains(" a  "));
}

/// g? or F1 opens a screen that lists every key by group, the movement keys scroll it, and q or
/// Escape closes it. Other keys do nothing while it is open, and the links under it do not open.
///
/// Why it matters: the help screen is where a reader finds a key they do not know, so a key it
/// leaves out stays hidden, and a screen that cannot scroll or close traps the reader.
#[test]
fn lists_every_key_on_a_help_screen() {
    // Given: a page with a link
    let mut viewer = Viewer::new("doc.md", "# Title\n\n[a link](https://example.com)\n");
    draw(&mut viewer, 80, 50);
    assert!(!viewer.links().is_empty());

    // When: the reader types g?
    type_keys(&mut viewer, "g?");
    let shown = body(&draw(&mut viewer, 80, 50));
    // Then: the screen lists each group, and each key with what it does
    for group in ["Move", "Sections", "Search", "Folds", "Other"] {
        assert!(
            shown.iter().any(|row| row.trim() == group),
            "{group} missing"
        );
    }
    for (keys, label) in [
        ("j, C-e, ↓", "scroll down"),
        ("gg, Home", "go to top or line N"),
        ("]]", "next section"),
        ("a, s, d, …", "go to a section, with the contents open"),
        ("/", "search forward"),
        ("za", "open or close fold"),
        ("g?, F1", "show the keys"),
        ("C-c", "quit"),
        ("Esc", "drop typed keys, leave the tree or help, or quit"),
    ] {
        assert!(
            shown.iter().any(|row| row
                .find(keys)
                .is_some_and(|at| row[at..].contains(&format!("  {label}")))),
            "{keys}  {label} missing from\n{}",
            shown.join("\n")
        );
    }
    // And: no link under the screen opens
    assert!(viewer.links().is_empty());

    // When: the screen is short and the reader presses j
    let first_row = |viewer: &mut Viewer| body(&draw(viewer, 80, 12))[0].clone();
    let before = first_row(&mut viewer);
    type_keys(&mut viewer, "j");
    // Then: the list scrolls, and the bar names the keys of the help screen
    assert_ne!(first_row(&mut viewer), before);
    assert_eq!(hint_bar(&draw(&mut viewer, 80, 12)), " j/k scroll  q close");

    // When: the reader presses G
    type_keys(&mut viewer, "G");
    // Then: the list stops at its last entry instead of scrolling past it
    let shown = body(&draw(&mut viewer, 80, 12));
    assert!(
        shown
            .last()
            .unwrap()
            .contains("drop typed keys, leave the tree or help, or quit")
    );

    // When: the reader presses t
    let shown = rows(&draw(&mut viewer, 80, 12));
    type_keys(&mut viewer, "t");
    // Then: the screen stays as it was
    assert_eq!(rows(&draw(&mut viewer, 80, 12)), shown);

    // When: the reader presses q
    // Then: the help closes, the viewer goes on, and the page and its link are back
    assert_eq!(type_keys(&mut viewer, "q"), Flow::Continue);
    assert_eq!(body(&draw(&mut viewer, 80, 12))[0], " Title");
    assert!(!viewer.links().is_empty());

    // When: the reader presses F1, then Escape
    press(&mut viewer, KeyCode::F(1));
    assert!(
        body(&draw(&mut viewer, 80, 50))
            .iter()
            .any(|row| row.trim() == "Folds")
    );
    press(&mut viewer, KeyCode::Esc);
    // Then: the help opens, then closes
    assert_eq!(body(&draw(&mut viewer, 80, 12))[0], " Title");
}

/// While the key popup is open, a link under it does not open on a click.
///
/// Why it matters: the terminal opens a link from any cell of its text, so popup text drawn over a
/// link would open that link when the reader clicks the popup.
#[test]
fn leaves_out_the_links_under_the_key_popup() {
    // Given: a page that is one long link
    let markdown = format!("[{}](https://example.com)\n", "linked words ".repeat(80));
    let mut viewer = Viewer::new("doc.md", &markdown);
    // A link area is in screen rows, and `find` counts rows below the breadcrumb.
    let covers = |viewer: &Viewer, (x, y): (u16, u16)| {
        viewer.links().iter().any(|link| {
            link.area.y == y + HEADER_ROWS && (link.area.x..link.area.right()).contains(&x)
        })
    };

    // When: the reader presses z
    type_keys(&mut viewer, "z");
    let cell = find(&draw(&mut viewer, 80, 14), "open all folds");
    // Then: no link covers the popup text
    assert!(!covers(&viewer, cell));

    // When: the reader presses Escape
    press(&mut viewer, KeyCode::Esc);
    draw(&mut viewer, 80, 14);
    // Then: the link covers that cell again
    assert!(covers(&viewer, cell));
}

/// The top row names the file and the headings above the reading position, and on a narrow
/// screen it drops the outer headings first.
///
/// Why it matters: deep in a long page the reader loses which section they are in, and a path cut
/// at its end would hide the one heading that answers that.
#[test]
fn shows_the_file_and_the_sections_above_the_reading_position() {
    // Given: a page with three levels of headings, then a second section
    let filler: String = (1..=8).map(|n| format!("filler {n}\n\n")).collect();
    let markdown = format!(
        "# Semiconductor\n\nintro\n\n## Band gap\n\ntext\n\n### Doping\n\n{filler}## Junction\n\n{filler}"
    );
    let mut viewer = Viewer::new("doc.md", &markdown);
    let header = |viewer: &mut Viewer, width| rows(&draw_screen(viewer, width, 12))[0].clone();
    // Then: at the top it names the file and the first heading
    assert_eq!(header(&mut viewer, 80), " doc.md › Semiconductor");

    // When: the reader jumps two sections on
    type_keys(&mut viewer, "2]]");
    // Then: it names every heading above, from the outermost
    assert_eq!(
        header(&mut viewer, 80),
        " doc.md › Semiconductor › Band gap › Doping"
    );

    // When: the screen is 30 columns wide
    // Then: the outer headings give way to an ellipsis, and the file and the section stay
    assert_eq!(header(&mut viewer, 30), " doc.md › … › Doping");

    // When: the reader jumps to the next section, one level up
    type_keys(&mut viewer, "]]");
    // Then: the headings below it leave the path
    assert_eq!(
        header(&mut viewer, 80),
        " doc.md › Semiconductor › Junction"
    );

    // And: a page with no headings shows the file alone
    assert_eq!(header(&mut Viewer::new("doc.md", "text\n"), 80), " doc.md");
}

/// Thirty one-line paragraphs, with "needle" in paragraphs 3, 15 and 25.
fn haystack() -> Viewer {
    let markdown: String = (1..=30)
        .map(|n| match n {
            3 | 15 | 25 => format!("line {n} needle\n\n"),
            _ => format!("line {n}\n\n"),
        })
        .collect();
    let mut viewer = Viewer::new("doc.md", &markdown);
    draw(&mut viewer, 30, 7);
    viewer
}

/// / searches forward from the page, and n and N step through the matches, wrapping at the ends.
///
/// Why it matters: search is how a reader finds a term on a long page, and a step that skips a
/// match or wraps without saying so makes them think the term is not there.
#[test]
fn steps_through_the_matches_of_a_search() {
    // Given: a page with three matches, the first one on screen
    let mut viewer = haystack();

    // When: the reader types /nee
    type_keys(&mut viewer, "/nee");
    // Then: the status line shows the pattern being typed
    assert!(status(&draw(&mut viewer, 30, 7)).starts_with(" /nee"));

    // When: the reader completes the pattern and presses Enter
    type_keys(&mut viewer, "dle");
    press(&mut viewer, KeyCode::Enter);
    // Then: the match already on screen is the current one, and the page does not move
    let buffer = draw(&mut viewer, 30, 7);
    assert_eq!(body(&buffer)[4], " line 3 needle");
    assert_eq!(buffer[(8, 4)].bg, theme::ROSE);

    // When: the reader presses n, then n again
    type_keys(&mut viewer, "n");
    // Then: each next match comes to the top as the current one
    let buffer = draw(&mut viewer, 30, 7);
    assert_eq!(body(&buffer)[0], " line 15 needle");
    assert_eq!(buffer[(9, 0)].bg, theme::ROSE);
    type_keys(&mut viewer, "n");
    assert_eq!(body(&draw(&mut viewer, 30, 7))[0], " line 25 needle");

    // When: the reader presses n past the last match
    type_keys(&mut viewer, "n");
    // Then: the search wraps to the first match and says so, on a screen wide enough to read it
    let buffer = draw(&mut viewer, 60, 7);
    assert_eq!(body(&buffer)[0], " line 3 needle");
    assert!(status(&buffer).contains("search hit BOTTOM, continuing at TOP"));

    // When: the reader presses N
    type_keys(&mut viewer, "N");
    // Then: the search goes back past the first match to the last, and says so
    let buffer = draw(&mut viewer, 60, 7);
    assert_eq!(body(&buffer)[0], " line 25 needle");
    assert!(status(&buffer).contains("search hit TOP, continuing at BOTTOM"));

    // When: the reader types 2n from the last match
    type_keys(&mut viewer, "2n");
    // Then: the count steps twice, wrapping past the end to the second match
    assert_eq!(body(&draw(&mut viewer, 30, 7))[0], " line 15 needle");
}

/// ? searches backward, a missing pattern is reported, and Escape leaves the prompt unrun.
///
/// Why it matters: a search that fails silently looks like a search that found nothing on screen,
/// and an Escape that runs the half-typed pattern moves the page the reader meant to keep.
#[test]
fn searches_backward_reports_a_missing_pattern_and_cancels_on_escape() {
    // Given: a page with three matches, at the top
    let mut viewer = haystack();

    // When: the reader types ?needle and Enter
    type_keys(&mut viewer, "?needle");
    press(&mut viewer, KeyCode::Enter);
    // Then: with nothing above the top, the search wraps to the last match
    assert_eq!(body(&draw(&mut viewer, 30, 7))[0], " line 25 needle");

    // When: the reader searches for a pattern the page does not hold
    type_keys(&mut viewer, "/absent");
    press(&mut viewer, KeyCode::Enter);
    // Then: the status line says so in the error colour, and the page stays
    let buffer = draw(&mut viewer, 60, 7);
    assert_eq!(body(&buffer)[0], " line 25 needle");
    let (x, y) = find(&buffer, "Pattern not found: absent");
    assert_eq!(buffer[(x, y)].fg, theme::LOVE);

    // When: the reader types /line 3 and presses Escape
    type_keys(&mut viewer, "/line 3");
    // Then: the prompt closes, the title is back, and the page stays
    assert_eq!(press(&mut viewer, KeyCode::Esc), Flow::Continue);
    let buffer = draw(&mut viewer, 30, 7);
    assert!(status(&buffer).starts_with(" doc.md"));
    assert_eq!(body(&buffer)[0], " line 25 needle");
}

/// The search reads the rendered text, ignores case unless the pattern has a capital, and marks
/// every match, again after a reload.
///
/// Why it matters: a reader searches for the words they see, not for the Markdown around them, and
/// a highlight left on old positions after a save points at the wrong text.
#[test]
fn searches_the_rendered_text_with_smartcase_and_marks_every_match() {
    // Given: a line with the word twice, once inside bold markup
    let markdown = "Some **Bold** text and bold again\n";
    let mut viewer = Viewer::new("doc.md", markdown);
    draw(&mut viewer, 40, 5);

    // When: the reader searches for the word in lowercase
    type_keys(&mut viewer, "/bold");
    press(&mut viewer, KeyCode::Enter);
    // Then: both are marked, the first as the current match
    let buffer = draw(&mut viewer, 40, 5);
    assert_eq!(body(&buffer)[0], " Some Bold text and bold again");
    assert_eq!(buffer[(6, 0)].bg, theme::ROSE);
    assert_eq!(buffer[(20, 0)].bg, theme::GOLD);

    // When: the reader searches with a capital letter
    type_keys(&mut viewer, "/Bold");
    press(&mut viewer, KeyCode::Enter);
    // Then: only the capitalised word matches
    let buffer = draw(&mut viewer, 40, 5);
    assert_eq!(buffer[(6, 0)].bg, theme::ROSE);
    assert_eq!(buffer[(20, 0)].bg, theme::BASE);

    // When: the file is saved with a new first line whose word sits where the current match was
    viewer.reload(&format!("Some Bold here\n\n{markdown}"));
    // Then: the marks move with the text, and none takes the current colour from the old place
    let buffer = draw(&mut viewer, 40, 5);
    assert_eq!(buffer[(6, 0)].bg, theme::GOLD);
    assert_eq!(buffer[(6, 2)].bg, theme::GOLD);
}

/// Inline math shows as Unicode text in the math colour, inside the sentence around it.
///
/// Why it matters: LaTeX source in a sentence reads as noise, and a reader of a formula page needs
/// the symbols, not the commands that name them.
#[test]
fn renders_inline_math_as_unicode_text() {
    // Given: a sentence with two inline formulas
    let markdown = "Energy $E = mc^2$ and $\\alpha_i$ here.\n";

    // When: it is drawn
    let buffer = draw(&mut Viewer::new("doc.md", markdown), 40, 4);

    // Then: the formulas read as symbols, in the math colour, and the words stay plain
    assert_eq!(body(&buffer)[0], " Energy E = mc² and αᵢ here.");
    assert_eq!(buffer[find(&buffer, "mc²")].fg, theme::IRIS);
    assert_eq!(buffer[find(&buffer, "here")].fg, theme::TEXT);
}

/// Display math sits centred on its own lines, also when it is written inside a paragraph.
///
/// Why it matters: a display formula is set apart on a rendered page, and run into the text it
/// reads as part of the sentence.
#[test]
fn centres_display_math_on_its_own_lines() {
    // Given: a display formula as its own paragraph, and one inside a sentence
    let markdown = "Before\n\n$$\\sqrt{\\sum_{i=1}^n x_i^2}$$\n\nMix $$x^2$$ end\n";

    // When: it is drawn with 28 columns for text
    let screen = shown(markdown, 30, 11);

    // Then: each formula is centred in the 28 columns, with a blank line around it
    assert_eq!(
        screen,
        [
            " Before",
            "",
            "         √(∑ᵢ₌₁ⁿ xᵢ²)",
            "",
            " Mix",
            "",
            "              x²",
            "",
            " end",
        ]
    );
}

/// A command the converter does not know stays as source, and dollar amounts stay as text.
///
/// Why it matters: a formula that loses an unknown command shows a wrong equation without a sign,
/// and prices read as math would garble a plain sentence.
#[test]
fn keeps_unknown_commands_and_leaves_dollar_amounts_alone() {
    // Given: a formula with a made-up command, and a sentence with two prices
    let markdown = "Odd $\\nosuchcommand + 1$ here.\n\nIt costs $5 and $10 today.\n\nRe-reads cost $3.93 in all ($0.31 each).\n";

    // When: it is drawn
    let screen = shown(markdown, 50, 8);

    // Then: the command shows as written, and the prices read as plain text, also where a dollar
    // sign before a digit could close a formula, which VS Code's preview does not allow either
    assert!(screen[0].contains("\\nosuchcommand"));
    assert_eq!(screen[2], " It costs $5 and $10 today.");
    assert_eq!(screen[4], " Re-reads cost $3.93 in all ($0.31 each).");
}

/// Dollar ratings in a table and display math in a heading stay inside their block.
///
/// Why it matters: pages rate cost as $$$$ in table cells, and reading that as an empty formula
/// breaks the table apart and scatters its cells as loose text.
#[test]
fn keeps_dollar_ratings_in_tables_and_math_in_headings() {
    // Given: a table with dollar ratings, and a heading with a display formula
    let markdown = "| Tier | Cost |\n|---|---|\n| Pilot | $$ |\n| Active | $$$$ |\n| Eq | $$y_1$$ |\n\n## Fit $$x^2$$ here\n";

    // When: it is drawn
    let screen = shown(markdown, 40, 11);

    // Then: the table keeps every row with its ratings, and the heading stays one line
    assert_eq!(
        screen[..8],
        [
            " ┌────────┬──────┐",
            " │ Tier   │ Cost │",
            " ├────────┼──────┤",
            " │ Pilot  │ $$   │",
            " │ Active │ $$$$ │",
            " │ Eq     │ y₁   │",
            " └────────┴──────┘",
            "",
        ]
    );
    assert_eq!(screen[8], " Fit x² here");
}

/// Front matter shows as a muted table of keys and values above the page.
///
/// Why it matters: raw front matter reads as a heading and a paragraph of YAML, which puts the
/// metadata in front of the text as if it were the first section.
#[test]
fn shows_front_matter_as_keys_and_values() {
    // Given: front matter with a long value and a list, then a heading
    let markdown = "---\ntitle: Notes\ndescription: A long description that wraps past the width\ntags: [rust, tui]\n---\n\n# Body\n";

    // When: it is drawn with 38 columns for text
    let buffer = draw(&mut Viewer::new("doc.md", markdown), 40, 11);

    // Then: keys and values line up in two columns, the long value wraps under itself, and the
    // page follows after a blank line
    assert_eq!(
        body(&buffer)[..6],
        [
            " title        Notes",
            " description  A long description that",
            "              wraps past the width",
            " tags         rust, tui",
            "",
            " Body",
        ]
    );
    assert_eq!(buffer[find(&buffer, "title")].fg, theme::MUTED);
    assert_eq!(buffer[find(&buffer, "Notes")].fg, theme::SUBTLE);
}

/// A list written as indented items joins with commas, and a quoted value loses its quotes.
///
/// Why it matters: both are common YAML spellings of the same data, and showing their syntax makes
/// one page's metadata read differently from another's.
#[test]
fn joins_block_lists_and_unquotes_values_in_front_matter() {
    // Given: front matter with an indented list and a quoted value holding a colon
    let markdown = "---\nauthors:\n  - Ada\n  - Linus\nsummary: \"Quoted: value\"\n---\nText\n";

    // When: it is drawn
    let screen = shown(markdown, 40, 7);

    // Then: the list reads as one value, and the quotes are gone
    assert_eq!(
        screen[..4],
        [
            " authors  Ada, Linus",
            " summary  Quoted: value",
            "",
            " Text"
        ]
    );
}

/// Front matter this viewer cannot read as keys and values shows as its source.
///
/// Why it matters: a nested map flattened into a table would show values under the wrong keys,
/// while the source is at least what the author wrote.
#[test]
fn shows_front_matter_it_cannot_read_as_source() {
    // Given: front matter with a nested map
    let markdown = "---\nnested:\n  key: value\n---\nText\n";

    // When: it is drawn
    let buffer = draw(&mut Viewer::new("doc.md", markdown), 40, 9);

    // Then: the block shows as written, in the muted source colour
    assert_eq!(
        body(&buffer)[..6],
        [" ---", " nested:", "   key: value", " ---", "", " Text"]
    );
    assert_eq!(buffer[find(&buffer, "nested")].fg, theme::MUTED);
}

/// A details block shows only its summary until it is opened with the vim fold keys.
///
/// Why it matters: a details block hides material the author chose to fold away, and showing its
/// raw HTML or its whole content buries the page the way the author meant it to read.
#[test]
fn folds_details_blocks_and_opens_them_with_the_fold_keys() {
    // Given: a details block with inline HTML and an entity in its summary
    let markdown = "Intro\n\n<details>\n<summary>More <b>info</b> &amp; notes</summary>\n\nHidden text\n\n> quoted\n\n</details>\n\nAfter\n";
    let mut viewer = Viewer::new("doc.md", markdown);
    let screen = |viewer: &mut Viewer| body(&draw(viewer, 40, 11));

    // Then: the block is closed, showing its summary as plain text behind a marker
    assert_eq!(
        screen(&mut viewer)[..5],
        [" Intro", "", " ▸ More info & notes", "", " After"]
    );

    // When: the reader types za
    type_keys(&mut viewer, "za");
    // Then: the fold opens and its content renders as Markdown
    assert_eq!(
        screen(&mut viewer)[..9],
        [
            " Intro",
            "",
            " ▾ More info & notes",
            "",
            " Hidden text",
            "",
            " ▎ quoted",
            "",
            " After"
        ]
    );

    // When: the reader types zM, then zR
    type_keys(&mut viewer, "zM");
    assert_eq!(
        screen(&mut viewer)[2..5],
        [" ▸ More info & notes", "", " After"]
    );
    type_keys(&mut viewer, "zR");
    // Then: every fold closes, then every fold opens
    assert_eq!(screen(&mut viewer)[4], " Hidden text");
}

/// A details block marked open starts open, and a fold the reader toggled keeps its state when
/// the file is saved.
///
/// Why it matters: a fold that snaps back on every save makes the reader open it again after each
/// edit in the editor beside the preview.
#[test]
fn starts_open_folds_open_and_keeps_fold_state_across_a_reload() {
    // Given: a closed fold, one marked open, and a third with the first one's summary
    let markdown = "<details>\n<summary>First</summary>\n\none\n\n</details>\n\n<details open>\n<summary>Second</summary>\n\ntwo\n\n</details>\n\n<details>\n<summary>First</summary>\n\nthree\n\n</details>\n";
    let mut viewer = Viewer::new("doc.md", markdown);
    let screen = |viewer: &mut Viewer| body(&draw(viewer, 40, 13));

    // Then: the first is closed and the second open
    assert_eq!(
        screen(&mut viewer)[..5],
        [" ▸ First", "", " ▾ Second", "", " two"]
    );

    // When: the reader opens the first, and the file is saved with a line added at the end
    type_keys(&mut viewer, "za");
    viewer.reload(&format!("{markdown}\nadded\n"));
    // Then: both folds keep their state, and the third, which shares the first one's summary,
    // stays closed
    assert_eq!(
        screen(&mut viewer)[..9],
        [
            " ▾ First",
            "",
            " one",
            "",
            " ▾ Second",
            "",
            " two",
            "",
            " ▸ First"
        ]
    );
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
    draw(&mut viewer, 60, 5);
    press(&mut viewer, KeyCode::Char('j'));
    press(&mut viewer, KeyCode::Char('j'));
    assert_eq!(
        body(&draw(&mut viewer, 60, 5))[0],
        " second paragraph that wraps when narrow"
    );

    // When: the screen narrows so the paragraph wraps
    let screen = body(&draw(&mut viewer, 22, 5));

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
    let markdown = "<div align=\"center\">\n<span>More</span>\n</div>\n\nkeep <kbd>K</kbd><!-- note --> this\n\n<!--\nhidden\n-->\n\nend\n";

    // When: it is drawn
    let buffer = draw(&mut Viewer::new("doc.md", markdown), 40, 10);

    // Then: the HTML shows in the muted colour, and neither comment appears
    assert_eq!(
        body(&buffer),
        [
            " <div align=\"center\">",
            " <span>More</span>",
            " </div>",
            "",
            " keep <kbd>K</kbd> this",
            "",
            " end",
            "",
        ]
    );
    assert_eq!(buffer[find(&buffer, "<span>")].fg, theme::MUTED);
    assert_eq!(buffer[find(&buffer, "<kbd>")].fg, theme::MUTED);
}

const SECTIONS: &str = "intro\n\n## Notes\n\nfirst section\n\n## Notes\n\nline one\n\nline two\n\nline three\n\nline four\n";

/// Scrolls `SECTIONS` so that "line two", five lines under the second Notes heading, is at the top.
fn reading_line_two() -> Viewer {
    let mut viewer = Viewer::new("doc.md", SECTIONS);
    draw(&mut viewer, 30, 5);
    for _ in 0..12 {
        press(&mut viewer, KeyCode::Char('j'));
    }
    assert_eq!(body(&draw(&mut viewer, 30, 5))[0], " line two");
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
        body(&draw(&mut viewer, 30, 5)),
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
        body(&draw(&mut viewer, 30, 5)),
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
    let buffer = draw(&mut viewer, 60, 5);
    assert!(status(&buffer).contains("No such file or directory"));
    assert_eq!(body(&buffer)[0], " line two");

    // When: the next reload succeeds
    viewer.reload(SECTIONS);

    // Then: the error is gone
    assert!(!status(&draw(&mut viewer, 60, 5)).contains("No such file"));
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
    let buffer = draw(&mut Viewer::new("doc.md", markdown), 42, 13);
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
    let buffer = draw(&mut Viewer::new("doc.md", markdown), 42, 11);

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
    let buffer = draw(&mut Viewer::new("doc.md", markdown), 42, 9);

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
    let screen = shown(markdown, 22, 13);

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
    let screen = shown(markdown, 30, 5);

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
    let buffer = draw(&mut Viewer::new("doc.md", markdown), 30, 7);

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
    let screen = shown(markdown, 30, 12);

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
    draw(&mut viewer, 60, 5);
    for _ in 0..4 {
        press(&mut viewer, KeyCode::Char('j'));
    }
    assert_eq!(body(&draw(&mut viewer, 60, 5))[0], " end paragraph");

    // When: the screen narrows
    let screen = body(&draw(&mut viewer, 40, 5));

    // Then: the same paragraph is still at the top
    assert_eq!(screen[0], " end paragraph");
}
