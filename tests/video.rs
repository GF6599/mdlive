#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use mdlive::{Viewer, find_ffmpeg};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::Color;
use ratatui_image::picker::{Picker, ProtocolType};

mod common;

use common::{HEADER_ROWS, below_header};

const WIDTH: u16 = 40;
const HEIGHT: u16 = 12;
/// The row of the page under the "intro" line and the blank line after it, where the picture of
/// the video starts.
const PICTURE_ROW: u16 = 2;

/// A red video of 40 by 20 pixels in `dir`, made with the real ffmpeg, or `None` with a note when
/// ffmpeg is not installed, so that the test is skipped.
fn red_clip(dir: &Path, name: &str) -> Option<PathBuf> {
    let Some(ffmpeg) = find_ffmpeg() else {
        eprintln!("skipped: ffmpeg is not installed");
        return None;
    };
    let status = Command::new(ffmpeg)
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=c=red:s=40x20:d=1:r=5",
        ])
        .args(["-c:v", "mpeg4", "-pix_fmt", "yuv420p"])
        .arg(dir.join(name))
        .status()
        .unwrap();
    assert!(status.success());
    Some(dir.join(name))
}

/// `from_fontsize` is the only constructor that takes a cell size, because `from_query_stdio` asks
/// a real terminal.
#[allow(deprecated)]
fn viewer(dir: &Path, markdown: &str) -> Viewer {
    let mut picker = Picker::from_fontsize((10, 20).into());
    picker.set_protocol_type(ProtocolType::Halfblocks);
    Viewer::new("doc.md", markdown).with_images(dir, picker)
}

fn draw(viewer: &mut Viewer) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(WIDTH, HEIGHT + HEADER_ROWS)).unwrap();
    terminal.draw(|frame| viewer.draw(frame)).unwrap();
    below_header(terminal.backend().buffer())
}

fn row(buffer: &Buffer, y: u16) -> String {
    (0..buffer.area.width)
        .map(|x| buffer[(x, y)].symbol())
        .collect::<String>()
}

/// Whether a cell of the picture row is close to red. Video stores colour at less precision than
/// RGB, so red comes back a step or two off.
fn shows_red(buffer: &Buffer) -> bool {
    (0..buffer.area.width).any(|x| {
        matches!(buffer[(x, PICTURE_ROW)].bg, Color::Rgb(r, g, b) if r > 200 && g < 50 && b < 50)
    })
}

/// A Markdown picture link to a video file shows the first frame of the video.
///
/// Why it matters: a page that links a screen recording showed only "cannot read" where the
/// docs site shows the video.
#[test]
fn shows_the_first_frame_of_a_linked_video_on_the_page() {
    // Given: a page that links a red video
    let dir = tempfile::tempdir().unwrap();
    if red_clip(dir.path(), "clip.mp4").is_none() {
        return;
    }
    let mut viewer = viewer(dir.path(), "intro\n\n![clip](clip.mp4)\n\nafter\n");

    // When: mdlive draws the page
    let page = draw(&mut viewer);

    // Then: the picture rows show the red frame
    assert!(shows_red(&page), "{:?}", row(&page, PICTURE_ROW));
}

/// An HTML video tag shows the first frame of its video, from its own `src` or from a `source`
/// inside it, unless text follows it in its paragraph.
///
/// Why it matters: GitHub READMEs embed recordings with a video tag, and mdlive showed the tag as
/// raw HTML.
#[test]
fn shows_the_first_frame_of_a_video_tag_on_the_page() {
    // Given: a red video
    let dir = tempfile::tempdir().unwrap();
    if red_clip(dir.path(), "clip.mkv").is_none() {
        return;
    }

    // When: a page names it in the src of a video tag
    let mut own = viewer(
        dir.path(),
        "intro\n\n<video src=\"clip.mkv\" controls></video>\n\nafter\n",
    );
    // And: another page names it in a source tag inside a video tag
    let mut inner = viewer(
        dir.path(),
        "intro\n\n<video controls>\n  <source src='clip.mkv' type='video/x-matroska'>\n</video>\n\nafter\n",
    );

    // And: a third page follows the video tag with a caption on the same line
    let mut captioned = viewer(
        dir.path(),
        "intro\n\n<video src=\"clip.mkv\"></video> A caption\n\nafter\n",
    );

    // Then: the first two pages show the red frame
    assert!(shows_red(&draw(&mut own)));
    assert!(shows_red(&draw(&mut inner)));
    // And: the third keeps its caption as text, on the two rows the paragraph wraps to
    let page = draw(&mut captioned);
    let text = format!(
        "{} {}",
        row(&page, PICTURE_ROW),
        row(&page, PICTURE_ROW + 1)
    );
    assert!(text.contains("caption"), "{text:?}");
}

