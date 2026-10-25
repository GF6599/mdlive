use std::ffi::OsString;
use std::path::Path;
use std::time::Duration;

use notify_debouncer_mini::notify::{self, RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{DebounceEventResult, Debouncer, new_debouncer};

/// Watches one file until dropped.
pub struct FileWatch {
    _debouncer: Debouncer<RecommendedWatcher>,
}

impl std::fmt::Debug for FileWatch {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("FileWatch")
    }
}

/// Calls `on_change` once per burst of writes to the file at `path`, or with the watcher's error.
///
/// It watches the folder and matches the file name. inotify and kqueue follow a file's inode, so
/// when an editor saves by renaming a new file over the old one, a watch on the file itself is
/// left on the deleted original.
pub fn watch_file(
    path: &Path,
    debounce: Duration,
    mut on_change: impl FnMut(Result<(), String>) + Send + 'static,
) -> notify::Result<FileWatch> {
    let name: OsString = path
        .file_name()
        .ok_or_else(|| notify::Error::generic("the path names no file"))?
        .to_owned();
    let folder = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    let mut debouncer = new_debouncer(debounce, move |result: DebounceEventResult| match result {
        Ok(events) => {
            if events
                .iter()
                .any(|event| event.path.file_name() == Some(&name))
            {
                on_change(Ok(()));
            }
        }
        Err(error) => on_change(Err(error.to_string())),
    })?;
    debouncer
        .watcher()
        .watch(folder, RecursiveMode::NonRecursive)?;
    Ok(FileWatch {
        _debouncer: debouncer,
    })
}
