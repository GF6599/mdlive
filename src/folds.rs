use std::collections::HashMap;

/// Which details blocks the reader has opened or closed. A fold is known by its summary text and
/// how many folds with that text come before it, so its state survives an edit elsewhere.
#[derive(Debug, Default, Clone)]
pub struct Folds {
    all: Option<bool>,
    toggled: HashMap<(String, usize), bool>,
}

impl Folds {
    pub fn is_open(&self, summary: &str, occurrence: usize, default: bool) -> bool {
        self.toggled
            .get(&(summary.to_string(), occurrence))
            .copied()
            .or(self.all)
            .unwrap_or(default)
    }

    pub fn set(&mut self, summary: &str, occurrence: usize, open: bool) {
        self.toggled.insert((summary.to_string(), occurrence), open);
    }

    pub fn set_all(&mut self, open: bool) {
        self.all = Some(open);
        self.toggled.clear();
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum DetailsTag {
    Start { open: bool, summary: String },
    End,
}

/// The `<details>` and `</details>` tags of a raw HTML block, in order, each start with the plain
/// text of the `<summary>` that follows it. `None` when the block holds neither tag.
pub fn details_tags(html: &str) -> Option<Vec<DetailsTag>> {
    let lower = html.to_ascii_lowercase();
    let mut tags = Vec::new();
    let mut at = 0;
    while let Some(found) = lower[at..].find("details") {
        let start = at + found;
        at = start + "details".len();
        if lower[..start].ends_with("</") {
            tags.push(DetailsTag::End);
        } else if lower[..start].ends_with('<') {
            let close = lower[at..].find('>').map_or(lower.len(), |end| at + end);
            let open = lower[at..close]
                .split_whitespace()
                .any(|word| word == "open" || word.starts_with("open="));
            let summary = lower[close..]
                .find("<summary")
                .and_then(|from| {
                    let from = close + from;
                    let text = from + lower[from..].find('>')? + 1;
                    let end = text + lower[text..].find("</summary>")?;
                    Some(plain_text(&html[text..end]))
                })
                .unwrap_or_else(|| "Details".to_string());
            tags.push(DetailsTag::Start { open, summary });
        }
    }
    (!tags.is_empty()).then_some(tags)
}

/// HTML as a reader sees it: tags dropped, the common entities decoded, spaces collapsed.
fn plain_text(html: &str) -> String {
    let mut text = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => text.push(c),
            _ => {}
        }
    }
    let decoded = ENTITIES
        .iter()
        .fold(text, |text, (entity, glyph)| text.replace(entity, glyph));
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `&amp;` comes last, so an escaped entity such as `&amp;lt;` decodes once, to `&lt;`.
const ENTITIES: [(&str, &str); 10] = [
    ("&lt;", "<"),
    ("&gt;", ">"),
    ("&quot;", "\""),
    ("&#39;", "'"),
    ("&nbsp;", " "),
    ("&middot;", "·"),
    ("&mdash;", "—"),
    ("&ndash;", "–"),
    ("&hellip;", "…"),
    ("&amp;", "&"),
];
