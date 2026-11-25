//! The menu bar along the top of the screen on macOS. It lists only what works. The table of
//! menus is plain data, so it can be checked on any computer; building the real menus needs
//! macOS.

#[cfg(any(target_os = "macos", test))]
use winit::keyboard::{KeyCode, ModifiersState};

use crate::i18n::{t, tk};

/// What a menu item does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    ExportSongs,
    ReloadSongs,
    Settings,
    InstallCommandLine,
    SetUp,
    Quit,
    NewSong,
    /// Import Songs › Song Files…: song documents, ChordPro and lyric files, songbooks.
    ImportSongs,
    /// Import Songs › A Library Folder…: each folder in it becomes a library.
    ImportLibraryFolder,
    /// Import Songs › A Song Bundle…: songs with the pictures and videos they show.
    ImportBundle,
    /// Import Songs › Words from Text…: text, rich-text and Word files, a song each.
    ImportWords,
    ImportSlides,
    NewSlide,
    CloseWindow,
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    Duplicate,
    Delete,
    SelectAll,
    DeselectAll,
    Find,
    Bold,
    Italic,
    Underline,
    Bigger,
    Smaller,
    AlignLeft,
    AlignCenter,
    AlignRight,
    Justify,
    UpperCase,
    LowerCase,
    Capitalize,
    CopyStyle,
    PasteStyle,
    BringForward,
    BringToFront,
    SendBackward,
    SendToBack,
    Lock,
    UnlockAll,
    InsertText,
    AlignObjects(u8),
    Flip(bool),
    NewWindowedOutput,
    PasteMatchStyle,
    InsertPicture,
    /// A shape, by its place in the list of shapes.
    InsertShape(u8),
    ShowView,
    EditView,
    BibleView,
    ThemesView,
    ReflowView,
    TimersTab,
    MessagesTab,
    PropsTab,
    MacrosTab,
    StageTab,
    AudioTab,
    MediaBin,
    ShowLiveSong,
    GridView,
    EasyView,
    TableView,
    NextSlide,
    PreviousSlide,
    ClearAll,
    ClearSlide,
    ClearMedia,
    ClearProps,
    ClearMessages,
    ClearAudio,
    ClearAnnouncements,
    Audience,
    Stage,
    Continuous,
    TestBars,
    TestGrid,
    TestWhite,
    IdentifyScreens,
    ConfigureScreens,
    StageLayouts,
    Website,
    ReportIssue,
    WhatsNew,
    /// Each button's key written beside it, all the time, or never (the key list has the same
    /// switch, for computers without this menu bar).
    KeysOnButtons,
    /// File › Import…: opens the import menu over the left column (songs by kind of file, slides).
    Import,
    /// Help › Keys…: the list of every key, as the top bar's ? opens it.
    KeysSheet,
    /// SoftPresenter › Check for Updates…: asks once whether there is a newer version, and opens
    /// Settings › About, where the answer shows (`updates.rs`).
    CheckForUpdates,
}

/// A keyboard shortcut. Every one has Command or Control in it, so none is ever taken from
/// typing or from the show's own keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shortcut {
    pub command: bool,
    pub control: bool,
    pub option: bool,
    pub shift: bool,
    /// A capital letter, a digit, or a comma.
    pub key: char,
}

const fn cmd(key: char) -> Option<Shortcut> {
    Some(Shortcut { command: true, control: false, option: false, shift: false, key })
}

const fn shift_cmd(key: char) -> Option<Shortcut> {
    Some(Shortcut { command: true, control: false, option: false, shift: true, key })
}

const fn option_cmd(key: char) -> Option<Shortcut> {
    Some(Shortcut { command: true, control: false, option: true, shift: false, key })
}

const fn option_shift_cmd(key: char) -> Option<Shortcut> {
    Some(Shortcut { command: true, control: false, option: true, shift: true, key })
}

const fn ctrl(key: char) -> Option<Shortcut> {
    Some(Shortcut { command: false, control: true, option: false, shift: false, key })
}

const fn ctrl_shift(key: char) -> Option<Shortcut> {
    Some(Shortcut { command: false, control: true, option: false, shift: true, key })
}

/// Items the system provides and runs itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum System {
    About,
    Services,
    Hide,
    HideOthers,
    ShowAll,
    Minimize,
    Zoom,
    FullScreen,
    BringAllToFront,
}

