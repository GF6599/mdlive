mod contents;
mod document;
mod folds;
mod front_matter;
mod highlight;
mod hyperlink;
mod keys;
mod math;
mod pictures;
mod search;
mod table;
pub mod theme;
mod watch;
mod wrap;

use std::path::Path;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui_image::picker::Picker;
use ratatui_image::sliced::SignedPosition;

use crate::document::Document;
use crate::folds::Folds;
pub use crate::hyperlink::{LinkArea, hyperlink_cells};
use crate::keys::{Command, KeyReader};
use crate::pictures::Pictures;
use crate::search::{Match, find_all};
pub use crate::watch::{FileWatch, watch_file};

/// Beyond this width a line of prose gets too long to read, so wider screens centre the text.
const MAX_TEXT_WIDTH: u16 = 100;
/// Below this width the text wraps every few words, so the contents covers the page instead.
const MIN_TEXT_BESIDE_CONTENTS: u16 = 40;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    Continue,
    Quit,
}

#[derive(Debug)]
struct Search {
    pattern: String,
    forward: bool,
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
    contents_open: bool,
    folds: Folds,
    /// The search being typed in the status line.
    prompt: Option<Search>,
    search: Option<Search>,
    current_match: Option<Match>,
    /// A note on the last search, such as a wrap at the end of the page.
    notice: Option<(String, Style)>,
    links: Vec<LinkArea>,
    pictures: Pictures,
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
            contents_open: false,
            folds: Folds::default(),
            prompt: None,
            search: None,
            current_match: None,
            notice: None,
            links: Vec::new(),
            pictures: Pictures::new(Path::new("."), Picker::halfblocks()),
        }
    }

    /// Reads pictures at relative paths from `base`, the folder of the Markdown file, and draws
    /// them the way `picker` found the terminal can.
    #[must_use]
    pub fn with_images(mut self, base: &Path, picker: Picker) -> Self {
        self.pictures = Pictures::new(base, picker);
        self
    }

    pub fn draw(&mut self, frame: &mut Frame) {
        let area = frame.area();
        frame.buffer_mut().set_style(area, theme::page());
        let [body, status] =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(area);

        let panel = self.contents_open.then(|| Rect {
            width: contents::WIDTH.min(body.width),
            ..body
        });
        let page = match panel {
            Some(panel) if body.width - panel.width >= MIN_TEXT_BESIDE_CONTENTS + 2 => Rect {
                x: body.x + panel.width,
                width: body.width - panel.width,
                ..body
            },
            _ => body,
        };
        let text_width = page.width.saturating_sub(2).clamp(1, MAX_TEXT_WIDTH);
        self.lay_out(usize::from(text_width));
        self.page = usize::from(body.height);
        self.top = self.top.min(self.last_top());

        let text = Rect {
            x: page.x + (page.width.saturating_sub(text_width)) / 2,
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
        self.mark_matches(frame, text);
        self.links = self.links_on_screen(text, panel);
        self.draw_pictures(frame, text, panel);
        if let Some(panel) = panel {
            let current = self.document.heading_at(self.top);
            contents::draw(frame, panel, self.document.headings(), current);
        }

        frame.buffer_mut().set_style(status, theme::status());
        let mut left = match (&self.prompt, &self.notice) {
            (Some(prompt), _) => {
                let mark = if prompt.forward { '/' } else { '?' };
                vec![Span::raw(format!(" {mark}{}", prompt.pattern))]
            }
            (None, Some((notice, style))) => vec![Span::styled(format!(" {notice}"), *style)],
            (None, None) => vec![Span::raw(format!(" {}", self.title))],
        };
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

    /// Draws the pictures in view, each from its first row, which may be above the page. A picture
    /// under the contents panel is left out, because a graphics protocol draws over the panel.
    fn draw_pictures(&mut self, frame: &mut Frame, text: Rect, panel: Option<Rect>) {
        let bottom = self.top + usize::from(text.height);
        for picture in self.document.pictures() {
            let end = picture.line + usize::from(picture.placed.rows);
            if end <= self.top || picture.line >= bottom {
                continue;
            }
            let Ok(column) = u16::try_from(picture.column) else {
                continue;
            };
            if panel.is_some_and(|panel| panel.right() > text.x.saturating_add(column)) {
                continue;
            }
            let offset = picture.line.abs_diff(self.top);
            let (Ok(x), Ok(y)) = (i16::try_from(column), i16::try_from(offset)) else {
                continue;
            };
            let y = if picture.line < self.top { -y } else { y };
            self.pictures
                .draw(frame, &picture.placed, text, SignedPosition { x, y });
        }
    }

    /// The links drawn in the last frame, for the terminal to open on a click.
    pub fn links(&self) -> &[LinkArea] {
        &self.links
    }

    /// The links in view, cut where the contents panel covers the page.
    fn links_on_screen(&self, text: Rect, panel: Option<Rect>) -> Vec<LinkArea> {
        let rows = self.top..self.top + usize::from(text.height);
        self.document
            .links()
            .iter()
            .filter(|link| rows.contains(&link.line))
            .filter_map(|link| {
                let x = text.x.checked_add(u16::try_from(link.column).ok()?)?;
                let right = x.checked_add(u16::try_from(link.width).ok()?)?;
                let y = text
                    .y
                    .checked_add(u16::try_from(link.line - self.top).ok()?)?;
                let x = panel.map_or(x, |panel| x.max(panel.right()));
                (x < right).then(|| LinkArea {
                    area: Rect {
                        x,
                        y,
                        width: right - x,
                        height: 1,
                    },
                    id: link.url,
                    url: self.document.url(link).to_string(),
                })
            })
            .collect()
    }

    /// Shows new file contents, keeping the reader under the same heading.
    pub fn reload(&mut self, markdown: &str) {
        self.error = None;
        self.markdown = markdown.to_string();
        self.render_in_place();
    }

    /// Lays the page out at `width`, and lets go of the pictures it no longer shows at their size.
    fn render(&mut self, width: usize) {
        self.document = Document::render(&self.markdown, width, &self.folds, &self.pictures);
        let placed = self
            .document
            .pictures()
            .iter()
            .map(|picture| &picture.placed);
        self.pictures.keep_only(placed);
    }

    /// Renders the page again, keeping the reader under the same heading.
    fn render_in_place(&mut self) {
        if let Some(width) = self.width {
            let anchor = self.document.anchor(self.top);
            self.render(width);
            self.current_match = None;
            self.top = self.document.screen_line_at(&anchor);
        }
    }

    /// Keeps the last good render on screen and names the failure in the status line.
    pub fn show_error(&mut self, message: &str) {
        self.error = Some(message.to_string());
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Flow {
        if self.prompt.is_some() {
            self.edit_prompt(key);
            return Flow::Continue;
        }
        self.notice = None;
        let half_page = (self.page / 2).max(1);
        self.keys.hint_targets = if self.contents_open {
            self.document.headings().len()
        } else {
            0
        };
        let Some(command) = self.keys.read(key) else {
            return Flow::Continue;
        };
        match command {
            Command::Quit => return Flow::Quit,
            Command::ToggleContents => self.contents_open = !self.contents_open,
            Command::Search { forward } => {
                self.prompt = Some(Search {
                    pattern: String::new(),
                    forward,
                });
            }
            Command::RepeatSearch { reverse, times } => self.step_search(reverse, times),
            Command::ToggleFold => {
                let bottom = self.top + self.page;
                if let Some(fold) = self
                    .document
                    .folds()
                    .iter()
                    .find(|fold| (self.top..bottom).contains(&fold.line))
                {
                    self.folds.set(&fold.summary, fold.occurrence, !fold.open);
                    self.render_in_place();
                }
            }
            Command::OpenAllFolds => {
                self.folds.set_all(true);
                self.render_in_place();
            }
            Command::CloseAllFolds => {
                self.folds.set_all(false);
                self.render_in_place();
            }
            Command::Heading(index) => {
                if let Some(heading) = self.document.headings().get(index) {
                    self.scroll_to(heading.line);
                }
            }
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
            Command::NextHeading(times) => {
                self.jump(times, usize::MAX, |document, top| {
                    document.next_heading(top)
                });
            }
            Command::PreviousHeading(times) => {
                self.jump(times, 0, |document, top| document.previous_heading(top));
            }
            Command::NextBlock(times) => {
                self.jump(times, usize::MAX, |document, top| document.next_block(top));
            }
            Command::PreviousBlock(times) => {
                self.jump(times, 0, |document, top| document.previous_block(top));
            }
        }
        Flow::Continue
    }

    /// Steps `times` targets from the top line, and goes to `beyond` when the targets run out, as
    /// vim goes to the first or last line.
    fn jump(
        &mut self,
        times: usize,
        beyond: usize,
        step: impl Fn(&Document, usize) -> Option<usize>,
    ) {
        let mut top = self.top;
        for _ in 0..times {
            match step(&self.document, top) {
                Some(line) => top = line,
                None => {
                    top = beyond;
                    break;
                }
            }
        }
        self.scroll_to(top);
    }

    fn edit_prompt(&mut self, key: KeyEvent) {
        let Some(prompt) = &mut self.prompt else {
            return;
        };
        let control = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Enter => {
                self.search = self
                    .prompt
                    .take()
                    .filter(|search| !search.pattern.is_empty());
                self.current_match = None;
                if self.search.is_some() {
                    self.step_search(false, 1);
                }
            }
            KeyCode::Esc => self.prompt = None,
            KeyCode::Char('c') if control => self.prompt = None,
            KeyCode::Backspace => {
                if prompt.pattern.pop().is_none() {
                    self.prompt = None;
                }
            }
            KeyCode::Char(c) if !control => prompt.pattern.push(c),
            _ => {}
        }
    }

    /// Moves `times` matches on in the search direction, or against it when `reverse`, and wraps
    /// at the ends of the page. The page scrolls only when the match is off screen.
    fn step_search(&mut self, reverse: bool, times: usize) {
        let Some(search) = &self.search else {
            return;
        };
        let matches = find_all(self.document.lines(), &search.pattern);
        let forward = search.forward != reverse;
        if matches.is_empty() {
            let notice = format!("Pattern not found: {}", search.pattern);
            self.notice = Some((notice, theme::error()));
            return;
        }
        // Counted from the end of the page when going backward, so both directions step up.
        let ahead = match (self.current_match, forward) {
            (Some(current), true) => matches.iter().filter(|m| **m <= current).count(),
            (Some(current), false) => matches.iter().filter(|m| **m >= current).count(),
            (None, true) => matches.iter().filter(|m| m.line < self.top).count(),
            (None, false) => matches.iter().filter(|m| m.line >= self.top).count(),
        };
        let steps = ahead.saturating_add(times.max(1) - 1);
        let wrapped = steps >= matches.len();
        let index = steps % matches.len();
        let found = if forward {
            matches[index]
        } else {
            matches[matches.len() - 1 - index]
        };
        if wrapped {
            let notice = if forward {
                "search hit BOTTOM, continuing at TOP"
            } else {
                "search hit TOP, continuing at BOTTOM"
            };
            self.notice = Some((notice.to_string(), theme::status()));
        }
        self.current_match = Some(found);
        if found.line < self.top || found.line >= self.top + self.page {
            self.scroll_to(found.line);
        }
    }

    fn mark_matches(&self, frame: &mut Frame, text: Rect) {
        let Some(search) = &self.search else {
            return;
        };
        let lines = self.document.lines();
        let bottom = (self.top + usize::from(text.height)).min(lines.len());
        let shown = lines.get(self.top..bottom).unwrap_or_default();
        for mut found in find_all(shown, &search.pattern) {
            found.line += self.top;
            let style = if self.current_match == Some(found) {
                theme::search_current()
            } else {
                theme::search_match()
            };
            let (Ok(row), Ok(column), Ok(width)) = (
                u16::try_from(found.line - self.top),
                u16::try_from(found.column),
                u16::try_from(found.width),
            ) else {
                continue;
            };
            let area = Rect {
                x: text.x + column,
                y: text.y + row,
                width: width.min(text.width.saturating_sub(column)),
                height: 1,
            };
            frame.buffer_mut().set_style(area, style);
        }
    }

    /// Lays the text out again when the width changes, keeping the block at the top in place.
    fn lay_out(&mut self, width: usize) {
        if self.width == Some(width) {
            return;
        }
        let anchor = self.document.source_line_at(self.top);
        self.render(width);
        self.current_match = None;
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
