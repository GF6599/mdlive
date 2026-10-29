use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

use crate::contents::fit;
use crate::document::Heading;
use crate::theme::Palette;

const SEPARATOR: &str = " › ";
const ELLIPSIS: &str = "…";

/// The heading at `current` and each heading that holds it, outermost first.
pub fn sections(headings: &[Heading], current: Option<usize>) -> Vec<&str> {
    let Some(current) = current.filter(|current| *current < headings.len()) else {
        return Vec::new();
    };
    let mut level = headings[current].level;
    let mut path = vec![headings[current].text.as_str()];
    for heading in headings[..current].iter().rev() {
        if heading.level < level {
            level = heading.level;
            path.push(&heading.text);
        }
    }
    path.reverse();
    path
}

/// Draws ` file › outer › … › section` in `area`. When it does not fit, the outer sections give
/// way to an ellipsis first, because the innermost one says where the reader is.
pub fn draw(frame: &mut Frame, area: Rect, file: &str, sections: &[&str], palette: &Palette) {
    let width = usize::from(area.width);
    let mut skipped = 0;
    while skipped + 1 < sections.len()
        && line_width(file, &sections[skipped..], skipped > 0) > width
    {
        skipped += 1;
    }
    let mut spans = vec![Span::styled(format!(" {file}"), palette.breadcrumb())];
    if skipped > 0 {
        spans.push(Span::styled(SEPARATOR, palette.breadcrumb_separator()));
        spans.push(Span::styled(ELLIPSIS, palette.breadcrumb()));
    }
    let shown = &sections[skipped..];
    for (index, section) in shown.iter().enumerate() {
        spans.push(Span::styled(SEPARATOR, palette.breadcrumb_separator()));
        if index + 1 == shown.len() {
            let used: usize = spans.iter().map(|span| span.content.width()).sum();
            let room = width.saturating_sub(used);
            spans.push(Span::styled(
                fit(section, room),
                palette.breadcrumb_current(),
            ));
        } else {
            spans.push(Span::styled(*section, palette.breadcrumb()));
        }
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn line_width(file: &str, sections: &[&str], elided: bool) -> usize {
    let parts = sections
        .iter()
        .map(|section| SEPARATOR.width() + section.width());
    let ellipsis = if elided {
        SEPARATOR.width() + ELLIPSIS.width()
    } else {
        0
    };
    1 + file.width() + ellipsis + parts.sum::<usize>()
}
