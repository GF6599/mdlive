mod document;
mod highlight;
mod keys;
mod table;
pub mod theme;
mod watch;
mod wrap;

use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::document::Document;
use crate::keys::{Command, KeyReader};
pub use crate::watch::{FileWatch, watch_file};

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
    error: Option<String>,
    keys: KeyReader,
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
            error: None,
            keys: KeyReader::default(),
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
        let mut left = vec![Span::raw(format!(" {}", self.title))];
        if let Some(error) = &self.error {
            left.push(Span::styled(format!("  {error}"), theme::error()));
        }
        frame.render_widget(Paragraph::new(Line::from(left)), status);
        let pending = self.keys.pending();
        let gap = if pending.is_empty() { "" } else { "  " };
        frame.render_widget(
            Paragraph::new(format!("{pending}{gap}{}% ", self.percent())).right_aligned(),
            status,
        );
    }

    /// Shows new file contents, keeping the reader under the same heading.
    pub fn reload(&mut self, markdown: &str) {
        self.error = None;
        self.markdown = markdown.to_string();
        if let Some(width) = self.width {
            let anchor = self.document.anchor(self.top);
            self.document = Document::render(&self.markdown, width);
            self.top = self.document.screen_line_at(&anchor);
        }
    }

    /// Keeps the last good render on screen and names the failure in the status line.
    pub fn show_error(&mut self, message: &str) {
        self.error = Some(message.to_string());
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Flow {
        let half_page = (self.page / 2).max(1);
        let Some(command) = self.keys.read(key) else {
            return Flow::Continue;
        };
        match command {
            Command::Quit => return Flow::Quit,
            Command::Down(times) => self.scroll_to(self.top.saturating_add(times)),
            Command::Up(times) => self.scroll_to(self.top.saturating_sub(times)),
            Command::HalfPageDown(times) => {
                self.scroll_to(self.top.saturating_add(half_page.saturating_mul(times)));
            }
            Command::HalfPageUp(times) => {
                self.scroll_to(self.top.saturating_sub(half_page.saturating_mul(times)));
            }
            Command::PageDown(times) => {
                self.scroll_to(self.top.saturating_add(self.page.saturating_mul(times)));
            }
            Command::PageUp(times) => {
                self.scroll_to(self.top.saturating_sub(self.page.saturating_mul(times)));
            }
            Command::Top => self.scroll_to(0),
            Command::End => self.scroll_to(usize::MAX),
            Command::SourceLine(line) => {
                self.scroll_to(self.document.screen_line_of(line.saturating_sub(1)));
            }
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
