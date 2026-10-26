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
    Search {
        forward: bool,
    },
    ToggleFold,
    OpenAllFolds,
    CloseAllFolds,
    RepeatSearch {
        reverse: bool,
        times: usize,
    },
}

/// What a binding does, before the count typed ahead of it turns it into a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Quit,
    Down,
    Up,
    HalfPageDown,
    HalfPageUp,
    PageDown,
    PageUp,
    Top,
    End,
    ToggleContents,
    NextHeading,
    PreviousHeading,
    NextBlock,
    PreviousBlock,
    SearchForward,
    SearchBackward,
    NextMatch,
    PreviousMatch,
    ToggleFold,
    OpenAllFolds,
    CloseAllFolds,
}

impl Action {
    fn command(self, count: Option<usize>) -> Command {
        let times = count.unwrap_or(1);
        match self {
            Self::Quit => Command::Quit,
            Self::Down => Command::Down(times),
            Self::Up => Command::Up(times),
            Self::HalfPageDown => Command::HalfPageDown(times),
            Self::HalfPageUp => Command::HalfPageUp(times),
            Self::PageDown => Command::PageDown(times),
            Self::PageUp => Command::PageUp(times),
            Self::Top => count.map_or(Command::Top, Command::SourceLine),
            Self::End => count.map_or(Command::End, Command::SourceLine),
            Self::ToggleContents => Command::ToggleContents,
            Self::NextHeading => Command::NextHeading(times),
            Self::PreviousHeading => Command::PreviousHeading(times),
            Self::NextBlock => Command::NextBlock(times),
            Self::PreviousBlock => Command::PreviousBlock(times),
            Self::SearchForward => Command::Search { forward: true },
            Self::SearchBackward => Command::Search { forward: false },
            Self::NextMatch => Command::RepeatSearch {
                reverse: false,
                times,
            },
            Self::PreviousMatch => Command::RepeatSearch {
                reverse: true,
                times,
            },
            Self::ToggleFold => Command::ToggleFold,
            Self::OpenAllFolds => Command::OpenAllFolds,
            Self::CloseAllFolds => Command::CloseAllFolds,
        }
    }
}

/// One key press as a binding names it. Shift is left out, because the terminal already sends the
/// shifted character, such as `G`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Key {
    code: KeyCode,
    control: bool,
}

impl Key {
    const fn plain(letter: char) -> Self {
        Self::named(KeyCode::Char(letter))
    }

    const fn control(letter: char) -> Self {
        Self {
            code: KeyCode::Char(letter),
            control: true,
        }
    }

    const fn named(code: KeyCode) -> Self {
        Self {
            code,
            control: false,
        }
    }

    fn of(event: KeyEvent) -> Self {
        Self {
            code: event.code,
            control: event.modifiers.contains(KeyModifiers::CONTROL),
        }
    }

    pub fn label(self) -> String {
        match (self.code, self.control) {
            (KeyCode::Char(' '), _) => String::from("Space"),
            (KeyCode::Char(letter), true) => format!("C-{letter}"),
            (KeyCode::Char(letter), false) => letter.to_string(),
            (KeyCode::Down, _) => String::from("↓"),
            (KeyCode::Up, _) => String::from("↑"),
            (KeyCode::PageDown, _) => String::from("PgDn"),
            (KeyCode::PageUp, _) => String::from("PgUp"),
            (code, _) => code.to_string(),
        }
    }
}

struct Binding {
    keys: &'static [Key],
    action: Action,
}

const fn bind(keys: &'static [Key], action: Action) -> Binding {
    Binding { keys, action }
}

