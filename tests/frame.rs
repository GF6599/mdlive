#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use mdlive::{Viewer, draw_frame};

mod common;

const BEGIN_SYNC: &str = "\x1b[?2026h";
const END_SYNC: &str = "\x1b[?2026l";
const URL: &str = "https://example.com/mdlive";

/// The bytes mdlive writes to the terminal for one frame of a page with a link.
fn one_frame() -> String {
    let (mut terminal, output) = common::terminal(40, 8);
    let mut viewer = Viewer::new("doc.md", &format!("see [the site]({URL}) here\n"));
    let mut shown = Vec::new();
    draw_frame(&mut terminal, &mut viewer, &mut shown).unwrap();
    output.take()
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
