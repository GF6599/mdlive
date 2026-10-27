use std::collections::HashMap;
use std::ops::Range;

use pulldown_cmark::{
    Alignment, BlockQuoteKind, CodeBlockKind, Event, HeadingLevel, MetadataBlockKind, Options,
    Parser, Tag, TagEnd,
};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use crate::diagrams::{self, Diagrams};
use crate::folds::{DetailsTag, Folds, details_tags};
use crate::front_matter;
use crate::highlight::highlight;
use crate::math::to_unicode;
use crate::pictures::{Pictures, Placed};
use crate::table::{self, Cell};
use crate::theme;
use crate::wrap::{BREAK, Run, hard_wrap, wrap, wrap_runs};

const BULLET: &str = "• ";
const QUOTE_BAR: &str = "▎ ";
const TAB: &str = "    ";
const OPEN_TASK: &str = "☐ ";
const DONE_TASK: &str = "☑ ";
const FOLD_CLOSED: &str = "▸ ";
const FOLD_OPEN: &str = "▾ ";
const PICTURE_MARKER: &str = "▣ ";
const SUPERSCRIPT_DIGITS: [char; 10] = ['⁰', '¹', '²', '³', '⁴', '⁵', '⁶', '⁷', '⁸', '⁹'];

/// A Markdown source laid out for one text width.
#[derive(Debug, Default)]
pub struct Document {
    lines: Vec<Line<'static>>,
    /// The source line each screen line came from. A blank line between blocks takes the
    /// source line of the block above it, so a position never anchors to a gap.
    sources: Vec<usize>,
    headings: Vec<Heading>,
    /// The first screen line after each blank line between blocks, and the first line of the page.
    /// A tight list has no blank line between its items, so it is one block, as in vim.
    blocks: Vec<usize>,
    folds: Vec<Fold>,
    links: Vec<Link>,
    urls: Vec<String>,
    pictures: Vec<Picture>,
}

/// A picture on the page, in rows of its own from `line`.
#[derive(Debug)]
pub struct Picture {
    pub line: usize,
    pub column: usize,
    pub placed: Placed,
}

/// The text of one link on one screen line.
#[derive(Debug)]
pub struct Link {
    pub line: usize,
    pub column: usize,
    pub width: usize,
    /// Each link has its own index, so the rows of a wrapped link share it and two links to the
    /// same address do not.
    pub url: usize,
}

/// The summary line of a details block.
#[derive(Debug)]
pub struct Fold {
    pub line: usize,
    pub summary: String,
    pub occurrence: usize,
    pub open: bool,
}

#[derive(Debug)]
pub struct Heading {
    pub line: usize,
    pub level: u8,
    pub text: String,
}

/// A place in a document that survives an edit: a heading and the lines below it, with the source
/// line as the fallback when the heading is gone.
#[derive(Debug)]
pub struct Anchor {
    heading: Option<(String, usize, usize)>,
    source_line: usize,
}

impl Document {
    pub fn render(
        markdown: &str,
        width: usize,
        folds: &Folds,
        pictures: &Pictures,
        diagrams: &Diagrams,
    ) -> Self {
        Builder::new(markdown, width.max(1), folds.clone(), pictures, diagrams).build(markdown)
    }

    pub fn pictures(&self) -> &[Picture] {
        &self.pictures
    }

    pub fn folds(&self) -> &[Fold] {
        &self.folds
    }

    pub fn links(&self) -> &[Link] {
        &self.links
    }

    pub fn url(&self, link: &Link) -> &str {
        self.urls.get(link.url).map_or("", String::as_str)
    }

