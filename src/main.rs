use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use crossterm::event::{self, Event, KeyEvent, KeyEventKind};
use mdlive::{Flow, LinkArea, Viewer, hyperlink_cells, watch_file};
use ratatui::backend::Backend;

/// Long enough to fold the writes of one editor save into one reload, short enough to feel live.
const DEBOUNCE: Duration = Duration::from_millis(100);

enum Input {
    Key(KeyEvent),
    Resize,
    Changed(Result<(), String>),
    Closed(std::io::Error),
}

fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1);
    let (Some(path), None) = (args.next().map(PathBuf::from), args.next()) else {
        eprintln!("usage: mdlive <file.md>");
        return ExitCode::from(2);
    };
    let markdown = match std::fs::read_to_string(&path) {
        Ok(markdown) => markdown,
        Err(error) => {
            eprintln!("mdlive: {}: {error}", path.display());
            return ExitCode::FAILURE;
        }
    };
    let title = path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    );

    let (sender, inputs) = mpsc::channel();
    let changes = sender.clone();
    let _watch = match watch_file(&path, DEBOUNCE, move |change| {
        let _ = changes.send(Input::Changed(change));
    }) {
        Ok(watch) => watch,
        Err(error) => {
            eprintln!("mdlive: cannot watch {}: {error}", path.display());
            return ExitCode::FAILURE;
        }
    };
    std::thread::spawn(move || read_terminal(&sender));

    let mut terminal = ratatui::init();
    let result = run(
        &mut terminal,
        Viewer::new(&title, &markdown),
        &path,
        &inputs,
    );
    ratatui::restore();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("mdlive: {error}");
            ExitCode::FAILURE
        }
    }
}

fn read_terminal(sender: &Sender<Input>) {
    loop {
        let input = match event::read() {
            Ok(Event::Key(key)) if key.kind == KeyEventKind::Press => Input::Key(key),
            Ok(Event::Resize(..)) => Input::Resize,
            Ok(_) => continue,
            Err(error) => Input::Closed(error),
        };
        let closed = matches!(input, Input::Closed(_));
        if sender.send(input).is_err() || closed {
            return;
        }
    }
}

fn run(
    terminal: &mut ratatui::DefaultTerminal,
    mut viewer: Viewer,
    path: &Path,
    inputs: &Receiver<Input>,
) -> std::io::Result<()> {
    let mut links: Vec<LinkArea> = Vec::new();
    loop {
        let frame = terminal.draw(|frame| viewer.draw(frame))?;
        let cells = hyperlink_cells(frame.buffer, viewer.links(), &links);
        if !cells.is_empty() {
            let backend = terminal.backend_mut();
            backend.draw(cells.iter().map(|(x, y, cell)| (*x, *y, cell)))?;
            Backend::flush(backend)?;
        }
        links = viewer.links().to_vec();
        let Ok(input) = inputs.recv() else {
            return Ok(());
        };
        match input {
            Input::Key(key) => {
                if viewer.handle_key(key) == Flow::Quit {
                    return Ok(());
                }
            }
            Input::Resize => {}
            Input::Changed(Ok(())) => match std::fs::read_to_string(path) {
                Ok(markdown) => viewer.reload(&markdown),
                Err(error) => viewer.show_error(&format!("cannot read the file: {error}")),
            },
            Input::Changed(Err(error)) => viewer.show_error(&format!("watch failed: {error}")),
            Input::Closed(error) => return Err(error),
        }
    }
}
