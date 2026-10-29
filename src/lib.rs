mod breadcrumb;
mod contents;
mod diagrams;
mod document;
mod folds;
mod frame;
mod front_matter;
mod help;
mod highlight;
mod hint_bar;
mod hyperlink;
mod keys;
mod math;
mod picture_view;
mod pictures;
mod search;
mod table;
pub mod theme;
mod tree;
mod watch;
mod which_key;
mod wrap;

use std::fmt;
use std::path::{Path, PathBuf};

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui_image::picker::Picker;
use ratatui_image::sliced::SignedPosition;

use crate::diagrams::Diagrams;
use crate::document::Document;
use crate::folds::Folds;
pub use crate::frame::draw_frame;
use crate::hyperlink::is_openable;
pub use crate::hyperlink::{LinkArea, hyperlink_cells};
use crate::keys::{Command, Focus, KeyReader};
use crate::picture_view::PictureView;
use crate::pictures::{Pictures, pixel_size};
use crate::search::{Match, find_all};
use crate::tree::{Tree, root_of};
pub use crate::watch::{FileWatch, OpenFileWatch, watch_file};
use crate::which_key::Popup;

/// Beyond this width a line of prose gets too long to read, so wider screens centre the text.
const MAX_TEXT_WIDTH: u16 = 100;
/// Below this width the text wraps every few words, so the contents covers the page instead.
const MIN_TEXT_BESIDE_CONTENTS: u16 = 40;
/// A page with fewer headings than this reads fine without the contents, so it starts closed.
const MIN_HEADINGS_TO_OPEN_CONTENTS: usize = 3;
/// How far one turn of the wheel moves, as most readers move a page.
const WHEEL_LINES: usize = 3;
const PICTURE_MARK: &str = "▶";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Flow {
    Continue,
    Quit,
    /// Open the editor on the file, at the source line, counted from 1.
    Edit(PathBuf, usize),
}

/// Handles each event that `next` has ready, until none is left or one leaves the page, so a burst
/// of keys or wheel events costs one frame. `None` when `handle` used no event.
pub fn handle_burst(
    mut next: impl FnMut() -> std::io::Result<Option<Event>>,
    mut handle: impl FnMut(Event) -> Option<Flow>,
) -> std::io::Result<Option<Flow>> {
    let mut handled = None;
    while let Some(event) = next()? {
        match handle(event) {
            Some(Flow::Continue) => handled = Some(Flow::Continue),
            Some(flow) => return Ok(Some(flow)),
            None => {}
        }
    }
    Ok(handled)
}

#[derive(Debug)]
struct Search {
    pattern: String,
    forward: bool,
}

/// The page lines selected in visual mode, as screen lines: where the selection started and where
/// its cursor is.
#[derive(Debug, Clone, Copy)]
struct Selection {
    anchor: usize,
    cursor: usize,
}

impl Selection {
    /// The first and the last selected line, whichever way the cursor moved.
    fn lines(self) -> (usize, usize) {
        (self.anchor.min(self.cursor), self.anchor.max(self.cursor))
    }
}

/// Where the left button went down on the page: its screen line, and the link or picture under
/// it.
#[derive(Debug, Clone, Copy)]
struct Press {
    line: usize,
    link: Option<usize>,
    picture: Option<usize>,
}

/// A page the reader left, and the source line at the top of the screen when they left it.
#[derive(Debug)]
struct Place {
    path: PathBuf,
    source_line: usize,
}

#[derive(Debug)]
pub struct Viewer {
    /// The file on the page, none on the empty page of a viewer started on a folder.
    path: Option<PathBuf>,
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
    /// The file tree, while it is open in place of the contents.
    tree: Option<Tree>,
    /// The folder the tree opens at, when the viewer started on a folder instead of a file.
    tree_root: Option<PathBuf>,
    folds: Folds,
    /// The search being typed in the status line.
    prompt: Option<Search>,
    search: Option<Search>,
    current_match: Option<Match>,
    /// A note on the last search, such as a wrap at the end of the page.
    notice: Option<(String, Style)>,
    links: Vec<LinkArea>,
    /// Every link drawn in the last frame, for a click to find the one it hit.
    link_areas: Vec<LinkArea>,
    /// The cells of each picture drawn in the last frame, by its index in the page's pictures.
    picture_areas: Vec<(usize, Rect)>,
    pictures: Pictures,
    diagrams: Diagrams,
    back: Vec<Place>,
    /// The link index of the link that Enter follows.
    marked_link: Option<usize>,
    /// The index in the page's pictures of the picture that Enter opens.
    marked_picture: Option<usize>,
    picture_view: Option<PictureView>,
    opener: Handoff,
    selection: Option<Selection>,
    copier: Handoff,
    /// Where the last frame drew the contents or the tree, for a click to find what it hit.
    panel: Option<Rect>,
    /// Where the last frame drew the page text, for a press or a drag to find its line.
    text_area: Rect,
    press: Option<Press>,
    forward: Vec<Place>,
}

