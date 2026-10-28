use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// The hint letters a key list names before it trails off, as the contents shows them all.
pub const HINT_LETTERS_LISTED: usize = 3;
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
    ToggleHelp,
    Back,
    Forward,
    ToggleTree,
    OpenEntry,
    CloseFolder,
    FilterTree,
    NextLink(usize),
    PreviousLink(usize),
    NextPicture(usize),
    PreviousPicture(usize),
    /// Enter: follow the marked link, or open the marked picture.
    OpenMark,
    ZoomIn(usize),
    ZoomOut(usize),
    ZoomFit,
    /// Moves the part of the picture shown, in eighths of the page.
    Pan {
        across: i32,
        down: i32,
    },
    ClosePicture,
    Edit,
    SelectLines,
    CopySelection,
    /// Escape with no keys typed toward a command, which leaves whatever has the keys.
    Cancel,
}

/// What the keys act on: the page, the file tree while it has them, the selected page lines in
/// visual mode, or a picture open on the whole page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Focus {
    #[default]
    Page,
    Tree,
    Visual,
    Picture,
}

/// The focuses a binding acts in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Scope {
    page: bool,
    tree: bool,
    visual: bool,
    picture: bool,
}

impl Scope {
    const fn reaches(self, focus: Focus) -> bool {
        match focus {
            Focus::Page => self.page,
            Focus::Tree => self.tree,
            Focus::Visual => self.visual,
            Focus::Picture => self.picture,
        }
    }
}

/// A heading of the help screen, in the order the screen shows them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Move,
    Sections,
    Links,
    Pictures,
    Search,
    Folds,
    Select,
    Files,
    Other,
}

impl Group {
    pub const ALL: [Self; 9] = [
        Self::Move,
        Self::Sections,
        Self::Links,
        Self::Pictures,
        Self::Search,
        Self::Folds,
        Self::Select,
        Self::Files,
        Self::Other,
    ];

    pub const fn title(self) -> &'static str {
        match self {
            Self::Move => "Move",
            Self::Sections => "Sections",
            Self::Links => "Links",
            Self::Pictures => "Pictures",
            Self::Search => "Search",
            Self::Folds => "Folds",
            Self::Select => "Select",
            Self::Files => "Files",
            Self::Other => "Other",
        }
    }
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
    ToggleHelp,
    Back,
    Forward,
    ToggleTree,
    OpenEntry,
    CloseFolder,
    FilterTree,
    NextLink,
    PreviousLink,
    NextPicture,
    PreviousPicture,
    OpenMark,
    ZoomIn,
    ZoomOut,
    ZoomFit,
    PanLeft,
    PanDown,
    PanUp,
    PanRight,
    ClosePicture,
    Edit,
    SelectLines,
    CopySelection,
}

