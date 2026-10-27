mod contents;
mod diagrams;
mod document;
mod folds;
mod front_matter;
mod help;
mod highlight;
mod hint_bar;
mod hyperlink;
mod keys;
mod math;
mod pictures;
mod search;
mod table;
pub mod theme;
mod watch;
mod which_key;
mod wrap;

use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui_image::picker::Picker;
use ratatui_image::sliced::SignedPosition;

use crate::diagrams::Diagrams;
use crate::document::Document;
use crate::folds::Folds;
pub use crate::hyperlink::{LinkArea, hyperlink_cells};
use crate::keys::{Command, KeyReader};
use crate::pictures::Pictures;
use crate::search::{Match, find_all};
pub use crate::watch::{FileWatch, OpenFileWatch, watch_file};
use crate::which_key::Popup;

/// Beyond this width a line of prose gets too long to read, so wider screens centre the text.
const MAX_TEXT_WIDTH: u16 = 100;
/// Below this width the text wraps every few words, so the contents covers the page instead.
const MIN_TEXT_BESIDE_CONTENTS: u16 = 40;
/// A page with fewer headings than this reads fine without the contents, so it starts closed.
const MIN_HEADINGS_TO_OPEN_CONTENTS: usize = 3;

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

/// A page the reader left, and the source line at the top of the screen when they left it.
#[derive(Debug)]
struct Place {
    path: PathBuf,
    source_line: usize,
}

#[derive(Debug)]
pub struct Viewer {
    path: PathBuf,
    title: String,
    markdown: String,
    document: Document,
    width: Option<usize>,
    top: usize,
    page: usize,
    error: Option<String>,
    keys: KeyReader,
    /// `None` until the first draw decides from the screen width, or the reader presses t.
    contents_open: Option<bool>,
    /// The first row of the help screen in view, while it is open.
    help_top: Option<usize>,
    folds: Folds,
    /// The search being typed in the status line.
    prompt: Option<Search>,
    search: Option<Search>,
    current_match: Option<Match>,
    /// A note on the last search, such as a wrap at the end of the page.
    notice: Option<(String, Style)>,
    links: Vec<LinkArea>,
    pictures: Pictures,
    diagrams: Diagrams,
    back: Vec<Place>,
    forward: Vec<Place>,
}

impl Viewer {
    pub fn new(title: &str, markdown: &str) -> Self {
        Self {
            path: PathBuf::from(title),
            title: title.to_string(),
            markdown: markdown.to_string(),
            document: Document::default(),
            width: None,
            top: 0,
            page: 0,
            error: None,
            keys: KeyReader::default(),
            contents_open: None,
            help_top: None,
            folds: Folds::default(),
            prompt: None,
            search: None,
            current_match: None,
            notice: None,
            links: Vec::new(),
            pictures: Pictures::new(Path::new("."), Picker::halfblocks()),
            diagrams: Diagrams::default(),
            back: Vec::new(),
            forward: Vec::new(),
        }
    }

    /// Names the file the page came from, which the reader goes back to after opening another.
    #[must_use]
    pub fn with_path(mut self, path: &Path) -> Self {
        self.path = path.to_path_buf();
        self
    }

    /// The file on screen, which a watch follows to reload the page.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Shows the Markdown file at `path` from its top, and keeps this page for Ctrl-o.
    pub fn open(&mut self, path: &Path) {
        match std::fs::read_to_string(path) {
            Ok(markdown) => {
                let here = self.place();
                self.back.push(here);
                self.forward.clear();
                self.show(path, &markdown, 0);
            }
            Err(error) => self.show_error(&format!("cannot open {}: {error}", path.display())),
        }
    }

    /// Goes to the last page of `from` and keeps this page on `to`, the other way through the
    /// history. A page that cannot be read again stays where it was.
    fn step_history(
        &mut self,
        from: fn(&mut Self) -> &mut Vec<Place>,
        to: fn(&mut Self) -> &mut Vec<Place>,
    ) {
        let Some(place) = from(self).pop() else {
            return;
        };
        match std::fs::read_to_string(&place.path) {
            Ok(markdown) => {
                let here = self.place();
                to(self).push(here);
                self.show(&place.path, &markdown, place.source_line);
            }
            Err(error) => {
                self.show_error(&format!("cannot open {}: {error}", place.path.display()));
                from(self).push(place);
            }
        }
    }

    fn place(&self) -> Place {
        Place {
            path: self.path.clone(),
            source_line: self.document.source_line_at(self.top),
        }
    }

