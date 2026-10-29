use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use unicode_width::UnicodeWidthStr;

use crate::contents::fit;
use crate::theme::Palette;

const MARKER: &str = "◀";
const INDENT: usize = 2;
const MARKDOWN_EXTENSIONS: [&str; 2] = ["md", "markdown"];

/// A folder or a Markdown file, with how deep it sits under the root.
#[derive(Debug)]
struct Entry {
    path: PathBuf,
    depth: usize,
    folder: bool,
}

/// The Markdown files under a root folder, browsed as folders that open and close.
#[derive(Debug)]
pub struct Tree {
    root: PathBuf,
    /// Every entry, each folder before what it holds, folders before files, by name.
    entries: Vec<Entry>,
    open_folders: HashSet<PathBuf>,
    /// The row the cursor is on, counted in the rows on view.
    cursor: usize,
    /// The first row the panel shows.
    top: usize,
    /// Words that every file on view holds in its path, in any case.
    filter: String,
    /// Whether the keys type into the filter.
    typing: bool,
}

/// The folder a tree for the file at `path` starts from: the repository that holds it, or its own
/// folder outside a repository.
pub fn root_of(path: &Path) -> PathBuf {
    let folder = crate::folder_of(path);
    let folder = folder
        .canonicalize()
        .unwrap_or_else(|_| folder.to_path_buf());
    folder
        .ancestors()
        .find(|ancestor| ancestor.join(".git").exists())
        .unwrap_or(&folder)
        .to_path_buf()
}

#[derive(Default)]
struct Folder {
    folders: BTreeMap<OsString, Folder>,
    files: BTreeSet<OsString>,
}

impl Folder {
    fn add(&mut self, relative: &Path) {
        let mut folder = self;
        let mut parts = relative.iter().peekable();
        while let Some(part) = parts.next() {
            if parts.peek().is_none() {
                folder.files.insert(part.to_owned());
            } else {
                folder = folder.folders.entry(part.to_owned()).or_default();
            }
        }
    }

    fn flatten(&self, path: &Path, depth: usize, entries: &mut Vec<Entry>) {
        for (name, folder) in &self.folders {
            let path = path.join(name);
            entries.push(Entry {
                path: path.clone(),
                depth,
                folder: true,
            });
            folder.flatten(&path, depth + 1, entries);
        }
        entries.extend(self.files.iter().map(|name| Entry {
            path: path.join(name),
            depth,
            folder: false,
        }));
    }
}

impl Tree {
    /// Lists the Markdown files under `root` that the repository does not ignore.
    pub fn scan(root: &Path) -> Self {
        let root = &root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        let mut top = Folder::default();
        for found in ignore::WalkBuilder::new(root).build().flatten() {
            let is_markdown = found.file_type().is_some_and(|kind| kind.is_file())
                && found.path().extension().is_some_and(|extension| {
                    MARKDOWN_EXTENSIONS
                        .iter()
                        .any(|markdown| extension.eq_ignore_ascii_case(markdown))
                });
            if let (true, Ok(relative)) = (is_markdown, found.path().strip_prefix(root)) {
                top.add(relative);
            }
        }
        let mut entries = Vec::new();
        top.flatten(root, 0, &mut entries);
        Self {
            root: root.to_path_buf(),
            entries,
            open_folders: HashSet::new(),
            cursor: 0,
            top: 0,
            filter: String::new(),
            typing: false,
        }
    }

    pub fn is_typing(&self) -> bool {
        self.typing
    }

    /// Starts typing a new filter.
    pub fn start_filter(&mut self) {
        self.typing = true;
        self.set_filter(String::new());
    }

    /// Clears the filter, and answers whether there was one to clear.
    pub fn clear_filter(&mut self) -> bool {
        let had_one = !self.filter.is_empty();
        self.typing = false;
        self.set_filter(String::new());
        had_one
    }