impl Action {
    /// What the action does, in the words a key list shows.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Quit => "quit",
            Self::Down => "scroll down",
            Self::Up => "scroll up",
            Self::HalfPageDown => "half page down",
            Self::HalfPageUp => "half page up",
            Self::PageDown => "page down",
            Self::PageUp => "page up",
            Self::Top => "go to top or line N",
            Self::End => "go to end or line N",
            Self::ToggleContents => "open or close contents",
            Self::NextHeading => "next section",
            Self::PreviousHeading => "previous section",
            Self::NextBlock => "next paragraph",
            Self::PreviousBlock => "previous paragraph",
            Self::SearchForward => "search forward",
            Self::SearchBackward => "search backward",
            Self::NextMatch => "next match",
            Self::PreviousMatch => "previous match",
            Self::ToggleFold => "open or close fold",
            Self::OpenAllFolds => "open all folds",
            Self::CloseAllFolds => "close all folds",
            Self::ToggleHelp => "show the keys",
            Self::Back => "back to the last file",
            Self::Forward => "forward again",
            Self::ToggleTree => "open, enter or close the file tree",
            Self::OpenEntry => "open folder or read file, in the tree",
            Self::CloseFolder => "close folder, in the tree",
            Self::FilterTree => "filter the files, in the tree",
            Self::NextLink => "mark the next link",
            Self::PreviousLink => "mark the previous link",
            Self::NextPicture => "mark the next picture",
            Self::PreviousPicture => "mark the previous picture",
            Self::OpenMark => "follow the marked link, or open the marked picture",
            Self::ZoomIn => "zoom in, in a picture",
            Self::ZoomOut => "zoom out, in a picture",
            Self::ZoomFit => "fit the picture to the page",
            Self::PanLeft => "pan left, in a picture",
            Self::PanDown => "pan down, in a picture",
            Self::PanUp => "pan up, in a picture",
            Self::PanRight => "pan right, in a picture",
            Self::ClosePicture => "close the picture",
            Self::Edit => "edit the file at this line",
            Self::SelectLines => "select lines",
            Self::CopySelection => "copy the source of the selected lines",
        }
    }

    const fn group(self) -> Group {
        match self {
            Self::NextHeading | Self::PreviousHeading | Self::ToggleContents => Group::Sections,
            Self::SearchForward | Self::SearchBackward | Self::NextMatch | Self::PreviousMatch => {
                Group::Search
            }
            Self::ToggleFold | Self::OpenAllFolds | Self::CloseAllFolds => Group::Folds,
            Self::Back
            | Self::Forward
            | Self::ToggleTree
            | Self::OpenEntry
            | Self::CloseFolder
            | Self::FilterTree
            | Self::Edit => Group::Files,
            Self::NextLink | Self::PreviousLink | Self::OpenMark => Group::Links,
            Self::NextPicture
            | Self::PreviousPicture
            | Self::ZoomIn
            | Self::ZoomOut
            | Self::ZoomFit
            | Self::PanLeft
            | Self::PanDown
            | Self::PanUp
            | Self::PanRight
            | Self::ClosePicture => Group::Pictures,
            Self::SelectLines | Self::CopySelection => Group::Select,
            Self::Quit | Self::ToggleHelp => Group::Other,
            _ => Group::Move,
        }
    }

    fn command(self, count: Option<usize>) -> Command {
        let times = count.unwrap_or(1);
        let steps = i32::try_from(times).unwrap_or(i32::MAX);
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
            Self::ToggleHelp => Command::ToggleHelp,
            Self::Back => Command::Back,
            Self::Forward => Command::Forward,
            Self::ToggleTree => Command::ToggleTree,
            Self::OpenEntry => Command::OpenEntry,
            Self::CloseFolder => Command::CloseFolder,
            Self::FilterTree => Command::FilterTree,
            Self::NextLink => Command::NextLink(times),
            Self::PreviousLink => Command::PreviousLink(times),
            Self::NextPicture => Command::NextPicture(times),
            Self::PreviousPicture => Command::PreviousPicture(times),
            Self::OpenMark => Command::OpenMark,
            Self::ZoomIn => Command::ZoomIn(times),
            Self::ZoomOut => Command::ZoomOut(times),
            Self::ZoomFit => Command::ZoomFit,
            Self::PanLeft => Command::Pan {
                across: -steps,
                down: 0,
            },
            Self::PanDown => Command::Pan {
                across: 0,
                down: steps,
            },
            Self::PanUp => Command::Pan {
                across: 0,
                down: -steps,
            },
            Self::PanRight => Command::Pan {
                across: steps,
                down: 0,
            },
            Self::ClosePicture => Command::ClosePicture,
            Self::Edit => Command::Edit,
            Self::SelectLines => Command::SelectLines,
            Self::CopySelection => Command::CopySelection,
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

    pub const fn named(code: KeyCode) -> Self {
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
            (KeyCode::Enter, _) => String::from("Enter"),
            (KeyCode::Right, _) => String::from("→"),
            (KeyCode::Left, _) => String::from("←"),
            (code, _) => code.to_string(),
        }
    }
}

struct Binding {
    keys: &'static [Key],
    action: Action,
    scope: Scope,
}

const fn scoped(keys: &'static [Key], action: Action, scope: Scope) -> Binding {
    Binding {
        keys,
        action,
        scope,
    }
}

const NONE: Scope = Scope {
    page: false,
    tree: false,
    visual: false,
    picture: false,
};

/// A key of the page and the tree.
const fn bind(keys: &'static [Key], action: Action) -> Binding {
    let scope = Scope {
        page: true,
        tree: true,
        ..NONE
    };
    scoped(keys, action, scope)
}

/// A movement key, which also moves the cursor of a selection.
const fn move_key(keys: &'static [Key], action: Action) -> Binding {
    let scope = Scope {
        page: true,
        tree: true,
        visual: true,
        ..NONE
    };
    scoped(keys, action, scope)
}

/// A movement key of the page, which also moves the cursor of a selection.
const fn page_move(keys: &'static [Key], action: Action) -> Binding {
    let scope = Scope {
        page: true,
        visual: true,
        ..NONE
    };
    scoped(keys, action, scope)
}