impl Viewer {
    pub fn new(title: &str, markdown: &str) -> Self {
        Self {
            path: None,
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
            tree: None,
            tree_root: None,
            folds: Folds::default(),
            prompt: None,
            search: None,
            current_match: None,
            notice: None,
            links: Vec::new(),
            link_areas: Vec::new(),
            picture_areas: Vec::new(),
            pictures: Pictures::new(Path::new("."), Picker::halfblocks()),
            diagrams: Diagrams::default(),
            back: Vec::new(),
            marked_link: None,
            marked_picture: None,
            picture_view: None,
            opener: Handoff(None),
            selection: None,
            copier: Handoff(None),
            panel: None,
            text_area: Rect::default(),
            press: None,
            forward: Vec::new(),
        }
    }

    /// Hands web and mail addresses to `open`, which starts the program that shows them.
    #[must_use]
    pub fn with_opener(mut self, open: impl FnMut(&str) -> Result<(), String> + 'static) -> Self {
        self.opener = Handoff(Some(Box::new(open)));
        self
    }

    /// Hands the Markdown source of the selected lines to `copy`, which puts it on the clipboard.
    #[must_use]
    pub fn with_copier(mut self, copy: impl FnMut(&str) -> Result<(), String> + 'static) -> Self {
        self.copier = Handoff(Some(Box::new(copy)));
        self
    }

    /// Names the file the page came from, which the reader goes back to after opening another.
    #[must_use]
    pub fn with_path(mut self, path: &Path) -> Self {
        self.path = Some(path.to_path_buf());
        self
    }

    /// Starts with the tree of `root` holding the keys, for a viewer started on a folder.
    #[must_use]
    pub fn with_tree(mut self, root: &Path) -> Self {
        self.tree_root = Some(root.to_path_buf());
        self.enter_tree();
        self
    }

    /// The file on screen, which a watch follows to reload the page.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Shows the Markdown file at `path` from its top, and keeps this page for Ctrl-o.
    pub fn open(&mut self, path: &Path) {
        match std::fs::read_to_string(path) {
            Ok(markdown) => {
                if let Some(here) = self.place() {
                    self.back.push(here);
                }
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
                if let Some(here) = self.place() {
                    to(self).push(here);
                }
                self.show(&place.path, &markdown, place.source_line);
            }
            Err(error) => {
                self.show_error(&format!("cannot open {}: {error}", place.path.display()));
                from(self).push(place);
            }
        }
    }

    fn place(&self) -> Option<Place> {
        Some(Place {
            path: self.path.clone()?,
            source_line: self.document.source_line_at(self.top),
        })
    }

