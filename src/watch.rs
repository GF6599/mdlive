use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;
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

/// Watches the file the reader has open, and moves the watch when they open another.
pub struct OpenFileWatch {
    path: PathBuf,
    debounce: Duration,
    on_change: Arc<dyn Fn(Result<(), String>) + Send + Sync>,
    _watch: FileWatch,
}

impl std::fmt::Debug for OpenFileWatch {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OpenFileWatch")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl OpenFileWatch {
    pub fn new(
        path: &Path,
        debounce: Duration,
        on_change: impl Fn(Result<(), String>) + Send + Sync + 'static,
    ) -> notify::Result<Self> {
        let on_change: Arc<dyn Fn(Result<(), String>) + Send + Sync> = Arc::new(on_change);
        Ok(Self {
            path: path.to_path_buf(),
            debounce,
            _watch: watch_shared(path, debounce, &on_change)?,
            on_change,
        })
    }

    /// Watches `path` from now on. On an error the watch stays on the file it had.
    pub fn follow(&mut self, path: &Path) -> notify::Result<()> {
        if path != self.path {
            self._watch = watch_shared(path, self.debounce, &self.on_change)?;
            self.path = path.to_path_buf();
        }
        Ok(())
    }
}

fn watch_shared(
    path: &Path,
    debounce: Duration,
    on_change: &Arc<dyn Fn(Result<(), String>) + Send + Sync>,
) -> notify::Result<FileWatch> {
    let on_change = Arc::clone(on_change);
    watch_file(path, debounce, move |change| on_change(change))
}
