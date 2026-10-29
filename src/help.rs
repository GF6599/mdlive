use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};
use unicode_width::UnicodeWidthStr;

use crate::keys::{Group, key_list};
use crate::theme::Palette;

const GAP: &str = "  ";

/// Each group title, then its keys and what each does, with the labels in one column.
fn lines(palette: &Palette) -> Vec<Line<'static>> {
    let groups: Vec<_> = Group::ALL
        .iter()
        .map(|group| (group.title(), key_list(*group)))
        .collect();
    let key_width = groups
        .iter()
        .flat_map(|(_, list)| list.iter().map(|(keys, _)| keys.width()))
        .max()
        .unwrap_or(0);
    let mut lines = Vec::new();
    for (title, list) in groups {
        if !lines.is_empty() {
            lines.push(Line::default());
        }
        lines.push(Line::styled(format!(" {title}"), palette.table_header()));
        for (keys, label) in list {
            let pad = " ".repeat(key_width - keys.width());
            lines.push(Line::from(vec![
                Span::styled(format!("   {keys}"), palette.hint()),
                Span::raw(format!("{pad}{GAP}{label}")),
            ]));
        }
    }
    lines
}

/// The first row that still fills `height` rows, so the list never scrolls past its end.
pub fn last_top(height: u16, palette: &Palette) -> usize {
    lines(palette).len().saturating_sub(usize::from(height))
}

/// Covers `area` with the key list, from row `top`.
pub fn draw(frame: &mut Frame, area: Rect, top: usize, palette: &Palette) {
    frame.render_widget(Clear, area);
    frame.buffer_mut().set_style(area, palette.page());
    let rows: Vec<Line> = lines(palette).into_iter().skip(top).collect();
    frame.render_widget(Paragraph::new(rows), area);
}
