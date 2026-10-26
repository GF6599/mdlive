use ratatui::text::Line;
use unicode_width::UnicodeWidthChar;

/// A place a pattern matched on screen, in display columns of its line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Match {
    pub line: usize,
    pub column: usize,
    pub width: usize,
}

/// Every match of `pattern` in the rendered lines. The match ignores case unless the pattern holds
/// a capital letter, as vim's smartcase does.
pub fn find_all(lines: &[Line], pattern: &str) -> Vec<Match> {
    let exact = pattern.chars().any(char::is_uppercase);
    let same = |a: char, b: char| {
        if exact {
            a == b
        } else {
            a.to_lowercase().eq(b.to_lowercase())
        }
    };
    let needle: Vec<char> = pattern.chars().collect();
    if needle.is_empty() {
        return Vec::new();
    }
    let mut matches = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let mut column = 0;
        let cells: Vec<(char, usize)> = line
            .spans
            .iter()
            .flat_map(|span| span.content.chars())
            .map(|c| {
                let at = column;
                column += c.width().unwrap_or(0);
                (c, at)
            })
            .collect();
        let mut start = 0;
        while start + needle.len() <= cells.len() {
            let window = &cells[start..start + needle.len()];
            if window.iter().zip(&needle).all(|(&(c, _), &n)| same(c, n)) {
                let first = window[0].1;
                let end = cells
                    .get(start + needle.len())
                    .map_or(column, |&(_, at)| at);
                matches.push(Match {
                    line: index,
                    column: first,
                    width: end - first,
                });
                start += needle.len();
            } else {
                start += 1;
            }
        }
    }
    matches
}
