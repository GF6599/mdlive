#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::path::Path;
use std::process::Command;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use mdlive::Viewer;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui_image::picker::{Picker, ProtocolType};

mod common;

/// The ffmpeg processes that this test process started and has not waited for. An ffmpeg that ends
/// on a closed pipe and is never waited for stays as a zombie, which macOS lists as `<defunct>`
/// and Linux under its name, so a zombie child counts too. This test is the only one in its file,
/// because the tests of one file run at once and start ffmpeg processes too.
fn ffmpeg_children() -> usize {
    let output = Command::new("ps")
        .args(["-A", "-o", "ppid=,stat=,comm="])
        .output()
        .unwrap();
    let me = std::process::id().to_string();
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| {
            let mut fields = line.split_whitespace();
            let (parent, state) = (fields.next(), fields.next().unwrap_or(""));
            let command = fields.collect::<Vec<_>>().join(" ");
            parent == Some(me.as_str()) && (command.contains("ffmpeg") || state.starts_with('Z'))
        })
        .count()
}

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
    assert_eq!(ffmpeg_children(), 1);

    // When: the reader closes the view
    viewer.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));

    // Then: no ffmpeg of this process runs, and no frame is due
    assert_eq!(ffmpeg_children(), 0);
    assert_eq!(viewer.until_next_frame(), None);
}