    /// Types `key` into the filter: Enter keeps it, Escape clears it.
    pub fn edit_filter(&mut self, key: KeyEvent) {
        let control = key.modifiers.contains(KeyModifiers::CONTROL);
        let mut filter = self.filter.clone();
        match key.code {
            KeyCode::Enter => self.typing = false,
            KeyCode::Esc => {
                self.clear_filter();
            }
            KeyCode::Char('c') if control => {
                self.clear_filter();
            }
            KeyCode::Backspace => {
                filter.pop();
                self.set_filter(filter);
            }
            KeyCode::Char(typed) if !control => {
                filter.push(typed);
                self.set_filter(filter);
            }
            _ => {}
        }
    }

    /// Filters the tree and puts the cursor on the first file left.
    fn set_filter(&mut self, filter: String) {
        self.filter = filter;
        self.top = 0;
        if self.filter.trim().is_empty() {
            return;
        }
        let rows = self.rows();
        self.cursor = rows
            .iter()
            .position(|&index| !self.entries[index].folder)
            .unwrap_or(0);
    }

    /// The files whose path under the root holds every word of the filter, and their folders.
    fn filtered(&self) -> Option<HashSet<&Path>> {
        let words: Vec<String> = self
            .filter
            .split_whitespace()
            .map(str::to_lowercase)
            .collect();
        if words.is_empty() {
            return None;
        }
        let mut kept = HashSet::new();
        for entry in self.entries.iter().filter(|entry| !entry.folder) {
            let relative = entry.path.strip_prefix(&self.root).unwrap_or(&entry.path);
            let relative = relative.to_string_lossy().to_lowercase();
            if words.iter().all(|word| relative.contains(word.as_str())) {
                kept.extend(
                    entry
                        .path
                        .ancestors()
                        .take_while(|ancestor| *ancestor != self.root),
                );
            }
        }
        Some(kept)
    }

    /// Opens the folders down to `file` and puts the cursor on it, when the tree holds it.
    pub fn reveal(&mut self, file: &Path) {
        let file = file.canonicalize().unwrap_or_else(|_| file.to_path_buf());
        let Ok(relative) = file.strip_prefix(&self.root) else {
            return;
        };
        let mut folder = self.root.clone();
        for part in relative.parent().into_iter().flat_map(Path::iter) {
            folder.push(part);
            self.open_folders.insert(folder.clone());
        }
        if let Some(row) = self
            .rows()
            .iter()
            .position(|&index| self.entries[index].path == file)
        {
            self.cursor = row;
        }
    }

    /// The entries on view: those the filter keeps, or with no filter those whose folders are
    /// all open.
    fn rows(&self) -> Vec<usize> {
        if let Some(kept) = self.filtered() {
            return (0..self.entries.len())
                .filter(|&index| kept.contains(self.entries[index].path.as_path()))
                .collect();
        }
        let mut rows = Vec::new();
        let mut closed_below: Option<usize> = None;
        for (index, entry) in self.entries.iter().enumerate() {
            if closed_below.is_some_and(|depth| entry.depth > depth) {
                continue;
            }
            closed_below = None;
            rows.push(index);
            if entry.folder && !self.open_folders.contains(&entry.path) {
                closed_below = Some(entry.depth);
            }
        }
        rows
    }

    fn under_cursor(&self) -> Option<&Entry> {
        self.rows()
            .get(self.cursor)
            .map(|&index| &self.entries[index])
    }

    pub fn move_down(&mut self, rows: usize) {
        let last = self.rows().len().saturating_sub(1);
        self.cursor = self.cursor.saturating_add(rows).min(last);
    }

    pub fn move_up(&mut self, rows: usize) {
        self.cursor = self.cursor.saturating_sub(rows);
    }

    /// Opens the folder under the cursor, or answers the file under it for the page to read.
    pub fn open_entry(&mut self) -> Option<PathBuf> {
        let entry = self.under_cursor()?;
        if entry.folder {
            let folder = entry.path.clone();
            self.open_folders.insert(folder);
            None
        } else {
            Some(entry.path.clone())
        }
    }