    /// Puts the page of `path` on screen with `source_line` at the top, and drops what belonged to
    /// the page before it: its folds, its search and its error.
    fn show(&mut self, path: &Path, markdown: &str, source_line: usize) {
        self.path = Some(path.to_path_buf());
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
        self.marked_link = None;
        self.marked_picture = None;
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

    /// Draws pictures the way `picker` found the terminal can, as after a zoom changes the size of
    /// its cells.
    pub fn set_picker(&mut self, picker: Picker) {
        self.pictures.set_picker(picker);
        self.render_in_place();
    }

    pub fn draw(&mut self, frame: &mut Frame) {
        let area = frame.area();
        frame.buffer_mut().set_style(area, theme::page());
        let [header, body, bar, status] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Fill(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(area);

        if let Some(view) = &mut self.picture_view {
            self.pictures.draw_view(frame, view, body);
            self.links.clear();
            self.link_areas.clear();
            self.picture_areas.clear();
            breadcrumb::draw(frame, header, &self.title, &[]);
            hint_bar::draw(frame, bar, &hint_bar::hints_for(&hint_bar::Mode::Picture));
            self.draw_status(frame, status);
            return;
        }
        if self.contents_open.is_none() {
            self.contents_open = Some(self.contents_fit_at_start(body.width));
        }
        let panel = (self.tree.is_some() || self.contents_is_open()).then(|| Rect {
            width: contents::WIDTH.min(body.width),
            ..body
        });
        self.panel = panel;
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
        self.text_area = text;
        self.mark_matches(frame, text);
        self.mark_link(frame, text);
        self.mark_picture(frame, text);
        self.mark_selection(frame, text);
        let popup = Popup::of(&self.keys).map(|popup| (popup.area(body), popup));
        if let Some(top) = self.help_top {
            self.help_top = Some(top.min(help::last_top(body.height)));
        }
        let covered = match self.help_top {
            Some(_) => Some(body),
            None => popup.as_ref().map(|(area, _)| *area),
        };
        self.link_areas = self.links_on_screen(text, panel, covered);
        self.links = self
            .link_areas
            .iter()
            .filter(|link| is_openable(&link.url))
            .cloned()
            .collect();
        self.draw_pictures(frame, text, panel, covered);
        match (&mut self.tree, panel) {
            (Some(tree), Some(panel)) => {
                let focused = self.keys.focus == Focus::Tree;
                tree.draw(frame, panel, self.path.as_deref(), focused);
            }
            (None, Some(panel)) => {
                let current = self.document.heading_at(self.top);
                contents::draw(frame, panel, self.document.headings(), current);
            }
            _ => {}
        }
        if let Some(top) = self.help_top {
            help::draw(frame, body, top);
        }
        if let Some((area, popup)) = &popup {
            popup.draw(frame, *area);
        }

        let sections =
            breadcrumb::sections(self.document.headings(), self.document.heading_at(self.top));
        breadcrumb::draw(frame, header, &self.title, &sections);
        hint_bar::draw(frame, bar, &hint_bar::hints_for(&self.mode()));
        self.draw_status(frame, status);
    }

    fn draw_status(&self, frame: &mut Frame, status: Rect) {
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
        self.picture_areas.clear();
        for (index, picture) in self.document.pictures().iter().enumerate() {
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
            self.picture_areas.push((index, shown.intersection(text)));
            self.pictures
                .draw(frame, &picture.placed, text, SignedPosition { x, y });
        }
    }

    /// The links drawn in the last frame, for the terminal to open on a click.
    pub fn links(&self) -> &[LinkArea] {
        &self.links
    }

    /// The Kitty commands for the picture view that go out after the cells of the last frame.
    pub fn take_picture_commands(&mut self) -> String {
        self.pictures.take_commands()
    }

    /// Tells the viewer that the terminal cleared the screen, as ratatui does on each resize.
    pub fn screen_cleared(&mut self) {
        self.pictures.screen_cleared();
    }

    /// The links in view, cut where the contents panel covers the page, and
    /// without the ones the `covered` key popup overlaps.
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
        self.marked_link = None;
        self.marked_picture = None;
        self.leave_visual();
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
        if let Some(tree) = self.tree.as_mut().filter(|tree| tree.is_typing()) {
            tree.edit_filter(key);
            return Flow::Continue;
        }
        self.notice = None;
        let half_page = (self.page / 2).max(1);
        self.keys.hint_targets = if self.contents_is_open() && self.tree.is_none() {
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
        match self.keys.focus {
            Focus::Tree => return self.handle_tree_command(command, half_page),
            Focus::Visual => return self.handle_visual_command(command, half_page),
            Focus::Picture => return self.handle_picture_command(command),
            Focus::Page => {}
        }
        match command {
            Command::Quit => return Flow::Quit,
            Command::Cancel => {
                let marked = self.marked_link.take().or(self.marked_picture.take());
                if marked.is_none() {
                    return Flow::Quit;
                }
            }
            Command::NextLink(times) => (0..times).for_each(|_| self.step_link(true)),
            Command::PreviousLink(times) => (0..times).for_each(|_| self.step_link(false)),
            Command::NextPicture(times) => (0..times).for_each(|_| self.step_picture(true)),
            Command::PreviousPicture(times) => (0..times).for_each(|_| self.step_picture(false)),
            Command::OpenMark => match self.marked_picture {
                Some(index) => self.open_picture(index),
                None => self.follow_marked_link(),
            },
            Command::Edit => {
                let Some(path) = self.path.clone() else {
                    self.show_error("no file to edit");
                    return Flow::Continue;
                };
                return Flow::Edit(path, self.document.source_line_at(self.top) + 1);
            }
            Command::ToggleTree => self.enter_tree(),
            Command::SelectLines if !self.document.is_empty() => {
                let cursor = (self.top + self.page / 2).min(self.document.len() - 1);
                self.selection = Some(Selection {
                    anchor: cursor,
                    cursor,
                });
                self.keys.focus = Focus::Visual;
            }
            Command::OpenEntry
            | Command::CloseFolder
            | Command::FilterTree
            | Command::SelectLines
            | Command::CopySelection
            | Command::ZoomIn(_)
            | Command::ZoomOut(_)
            | Command::ZoomFit
            | Command::Pan { .. }
            | Command::ClosePicture => {}
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

    /// Puts the line `times` targets from the top line at the top.
    fn jump(
        &mut self,
        times: usize,
        beyond: usize,
        step: impl Fn(&Document, usize) -> Option<usize>,
    ) {
        let top = self.target(self.top, times, beyond, step);
        self.scroll_to(top);
    }

    /// The line `times` targets from `from`, or `beyond` when the targets run out, as vim goes to
    /// the first or last line.
    fn target(
        &self,
        from: usize,
        times: usize,
        beyond: usize,
        step: impl Fn(&Document, usize) -> Option<usize>,
    ) -> usize {
        let mut line = from;
        for _ in 0..times {
            match step(&self.document, line) {
                Some(next) => line = next,
                None => return beyond,
            }
        }
        line
    }

    /// Moves the cursor of the selection for a movement key and keeps it on screen, copies for y,
    /// and leaves visual mode for Escape.
    fn handle_visual_command(&mut self, command: Command, half_page: usize) -> Flow {
        let Some(selection) = self.selection else {
            self.leave_visual();
            return Flow::Continue;
        };
        let cursor = selection.cursor;
        let moved = match command {
            Command::Quit => return Flow::Quit,
            Command::Cancel => {
                self.leave_visual();
                return Flow::Continue;
            }
            Command::CopySelection => {
                self.leave_visual();
                self.copy(selection);
                return Flow::Continue;
            }
            Command::Down(times) => cursor.saturating_add(times),
            Command::Up(times) => cursor.saturating_sub(times),
            Command::HalfPageDown(times) => cursor.saturating_add(half_page.saturating_mul(times)),
            Command::HalfPageUp(times) => cursor.saturating_sub(half_page.saturating_mul(times)),
            Command::PageDown(times) => cursor.saturating_add(self.page.saturating_mul(times)),
            Command::PageUp(times) => cursor.saturating_sub(self.page.saturating_mul(times)),
            Command::Top => 0,
            Command::End => usize::MAX,
            Command::SourceLine(line) => self.document.screen_line_of(line.saturating_sub(1)),
            Command::NextHeading(times) => {
                self.target(cursor, times, usize::MAX, Document::next_heading)
            }
            Command::PreviousHeading(times) => {
                self.target(cursor, times, 0, Document::previous_heading)
            }
            Command::NextBlock(times) => {
                self.target(cursor, times, usize::MAX, Document::next_block)
            }
            Command::PreviousBlock(times) => {
                self.target(cursor, times, 0, Document::previous_block)
            }
            _ => return Flow::Continue,
        };
        let cursor = moved.min(self.document.len().saturating_sub(1));
        self.selection = Some(Selection {
            cursor,
            ..selection
        });
        if cursor < self.top {
            self.scroll_to(cursor);
        } else if cursor >= self.top + self.page {
            self.scroll_to(cursor + 1 - self.page);
        }
        Flow::Continue
    }

    fn leave_visual(&mut self) {
        self.selection = None;
        if self.keys.focus == Focus::Visual {
            self.keys.focus = Focus::Page;
        }
    }

    /// Hands the Markdown source behind `selection` to the copier, without the blank lines that
    /// end it, and names the outcome in the status line.
    fn copy(&mut self, selection: Selection) {
        let (first, last) = selection.lines();
        let range = self.document.source_lines_of(first, last);
        let lines: Vec<&str> = self
            .markdown
            .lines()
            .skip(range.start)
            .take(range.len())
            .collect();
        let count = lines
            .iter()
            .rposition(|line| !line.trim().is_empty())
            .map_or(0, |at| at + 1);
        let text = lines[..count].join("\n");
        let result = match &mut self.copier.0 {
            Some(copy) => copy(&text),
            None => Err(String::from("no clipboard")),
        };
        match result {
            Ok(()) => {
                let plural = if count == 1 { "" } else { "s" };
                self.notice = Some((format!("copied {count} line{plural}"), theme::status()));
            }
            Err(error) => self.show_error(&format!("cannot copy: {error}")),
        }
    }

    /// Colours the selected rows on screen across the text, and the cursor row brighter.
    fn mark_selection(&self, frame: &mut Frame, text: Rect) {
        let Some(selection) = self.selection else {
            return;
        };
        let (first, last) = selection.lines();
        let bottom = self.top + usize::from(text.height);
        for line in first.max(self.top)..=last.min(bottom.saturating_sub(1)) {
            let Ok(row) = u16::try_from(line - self.top) else {
                continue;
            };
            let style = if line == selection.cursor {
                theme::selection_cursor()
            } else {
                theme::selection()
            };
            let area = Rect {
                y: text.y.saturating_add(row),
                height: 1,
                ..text
            };
            frame.buffer_mut().set_style(area, style);
        }
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

    /// `None` for an event the viewer does not use, such as a key release or a pointer move.
    pub fn handle_event(&mut self, event: Event) -> Option<Flow> {
        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => Some(self.handle_key(key)),
            Event::Mouse(mouse) if is_used(mouse.kind) => Some(self.handle_mouse(mouse)),
            _ => None,
        }
    }

    /// The wheel moves what is under the pointer, the tree or the page, and a click acts on a
    /// contents entry, a tree row or a link, and gives the keys to the tree or the page it lands on.
    /// A drag over the page selects lines, and the release copies them.
    pub fn handle_mouse(&mut self, event: MouseEvent) -> Flow {
        if let Some(view) = &mut self.picture_view {
            match event.kind {
                MouseEventKind::ScrollDown => view.pan(0, 1),
                MouseEventKind::ScrollUp => view.pan(0, -1),
                _ => {}
            }
            return Flow::Continue;
        }
        if self.prompt.is_some() || self.tree.as_ref().is_some_and(Tree::is_typing) {
            return Flow::Continue;
        }
        let at = Position::new(event.column, event.row);
        let panel = self.panel.filter(|panel| panel.contains(at));
        match event.kind {
            MouseEventKind::ScrollDown | MouseEventKind::ScrollUp => {
                let down = event.kind == MouseEventKind::ScrollDown;
                if let Some(top) = self.help_top {
                    self.help_top = Some(if down {
                        top.saturating_add(WHEEL_LINES)
                    } else {
                        top.saturating_sub(WHEEL_LINES)
                    });
                } else if let (Some(tree), Some(_)) = (&mut self.tree, panel) {
                    if down {
                        tree.move_down(WHEEL_LINES);
                    } else {
                        tree.move_up(WHEEL_LINES);
                    }
                } else if down {
                    self.scroll_to(self.top.saturating_add(WHEEL_LINES));
                } else {
                    self.scroll_to(self.top.saturating_sub(WHEEL_LINES));
                }
            }
            MouseEventKind::Down(MouseButton::Left) if self.help_top.is_none() => {
                self.click(panel, at, event.modifiers);
            }
            MouseEventKind::Drag(MouseButton::Left) if self.help_top.is_none() => {
                self.drag(event.row);
            }
            MouseEventKind::Up(MouseButton::Left) => self.release(),
            _ => {}
        }
        Flow::Continue
    }

    /// A press on the page waits for the release, because it may start a drag. A Ctrl-click on a
    /// link is the terminal's, which opens a web link itself.
    fn click(&mut self, panel: Option<Rect>, at: Position, modifiers: KeyModifiers) {
        self.leave_visual();
        self.press = None;
        let row = at.y;
        let Some(panel) = panel else {
            self.keys.focus = Focus::Page;
            let text = self.text_area;
            if !modifiers.contains(KeyModifiers::CONTROL) && (text.y..text.bottom()).contains(&row)
            {
                self.press = Some(Press {
                    line: (self.top + usize::from(row - text.y))
                        .min(self.document.len().saturating_sub(1)),
                    link: self
                        .link_areas
                        .iter()
                        .find(|link| link.area.contains(at))
                        .map(|link| link.id),
                    picture: self
                        .picture_areas
                        .iter()
                        .find(|(_, area)| area.contains(at))
                        .map(|(index, _)| *index),
                });
            }
            return;
        };
        if let Some(tree) = &mut self.tree {
            self.keys.focus = Focus::Tree;
            if let Some(file) = tree.click(panel, row) {
                self.read_from_tree(&file);
            }
            return;
        }
        let headings = self.document.headings();
        let current = self.document.heading_at(self.top);
        if let Some(line) = contents::entry_at(panel, headings.len(), current, row)
            .and_then(|entry| headings.get(entry))
            .map(|heading| heading.line)
        {
            self.scroll_to(line);
        }
    }

    /// Selects from the pressed line to the line at `row`, once the pointer has left the pressed
    /// line, so a click that shakes stays a click. Past the top or bottom of the page text, the
    /// page scrolls a line.
    fn drag(&mut self, row: u16) {
        let Some(press) = self.press else {
            return;
        };
        let text = self.text_area;
        if row < text.y {
            self.scroll_to(self.top.saturating_sub(1));
        } else if row >= text.bottom() {
            self.scroll_to(self.top + 1);
        }
        let row = row.clamp(text.y, text.bottom().saturating_sub(1));
        let cursor =
            (self.top + usize::from(row - text.y)).min(self.document.len().saturating_sub(1));
        if self.selection.is_none() && cursor == press.line {
            return;
        }
        self.selection = Some(Selection {
            anchor: press.line,
            cursor,
        });
        self.keys.focus = Focus::Visual;
    }

    /// Copies what a drag selected, or follows the link or opens the picture a click pressed.
    fn release(&mut self) {
        let Some(press) = self.press.take() else {
            return;
        };
        if let Some(selection) = self.selection {
            self.leave_visual();
            self.copy(selection);
        } else if let Some(link) = press.link {
            self.marked_link = Some(link);
            self.follow_marked_link();
        } else if let Some(picture) = press.picture {
            self.marked_picture = Some(picture);
            self.open_picture(picture);
        }
    }

    /// Reads a file the reader picked in the tree, and gives the page the keys to read it with. The
    /// tree stays open beside the page.
    fn read_from_tree(&mut self, file: &Path) {
        if !self
            .path
            .as_deref()
            .is_some_and(|path| same_file(file, path))
        {
            self.open(file);
        }
        self.keys.focus = Focus::Page;
    }

    /// Gives the tree the keys, opening it at the root of the page's file when it is closed.
    fn enter_tree(&mut self) {
        if self.tree.is_none() {
            let root = match (&self.tree_root, &self.path) {
                (Some(root), _) => root.clone(),
                (None, Some(path)) => root_of(path),
                (None, None) => PathBuf::from("."),
            };
            let mut tree = Tree::scan(&root);
            if let Some(path) = &self.path {
                tree.reveal(path);
            }
            self.tree = Some(tree);
        }
        self.keys.focus = Focus::Tree;
    }

    fn handle_tree_command(&mut self, command: Command, half_page: usize) -> Flow {
        let Some(tree) = &mut self.tree else {
            self.keys.focus = Focus::Page;
            return Flow::Continue;
        };
        match command {
            Command::Quit => return Flow::Quit,
            Command::Cancel => {
                if !tree.clear_filter() {
                    self.keys.focus = Focus::Page;
                }
            }
            Command::FilterTree => tree.start_filter(),
            Command::ToggleTree => {
                self.tree = None;
                self.keys.focus = Focus::Page;
            }
            Command::Down(times) => tree.move_down(times),
            Command::Up(times) => tree.move_up(times),
            Command::HalfPageDown(times) => tree.move_down(half_page.saturating_mul(times)),
            Command::HalfPageUp(times) => tree.move_up(half_page.saturating_mul(times)),
            Command::PageDown(times) => tree.move_down(self.page.saturating_mul(times)),
            Command::PageUp(times) => tree.move_up(self.page.saturating_mul(times)),
            Command::Top | Command::SourceLine(_) => tree.move_up(usize::MAX),
            Command::End => tree.move_down(usize::MAX),
            Command::CloseFolder => tree.close_folder(),
            Command::OpenEntry => {
                if let Some(file) = tree.open_entry() {
                    self.read_from_tree(&file);
                }
            }
            Command::ToggleHelp => self.help_top = Some(0),
            Command::Back => {
                self.step_history(|viewer| &mut viewer.back, |viewer| &mut viewer.forward);
            }
            Command::Forward => {
                self.step_history(|viewer| &mut viewer.forward, |viewer| &mut viewer.back);
            }
            _ => {}
        }
        Flow::Continue
    }

    /// Marks the link after the marked one, or before it when not `forward`. With no marked link
    /// on screen, it marks the first link from the top of the screen, or the last from its bottom.
    fn step_link(&mut self, forward: bool) {
        let links = self.document.links();
        let bottom = self.top + self.page;
        let on_screen = |line: usize| (self.top..bottom).contains(&line);
        let mut stops: Vec<(usize, usize)> = Vec::new();
        for link in links {
            if stops.last().is_none_or(|(url, _)| *url != link.url) {
                stops.push((link.url, link.line));
            }
        }
        let marked = self.marked_link.filter(|marked| {
            links
                .iter()
                .any(|link| link.url == *marked && on_screen(link.line))
        });
        let next = match (
            marked.and_then(|marked| stops.iter().position(|(url, _)| *url == marked)),
            forward,
        ) {
            (Some(at), true) => stops.get(at + 1).or(stops.get(at)),
            (Some(at), false) => stops.get(at.saturating_sub(1)),
            (None, true) => stops.iter().find(|(_, line)| *line >= self.top),
            (None, false) => stops.iter().rev().find(|(_, line)| *line < bottom),
        };
        let Some(&(url, line)) = next else {
            return;
        };
        self.marked_link = Some(url);
        self.marked_picture = None;
        if line < self.top {
            self.scroll_to(line);
        } else if line >= bottom {
            self.scroll_to(line + 1 - self.page);
        }
    }

    /// Marks the picture after the marked one, or before it when not `forward`, the way
    /// `step_link` marks links.
    fn step_picture(&mut self, forward: bool) {
        let pictures = self.document.pictures();
        let bottom = self.top + self.page;
        let marked = self.marked_picture.filter(|marked| {
            pictures.get(*marked).is_some_and(|picture| {
                picture.line < bottom && picture.line + usize::from(picture.placed.rows) > self.top
            })
        });
        let next = match (marked, forward) {
            (Some(at), true) => Some((at + 1).min(pictures.len() - 1)),
            (Some(at), false) => Some(at.saturating_sub(1)),
            (None, true) => pictures.iter().position(|picture| picture.line >= self.top),
            (None, false) => pictures.iter().rposition(|picture| picture.line < bottom),
        };
        let Some(index) = next else {
            return;
        };
        let line = pictures[index].line;
        self.marked_picture = Some(index);
        self.marked_link = None;
        if line < self.top {
            self.scroll_to(line);
        } else if line >= bottom {
            self.scroll_to(line + 1 - self.page);
        }
    }

    fn open_picture(&mut self, index: usize) {
        let Some(source) = self
            .document
            .pictures()
            .get(index)
            .map(|picture| picture.placed.source.clone())
        else {
            return;
        };
        match pixel_size(&source) {
            Ok(size) => {
                self.picture_view = Some(PictureView::new(source, size));
                self.keys.focus = Focus::Picture;
            }
            Err(reason) => self.show_error(&reason),
        }
    }

    fn handle_picture_command(&mut self, command: Command) -> Flow {
        let Some(view) = self.picture_view.as_mut() else {
            self.keys.focus = Focus::Page;
            return Flow::Continue;
        };
        match command {
            Command::Quit => return Flow::Quit,
            Command::ZoomIn(times) => view.zoom_in(times),
            Command::ZoomOut(times) => view.zoom_out(times),
            Command::ZoomFit => view.fit(),
            Command::Pan { across, down } => view.pan(across, down),
            Command::ClosePicture | Command::Cancel => self.close_picture(),
            _ => {}
        }
        Flow::Continue
    }

    fn close_picture(&mut self) {
        self.picture_view = None;
        self.pictures.close_view();
        self.keys.focus = Focus::Page;
    }

    /// Puts a marker in the margin beside the first row on screen of the marked picture, because
    /// a graphics protocol draws over the cells that a link mark would colour.
    fn mark_picture(&self, frame: &mut Frame, text: Rect) {
        let Some(picture) = self
            .marked_picture
            .and_then(|marked| self.document.pictures().get(marked))
        else {
            return;
        };
        let end = picture.line + usize::from(picture.placed.rows);
        let bottom = self.top + usize::from(text.height);
        if end <= self.top || picture.line >= bottom || text.x == 0 {
            return;
        }
        let (Ok(row), Ok(column)) = (
            u16::try_from(picture.line.max(self.top) - self.top),
            u16::try_from(picture.column),
        ) else {
            return;
        };
        let at = Rect {
            x: (text.x + column).saturating_sub(1),
            y: text.y + row,
            width: 1,
            height: 1,
        };
        frame.render_widget(
            Paragraph::new(Span::styled(PICTURE_MARK, theme::link_selected())),
            at,
        );
    }

    fn follow_marked_link(&mut self) {
        let Some(url) = self
            .marked_link
            .and_then(|marked| self.document.links().iter().find(|link| link.url == marked))
            .map(|link| self.document.url(link).to_string())
        else {
            return;
        };
        if let Some(anchor) = url.strip_prefix('#') {
            self.jump_to_anchor(anchor);
            return;
        }
        if let Some(scheme) = scheme_of(&url) {
            if !is_openable(&url) {
                self.show_error(&format!("will not open {scheme}: links"));
                return;
            }
            let result = match &mut self.opener.0 {
                Some(open) => open(&url),
                None => Err(String::from("no program to open it")),
            };
            if let Err(error) = result {
                self.show_error(&format!("cannot open {url}: {error}"));
            }
            return;
        }
        let (target, anchor) = url.split_once('#').unwrap_or((&url, ""));
        let folder = self.path.as_deref().map_or(Path::new("."), folder_of);
        let path = folder.join(target);
        let with_extension = path.with_extension("md");
        let file = [path, with_extension]
            .into_iter()
            .find(|file| file.is_file());
        let Some(file) = file else {
            self.show_error(&format!("not found: {target}"));
            return;
        };
        self.open(&file);
        if !anchor.is_empty() && self.error.is_none() {
            self.jump_to_anchor(anchor);
        }
    }

    /// Puts the heading whose anchor is `anchor` at the top, and drops the mark of the link that
    /// led there.
    fn jump_to_anchor(&mut self, anchor: &str) {
        let anchor = anchor.to_lowercase();
        let line = self
            .document
            .headings()
            .iter()
            .find(|heading| heading_anchor(&heading.text) == anchor)
            .map(|heading| heading.line);
        match line {
            Some(line) => {
                self.marked_link = None;
                self.scroll_to(line);
            }
            None => self.show_error(&format!("no heading #{anchor}")),
        }
    }

    /// Colours the rows of the marked link that are on screen.
    fn mark_link(&self, frame: &mut Frame, text: Rect) {
        let Some(marked) = self.marked_link else {
            return;
        };
        let rows = self.top..self.top + usize::from(text.height);
        for link in self
            .document
            .links()
            .iter()
            .filter(|link| link.url == marked && rows.contains(&link.line))
        {
            let (Ok(column), Ok(width), Ok(row)) = (
                u16::try_from(link.column),
                u16::try_from(link.width),
                u16::try_from(link.line - self.top),
            ) else {
                continue;
            };
            let area = Rect {
                x: text.x.saturating_add(column),
                y: text.y.saturating_add(row),
                width,
                height: 1,
            }
            .intersection(text);
            frame.buffer_mut().set_style(area, theme::link_selected());
        }
    }

    fn mode(&self) -> hint_bar::Mode {
        if self.prompt.is_some() {
            return hint_bar::Mode::TypingSearch;
        }
        if self.help_top.is_some() {
            return hint_bar::Mode::Help;
        }
        if self.keys.focus == Focus::Visual {
            return hint_bar::Mode::Visual;
        }
        match &self.tree {
            Some(tree) if tree.is_typing() => return hint_bar::Mode::TypingFilter,
            Some(_) if self.keys.focus == Focus::Tree => return hint_bar::Mode::Tree,
            _ => {}
        }
        hint_bar::Mode::Reading(hint_bar::Reading {
            headings_in_contents: self
                .contents_is_open()
                .then(|| self.document.headings().len()),
            search_active: self.search.is_some(),
            link_marked: self.marked_link.is_some(),
            picture_marked: self.marked_picture.is_some(),
            has_links: !self.document.links().is_empty(),
            has_pictures: !self.document.pictures().is_empty(),
            can_go_back: !self.back.is_empty(),
            has_folds: !self.document.folds().is_empty(),
        })
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
        let selection = self.selection.map(|selection| {
            (
                self.document.source_line_at(selection.anchor),
                self.document.source_line_at(selection.cursor),
            )
        });
        self.render(width);
        self.current_match = None;
        if self.width.is_some() {
            self.top = self.document.screen_line_of(anchor);
            self.selection = selection.map(|(anchor, cursor)| Selection {
                anchor: self.document.screen_line_of(anchor),
                cursor: self.document.screen_line_of(cursor),
            });
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
        Command::Quit | Command::ToggleHelp | Command::Cancel => None,
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

/// The wheel and the left button. Every other event, such as each move of the pointer, would only
/// cost a redraw.
fn is_used(kind: MouseEventKind) -> bool {
    matches!(
        kind,
        MouseEventKind::ScrollDown
            | MouseEventKind::ScrollUp
            | MouseEventKind::Down(MouseButton::Left)
            | MouseEventKind::Drag(MouseButton::Left)
            | MouseEventKind::Up(MouseButton::Left)
    )
}

/// The folder that relative paths in the file at `path` start from.
pub fn folder_of(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
}

fn same_file(one: &Path, other: &Path) -> bool {
    match (one.canonicalize(), other.canonicalize()) {
        (Ok(one), Ok(other)) => one == other,
        _ => one == other,
    }
}

type TakeText = Box<dyn FnMut(&str) -> Result<(), String>>;

/// A program outside the viewer that takes one text, such as an address to show or lines to copy,
/// none in a viewer built without one.
struct Handoff(Option<TakeText>);

impl fmt::Debug for Handoff {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(if self.0.is_some() {
            "Handoff"
        } else {
            "no Handoff"
        })
    }
}

/// The anchor GitHub gives a heading: lower case, spaces as hyphens, and punctuation left out.
fn heading_anchor(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .filter_map(|letter| match letter {
            ' ' | '-' => Some('-'),
            '_' => Some('_'),
            letter if letter.is_alphanumeric() => Some(letter),
            _ => None,
        })
        .collect()
}

/// The scheme of `url`, when it has one before any path separator, such as `https` or `file`.
fn scheme_of(url: &str) -> Option<&str> {
    let (scheme, _) = url.split_once(':')?;
    (!scheme.is_empty()
        && scheme
            .chars()
            .all(|letter| letter.is_ascii_alphanumeric() || "+.-".contains(letter)))
    .then_some(scheme)
}
