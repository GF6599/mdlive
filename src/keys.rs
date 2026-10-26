use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// The letters no other key takes, home row first. `z` stays out because the fold keys start
/// with it.
const HINT_LETTERS: [char; 19] = [
    'a', 's', 'd', 'f', 'h', 'l', 'e', 'r', 'u', 'i', 'o', 'p', 'w', 'y', 'c', 'v', 'b', 'm', 'x',
];

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
    ToggleContents,
    NextHeading(usize),
    PreviousHeading(usize),
    NextBlock(usize),
    PreviousBlock(usize),
    /// A line of the Markdown source, counted from 1 as an editor counts it.
    SourceLine(usize),
    /// A heading of the contents, by its place in the list.
    Heading(usize),
}

/// The hint of each of `count` entries. Past one letter's worth, every hint takes two letters so
/// that no hint starts another. Entries past two letters' worth get none.
pub fn hints(count: usize) -> Vec<String> {
    if count <= HINT_LETTERS.len() {
        return HINT_LETTERS[..count].iter().map(char::to_string).collect();
    }
    HINT_LETTERS
        .iter()
        .flat_map(|first| {
            HINT_LETTERS
                .iter()
                .map(move |second| format!("{first}{second}"))
        })
        .take(count)
        .collect()
}

/// Turns key presses into commands, holding a count and a prefix key until the command is whole.
#[derive(Debug, Default)]
pub struct KeyReader {
    count: Option<usize>,
    prefix: Option<char>,
    /// How many contents entries the hint letters reach, 0 while the contents is closed.
    pub hint_targets: usize,
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
            let times = count.unwrap_or(1);
            return match (prefix, key.code) {
                ('g', KeyCode::Char('g')) => Some(count.map_or(Command::Top, Command::SourceLine)),
                (']', KeyCode::Char(']')) => Some(Command::NextHeading(times)),
                ('[', KeyCode::Char('[')) => Some(Command::PreviousHeading(times)),
                (first, KeyCode::Char(second)) => self.hint(&format!("{first}{second}")),
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

        if let KeyCode::Char(letter) = key.code
            && !control
            && self.hint_targets > 0
            && HINT_LETTERS.contains(&letter)
        {
            self.count = None;
            if self.hint_targets > HINT_LETTERS.len() {
                self.prefix = Some(letter);
                return None;
            }
            return self.hint(&letter.to_string());
        }

        let count = self.count.take();
        let times = count.unwrap_or(1);
        match key.code {
            KeyCode::Char('q') if !control => Some(Command::Quit),
            KeyCode::Char('t') if !control => Some(Command::ToggleContents),
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
            KeyCode::Char(prefix @ ('g' | ']' | '[')) if !control => {
                self.count = count;
                self.prefix = Some(prefix);
                None
            }
            KeyCode::Char('}') => Some(Command::NextBlock(times)),
            KeyCode::Char('{') => Some(Command::PreviousBlock(times)),
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

    fn hint(&self, typed: &str) -> Option<Command> {
        hints(self.hint_targets)
            .iter()
            .position(|hint| hint == typed)
            .map(Command::Heading)
    }

    fn clear(&mut self) {
        self.count = None;
        self.prefix = None;
    }
}