    /// Closes the open folder under the cursor, or moves the cursor up to the folder that holds
    /// the entry under it.
    pub fn close_folder(&mut self) {
        let Some(entry) = self.under_cursor() else {
            return;
        };
        if entry.folder && self.open_folders.contains(&entry.path) {
            let folder = entry.path.clone();
            self.open_folders.remove(&folder);
            return;
        }
        let Some(parent) = entry.path.parent().map(Path::to_path_buf) else {
            return;
        };
        if let Some(row) = self
            .rows()
            .iter()
            .position(|&index| self.entries[index].path == parent)
        {
            self.cursor = row;
        }
    }

    /// Acts on the row drawn on screen row `y` of a tree drawn in `area`: puts the cursor on it,
    /// opens or closes a folder, and answers a file for the page to read.
    pub fn click(&mut self, area: Rect, y: u16) -> Option<PathBuf> {
        let inner = Block::new().borders(Borders::ALL).inner(area);
        let row = self.top + usize::from(y.checked_sub(inner.y)?);
        let entry = &self.entries[*self.rows().get(row)?];
        let (path, folder) = (entry.path.clone(), entry.folder);
        self.cursor = row;
        if !folder {
            return Some(path);
        }
        if !self.open_folders.remove(&path) {
            self.open_folders.insert(path);
        }
        None
    }

    /// Draws the rows on view in `area`, marking `current`, the file on the page, and showing the
    /// cursor while the tree has the keys.
    pub fn draw(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        current: Option<&Path>,
        focused: bool,
        palette: &Palette,
    ) {
        let name = self.root.file_name().map_or_else(
            || self.root.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
        let block = Block::new()
            .borders(Borders::ALL)
            .border_style(palette.panel_border())
            .title(Span::styled(
                if self.typing || !self.filter.is_empty() {
                    format!(" {name} /{} ", self.filter)
                } else {
                    format!(" {name} ")
                },
                palette.panel_title(),
            ));
        let inner = block.inner(area);
        frame.render_widget(Clear, area);
        frame.render_widget(block.style(palette.page()), area);

        let rows = self.rows();
        let height = usize::from(inner.height);
        if self.cursor < self.top {
            self.top = self.cursor;
        } else if height > 0 && self.cursor >= self.top + height {
            self.top = self.cursor + 1 - height;
        }
        let current =
            current.map(|path| path.canonicalize().unwrap_or_else(|_| path.to_path_buf()));
        let text_width = usize::from(inner.width).saturating_sub(MARKER.width());
        let lines: Vec<Line> = rows
            .iter()
            .enumerate()
            .skip(self.top)
            .take(height)
            .map(|(row, &index)| {
                let entry = &self.entries[index];
                let open =
                    !self.filter.trim().is_empty() || self.open_folders.contains(&entry.path);
                let lead = " ".repeat(INDENT * entry.depth)
                    + match (entry.folder, open) {
                        (true, true) => "▾ ",
                        (true, false) => "▸ ",
                        (false, _) => "  ",
                    };
                let name = entry
                    .path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let text = fit(&name, text_width.saturating_sub(lead.width()));
                let pad = " ".repeat(text_width.saturating_sub(lead.width() + text.width()));
                let is_current = current.as_ref() == Some(&entry.path);
                let mut style = match (entry.folder, is_current) {
                    (true, _) => palette.tree_folder(),
                    (false, true) => palette.contents_marked(),
                    (false, false) => palette.contents_entry(1),
                };
                let mut lead_style = palette.tree_folder();
                if focused && row == self.cursor {
                    style = style.patch(palette.tree_cursor());
                    lead_style = lead_style.patch(palette.tree_cursor());
                }
                let marker = if is_current { MARKER } else { " " };
                Line::from(vec![
                    Span::styled(lead, lead_style),
                    Span::styled(text + &pad, style),
                    Span::styled(marker, palette.contents_marked()),
                ])
            })
            .collect();
        frame.render_widget(Paragraph::new(lines), inner);
    }
}
