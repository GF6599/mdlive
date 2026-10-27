use std::io::{Write, stdout};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use crossterm::cursor::{Hide, Show};
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind, MouseButton, MouseEventKind,
};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use mdlive::{Flow, LinkArea, OpenFileWatch, Viewer, folder_of, hyperlink_cells};
use ratatui::backend::Backend;
use ratatui_image::picker::Picker;

/// Long enough to fold the writes of one editor save into one reload, short enough to feel live.
const DEBOUNCE: Duration = Duration::from_millis(100);
/// How long a wait for a key lasts before the loop looks for a file change.
const POLL: Duration = Duration::from_millis(50);

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

    let (sender, changes) = mpsc::channel();
    let mut watch = OpenFileWatch::new(DEBOUNCE, move |change| {
        let _ = sender.send(change);
    });
    if !target.is_dir()
        && let Err(error) = watch.follow(&target)
    {
        eprintln!("mdlive: cannot watch {}: {error}", target.display());
        return ExitCode::FAILURE;
    }
    let mut terminal = ratatui::init();
    // The query reads the terminal's answer from stdin, so it runs before mouse capture, which
    // would mix mouse events into the answer.
    let picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());
    capture_mouse();
    let viewer = Viewer::new(&title, &markdown)
        .with_opener(open_in_system)
        .with_copier(copy_to_clipboard);
    let viewer = if target.is_dir() {
        viewer.with_images(&target, picker).with_tree(&target)
    } else {
        viewer
            .with_images(folder_of(&target), picker)
            .with_path(&target)
    };
    let result = run(&mut terminal, viewer, watch, &changes);
    release_mouse();
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
    let status = Command::new(opener)
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|error| error.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{opener} failed with {status}"))
    }
}

/// Puts `text` on the clipboard: through `pbcopy` on macOS, which needs nothing from the terminal,
/// and elsewhere through an OSC 52 code, which the terminal passes to the clipboard.
fn copy_to_clipboard(text: &str) -> Result<(), String> {
    if !cfg!(target_os = "macos") {
        let code = format!("\x1b]52;c;{}\x07", STANDARD.encode(text));
        let mut terminal = stdout();
        return terminal
            .write_all(code.as_bytes())
            .and_then(|()| terminal.flush())
            .map_err(|error| error.to_string());
    }
    let mut pbcopy = Command::new("pbcopy")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("cannot start pbcopy: {error}"))?;
    let written = pbcopy
        .stdin
        .take()
        .map_or(Ok(()), |mut input| input.write_all(text.as_bytes()));
    let status = pbcopy.wait().map_err(|error| error.to_string())?;
    written.map_err(|error| error.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("pbcopy failed with {status}"))
    }
}

/// Moves the watch to the file on the page, which a key or a click may have changed.
fn follow_open_file(viewer: &mut Viewer, watch: &mut OpenFileWatch) {
    if let Some(path) = viewer.path()
        && let Err(error) = watch.follow(path)
    {
        let message = format!("cannot watch the file: {error}");
        viewer.show_error(&message);
    }
}

/// The wheel and the left button. Every other event, such as each move of the pointer, would only
/// cost a redraw.
fn is_used(kind: MouseEventKind) -> bool {
    matches!(
        kind,
        MouseEventKind::ScrollDown
            | MouseEventKind::ScrollUp
            | MouseEventKind::Down(MouseButton::Left)
    )
}

/// Asks the terminal for mouse events. The panic hook that `ratatui::init` installed restores the
/// screen but not the mouse, so a hook in front of it gives the mouse back first, or a crash leaves
/// the shell printing mouse codes.
fn capture_mouse() {
    let restore_screen = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        release_mouse();
        restore_screen(info);
    }));
    enable_mouse();
}

fn enable_mouse() {
    let _ = crossterm::execute!(stdout(), EnableMouseCapture);
}

fn release_mouse() {
    let _ = crossterm::execute!(stdout(), DisableMouseCapture);
}

/// Draws the viewer, then waits for a key, a click or a file change, on this one thread. No other
/// thread reads the terminal, so while the editor runs, every key goes to the editor.
fn run(
    terminal: &mut ratatui::DefaultTerminal,
    mut viewer: Viewer,
    mut watch: OpenFileWatch,
    changes: &Receiver<Result<(), String>>,
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
        let flow = loop {
            if event::poll(POLL)? {
                match event::read()? {
                    Event::Key(key) if key.kind == KeyEventKind::Press => {
                        break viewer.handle_key(key);
                    }
                    Event::Mouse(mouse) if is_used(mouse.kind) => break viewer.handle_mouse(mouse),
                    Event::Resize(..) => break Flow::Continue,
                    _ => {}
                }
            }
            if let Ok(change) = changes.try_recv() {
                reload(&mut viewer, change);
                break Flow::Continue;
            }
        };
        match flow {
            Flow::Quit => return Ok(()),
            Flow::Edit(path, line) => {
                edit(terminal, &mut viewer, &path, line)?;
                links.clear();
            }
            Flow::Continue => {}
        }
        follow_open_file(&mut viewer, &mut watch);
    }
}

fn reload(viewer: &mut Viewer, change: Result<(), String>) {
    let Some(path) = viewer.path() else {
        return;
    };
    match change.and_then(|()| std::fs::read_to_string(path).map_err(|error| error.to_string())) {
        Ok(markdown) => viewer.reload(&markdown),
        Err(error) => viewer.show_error(&format!("cannot read the file: {error}")),
    }
}

/// Hands the terminal to the editor in `$VISUAL` or `$EDITOR` on `path` at `line`, the way git
/// starts one: the setting may carry its own arguments, and the file goes as an argument the shell
/// does not parse. A save reloads the page through the watch.
fn edit(
    terminal: &mut ratatui::DefaultTerminal,
    viewer: &mut Viewer,
    path: &Path,
    line: usize,
) -> std::io::Result<()> {
    let Some(editor) = ["VISUAL", "EDITOR"].iter().find_map(|name| {
        std::env::var(name)
            .ok()
            .filter(|value| !value.trim().is_empty())
    }) else {
        viewer.show_error("set $EDITOR to edit the file");
        return Ok(());
    };
    release_mouse();
    disable_raw_mode()?;
    crossterm::execute!(stdout(), LeaveAlternateScreen, Show)?;
    let status = Command::new("sh")
        .arg("-c")
        .arg(format!("{editor} \"$@\""))
        .arg("mdlive")
        .arg(format!("+{line}"))
        .arg(path)
        .status();
    enable_raw_mode()?;
    crossterm::execute!(stdout(), EnterAlternateScreen, Hide)?;
    enable_mouse();
    terminal.clear()?;
    match status {
        Ok(status) if status.success() => {}
        Ok(status) => viewer.show_error(&format!("the editor exited with {status}")),
        Err(error) => viewer.show_error(&format!("cannot start the editor: {error}")),
    }
    Ok(())
}
