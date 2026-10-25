use ratatui::style::Style;
use ratatui::text::Span;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub const BREAK: &str = "\n";

enum Token {
    Word(Vec<(String, Style)>),
    Space(Style),
    Break,
}

/// A word is everything between two spaces, so `**bo**ld` stays one word across its two styles.
fn tokens(spans: &[Span<'static>]) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut word: Vec<(String, Style)> = Vec::new();
    let flush = |word: &mut Vec<(String, Style)>, tokens: &mut Vec<Token>| {
        if !word.is_empty() {
            tokens.push(Token::Word(std::mem::take(word)));
        }
    };
    for span in spans {
        for grapheme in span.content.graphemes(true) {
            if grapheme == BREAK {
                flush(&mut word, &mut tokens);
                tokens.push(Token::Break);
            } else if grapheme.chars().all(char::is_whitespace) {
                flush(&mut word, &mut tokens);
                tokens.push(Token::Space(span.style));
            } else {
                match word.last_mut() {
                    Some((text, style)) if *style == span.style => text.push_str(grapheme),
                    _ => word.push((grapheme.to_string(), span.style)),
                }
            }
        }
    }
    flush(&mut word, &mut tokens);
    tokens
}

#[derive(Default)]
struct Lines {
    done: Vec<Vec<Span<'static>>>,
    line: Vec<Span<'static>>,
    used: usize,
}

impl Lines {
    fn push(&mut self, text: &str, style: Style) {
        self.used += text.width();
        match self.line.last_mut() {
            Some(last) if last.style == style => last.content.to_mut().push_str(text),
            _ => self.line.push(Span::styled(text.to_string(), style)),
        }
    }

    fn end_line(&mut self) {
        self.done.push(std::mem::take(&mut self.line));
        self.used = 0;
    }

    fn push_split(&mut self, word: &[(String, Style)], width: usize) {
        for (text, style) in word {
            for grapheme in text.graphemes(true) {
                if self.used + grapheme.width() > width && self.used > 0 {
                    self.end_line();
                }
                self.push(grapheme, *style);
            }
        }
    }
}

/// Wraps styled text at spaces to fit `width` display columns, and splits only a word wider
/// than a line. Runs of spaces collapse to one, and no line starts with a space.
pub fn wrap(spans: &[Span<'static>], width: usize) -> Vec<Vec<Span<'static>>> {
    let mut lines = Lines::default();
    let mut space: Option<Style> = None;
    for token in tokens(spans) {
        match token {
            Token::Space(style) => space = Some(style),
            Token::Break => {
                lines.end_line();
                space = None;
            }
            Token::Word(word) => {
                let word_width: usize = word.iter().map(|(text, _)| text.width()).sum();
                let gap = usize::from(space.is_some() && lines.used > 0);
                if lines.used + gap + word_width <= width {
                    if gap == 1
                        && let Some(style) = space
                    {
                        lines.push(" ", style);
                    }
                    for (text, style) in &word {
                        lines.push(text, *style);
                    }
                } else {
                    if lines.used > 0 {
                        lines.end_line();
                    }
                    lines.push_split(&word, width);
                }
                space = None;
            }
        }
    }
    if !lines.line.is_empty() {
        lines.end_line();
    }
    lines.done
}

pub fn hard_wrap(line: &str, width: usize) -> Vec<String> {
    let mut rows = vec![String::new()];
    let mut used = 0;
    for grapheme in line.graphemes(true) {
        if used + grapheme.width() > width && used > 0 {
            rows.push(String::new());
            used = 0;
        }
        used += grapheme.width();
        if let Some(row) = rows.last_mut() {
            row.push_str(grapheme);
        }
    }
    rows
}