const fn page(keys: &'static [Key], action: Action) -> Binding {
    let scope = Scope { page: true, ..NONE };
    scoped(keys, action, scope)
}

const fn tree(keys: &'static [Key], action: Action) -> Binding {
    let scope = Scope { tree: true, ..NONE };
    scoped(keys, action, scope)
}

const fn visual(keys: &'static [Key], action: Action) -> Binding {
    let scope = Scope {
        visual: true,
        ..NONE
    };
    scoped(keys, action, scope)
}

const fn picture(keys: &'static [Key], action: Action) -> Binding {
    let scope = Scope {
        picture: true,
        ..NONE
    };
    scoped(keys, action, scope)
}

/// Every key sequence the reader accepts. The first binding of an action is the one a hint shows.
const BINDINGS: &[Binding] = &[
    bind(&[Key::plain('q')], Action::Quit),
    move_key(&[Key::plain('j')], Action::Down),
    move_key(&[Key::control('e')], Action::Down),
    move_key(&[Key::named(KeyCode::Down)], Action::Down),
    move_key(&[Key::plain('k')], Action::Up),
    move_key(&[Key::control('y')], Action::Up),
    move_key(&[Key::named(KeyCode::Up)], Action::Up),
    move_key(&[Key::control('d')], Action::HalfPageDown),
    move_key(&[Key::control('u')], Action::HalfPageUp),
    move_key(&[Key::control('f')], Action::PageDown),
    move_key(&[Key::plain(' ')], Action::PageDown),
    move_key(&[Key::named(KeyCode::PageDown)], Action::PageDown),
    move_key(&[Key::control('b')], Action::PageUp),
    move_key(&[Key::named(KeyCode::PageUp)], Action::PageUp),
    move_key(&[Key::plain('g'), Key::plain('g')], Action::Top),
    move_key(&[Key::named(KeyCode::Home)], Action::Top),
    move_key(&[Key::plain('G')], Action::End),
    move_key(&[Key::named(KeyCode::End)], Action::End),
    page_move(&[Key::plain(']'), Key::plain(']')], Action::NextHeading),
    page_move(&[Key::plain('['), Key::plain('[')], Action::PreviousHeading),
    page(&[Key::plain(']'), Key::plain('l')], Action::NextLink),
    page(&[Key::plain('['), Key::plain('l')], Action::PreviousLink),
    page(&[Key::plain(']'), Key::plain('p')], Action::NextPicture),
    page(&[Key::plain('['), Key::plain('p')], Action::PreviousPicture),
    page(&[Key::named(KeyCode::Enter)], Action::OpenMark),
    page_move(&[Key::plain('}')], Action::NextBlock),
    page_move(&[Key::plain('{')], Action::PreviousBlock),
    page(&[Key::plain('t')], Action::ToggleContents),
    page(&[Key::plain('/')], Action::SearchForward),
    page(&[Key::plain('?')], Action::SearchBackward),
    page(&[Key::plain('n')], Action::NextMatch),
    page(&[Key::plain('N')], Action::PreviousMatch),
    page(&[Key::plain('z'), Key::plain('a')], Action::ToggleFold),
    page(&[Key::plain('z'), Key::plain('R')], Action::OpenAllFolds),
    page(&[Key::plain('z'), Key::plain('M')], Action::CloseAllFolds),
    bind(&[Key::plain('g'), Key::plain('?')], Action::ToggleHelp),
    bind(&[Key::named(KeyCode::F(1))], Action::ToggleHelp),
    bind(&[Key::control('o')], Action::Back),
    bind(&[Key::control('i')], Action::Forward),
    bind(&[Key::named(KeyCode::Tab)], Action::Forward),
    bind(&[Key::plain('-')], Action::ToggleTree),
    page(&[Key::plain('E')], Action::Edit),
    page(&[Key::plain('V')], Action::SelectLines),
    visual(&[Key::plain('y')], Action::CopySelection),
    tree(&[Key::plain('l')], Action::OpenEntry),
    tree(&[Key::named(KeyCode::Enter)], Action::OpenEntry),
    tree(&[Key::named(KeyCode::Right)], Action::OpenEntry),
    tree(&[Key::plain('h')], Action::CloseFolder),
    tree(&[Key::named(KeyCode::Left)], Action::CloseFolder),
    tree(&[Key::plain('/')], Action::FilterTree),
    picture(&[Key::plain('+')], Action::ZoomIn),
    picture(&[Key::plain('=')], Action::ZoomIn),
    picture(&[Key::plain('-')], Action::ZoomOut),
    picture(&[Key::plain('0')], Action::ZoomFit),
    picture(&[Key::plain('h')], Action::PanLeft),
    picture(&[Key::named(KeyCode::Left)], Action::PanLeft),
    picture(&[Key::plain('j')], Action::PanDown),
    picture(&[Key::named(KeyCode::Down)], Action::PanDown),
    picture(&[Key::plain('k')], Action::PanUp),
    picture(&[Key::named(KeyCode::Up)], Action::PanUp),
    picture(&[Key::plain('l')], Action::PanRight),
    picture(&[Key::named(KeyCode::Right)], Action::PanRight),
    picture(&[Key::plain('q')], Action::ClosePicture),
];

