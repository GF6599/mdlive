use ratatui::buffer::{Buffer, Cell};
use ratatui::layout::Rect;
use unicode_width::UnicodeWidthStr;

const CLOSE: &str = "\x1b]8;;\x1b\\";

/// The text of a link on screen and the address it opens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkArea {
    pub area: Rect,
    pub id: usize,
    pub url: String,
}

/// Whether a terminal can open the address: it starts with a scheme, and it holds no control
/// character that could end the escape sequence it travels in.
pub fn is_openable(url: &str) -> bool {
    let Some((scheme, _)) = url.split_once(':') else {
        return false;
    };
    let mut chars = scheme.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
        && !url.chars().any(char::is_control)
}

/// The cells to write after a frame so the terminal links them: the cells of each link inside
/// OSC 8 codes, and the cells of a link that left the screen without them. ratatui skips control
/// characters in the text it lays out, so the codes ride in the symbols of these cells.
pub fn hyperlink_cells(
    buffer: &Buffer,
    now: &[LinkArea],
    before: &[LinkArea],
) -> Vec<(u16, u16, Cell)> {
    let mut cells = Vec::new();
    for old in before.iter().filter(|old| !now.contains(old)) {
        cells.extend(cells_in(buffer, old.area));
    }
    for link in now {
        let mut linked = cells_in(buffer, link.area);
        if let Some((_, _, first)) = linked.first_mut() {
            let symbol = format!("\x1b]8;id={};{}\x1b\\{}", link.id, link.url, first.symbol());
            first.set_symbol(&symbol);
        }
        if let Some((_, _, last)) = linked.last_mut() {
            let symbol = format!("{}{CLOSE}", last.symbol());
            last.set_symbol(&symbol);
        }
        cells.extend(linked);
    }
    cells
}

/// The cells of one row of `area`, without the cell a wide character covers beside itself.
fn cells_in(buffer: &Buffer, area: Rect) -> Vec<(u16, u16, Cell)> {
    let area = area.intersection(buffer.area);
    let mut cells = Vec::new();
    let mut x = area.x;
    while x < area.right() {
        let cell = buffer[(x, area.y)].clone();
        let width = u16::try_from(cell.symbol().width().max(1)).unwrap_or(1);
        cells.push((x, area.y, cell));
        x = x.saturating_add(width);
    }
    cells
}
