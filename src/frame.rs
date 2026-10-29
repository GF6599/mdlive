use std::io::{self, Write};

use crossterm::queue;
use crossterm::terminal::{BeginSynchronizedUpdate, EndSynchronizedUpdate};
use ratatui::Terminal;
use ratatui::backend::{Backend, CrosstermBackend};

use crate::Viewer;
use crate::hyperlink::{LinkArea, hyperlink_cells};

/// One synchronized update holds the frame and its link codes, so the terminal never shows a part
/// of a frame. `shown` holds the links on screen after the last frame, so a link that left the
/// screen loses its code.
pub fn draw_frame<W: Write>(
    terminal: &mut Terminal<CrosstermBackend<W>>,
    viewer: &mut Viewer,
    shown: &mut Vec<LinkArea>,
) -> io::Result<()> {
    queue!(terminal.backend_mut(), BeginSynchronizedUpdate)?;
    let drawn = draw_with_links(terminal, viewer, shown);
    // A terminal left inside the update stops showing new output, so the update ends even when
    // the frame failed.
    let backend = terminal.backend_mut();
    queue!(backend, EndSynchronizedUpdate)?;
    Backend::flush(backend)?;
    drawn
}

fn draw_with_links<W: Write>(
    terminal: &mut Terminal<CrosstermBackend<W>>,
    viewer: &mut Viewer,
    shown: &mut Vec<LinkArea>,
) -> io::Result<()> {
    let frame = terminal.draw(|frame| viewer.draw(frame))?;
    let cells = hyperlink_cells(frame.buffer, viewer.links(), shown);
    if !cells.is_empty() {
        terminal
            .backend_mut()
            .draw(cells.iter().map(|(x, y, cell)| (*x, *y, cell)))?;
    }
    *shown = viewer.links().to_vec();
    Ok(())
}