    /// Puts the page of `path` on screen with `source_line` at the top, and drops what belonged to
    /// the page before it: its folds, its search and its error.
    fn show(&mut self, path: &Path, markdown: &str, source_line: usize) {
        self.path = path.to_path_buf();
        self.title = path.file_name().map_or_else(
            || path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
        self.markdown = markdown.to_string();
        self.error = None;
        self.folds = Folds::default();
        self.prompt = None;
        self.search = None;
        self.current_match = None;
        self.pictures.set_base(folder_of(path));
        self.top = 0;
        if let Some(width) = self.width {
            self.render(width);
            self.top = self.document.screen_line_of(source_line);
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
        let [body, bar, status] = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(area);

        if self.contents_open.is_none() {
            self.contents_open = Some(self.contents_fit_at_start(body.width));
        }
        let panel = self.contents_is_open().then(|| Rect {
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
        let popup = Popup::of(&self.keys).map(|popup| (popup.area(body), popup));
        if let Some(top) = self.help_top {
            self.help_top = Some(top.min(help::last_top(body.height)));
        }
        let covered = match self.help_top {
            Some(_) => Some(body),
            None => popup.as_ref().map(|(area, _)| *area),
        };
        self.links = self.links_on_screen(text, panel, covered);
        self.draw_pictures(frame, text, panel, covered);
        if let Some(panel) = panel {
            let current = self.document.heading_at(self.top);
            contents::draw(frame, panel, self.document.headings(), current);
        }
        if let Some(top) = self.help_top {
            help::draw(frame, body, top);
        }
        if let Some((area, popup)) = &popup {
            popup.draw(frame, *area);
        }

        hint_bar::draw(frame, bar, &hint_bar::hints_for(&self.mode()));

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
    /// under the contents panel or the `covered` key popup is left out, because a graphics
    /// protocol draws over them.
    fn draw_pictures(
        &mut self,
        frame: &mut Frame,
        text: Rect,
        panel: Option<Rect>,
        covered: Option<Rect>,
    ) {
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
            let shown = Rect {
                x: text.x.saturating_add(column),
                y: text.y.saturating_add(
                    u16::try_from(picture.line.saturating_sub(self.top)).unwrap_or(u16::MAX),
                ),
                width: picture.placed.columns,
                height: u16::try_from(end.min(bottom) - picture.line.max(self.top))
                    .unwrap_or(u16::MAX),
            };
            if covered.is_some_and(|covered| covered.intersects(shown)) {
                continue;
            }
            self.pictures
                .draw(frame, &picture.placed, text, SignedPosition { x, y });
        }
    }

    /// The links drawn in the last frame, for the terminal to open on a click.
    pub fn links(&self) -> &[LinkArea] {
        &self.links
    }

    /// The links in view, cut where the contents panel covers the page, and without the ones the
    /// `covered` key popup overlaps.
    fn links_on_screen(
        &self,
        text: Rect,
        panel: Option<Rect>,
        covered: Option<Rect>,
    ) -> Vec<LinkArea> {
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
            .filter(|link| !covered.is_some_and(|covered| covered.intersects(link.area)))
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
        self.document = Document::render(
            &self.markdown,
            width,
            &self.folds,
            &self.pictures,
            &self.diagrams,
        );
        self.diagrams.forget_unused();
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
        self.keys.hint_targets = if self.contents_is_open() {
            self.document.headings().len()
        } else {
            0
        };
        let Some(command) = self.keys.read(key) else {
            return Flow::Continue;
        };
        if let Some(top) = self.help_top {
            self.help_top = scroll_help(command, top, half_page, self.page);
            return Flow::Continue;
        }
        match command {
            Command::Quit => return Flow::Quit,
            Command::ToggleHelp => self.help_top = Some(0),
            Command::Back => {
                self.step_history(|viewer| &mut viewer.back, |viewer| &mut viewer.forward)
            }
            Command::Forward => {
                self.step_history(|viewer| &mut viewer.forward, |viewer| &mut viewer.back)
            }
            Command::ToggleContents => self.contents_open = Some(!self.contents_is_open()),
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

    fn mode(&self) -> hint_bar::Mode {
        if self.prompt.is_some() {
            return hint_bar::Mode::TypingSearch;
        }
        if self.help_top.is_some() {
            return hint_bar::Mode::Help;
        }
        hint_bar::Mode::Reading {
            headings_in_contents: self
                .contents_is_open()
                .then(|| self.document.headings().len()),
            search_active: self.search.is_some(),
            can_go_back: !self.back.is_empty(),
            has_folds: !self.document.folds().is_empty(),
        }
    }

    fn contents_is_open(&self) -> bool {
        self.contents_open == Some(true)
    }

    /// The contents starts open only where it leaves the page its full text width.
    fn contents_fit_at_start(&mut self, width: u16) -> bool {
        if width < contents::WIDTH + MAX_TEXT_WIDTH + 2 {
            return false;
        }
        self.lay_out(usize::from(MAX_TEXT_WIDTH));
        self.document.headings().len() >= MIN_HEADINGS_TO_OPEN_CONTENTS
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

/// Moves the help screen for a movement command. Quit and the help keys close it, and every other
/// command does nothing while it is open.
fn scroll_help(command: Command, top: usize, half_page: usize, page: usize) -> Option<usize> {
    match command {
        Command::Quit | Command::ToggleHelp => None,
        Command::Down(times) => Some(top.saturating_add(times)),
        Command::Up(times) => Some(top.saturating_sub(times)),
        Command::HalfPageDown(times) => Some(top.saturating_add(half_page.saturating_mul(times))),
        Command::HalfPageUp(times) => Some(top.saturating_sub(half_page.saturating_mul(times))),
        Command::PageDown(times) => Some(top.saturating_add(page.saturating_mul(times))),
        Command::PageUp(times) => Some(top.saturating_sub(page.saturating_mul(times))),
        Command::Top => Some(0),
        Command::End => Some(usize::MAX),
        _ => Some(top),
    }
}

/// The folder that relative paths in the file at `path` start from.
pub fn folder_of(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
}
