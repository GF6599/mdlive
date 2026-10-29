use pulldown_cmark::Alignment;
use ratatui::style::Style;
use ratatui::text::Span;
use unicode_width::UnicodeWidthStr;

use crate::theme::Palette;
use crate::wrap::{Run, wrap_runs};

pub type Cell = Vec<Run>;

/// Lays out a table whose first row is its header, in a grid no wider than `width`.
pub fn lay_out(
    rows: &[Vec<Cell>],
    alignments: &[Alignment],
    width: usize,
    palette: &Palette,
) -> Vec<Cell> {
    let columns = rows
        .iter()
        .map(Vec::len)
        .chain([alignments.len()])
        .max()
        .unwrap_or(0);
    if columns == 0 {
        return Vec::new();
    }
    let widths = fit(
        &natural_widths(rows, columns),
        width.saturating_sub(1 + 3 * columns),
    );
    let wrapped: Vec<Vec<Vec<Cell>>> = rows
        .iter()
        .enumerate()
        .map(|(index, row)| {
            (0..columns)
                .map(|column| {
                    let cell = row.get(column).map_or(&[][..], Vec::as_slice);
                    let cell: Cell = if index == 0 {
                        cell.iter()
                            .map(|(span, link)| {
                                (span.clone().patch_style(palette.table_header()), *link)
                            })
                            .collect()
                    } else {
                        cell.to_vec()
                    };
                    wrap_runs(&cell, widths[column])
                })
                .collect()
        })
        .collect();
    let spans_rows = wrapped
        .iter()
        .skip(1)
        .any(|row| row.iter().any(|lines| lines.len() > 1));

    let mut lines = vec![rule(&widths, '┌', '┬', '┐', palette.rule())];
    for (index, row) in wrapped.iter().enumerate() {
        if index == 1 || (index > 1 && spans_rows) {
            lines.push(rule(&widths, '├', '┼', '┤', palette.rule()));
        }
        let height = row.iter().map(Vec::len).max().unwrap_or(0).max(1);
        for line in 0..height {
            lines.push(grid_row(row, line, &widths, alignments, palette.rule()));
        }
    }
    lines.push(rule(&widths, '└', '┴', '┘', palette.rule()));
    lines
}

fn natural_widths(rows: &[Vec<Cell>], columns: usize) -> Vec<usize> {
    (0..columns)
        .map(|column| {
            rows.iter()
                .filter_map(|row| row.get(column))
                .map(|cell| {
                    cell.iter()
                        .map(|(span, _)| span.content.width())
                        .sum::<usize>()
                })
                .max()
                .unwrap_or(0)
                .max(1)
        })
        .collect()
}

/// Shares `room` among the columns: a column narrower than an even share keeps its width, and
/// the wider columns split what remains.
fn fit(natural: &[usize], room: usize) -> Vec<usize> {
    if natural.iter().sum::<usize>() <= room {
        return natural.to_vec();
    }
    let mut order: Vec<usize> = (0..natural.len()).collect();
    order.sort_by_key(|&column| natural[column]);
    let mut widths = vec![1; natural.len()];
    let mut left = room;
    for (done, &column) in order.iter().enumerate() {
        let share = left / (natural.len() - done);
        widths[column] = natural[column].min(share).max(1);
        left = left.saturating_sub(widths[column]);
    }
    widths
}

fn rule(widths: &[usize], left: char, join: char, right: char, border: Style) -> Cell {
    let segments: Vec<String> = widths.iter().map(|width| "─".repeat(width + 2)).collect();
    vec![(
        Span::styled(
            format!("{left}{}{right}", segments.join(&join.to_string())),
            border,
        ),
        None,
    )]
}

fn grid_row(
    row: &[Vec<Cell>],
    line: usize,
    widths: &[usize],
    alignments: &[Alignment],
    border_style: Style,
) -> Cell {
    let border = || (Span::styled("│", border_style), None);
    let mut spans = vec![border()];
    for (column, lines) in row.iter().enumerate() {
        let content = lines.get(line).cloned().unwrap_or_default();
        let used: usize = content.iter().map(|(span, _)| span.content.width()).sum();
        let room = widths[column].saturating_sub(used);
        let before = match alignments.get(column) {
            Some(Alignment::Right) => room,
            Some(Alignment::Center) => room / 2,
            _ => 0,
        };
        spans.push((Span::raw(" ".repeat(before + 1)), None));
        spans.extend(content);
        spans.push((Span::raw(" ".repeat(room - before + 1)), None));
        spans.push(border());
    }
    spans
}
