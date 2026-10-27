#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::fs;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

use mdlive::{OpenFileWatch, watch_file};

const DEBOUNCE: Duration = Duration::from_millis(50);
/// Several debounce windows, so an event that was going to arrive has arrived.
const QUIET: Duration = Duration::from_millis(600);
const ARRIVAL: Duration = Duration::from_secs(5);

/// The watcher reports a write in place and a save by rename, and stays quiet for other files.
///
/// Why it matters: nvim saves by renaming a new file over the old one, which a watch on the file's
/// inode stops seeing after the first save, and a reload for every file in the folder would
/// redraw on each editor swap file.
#[test]
fn reports_saves_of_the_file_and_ignores_its_neighbours() {
    // Given: a watched file in a folder
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("doc.md");
    fs::write(&file, "one").unwrap();
    let (sender, changes) = mpsc::channel();
    let _watch = watch_file(&file, DEBOUNCE, move |change| {
        let _ = sender.send(change);
    })
    .unwrap();
    // FSEvents reports writes made just before a watch starts, so let those pass first.
    while changes.recv_timeout(QUIET).is_ok() {}

    // When: another file in the folder is written
    fs::write(dir.path().join("other.md"), "x").unwrap();
    // Then: nothing arrives in the quiet window
    assert_eq!(
        changes.recv_timeout(QUIET).err(),
        Some(RecvTimeoutError::Timeout)
    );

    // When: the file is written in place
    fs::write(&file, "two").unwrap();
    // Then: a change arrives
    assert_eq!(changes.recv_timeout(ARRIVAL), Ok(Ok(())));
    while changes.recv_timeout(QUIET).is_ok() {}

    // When: the file is saved the way nvim saves it, as a new file renamed over the old one
    let swap = dir.path().join("doc.md.new");
    fs::write(&swap, "three").unwrap();
    fs::rename(&swap, &file).unwrap();
    // Then: a change arrives
    assert_eq!(changes.recv_timeout(ARRIVAL), Ok(Ok(())));
}

/// A watch that follows the open file reports saves of the new file and none of the old one.
///
/// Why it matters: after the reader opens another page, a save of that page must reload it, and a
/// save of the page they left must not replace what they read.
#[test]
fn follows_the_file_the_reader_opens() {
    // Given: a watch on one file, moved to a second file
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("first.md");
    let second = dir.path().join("second.md");
    fs::write(&first, "one").unwrap();
    fs::write(&second, "one").unwrap();
    let (sender, changes) = mpsc::channel();
    let mut watch = OpenFileWatch::new(DEBOUNCE, move |change| {
        let _ = sender.send(change);
    });
    watch.follow(&first).unwrap();
    watch.follow(&second).unwrap();
    while changes.recv_timeout(QUIET).is_ok() {}

    // When: the first file is written
    fs::write(&first, "two").unwrap();
    // Then: nothing arrives in the quiet window
    assert_eq!(
        changes.recv_timeout(QUIET).err(),
        Some(RecvTimeoutError::Timeout)
    );

    // When: the second file is written
    fs::write(&second, "two").unwrap();
    // Then: a change arrives
    assert_eq!(changes.recv_timeout(ARRIVAL), Ok(Ok(())));
}
