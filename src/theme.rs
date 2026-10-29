//! Rose Pine (main) in the Glance reading idiom, mapped the way `doc.css` maps it for a page.

use pulldown_cmark::BlockQuoteKind;
use ratatui::style::{Color, Modifier, Style};

/// The colours of a theme, each named by the Rose Pine role it fills.
#[derive(Debug)]
pub struct Palette {
    pub name: &'static str,
    pub base: Color,
    pub surface: Color,
    pub overlay: Color,
    pub muted: Color,
    pub subtle: Color,
    pub text: Color,
    pub love: Color,
    pub gold: Color,
    pub rose: Color,
    pub pine: Color,
    pub foam: Color,
    pub iris: Color,
    pub leaf: Color,
    pub highlight_med: Color,
    pub highlight_high: Color,
}

pub const ROSE_PINE: Palette = Palette {
    name: "rose-pine",
    base: Color::Rgb(0x19, 0x17, 0x24),
    surface: Color::Rgb(0x1f, 0x1d, 0x2e),
    overlay: Color::Rgb(0x26, 0x23, 0x3a),
    muted: Color::Rgb(0x6e, 0x6a, 0x86),
    subtle: Color::Rgb(0x90, 0x8c, 0xaa),
    text: Color::Rgb(0xe0, 0xde, 0xf4),
    love: Color::Rgb(0xeb, 0x6f, 0x92),
    gold: Color::Rgb(0xf6, 0xc1, 0x77),
    rose: Color::Rgb(0xeb, 0xbc, 0xba),
    pine: Color::Rgb(0x31, 0x74, 0x8f),
    foam: Color::Rgb(0x9c, 0xcf, 0xd8),
    iris: Color::Rgb(0xc4, 0xa7, 0xe7),
    leaf: Color::Rgb(0x95, 0xb1, 0xac),
    highlight_med: Color::Rgb(0x40, 0x3d, 0x52),
    highlight_high: Color::Rgb(0x52, 0x4f, 0x67),
};

impl Palette {
    /// The page leaves its background to the terminal, so a terminal with opacity or a background
    /// picture shows through.
    pub fn page(&self) -> Style {
        Style::new().fg(self.text).bg(Color::Reset)
    }

    /// A popup floats over the text of the page, so it takes a fill to stand apart from it.
    pub fn popup(&self) -> Style {
        Style::new().fg(self.text).bg(self.base)
    }

    pub fn status(&self) -> Style {
        Style::new().fg(self.subtle).bg(self.surface)
    }

    pub fn error(&self) -> Style {
        Style::new().fg(self.love)
    }

    pub fn heading(&self, level: u8) -> Style {
        let bold = Style::new().add_modifier(Modifier::BOLD);
        match level {
            1 | 2 => bold.fg(self.text),
            3 => bold.fg(self.foam),
            4 => bold.fg(self.subtle),
            _ => Style::new().fg(self.subtle).add_modifier(Modifier::ITALIC),
        }
    }

    pub fn heading_rule(&self, level: u8) -> Option<(&'static str, Style)> {
        match level {
            1 => Some(("━", Style::new().fg(self.rose))),
            2 => Some(("─", Style::new().fg(self.highlight_med))),
            _ => None,
        }
    }

    pub fn rule(&self) -> Style {
        Style::new().fg(self.highlight_med)
    }

    pub fn marker(&self) -> Style {
        Style::new().fg(self.rose)
    }

    pub fn diagram(&self) -> Style {
        Style::new().fg(self.subtle)
    }

    pub fn picture_note(&self) -> Style {
        Style::new().fg(self.muted)
    }

    pub fn alert(&self, kind: BlockQuoteKind) -> Style {
        Style::new().fg(match kind {
            BlockQuoteKind::Note => self.foam,
            BlockQuoteKind::Tip => self.leaf,
            BlockQuoteKind::Important => self.iris,
            BlockQuoteKind::Warning => self.gold,
            BlockQuoteKind::Caution => self.love,
        })
    }

    pub fn alert_label(&self, kind: BlockQuoteKind) -> Style {
        self.alert(kind).add_modifier(Modifier::BOLD)
    }

    pub fn table_header(&self) -> Style {
        Style::new().fg(self.subtle).add_modifier(Modifier::BOLD)
    }

    pub fn quote(&self) -> Style {
        Style::new().fg(self.subtle)
    }

    pub fn strong(&self) -> Style {
        Style::new().add_modifier(Modifier::BOLD)
    }

    pub fn emphasis(&self) -> Style {
        Style::new().fg(self.subtle).add_modifier(Modifier::ITALIC)
    }

    pub fn strikethrough(&self) -> Style {
        Style::new()
            .fg(self.muted)
            .add_modifier(Modifier::CROSSED_OUT)
    }

    pub fn link(&self) -> Style {
        Style::new().fg(self.foam)
    }

    pub fn link_selected(&self) -> Style {
        Style::new().fg(self.base).bg(self.foam)
    }

    pub fn math(&self) -> Style {
        Style::new().fg(self.iris)
    }

    pub fn inline_code(&self) -> Style {
        Style::new().fg(self.rose).bg(self.overlay)
    }

    /// `doc.css` sets a code block apart with a border on a near-base fill. A terminal has no
    /// border to draw, so the panel takes the lighter surface colour instead.
    pub fn code_block(&self) -> Style {
        Style::new().fg(self.text).bg(self.surface)
    }

    pub fn html(&self) -> Style {
        Style::new().fg(self.muted)
    }

    pub fn panel_border(&self) -> Style {
        Style::new().fg(self.highlight_med)
    }

    pub fn panel_title(&self) -> Style {
        Style::new().fg(self.subtle)
    }

    pub fn contents_entry(&self, level: u8) -> Style {
        Style::new().fg(if level <= 2 { self.text } else { self.subtle })
    }

    pub fn contents_marked(&self) -> Style {
        Style::new().fg(self.rose).add_modifier(Modifier::BOLD)
    }

    pub fn hint(&self) -> Style {
        Style::new().fg(self.gold)
    }

    pub fn tree_folder(&self) -> Style {
        Style::new().fg(self.foam)
    }

    pub fn tree_cursor(&self) -> Style {
        Style::new().bg(self.highlight_med)
    }

    pub fn breadcrumb(&self) -> Style {
        Style::new().fg(self.subtle)
    }

    pub fn breadcrumb_separator(&self) -> Style {
        Style::new().fg(self.muted)
    }

    pub fn breadcrumb_current(&self) -> Style {
        Style::new().fg(self.text).add_modifier(Modifier::BOLD)
    }

    pub fn hint_label(&self) -> Style {
        Style::new().fg(self.muted)
    }

    pub fn selection(&self) -> Style {
        Style::new().bg(self.highlight_med)
    }

    pub fn selection_cursor(&self) -> Style {
        Style::new().bg(self.highlight_high)
    }

    pub fn search_match(&self) -> Style {
        Style::new().fg(self.base).bg(self.gold)
    }

    pub fn search_current(&self) -> Style {
        Style::new().fg(self.base).bg(self.rose)
    }

    pub fn front_matter_key(&self) -> Style {
        Style::new().fg(self.muted)
    }

    pub fn front_matter_value(&self) -> Style {
        Style::new().fg(self.subtle)
    }
}
