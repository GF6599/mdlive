use ratatui::style::Style;
use ratatui::text::Span;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub const BREAK: &str = "\n";

/// A span of text and the index of the link it belongs to, so a link can be found again on the
/// rows it wraps onto.
pub type Run = (Span<'static>, Option<usize>);

/// What two pieces of text must share to join into one span.
type Key = (Style, Option<usize>);

enum Token {
    Word(Vec<(String, Key)>),
    Space(Key),
    Break,
}

/// A word is everything between two spaces, so `**bo**ld` stays one word across its two styles.
fn tokens(runs: &[Run]) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut word: Vec<(String, Key)> = Vec::new();
    let flush = |word: &mut Vec<(String, Key)>, tokens: &mut Vec<Token>| {
        if !word.is_empty() {
            tokens.push(Token::Word(std::mem::take(word)));
        }
    };
    for (span, link) in runs {
        let key = (span.style, *link);
        for grapheme in span.content.graphemes(true) {
            if grapheme == BREAK {
                flush(&mut word, &mut tokens);
                tokens.push(Token::Break);
            } else if grapheme.chars().all(char::is_whitespace) {
                flush(&mut word, &mut tokens);
                tokens.push(Token::Space(key));
            } else {
                match word.last_mut() {
                    Some((text, last)) if *last == key => text.push_str(grapheme),
                    _ => word.push((grapheme.to_string(), key)),
                }
            }
        }
    }
    flush(&mut word, &mut tokens);
    tokens
}

#[derive(Default)]
struct Lines {
    done: Vec<Vec<Run>>,
    line: Vec<Run>,
    used: usize,
}

impl Lines {
    fn push(&mut self, text: &str, (style, link): Key) {
        self.used += text.width();
        match self.line.last_mut() {
            Some((last, last_link)) if last.style == style && *last_link == link => {
                last.content.to_mut().push_str(text);
            }
            _ => self
                .line
                .push((Span::styled(text.to_string(), style), link)),
        }
    }

    fn end_line(&mut self) {
        self.done.push(std::mem::take(&mut self.line));
        self.used = 0;
    }

    fn push_split(&mut self, word: &[(String, Key)], width: usize) {
        for (text, key) in word {
            for grapheme in text.graphemes(true) {
                if self.used + grapheme.width() > width && self.used > 0 {
                    self.end_line();
                }
                self.push(grapheme, *key);
            }
        }
    }
}

pub fn wrap(spans: &[Span<'static>], width: usize) -> Vec<Vec<Span<'static>>> {
    let runs: Vec<Run> = spans.iter().map(|span| (span.clone(), None)).collect();
    wrap_runs(&runs, width)
        .into_iter()
        .map(|row| row.into_iter().map(|(span, _)| span).collect())
        .collect()
}

/// Wraps styled text at spaces to fit `width` display columns, and splits only a word wider
/// than a line. Runs of spaces collapse to one, and no line starts with a space.
pub fn wrap_runs(runs: &[Run], width: usize) -> Vec<Vec<Run>> {
    let mut lines = Lines::default();
    let mut space: Option<Key> = None;
    for token in tokens(runs) {
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
                        && let Some(key) = space
                    {
                        lines.push(" ", key);
                    }
                    for (text, key) in &word {
                        lines.push(text, *key);
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

/// Splits a line into rows of `width` columns at any grapheme, the way code wraps in a panel.
pub fn hard_wrap(spans: &[Span<'static>], width: usize) -> Vec<Vec<Span<'static>>> {
    let mut lines = Lines::default();
    for span in spans {
        for grapheme in span.content.graphemes(true) {
            if lines.used + grapheme.width() > width && lines.used > 0 {
                lines.end_line();
            }
            lines.push(grapheme, (span.style, None));
        }
    }
    lines.end_line();
    lines
        .done
        .into_iter()
        .map(|row| row.into_iter().map(|(span, _)| span).collect())
        .collect()
}