/// Every key sequence the reader accepts.
const BINDINGS: &[Binding] = &[
    bind(&[Key::plain('q')], Action::Quit),
    bind(&[Key::plain('j')], Action::Down),
    bind(&[Key::control('e')], Action::Down),
    bind(&[Key::named(KeyCode::Down)], Action::Down),
    bind(&[Key::plain('k')], Action::Up),
    bind(&[Key::control('y')], Action::Up),
    bind(&[Key::named(KeyCode::Up)], Action::Up),
    bind(&[Key::control('d')], Action::HalfPageDown),
    bind(&[Key::control('u')], Action::HalfPageUp),
    bind(&[Key::control('f')], Action::PageDown),
    bind(&[Key::plain(' ')], Action::PageDown),
    bind(&[Key::named(KeyCode::PageDown)], Action::PageDown),
    bind(&[Key::control('b')], Action::PageUp),
    bind(&[Key::named(KeyCode::PageUp)], Action::PageUp),
    bind(&[Key::plain('g'), Key::plain('g')], Action::Top),
    bind(&[Key::named(KeyCode::Home)], Action::Top),
    bind(&[Key::plain('G')], Action::End),
    bind(&[Key::named(KeyCode::End)], Action::End),
    bind(&[Key::plain(']'), Key::plain(']')], Action::NextHeading),
    bind(&[Key::plain('['), Key::plain('[')], Action::PreviousHeading),
    bind(&[Key::plain('}')], Action::NextBlock),
    bind(&[Key::plain('{')], Action::PreviousBlock),
    bind(&[Key::plain('t')], Action::ToggleContents),
    bind(&[Key::plain('/')], Action::SearchForward),
    bind(&[Key::plain('?')], Action::SearchBackward),
    bind(&[Key::plain('n')], Action::NextMatch),
    bind(&[Key::plain('N')], Action::PreviousMatch),
    bind(&[Key::plain('z'), Key::plain('a')], Action::ToggleFold),
    bind(&[Key::plain('z'), Key::plain('R')], Action::OpenAllFolds),
    bind(&[Key::plain('z'), Key::plain('M')], Action::CloseAllFolds),
];

fn action_of(keys: &[Key]) -> Option<Action> {
    BINDINGS
        .iter()
        .find(|binding| binding.keys == keys)
        .map(|binding| binding.action)
}

fn starts_a_sequence(key: Key) -> bool {
    BINDINGS
        .iter()
        .any(|binding| binding.keys.len() > 1 && binding.keys[0] == key)
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
    prefix: Option<Key>,
    /// How many contents entries the hint letters reach, 0 while the contents is closed.
    pub hint_targets: usize,
}

impl KeyReader {
    pub fn read(&mut self, event: KeyEvent) -> Option<Command> {
        let key = Key::of(event);
        if key == Key::control('c') {
            return Some(Command::Quit);
        }
        if key.code == KeyCode::Esc {
            let pending = self.count.is_some() || self.prefix.is_some();
            self.clear();
            return (!pending).then_some(Command::Quit);
        }
        if let Some(prefix) = self.prefix.take() {
            let count = self.count.take();
            if let Some(action) = action_of(&[prefix, key]) {
                return Some(action.command(count));
            }
            return match (prefix.code, key.code) {
                (KeyCode::Char(first), KeyCode::Char(second)) => {
                    self.hint(&format!("{first}{second}"))
                }
                _ => None,
            };
        }
        if let KeyCode::Char(digit @ '0'..='9') = key.code
            && !key.control
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
            && !key.control
            && self.hint_targets > 0
            && HINT_LETTERS.contains(&letter)
        {
            self.count = None;
            if self.hint_targets > HINT_LETTERS.len() {
                self.prefix = Some(key);
                return None;
            }
            return self.hint(&letter.to_string());
        }

        let count = self.count.take();
        if let Some(action) = action_of(&[key]) {
            return Some(action.command(count));
        }
        if starts_a_sequence(key) {
            self.count = count;
            self.prefix = Some(key);
        }
        None
    }

    /// The keys typed toward a command that is not whole yet, as vim's showcmd shows them.
    pub fn pending(&self) -> String {
        let count = self
            .count
            .map(|count| count.to_string())
            .unwrap_or_default();
        let prefix = self.prefix.map(Key::label).unwrap_or_default();
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
