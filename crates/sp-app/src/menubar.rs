//! The menu bar along the top of the screen on macOS. It lists only what works. The table of
//! menus is plain data, so it can be checked on any computer; building the real menus needs
//! macOS.

use winit::keyboard::{KeyCode, ModifiersState};

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

/// One line of a menu.
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

/// The menus, in order. The first is the app's own menu.
pub const MENUS: &[(&str, &[Entry])] = &[
    (
        "SoftPresenter",
        &[
            Entry::System(System::About),
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
            Line,
            Do(Action::ReportIssue, "Report an Issue…", None),
        ],
    ),
];

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
    for (menu, entries) in MENUS {
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

/// Every menu item that has a shortcut, with its menu, its name and the shortcut, in menu order.
pub fn shortcuts() -> Vec<(&'static str, &'static str, String)> {
    every_entry()
        .into_iter()
        .filter_map(|(menu, entry)| match entry {
            Entry::Do(_, label, Some(shortcut)) | Entry::Check(_, label, Some(shortcut)) => {
                Some((menu, *label, shortcut.text()))
            }
            _ => None,
        })
        .collect()
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
}

/// Whether a menu item can be chosen now. An item that only works in the editor is greyed
/// elsewhere; Undo and Redo are greyed when there is nothing to take back (never while typing,
/// as the field's own undo may have something); Paste Style is greyed until a look is copied.
/// A Mac menu item has no hover, so these say nothing more than their grey; the buttons in the
/// window that do the same say why.
pub fn enabled(action: Action, now: &Greying) -> bool {
    use Action as A;
    match action {
        A::Undo => now.typing || now.can_undo,
        A::Redo => now.typing || now.can_redo,
        A::PasteStyle => now.editing && now.style_copied,
        _ => now.editing || !needs_editor(action),
    }
}

/// True when a key pressed with these modifiers is one of the menus' shortcuts. Such a key
/// is left to the menu, so it is acted on once. An item that only works in the editor is
/// greyed out elsewhere and leaves its key to the window there (Command-B is Bold in the
/// editor and the Bible page elsewhere). Undo, Redo and Paste Style, greyed when they have
/// nothing to do, keep their keys: then the key does nothing, as the item would.
pub fn claims(key: KeyCode, held: ModifiersState, editing: bool) -> bool {
    let wanted = |action: &Action| editing || !needs_editor(*action);
    items().filter(|(action, _)| wanted(action)).filter_map(|(_, shortcut)| shortcut).any(|shortcut| {
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

    use super::{Action, Entry, Greying, MENUS, Shortcut, System, enabled, id_of};

    /// What a menu's lines are built into, and what the menu bar keeps of them.
    struct Built {
        checks: Vec<(Action, CheckMenuItem, bool)>,
        items: Vec<(Action, MenuItem, bool)>,
    }

    /// Builds one line of a menu (a group of choices with its own lines).
    fn item(entry: &Entry, built: &mut Built) -> Result<Box<dyn IsMenuItem>, String> {
        Ok(match entry {
            Entry::Do(action, label, shortcut) => {
                let item = MenuItem::with_id(id_of(*action), *label, true, accelerator(*shortcut));
                built.items.push((*action, item.clone(), true));
                Box::new(item)
            }
            Entry::Check(action, label, shortcut) => {
                let item = CheckMenuItem::with_id(id_of(*action), *label, true, false, accelerator(*shortcut));
                built.checks.push((*action, item.clone(), false));
                Box::new(item)
            }
            Entry::Sub(title, entries) => {
                let submenu = Submenu::new(*title, true);
                for entry in *entries {
                    let inside = item(entry, built)?;
                    submenu.append(inside.as_ref()).map_err(|e| format!("Could not build the menu bar: {e}"))?;
                }
                Box::new(submenu)
            }
            Entry::Line => Box::new(PredefinedMenuItem::separator()),
            Entry::System(system) => Box::new(match system {
                System::About => PredefinedMenuItem::about(
                    Some("About SoftPresenter"),
                    Some(AboutMetadata {
                        name: Some("SoftPresenter".to_owned()),
                        version: Some(env!("CARGO_PKG_VERSION").to_owned()),
                        comments: Some("A free, open-source presentation app for church services.".to_owned()),
                        website: Some("https://github.com/esraell/SoftPresenter".to_owned()),
                        ..AboutMetadata::default()
                    }),
                ),
                System::Services => PredefinedMenuItem::services(None),
                System::Hide => PredefinedMenuItem::hide(Some("Hide SoftPresenter")),
                System::HideOthers => PredefinedMenuItem::hide_others(None),
                System::ShowAll => PredefinedMenuItem::show_all(None),
                System::Minimize => PredefinedMenuItem::minimize(None),
                System::Zoom => PredefinedMenuItem::maximize(Some("Zoom")),
                System::FullScreen => PredefinedMenuItem::fullscreen(None),
                System::BringAllToFront => PredefinedMenuItem::bring_all_to_front(None),
            }),
        })
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
        checks: Vec<(Action, CheckMenuItem, bool)>,
        /// The items that do something, and whether each is enabled now.
        items: Vec<(Action, MenuItem, bool)>,
    }

    impl MenuBar {
        /// Builds the menus and puts them in the menu bar. Call once the app has started.
        pub fn install() -> Result<MenuBar, String> {
            let menu = Menu::new();
            let mut built = Built { checks: Vec::new(), items: Vec::new() };
            for (title, entries) in MENUS {
                let submenu = Submenu::new(*title, true);
                for entry in *entries {
                    let item = item(entry, &mut built)?;
                    submenu.append(item.as_ref()).map_err(|e| format!("Could not build the menu bar: {e}"))?;
                }
                menu.append(&submenu).map_err(|e| format!("Could not build the menu bar: {e}"))?;
                match *title {
                    "Window" => submenu.set_as_windows_menu_for_nsapp(),
                    "Help" => submenu.set_as_help_menu_for_nsapp(),
                    _ => {}
                }
            }
            menu.init_for_nsapp();
            Ok(MenuBar { _menu: menu, checks: built.checks, items: built.items })
        }

        /// Greys out the items that can't be chosen now (see [`enabled`]). Cheap when nothing
        /// changed.
        pub fn set_enabled(&mut self, now: &Greying) {
            for (action, item, shown) in &mut self.items {
                let wanted = enabled(*action, now);
                if *shown != wanted {
                    item.set_enabled(wanted);
                    *shown = wanted;
                }
            }
        }

        /// Ticks the items that are on and clears the others. Cheap when nothing changed.
        pub fn set_checked(&mut self, on: impl Fn(Action) -> bool) {
            for (action, item, shown) in &mut self.checks {
                let now = on(*action);
                if now != *shown {
                    item.set_checked(now);
                    *shown = now;
                }
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
        let file = MENUS.iter().find(|(menu, _)| *menu == "File").expect("a File menu").1;
        let inside = file.iter().find_map(|entry| match entry {
            Entry::Sub("Import Songs", inside) => Some(*inside),
            _ => None,
        });
        let words: Vec<&str> = inside
            .expect("File › Import Songs")
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
    }

    #[test]
    fn no_menu_has_two_dividers_together_or_one_at_an_end() {
        for (menu, entries) in MENUS.iter().copied().chain([("Import Songs", IMPORT_SONGS)]) {
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
        assert!(claims(KeyCode::Digit1, command | ModifiersState::ALT));
        assert!(claims(KeyCode::KeyT, ModifiersState::CONTROL));
        assert!(!claims(KeyCode::KeyS, ModifiersState::CONTROL), "Show is Command-1 now");
        assert!(claims(KeyCode::BracketLeft, command | ModifiersState::SHIFT));
        assert!(claims(KeyCode::KeyF, command | ModifiersState::SHIFT | ModifiersState::ALT));
        // Plain keys belong to typing and the show.
        assert!(!claims(KeyCode::KeyZ, ModifiersState::empty()));
        assert!(!claims(KeyCode::KeyS, ModifiersState::empty()));
        assert!(!claims(KeyCode::ArrowRight, ModifiersState::empty()));
        // Command-S is not a menu shortcut.
        assert!(!claims(KeyCode::KeyS, command));
        // Command-1, 2 and 3 are Show, Bible and Edit; 4 to 7 nothing; Option-1 to 7 (no
        // Command) the tool tabs', which the window reads; Shift-Command-1 and 2 the screens.
        assert!(claims(KeyCode::Digit1, command) && claims(KeyCode::Digit3, command));
        assert!(!claims(KeyCode::Digit4, command));
        assert!(!claims(KeyCode::Digit1, ModifiersState::ALT));
        assert!(claims(KeyCode::Digit1, command | ModifiersState::SHIFT));
        assert!(claims(KeyCode::Digit2, command | ModifiersState::SHIFT));
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
    fn an_editor_only_shortcut_is_left_to_the_window_outside_the_editor() {
        let command = ModifiersState::SUPER;
        assert!(claims(KeyCode::KeyB, command, true), "Bold in the editor");
        assert!(!claims(KeyCode::KeyB, command, false), "the Bible page elsewhere");
        // Duplicate only works on objects in the editor: elsewhere it is greyed.
        assert!(needs_editor(Action::Duplicate));
        assert!(claims(KeyCode::KeyD, command, true) && !claims(KeyCode::KeyD, command, false));
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
    }
}
