use std::path::PathBuf;
use std::process::ExitCode;

use crossterm::event::{self, Event, KeyEventKind};
use mdview::{Flow, Viewer};

fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1);
    let (Some(path), None) = (args.next().map(PathBuf::from), args.next()) else {
        eprintln!("usage: mdview <file.md>");
        return ExitCode::from(2);
    };
    let markdown = match std::fs::read_to_string(&path) {
        Ok(markdown) => markdown,
        Err(error) => {
            eprintln!("mdview: {}: {error}", path.display());
            return ExitCode::FAILURE;
        }
    };
    let title = path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    );

    let mut terminal = ratatui::init();
    let result = run(&mut terminal, Viewer::new(&title, &markdown));
    ratatui::restore();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("mdview: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(terminal: &mut ratatui::DefaultTerminal, mut viewer: Viewer) -> std::io::Result<()> {
    loop {
        terminal.draw(|frame| viewer.draw(frame))?;
        if let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
            && viewer.handle_key(key) == Flow::Quit
        {
            return Ok(());
        }
    }
}
