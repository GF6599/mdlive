use std::str::FromStr;
use std::sync::LazyLock;

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use syntect::easy::HighlightLines;
use syntect::highlighting::{
    self as syntect_style, FontStyle, ScopeSelectors, StyleModifier, Theme, ThemeItem,
    ThemeSettings,
};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

use crate::theme;

/// bat's grammars, which add TypeScript, TOML and others to syntect's own set. Loading them takes
/// about 100 ms in a release build, so it happens once for the life of the process, not per reload.
static SYNTAXES: LazyLock<SyntaxSet> = LazyLock::new(two_face::syntax::extra_newlines);
static ROSE_PINE: LazyLock<Theme> = LazyLock::new(rose_pine);

/// Rose Pine's own mapping of token kinds to colours.
const TOKENS: &[(&str, Color, FontStyle)] = &[
    (
        "comment, punctuation.definition.comment",
        theme::MUTED,
        FontStyle::ITALIC,
    ),
    (
        "string, constant.character, punctuation.definition.string",
        theme::GOLD,
        FontStyle::empty(),
    ),
    (
        "constant.numeric, constant.language",
        theme::GOLD,
        FontStyle::empty(),
    ),
    (
        "keyword, storage, keyword.operator.word",
        theme::PINE,
        FontStyle::empty(),
    ),
    (
        "keyword.operator, punctuation",
        theme::SUBTLE,
        FontStyle::empty(),
    ),
    (
        "entity.name.function, support.function, meta.function-call",
        theme::ROSE,
        FontStyle::empty(),
    ),
    (
        "entity.name.type, entity.name.class, support.type, support.class",
        theme::FOAM,
        FontStyle::empty(),
    ),
    ("entity.name.tag", theme::FOAM, FontStyle::empty()),
    (
        "entity.other.attribute-name",
        theme::IRIS,
        FontStyle::empty(),
    ),
    (
        "variable.language, support.constant",
        theme::LOVE,
        FontStyle::empty(),
    ),
];

/// One styled line per source line, or `None` when no grammar knows `language`.
pub fn highlight(code: &str, language: &str) -> Option<Vec<Vec<Span<'static>>>> {
    if language.is_empty() {
        return None;
    }
    let syntax = SYNTAXES.find_syntax_by_token(language)?;
    let mut highlighter = HighlightLines::new(syntax, &ROSE_PINE);
    let mut lines = Vec::new();
    for line in LinesWithEndings::from(code) {
        let ranges = highlighter.highlight_line(line, &SYNTAXES).ok()?;
        let spans = ranges
            .into_iter()
            .map(|(style, text)| {
                Span::styled(text.trim_end_matches('\n').to_string(), convert(style))
            })
            .filter(|span| !span.content.is_empty())
            .collect();
        lines.push(spans);
    }
    Some(lines)
}

fn convert(style: syntect_style::Style) -> Style {
    let mut converted = Style::new().fg(Color::Rgb(
        style.foreground.r,
        style.foreground.g,
        style.foreground.b,
    ));
    if style.font_style.contains(FontStyle::ITALIC) {
        converted = converted.add_modifier(Modifier::ITALIC);
    }
    if style.font_style.contains(FontStyle::BOLD) {
        converted = converted.add_modifier(Modifier::BOLD);
    }
    converted
}

fn syntect_color(color: Color) -> syntect_style::Color {
    match color {
        Color::Rgb(r, g, b) => syntect_style::Color { r, g, b, a: 0xff },
        _ => syntect_style::Color::WHITE,
    }
}

fn rose_pine() -> Theme {
    let scopes = TOKENS
        .iter()
        .filter_map(|&(selector, color, font_style)| {
            Some(ThemeItem {
                scope: ScopeSelectors::from_str(selector).ok()?,
                style: StyleModifier {
                    foreground: Some(syntect_color(color)),
                    background: None,
                    font_style: Some(font_style),
                },
            })
        })
        .collect();
    Theme {
        name: Some("Rose Pine".to_string()),
        author: None,
        settings: ThemeSettings {
            foreground: Some(syntect_color(theme::TEXT)),
            ..ThemeSettings::default()
        },
        scopes,
    }
}