/// Quits from anywhere, even partway through a key sequence.
const QUIT_ANYWHERE: Key = Key::control('c');
/// Drops the keys typed toward a command, or leaves whatever has the keys when there are none.
pub const CANCEL: Key = Key::named(KeyCode::Esc);

fn bindings_for(focus: Focus) -> impl Iterator<Item = &'static Binding> {
    BINDINGS
        .iter()
        .filter(move |binding| binding.scope.reaches(focus))
}

fn action_of(keys: &[Key], focus: Focus) -> Option<Action> {
    bindings_for(focus)
        .find(|binding| binding.keys == keys)
        .map(|binding| binding.action)
}

fn starts_a_sequence(key: Key, focus: Focus) -> bool {
    bindings_for(focus).any(|binding| binding.keys.len() > 1 && binding.keys[0] == key)
}

/// The keys that can follow `prefix` under `focus`, each with what it does, in the order of the
/// table.
pub fn next_keys(prefix: Key, focus: Focus) -> Vec<(Key, Action)> {
    bindings_for(focus)
        .filter_map(|binding| match binding.keys {
            [first, second] if *first == prefix => Some((*second, binding.action)),
            _ => None,
        })
        .collect()
}

/// Every key of `group` with what it does, one row per action, in the order of the table.
pub fn key_list(group: Group) -> Vec<(String, &'static str)> {
    let mut rows: Vec<(Action, Vec<String>)> = Vec::new();
    for binding in BINDINGS
        .iter()
        .filter(|binding| binding.action.group() == group)
    {
        let keys: String = binding.keys.iter().map(|key| key.label()).collect();
        match rows
            .iter_mut()
            .find(|(action, _)| *action == binding.action)
        {
            Some((_, all)) => all.push(keys),
            None => rows.push((binding.action, vec![keys])),
        }
    }
    let mut list: Vec<_> = rows
        .into_iter()
        .map(|(action, keys)| (keys.join(", "), action.label()))
        .collect();
    match group {
        Group::Sections => list.push((
            format!("{}, …", hints(HINT_LETTERS_LISTED).join(", ")),
            "go to a section, with the contents open",
        )),
        Group::Other => list.extend([
            (QUIT_ANYWHERE.label(), Action::Quit.label()),
            (
                CANCEL.label(),
                "drop typed keys, leave the tree, help or picture, or quit",
            ),
        ]),
        _ => {}
    }
    list
}

/// The keys of the first binding of `action`, as a hint shows them, such as `gg` or `C-d`.
pub fn keys_of(action: Action) -> String {
    BINDINGS
        .iter()
        .find(|binding| binding.action == action)
        .map(|binding| binding.keys.iter().map(|key| key.label()).collect())
        .unwrap_or_default()
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
    pub focus: Focus,
}

impl KeyReader {
    pub fn read(&mut self, event: KeyEvent) -> Option<Command> {
        let key = Key::of(event);
        if key == QUIT_ANYWHERE {
            return Some(Command::Quit);
        }
        if key == CANCEL {
            let pending = self.count.is_some() || self.prefix.is_some();
            self.clear();
            return (!pending).then_some(Command::Cancel);
        }
        if let Some(prefix) = self.prefix.take() {
            let count = self.count.take();
            if let Some(action) = action_of(&[prefix, key], self.focus) {
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
            && self.focus == Focus::Page
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
        if let Some(action) = action_of(&[key], self.focus) {
            return Some(action.command(count));
        }
        if starts_a_sequence(key, self.focus) {
            self.count = count;
            self.prefix = Some(key);
        }
        None
    }

    /// The first key of a sequence that is not whole yet.
    pub fn prefix(&self) -> Option<Key> {
        self.prefix
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
