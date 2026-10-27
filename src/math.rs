use std::sync::LazyLock;

use techxt::Converter;
use techxt::convert::UnknownMacroPolicy;
use techxt::mathfmt::{FontStyle, FontStyleKind};
use unicode_width::UnicodeWidthStr;

/// Upright letters, because the italic math alphabet is missing from most terminal fonts and
/// falls back to another face. An unknown command stays as written, so a formula never loses a
/// term in silence.
static CONVERTER: LazyLock<Option<Converter>> = LazyLock::new(|| {
    Converter::builder()
        .wrap_width(None)
        .math_font(FontStyle::Style(FontStyleKind::Upright))
        .unknown_macro(UnknownMacroPolicy::KeepSource)
        .build()
        .ok()
});

/// The LaTeX of a formula as Unicode text, or the LaTeX itself when it cannot be converted.
pub fn to_unicode(latex: &str) -> String {
    CONVERTER
        .as_ref()
        .and_then(|converter| converter.latex_to_text(&format!("${latex}$")).ok())
        .map_or_else(
            || latex.to_string(),
            |conversion| conversion.text.trim().to_string(),
        )
}

/// The rows of a display formula. A matrix or cases environment spans several rows, with its
/// columns aligned, brackets the height of its rows, and the text beside it on its middle row. A
/// formula without one, one that does not parse, or one wider than `width` is a single row.
pub fn display_rows(latex: &str, width: usize) -> Vec<String> {
    let one_row = || vec![to_unicode(latex)];
    let Some(parts) = parts(latex) else {
        return one_row();
    };
    if parts.iter().all(|part| matches!(part, Part::Text(_))) {
        return one_row();
    }
    let boxes: Vec<Vec<String>> = parts
        .iter()
        .map(|part| match part {
            Part::Text(text) => vec![to_unicode(text)],
            Part::Grid(kind, body) => grid(*kind, body),
        })
        .collect();
    let rows = beside(&boxes);
    if rows.iter().any(|row| row.width() > width) {
        return one_row();
    }
    rows
}

/// The environments drawn as a grid, and the bracket each draws on its left and right.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Grid {
    Matrix,
    Brackets,
    Parentheses,
    Bars,
    Cases,
}

const GRIDS: [(&str, Grid); 5] = [
    ("matrix", Grid::Matrix),
    ("bmatrix", Grid::Brackets),
    ("pmatrix", Grid::Parentheses),
    ("vmatrix", Grid::Bars),
    ("cases", Grid::Cases),
];

/// The glyphs of one side of a bracket: one row tall, then the top, middle and bottom of a taller
/// one, and the glyph at the centre row of a brace.
struct Side {
    single: &'static str,
    top: &'static str,
    middle: &'static str,
    bottom: &'static str,
    centre: &'static str,
}

const fn side(
    single: &'static str,
    top: &'static str,
    middle: &'static str,
    bottom: &'static str,
) -> Side {
    Side {
        single,
        top,
        middle,
        bottom,
        centre: middle,
    }
}

impl Grid {
    fn sides(self) -> (Option<Side>, Option<Side>) {
        match self {
            Self::Matrix => (None, None),
            Self::Brackets => (
                Some(side("[", "⎡", "⎢", "⎣")),
                Some(side("]", "⎤", "⎥", "⎦")),
            ),
            Self::Parentheses => (
                Some(side("(", "⎛", "⎜", "⎝")),
                Some(side(")", "⎞", "⎟", "⎠")),
            ),
            Self::Bars => (
                Some(side("|", "│", "│", "│")),
                Some(side("|", "│", "│", "│")),
            ),
            Self::Cases => (
                Some(Side {
                    centre: "⎨",
                    ..side("{", "⎧", "⎪", "⎩")
                }),
                None,
            ),
        }
    }
}

enum Part<'a> {
    Text(&'a str),
    Grid(Grid, &'a str),
}

/// The formula cut into the text between grid environments and the body of each, or none when an
/// environment does not close.
fn parts(latex: &str) -> Option<Vec<Part<'_>>> {
    let mut parts = Vec::new();
    let mut rest = latex;
    while let Some((at, name, kind)) = GRIDS
        .iter()
        .filter_map(|(name, kind)| {
            rest.find(&format!("\\begin{{{name}}}"))
                .map(|at| (at, *name, *kind))
        })
        .min_by_key(|(at, _, _)| *at)
    {
        let begin = format!("\\begin{{{name}}}");
        let end = format!("\\end{{{name}}}");
        if at > 0 {
            parts.push(Part::Text(&rest[..at]));
        }
        let body_start = at + begin.len();
        let body_len = closing(&rest[body_start..], &begin, &end)?;
        parts.push(Part::Grid(kind, &rest[body_start..body_start + body_len]));
        rest = &rest[body_start + body_len + end.len()..];
    }
    if !rest.trim().is_empty() {
        parts.push(Part::Text(rest));
    }
    Some(parts)
}

