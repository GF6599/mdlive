use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Quit,
    Down(usize),
    Up(usize),
    HalfPageDown(usize),
    HalfPageUp(usize),
    PageDown(usize),
    PageUp(usize),
    Top,
    End,
    /// A line of the Markdown source, counted from 1 as an editor counts it.
    SourceLine(usize),
}

/// Turns key presses into commands, holding a count and a prefix key until the command is whole.
#[derive(Debug, Default)]
pub struct KeyReader {
    count: Option<usize>,
    prefix: Option<char>,
}

impl KeyReader {
    pub fn read(&mut self, key: KeyEvent) -> Option<Command> {
        let control = key.modifiers.contains(KeyModifiers::CONTROL);
        if control && key.code == KeyCode::Char('c') {
            return Some(Command::Quit);
        }
        if key.code == KeyCode::Esc {
            let pending = self.count.is_some() || self.prefix.is_some();
            self.clear();
            return (!pending).then_some(Command::Quit);
        }
        if let Some(prefix) = self.prefix.take() {
            let count = self.count.take();
            return match (prefix, key.code) {
                ('g', KeyCode::Char('g')) => Some(count.map_or(Command::Top, Command::SourceLine)),
                _ => None,
            };
        }
        if let KeyCode::Char(digit @ '0'..='9') = key.code
            && !control
            && (digit != '0' || self.count.is_some())
        {
            let value = usize::from(digit as u8 - b'0');
            self.count = Some(
                self.count
                    .unwrap_or(0)
                    .saturating_mul(10)
                    .saturating_add(value),
            );
            return None;
        }

        let count = self.count.take();
        let times = count.unwrap_or(1);
        match key.code {
            KeyCode::Char('q') if !control => Some(Command::Quit),
            KeyCode::Char('j') if !control => Some(Command::Down(times)),
            KeyCode::Char('e') if control => Some(Command::Down(times)),
            KeyCode::Down => Some(Command::Down(times)),
            KeyCode::Char('k') if !control => Some(Command::Up(times)),
            KeyCode::Char('y') if control => Some(Command::Up(times)),
            KeyCode::Up => Some(Command::Up(times)),
            KeyCode::Char('d') if control => Some(Command::HalfPageDown(times)),
            KeyCode::Char('u') if control => Some(Command::HalfPageUp(times)),
            KeyCode::Char('f') if control => Some(Command::PageDown(times)),
            KeyCode::Char(' ') | KeyCode::PageDown => Some(Command::PageDown(times)),
            KeyCode::Char('b') if control => Some(Command::PageUp(times)),
            KeyCode::PageUp => Some(Command::PageUp(times)),
            KeyCode::Char('g') if !control => {
                self.count = count;
                self.prefix = Some('g');
                None
            }
            KeyCode::Char('G') if !control => Some(count.map_or(Command::End, Command::SourceLine)),
            KeyCode::Home => Some(Command::Top),
            KeyCode::End => Some(Command::End),
            _ => None,
        }
    }

    /// The keys typed toward a command that is not whole yet, as vim's showcmd shows them.
    pub fn pending(&self) -> String {
        let count = self
            .count
            .map(|count| count.to_string())
            .unwrap_or_default();
        let prefix = self.prefix.map(String::from).unwrap_or_default();
        count + &prefix
    }

    fn clear(&mut self) {
        self.count = None;
        self.prefix = None;
    }
}
