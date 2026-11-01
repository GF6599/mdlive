#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::path::Path;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use mdlive::Viewer;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui_image::picker::{Picker, ProtocolType};

mod common;

/// `from_fontsize` is the only constructor that takes a cell size, because `from_query_stdio` asks
/// a real terminal.
#[allow(deprecated)]
fn open_in_view(dir: &Path) -> Viewer {
    let mut picker = Picker::from_fontsize((10, 20).into());
    picker.set_protocol_type(ProtocolType::Halfblocks);
    let mut viewer =
        Viewer::new("doc.md", "intro\n\n![clip](clip.mkv)\n\nafter\n").with_images(dir, picker);
    let mut terminal = Terminal::new(TestBackend::new(40, 13)).unwrap();
    terminal.draw(|frame| viewer.draw(frame)).unwrap();
    for key in "]p".chars() {
        viewer.handle_key(KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE));
    }
    viewer.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    viewer
}

/// Closing the view stops the ffmpeg that reads the video.
///
/// Why it matters: an ffmpeg left behind would keep a process and its memory for each video the
/// reader opened.
#[test]
fn stops_ffmpeg_when_the_view_closes() {
    // Given: the red and blue video playing in the view, read by an ffmpeg of this process
    let dir = tempfile::tempdir().unwrap();
    if common::red_then_blue_clip(dir.path()).is_none() {
        return;
    }
    let mut viewer = open_in_view(dir.path());
    assert_eq!(common::ffmpeg_children(), 1);

    // When: the reader closes the view
    viewer.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));

    // Then: no ffmpeg of this process runs, and no frame is due
    assert_eq!(common::ffmpeg_children(), 0);
    assert_eq!(viewer.until_next_frame(), None);
}
