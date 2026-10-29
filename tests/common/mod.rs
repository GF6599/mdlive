// Each test binary compiles this module and uses only part of it.
#![allow(dead_code)]

use std::cell::RefCell;
use std::io::{self, Write};
use std::rc::Rc;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use ratatui::backend::CrosstermBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::{Terminal, TerminalOptions, Viewport};

/// The breadcrumb row the viewer draws above the page.
pub const HEADER_ROWS: u16 = 1;

/// The screen under the breadcrumb row: the page, the hint bar and the status line, from row 0.
/// Tests about the page read it this way, so a row number names a row of the page.
pub fn below_header(screen: &Buffer) -> Buffer {
    let area = Rect {
        height: screen.area.height - HEADER_ROWS,
        ..screen.area
    };
    let mut below = Buffer::empty(area);
    for y in 0..area.height {
        for x in 0..area.width {
            below[(x, y)] = screen[(x, y + HEADER_ROWS)].clone();
        }
    }
    below
}

/// A terminal output that the test can still read after the backend takes it.
#[derive(Clone, Default)]
pub struct Output(Rc<RefCell<Vec<u8>>>);

impl Output {
    pub fn take(&self) -> String {
        String::from_utf8(self.0.take()).unwrap()
    }
}

impl Write for Output {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.borrow_mut().write(bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// A terminal of `width` by `height` cells that writes to `Output`. A fixed viewport keeps it
/// from asking a real screen for its size.
pub fn terminal(width: u16, height: u16) -> (Terminal<CrosstermBackend<Output>>, Output) {
    let output = Output::default();
    let backend = CrosstermBackend::new(output.clone());
    let viewport = Viewport::Fixed(Rect::new(0, 0, width, height));
    let terminal = Terminal::with_options(backend, TerminalOptions { viewport }).unwrap();
    (terminal, output)
}

/// The pixels of the one Kitty picture that `buffer` sends, as RGBA bytes from the top left.
pub fn kitty_pixels(buffer: &Buffer) -> Vec<u8> {
    let sent: String = buffer.content().iter().map(|cell| cell.symbol()).collect();
    let payload: String = sent
        .split("\x1b_G")
        .skip(1)
        .filter_map(|command| command.split("\x1b\\").next()?.split_once(';'))
        .map(|(_, data)| data)
        .collect();
    STANDARD.decode(payload).unwrap()
}
