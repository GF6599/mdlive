use crossterm::event::KeyCode;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

use crate::keys::{Action, HINT_LETTERS_LISTED, Key, hints, keys_of};
use crate::theme;

const GAP: &str = "  ";

/// What the reader is doing, which decides the keys worth naming.
pub enum Mode {
    TypingSearch,
    Help,
    Reading {
        headings_in_contents: Option<usize>,
        search_active: bool,
        can_go_back: bool,
        has_folds: bool,
    },
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
    let (headings_in_contents, search_active, can_go_back, has_folds) = match *mode {
        Mode::TypingSearch => {
            return vec![
                hint(Key::named(KeyCode::Enter).label(), "search"),
                hint(Key::named(KeyCode::Esc).label(), "cancel"),
            ];
        }
        Mode::Help => {
            return vec![
                pair(Action::Down, Action::Up, "scroll"),
                hint(keys_of(Action::Quit), "close"),
            ];
        }
        Mode::Reading {
            headings_in_contents,
            search_active,
            can_go_back,
            has_folds,
        } => (headings_in_contents, search_active, can_go_back, has_folds),
    };
    let mut bar = Vec::new();
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
    let scroll = pair(Action::Down, Action::Up, "scroll");
    let help = hint(keys_of(Action::ToggleHelp), "help");
    let back = can_go_back.then(|| hint(keys_of(Action::Back), "back"));
    if headings_in_contents.is_some() {
        bar.extend(
            [Some(contents), Some(scroll), Some(help), back]
                .into_iter()
                .flatten(),
        );
    } else {
        bar.extend(
            [Some(scroll), Some(help), back, Some(contents)]
                .into_iter()
                .flatten(),
        );
    }
    bar.push(pair(
        Action::NextHeading,
        Action::PreviousHeading,
        "section",
    ));
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
