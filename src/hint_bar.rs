use crossterm::event::KeyCode;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

use crate::keys::{Action, CANCEL, HINT_LETTERS_LISTED, Key, hints, keys_of};
use crate::theme;

const GAP: &str = "  ";

/// What the reader is doing, which decides the keys worth naming.
pub enum Mode {
    TypingSearch,
    Help,
    Tree,
    TypingFilter,
    Reading(Reading),
}

/// What the page holds and what the reader has under way on it.
#[allow(clippy::struct_excessive_bools)]
pub struct Reading {
    pub headings_in_contents: Option<usize>,
    pub search_active: bool,
    pub link_marked: bool,
    pub has_links: bool,
    pub can_go_back: bool,
    pub has_folds: bool,
}

pub struct Hint {
    keys: String,
    label: &'static str,
}

fn hint(keys: String, label: &'static str) -> Hint {
    Hint { keys, label }
}

fn pair(first: Action, second: Action, label: &'static str) -> Hint {
    hint(format!("{}/{}", keys_of(first), keys_of(second)), label)
}

/// The hints for `mode`, the most useful first, because a narrow bar drops them from the end.
pub fn hints_for(mode: &Mode) -> Vec<Hint> {
    match mode {
        Mode::TypingSearch => vec![
            hint(Key::named(KeyCode::Enter).label(), "search"),
            hint(CANCEL.label(), "cancel"),
        ],
        Mode::Help => vec![
            pair(Action::Down, Action::Up, "scroll"),
            hint(keys_of(Action::Quit), "close"),
        ],
        Mode::TypingFilter => vec![
            hint(Key::named(KeyCode::Enter).label(), "keep"),
            hint(CANCEL.label(), "clear"),
        ],
        Mode::Tree => vec![
            pair(Action::Down, Action::Up, "move"),
            hint(keys_of(Action::OpenEntry), "open"),
            hint(keys_of(Action::CloseFolder), "close"),
            hint(keys_of(Action::FilterTree), "filter"),
            hint(CANCEL.label(), "page"),
            hint(keys_of(Action::ToggleTree), "close tree"),
            hint(keys_of(Action::Quit), "quit"),
        ],
        Mode::Reading(reading) => reading_hints(reading),
    }
}

fn reading_hints(reading: &Reading) -> Vec<Hint> {
    let Reading {
        headings_in_contents,
        search_active,
        link_marked,
        has_links,
        can_go_back,
        has_folds,
    } = *reading;
    let mut bar = Vec::new();
    if link_marked {
        bar.push(hint(keys_of(Action::FollowLink), "follow"));
    }
    if let Some(count) = headings_in_contents.filter(|count| *count > 0) {
        let letters = hints(count.min(HINT_LETTERS_LISTED)).join("/");
        let more = if count > HINT_LETTERS_LISTED {
            "…"
        } else {
            ""
        };
        bar.push(hint(format!("{letters}{more}"), "go to section"));
    }
    let contents_label = match headings_in_contents {
        Some(_) => "close contents",
        None => "contents",
    };
    let contents = hint(keys_of(Action::ToggleContents), contents_label);
    let files = hint(keys_of(Action::ToggleTree), "files");
    let scroll = pair(Action::Down, Action::Up, "scroll");
    let help = hint(keys_of(Action::ToggleHelp), "help");
    let back = can_go_back.then(|| hint(keys_of(Action::Back), "back"));
    if headings_in_contents.is_some() {
        bar.extend(
            [Some(contents), Some(scroll), Some(help), back, Some(files)]
                .into_iter()
                .flatten(),
        );
    } else {
        bar.extend(
            [Some(scroll), Some(help), back, Some(contents), Some(files)]
                .into_iter()
                .flatten(),
        );
    }
    bar.push(pair(
        Action::NextHeading,
        Action::PreviousHeading,
        "section",
    ));
    if has_links {
        bar.push(pair(Action::NextLink, Action::PreviousLink, "link"));
    }
    bar.push(hint(keys_of(Action::SearchForward), "search"));
    if search_active {
        bar.push(pair(Action::NextMatch, Action::PreviousMatch, "match"));
    }
    if has_folds {
        bar.push(hint(keys_of(Action::ToggleFold), "fold"));
    }
    bar.push(pair(Action::HalfPageDown, Action::HalfPageUp, "half page"));
    bar.push(hint(keys_of(Action::Quit), "quit"));
    bar
}

/// Draws as many whole hints as fit in `area`, in order.
pub fn draw(frame: &mut Frame, area: Rect, hints: &[Hint]) {
    let mut spans = vec![Span::raw(" ")];
    let mut used = 1;
    for (index, hint) in hints.iter().enumerate() {
        let gap = if index == 0 { "" } else { GAP };
        let width = gap.width() + hint.keys.width() + 1 + hint.label.width();
        if used + width > usize::from(area.width) {
            break;
        }
        used += width;
        spans.extend([
            Span::raw(gap),
            Span::styled(hint.keys.clone(), theme::hint()),
            Span::styled(format!(" {}", hint.label), theme::hint_label()),
        ]);
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}
