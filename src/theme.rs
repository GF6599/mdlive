//! Rose Pine (main) in the Glance reading idiom, mapped the way `doc.css` maps it for a page.

use pulldown_cmark::BlockQuoteKind;
use ratatui::style::{Color, Modifier, Style};

pub const BASE: Color = Color::Rgb(0x19, 0x17, 0x24);
pub const SURFACE: Color = Color::Rgb(0x1f, 0x1d, 0x2e);
pub const OVERLAY: Color = Color::Rgb(0x26, 0x23, 0x3a);
pub const MUTED: Color = Color::Rgb(0x6e, 0x6a, 0x86);
pub const SUBTLE: Color = Color::Rgb(0x90, 0x8c, 0xaa);
pub const TEXT: Color = Color::Rgb(0xe0, 0xde, 0xf4);
pub const LOVE: Color = Color::Rgb(0xeb, 0x6f, 0x92);
pub const GOLD: Color = Color::Rgb(0xf6, 0xc1, 0x77);
pub const ROSE: Color = Color::Rgb(0xeb, 0xbc, 0xba);
pub const PINE: Color = Color::Rgb(0x31, 0x74, 0x8f);
pub const FOAM: Color = Color::Rgb(0x9c, 0xcf, 0xd8);
pub const IRIS: Color = Color::Rgb(0xc4, 0xa7, 0xe7);
pub const LEAF: Color = Color::Rgb(0x95, 0xb1, 0xac);
pub const HL_MED: Color = Color::Rgb(0x40, 0x3d, 0x52);

pub fn page() -> Style {
    Style::new().fg(TEXT).bg(BASE)
}

pub fn status() -> Style {
    Style::new().fg(SUBTLE).bg(SURFACE)
}

pub fn error() -> Style {
    Style::new().fg(LOVE)
}

pub fn heading(level: u8) -> Style {
    let bold = Style::new().add_modifier(Modifier::BOLD);
    match level {
        1 | 2 => bold.fg(TEXT),
        3 => bold.fg(FOAM),
        4 => bold.fg(SUBTLE),
        _ => Style::new().fg(SUBTLE).add_modifier(Modifier::ITALIC),
    }
}

pub fn heading_rule(level: u8) -> Option<(&'static str, Style)> {
    match level {
        1 => Some(("━", Style::new().fg(ROSE))),
        2 => Some(("─", Style::new().fg(HL_MED))),
        _ => None,
    }
}

pub fn rule() -> Style {
    Style::new().fg(HL_MED)
}

pub fn marker() -> Style {
    Style::new().fg(ROSE)
}

pub fn diagram() -> Style {
    Style::new().fg(SUBTLE)
}

/// A colour as `#rrggbb`, for renderers that take CSS colours.
pub fn hex(color: Color) -> String {
    match color {
        Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
        _ => String::from("#000000"),
    }
}

pub fn picture_note() -> Style {
    Style::new().fg(MUTED)
}

pub fn alert(kind: BlockQuoteKind) -> Style {
    Style::new().fg(match kind {
        BlockQuoteKind::Note => FOAM,
        BlockQuoteKind::Tip => LEAF,
        BlockQuoteKind::Important => IRIS,
        BlockQuoteKind::Warning => GOLD,
        BlockQuoteKind::Caution => LOVE,
    })
}

pub fn alert_label(kind: BlockQuoteKind) -> Style {
    alert(kind).add_modifier(Modifier::BOLD)
}

pub fn table_header() -> Style {
    Style::new().fg(SUBTLE).add_modifier(Modifier::BOLD)
}

pub fn quote() -> Style {
    Style::new().fg(SUBTLE)
}

pub fn strong() -> Style {
    Style::new().add_modifier(Modifier::BOLD)
}

pub fn emphasis() -> Style {
    Style::new().fg(SUBTLE).add_modifier(Modifier::ITALIC)
}

pub fn strikethrough() -> Style {
    Style::new().fg(MUTED).add_modifier(Modifier::CROSSED_OUT)
}

pub fn link() -> Style {
    Style::new().fg(FOAM)
}

pub fn math() -> Style {
    Style::new().fg(IRIS)
}

pub fn inline_code() -> Style {
    Style::new().fg(ROSE).bg(OVERLAY)
}

/// `doc.css` sets a code block apart with a border on a near-base fill. A terminal has no
/// border to draw, so the panel takes the lighter surface colour instead.
pub fn code_block() -> Style {
    Style::new().fg(TEXT).bg(SURFACE)
}

pub fn html() -> Style {
    Style::new().fg(MUTED)
}

pub fn panel_border() -> Style {
    Style::new().fg(HL_MED)
}

pub fn panel_title() -> Style {
    Style::new().fg(SUBTLE)
}

pub fn contents_entry(level: u8) -> Style {
    Style::new().fg(if level <= 2 { TEXT } else { SUBTLE })
}

pub fn contents_marked() -> Style {
    Style::new().fg(ROSE).add_modifier(Modifier::BOLD)
}

pub fn hint() -> Style {
    Style::new().fg(GOLD)
}

pub fn search_match() -> Style {
    Style::new().fg(BASE).bg(GOLD)
}

pub fn search_current() -> Style {
    Style::new().fg(BASE).bg(ROSE)
}

pub fn front_matter_key() -> Style {
    Style::new().fg(MUTED)
}

pub fn front_matter_value() -> Style {
    Style::new().fg(SUBTLE)
}
