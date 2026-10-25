mod document;
pub mod theme;
mod wrap;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

use crate::document::Document;

/// Beyond this width a line of prose gets too long to read, so wider screens centre the text.
const MAX_TEXT_WIDTH: u16 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    Continue,
    Quit,
}

#[derive(Debug)]
pub struct Viewer {
    title: String,
    markdown: String,
    document: Document,
    width: Option<usize>,
    top: usize,
    page: usize,
}

impl Viewer {
    pub fn new(title: &str, markdown: &str) -> Self {
        Self {
            title: title.to_string(),
            markdown: markdown.to_string(),
            document: Document::default(),
            width: None,
            top: 0,
            page: 0,
        }
    }

    pub fn draw(&mut self, frame: &mut Frame) {
        let area = frame.area();
        frame.buffer_mut().set_style(area, theme::page());
        let [body, status] =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(area);

        let text_width = area.width.saturating_sub(2).clamp(1, MAX_TEXT_WIDTH);
        self.lay_out(usize::from(text_width));
        self.page = usize::from(body.height);
        self.top = self.top.min(self.last_top());

        let text = Rect {
            x: area.x + (area.width.saturating_sub(text_width)) / 2,
            width: text_width,
            ..body
        };
        let lines: Vec<Line> = self
            .document
            .lines()
            .iter()
            .skip(self.top)
            .take(self.page)
            .cloned()
            .collect();
        frame.render_widget(Paragraph::new(lines), text);

        frame.buffer_mut().set_style(status, theme::status());
        frame.render_widget(Paragraph::new(format!(" {}", self.title)), status);
        frame.render_widget(
            Paragraph::new(format!("{}% ", self.percent())).right_aligned(),
            status,
        );
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Flow {
        let control = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => return Flow::Quit,
            KeyCode::Char('c') if control => return Flow::Quit,
            KeyCode::Char('j') | KeyCode::Down => self.scroll_to(self.top.saturating_add(1)),
            KeyCode::Char('k') | KeyCode::Up => self.scroll_to(self.top.saturating_sub(1)),
            KeyCode::PageDown | KeyCode::Char(' ') => {
                self.scroll_to(self.top.saturating_add(self.page));
            }
            KeyCode::PageUp => self.scroll_to(self.top.saturating_sub(self.page)),
            KeyCode::Char('g') | KeyCode::Home => self.scroll_to(0),
            KeyCode::Char('G') | KeyCode::End => self.scroll_to(usize::MAX),
            _ => {}
        }
        Flow::Continue
    }

    /// Lays the text out again when the width changes, keeping the block at the top in place.
    fn lay_out(&mut self, width: usize) {
        if self.width == Some(width) {
            return;
        }
        let anchor = self.document.source_line_at(self.top);
        self.document = Document::render(&self.markdown, width);
        if self.width.is_some() {
            self.top = self.document.screen_line_of(anchor);
        }
        self.width = Some(width);
    }

    fn scroll_to(&mut self, top: usize) {
        self.top = match self.width {
            Some(_) => top.min(self.last_top()),
            None => top,
        };
    }

    fn last_top(&self) -> usize {
        self.document.len().saturating_sub(self.page)
    }

    fn percent(&self) -> usize {
        let len = self.document.len();
        if len == 0 {
            return 100;
        }
        (self.top + self.page).min(len) * 100 / len
    }
}