    pub fn lines(&self) -> &[Line<'static>] {
        &self.lines
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    pub fn headings(&self) -> &[Heading] {
        &self.headings
    }

    /// The index of the heading whose section holds the screen line `top`.
    pub fn heading_at(&self, top: usize) -> Option<usize> {
        self.headings
            .iter()
            .rposition(|heading| heading.line <= top)
    }

    pub fn source_line_at(&self, screen_line: usize) -> usize {
        self.sources
            .get(screen_line)
            .or(self.sources.last())
            .copied()
            .unwrap_or(0)
    }

    /// Where the screen line `top` sits: under which heading, the how-manyth with that text, and
    /// how far below it.
    pub fn anchor(&self, top: usize) -> Anchor {
        let heading = self.heading_at(top).map(|index| {
            let heading = &self.headings[index];
            let occurrence = self.headings[..index]
                .iter()
                .filter(|earlier| earlier.text == heading.text)
                .count();
            (heading.text.clone(), occurrence, top - heading.line)
        });
        Anchor {
            heading,
            source_line: self.source_line_at(top),
        }
    }

    pub fn screen_line_at(&self, anchor: &Anchor) -> usize {
        anchor
            .heading
            .as_ref()
            .and_then(|(text, occurrence, offset)| {
                self.headings
                    .iter()
                    .filter(|heading| heading.text == *text)
                    .nth(*occurrence)
                    .map(|heading| heading.line + offset)
            })
            .unwrap_or_else(|| self.screen_line_of(anchor.source_line))
    }

    pub fn next_heading(&self, after: usize) -> Option<usize> {
        next(self.headings.iter().map(|heading| heading.line), after)
    }

    pub fn previous_heading(&self, before: usize) -> Option<usize> {
        previous(self.headings.iter().map(|heading| heading.line), before)
    }

    pub fn next_block(&self, after: usize) -> Option<usize> {
        next(self.blocks.iter().copied(), after)
    }

    pub fn previous_block(&self, before: usize) -> Option<usize> {
        previous(self.blocks.iter().copied(), before)
    }

    /// The first screen line of the block that holds `source_line`.
    pub fn screen_line_of(&self, source_line: usize) -> usize {
        let after = self.sources.partition_point(|&line| line <= source_line);
        let block = self.sources[..after].last().copied().unwrap_or(0);
        self.sources.partition_point(|&line| line < block)
    }
}

fn next(mut lines: impl Iterator<Item = usize>, after: usize) -> Option<usize> {
    lines.find(|&line| line > after)
}

fn previous(lines: impl Iterator<Item = usize>, before: usize) -> Option<usize> {
    lines.take_while(|&line| line < before).last()
}

#[derive(Debug)]
enum Container {
    Quote(Option<BlockQuoteKind>),
    Item { marker: String, shown: bool },
}

enum Block {
    Text {
        spans: Vec<Run>,
        base: Style,
    },
    Heading {
        spans: Vec<Run>,
        level: u8,
    },
    Code {
        text: String,
        language: String,
    },
    Raw {
        text: String,
    },
    FrontMatter {
        text: String,
    },
    Table {
        alignments: Vec<Alignment>,
        rows: Vec<Vec<Cell>>,
    },
}

type Footnote = (String, Vec<(Event<'static>, Range<usize>)>);

struct Builder<'a> {
    width: usize,
    pictures: &'a Pictures,
    diagrams: &'a Diagrams,
    /// The address of the picture the parser is inside, and whether it opened its paragraph.
    image: Option<(String, bool)>,
    /// A picture that is the whole of its paragraph so far, with its alt text.
    lone_picture: Option<(String, String)>,
    line_starts: Vec<usize>,
    document: Document,
    containers: Vec<Container>,
    lists: Vec<Option<u64>>,
    styles: Vec<Style>,
    /// The link the parser is inside, when its address is one a terminal can open.
    link: Option<usize>,
    block: Option<(Block, usize)>,
    gap: bool,
    starts_block: bool,
    /// Footnote labels in the order the text first refers to them, which gives their numbers.
    footnote_order: Vec<String>,
    /// Each footnote's events, held back so the notes can be drawn at the end of the page.
    footnotes: Vec<Footnote>,
    fold_states: Folds,
    /// Whether each details block the parser is inside is open.
    open_folds: Vec<bool>,
    summaries_seen: HashMap<String, usize>,
}

impl<'a> Builder<'a> {
    fn new(
        markdown: &str,
        width: usize,
        fold_states: Folds,
        pictures: &'a Pictures,
        diagrams: &'a Diagrams,
    ) -> Self {
        let line_starts = std::iter::once(0)
            .chain(markdown.match_indices('\n').map(|(at, _)| at + 1))
            .collect();
        Self {
            width,
            pictures,
            diagrams,
            image: None,
            lone_picture: None,
            line_starts,
            document: Document::default(),
            containers: Vec::new(),
            lists: Vec::new(),
            styles: Vec::new(),
            link: None,
            block: None,
            gap: false,
            starts_block: true,
            footnote_order: Vec::new(),
            footnotes: Vec::new(),
            fold_states,
            open_folds: Vec::new(),
            summaries_seen: HashMap::new(),
        }
    }

    fn build(mut self, markdown: &str) -> Document {
        let options = Options::ENABLE_STRIKETHROUGH
            | Options::ENABLE_TABLES
            | Options::ENABLE_TASKLISTS
            | Options::ENABLE_FOOTNOTES
            | Options::ENABLE_GFM
            | Options::ENABLE_MATH
            | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS;
        let mut footnote: Option<Footnote> = None;
        for (event, range) in Parser::new_ext(markdown, options).into_offset_iter() {
            let event = match event {
                // pulldown-cmark closes a formula at a dollar sign before a digit, which reads two
                // prices in a sentence as math. VS Code's preview refuses that close, and so do we.
                Event::InlineMath(_) if closes_before_digit(markdown, &range) => {
                    Event::Text(markdown[range.clone()].to_string().into())
                }
                // Cost ratings such as $$$$ parse as an empty formula.
                Event::InlineMath(latex) | Event::DisplayMath(latex) if latex.trim().is_empty() => {
                    Event::Text(markdown[range.clone()].to_string().into())
                }
                event => event,
            };
            match (event, &mut footnote) {
                (Event::Start(Tag::FootnoteDefinition(label)), _) => {
                    footnote = Some((label.to_string(), Vec::new()));
                }
                (Event::End(TagEnd::FootnoteDefinition), _) => {
                    self.footnotes.extend(footnote.take());
                }
                (event, Some((_, events))) => events.push((event.into_static(), range)),
                (event, None) => self.event(event, range),
            }
        }
        self.finish_block();
        self.open_folds.clear();
        self.push_footnotes();
        self.document
    }

    fn footnote_number(&mut self, label: &str) -> usize {
        let index = self
            .footnote_order
            .iter()
            .position(|known| known == label)
            .unwrap_or_else(|| {
                self.footnote_order.push(label.to_string());
                self.footnote_order.len() - 1
            });
        index + 1
    }

    /// Draws the notes under a rule, numbered in reading order. A note the text never refers to
    /// still shows, after the others.
    fn push_footnotes(&mut self) {
        if self.footnotes.is_empty() {
            return;
        }
        let mut notes = std::mem::take(&mut self.footnotes);
        notes.sort_by_key(|(label, _)| {
            self.footnote_order
                .iter()
                .position(|known| known == label)
                .unwrap_or(usize::MAX)
        });
        self.event(Event::Rule, 0..0);
        for (label, events) in notes {
            let number = self.footnote_number(&label);
            self.containers.push(Container::Item {
                marker: format!("{number}. "),
                shown: false,
            });
            for (event, range) in events {
                self.event(event, range);
            }
            self.finish_block();
            self.containers.pop();
        }
    }

    fn source_line(&self, offset: usize) -> usize {
        self.line_starts
            .partition_point(|&start| start <= offset)
            .saturating_sub(1)
    }

    fn event(&mut self, event: Event<'_>, range: Range<usize>) {
        match event {
            Event::Start(tag) => self.start(tag, range),
            Event::End(tag) => self.end(tag),
            Event::Text(text) => self.text(&text, range),
            Event::Code(code) => {
                self.inline(&code, range, theme::inline_code());
            }
            Event::InlineHtml(html) if !is_comment(&html) => {
                self.inline(&html, range, theme::html());
            }
            Event::Html(html) => {
                if !matches!(self.block, Some((Block::Raw { .. }, _))) {
                    self.open(
                        Block::Raw {
                            text: String::new(),
                        },
                        range.start,
                    );
                }
                if let Some((Block::Raw { text }, _)) = &mut self.block {
                    text.push_str(&html);
                }
            }
            Event::TaskListMarker(done) => {
                if let Some(Container::Item { marker, .. }) = self.containers.last_mut() {
                    *marker = (if done { DONE_TASK } else { OPEN_TASK }).to_string();
                }
            }
            Event::InlineMath(latex) => {
                self.inline(&to_unicode(&latex), range, theme::math());
            }
            Event::DisplayMath(latex)
                if matches!(
                    self.block,
                    Some((Block::Table { .. } | Block::Heading { .. }, _))
                ) =>
            {
                self.inline(&to_unicode(&latex), range, theme::math());
            }
            Event::DisplayMath(latex) => self.push_display_math(&latex, range.start),
            Event::FootnoteReference(label) => {
                let number = self.footnote_number(&label);
                self.inline(&superscript(number), range, theme::link());
            }
            Event::SoftBreak => self.inline(" ", range, Style::new()),
            Event::HardBreak => self.inline(BREAK, range, Style::new()),
            Event::Rule => {
                self.finish_block();
                self.blank_before_block();
                let rule = "─".repeat(self.content_width());
                let line = self.source_line(range.start);
                self.push_line(vec![Span::styled(rule, theme::rule())], true, line);
                self.gap = true;
            }
            _ => {}
        }
    }

    fn start(&mut self, tag: Tag<'_>, range: Range<usize>) {
        match tag {
            Tag::Paragraph => self.open(
                Block::Text {
                    spans: Vec::new(),
                    base: self.base(),
                },
                range.start,
            ),
            Tag::Heading { level, .. } => {
                let level = heading_level(level);
                self.open(
                    Block::Heading {
                        spans: Vec::new(),
                        level,
                    },
                    range.start,
                );
            }
            Tag::CodeBlock(kind) => {
                let language = match kind {
                    CodeBlockKind::Fenced(info) => fence_language(&info).to_string(),
                    CodeBlockKind::Indented => String::new(),
                };
                self.open(
                    Block::Code {
                        text: String::new(),
                        language,
                    },
                    range.start,
                );
            }
            Tag::BlockQuote(kind) => {
                self.finish_block();
                self.blank_before_block();
                self.containers.push(Container::Quote(kind));
                if let Some(kind) = kind {
                    let label = Span::styled(alert_label(kind), theme::alert_label(kind));
                    let line = self.source_line(range.start);
                    self.push_line(vec![label], true, line);
                }
            }
            Tag::List(start) => {
                self.finish_block();
                self.lists.push(start);
            }
            Tag::Item => {
                self.finish_block();
                let marker = match self.lists.last_mut() {
                    Some(Some(number)) => {
                        let marker = format!("{number}. ");
                        *number += 1;
                        marker
                    }
                    _ => BULLET.to_string(),
                };
                self.containers.push(Container::Item {
                    marker,
                    shown: false,
                });
            }
            Tag::MetadataBlock(MetadataBlockKind::YamlStyle) => self.open(
                Block::FrontMatter {
                    text: String::new(),
                },
                range.start,
            ),
            Tag::Table(alignments) => self.open(
                Block::Table {
                    alignments,
                    rows: Vec::new(),
                },
                range.start,
            ),
            Tag::TableHead | Tag::TableRow => {
                if let Some((Block::Table { rows, .. }, _)) = &mut self.block {
                    rows.push(Vec::new());
                }
            }
            Tag::TableCell => {
                if let Some((Block::Table { rows, .. }, _)) = &mut self.block
                    && let Some(row) = rows.last_mut()
                {
                    row.push(Vec::new());
                }
            }
            Tag::Emphasis => self.styles.push(theme::emphasis()),
            Tag::Strong => self.styles.push(theme::strong()),
            Tag::Strikethrough => self.styles.push(theme::strikethrough()),
            Tag::Image { dest_url, .. } => {
                let opens_paragraph = match &self.block {
                    None => true,
                    Some((Block::Text { spans, .. }, _)) => spans.is_empty(),
                    Some(_) => false,
                };
                self.image = Some((dest_url.to_string(), opens_paragraph));
            }
            Tag::Link { dest_url, .. } => {
                self.styles.push(theme::link());
                self.document.urls.push(dest_url.to_string());
                self.link = Some(self.document.urls.len() - 1);
            }
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph
            | TagEnd::Heading(_)
            | TagEnd::CodeBlock
            | TagEnd::HtmlBlock
            | TagEnd::MetadataBlock(_)
            | TagEnd::Table => {
                self.finish_block();
                self.gap = true;
            }
            TagEnd::BlockQuote(_) => {
                self.finish_block();
                self.containers.pop();
                self.gap = true;
            }
            TagEnd::List(_) => {
                self.finish_block();
                self.lists.pop();
                if !self.in_item() {
                    self.gap = true;
                }
            }
            TagEnd::Item => {
                self.finish_block();
                self.containers.pop();
            }
            TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough => {
                self.styles.pop();
            }
            TagEnd::Link => {
                self.styles.pop();
                self.link = None;
            }
            TagEnd::Image => {
                if let Some((url, true)) = self.image.take()
                    && let Some((Block::Text { spans, .. }, _)) = &self.block
                {
                    let alt = spans
                        .iter()
                        .map(|(span, _)| span.content.as_ref())
                        .collect();
                    self.lone_picture = Some((url, alt));
                }
            }
            _ => {}
        }
    }

    fn in_item(&self) -> bool {
        self.containers
            .iter()
            .any(|container| matches!(container, Container::Item { .. }))
    }

    fn base(&self) -> Style {
        if self
            .containers
            .iter()
            .any(|container| matches!(container, Container::Quote(None)))
        {
            theme::quote()
        } else {
            Style::new()
        }
    }

    fn style(&self) -> Style {
        self.styles
            .iter()
            .fold(Style::new(), |style, next| style.patch(*next))
    }

    fn text(&mut self, text: &str, range: Range<usize>) {
        match &mut self.block {
            Some((Block::Code { text: code, .. }, _)) => code.push_str(text),
            Some((Block::Raw { text: raw } | Block::FrontMatter { text: raw }, _)) => {
                raw.push_str(text);
            }
            _ => self.inline(text, range, Style::new()),
        }
    }

    /// Adds inline text, opening a paragraph for the text of a tight list item.
    fn inline(&mut self, text: &str, range: Range<usize>, style: Style) {
        if self.image.is_none() {
            self.lone_picture = None;
        }
        if self.block.is_none() {
            self.open(
                Block::Text {
                    spans: Vec::new(),
                    base: self.base(),
                },
                range.start,
            );
        }
        let style = self.style().patch(style);
        let link = self.link;
        match &mut self.block {
            Some((Block::Text { spans, base }, _)) => {
                spans.push((Span::styled(text.to_string(), base.patch(style)), link));
            }
            Some((Block::Heading { spans, level }, _)) => {
                spans.push((
                    Span::styled(text.to_string(), theme::heading(*level).patch(style)),
                    link,
                ));
            }
            Some((Block::Table { rows, .. }, _)) => {
                if let Some(cell) = rows.last_mut().and_then(|row| row.last_mut()) {
                    cell.push((Span::styled(text.to_string(), style), link));
                }
            }
            _ => {}
        }
    }

    fn open(&mut self, block: Block, offset: usize) {
        self.finish_block();
        self.block = Some((block, self.source_line(offset)));
    }

    fn finish_block(&mut self) {
        let lone_picture = self.lone_picture.take();
        let Some((block, line)) = self.block.take() else {
            return;
        };
        if let Block::Raw { text } = &block
            && is_comment(text)
        {
            return;
        }
        if let Block::Raw { text } = &block
            && let Some(tags) = details_tags(text)
        {
            self.fold(tags, line);
            return;
        }
        if self.hidden() {
            return;
        }
        self.blank_before_block();
        let width = self.content_width();
        match block {
            Block::Text { base, .. } if let Some((url, alt)) = lone_picture => {
                self.push_picture(&url, &alt, base, width, line);
            }
            Block::Text { spans, .. } => self.push_wrapped(&spans, width, line),
            Block::Heading { spans, level } => {
                self.document.headings.push(Heading {
                    line: self.document.len(),
                    level,
                    text: spans
                        .iter()
                        .map(|(span, _)| span.content.as_ref())
                        .collect(),
                });
                self.push_wrapped(&spans, width, line);
                if let Some((glyph, style)) = theme::heading_rule(level) {
                    self.push_line(vec![Span::styled(glyph.repeat(width), style)], false, line);
                }
            }
            Block::Code { text, language } if language == "mermaid" => {
                self.push_diagram(&text, width, line);
            }
            Block::Code { text, language } => self.push_code(&text, &language, width, line),
            Block::Table { alignments, rows } => {
                for (index, row) in table::lay_out(&rows, &alignments, width)
                    .into_iter()
                    .enumerate()
                {
                    self.push_runs(row, index == 0, line);
                }
            }
            Block::Raw { text } => self.push_source(&text, width, line),
            Block::FrontMatter { text } => match front_matter::parse(&text) {
                Some(pairs) => self.push_front_matter(&pairs, width, line),
                None => self.push_source(&format!("---\n{text}---"), width, line),
            },
        }
    }

    fn hidden(&self) -> bool {
        self.open_folds.iter().any(|open| !open)
    }

    /// Shows the summary line of each details block that starts, unless an enclosing fold is
    /// closed, and hides what follows until the block ends when the fold is closed.
    fn fold(&mut self, tags: Vec<DetailsTag>, line: usize) {
        for tag in tags {
            match tag {
                DetailsTag::Start { open, summary } => {
                    let seen = self.summaries_seen.entry(summary.clone()).or_default();
                    let occurrence = *seen;
                    *seen += 1;
                    let open = self.fold_states.is_open(&summary, occurrence, open);
                    if !self.hidden() {
                        self.blank_before_block();
                        let marker = if open { FOLD_OPEN } else { FOLD_CLOSED };
                        self.document.folds.push(Fold {
                            line: self.document.len(),
                            summary: summary.clone(),
                            occurrence,
                            open,
                        });
                        let spans = vec![
                            Span::styled(marker, theme::marker()),
                            Span::styled(summary, theme::strong()),
                        ];
                        self.push_line(spans, true, line);
                        self.gap = true;
                    }
                    self.open_folds.push(open);
                }
                DetailsTag::End => {
                    self.open_folds.pop();
                }
            }
        }
    }

    fn push_source(&mut self, text: &str, width: usize, line: usize) {
        let mut first = true;
        for row in text.trim_end_matches('\n').lines() {
            for part in hard_wrap(&[Span::styled(row.to_string(), theme::html())], width) {
                self.push_line(part, first, line);
                first = false;
            }
        }
    }

    /// Keys in one column and values beside them, a long value wrapping under itself.
    fn push_front_matter(&mut self, pairs: &[(String, String)], width: usize, line: usize) {
        let key_width = pairs.iter().map(|(key, _)| key.width()).max().unwrap_or(0) + 2;
        let value_width = width.saturating_sub(key_width).max(1);
        for (key, value) in pairs {
            let value = [Span::styled(value.clone(), theme::front_matter_value())];
            for (index, row) in wrap(&value, value_width).into_iter().enumerate() {
                let lead = if index == 0 { key.as_str() } else { "" };
                let pad = " ".repeat(key_width - lead.width());
                let mut spans = vec![Span::styled(
                    format!("{lead}{pad}"),
                    theme::front_matter_key(),
                )];
                spans.extend(row);
                self.push_line(spans, index == 0, line);
            }
        }
    }

    /// A display formula breaks out of its paragraph onto centred lines of its own, and the text
    /// after it continues as a new block.
    fn push_display_math(&mut self, latex: &str, offset: usize) {
        if let Some((Block::Text { spans, .. }, _)) = &self.block
            && spans.is_empty()
        {
            self.block = None;
        }
        if self.block.is_some() {
            self.finish_block();
            self.gap = true;
        }
        self.blank_before_block();
        let width = self.content_width();
        let line = self.source_line(offset);
        let text = to_unicode(latex);
        let mut first = true;
        for row in text.lines() {
            for part in wrap(&[Span::styled(row.to_string(), theme::math())], width) {
                let used: usize = part.iter().map(|span| span.content.width()).sum();
                let mut centred = vec![Span::raw(" ".repeat(width.saturating_sub(used) / 2))];
                centred.extend(part);
                self.push_line(centred, first, line);
                first = false;
            }
        }
        self.gap = true;
    }

    fn push_wrapped(&mut self, runs: &[Run], width: usize, line: usize) {
        for (index, row) in wrap_runs(runs, width).into_iter().enumerate() {
            self.push_runs(row, index == 0, line);
        }
    }

    /// Reserves the rows of a picture, or shows its alt text and why it cannot be drawn.
    fn push_picture(&mut self, url: &str, alt: &str, base: Style, width: usize, line: usize) {
        match self.pictures.place(url, width) {
            Ok(placed) => self.push_placed(placed, line),
            Err(reason) => {
                let runs = [
                    (Span::styled(PICTURE_MARKER, theme::marker()), None),
                    (Span::styled(alt.to_string(), base), None),
                    (
                        Span::styled(format!(" ({reason})"), theme::picture_note()),
                        None,
                    ),
                ];
                self.push_wrapped(&runs, width, line);
            }
        }
    }

    fn push_placed(&mut self, placed: Placed, line: usize) {
        let rows = placed.rows;
        self.document.pictures.push(Picture {
            line: self.document.len(),
            column: self.containers.iter().map(container_width).sum(),
            placed,
        });
        for row in 0..rows {
            self.push_line(Vec::new(), row == 0, line);
        }
    }

    /// Draws a Mermaid block as a picture where the terminal has graphics, and as box text where
    /// half blocks would blur its labels. A block neither can draw keeps its source and the reason.
    fn push_diagram(&mut self, source: &str, width: usize, line: usize) {
        let failure = if self.pictures.has_graphics() {
            match self
                .diagrams
                .svg(source)
                .and_then(|svg| self.pictures.place_svg(svg, width))
            {
                Ok(placed) => return self.push_placed(placed, line),
                Err(reason) => reason,
            }
        } else {
            match diagrams::text(source, width) {
                Ok(rows) => {
                    for (index, row) in rows.into_iter().enumerate() {
                        self.push_line(vec![Span::styled(row, theme::diagram())], index == 0, line);
                    }
                    return;
                }
                Err(reason) => reason,
            }
        };
        self.push_code(source, "mermaid", width, line);
        let note = [(
            Span::styled(format!("(diagram: {failure})"), theme::picture_note()),
            None,
        )];
        self.push_wrapped(&note, width, line);
    }

    /// Adds a line and notes where the text of each link sits on it.
    fn push_runs(&mut self, content: Vec<Run>, first: bool, source: usize) {
        if self.hidden() {
            return;
        }
        let line = self.document.len();
        let mut column: usize = self.containers.iter().map(container_width).sum();
        for (span, link) in &content {
            let width = span.content.width();
            if let Some(url) = *link {
                match self.document.links.last_mut() {
                    Some(last)
                        if last.line == line
                            && last.url == url
                            && last.column + last.width == column =>
                    {
                        last.width += width;
                    }
                    _ => self.document.links.push(Link {
                        line,
                        column,
                        width,
                        url,
                    }),
                }
            }
            column += width;
        }
        let spans = content.into_iter().map(|(span, _)| span).collect();
        self.push_line(spans, first, source);
    }

    /// A code block keeps its own line breaks and sits on a filled panel, padded by one column
    /// and one row, so it reads as one block even when a line is blank.
    fn push_code(&mut self, text: &str, language: &str, width: usize, line: usize) {
        let inner = width.saturating_sub(2).max(1);
        let text = text.trim_end_matches('\n').replace('\t', TAB);
        let rows = highlight(&text, language).unwrap_or_else(|| {
            text.split('\n')
                .map(|row| vec![Span::styled(row.to_string(), theme::code_block())])
                .collect()
        });
        let panel = theme::code_block();
        let fill = |content: Vec<Span<'static>>| {
            let used: usize = content.iter().map(|span| span.content.width()).sum();
            let mut spans = vec![Span::styled(" ", panel)];
            spans.extend(content.into_iter().map(|span| {
                let style = panel.patch(span.style);
                span.style(style)
            }));
            spans.push(Span::styled(
                " ".repeat(width.saturating_sub(used + 1)),
                panel,
            ));
            spans
        };
        self.push_line(fill(Vec::new()), true, line);
        for row in rows {
            for part in hard_wrap(&row, inner) {
                self.push_line(fill(part), false, line);
            }
        }
        self.push_line(fill(Vec::new()), false, line);
    }

    fn blank_before_block(&mut self) {
        if self.gap && !self.document.is_empty() {
            let line = self.document.sources.last().copied().unwrap_or(0);
            self.push_line(Vec::new(), false, line);
            self.starts_block = true;
        }
        self.gap = false;
    }

    fn content_width(&self) -> usize {
        let prefix: usize = self.containers.iter().map(container_width).sum();
        self.width.saturating_sub(prefix).max(1)
    }

    fn push_line(&mut self, content: Vec<Span<'static>>, first: bool, source: usize) {
        if self.hidden() {
            return;
        }
        let mut spans = Vec::new();
        for container in &mut self.containers {
            match container {
                Container::Quote(kind) => spans.push(Span::styled(QUOTE_BAR, quote_bar(*kind))),
                Container::Item { marker, shown } if first && !*shown => {
                    spans.push(Span::styled(marker.clone(), theme::marker()));
                    *shown = true;
                }
                Container::Item { marker, .. } => spans.push(Span::raw(" ".repeat(marker.width()))),
            }
        }
        spans.extend(content);
        // The notes sit at the end of the page but may be defined anywhere, and a source line
        // that steps back would break the search that maps source lines to screen lines.
        let source = source.max(self.document.sources.last().copied().unwrap_or(0));
        if std::mem::take(&mut self.starts_block) {
            self.document.blocks.push(self.document.len());
        }
        self.document.lines.push(Line::from(spans));
        self.document.sources.push(source);
    }
}

fn container_width(container: &Container) -> usize {
    match container {
        Container::Quote(_) => QUOTE_BAR.width(),
        Container::Item { marker, .. } => marker.width(),
    }
}

fn quote_bar(kind: Option<BlockQuoteKind>) -> Style {
    kind.map_or_else(theme::marker, theme::alert)
}

fn alert_label(kind: BlockQuoteKind) -> &'static str {
    match kind {
        BlockQuoteKind::Note => "Note",
        BlockQuoteKind::Tip => "Tip",
        BlockQuoteKind::Important => "Important",
        BlockQuoteKind::Warning => "Warning",
        BlockQuoteKind::Caution => "Caution",
    }
}

fn superscript(number: usize) -> String {
    number
        .to_string()
        .chars()
        .filter_map(|digit| digit.to_digit(10))
        .map(|digit| SUPERSCRIPT_DIGITS[digit as usize])
        .collect()
}

fn closes_before_digit(markdown: &str, range: &Range<usize>) -> bool {
    markdown[range.end..].starts_with(|c: char| c.is_ascii_digit())
}

/// The language of a fence's info string, such as `rust` in `rust,ignore` or `ts title="x"`.
fn fence_language(info: &str) -> &str {
    info.split(|c: char| c.is_whitespace() || c == ',' || c == '{')
        .next()
        .unwrap_or_default()
}

/// A rendered page hides an HTML comment, so the preview does too.
fn is_comment(html: &str) -> bool {
    let html = html.trim();
    html.starts_with("<!--") && html.ends_with("-->")
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}
