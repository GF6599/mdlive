//! The reading palettes. Rose Pine (main) takes its roles the way `doc.css` maps it for a page in
//! the Glance reading idiom, and each other theme fills those roles from its own palette.

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

/// Catppuccin Mocha, from `palette.json` of catppuccin/palette, placed by its style guide: links
/// are blue, subtle text is Overlay 1, and secondary panes are Mantle. Rose takes Flamingo, the
/// nearest pink, and the keywords take Sapphire, because Mauve already holds Iris.
pub const CATPPUCCIN_MOCHA: Palette = Palette {
    name: "catppuccin-mocha",
    base: Color::Rgb(0x1e, 0x1e, 0x2e),
    surface: Color::Rgb(0x18, 0x18, 0x25),
    overlay: Color::Rgb(0x31, 0x32, 0x44),
    muted: Color::Rgb(0x7f, 0x84, 0x9c),
    subtle: Color::Rgb(0xa6, 0xad, 0xc8),
    text: Color::Rgb(0xcd, 0xd6, 0xf4),
    love: Color::Rgb(0xf3, 0x8b, 0xa8),
    gold: Color::Rgb(0xf9, 0xe2, 0xaf),
    rose: Color::Rgb(0xf2, 0xcd, 0xcd),
    pine: Color::Rgb(0x74, 0xc7, 0xec),
    foam: Color::Rgb(0x89, 0xb4, 0xfa),
    iris: Color::Rgb(0xcb, 0xa6, 0xf7),
    leaf: Color::Rgb(0xa6, 0xe3, 0xa1),
    highlight_med: Color::Rgb(0x45, 0x47, 0x5a),
    highlight_high: Color::Rgb(0x58, 0x5b, 0x70),
};

/// Tokyo Night, from the night colours of folke/tokyonight.nvim. Secondary panes take `bg_dark`
/// as the side bars there do. The theme has no pink, so Rose takes the warm accent, orange.
pub const TOKYO_NIGHT: Palette = Palette {
    name: "tokyo-night",
    base: Color::Rgb(0x1a, 0x1b, 0x26),
    surface: Color::Rgb(0x16, 0x16, 0x1e),
    overlay: Color::Rgb(0x29, 0x2e, 0x42),
    muted: Color::Rgb(0x56, 0x5f, 0x89),
    subtle: Color::Rgb(0xa9, 0xb1, 0xd6),
    text: Color::Rgb(0xc0, 0xca, 0xf5),
    love: Color::Rgb(0xf7, 0x76, 0x8e),
    gold: Color::Rgb(0xe0, 0xaf, 0x68),
    rose: Color::Rgb(0xff, 0x9e, 0x64),
    pine: Color::Rgb(0x7a, 0xa2, 0xf7),
    foam: Color::Rgb(0x7d, 0xcf, 0xff),
    iris: Color::Rgb(0xbb, 0x9a, 0xf7),
    leaf: Color::Rgb(0x9e, 0xce, 0x6a),
    highlight_med: Color::Rgb(0x3b, 0x42, 0x61),
    highlight_high: Color::Rgb(0x41, 0x48, 0x68),
};

/// Gruvbox Dark, from the bright colours of morhetz/gruvbox, whose panes are lighter than the
/// page. Links are blue there, so Foam takes blue and the keywords take aqua. Rose takes orange.
pub const GRUVBOX_DARK: Palette = Palette {
    name: "gruvbox-dark",
    base: Color::Rgb(0x28, 0x28, 0x28),
    surface: Color::Rgb(0x3c, 0x38, 0x36),
    overlay: Color::Rgb(0x50, 0x49, 0x45),
    muted: Color::Rgb(0x92, 0x83, 0x74),
    subtle: Color::Rgb(0xa8, 0x99, 0x84),
    text: Color::Rgb(0xeb, 0xdb, 0xb2),
    love: Color::Rgb(0xfb, 0x49, 0x34),
    gold: Color::Rgb(0xfa, 0xbd, 0x2f),
    rose: Color::Rgb(0xfe, 0x80, 0x19),
    pine: Color::Rgb(0x8e, 0xc0, 0x7c),
    foam: Color::Rgb(0x83, 0xa5, 0x98),
    iris: Color::Rgb(0xd3, 0x86, 0x9b),
    leaf: Color::Rgb(0xb8, 0xbb, 0x26),
    highlight_med: Color::Rgb(0x50, 0x49, 0x45),
    highlight_high: Color::Rgb(0x66, 0x5c, 0x54),
};

/// Nord, from nordtheme/vim, placed by the Nord colour guide: nord4 is the main text, nord5 the
/// subtle text, nord1 the elevated panels, and the brightened nord3 the comments. Nord keeps
/// nord9 for keywords and nord8 for links, and Rose takes nord12, the warm accent.
pub const NORD: Palette = Palette {
    name: "nord",
    base: Color::Rgb(0x2e, 0x34, 0x40),
    surface: Color::Rgb(0x3b, 0x42, 0x52),
    overlay: Color::Rgb(0x43, 0x4c, 0x5e),
    muted: Color::Rgb(0x61, 0x6e, 0x88),
    subtle: Color::Rgb(0xe5, 0xe9, 0xf0),
    text: Color::Rgb(0xd8, 0xde, 0xe9),
    love: Color::Rgb(0xbf, 0x61, 0x6a),
    gold: Color::Rgb(0xeb, 0xcb, 0x8b),
    rose: Color::Rgb(0xd0, 0x87, 0x70),
    pine: Color::Rgb(0x81, 0xa1, 0xc1),
    foam: Color::Rgb(0x88, 0xc0, 0xd0),
    iris: Color::Rgb(0xb4, 0x8e, 0xad),
    leaf: Color::Rgb(0xa3, 0xbe, 0x8c),
    highlight_med: Color::Rgb(0x43, 0x4c, 0x5e),
    highlight_high: Color::Rgb(0x4c, 0x56, 0x6a),
};

/// Every theme a config file can name.
pub const THEMES: &[&Palette] = &[
    &ROSE_PINE,
    &CATPPUCCIN_MOCHA,
    &TOKYO_NIGHT,
    &GRUVBOX_DARK,
    &NORD,
];

pub fn named(name: &str) -> Option<&'static Palette> {
    THEMES.iter().copied().find(|palette| palette.name == name)
}

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