/// Where the `end` that closes an environment starts in `body`, past any nested `begin` of the
/// same name.
fn closing(body: &str, begin: &str, end: &str) -> Option<usize> {
    let mut depth = 0_usize;
    let mut at = 0;
    while at < body.len() {
        let rest = &body[at..];
        if rest.starts_with(end) {
            if depth == 0 {
                return Some(at);
            }
            depth -= 1;
            at += end.len();
        } else if rest.starts_with(begin) {
            depth += 1;
            at += begin.len();
        } else {
            at += rest.chars().next().map_or(1, char::len_utf8);
        }
    }
    None
}

/// The cells of an environment body, row by row: rows end at `\\` and cells at `&`, but only
/// outside braces and nested environments.
fn cells(body: &str) -> Vec<Vec<&str>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut depth = 0_usize;
    let mut start = 0;
    let mut at = 0;
    while at < body.len() {
        let rest = &body[at..];
        if rest.starts_with("\\begin{") {
            depth += 1;
            at += "\\begin{".len();
        } else if rest.starts_with("\\end{") {
            depth = depth.saturating_sub(1);
            at += "\\end{".len();
        } else if rest.starts_with("\\\\") && depth == 0 {
            row.push(&body[start..at]);
            rows.push(std::mem::take(&mut row));
            at += 2;
            start = at;
        } else if let Some(escaped) = rest.strip_prefix('\\') {
            at += 1 + escaped.chars().next().map_or(0, char::len_utf8);
        } else if rest.starts_with('{') {
            depth += 1;
            at += 1;
        } else if rest.starts_with('}') {
            depth = depth.saturating_sub(1);
            at += 1;
        } else if rest.starts_with('&') && depth == 0 {
            row.push(&body[start..at]);
            at += 1;
            start = at;
        } else {
            at += rest.chars().next().map_or(1, char::len_utf8);
        }
    }
    row.push(&body[start..]);
    rows.push(row);
    if rows
        .last()
        .is_some_and(|last| last.iter().all(|cell| cell.trim().is_empty()))
    {
        rows.pop();
    }
    rows
}

/// Gap between two columns of a grid.
const COLUMN_GAP: &str = "  ";

/// The rows of a grid environment: each cell as Unicode text, matrix cells centred and cases
/// cells from the left, between brackets as tall as the rows.
fn grid(kind: Grid, body: &str) -> Vec<String> {
    let table: Vec<Vec<String>> = cells(body)
        .iter()
        .map(|row| {
            row.iter()
                .map(|cell| match cell.trim() {
                    "" => String::new(),
                    cell => to_unicode(cell),
                })
                .collect()
        })
        .collect();
    let columns = table.iter().map(Vec::len).max().unwrap_or(0);
    let widths: Vec<usize> = (0..columns)
        .map(|column| {
            table
                .iter()
                .filter_map(|row| row.get(column))
                .map(|cell| cell.width())
                .max()
                .unwrap_or(0)
        })
        .collect();
    let lines: Vec<String> = table
        .iter()
        .map(|row| {
            widths
                .iter()
                .enumerate()
                .map(|(column, &wide)| {
                    let cell = row.get(column).map_or("", String::as_str);
                    let spare = wide - cell.width();
                    let before = if kind == Grid::Cases { 0 } else { spare / 2 };
                    format!("{}{cell}{}", " ".repeat(before), " ".repeat(spare - before))
                })
                .collect::<Vec<_>>()
                .join(COLUMN_GAP)
        })
        .collect();
    let height = lines.len();
    let (left, right) = kind.sides();
    let glyph = |side: &Side, row: usize| {
        if height == 1 {
            side.single
        } else if row == 0 {
            side.top
        } else if row == height - 1 {
            side.bottom
        } else if row == (height - 1) / 2 {
            side.centre
        } else {
            side.middle
        }
    };
    lines
        .iter()
        .enumerate()
        .map(|(row, line)| {
            let mut text = String::new();
            if let Some(left) = &left {
                text.push_str(glyph(left, row));
                text.push(' ');
            }
            text.push_str(line);
            if let Some(right) = &right {
                text.push(' ');
                text.push_str(glyph(right, row));
            }
            text
        })
        .collect()
}

/// Sets the boxes side by side, each centred on the tallest, with the rows padded to one width.
fn beside(boxes: &[Vec<String>]) -> Vec<String> {
    let height = boxes.iter().map(Vec::len).max().unwrap_or(0);
    let mut rows = vec![String::new(); height];
    for (index, block) in boxes.iter().enumerate() {
        let wide = block.iter().map(|row| row.width()).max().unwrap_or(0);
        let offset = (height - block.len()) / 2;
        let gap = if index == 0 { "" } else { " " };
        for (row, text) in rows.iter_mut().enumerate() {
            let line = row
                .checked_sub(offset)
                .and_then(|inside| block.get(inside))
                .map_or("", String::as_str);
            text.push_str(gap);
            text.push_str(line);
            text.push_str(&" ".repeat(wide - line.width()));
        }
    }
    rows.into_iter()
        .map(|row| row.trim_end().to_string())
        .collect()
}
