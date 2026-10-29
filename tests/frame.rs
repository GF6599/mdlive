#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::cell::RefCell;
use std::io::{self, Write};
use std::rc::Rc;

use mdlive::{Viewer, draw_frame};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::{Terminal, TerminalOptions, Viewport};

const BEGIN_SYNC: &str = "\x1b[?2026h";
const END_SYNC: &str = "\x1b[?2026l";
const URL: &str = "https://example.com/mdlive";

/// A terminal output that the test can still read after the backend takes it.
#[derive(Clone, Default)]
struct Output(Rc<RefCell<Vec<u8>>>);

impl Write for Output {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.borrow_mut().write(bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// The bytes mdlive writes to the terminal for one frame of a page with a link. A fixed viewport
/// keeps the terminal from asking a real screen for its size.
fn one_frame() -> String {
    let output = Output::default();
    let backend = CrosstermBackend::new(output.clone());
    let viewport = Viewport::Fixed(Rect::new(0, 0, 40, 8));
    let mut terminal = Terminal::with_options(backend, TerminalOptions { viewport }).unwrap();
    let mut viewer = Viewer::new("doc.md", &format!("see [the site]({URL}) here\n"));
    let mut shown = Vec::new();
    draw_frame(&mut terminal, &mut viewer, &mut shown).unwrap();
    String::from_utf8(output.0.take()).unwrap()
}

/// A frame reaches the terminal as one synchronized update, with its link codes inside.
///
/// Why it matters: a frame goes out in two writes, the cells and then the link codes. Outside a
/// synchronized update the terminal can show the screen between or during them, which tears a
/// scrolling page.
#[test]
fn writes_a_frame_and_its_links_in_one_synchronized_update() {
    // Given: a page with one link
    // When: mdlive draws a frame
    let output = one_frame();

    // Then: the frame opens and closes one synchronized update
    assert!(output.starts_with(BEGIN_SYNC), "{output:?}");
    assert!(output.ends_with(END_SYNC), "{output:?}");
    assert_eq!(output.matches(BEGIN_SYNC).count(), 1);
    // And: the link code is inside it
    assert!(output.contains(&format!(";{URL}\x1b\\")), "{output:?}");
}