/// One line of a menu. Its words are in English, the name `sp menu` lists and chooses it by; the
/// menu bar draws them in the language that is set (`item_words`, `title_words`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Entry {
    /// An item that does something when chosen.
    Do(Action, &'static str, Option<Shortcut>),
    /// The same, with a tick beside it when it is on.
    Check(Action, &'static str, Option<Shortcut>),
    /// A group of choices that opens beside it.
    Sub(&'static str, &'static [Entry]),
    System(System),
    Line,
}

/// The choices of File › Import Songs, by kind of file; the import button over the left column
/// offers the same.
pub const IMPORT_SONGS: &[Entry] = &[
    Do(Action::ImportLibraryFolder, "A Library Folder\u{2026}", None),
    Do(Action::ImportBundle, "A Song Bundle\u{2026}", None),
    Do(Action::ImportSongs, "Song Files\u{2026}", None),
    Do(Action::ImportWords, "Words from Text\u{2026}", None),
];

use Entry::{Check, Do, Line};

/// Every command, grouped as the menu bar once had them. `sp menu` lists and chooses by these (so
/// a script can still do any of them), and each has its place in the window; the menu bar itself
/// is the shorter `MENUS`.
pub const COMMANDS: &[(&str, &[Entry])] = &[
    (
        "SoftPresenter",
        &[
            Entry::System(System::About),
            Do(Action::CheckForUpdates, "Check for Updates…", None),
            Line,
            Do(Action::Settings, "Settings…", cmd(',')),
            Do(Action::SetUp, "Set Up SoftPresenter…", None),
            Do(Action::InstallCommandLine, "Install Command Line Tool…", None),
            Line,
            Entry::System(System::Services),
            Line,
            Entry::System(System::Hide),
            Entry::System(System::HideOthers),
            Entry::System(System::ShowAll),
            Line,
            Do(Action::Quit, "Quit SoftPresenter", cmd('Q')),
        ],
    ),
    (
        "File",
        &[
            Do(Action::NewSong, "New Song", cmd('N')),
            Do(Action::NewSlide, "Add Slide", shift_cmd('N')),
            Entry::Sub("Import Songs", IMPORT_SONGS),
            Do(Action::ImportSlides, "Import Slides…", None),
            Do(Action::Import, "Import…", None),
            Line,
            Do(Action::ExportSongs, "Export Songs…", None),
            Do(Action::ReloadSongs, "Reload Songs from Their Files", None),
            Line,
            Do(Action::CloseWindow, "Close Window", cmd('W')),
        ],
    ),
    (
        "Edit",
        &[
            Do(Action::Undo, "Undo", cmd('Z')),
            Do(Action::Redo, "Redo", shift_cmd('Z')),
            Line,
            Do(Action::Cut, "Cut", cmd('X')),
            Do(Action::Copy, "Copy", cmd('C')),
            Do(Action::Paste, "Paste", cmd('V')),
            Do(Action::PasteMatchStyle, "Paste and Match Style", option_shift_cmd('V')),
            Do(Action::Duplicate, "Duplicate", cmd('D')),
            Do(Action::Delete, "Delete", None),
            Do(Action::SelectAll, "Select All", cmd('A')),
            Do(Action::DeselectAll, "Deselect All", shift_cmd('A')),
            Line,
            Do(Action::Find, "Find…", cmd('F')),
        ],
    ),
    (
        "Format",
        &[
            Do(Action::Bold, "Bold", cmd('B')),
            Do(Action::Italic, "Italic", cmd('I')),
            Do(Action::Underline, "Underline", cmd('U')),
            Line,
            Do(Action::Bigger, "Bigger", cmd('=')),
            Do(Action::Smaller, "Smaller", cmd('-')),
            Line,
            Do(Action::AlignLeft, "Text Left", shift_cmd('[')),
            Do(Action::AlignCenter, "Text Centre", shift_cmd('\\')),
            Do(Action::AlignRight, "Text Right", shift_cmd(']')),
            Do(Action::Justify, "Justify", option_shift_cmd('\\')),
            Line,
            Do(Action::UpperCase, "Make Upper Case", None),
            Do(Action::LowerCase, "Make Lower Case", None),
            Do(Action::Capitalize, "Capitalise", None),
            Line,
            Do(Action::CopyStyle, "Copy Style", option_cmd('C')),
            Do(Action::PasteStyle, "Paste Style", option_cmd('V')),
        ],
    ),
    (
        "Arrange",
        &[
            Do(Action::BringForward, "Bring Forward", option_shift_cmd('F')),
            Do(Action::BringToFront, "Bring to Front", shift_cmd('F')),
            Do(Action::SendBackward, "Send Backward", option_shift_cmd('B')),
            Do(Action::SendToBack, "Send to Back", shift_cmd('B')),
            Line,
            Do(Action::AlignObjects(0), "Align Left Edges", None),
            Do(Action::AlignObjects(1), "Align Centres", None),
            Do(Action::AlignObjects(2), "Align Right Edges", None),
            Do(Action::AlignObjects(3), "Align Tops", None),
            Do(Action::AlignObjects(4), "Align Middles", None),
            Do(Action::AlignObjects(5), "Align Bottoms", None),
            Line,
            Do(Action::Flip(true), "Flip Horizontally", None),
            Do(Action::Flip(false), "Flip Vertically", None),
            Line,
            Do(Action::Lock, "Lock", None),
            Do(Action::UnlockAll, "Unlock All", None),
        ],
    ),
    (
        "Insert",
        &[
            Do(Action::InsertText, "Text Box", None),
            Do(Action::InsertPicture, "Media…", shift_cmd('I')),
            Line,
            Do(Action::InsertShape(0), "Rounded Rect", None),
            Do(Action::InsertShape(1), "Rectangle", None),
            Do(Action::InsertShape(2), "Ellipse", None),
            Do(Action::InsertShape(3), "Triangle", None),
            Do(Action::InsertShape(4), "Right Triangle", None),
            Do(Action::InsertShape(5), "Rhombus", None),
            Do(Action::InsertShape(6), "Polygon", None),
            Do(Action::InsertShape(7), "Star", None),
            Do(Action::InsertShape(8), "Double Arrow", None),
            Do(Action::InsertShape(9), "Arrow", None),
        ],
    ),
    (
        "View",
        &[
            Check(Action::ShowView, "Service", cmd('1')),
            Check(Action::BibleView, "Bible", cmd('2')),
            Check(Action::EditView, "Edit", cmd('3')),
            Check(Action::ThemesView, "Themes", ctrl('T')),
            Do(Action::ReflowView, "Edit Words", ctrl('R')),
            Line,
            Check(Action::TimersTab, "Timers", ctrl('C')),
            Check(Action::MessagesTab, "Messages", ctrl('M')),
            Check(Action::PropsTab, "Props", ctrl('P')),
            Check(Action::MacrosTab, "Macros", ctrl_shift('M')),
            Check(Action::StageTab, "Stage", shift_cmd('S')),
            Check(Action::AudioTab, "Audio", ctrl('A')),
            Check(Action::MediaBin, "Media Bin", ctrl('V')),
            Line,
            Check(Action::Continuous, "Continuous", None),
            Line,
            Check(Action::GridView, "Grid View", None),
            Check(Action::EasyView, "Words Only", None),
            Check(Action::TableView, "Table View", None),
            Line,
            Entry::System(System::FullScreen),
        ],
    ),
    (
        "Presentation",
        &[
            Do(Action::NextSlide, "Next Slide", None),
            Do(Action::PreviousSlide, "Previous Slide", None),
            Line,
            Do(Action::ClearAll, "Clear All", None),
            Do(Action::ClearSlide, "Clear Slide", None),
            Do(Action::ClearMedia, "Clear Media", None),
            Do(Action::ClearProps, "Clear Props", None),
            Do(Action::ClearMessages, "Clear Messages", None),
            Do(Action::ClearAudio, "Clear Audio", None),
            Do(Action::ClearAnnouncements, "Clear Announcements", None),
            Line,
            Do(Action::ShowLiveSong, "Go to the Slide on Screen", cmd('L')),
        ],
    ),
    (
        "Screens",
        &[
            Check(Action::Audience, "Audience Screens", shift_cmd('1')),
            Check(Action::Stage, "Stage Screens", shift_cmd('2')),
            Check(Action::IdentifyScreens, "Identify Screens", None),
            Line,
            Check(Action::TestBars, "Test Pattern: Colour Bars", None),
            Check(Action::TestGrid, "Test Pattern: Grid", None),
            Check(Action::TestWhite, "Test Pattern: White", None),
            Line,
            Do(Action::ConfigureScreens, "Configure Screens…", option_cmd('1')),
            Do(Action::StageLayouts, "Edit Stage Layouts…", None),
        ],
    ),
    (
        "Window",
        &[
            Entry::System(System::Minimize),
            Entry::System(System::Zoom),
            Line,
            Do(Action::NewWindowedOutput, "New Windowed Output", None),
            Line,
            Entry::System(System::BringAllToFront),
        ],
    ),
    (
        "Help",
        &[
            Do(Action::Website, "SoftPresenter Help", None),
            Do(Action::WhatsNew, "What's New in SoftPresenter", None),
            Line,
            Check(Action::KeysOnButtons, "Show Keys on Buttons", None),
            Do(Action::KeysSheet, "Keys…", None),
            Line,
            Do(Action::ReportIssue, "Report an Issue…", None),
        ],
    ),
];

/// The menu bar: six menus with what a Mac user expects (the owner, 24 November: "keep only whats
/// needed and expected"). Everything else is in the window (`COMMANDS` names it all), and its keys
/// are read by the window (`Entry::System` items are the system's own).
pub const MENUS: &[(&str, &[Entry])] = &[
    (
        "SoftPresenter",
        &[
            Entry::System(System::About),
            Do(Action::CheckForUpdates, "Check for Updates…", None),
            Line,
            Do(Action::Settings, "Settings…", cmd(',')),
            Line,
            Entry::System(System::Services),
            Line,
            Entry::System(System::Hide),
            Entry::System(System::HideOthers),
            Entry::System(System::ShowAll),
            Line,
            Do(Action::Quit, "Quit SoftPresenter", cmd('Q')),
        ],
    ),
    (
        "File",
        &[
            Do(Action::NewSong, "New Song", cmd('N')),
            Do(Action::Import, "Import…", None),
            Line,
            Do(Action::CloseWindow, "Close Window", cmd('W')),
        ],
    ),
    (
        "Edit",
        &[
            Do(Action::Undo, "Undo", cmd('Z')),
            Do(Action::Redo, "Redo", shift_cmd('Z')),
            Line,
            Do(Action::Cut, "Cut", cmd('X')),
            Do(Action::Copy, "Copy", cmd('C')),
            Do(Action::Paste, "Paste", cmd('V')),
            Do(Action::PasteMatchStyle, "Paste and Match Style", option_shift_cmd('V')),
            Do(Action::Delete, "Delete", None),
            Do(Action::SelectAll, "Select All", cmd('A')),
            Line,
            Do(Action::Find, "Find…", cmd('F')),
        ],
    ),
    (
        "View",
        &[
            Check(Action::ShowView, "Service", cmd('1')),
            Check(Action::BibleView, "Bible", cmd('2')),
            Check(Action::EditView, "Edit", cmd('3')),
            Line,
            Entry::System(System::FullScreen),
        ],
    ),
    (
        "Window",
        &[Entry::System(System::Minimize), Entry::System(System::Zoom), Line, Entry::System(System::BringAllToFront)],
    ),
    (
        "Help",
        &[
            Do(Action::Website, "SoftPresenter Help", None),
            Do(Action::KeysSheet, "Keys…", None),
            Line,
            Do(Action::ReportIssue, "Report an Issue…", None),
        ],
    ),
];

/// Every item of the menu bar that does something, with its shortcut.
#[cfg(any(target_os = "macos", test))]
fn menu_items() -> Vec<(Action, Option<Shortcut>)> {
    fn add(entries: &[Entry], into: &mut Vec<(Action, Option<Shortcut>)>) {
        for entry in entries {
            match entry {
                Do(action, _, shortcut) | Check(action, _, shortcut) => into.push((*action, *shortcut)),
                Entry::Sub(_, inside) => add(inside, into),
                _ => {}
            }
        }
    }
    let mut every = Vec::new();
    for (_, entries) in MENUS {
        add(entries, &mut every);
    }
    every
}

/// Every line of the menus, with its menu, in menu order; a group's choices follow it.
fn every_entry() -> Vec<(&'static str, &'static Entry)> {
    fn add(menu: &'static str, entries: &'static [Entry], into: &mut Vec<(&'static str, &'static Entry)>) {
        for entry in entries {
            into.push((menu, entry));
            if let Entry::Sub(_, inside) = entry {
                add(menu, inside, into);
            }
        }
    }
    let mut every = Vec::new();
    for (menu, entries) in COMMANDS {
        add(menu, entries, &mut every);
    }
    every
}

/// Every item that does something, with the words on it, in menu order.
pub fn labelled_items() -> Vec<(Action, String)> {
    every_entry()
        .into_iter()
        .filter_map(|(_, entry)| match entry {
            Do(action, label, _) | Check(action, label, _) => Some((*action, (*label).to_owned())),
            _ => None,
        })
        .collect()
}

/// Every item that does something, with its shortcut.
fn items() -> impl Iterator<Item = (Action, Option<Shortcut>)> {
    every_entry().into_iter().filter_map(|(_, entry)| match entry {
        Do(action, _, shortcut) | Check(action, _, shortcut) => Some((*action, *shortcut)),
        _ => None,
    })
}

impl Shortcut {
    /// The shortcut as a Mac menu writes it: "⇧⌘F".
    pub fn text(self) -> String {
        let mut out = String::new();
        for (on, mark) in [(self.control, "⌃"), (self.option, "⌥"), (self.shift, "⇧"), (self.command, "⌘")] {
            if on {
                out.push_str(mark);
            }
        }
        out.push(self.key);
        out
    }
}

impl Shortcut {
    /// The shortcut as Windows and Linux write it: "Ctrl+Shift+F" (there the Command key is Ctrl,
    /// and a Control shortcut is Ctrl too), with Shift as the list of keys writes it, as the
    /// language's keyboards print it (Mayús in Spanish).
    pub fn words(self) -> String {
        let mut out = String::new();
        let shift = t("keys.cap.shift", "Shift");
        for (on, word) in [(self.command || self.control, "Ctrl"), (self.option, "Alt"), (self.shift, shift)] {
            if on {
                out.push_str(word);
                out.push('+');
            }
        }
        out.push(self.key);
        out
    }
}

/// The words of a line of the menu bar: its key in the tables and its English.
type Words = (&'static str, &'static str);

/// The words the menu bar draws for an item: every item of `MENUS` has them, with the same
/// English as the table (a test checks it). The items only `sp menu` has are never drawn, and
/// keep their English.
fn item_words(action: Action) -> Option<Words> {
    use Action as A;
    Some(match action {
        A::CheckForUpdates => tk("menubar.app.check_updates", "Check for Updates…"),
        A::Settings => tk("menubar.app.settings", "Settings…"),
        A::Quit => tk("menubar.app.quit", "Quit SoftPresenter"),
        A::NewSong => tk("menubar.file.new_song", "New Song"),
        A::Import => tk("menubar.file.import", "Import…"),
        A::CloseWindow => tk("menubar.file.close_window", "Close Window"),
        A::Undo => tk("menubar.edit.undo", "Undo"),
        A::Redo => tk("menubar.edit.redo", "Redo"),
        A::Cut => tk("menubar.edit.cut", "Cut"),
        A::Copy => tk("menubar.edit.copy", "Copy"),
        A::Paste => tk("menubar.edit.paste", "Paste"),
        A::PasteMatchStyle => tk("menubar.edit.paste_match_style", "Paste and Match Style"),
        A::Delete => tk("menubar.edit.delete", "Delete"),
        A::SelectAll => tk("menubar.edit.select_all", "Select All"),
        A::Find => tk("menubar.edit.find", "Find…"),
        A::ShowView => tk("menubar.view.service", "Service"),
        A::BibleView => tk("menubar.view.bible", "Bible"),
        A::EditView => tk("menubar.view.edit", "Edit"),
        A::Website => tk("menubar.help.website", "SoftPresenter Help"),
        A::KeysSheet => tk("menubar.help.keys", "Keys…"),
        A::ReportIssue => tk("menubar.help.report_issue", "Report an Issue…"),
        _ => return None,
    })
}

/// The words the menu bar draws for a menu's title. The code knows a menu by its English (the
/// Window and Help menus are handed to macOS by it). The first menu has none: macOS writes the
/// app's name there itself.
fn title_words(title: &str) -> Option<Words> {
    Some(match title {
        "File" => tk("menubar.file", "File"),
        "Edit" => tk("menubar.edit", "Edit"),
        "View" => tk("menubar.view", "View"),
        "Window" => tk("menubar.window", "Window"),
        "Help" => tk("menubar.help", "Help"),
        _ => return None,
    })
}

/// The words in the language that is set, or `english` where there are none.
fn drawn(words: Option<Words>, english: &'static str) -> &'static str {
    words.map_or(english, |(key, english)| t(key, english))
}

/// Every menu bar item that has a shortcut, with its menu, its name (both as the menu bar draws
/// them, in the language that is set) and the shortcut as this computer writes it, in menu order
/// (the list of keys shows them).
pub fn shortcuts() -> Vec<(&'static str, &'static str, String)> {
    fn add(menu: &'static str, entries: &'static [Entry], into: &mut Vec<(&'static str, &'static str, String)>) {
        for entry in entries {
            match entry {
                Entry::Do(action, label, Some(shortcut)) | Entry::Check(action, label, Some(shortcut)) => {
                    let text = if cfg!(target_os = "macos") { shortcut.text() } else { shortcut.words() };
                    into.push((menu, drawn(item_words(*action), label), text));
                }
                Entry::Sub(_, inside) => add(menu, inside, into),
                _ => {}
            }
        }
    }
    let mut every = Vec::new();
    for (menu, entries) in MENUS {
        add(drawn(title_words(menu), menu), entries, &mut every);
    }
    every
}

/// The id a menu item is known by.
pub fn id_of(action: Action) -> String {
    format!("{action:?}")
}

/// The action a menu item's id stands for.
pub fn action_of(id: &str) -> Option<Action> {
    items().map(|(action, _)| action).find(|action| id_of(*action) == id)
}

/// The key on the keyboard a shortcut's character is.
#[cfg(any(target_os = "macos", test))]
pub fn key_code(key: char) -> Option<KeyCode> {
    const LETTERS: [KeyCode; 26] = [
        KeyCode::KeyA,
        KeyCode::KeyB,
        KeyCode::KeyC,
        KeyCode::KeyD,
        KeyCode::KeyE,
        KeyCode::KeyF,
        KeyCode::KeyG,
        KeyCode::KeyH,
        KeyCode::KeyI,
        KeyCode::KeyJ,
        KeyCode::KeyK,
        KeyCode::KeyL,
        KeyCode::KeyM,
        KeyCode::KeyN,
        KeyCode::KeyO,
        KeyCode::KeyP,
        KeyCode::KeyQ,
        KeyCode::KeyR,
        KeyCode::KeyS,
        KeyCode::KeyT,
        KeyCode::KeyU,
        KeyCode::KeyV,
        KeyCode::KeyW,
        KeyCode::KeyX,
        KeyCode::KeyY,
        KeyCode::KeyZ,
    ];
    const DIGITS: [KeyCode; 10] = [
        KeyCode::Digit0,
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
        KeyCode::Digit7,
        KeyCode::Digit8,
        KeyCode::Digit9,
    ];
    match key {
        'A'..='Z' => LETTERS.get(usize::from(key as u8 - b'A')).copied(),
        '0'..='9' => DIGITS.get(usize::from(key as u8 - b'0')).copied(),
        ',' => Some(KeyCode::Comma),
        '=' => Some(KeyCode::Equal),
        '-' => Some(KeyCode::Minus),
        '[' => Some(KeyCode::BracketLeft),
        ']' => Some(KeyCode::BracketRight),
        '\\' => Some(KeyCode::Backslash),
        _ => None,
    }
}

/// The actions that change the objects selected in the editor, and only work there.
#[cfg(any(target_os = "macos", test))]
pub fn needs_editor(action: Action) -> bool {
    use Action as A;
    matches!(
        action,
        A::Duplicate
            | A::DeselectAll
            | A::Bold
            | A::Italic
            | A::Underline
            | A::Bigger
            | A::Smaller
            | A::AlignLeft
            | A::AlignCenter
            | A::AlignRight
            | A::Justify
            | A::UpperCase
            | A::LowerCase
            | A::Capitalize
            | A::CopyStyle
            | A::PasteStyle
            | A::BringForward
            | A::BringToFront
            | A::SendBackward
            | A::SendToBack
            | A::Lock
            | A::UnlockAll
            | A::InsertText
            | A::InsertPicture
            | A::InsertShape(_)
            | A::AlignObjects(_)
            | A::Flip(_)
    )
}

/// What decides which of the menus' items are greyed now (see [`enabled`]).
#[cfg(any(target_os = "macos", test))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Greying {
    /// Something is open in the editor (a slide, a template, a prop or a stage layout).
    pub editing: bool,
    /// The keyboard is in a field being typed in, or in a window of its own (Settings, say),
    /// which has an undo of its own the app can't see.
    pub typing: bool,
    /// The history in force has a step to undo, and one to redo (`HomeState::can_undo`).
    pub can_undo: bool,
    pub can_redo: bool,
    /// A look has been copied, for Paste Style.
    pub style_copied: bool,
    /// Check for Updates is running: its item is greyed, and says so.
    pub checking_updates: bool,
}

/// Whether a menu item can be chosen now. An item that only works in the editor is greyed
/// elsewhere; Undo and Redo are greyed when there is nothing to take back (never while typing,
/// as the field's own undo may have something); Paste Style is greyed until a look is copied.
/// A Mac menu item has no hover, so these say nothing more than their grey; the buttons in the
/// window that do the same say why.
#[cfg(any(target_os = "macos", test))]
pub fn enabled(action: Action, now: &Greying) -> bool {
    use Action as A;
    match action {
        A::Undo => now.typing || now.can_undo,
        A::Redo => now.typing || now.can_redo,
        A::PasteStyle => now.editing && now.style_copied,
        A::CheckForUpdates => !now.checking_updates,
        _ => now.editing || !needs_editor(action),
    }
}

/// True when a key pressed with these modifiers is one of the menus' shortcuts. Such a key
/// is left to the menu, so it is acted on once. An item that only works in the editor is
/// greyed out elsewhere and leaves its key to the window there (Command-B is Bold in the
/// editor and the Bible page elsewhere). Undo, Redo and Paste Style, greyed when they have
/// nothing to do, keep their keys: then the key does nothing, as the item would.
#[cfg(any(target_os = "macos", test))]
pub fn claims(key: KeyCode, held: ModifiersState, editing: bool) -> bool {
    let wanted = |action: &Action| editing || !needs_editor(*action);
    menu_items().into_iter().filter(|(action, _)| wanted(action)).filter_map(|(_, shortcut)| shortcut).any(|shortcut| {
        key_code(shortcut.key) == Some(key)
            && shortcut.command == held.super_key()
            && shortcut.control == held.control_key()
            && shortcut.option == held.alt_key()
            && shortcut.shift == held.shift_key()
    })
}

#[cfg(target_os = "macos")]
pub use mac::MenuBar;

#[cfg(target_os = "macos")]
mod mac {
    use muda::accelerator::{Accelerator, Code, Modifiers};
    use muda::{AboutMetadata, CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};

    use super::{
        Action, Entry, Greying, MENUS, Shortcut, System, Words, drawn, enabled, id_of, item_words, title_words,
    };
    use crate::i18n::{self, Language, t, tk};

    /// A line of the menus that has words, kept so they can follow the language.
    enum Worded {
        Item(MenuItem),
        Check(CheckMenuItem),
        Menu(Submenu),
        System(PredefinedMenuItem),
    }

    impl Worded {
        fn set_text(&self, text: &str) {
            match self {
                Worded::Item(item) => item.set_text(text),
                Worded::Check(item) => item.set_text(text),
                Worded::Menu(menu) => menu.set_text(text),
                Worded::System(item) => item.set_text(text),
            }
        }
    }

    /// What Check for Updates says while a check runs.
    fn checking_words() -> &'static str {
        t("menubar.app.checking_updates", "Checking for Updates…")
    }

    /// Our words for the items the system runs: muda's own are English. macOS may still name some
    /// itself (Full Screen, which it calls Enter Full Screen or Exit Full Screen), in its own
    /// language.
    fn system_words(system: System) -> Words {
        match system {
            System::About => tk("menubar.app.about", "About SoftPresenter"),
            System::Services => tk("menubar.app.services", "Services"),
            System::Hide => tk("menubar.app.hide", "Hide SoftPresenter"),
            System::HideOthers => tk("menubar.app.hide_others", "Hide Others"),
            System::ShowAll => tk("menubar.app.show_all", "Show All"),
            System::Minimize => tk("menubar.window.minimize", "Minimize"),
            System::Zoom => tk("menubar.window.zoom", "Zoom"),
            System::FullScreen => tk("menubar.view.full_screen", "Toggle Full Screen"),
            System::BringAllToFront => tk("menubar.window.bring_all_to_front", "Bring All to Front"),
        }
    }

    /// What a menu's lines are built into, and what the menu bar keeps of them.
    struct Built {
        checks: Vec<(Action, CheckMenuItem, Option<bool>)>,
        items: Vec<(Action, MenuItem, bool)>,
        /// Every line with words, and its words.
        words: Vec<(Worded, Words)>,
    }

    impl Built {
        fn keep_words(&mut self, line: Worded, words: Option<Words>) {
            if let Some(words) = words {
                self.words.push((line, words));
            }
        }
    }

    /// Builds one line of a menu (a group of choices with its own lines).
    fn item(entry: &Entry, built: &mut Built) -> Result<Box<dyn IsMenuItem>, String> {
        Ok(match entry {
            Entry::Do(action, label, shortcut) => {
                let words = item_words(*action);
                let item = MenuItem::with_id(id_of(*action), drawn(words, label), true, accelerator(*shortcut));
                built.items.push((*action, item.clone(), true));
                built.keep_words(Worded::Item(item.clone()), words);
                Box::new(item)
            }
            Entry::Check(action, label, shortcut) => {
                let words = item_words(*action);
                let item =
                    CheckMenuItem::with_id(id_of(*action), drawn(words, label), true, false, accelerator(*shortcut));
                built.checks.push((*action, item.clone(), Some(false)));
                built.keep_words(Worded::Check(item.clone()), words);
                Box::new(item)
            }
            Entry::Sub(title, entries) => {
                let words = title_words(title);
                let submenu = Submenu::new(drawn(words, title), true);
                for entry in *entries {
                    let inside = item(entry, built)?;
                    submenu.append(inside.as_ref()).map_err(|e| format!("Could not build the menu bar: {e}"))?;
                }
                built.keep_words(Worded::Menu(submenu.clone()), words);
                Box::new(submenu)
            }
            Entry::Line => Box::new(PredefinedMenuItem::separator()),
            Entry::System(system) => {
                let words = system_words(*system);
                let label = Some(t(words.0, words.1));
                let item = system_item(*system, label);
                built.keep_words(Worded::System(item.clone()), Some(words));
                Box::new(item)
            }
        })
    }

    /// An item the system runs, with our words on it.
    fn system_item(system: System, label: Option<&str>) -> PredefinedMenuItem {
        match system {
            System::About => PredefinedMenuItem::about(
                label,
                // The panel macOS shows reads the name and the version; the comments and the
                // website are other systems'. The version is the one the app shows (a
                // development build's 0.9.0-dev.15); the build number macOS would add after it in
                // brackets, the bundle's plain 0.9.0, is left blank (an empty one is, as AppKit
                // says), so it isn't "0.9.0-dev.15 (0.9.0)".
                Some(AboutMetadata {
                    name: Some("SoftPresenter".to_owned()),
                    version: Some(crate::updates::VERSION.to_owned()),
                    short_version: Some(String::new()),
                    comments: Some("A free, open-source presentation app for church services.".to_owned()),
                    website: Some("https://github.com/esraell/SoftPresenter".to_owned()),
                    ..AboutMetadata::default()
                }),
            ),
            System::Services => PredefinedMenuItem::services(label),
            System::Hide => PredefinedMenuItem::hide(label),
            System::HideOthers => PredefinedMenuItem::hide_others(label),
            System::ShowAll => PredefinedMenuItem::show_all(label),
            System::Minimize => PredefinedMenuItem::minimize(label),
            System::Zoom => PredefinedMenuItem::maximize(label),
            System::FullScreen => PredefinedMenuItem::fullscreen(label),
            System::BringAllToFront => PredefinedMenuItem::bring_all_to_front(label),
        }
    }

    fn code(key: char) -> Option<Code> {
        const LETTERS: [Code; 26] = [
            Code::KeyA,
            Code::KeyB,
            Code::KeyC,
            Code::KeyD,
            Code::KeyE,
            Code::KeyF,
            Code::KeyG,
            Code::KeyH,
            Code::KeyI,
            Code::KeyJ,
            Code::KeyK,
            Code::KeyL,
            Code::KeyM,
            Code::KeyN,
            Code::KeyO,
            Code::KeyP,
            Code::KeyQ,
            Code::KeyR,
            Code::KeyS,
            Code::KeyT,
            Code::KeyU,
            Code::KeyV,
            Code::KeyW,
            Code::KeyX,
            Code::KeyY,
            Code::KeyZ,
        ];
        // Every digit: a shortcut with one this table lacked got no key in the menu bar, while
        // the window still left that key to the menu (see `claims`), so it did nothing at all.
        // Command-3 (Edit) was lost that way.
        const DIGITS: [Code; 10] = [
            Code::Digit0,
            Code::Digit1,
            Code::Digit2,
            Code::Digit3,
            Code::Digit4,
            Code::Digit5,
            Code::Digit6,
            Code::Digit7,
            Code::Digit8,
            Code::Digit9,
        ];
        match key {
            'A'..='Z' => LETTERS.get(usize::from(key as u8 - b'A')).copied(),
            '0'..='9' => DIGITS.get(usize::from(key as u8 - b'0')).copied(),
            ',' => Some(Code::Comma),
            '=' => Some(Code::Equal),
            '-' => Some(Code::Minus),
            '[' => Some(Code::BracketLeft),
            ']' => Some(Code::BracketRight),
            '\\' => Some(Code::Backslash),
            _ => None,
        }
    }

    fn accelerator(shortcut: Option<Shortcut>) -> Option<Accelerator> {
        let shortcut = shortcut?;
        let mut mods = Modifiers::empty();
        for (on, modifier) in [
            (shortcut.command, Modifiers::META),
            (shortcut.control, Modifiers::CONTROL),
            (shortcut.option, Modifiers::ALT),
            (shortcut.shift, Modifiers::SHIFT),
        ] {
            if on {
                mods |= modifier;
            }
        }
        Some(Accelerator::new(mods, code(shortcut.key)?))
    }

    /// The menu bar, kept alive for as long as the app runs, and its ticked items.
    pub struct MenuBar {
        _menu: Menu,
        checks: Vec<(Action, CheckMenuItem, Option<bool>)>,
        /// The items that do something, and whether each is enabled now.
        items: Vec<(Action, MenuItem, bool)>,
        /// Every line with words (the menus' titles too), and its words.
        words: Vec<(Worded, Words)>,
        /// The language the words were last set in.
        language: Language,
    }

    impl MenuBar {
        /// Builds the menus and puts them in the menu bar. Call once the app has started.
        pub fn install() -> Result<MenuBar, String> {
            // The language is read before the words: should the computer's language come in
            // while they are built, the next `follow_language` sets them all again.
            let language = i18n::current();
            let menu = Menu::new();
            let mut built = Built { checks: Vec::new(), items: Vec::new(), words: Vec::new() };
            for (title, entries) in MENUS {
                let words = title_words(title);
                let submenu = Submenu::new(drawn(words, title), true);
                for entry in *entries {
                    let item = item(entry, &mut built)?;
                    submenu.append(item.as_ref()).map_err(|e| format!("Could not build the menu bar: {e}"))?;
                }
                menu.append(&submenu).map_err(|e| format!("Could not build the menu bar: {e}"))?;
                built.keep_words(Worded::Menu(submenu.clone()), words);
                match *title {
                    "Window" => submenu.set_as_windows_menu_for_nsapp(),
                    "Help" => submenu.set_as_help_menu_for_nsapp(),
                    _ => {}
                }
            }
            menu.init_for_nsapp();
            Ok(MenuBar { _menu: menu, checks: built.checks, items: built.items, words: built.words, language })
        }

        /// Puts the menus' words in the language that is set, which can change while the app
        /// runs (Settings › General). Cheap when it hasn't changed.
        pub fn follow_language(&mut self) {
            let now = i18n::current();
            if now == self.language {
                return;
            }
            self.language = now;
            for (line, (key, english)) in &self.words {
                line.set_text(t(key, english));
            }
            // Check for Updates says it is checking for as long as it is greyed.
            for (action, item, shown) in &self.items {
                if *action == Action::CheckForUpdates && !*shown {
                    item.set_text(checking_words());
                }
            }
        }

        /// Greys out the items that can't be chosen now (see [`enabled`]). Cheap when nothing
        /// changed.
        pub fn set_enabled(&mut self, now: &Greying) {
            for (action, item, shown) in &mut self.items {
                let wanted = enabled(*action, now);
                if *shown != wanted {
                    item.set_enabled(wanted);
                    *shown = wanted;
                    // Greyed, Check for Updates says it is checking.
                    if *action == Action::CheckForUpdates {
                        item.set_text(if wanted {
                            drawn(item_words(*action), "Check for Updates…")
                        } else {
                            checking_words()
                        });
                    }
                }
            }
        }

        /// Ticks the items that are on and clears the others. Cheap when nothing changed.
        pub fn set_checked(&mut self, on: impl Fn(Action) -> bool) {
            for (action, item, shown) in &mut self.checks {
                let now = on(*action);
                if *shown != Some(now) {
                    item.set_checked(now);
                    *shown = Some(now);
                }
            }
        }

        /// Forgets what the ticks were last set to, so the next `set_checked` sets them all: a
        /// click on a ticked item flips its tick whatever the app then does (Service, while
        /// already in the Service view, would have been left unticked).
        pub fn forget_ticks(&mut self) {
            for (_, _, shown) in &mut self.checks {
                *shown = None;
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn every_shortcut_the_window_leaves_to_the_menu_has_a_key_in_the_menu() {
            for (action, shortcut) in super::super::items() {
                let Some(shortcut) = shortcut else { continue };
                assert!(code(shortcut.key).is_some(), "{action:?}: {}", shortcut.key);
                assert!(accelerator(Some(shortcut)).is_some(), "{action:?}");
            }
            assert_eq!(code('3'), Some(Code::Digit3), "Command-3 is Edit");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shortcut_is_used_once_has_a_key_and_needs_command_or_control() {
        let shortcuts: Vec<Shortcut> = items().filter_map(|(_, shortcut)| shortcut).collect();
        for (at, shortcut) in shortcuts.iter().enumerate() {
            assert!(key_code(shortcut.key).is_some(), "{shortcut:?} has no key");
            assert!(shortcut.command || shortcut.control, "{shortcut:?} could be taken from typing");
            assert!(!shortcuts[..at].contains(shortcut), "{shortcut:?} is used twice");
        }
    }

    #[test]
    fn import_songs_offers_each_kind_of_file() {
        // The import menu over the left column (and `sp menu`) offers them by kind of file; the
        // menu bar's File menu has the one Import… that opens it.
        let words: Vec<&str> = IMPORT_SONGS
            .iter()
            .filter_map(|entry| if let Do(_, label, _) = entry { Some(*label) } else { None })
            .collect();
        assert_eq!(
            words,
            ["A Library Folder\u{2026}", "A Song Bundle\u{2026}", "Song Files\u{2026}", "Words from Text\u{2026}"]
        );
        // Each is found by its id, as `sp menu` and the import button over the left column find it.
        assert_eq!(action_of("ImportBundle"), Some(Action::ImportBundle));
        assert_eq!(action_of("ImportWords"), Some(Action::ImportWords));
        assert_eq!(action_of("ImportLibraryFolder"), Some(Action::ImportLibraryFolder));
        assert_eq!(action_of("Import"), Some(Action::Import));
    }

    #[test]
    fn the_menu_bar_is_six_short_menus_of_what_a_mac_user_expects() {
        let titles: Vec<&str> = MENUS.iter().map(|(title, _)| *title).collect();
        assert_eq!(titles, ["SoftPresenter", "File", "Edit", "View", "Window", "Help"]);
        let labels = |menu: &str| -> Vec<&str> {
            MENUS
                .iter()
                .find(|(title, _)| *title == menu)
                .expect(menu)
                .1
                .iter()
                .filter_map(|entry| match entry {
                    Do(_, label, _) | Check(_, label, _) => Some(*label),
                    _ => None,
                })
                .collect()
        };
        assert_eq!(labels("SoftPresenter"), ["Check for Updates\u{2026}", "Settings\u{2026}", "Quit SoftPresenter"]);
        // Check for Updates comes straight after About, as on other Mac apps.
        let app = MENUS[0].1;
        assert_eq!(&app[..2], &[Entry::System(System::About), Do(Action::CheckForUpdates, "Check for Updates…", None)]);
        assert_eq!(labels("File"), ["New Song", "Import\u{2026}", "Close Window"]);
        assert_eq!(labels("View"), ["Service", "Bible", "Edit"]);
        assert_eq!(labels("Help"), ["SoftPresenter Help", "Keys\u{2026}", "Report an Issue\u{2026}"]);
        // None of the View menu's Control keys (or anything else's) is left.
        assert!(menu_items().iter().filter_map(|(_, shortcut)| *shortcut).all(|shortcut| !shortcut.control));
        // Every menu item is one of the commands `sp menu` knows, with the same words and key.
        for (action, shortcut) in menu_items() {
            let command = every_entry().into_iter().find_map(|(_, entry)| match entry {
                Do(have, label, key) | Check(have, label, key) if *have == action => Some((*label, *key)),
                _ => None,
            });
            assert!(command.is_some(), "{action:?} is in the menu bar but not among the commands");
            assert_eq!(command.map(|(_, key)| key), Some(shortcut), "{action:?}");
        }
    }

    #[test]
    fn the_menu_bar_draws_every_line_in_the_language_and_sp_menu_keeps_the_english() {
        use crate::i18n::{Language, with_language};
        // Every title and item of the menu bar has its words, with the table's English.
        for (title, entries) in MENUS {
            if *title != "SoftPresenter" {
                assert_eq!(title_words(title).map(|(_, english)| english), Some(*title));
            }
            for entry in *entries {
                if let Do(action, label, _) | Check(action, label, _) = entry {
                    let words = item_words(*action);
                    assert_eq!(words.map(|(_, english)| english), Some(*label), "{action:?}");
                    assert!(words.is_some_and(|(key, _)| key.starts_with("menubar.")), "{action:?}");
                }
            }
        }
        // In Spanish the menu bar's words, and so the keys' list of its shortcuts, change; what
        // `sp menu` lists and chooses by doesn't.
        let english = with_language(Language::English, labelled_items);
        assert_eq!(with_language(Language::Spanish, labelled_items), english);
        let (english, spanish) =
            (with_language(Language::English, shortcuts), with_language(Language::Spanish, shortcuts));
        assert_eq!(english.len(), spanish.len());
        for (english, spanish) in english.iter().zip(&spanish) {
            assert_ne!(english.1, spanish.1, "{english:?}");
            if english.0 != "SoftPresenter" {
                assert_ne!(english.0, spanish.0, "{english:?}");
            }
        }
    }

    #[test]
    fn no_menu_has_two_dividers_together_or_one_at_an_end() {
        for (menu, entries) in
            MENUS.iter().copied().chain(COMMANDS.iter().copied()).chain([("Import Songs", IMPORT_SONGS)])
        {
            assert!(!matches!(entries.first(), Some(Entry::Line)), "{menu} starts with a divider");
            assert!(!matches!(entries.last(), Some(Entry::Line)), "{menu} ends with a divider");
            for pair in entries.windows(2) {
                assert!(!matches!(pair, [Entry::Line, Entry::Line]), "{menu} has two dividers together");
            }
        }
    }

    #[test]
    fn every_item_is_found_again_by_its_id_and_appears_once() {
        let actions: Vec<Action> = items().map(|(action, _)| action).collect();
        for (at, action) in actions.iter().enumerate() {
            assert_eq!(action_of(&id_of(*action)), Some(*action));
            assert!(!actions[..at].contains(action), "{action:?} is in the menus twice");
        }
        assert_eq!(action_of("Nothing"), None);
    }

    #[test]
    fn a_shortcut_is_claimed_only_with_exactly_its_modifiers() {
        let command = ModifiersState::SUPER;
        let claims = |key, held| claims(key, held, true);
        assert!(claims(KeyCode::KeyZ, command));
        assert!(claims(KeyCode::KeyZ, command | ModifiersState::SHIFT));
        assert!(claims(KeyCode::Comma, command));
        assert!(claims(KeyCode::KeyV, command | ModifiersState::SHIFT | ModifiersState::ALT), "Paste and Match Style");
        // Plain keys belong to typing and the show.
        assert!(!claims(KeyCode::KeyZ, ModifiersState::empty()));
        assert!(!claims(KeyCode::KeyS, ModifiersState::empty()));
        assert!(!claims(KeyCode::ArrowRight, ModifiersState::empty()));
        // Command-S is not a menu shortcut.
        assert!(!claims(KeyCode::KeyS, command));
        // Command-1, 2 and 3 are Service, Bible and Edit; 4 to 7 nothing.
        assert!(claims(KeyCode::Digit1, command) && claims(KeyCode::Digit3, command));
        assert!(!claims(KeyCode::Digit4, command));
        // The keys that went with the other menus are the window's own now, or gone: the tool
        // tabs' Option-1 to 7, Shift-Command-1 and 2 (the screens), Command-B, I, U and D (the
        // editor's), and the Control letters (View's, gone).
        assert!(!claims(KeyCode::Digit1, ModifiersState::ALT));
        assert!(!claims(KeyCode::Digit1, command | ModifiersState::SHIFT));
        assert!(!claims(KeyCode::Digit2, command | ModifiersState::SHIFT));
        assert!(!claims(KeyCode::KeyB, command) && !claims(KeyCode::KeyD, command));
        assert!(!claims(KeyCode::KeyT, ModifiersState::CONTROL));
        assert!(!claims(KeyCode::KeyM, command | ModifiersState::SHIFT), "Command-Shift-M is the media's numbers");
    }

    #[test]
    fn each_action_has_its_one_name_in_british_spelling() {
        let items = labelled_items();
        for (action, label) in &items {
            for american in ["Color", "Center", "Capitaliz"] {
                assert!(!label.contains(american), "{action:?}: {label}");
            }
        }
        let label = |action: Action| items.iter().find(|(have, _)| *have == action).map(|(_, label)| label.as_str());
        for (action, name) in [
            (Action::ReflowView, "Edit Words"),
            (Action::NewSlide, "Add Slide"),
            (Action::EasyView, "Words Only"),
            (Action::Continuous, "Continuous"),
            (Action::Find, "Find\u{2026}"),
            (Action::Audience, "Audience Screens"),
            (Action::Stage, "Stage Screens"),
            (Action::ShowLiveSong, "Go to the Slide on Screen"),
            (Action::AlignLeft, "Text Left"),
            (Action::AlignCenter, "Text Centre"),
            (Action::AlignRight, "Text Right"),
            (Action::Justify, "Justify"),
            (Action::AlignObjects(0), "Align Left Edges"),
            (Action::AlignObjects(1), "Align Centres"),
            (Action::AlignObjects(5), "Align Bottoms"),
            (Action::SendBackward, "Send Backward"),
            (Action::CopyStyle, "Copy Style"),
            (Action::Capitalize, "Capitalise"),
        ] {
            assert_eq!(label(action), Some(name), "{action:?}");
        }
    }

    #[test]
    fn what_only_works_in_the_editor_is_not_in_the_menu_bar_and_its_keys_are_the_windows() {
        let command = ModifiersState::SUPER;
        // Bold and Duplicate act on objects in the editor; the editor reads their keys itself.
        assert!(needs_editor(Action::Duplicate) && needs_editor(Action::Bold));
        assert!(menu_items().iter().all(|(action, _)| !needs_editor(*action)));
        assert!(!claims(KeyCode::KeyB, command, true) && !claims(KeyCode::KeyD, command, true));
        assert!(claims(KeyCode::KeyZ, command, false), "Undo is everywhere");
    }

    #[test]
    fn undo_redo_and_paste_style_are_greyed_when_they_have_nothing_to_do() {
        let none = Greying { editing: true, ..Greying::default() };
        assert!(!enabled(Action::Undo, &none) && !enabled(Action::Redo, &none), "nothing to take back");
        let undo = Greying { can_undo: true, ..none };
        assert!(enabled(Action::Undo, &undo) && !enabled(Action::Redo, &undo), "only Undo has a step");
        let redo = Greying { can_redo: true, ..none };
        assert!(!enabled(Action::Undo, &redo) && enabled(Action::Redo, &redo));
        // Typing, the field's own undo may have something the app can't see.
        let typing = Greying { typing: true, ..none };
        assert!(enabled(Action::Undo, &typing) && enabled(Action::Redo, &typing));
        // Paste Style: in the editor, once a look is copied.
        assert!(!enabled(Action::PasteStyle, &none), "nothing copied");
        assert!(enabled(Action::PasteStyle, &Greying { style_copied: true, ..none }));
        let elsewhere = Greying { style_copied: true, ..Greying::default() };
        assert!(!enabled(Action::PasteStyle, &elsewhere), "not outside the editor");
        // The rest as before: editor-only items greyed outside the editor, others always on.
        assert!(!enabled(Action::Bold, &Greying::default()) && enabled(Action::Bold, &none));
        assert!(enabled(Action::NewSong, &Greying::default()));
        // Check for Updates is greyed while a check runs, and only then.
        assert!(enabled(Action::CheckForUpdates, &Greying::default()));
        assert!(!enabled(Action::CheckForUpdates, &Greying { checking_updates: true, ..Greying::default() }));
    }
}
