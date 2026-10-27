use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use crossterm::event::{self, Event, KeyEvent, KeyEventKind};
use mdlive::{Flow, LinkArea, OpenFileWatch, Viewer, folder_of, hyperlink_cells};
use ratatui::backend::Backend;
use ratatui_image::picker::Picker;

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
    let (target, None) = (args.next().map(PathBuf::from), args.next()) else {
        eprintln!("usage: mdlive [file.md | folder]");
        return ExitCode::from(2);
    };
    let target = target.unwrap_or_else(|| PathBuf::from("."));
    let name = target.canonicalize().unwrap_or_else(|_| target.clone());
    let title = name.file_name().map_or_else(
        || target.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    );
    let markdown = if target.is_dir() {
        String::new()
    } else {
        match std::fs::read_to_string(&target) {
            Ok(markdown) => markdown,
            Err(error) => {
                eprintln!("mdlive: {}: {error}", target.display());
                return ExitCode::FAILURE;
            }
        }
    };

    let (sender, inputs) = mpsc::channel();
    let changes = sender.clone();
    let mut watch = OpenFileWatch::new(DEBOUNCE, move |change| {
        let _ = changes.send(Input::Changed(change));
    });
    if !target.is_dir()
        && let Err(error) = watch.follow(&target)
    {
        eprintln!("mdlive: cannot watch {}: {error}", target.display());
        return ExitCode::FAILURE;
    }
    let mut terminal = ratatui::init();
    // The query reads the terminal's answer from stdin, so it runs before the key reader starts.
    let picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());
    std::thread::spawn(move || read_terminal(&sender));
    let viewer = Viewer::new(&title, &markdown).with_opener(open_in_system);
    let viewer = if target.is_dir() {
        viewer.with_images(&target, picker).with_tree(&target)
    } else {
        viewer
            .with_images(folder_of(&target), picker)
            .with_path(&target)
    };
    let result = run(&mut terminal, viewer, watch, &inputs);
    ratatui::restore();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("mdlive: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Shows a web or mail address in the program the system picks for it. The address goes as one
/// argument with no shell, and the program's output stays off the screen mdlive draws.
fn open_in_system(url: &str) -> Result<(), String> {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let status = std::process::Command::new(opener)
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|error| error.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{opener} failed with {status}"))
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
    mut watch: OpenFileWatch,
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
                if let Some(path) = viewer.path()
                    && let Err(error) = watch.follow(path)
                {
                    let message = format!("cannot watch the file: {error}");
                    viewer.show_error(&message);
                }
            }
            Input::Resize => {}
            Input::Changed(Ok(())) => {
                if let Some(path) = viewer.path() {
                    match std::fs::read_to_string(path) {
                        Ok(markdown) => viewer.reload(&markdown),
                        Err(error) => viewer.show_error(&format!("cannot read the file: {error}")),
                    }
                }
            }
            Input::Changed(Err(error)) => viewer.show_error(&format!("watch failed: {error}")),
            Input::Closed(error) => return Err(error),
        }
    }
}
