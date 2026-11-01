#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::path::Path;
use std::process::Command;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use mdlive::Viewer;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui_image::picker::{Picker, ProtocolType};

mod common;

/// Longer than mdlive may wait for ffmpeg, so a wait past it is a frozen viewer.
const FROZEN: Duration = Duration::from_secs(20);
const FRAME: Duration = Duration::from_millis(40);

/// Stops the ffmpeg that sends the frames of a playing video, the way a stalled disk or network
/// share stops it.
fn stop_the_stream() {
    let output = Command::new("ps")
        .args(["-A", "-o", "pid=,ppid=,args="])
        .output()
        .unwrap();
    let me = std::process::id().to_string();
    let streams: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let (pid, parent) = (fields.next()?, fields.next()?);
            let args = fields.collect::<Vec<_>>().join(" ");
            (parent == me && args.contains("rawvideo")).then(|| pid.to_string())
        })
        .collect();
    assert_eq!(streams.len(), 1, "{streams:?}");
    assert!(
        Command::new("kill")
            .args(["-STOP", &streams[0]])
            .status()
            .unwrap()
            .success()
    );
}

/// `from_fontsize` is the only constructor that takes a cell size, because `from_query_stdio` asks
/// a real terminal.
#[allow(deprecated)]
fn viewer(dir: &Path, page: &str) -> Viewer {
    let mut picker = Picker::from_fontsize((10, 20).into());
    picker.set_protocol_type(ProtocolType::Halfblocks);
    Viewer::new("doc.md", page).with_images(dir, picker)
}

fn screen_text(buffer: &Buffer) -> String {
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn draw(viewer: &mut Viewer) -> String {
    let mut terminal = Terminal::new(TestBackend::new(60, 13)).unwrap();
    terminal.draw(|frame| viewer.draw(frame)).unwrap();
    screen_text(terminal.backend().buffer())
}

fn in_background<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> Receiver<T> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let _ = sender.send(work());
    });
    receiver
}

fn finished<T>(what: &str, work: &Receiver<T>) -> T {
    work.recv_timeout(FROZEN)
        .unwrap_or_else(|_| panic!("{what} froze for {FROZEN:?}"))
}

/// An ffmpeg that does not answer is stopped, on the page and during playback, and its reason
/// shows where the picture would.
///
/// Why it matters: mdlive waited for ffmpeg on its only thread with no time limit, so a video that
/// ffmpeg could not finish, on a stalled share or a named pipe, froze the whole viewer.
#[test]
fn stops_an_ffmpeg_that_does_not_answer() {
    let dir = tempfile::tempdir().unwrap();
    if common::red_then_blue_clip(dir.path()).is_none() {
        return;
    }

    // Given: a page that links a named pipe with a video name, which no program writes
    let stuck = dir.path().join("stuck.mp4");
    assert!(
        Command::new("mkfifo")
            .arg(&stuck)
            .status()
            .unwrap()
            .success()
    );
    // And: a video playing in the view of another viewer, whose ffmpeg then stops

    // When: mdlive draws the first page, and at the same time plays the video past the frames
    // that ffmpeg sent
    let folder = dir.path().to_path_buf();
    let page = in_background(move || {
        draw(&mut viewer(
            &folder,
            "intro\n\n![stuck](stuck.mp4)\n\nafter\n",
        ))
    });
    let folder = dir.path().to_path_buf();
    let playback = in_background(move || {
        let mut viewer = viewer(&folder, "intro\n\n![clip](clip.mkv)\n\nafter\n");
        draw(&mut viewer);
        for key in "]p".chars() {
            viewer.handle_key(KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE));
        }
        viewer.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        stop_the_stream();
        while viewer.until_next_frame().is_some() {
            viewer.play(FRAME);
        }
        (draw(&mut viewer), viewer.until_next_frame())
    });
    let page = finished("the page", &page);
    let (status, due) = finished("the playback", &playback);

    // Then: the page shows that ffmpeg did not answer
    assert!(page.contains("did not answer"), "{page}");
    // And: the status line shows it too, and the video is paused
    assert!(status.contains("did not answer"), "{status}");
    assert_eq!(due, None);
    // And: no ffmpeg is left
    assert_eq!(common::ffmpeg_children(), 0);
}
