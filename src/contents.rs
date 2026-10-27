use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::document::Heading;
use crate::keys::hints;
use crate::theme;

pub const WIDTH: u16 = 30;
const MARKER: &str = "◀";
const INDENT: usize = 2;

/// Draws the headings as a list, marking `current` and scrolling so it stays in view.
pub fn draw(frame: &mut Frame, area: Rect, headings: &[Heading], current: Option<usize>) {
    let block = Block::new()
        .borders(Borders::ALL)
        .border_style(theme::panel_border())
        .title(Span::styled(" Contents ", theme::panel_title()));
    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block.style(theme::page()), area);

    let rows = usize::from(inner.height);
    let text_width = usize::from(inner.width).saturating_sub(MARKER.width());
    if headings.is_empty() {
        let note = Span::styled(" No headings", theme::html());
        frame.render_widget(Paragraph::new(Line::from(note)), inner);
        return;
    }
    let top_level = headings
        .iter()
        .map(|heading| heading.level)
        .min()
        .unwrap_or(1);
    let first = first_entry(rows, headings.len(), current);
    let hints = hints(headings.len());
    let lines: Vec<Line> = headings
        .iter()
        .enumerate()
        .skip(first)
        .take(rows)
        .map(|(index, heading)| {
            let hint = hints
                .get(index)
                .map(|hint| format!("[{hint}] "))
                .unwrap_or_default();
            let indent = " ".repeat(INDENT * usize::from(heading.level - top_level));
            let lead = format!(" {hint}{indent}");
            let text = fit(&heading.text, text_width.saturating_sub(lead.width()));
            let pad = " ".repeat(text_width.saturating_sub(lead.width() + text.width()));
            let (style, marker) = if current == Some(index) {
                (theme::contents_marked(), MARKER)
            } else {
                (theme::contents_entry(heading.level), " ")
            };
            Line::from(vec![
                Span::styled(lead, theme::hint()),
                Span::styled(text, style),
                Span::raw(pad),
                Span::styled(marker, theme::contents_marked()),
            ])
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), inner);
}

/// Cuts `text` to `width` columns, ending in an ellipsis when it had to cut.
pub fn fit(text: &str, width: usize) -> String {
    if text.width() <= width {
        return text.to_string();
    }
    let mut cut = String::new();
    for grapheme in text.graphemes(true) {
        if cut.width() + grapheme.width() + 1 > width {
            break;
        }
        cut.push_str(grapheme);
    }
    cut + "…"
}

/// The entry at the top of a list of `count` entries in `rows` rows, which keeps `current` near
/// the middle.
fn first_entry(rows: usize, count: usize, current: Option<usize>) -> usize {
    current
        .unwrap_or(0)
        .saturating_sub(rows / 2)
        .min(count.saturating_sub(rows))
}

/// The entry drawn on screen row `y` of a contents drawn in `area`, as `draw` lays it out.
pub fn entry_at(area: Rect, count: usize, current: Option<usize>, y: u16) -> Option<usize> {
    let inner = Block::new().borders(Borders::ALL).inner(area);
    let row = usize::from(y.checked_sub(inner.y)?);
    if row >= usize::from(inner.height) {
        return None;
    }
    let entry = first_entry(usize::from(inner.height), count, current) + row;
    (entry < count).then_some(entry)
}
