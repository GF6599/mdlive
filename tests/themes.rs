#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use mdlive::{Viewer, theme};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::{Buffer, Cell};
use ratatui::style::Color;

fn rgb(hex: u32) -> Color {
    let [_, r, g, b] = hex.to_be_bytes();
    Color::Rgb(r, g, b)
}

fn cell<'a>(buffer: &'a Buffer, text: &str) -> &'a Cell {
    let width = usize::from(buffer.area.width);
    let symbols: Vec<&str> = buffer.content().iter().map(Cell::symbol).collect();
    let at = (0..symbols.len())
        .find(|&at| symbols[at..].concat().starts_with(text) && at % width + text.len() <= width)
        .unwrap_or_else(|| panic!("{text:?} is not on screen"));
    &buffer.content()[at]
}

/// A page drawn in each theme puts the colours that theme publishes on the rule under a title, a
/// link, a keyword and the code panel.
///
/// Why it matters: a theme that a reader picks by name must look like that theme, and a colour
/// copied wrong is only caught against the published values.
#[test]
fn draws_a_page_in_the_published_colours_of_each_theme() {
    let markdown = "# Title\n\n[link](https://example.com)\n\n```rust\nfn main() {}\n```\n";
    // The rule, the link, the keyword and the code panel, from each theme's own palette file.
    for (name, rule, link, keyword, panel) in [
        ("rose-pine", 0xebbcba, 0x9ccfd8, 0x31748f, 0x1f1d2e),
        ("catppuccin-mocha", 0xf2cdcd, 0x89b4fa, 0x74c7ec, 0x181825),
        ("tokyo-night", 0xff9e64, 0x7dcfff, 0x7aa2f7, 0x16161e),
        ("gruvbox-dark", 0xfe8019, 0x83a598, 0x8ec07c, 0x3c3836),
        ("nord", 0xd08770, 0x88c0d0, 0x81a1c1, 0x3b4252),
    ] {
        // Given: the theme a config file names
        let palette = theme::named(name).unwrap_or_else(|| panic!("no theme {name}"));
        let mut viewer = Viewer::new("doc.md", markdown).with_palette(palette);

        // When: the page is drawn
        let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();
        terminal.draw(|frame| viewer.draw(frame)).unwrap();
        let buffer = terminal.backend().buffer();

        // Then: each part takes the colour the theme publishes for it
        assert_eq!(cell(buffer, "━").fg, rgb(rule), "{name} rule");
        assert_eq!(cell(buffer, "link").fg, rgb(link), "{name} link");
        assert_eq!(cell(buffer, "fn").fg, rgb(keyword), "{name} keyword");
        assert_eq!(cell(buffer, "fn").bg, rgb(panel), "{name} code panel");
    }
}