/// Without ffmpeg, a video shows its alt text and says that ffmpeg is missing.
///
/// Why it matters: the reader must learn what to install, not see a picture that fails with no
/// reason.
#[test]
fn says_that_ffmpeg_is_missing_where_a_video_would_show() {
    // Given: a page that links a video, in a viewer with no ffmpeg
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("clip.mp4"), b"not read").unwrap();
    let mut viewer = viewer(dir.path(), "intro\n\n![clip](clip.mp4)\n\nafter\n").with_ffmpeg(None);

    // When: mdlive draws the page
    let page = draw(&mut viewer);

    // Then: the line of the video names its alt text and the missing ffmpeg
    let line = row(&page, PICTURE_ROW);
    assert!(line.contains("clip (ffmpeg not found)"), "{line:?}");
}

/// A file named as a video that is really an ffmpeg script, which names another file to read,
/// shows as a video that cannot be read.
///
/// Why it matters: ffmpeg picks how to read a file from what the file holds, not from its name,
/// and a concat script or a playlist makes ffmpeg open other files or addresses, while the README
/// says that opening a page makes no network call.
#[test]
fn refuses_a_video_that_is_an_ffmpeg_script() {
    // Given: a red video, and a page that links a concat script named as a video, which names it
    let dir = tempfile::tempdir().unwrap();
    if red_clip(dir.path(), "red.mkv").is_none() {
        return;
    }
    std::fs::write(
        dir.path().join("script.mp4"),
        "ffconcat version 1.0\nfile red.mkv\n",
    )
    .unwrap();
    let mut viewer = viewer(dir.path(), "intro\n\n![script](script.mp4)\n\nafter\n");

    // When: mdlive draws the page
    let page = draw(&mut viewer);

    // Then: the page shows no frame of the red video, and says ffmpeg cannot read the file
    assert!(!shows_red(&page));
    let line = row(&page, PICTURE_ROW);
    assert!(line.contains("script (ffmpeg cannot read"), "{line:?}");
}

/// The delay of each frame of a video in the view, which ffmpeg sends at 25 frames a second.
const FRAME: Duration = Duration::from_millis(40);

/// A viewer with the red and blue video open in the picture view.
pub fn playing(dir: &Path) -> Option<Viewer> {
    common::red_then_blue_clip(dir)?;
    let mut viewer = viewer(dir, "intro\n\n![clip](clip.mkv)\n\nafter\n");
    draw(&mut viewer);
    for key in "]p".chars() {
        viewer.handle_key(KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE));
    }
    viewer.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    Some(viewer)
}

fn press(viewer: &mut Viewer, key: char) {
    viewer.handle_key(KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE));
}

/// The colour of the middle of the view, the nearest of red and blue.
fn view_colour(viewer: &mut Viewer) -> &'static str {
    match draw(viewer)[(WIDTH / 2, HEIGHT / 2 - 1)].bg {
        Color::Rgb(r, _, b) if r > 200 && b < 50 => "red",
        Color::Rgb(r, _, b) if b > 200 && r < 50 => "blue",
        other => panic!("the middle of the view is {other:?}"),
    }
}

/// The picture view plays a video frame by frame at 25 frames a second, and starts it again after
/// its last frame.
///
/// Why it matters: a page that embeds a recording showed only its first frame, so the reader
/// could not watch it in mdlive.
#[test]
fn plays_the_frames_of_a_video_in_turn() {
    // Given: the red and blue video open in the view, on its first frame
    let dir = tempfile::tempdir().unwrap();
    let Some(mut viewer) = playing(dir.path()) else {
        return;
    };
    assert_eq!(view_colour(&mut viewer), "red");
    assert_eq!(viewer.until_next_frame(), Some(FRAME));

    // When: four frames pass
    for _ in 0..4 {
        assert!(viewer.play(FRAME));
    }

    // Then: the red part still shows
    assert_eq!(view_colour(&mut viewer), "red");

    // When: eight more frames pass, past the end of the red 0.4 s
    for _ in 0..8 {
        assert!(viewer.play(FRAME));
    }

    // Then: the blue part shows
    assert_eq!(view_colour(&mut viewer), "blue");

    // When: frames pass until red shows again
    let mut played = 12;
    while view_colour(&mut viewer) == "blue" && played < 400 {
        viewer.play(FRAME);
        played += 1;
    }

    // Then: red shows again only after the whole 10.4 s of the video, from its first frame
    assert_eq!(view_colour(&mut viewer), "red");
    assert!(played > 250, "red again after {played} frames");
}

/// Space pauses a video on its frame, and Space again plays it on from there.
///
/// Why it matters: a reader who wants to read one frame of a recording must stop it there.
#[test]
fn pauses_and_plays_a_video_with_space() {
    // Given: the red and blue video playing in the view
    let dir = tempfile::tempdir().unwrap();
    let Some(mut viewer) = playing(dir.path()) else {
        return;
    };

    // When: the reader pauses and a second passes
    press(&mut viewer, ' ');
    let shown = viewer.play(Duration::from_secs(1));

    // Then: no frame is due and the red frame stays
    assert!(!shown);
    assert_eq!(viewer.until_next_frame(), None);
    assert_eq!(view_colour(&mut viewer), "red");

    // When: the reader plays it again and twelve frames pass
    press(&mut viewer, ' ');
    for _ in 0..12 {
        viewer.play(FRAME);
    }

    // Then: the blue part shows
    assert_eq!(view_colour(&mut viewer), "blue");
}
