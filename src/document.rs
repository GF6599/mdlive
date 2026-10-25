use std::ops::Range;

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use crate::theme;
use crate::wrap::{BREAK, hard_wrap, wrap};

const BULLET: &str = "• ";
const QUOTE_BAR: &str = "▎ ";
const TAB: &str = "    ";

/// A Markdown source laid out for one text width.
#[derive(Debug, Default)]
pub struct Document {
    lines: Vec<Line<'static>>,
    /// The source line each screen line came from. A blank line between blocks takes the
    /// source line of the block above it, so a position never anchors to a gap.
    sources: Vec<usize>,
    headings: Vec<Heading>,
}

#[derive(Debug)]
struct Heading {
    line: usize,
    text: String,
}

/// A place in a document that survives an edit: a heading and the lines below it, with the source
/// line as the fallback when the heading is gone.
#[derive(Debug)]
pub struct Anchor {
    heading: Option<(String, usize, usize)>,
    source_line: usize,
}

impl Document {
    pub fn render(markdown: &str, width: usize) -> Self {
        Builder::new(markdown, width.max(1)).build(markdown)
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
        let heading = self
            .headings
            .iter()
            .rposition(|heading| heading.line <= top)
            .map(|index| {
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

    /// The first screen line of the block that holds `source_line`.
    pub fn screen_line_of(&self, source_line: usize) -> usize {
        let after = self.sources.partition_point(|&line| line <= source_line);
        let block = self.sources[..after].last().copied().unwrap_or(0);
        self.sources.partition_point(|&line| line < block)
    }
}

#[derive(Debug)]
enum Container {
    Quote,
    Item { marker: String, shown: bool },
}

enum Block {
    Text {
        spans: Vec<Span<'static>>,
        base: Style,
    },
    Heading {
        spans: Vec<Span<'static>>,
        level: u8,
    },
    Code {
        text: String,
    },
    Raw {
        text: String,
    },
}

struct Builder {
    width: usize,
    line_starts: Vec<usize>,
    document: Document,
    containers: Vec<Container>,
    lists: Vec<Option<u64>>,
    styles: Vec<Style>,
    block: Option<(Block, usize)>,
    gap: bool,
}

impl Builder {
    fn new(markdown: &str, width: usize) -> Self {
        let line_starts = std::iter::once(0)
            .chain(markdown.match_indices('\n').map(|(at, _)| at + 1))
            .collect();
        Self {
            width,
            line_starts,
            document: Document::default(),
            containers: Vec::new(),
            lists: Vec::new(),
            styles: Vec::new(),
            block: None,
            gap: false,
        }
    }

    fn build(mut self, markdown: &str) -> Document {
        for (event, range) in
            Parser::new_ext(markdown, Options::ENABLE_STRIKETHROUGH).into_offset_iter()
        {
            self.event(event, range);
        }
        self.finish_block();
        self.document
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
            Tag::CodeBlock(_) => self.open(
                Block::Code {
                    text: String::new(),
                },
                range.start,
            ),
            Tag::BlockQuote(_) => {
                self.finish_block();
                self.containers.push(Container::Quote);
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
            Tag::Emphasis => self.styles.push(theme::emphasis()),
            Tag::Strong => self.styles.push(theme::strong()),
            Tag::Strikethrough => self.styles.push(theme::strikethrough()),
            Tag::Link { .. } => self.styles.push(theme::link()),
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::CodeBlock | TagEnd::HtmlBlock => {
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
            TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough | TagEnd::Link => {
                self.styles.pop();
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
            .any(|container| matches!(container, Container::Quote))
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
            Some((Block::Code { text: code }, _)) => code.push_str(text),
            Some((Block::Raw { text: raw }, _)) => raw.push_str(text),
            _ => self.inline(text, range, Style::new()),
        }
    }

    /// Adds inline text, opening a paragraph for the text of a tight list item.
    fn inline(&mut self, text: &str, range: Range<usize>, style: Style) {
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
        match &mut self.block {
            Some((Block::Text { spans, base }, _)) => {
                spans.push(Span::styled(text.to_string(), base.patch(style)));
            }
            Some((Block::Heading { spans, level }, _)) => {
                spans.push(Span::styled(
                    text.to_string(),
                    theme::heading(*level).patch(style),
                ));
            }
            _ => {}
        }
    }

    fn open(&mut self, block: Block, offset: usize) {
        self.finish_block();
        self.block = Some((block, self.source_line(offset)));
    }

    fn finish_block(&mut self) {
        let Some((block, line)) = self.block.take() else {
            return;
        };
        if let Block::Raw { text } = &block
            && is_comment(text)
        {
            return;
        }
        self.blank_before_block();
        let width = self.content_width();
        match block {
            Block::Text { spans, .. } => self.push_wrapped(&spans, width, line),
            Block::Heading { spans, level } => {
                self.document.headings.push(Heading {
                    line: self.document.len(),
                    text: spans.iter().map(|span| span.content.as_ref()).collect(),
                });
                self.push_wrapped(&spans, width, line);
                if let Some((glyph, style)) = theme::heading_rule(level) {
                    self.push_line(vec![Span::styled(glyph.repeat(width), style)], false, line);
                }
            }
            Block::Code { text } => self.push_code(&text, width, line),
            Block::Raw { text } => {
                let mut first = true;
                for row in text.trim_end_matches('\n').lines() {
                    for part in hard_wrap(row, width) {
                        self.push_line(vec![Span::styled(part, theme::html())], first, line);
                        first = false;
                    }
                }
            }
        }
    }

    fn push_wrapped(&mut self, spans: &[Span<'static>], width: usize, line: usize) {
        for (index, row) in wrap(spans, width).into_iter().enumerate() {
            self.push_line(row, index == 0, line);
        }
    }

    /// A code block keeps its own line breaks and sits on a filled panel, padded by one column
    /// and one row, so it reads as one block even when a line is blank.
    fn push_code(&mut self, text: &str, width: usize, line: usize) {
        let inner = width.saturating_sub(2).max(1);
        let style = theme::code_block();
        let fill = |content: String| {
            let pad = width.saturating_sub(content.width() + 1);
            vec![Span::styled(
                format!(" {content}{}", " ".repeat(pad)),
                style,
            )]
        };
        self.push_line(fill(String::new()), true, line);
        for row in text.trim_end_matches('\n').split('\n') {
            for part in hard_wrap(&row.replace('\t', TAB), inner) {
                self.push_line(fill(part), false, line);
            }
        }
        self.push_line(fill(String::new()), false, line);
    }

    fn blank_before_block(&mut self) {
        if self.gap && !self.document.is_empty() {
            let line = self.document.sources.last().copied().unwrap_or(0);
            self.push_line(Vec::new(), false, line);
        }
        self.gap = false;
    }

    fn content_width(&self) -> usize {
        let prefix: usize = self.containers.iter().map(container_width).sum();
        self.width.saturating_sub(prefix).max(1)
    }

    fn push_line(&mut self, content: Vec<Span<'static>>, first: bool, source: usize) {
        let mut spans = Vec::new();
        for container in &mut self.containers {
            match container {
                Container::Quote => spans.push(Span::styled(QUOTE_BAR, theme::marker())),
                Container::Item { marker, shown } if first && !*shown => {
                    spans.push(Span::styled(marker.clone(), theme::marker()));
                    *shown = true;
                }
                Container::Item { marker, .. } => spans.push(Span::raw(" ".repeat(marker.width()))),
            }
        }
        spans.extend(content);
        self.document.lines.push(Line::from(spans));
        self.document.sources.push(source);
    }
}

fn container_width(container: &Container) -> usize {
    match container {
        Container::Quote => QUOTE_BAR.width(),
        Container::Item { marker, .. } => marker.width(),
    }
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
