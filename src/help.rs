use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};
use unicode_width::UnicodeWidthStr;

use crate::keys::{Group, key_list};
use crate::theme;

const GAP: &str = "  ";

/// Each group title, then its keys and what each does, with the labels in one column.
fn lines() -> Vec<Line<'static>> {
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
        lines.push(Line::styled(format!(" {title}"), theme::table_header()));
        for (keys, label) in list {
            let pad = " ".repeat(key_width - keys.width());
            lines.push(Line::from(vec![
                Span::styled(format!("   {keys}"), theme::hint()),
                Span::raw(format!("{pad}{GAP}{label}")),
            ]));
        }
    }
    lines
}

/// The first row that still fills `height` rows, so the list never scrolls past its end.
pub fn last_top(height: u16) -> usize {
    lines().len().saturating_sub(usize::from(height))
}

/// Covers `area` with the key list, from row `top`.
pub fn draw(frame: &mut Frame, area: Rect, top: usize) {
    frame.render_widget(Clear, area);
    frame.buffer_mut().set_style(area, theme::page());
    let rows: Vec<Line> = lines().into_iter().skip(top).collect();
    frame.render_widget(Paragraph::new(rows), area);
}
