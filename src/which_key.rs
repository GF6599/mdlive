use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use unicode_width::UnicodeWidthStr;

use crate::keys::{KeyReader, next_keys};
use crate::theme::Palette;

const GAP: &str = "  ";

/// The keys that can complete the sequence the reader has started.
pub struct Popup {
    title: String,
    entries: Vec<(String, &'static str)>,
}

impl Popup {
    /// The popup for the keys typed so far, or none when no binding starts with them, as with a
    /// contents hint letter.
    pub fn of(keys: &KeyReader) -> Option<Self> {
        let entries: Vec<_> = next_keys(keys.prefix()?, keys.focus)
            .into_iter()
            .map(|(key, action)| (key.label(), action.label()))
            .collect();
        (!entries.is_empty()).then(|| Self {
            title: keys.pending(),
            entries,
        })
    }

    /// The bottom right of `body`, one column in from its edge, cut to fit.
    pub fn area(&self, body: Rect) -> Rect {
        let key_width = self.width_of(|(key, _)| key.width());
        let label_width = self.width_of(|(_, label)| label.width());
        let inner = (1 + key_width + GAP.len() + label_width + 1).max(self.title.width() + 4);
        let width = u16::try_from(inner + 2)
            .unwrap_or(u16::MAX)
            .min(body.width.saturating_sub(1));
        let height = u16::try_from(self.entries.len() + 2)
            .unwrap_or(u16::MAX)
            .min(body.height);
        Rect {
            x: body.right().saturating_sub(width + 1).max(body.x),
            y: body.bottom() - height,
            width,
            height,
        }
    }

    fn width_of(&self, measure: impl Fn(&(String, &'static str)) -> usize) -> usize {
        self.entries.iter().map(measure).max().unwrap_or(0)
    }

    pub fn draw(&self, frame: &mut Frame, area: Rect, palette: &Palette) {
        let key_width = self.width_of(|(key, _)| key.width());
        let block = Block::new()
            .borders(Borders::ALL)
            .border_style(palette.panel_border())
            .title(Span::styled(
                format!(" {} ", self.title),
                palette.panel_title(),
            ));
        let inner = block.inner(area);
        frame.render_widget(Clear, area);
        frame.render_widget(block.style(palette.popup()), area);
        let lines: Vec<Line> = self
            .entries
            .iter()
            .map(|(key, label)| {
                Line::from(vec![
                    Span::styled(format!(" {key:<key_width$}"), palette.hint()),
                    Span::raw(format!("{GAP}{label}")),
                ])
            })
            .collect();
        frame.render_widget(Paragraph::new(lines), inner);
    }
}
