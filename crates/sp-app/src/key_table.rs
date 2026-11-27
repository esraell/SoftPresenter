//! What each key that runs the show does (Settings › Keys). Every layout lists the same rows:
//! Esc, F1 to F12 (off a Mac), Space, → and ←, Page Down and Page Up (a clicker's forward and
//! back), Home and End, and "." (the blank-screen button most clickers send). Any other key can
//! be added (Add a Key…), at the top of the list, set to Nothing; an F key added on a Mac starts
//! at what the layout gives it off a Mac. Keys are known by their place on the keyboard, not the
//! character they type, so a clicker's "." works on a Russian layout, where that key types ю.
//!
//! A style (the Key layout switch) gives every row the action it comes with: SoftPresenter's,
//! Standard (Esc, F1 and "." clear everything, F2 to F6 one layer each), or SoftProjector's (Esc
//! and "." hide the words, F1 clears everything, F5 shows, F2 adds to the playlist). Only the rows
//! changed from their style's action, and the keys added with theirs, are kept, with the
//! preferences, by name. A name this copy doesn't know (a newer copy's key or action) is passed
//! over, and that key does what its style says.
//!
//! Enter, Tab, ↑ and ↓, Delete and Backspace, / and ? are never in the table: what they do is
//! fixed (`main.rs`), and Add a Key refuses them (`Refusal`). A letter or a digit not added is a
//! slide's hot key or a group's letter, by the character it types (a church typing in Russian
//! sees its own letters); added, it does its row in the Service view, by its place.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use winit::keyboard::{Key, KeyCode, ModifiersState, PhysicalKey};

use crate::i18n::{t, tf};

/// A key pressed, as the window reports it: what it does or types (`logical`), its place on the
/// keyboard (`physical`), which the table goes by, and the modifiers held with it.
#[derive(Clone, Debug, PartialEq)]
pub struct Pressed {
    pub logical: Key,
    pub physical: PhysicalKey,
    pub held: ModifiersState,
}

impl Pressed {
    /// The key of a window's key event, with the modifiers held as it came.
    pub fn of(event: &winit::event::KeyEvent, held: ModifiersState) -> Pressed {
        Pressed { logical: event.logical_key.clone(), physical: event.physical_key, held }
    }
}

#[cfg(test)]
impl From<Key> for Pressed {
    /// The key as on a US keyboard, with nothing held: at the place that types it there.
    fn from(logical: Key) -> Pressed {
        use winit::keyboard::NamedKey as N;
        let code = match &logical {
            Key::Named(named) => match named {
                N::Escape => Some(KeyCode::Escape),
                N::F1 => Some(KeyCode::F1),
                N::F2 => Some(KeyCode::F2),
                N::F3 => Some(KeyCode::F3),
                N::F4 => Some(KeyCode::F4),
                N::F5 => Some(KeyCode::F5),
                N::F6 => Some(KeyCode::F6),
                N::F7 => Some(KeyCode::F7),
                N::F8 => Some(KeyCode::F8),
                N::F9 => Some(KeyCode::F9),
                N::F10 => Some(KeyCode::F10),
                N::F11 => Some(KeyCode::F11),
                N::F12 => Some(KeyCode::F12),
                N::Space => Some(KeyCode::Space),
                N::ArrowRight => Some(KeyCode::ArrowRight),
                N::ArrowLeft => Some(KeyCode::ArrowLeft),
                N::ArrowUp => Some(KeyCode::ArrowUp),
                N::ArrowDown => Some(KeyCode::ArrowDown),
                N::PageDown => Some(KeyCode::PageDown),
                N::PageUp => Some(KeyCode::PageUp),
                N::Home => Some(KeyCode::Home),
                N::End => Some(KeyCode::End),
                N::Enter => Some(KeyCode::Enter),
                N::Tab => Some(KeyCode::Tab),
                N::Delete => Some(KeyCode::Delete),
                N::Backspace => Some(KeyCode::Backspace),
                N::Insert => Some(KeyCode::Insert),
                N::Shift => Some(KeyCode::ShiftLeft),
                N::Control => Some(KeyCode::ControlLeft),
                N::Alt => Some(KeyCode::AltLeft),
                N::Super => Some(KeyCode::SuperLeft),
                _ => None,
            },
            Key::Character(typed) => {
                let letters = "abcdefghijklmnopqrstuvwxyz";
                let letter = [
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
                let digit = [
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
                let lower = typed.to_lowercase();
                let mut chars = lower.chars();
                match (chars.next(), chars.next()) {
                    (Some(one), None) => letters
                        .find(one)
                        .map(|at| letter[at])
                        .or_else(|| one.to_digit(10).map(|at| digit[at as usize]))
                        .or(match one {
                            '.' => Some(KeyCode::Period),
                            ',' => Some(KeyCode::Comma),
                            '/' | '?' => Some(KeyCode::Slash),
                            ';' => Some(KeyCode::Semicolon),
                            '\'' => Some(KeyCode::Quote),
                            '[' => Some(KeyCode::BracketLeft),
                            ']' => Some(KeyCode::BracketRight),
                            '-' => Some(KeyCode::Minus),
                            '=' => Some(KeyCode::Equal),
                            '`' => Some(KeyCode::Backquote),
                            '\\' => Some(KeyCode::Backslash),
                            _ => None,
                        }),
                    _ => None,
                }
            }
            _ => None,
        };
        let physical =
            code.map_or(PhysicalKey::Unidentified(winit::keyboard::NativeKeyCode::Unidentified), PhysicalKey::Code);
        Pressed { logical, physical, held: ModifiersState::empty() }
    }
}

#[cfg(test)]
impl Pressed {
    /// A key at `place` typing `logical` (as on another layout than a US one), nothing held.
    pub fn at(logical: Key, place: KeyCode) -> Pressed {
        Pressed { logical, physical: PhysicalKey::Code(place), held: ModifiersState::empty() }
    }
}

/// A key that runs the show and that Settings › Keys can change: one of the rows every layout
/// lists, in their order, or a key added there (`Other`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ShowKey {
    Escape,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    Space,
    Right,
    Left,
    PageDown,
    PageUp,
    Home,
    End,
    /// The full stop: the blank-screen button most clickers send.
    Period,
    /// Any other key, by its place on the keyboard (one of `PLACES`): a letter, a digit, a mark,
    /// a key of the number pad, Insert, F13 and on, a media key. Never a place with a variant of
    /// its own: `ShowKey::of` and `ShowKey::from_name` make every key.
    Other(KeyCode),
}

/// The places other than the rows' that a key can be added at, each with its name in the
/// preferences file: a letter or a digit by itself, any other key by its place's name in lower
/// case. Not the modifiers and lock keys, nor the keys the app keeps (`Kept`), nor the few the
/// app doesn't read (a system's power, sleep and sound keys, a browser's, an input method's).
const PLACES: &[(KeyCode, &str)] = &[
    (KeyCode::KeyA, "a"),
    (KeyCode::KeyB, "b"),
    (KeyCode::KeyC, "c"),
    (KeyCode::KeyD, "d"),
    (KeyCode::KeyE, "e"),
    (KeyCode::KeyF, "f"),
    (KeyCode::KeyG, "g"),
    (KeyCode::KeyH, "h"),
    (KeyCode::KeyI, "i"),
    (KeyCode::KeyJ, "j"),
    (KeyCode::KeyK, "k"),
    (KeyCode::KeyL, "l"),
    (KeyCode::KeyM, "m"),
    (KeyCode::KeyN, "n"),
    (KeyCode::KeyO, "o"),
    (KeyCode::KeyP, "p"),
    (KeyCode::KeyQ, "q"),
    (KeyCode::KeyR, "r"),
    (KeyCode::KeyS, "s"),
    (KeyCode::KeyT, "t"),
    (KeyCode::KeyU, "u"),
    (KeyCode::KeyV, "v"),
    (KeyCode::KeyW, "w"),
    (KeyCode::KeyX, "x"),
    (KeyCode::KeyY, "y"),
    (KeyCode::KeyZ, "z"),
    (KeyCode::Digit0, "0"),
    (KeyCode::Digit1, "1"),
    (KeyCode::Digit2, "2"),
    (KeyCode::Digit3, "3"),
    (KeyCode::Digit4, "4"),
    (KeyCode::Digit5, "5"),
    (KeyCode::Digit6, "6"),
    (KeyCode::Digit7, "7"),
    (KeyCode::Digit8, "8"),
    (KeyCode::Digit9, "9"),
    (KeyCode::Backquote, "backquote"),
    (KeyCode::Backslash, "backslash"),
    (KeyCode::BracketLeft, "bracketleft"),
    (KeyCode::BracketRight, "bracketright"),
    (KeyCode::Comma, "comma"),
    (KeyCode::Equal, "equal"),
    (KeyCode::IntlBackslash, "intlbackslash"),
    (KeyCode::IntlRo, "intlro"),
    (KeyCode::IntlYen, "intlyen"),
    (KeyCode::Minus, "minus"),
    (KeyCode::Quote, "quote"),
    (KeyCode::Semicolon, "semicolon"),
    (KeyCode::Insert, "insert"),
    (KeyCode::Numpad0, "numpad0"),
    (KeyCode::Numpad1, "numpad1"),
    (KeyCode::Numpad2, "numpad2"),
    (KeyCode::Numpad3, "numpad3"),
    (KeyCode::Numpad4, "numpad4"),
    (KeyCode::Numpad5, "numpad5"),
    (KeyCode::Numpad6, "numpad6"),
    (KeyCode::Numpad7, "numpad7"),
    (KeyCode::Numpad8, "numpad8"),
    (KeyCode::Numpad9, "numpad9"),
    (KeyCode::NumpadAdd, "numpadadd"),
    (KeyCode::NumpadSubtract, "numpadsubtract"),
    (KeyCode::NumpadMultiply, "numpadmultiply"),
    (KeyCode::NumpadDecimal, "numpaddecimal"),
    (KeyCode::NumpadEqual, "numpadequal"),
    (KeyCode::NumpadComma, "numpadcomma"),
    (KeyCode::F13, "f13"),
    (KeyCode::F14, "f14"),
    (KeyCode::F15, "f15"),
    (KeyCode::F16, "f16"),
    (KeyCode::F17, "f17"),
    (KeyCode::F18, "f18"),
    (KeyCode::F19, "f19"),
    (KeyCode::F20, "f20"),
    (KeyCode::F21, "f21"),
    (KeyCode::F22, "f22"),
    (KeyCode::F23, "f23"),
    (KeyCode::F24, "f24"),
    (KeyCode::PrintScreen, "printscreen"),
    (KeyCode::ScrollLock, "scrolllock"),
    (KeyCode::Pause, "pause"),
    (KeyCode::ContextMenu, "contextmenu"),
    (KeyCode::MediaPlayPause, "mediaplaypause"),
    (KeyCode::MediaStop, "mediastop"),
    (KeyCode::MediaTrackNext, "mediatracknext"),
    (KeyCode::MediaTrackPrevious, "mediatrackprevious"),
];

impl ShowKey {
    /// The rows every layout lists, in their order.
    const ALL: [ShowKey; 21] = [
        ShowKey::Escape,
        ShowKey::F1,
        ShowKey::F2,
        ShowKey::F3,
        ShowKey::F4,
        ShowKey::F5,
        ShowKey::F6,
        ShowKey::F7,
        ShowKey::F8,
        ShowKey::F9,
        ShowKey::F10,
        ShowKey::F11,
        ShowKey::F12,
        ShowKey::Space,
        ShowKey::Right,
        ShowKey::Left,
        ShowKey::PageDown,
        ShowKey::PageUp,
        ShowKey::Home,
        ShowKey::End,
        ShowKey::Period,
    ];

    /// The key at a place on the keyboard, if a key there can be in the table: not a modifier or
    /// a lock key, nor a key the app keeps (`Kept`), nor one it doesn't read (see `PLACES`).
    pub fn of(place: &PhysicalKey) -> Option<ShowKey> {
        let PhysicalKey::Code(code) = place else { return None };
        Some(match code {
            KeyCode::Escape => ShowKey::Escape,
            KeyCode::F1 => ShowKey::F1,
            KeyCode::F2 => ShowKey::F2,
            KeyCode::F3 => ShowKey::F3,
            KeyCode::F4 => ShowKey::F4,
            KeyCode::F5 => ShowKey::F5,
            KeyCode::F6 => ShowKey::F6,
            KeyCode::F7 => ShowKey::F7,
            KeyCode::F8 => ShowKey::F8,
            KeyCode::F9 => ShowKey::F9,
            KeyCode::F10 => ShowKey::F10,
            KeyCode::F11 => ShowKey::F11,
            KeyCode::F12 => ShowKey::F12,
            KeyCode::Space => ShowKey::Space,
            KeyCode::ArrowRight => ShowKey::Right,
            KeyCode::ArrowLeft => ShowKey::Left,
            KeyCode::PageDown => ShowKey::PageDown,
            KeyCode::PageUp => ShowKey::PageUp,
            KeyCode::Home => ShowKey::Home,
            KeyCode::End => ShowKey::End,
            KeyCode::Period => ShowKey::Period,
            other => return PLACES.iter().any(|(place, _)| place == other).then_some(ShowKey::Other(*other)),
        })
    }

    /// The row a named key is, by what the window says it does rather than where it is: for a key
    /// whose place has no row (the number pad's 6 with Num Lock off is →), or whose place the
    /// system doesn't say.
    pub fn named(key: &Key) -> Option<ShowKey> {
        use winit::keyboard::NamedKey as N;
        let Key::Named(named) = key else { return None };
        Some(match named {
            N::Escape => ShowKey::Escape,
            N::F1 => ShowKey::F1,
            N::F2 => ShowKey::F2,
            N::F3 => ShowKey::F3,
            N::F4 => ShowKey::F4,
            N::F5 => ShowKey::F5,
            N::F6 => ShowKey::F6,
            N::F7 => ShowKey::F7,
            N::F8 => ShowKey::F8,
            N::F9 => ShowKey::F9,
            N::F10 => ShowKey::F10,
            N::F11 => ShowKey::F11,
            N::F12 => ShowKey::F12,
            N::Space => ShowKey::Space,
            N::ArrowRight => ShowKey::Right,
            N::ArrowLeft => ShowKey::Left,
            N::PageDown => ShowKey::PageDown,
            N::PageUp => ShowKey::PageUp,
            N::Home => ShowKey::Home,
            N::End => ShowKey::End,
            _ => return None,
        })
    }

    /// The key's name in the preferences file ("" for an `Other` place not in `PLACES`, which
    /// nothing makes).
    pub fn name(self) -> &'static str {
        match self {
            ShowKey::Escape => "esc",
            ShowKey::F1 => "f1",
            ShowKey::F2 => "f2",
            ShowKey::F3 => "f3",
            ShowKey::F4 => "f4",
            ShowKey::F5 => "f5",
            ShowKey::F6 => "f6",
            ShowKey::F7 => "f7",
            ShowKey::F8 => "f8",
            ShowKey::F9 => "f9",
            ShowKey::F10 => "f10",
            ShowKey::F11 => "f11",
            ShowKey::F12 => "f12",
            ShowKey::Space => "space",
            ShowKey::Right => "right",
            ShowKey::Left => "left",
            ShowKey::PageDown => "pagedown",
            ShowKey::PageUp => "pageup",
            ShowKey::Home => "home",
            ShowKey::End => "end",
            ShowKey::Period => "period",
            ShowKey::Other(code) => PLACES.iter().find(|(place, _)| *place == code).map_or("", |(_, name)| name),
        }
    }

    /// The key of that name in the preferences file, if this copy knows it.
    pub fn from_name(name: &str) -> Option<ShowKey> {
        ShowKey::ALL
            .into_iter()
            .find(|key| key.name() == name)
            .or_else(|| PLACES.iter().find(|(_, have)| *have == name).map(|(place, _)| ShowKey::Other(*place)))
    }

    /// The key Add a Key adds for the key pressed, or why it adds none (but for its being in the
    /// list already, which is the table's to say).
    fn to_add(pressed: &Pressed) -> Result<ShowKey, Refusal> {
        use winit::keyboard::NamedKey as N;
        let modifier = matches!(
            pressed.logical,
            Key::Named(
                N::Shift
                    | N::Control
                    | N::Alt
                    | N::AltGraph
                    | N::Super
                    | N::Meta
                    | N::Hyper
                    | N::Fn
                    | N::FnLock
                    | N::CapsLock
                    | N::NumLock
                    | N::Symbol
                    | N::SymbolLock
            )
        ) || matches!(
            pressed.physical,
            PhysicalKey::Code(
                KeyCode::ShiftLeft
                    | KeyCode::ShiftRight
                    | KeyCode::ControlLeft
                    | KeyCode::ControlRight
                    | KeyCode::AltLeft
                    | KeyCode::AltRight
                    | KeyCode::SuperLeft
                    | KeyCode::SuperRight
                    | KeyCode::Meta
                    | KeyCode::Hyper
                    | KeyCode::Fn
                    | KeyCode::FnLock
                    | KeyCode::CapsLock
                    | KeyCode::NumLock
            )
        );
        if modifier {
            return Err(Refusal::NoKey);
        }
        let held = pressed.held;
        if held.super_key() || held.control_key() || held.alt_key() {
            return Err(Refusal::Held);
        }
        if let Some(kept) = Kept::of(pressed) {
            return Err(Refusal::Kept(kept));
        }
        ShowKey::of(&pressed.physical).or_else(|| ShowKey::named(&pressed.logical)).ok_or(Refusal::NoKey)
    }

    /// The key as its cap says it (a US keyboard's legend for a mark), as the list of keys and
    /// Settings › Keys write it.
    #[allow(dead_code)] // Settings › Keys reads it, next.
    pub fn cap(self) -> String {
        let cap = match self {
            ShowKey::Escape => "Esc",
            ShowKey::Space => t("keys.cap.space", "Space"),
            ShowKey::Right => "\u{2192}",
            ShowKey::Left => "\u{2190}",
            ShowKey::PageDown => t("keys.cap.page_down", "Page Down"),
            ShowKey::PageUp => t("keys.cap.page_up", "Page Up"),
            ShowKey::Home => t("keys.cap.home", "Home"),
            ShowKey::End => t("keys.cap.end", "End"),
            ShowKey::Period => ".",
            ShowKey::Other(code) => return other_cap(code, self.name()),
            // The F keys, F1 to F12.
            f_key => return f_key.name().to_uppercase(),
        };
        cap.to_owned()
    }

    /// Whether it is one of the F rows, F1 to F12, which a layout lists off a Mac only.
    pub fn is_f_key(self) -> bool {
        matches!(
            self,
            ShowKey::F1
                | ShowKey::F2
                | ShowKey::F3
                | ShowKey::F4
                | ShowKey::F5
                | ShowKey::F6
                | ShowKey::F7
                | ShowKey::F8
                | ShowKey::F9
                | ShowKey::F10
                | ShowKey::F11
                | ShowKey::F12
        )
    }

    /// Whether it is a letter's or a digit's place: a to z, 0 to 9 and the number pad's 0 to 9.
    /// Added, these do their row in the Service view in place of a slide's hot key or a group's
    /// letter (Settings › Keys says so in their note). In the Bible view what a key types decides
    /// (`show_key_held_back` in `main.rs`): a letter or a digit always starts a reference.
    #[allow(dead_code)] // Settings › Keys' notes read it, next.
    pub fn is_letter_or_digit(self) -> bool {
        let ShowKey::Other(code) = self else { return false };
        // a to z and 0 to 9 are named by themselves.
        self.name().len() == 1
            || matches!(
                code,
                KeyCode::Numpad0
                    | KeyCode::Numpad1
                    | KeyCode::Numpad2
                    | KeyCode::Numpad3
                    | KeyCode::Numpad4
                    | KeyCode::Numpad5
                    | KeyCode::Numpad6
                    | KeyCode::Numpad7
                    | KeyCode::Numpad8
                    | KeyCode::Numpad9
            )
    }
}

/// The cap of a key added (`ShowKey::Other`), named `name` in the preferences file.
fn other_cap(code: KeyCode, name: &str) -> String {
    let mark = match code {
        KeyCode::Backquote => "`",
        KeyCode::Backslash => "\\",
        KeyCode::BracketLeft => "[",
        KeyCode::BracketRight => "]",
        KeyCode::Comma => ",",
        KeyCode::Equal => "=",
        KeyCode::IntlBackslash => "\u{a7}",
        KeyCode::IntlRo => "\u{308d}",
        KeyCode::IntlYen => "\u{a5}",
        KeyCode::Minus => "-",
        KeyCode::Quote => "'",
        KeyCode::Semicolon => ";",
        KeyCode::Insert => t("keys.cap.insert", "Insert"),
        KeyCode::PrintScreen => t("keys.cap.print_screen", "Print Screen"),
        KeyCode::ScrollLock => t("keys.cap.scroll_lock", "Scroll Lock"),
        KeyCode::Pause => t("keys.cap.pause", "Pause"),
        KeyCode::ContextMenu => t("keys.cap.menu", "Menu"),
        KeyCode::MediaPlayPause => t("keys.cap.play_pause", "Play/Pause"),
        KeyCode::MediaStop => t("keys.cap.stop", "Stop"),
        KeyCode::MediaTrackNext => t("keys.cap.next_track", "Next Track"),
        KeyCode::MediaTrackPrevious => t("keys.cap.previous_track", "Previous Track"),
        _ => "",
    };
    if !mark.is_empty() {
        return mark.to_owned();
    }
    // The number pad's: "Num 1", "Num +".
    let pad = match code {
        KeyCode::NumpadAdd => Some("+"),
        KeyCode::NumpadSubtract => Some("-"),
        KeyCode::NumpadMultiply => Some("*"),
        KeyCode::NumpadDecimal => Some("."),
        KeyCode::NumpadEqual => Some("="),
        KeyCode::NumpadComma => Some(","),
        _ => name.strip_prefix("numpad"),
    };
    if let Some(pad) = pad {
        return tf("keys.cap.numpad", "Num {key}", &[("key", pad)]);
    }
    // A letter, a digit, F13 and on.
    name.to_uppercase()
}

/// The keys Settings › Keys has a row for in every layout, in order: the F keys only with
/// `f_keys` (off a Mac, `keys::f_keys()`), since a Mac's are the system's (brightness, sound,
/// Mission Control). Keys added come before them (`KeyTable::listed`).
pub fn rows(f_keys: bool) -> Vec<ShowKey> {
    ShowKey::ALL.into_iter().filter(|key| f_keys || !key.is_f_key()).collect()
}

/// A key the app always keeps for one thing, whatever the table says, which Add a Key refuses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kept {
    /// Puts the picked slide on screen.
    Enter,
    /// Moves between the buttons.
    Tab,
    /// The item above.
    Up,
    /// The item below.
    Down,
    /// Opens the find box.
    Slash,
    /// Opens the list of keys.
    QuestionMark,
    /// Deletes what is picked.
    Delete,
    /// Deletes what is picked too: a Mac's Delete key is at its place.
    Backspace,
}

impl Kept {
    /// The key the app keeps that `pressed` is: by what it does or types (? and / wherever the
    /// layout puts them), else by its place (the number pad's Enter, a place typing another
    /// character than / on this layout).
    fn of(pressed: &Pressed) -> Option<Kept> {
        use winit::keyboard::NamedKey as N;
        let kept = match &pressed.logical {
            Key::Character(typed) if typed == "?" => Some(Kept::QuestionMark),
            Key::Character(typed) if typed == "/" => Some(Kept::Slash),
            Key::Named(N::Enter) => Some(Kept::Enter),
            Key::Named(N::Tab) => Some(Kept::Tab),
            Key::Named(N::ArrowUp) => Some(Kept::Up),
            Key::Named(N::ArrowDown) => Some(Kept::Down),
            Key::Named(N::Delete) => Some(Kept::Delete),
            Key::Named(N::Backspace) => Some(Kept::Backspace),
            _ => None,
        };
        kept.or(match pressed.physical {
            PhysicalKey::Code(KeyCode::Enter | KeyCode::NumpadEnter) => Some(Kept::Enter),
            PhysicalKey::Code(KeyCode::Tab) => Some(Kept::Tab),
            PhysicalKey::Code(KeyCode::ArrowUp) => Some(Kept::Up),
            PhysicalKey::Code(KeyCode::ArrowDown) => Some(Kept::Down),
            PhysicalKey::Code(KeyCode::Slash | KeyCode::NumpadDivide) => Some(Kept::Slash),
            PhysicalKey::Code(KeyCode::Delete) => Some(Kept::Delete),
            PhysicalKey::Code(KeyCode::Backspace | KeyCode::NumpadBackspace) => Some(Kept::Backspace),
            _ => None,
        })
    }
}

/// Why Add a Key adds no key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// No key to add: a modifier or a lock key by itself, or a key the app doesn't read (see
    /// `PLACES`). Add a Key goes on listening, and says nothing.
    NoKey,
    /// Held with Command, Control or Option (Ctrl or Alt off a Mac): such keys are the menus'.
    /// Add a Key goes on listening.
    Held,
    /// A key the app keeps for one thing. Add a Key goes on listening.
    Kept(Kept),
    /// A key that is in the list already, which Settings › Keys shows instead. Esc comes back as
    /// this too: Settings › Keys takes it first, to stop listening.
    Listed(ShowKey),
}

impl Refusal {
    /// What Add a Key says, in its line in the table's head (`None` for no key). `mac` is for the
    /// Mac's modifiers (⌘, ⌃, ⌥) and its Delete key, at Backspace's place.
    #[allow(dead_code)] // Settings › Keys says it, next.
    pub fn reason(self, mac: bool) -> Option<String> {
        let words = match self {
            Refusal::NoKey => return None,
            Refusal::Listed(key) => {
                return Some(tf(
                    "settings.keys.refused.listed",
                    "{key} is in the list already.",
                    &[("key", &key.cap())],
                ));
            }
            Refusal::Held if mac => t(
                "settings.keys.refused.held_mac",
                "Keys held with \u{2318}, \u{2303} or \u{2325} are the menus\u{2019}. Press a key by itself.",
            ),
            Refusal::Held => t(
                "settings.keys.refused.held",
                "Keys held with Ctrl or Alt are the menus\u{2019}. Press a key by itself.",
            ),
            Refusal::Kept(Kept::Enter) => {
                t("settings.keys.refused.enter", "Enter always puts the picked slide on screen. Press another.")
            }
            Refusal::Kept(Kept::Tab) => {
                t("settings.keys.refused.tab", "Tab always moves between buttons. Press another.")
            }
            Refusal::Kept(Kept::Up) => {
                t("settings.keys.refused.up", "\u{2191} always goes to the item above. Press another.")
            }
            Refusal::Kept(Kept::Down) => {
                t("settings.keys.refused.down", "\u{2193} always goes to the item below. Press another.")
            }
            Refusal::Kept(Kept::Slash) => {
                t("settings.keys.refused.slash", "/ always opens the find box. Press another.")
            }
            Refusal::Kept(Kept::QuestionMark) => {
                t("settings.keys.refused.question_mark", "? always opens the list of keys. Press another.")
            }
            // A Mac's Delete key is at Backspace's place.
            Refusal::Kept(Kept::Backspace) if !mac => {
                t("settings.keys.refused.backspace", "Backspace always deletes what is picked. Press another.")
            }
            Refusal::Kept(Kept::Delete | Kept::Backspace) => {
                t("settings.keys.refused.delete", "Delete always deletes what is picked. Press another.")
            }
        };
        Some(words.to_owned())
    }
}

/// What a show key can do.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KeyAction {
    ClearAll,
    ClearSlide,
    ClearMedia,
    ClearProps,
    ClearAudio,
    ClearMessages,
    /// The slide's words go; its background, the theme's objects and the media stay.
    HideWords,
    Next,
    Previous,
    First,
    Last,
    /// What Enter puts on screen.
    ShowSelected,
    /// The hidden words back, or the slide the last clear took off.
    ShowAgain,
    AddToPlaylist,
    RemoveFromPlaylist,
    GoToBible,
    GoToService,
    /// Nothing in the show.
    Nothing,
}

/// The parts the chooser of an action lists them in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionGroup {
    Clear,
    Slides,
    PlaylistViews,
    None,
}

impl KeyAction {
    /// Every action, group after group in the chooser's order.
    pub const ALL: [KeyAction; 18] = [
        KeyAction::ClearAll,
        KeyAction::HideWords,
        KeyAction::ClearSlide,
        KeyAction::ClearMedia,
        KeyAction::ClearProps,
        KeyAction::ClearAudio,
        KeyAction::ClearMessages,
        KeyAction::ShowSelected,
        KeyAction::ShowAgain,
        KeyAction::Next,
        KeyAction::Previous,
        KeyAction::First,
        KeyAction::Last,
        KeyAction::AddToPlaylist,
        KeyAction::RemoveFromPlaylist,
        KeyAction::GoToBible,
        KeyAction::GoToService,
        KeyAction::Nothing,
    ];

    /// The action's name in the preferences file.
    pub fn name(self) -> &'static str {
        match self {
            KeyAction::ClearAll => "clear_all",
            KeyAction::ClearSlide => "clear_slide",
            KeyAction::ClearMedia => "clear_media",
            KeyAction::ClearProps => "clear_props",
            KeyAction::ClearAudio => "clear_audio",
            KeyAction::ClearMessages => "clear_messages",
            KeyAction::HideWords => "hide_words",
            KeyAction::Next => "next",
            KeyAction::Previous => "previous",
            KeyAction::First => "first",
            KeyAction::Last => "last",
            KeyAction::ShowSelected => "show_selected",
            KeyAction::ShowAgain => "show_again",
            KeyAction::AddToPlaylist => "add_to_playlist",
            KeyAction::RemoveFromPlaylist => "remove_from_playlist",
            KeyAction::GoToBible => "go_bible",
            KeyAction::GoToService => "go_service",
            KeyAction::Nothing => "nothing",
        }
    }

    /// The action of that name in the preferences file, if this copy knows it.
    pub fn from_name(name: &str) -> Option<KeyAction> {
        KeyAction::ALL.into_iter().find(|action| action.name() == name)
    }

    /// The action in words, as its chooser says it.
    #[allow(dead_code)] // Settings › Keys reads it, next.
    pub fn words(self) -> String {
        match self {
            KeyAction::ClearAll => t("settings.keys.do.clear_all", "Clear All"),
            KeyAction::ClearSlide => t("settings.keys.do.clear_slide", "Clear Slide"),
            KeyAction::ClearMedia => t("settings.keys.do.clear_media", "Clear Media"),
            KeyAction::ClearProps => t("settings.keys.do.clear_props", "Clear Props"),
            KeyAction::ClearAudio => t("settings.keys.do.clear_audio", "Clear Audio"),
            KeyAction::ClearMessages => t("settings.keys.do.clear_messages", "Clear Messages"),
            KeyAction::HideWords => t("settings.keys.do.hide_words", "Hide the Words"),
            KeyAction::Next => t("settings.keys.do.next", "Next Slide"),
            KeyAction::Previous => t("settings.keys.do.previous", "Previous Slide"),
            KeyAction::First => t("settings.keys.do.first", "First Slide"),
            KeyAction::Last => t("settings.keys.do.last", "Last Slide"),
            KeyAction::ShowSelected => t("settings.keys.do.show_selected", "Show Selected"),
            KeyAction::ShowAgain => t("settings.keys.do.show_again", "Show Again"),
            KeyAction::AddToPlaylist => t("settings.keys.do.add_to_playlist", "Add to Playlist"),
            KeyAction::RemoveFromPlaylist => t("settings.keys.do.remove_from_playlist", "Remove from Playlist"),
            KeyAction::GoToBible => t("settings.keys.do.go_bible", "Go to Bible"),
            KeyAction::GoToService => t("settings.keys.do.go_service", "Go to Service"),
            KeyAction::Nothing => t("settings.keys.do.nothing", "Nothing"),
        }
        .to_owned()
    }

    /// The part of the chooser the action is listed in.
    pub fn group(self) -> ActionGroup {
        match self {
            KeyAction::ClearAll
            | KeyAction::HideWords
            | KeyAction::ClearSlide
            | KeyAction::ClearMedia
            | KeyAction::ClearProps
            | KeyAction::ClearAudio
            | KeyAction::ClearMessages => ActionGroup::Clear,
            KeyAction::ShowSelected
            | KeyAction::ShowAgain
            | KeyAction::Next
            | KeyAction::Previous
            | KeyAction::First
            | KeyAction::Last => ActionGroup::Slides,
            KeyAction::AddToPlaylist
            | KeyAction::RemoveFromPlaylist
            | KeyAction::GoToBible
            | KeyAction::GoToService => ActionGroup::PlaylistViews,
            KeyAction::Nothing => ActionGroup::None,
        }
    }
}

/// Which program's keys the table starts from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KeyStyle {
    #[default]
    SoftPresenter,
    SoftProjector,
}

/// What every show key does: the style chosen; the rows changed from it and the keys added, by
/// name (key name → action name), kept as words so that a name this copy doesn't know is passed
/// over; and the keys added, newest first, by name (saved only when there are some, so a table
/// saved before keys could be added is saved again as it was).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "serde_json::Value")]
pub struct KeyTable {
    pub style: KeyStyle,
    changed: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    added: Vec<String>,
}

impl From<serde_json::Value> for KeyTable {
    /// The table as saved, taking what can be read: a style this copy doesn't know is
    /// SoftPresenter's, and a row or a key added saved as anything but a name is passed over (a
    /// key added twice is kept once). The preferences are read as one, so a table that failed to
    /// read would lose every other choice with it.
    fn from(saved: serde_json::Value) -> KeyTable {
        let style = saved.get("style").and_then(|style| KeyStyle::deserialize(style).ok()).unwrap_or_default();
        let changed = saved
            .get("changed")
            .and_then(serde_json::Value::as_object)
            .map(|rows| {
                rows.iter().filter_map(|(key, action)| Some((key.clone(), action.as_str()?.to_owned()))).collect()
            })
            .unwrap_or_default();
        let mut added: Vec<String> = Vec::new();
        let names = saved.get("added").and_then(serde_json::Value::as_array).into_iter().flatten();
        for name in names.filter_map(serde_json::Value::as_str).filter(|name| !name.is_empty()) {
            if !added.iter().any(|have| have == name) {
                added.push(name.to_owned());
            }
        }
        KeyTable { style, changed, added }
    }
}

impl KeyTable {
    /// What `key` does: what it was changed or set to, else what its style says.
    pub fn action(&self, key: ShowKey) -> KeyAction {
        self.changed
            .get(key.name())
            .and_then(|name| KeyAction::from_name(name))
            .unwrap_or_else(|| KeyTable::default_action(self.style, key))
    }

    /// What `key` does in `style`, as it comes: the rows every layout lists, and a key added
    /// starts at Nothing (an F key added on a Mac at its row's action, `add`).
    pub fn default_action(style: KeyStyle, key: ShowKey) -> KeyAction {
        match (style, key) {
            (_, ShowKey::Space | ShowKey::Right) => KeyAction::Next,
            (_, ShowKey::Left) => KeyAction::Previous,
            (_, ShowKey::Home) => KeyAction::First,
            (_, ShowKey::End) => KeyAction::Last,
            (KeyStyle::SoftPresenter, ShowKey::Escape | ShowKey::F1) => KeyAction::ClearAll,
            (KeyStyle::SoftPresenter, ShowKey::F2) => KeyAction::ClearSlide,
            (KeyStyle::SoftPresenter, ShowKey::F3) => KeyAction::ClearMedia,
            (KeyStyle::SoftPresenter, ShowKey::F4) => KeyAction::ClearProps,
            (KeyStyle::SoftPresenter, ShowKey::F5) => KeyAction::ClearAudio,
            (KeyStyle::SoftPresenter, ShowKey::F6) => KeyAction::ClearMessages,
            (KeyStyle::SoftPresenter, ShowKey::F7 | ShowKey::F8) => KeyAction::Nothing,
            (KeyStyle::SoftProjector, ShowKey::Escape) => KeyAction::HideWords,
            (KeyStyle::SoftProjector, ShowKey::F1) => KeyAction::ClearAll,
            (KeyStyle::SoftProjector, ShowKey::F8) => KeyAction::Nothing,
            (KeyStyle::SoftProjector, ShowKey::F2) => KeyAction::AddToPlaylist,
            (KeyStyle::SoftProjector, ShowKey::F3) => KeyAction::RemoveFromPlaylist,
            (KeyStyle::SoftProjector, ShowKey::F4) => KeyAction::ShowAgain,
            (KeyStyle::SoftProjector, ShowKey::F5) => KeyAction::ShowSelected,
            (KeyStyle::SoftProjector, ShowKey::F6) => KeyAction::GoToBible,
            (KeyStyle::SoftProjector, ShowKey::F7) => KeyAction::GoToService,
            (_, ShowKey::F9 | ShowKey::F10 | ShowKey::F11 | ShowKey::F12) => KeyAction::Nothing,
            (_, ShowKey::PageDown) => KeyAction::Next,
            (_, ShowKey::PageUp) => KeyAction::Previous,
            (KeyStyle::SoftPresenter, ShowKey::Period) => KeyAction::ClearAll,
            (KeyStyle::SoftProjector, ShowKey::Period) => KeyAction::HideWords,
            (_, ShowKey::Other(_)) => KeyAction::Nothing,
        }
    }

    /// The row a key pressed is in, if it has one (`is_listed`): by its place on the keyboard,
    /// or, for a key whose place has no row, by the named key the window says it is (the number
    /// pad's 6 with Num Lock off is →). A key typing / or ? never has one (the find box and the
    /// list of keys are theirs, wherever the layout puts them), nor a character typed with ⌥ (Alt
    /// off a Mac), which is the window's: ⌥1 to ⌥7 number a tool tab's things.
    pub fn row_of(&self, pressed: &Pressed, f_keys: bool) -> Option<ShowKey> {
        if let Key::Character(typed) = &pressed.logical
            && (typed == "/" || typed == "?" || pressed.held.alt_key())
        {
            return None;
        }
        let listed = |key: &ShowKey| self.is_listed(*key, f_keys);
        ShowKey::of(&pressed.physical).filter(listed).or_else(|| ShowKey::named(&pressed.logical).filter(listed))
    }

    /// Whether `key` has a row: one every layout lists (an F key only with `f_keys`), or a key
    /// added (an F key on a Mac too).
    pub fn is_listed(&self, key: ShowKey, f_keys: bool) -> bool {
        (ShowKey::ALL.contains(&key) && (f_keys || !key.is_f_key())) || self.is_added(key)
    }

    /// Whether `key` was added (Add a Key…).
    #[allow(dead_code)] // Settings › Keys reads it, next.
    pub fn is_added(&self, key: ShowKey) -> bool {
        self.added.iter().any(|name| name == key.name())
    }

    /// Every row of Settings › Keys, in order: the keys added, newest first, then the rows every
    /// layout lists (`rows`). A key added that is a layout's row here (an F key added on a Mac,
    /// read off one) is listed once, in its row's place.
    #[allow(dead_code)] // Settings › Keys lists its rows from it, next.
    pub fn listed(&self, f_keys: bool) -> Vec<ShowKey> {
        let rows = rows(f_keys);
        let mut listed: Vec<ShowKey> = Vec::new();
        for key in self.added.iter().filter_map(|name| ShowKey::from_name(name)) {
            if !rows.contains(&key) && !listed.contains(&key) {
                listed.push(key);
            }
        }
        listed.extend(rows);
        listed
    }

    /// Adds the key pressed at the top of the list (Add a Key…), set to Nothing, or an F key on
    /// a Mac (no row there, `f_keys` false) to what the layout gives it off one; or says why not.
    /// The key added is returned.
    #[allow(dead_code)] // Settings › Keys adds with it, next.
    pub fn add(&mut self, pressed: &Pressed, f_keys: bool) -> Result<ShowKey, Refusal> {
        let key = ShowKey::to_add(pressed)?;
        if self.is_listed(key, f_keys) {
            return Err(Refusal::Listed(key));
        }
        let starts = if key.is_f_key() { KeyTable::default_action(self.style, key) } else { KeyAction::Nothing };
        self.added.insert(0, key.name().to_owned());
        self.changed.insert(key.name().to_owned(), starts.name().to_owned());
        Ok(key)
    }

    /// Takes a key added out of the list (its ×), with what it was set to. A row every layout
    /// lists can't be taken out.
    #[allow(dead_code)] // Settings › Keys takes keys out with it, next.
    pub fn remove(&mut self, key: ShowKey) {
        if !self.is_added(key) {
            return;
        }
        self.added.retain(|name| name != key.name());
        self.changed.remove(key.name());
    }

    /// Sets what `key` does. For a row, its style's own action is no change, so the row is
    /// forgotten; a key added keeps what it is set to, so that a style chosen leaves it be.
    #[allow(dead_code)] // Settings › Keys changes the table with it, next.
    pub fn set(&mut self, key: ShowKey, action: KeyAction) {
        if action == KeyTable::default_action(self.style, key) && !self.is_added(key) {
            self.changed.remove(key.name());
        } else {
            self.changed.insert(key.name().to_owned(), action.name().to_owned());
        }
    }

    /// Chooses a style: every row does what it says, and the rows changed are changed no more.
    /// The keys added stay, each doing what it was set to.
    #[allow(dead_code)] // Settings › Keys changes the table with it, next.
    pub fn set_style(&mut self, style: KeyStyle) {
        self.style = style;
        let added = &self.added;
        self.changed.retain(|name, _| added.contains(name));
    }

    /// Every row back to what its style says, and the keys added taken out (Reset to Defaults).
    #[allow(dead_code)] // Settings › Keys changes the table with it, next.
    pub fn reset(&mut self) {
        self.changed.clear();
        self.added.clear();
    }

    /// Whether `key` does something other than its style says.
    #[allow(dead_code)] // Settings › Keys reads it, next.
    pub fn is_changed(&self, key: ShowKey) -> bool {
        self.action(key) != KeyTable::default_action(self.style, key)
    }

    /// Whether Reset to Defaults would change anything: a row does something other than its
    /// style says (an F key on a Mac too), or a key was added.
    #[allow(dead_code)] // Settings › Keys reads it, next.
    pub fn any_changed(&self) -> bool {
        ShowKey::ALL.into_iter().any(|key| self.is_changed(key))
            || self.added.iter().any(|name| ShowKey::from_name(name).is_some())
    }

    /// The keys listed (`listed`: the F rows only with `f_keys`, keys added first) that do
    /// `action`, in the list's order.
    #[allow(dead_code)] // Settings › Keys reads it, next.
    pub fn keys_for(&self, action: KeyAction, f_keys: bool) -> Vec<ShowKey> {
        self.listed(f_keys).into_iter().filter(|key| self.action(*key) == action).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{Language, with_language};
    use winit::keyboard::NamedKey;

    /// A key typing `typed` at `place`, with nothing held.
    fn typing(typed: &str, place: KeyCode) -> Pressed {
        Pressed::at(Key::Character(typed.into()), place)
    }

    /// A key at `place` doing the named key `named`, with nothing held.
    fn named(named: NamedKey, place: KeyCode) -> Pressed {
        Pressed::at(Key::Named(named), place)
    }

    /// `pressed` held with `held`.
    fn held(pressed: Pressed, held: ModifiersState) -> Pressed {
        Pressed { held, ..pressed }
    }

    #[test]
    fn each_style_gives_the_spec_table() {
        use KeyAction::*;
        use ShowKey::*;
        // The key, then what it does in SoftPresenter's style and in SoftProjector's.
        let table = [
            (Escape, ClearAll, HideWords),
            (F1, ClearAll, ClearAll),
            (F2, ClearSlide, AddToPlaylist),
            (F3, ClearMedia, RemoveFromPlaylist),
            (F4, ClearProps, ShowAgain),
            (F5, ClearAudio, ShowSelected),
            (F6, ClearMessages, GoToBible),
            (F7, Nothing, GoToService),
            (F8, Nothing, Nothing),
            (F9, Nothing, Nothing),
            (F10, Nothing, Nothing),
            (F11, Nothing, Nothing),
            (F12, Nothing, Nothing),
            (Space, Next, Next),
            (Right, Next, Next),
            (Left, Previous, Previous),
            (PageDown, Next, Next),
            (PageUp, Previous, Previous),
            (Home, First, First),
            (End, Last, Last),
            (Period, ClearAll, HideWords),
        ];
        for (key, softpresenter, softprojector) in table {
            assert_eq!(KeyTable::default_action(KeyStyle::SoftPresenter, key), softpresenter, "{key:?}");
            assert_eq!(KeyTable::default_action(KeyStyle::SoftProjector, key), softprojector, "{key:?}");
            assert_eq!(KeyTable::default().action(key), softpresenter, "a new table is SoftPresenter's: {key:?}");
        }
        assert_eq!(table.map(|row| row.0).to_vec(), rows(true), "every row is in the table");
    }

    #[test]
    fn the_new_rows_have_the_same_defaults_in_both_layouts_except_period() {
        use KeyAction::*;
        let both = |key| {
            (
                KeyTable::default_action(KeyStyle::SoftPresenter, key),
                KeyTable::default_action(KeyStyle::SoftProjector, key),
            )
        };
        // A clicker's forward and back.
        assert_eq!(both(ShowKey::PageDown), (Next, Next));
        assert_eq!(both(ShowKey::PageUp), (Previous, Previous));
        for f in [ShowKey::F9, ShowKey::F10, ShowKey::F11, ShowKey::F12] {
            assert_eq!(both(f), (Nothing, Nothing), "{f:?}");
        }
        // The blank-screen button: Clear All in Standard, Hide the Words like SoftProjector.
        assert_eq!(both(ShowKey::Period), (ClearAll, HideWords));
        // Both layouts list the same rows, so the switch moves nothing.
        let mut softprojector = KeyTable::default();
        softprojector.set_style(KeyStyle::SoftProjector);
        for f_keys in [true, false] {
            assert_eq!(KeyTable::default().listed(f_keys), softprojector.listed(f_keys));
        }
        // A key added starts at Nothing in either.
        assert_eq!(both(ShowKey::Other(KeyCode::KeyB)), (Nothing, Nothing));
    }

    #[test]
    fn a_mac_has_nine_rows_and_a_pc_twenty_one() {
        use ShowKey::*;
        assert_eq!(rows(false), [Escape, Space, Right, Left, PageDown, PageUp, Home, End, Period]);
        assert_eq!(
            rows(true),
            [
                Escape, F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12, Space, Right, Left, PageDown, PageUp, Home,
                End, Period
            ]
        );
        let table = KeyTable::default();
        assert_eq!(table.listed(false).len(), 9);
        assert_eq!(table.listed(true).len(), 21);
        assert_eq!(table.listed(true), rows(true), "with nothing added the list is the rows");
        let names: Vec<&str> = rows(true).iter().map(|key| key.name()).collect();
        assert_eq!(
            names,
            [
                "esc", "f1", "f2", "f3", "f4", "f5", "f6", "f7", "f8", "f9", "f10", "f11", "f12", "space", "right",
                "left", "pagedown", "pageup", "home", "end", "period"
            ]
        );
        let caps: Vec<String> = with_language(Language::English, || rows(false).iter().map(|key| key.cap()).collect());
        assert_eq!(caps, ["Esc", "Space", "\u{2192}", "\u{2190}", "Page Down", "Page Up", "Home", "End", "."]);
        assert_eq!((ShowKey::F7.cap(), ShowKey::F12.cap()), ("F7".to_owned(), "F12".to_owned()));
    }

    #[test]
    fn a_task_1_saved_file_reads_back_unchanged() {
        let saved = serde_json::json!({
            "style": "softprojector",
            "changed": {"esc": "clear_slide", "f2": "nothing", "space": "previous"}
        });
        let table: KeyTable = serde_json::from_value(saved.clone()).unwrap();
        assert_eq!(table.style, KeyStyle::SoftProjector);
        assert_eq!(table.action(ShowKey::Escape), KeyAction::ClearSlide);
        assert_eq!(table.action(ShowKey::F2), KeyAction::Nothing);
        assert_eq!(table.action(ShowKey::Space), KeyAction::Previous);
        assert_eq!(table.action(ShowKey::Period), KeyAction::HideWords, "a new row does what its style says");
        assert_eq!(table.listed(true), rows(true), "nothing was added");
        assert_eq!(serde_json::to_value(&table).unwrap(), saved, "written again just as it was");
        let fresh = serde_json::to_value(KeyTable::default()).unwrap();
        assert_eq!(fresh, serde_json::json!({"style": "softpresenter", "changed": {}}));
    }

    #[test]
    fn every_place_has_one_name_that_reads_back() {
        let mut names: Vec<&str> = ShowKey::ALL.iter().map(|key| key.name()).collect();
        names.extend(PLACES.iter().map(|(_, name)| *name));
        let count = names.len();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), count, "no two keys share a name");
        for key in ShowKey::ALL {
            assert_eq!(ShowKey::from_name(key.name()), Some(key));
        }
        for (place, name) in PLACES {
            let key = ShowKey::Other(*place);
            assert_eq!(key.name(), *name);
            assert_eq!(ShowKey::from_name(name), Some(key), "{name}");
            assert_eq!(ShowKey::of(&PhysicalKey::Code(*place)), Some(key), "{name}");
        }
        // A place with a row of its own is that row, never `Other`.
        assert_eq!(ShowKey::of(&PhysicalKey::Code(KeyCode::Period)), Some(ShowKey::Period));
        assert_eq!(ShowKey::of(&PhysicalKey::Code(KeyCode::PageDown)), Some(ShowKey::PageDown));
        assert_eq!(ShowKey::of(&PhysicalKey::Code(KeyCode::F12)), Some(ShowKey::F12));
        assert!(PLACES.iter().all(
            |(place, _)| !matches!(ShowKey::of(&PhysicalKey::Code(*place)), Some(row) if ShowKey::ALL.contains(&row))
        ));
        // As the brief names them.
        assert_eq!(ShowKey::of(&PhysicalKey::Code(KeyCode::KeyC)).map(ShowKey::name), Some("c"));
        assert_eq!(ShowKey::of(&PhysicalKey::Code(KeyCode::Digit7)).map(ShowKey::name), Some("7"));
        assert_eq!(ShowKey::of(&PhysicalKey::Code(KeyCode::Comma)).map(ShowKey::name), Some("comma"));
        assert_eq!(ShowKey::of(&PhysicalKey::Code(KeyCode::BracketLeft)).map(ShowKey::name), Some("bracketleft"));
        // Modifiers, the keys the app keeps and a place the system doesn't say have none.
        for none in [KeyCode::ShiftLeft, KeyCode::SuperRight, KeyCode::CapsLock, KeyCode::Enter, KeyCode::Slash] {
            assert_eq!(ShowKey::of(&PhysicalKey::Code(none)), None, "{none:?}");
        }
        assert_eq!(ShowKey::of(&PhysicalKey::Unidentified(winit::keyboard::NativeKeyCode::Unidentified)), None);
        assert_eq!(ShowKey::from_name("fly_away"), None);
        // A letter's and a digit's place, and the caps a key added shows.
        assert!(
            ShowKey::Other(KeyCode::KeyB).is_letter_or_digit() && ShowKey::Other(KeyCode::Numpad4).is_letter_or_digit()
        );
        assert!(!ShowKey::Other(KeyCode::Comma).is_letter_or_digit() && !ShowKey::Period.is_letter_or_digit());
        let caps: Vec<String> = with_language(Language::English, || {
            [
                KeyCode::KeyB,
                KeyCode::Digit3,
                KeyCode::Comma,
                KeyCode::Numpad1,
                KeyCode::NumpadAdd,
                KeyCode::Insert,
                KeyCode::F13,
            ]
            .map(|place| ShowKey::Other(place).cap())
            .to_vec()
        });
        assert_eq!(caps, ["B", "3", ",", "Num 1", "Num +", "Insert", "F13"]);
    }

    #[test]
    fn adding_a_key_puts_it_at_the_top_set_to_nothing_and_reset_removes_it() {
        let mut table = KeyTable::default();
        let b = ShowKey::Other(KeyCode::KeyB);
        let comma = ShowKey::Other(KeyCode::Comma);
        assert_eq!(table.add(&typing("b", KeyCode::KeyB), true), Ok(b));
        assert_eq!(table.add(&typing(",", KeyCode::Comma), true), Ok(comma));
        assert_eq!(table.listed(true)[..3], [comma, b, ShowKey::Escape], "newest first, above the rows");
        assert_eq!(table.listed(true).len(), 23);
        assert!(table.is_added(b) && table.is_listed(b, false) && table.is_listed(b, true));
        assert_eq!(table.action(b), KeyAction::Nothing, "set to Nothing");
        assert!(table.any_changed(), "Reset is lit once a key is added");
        // A key already in the list is shown, not added twice: a key added, a row, or the same
        // place typing another letter (ю at "." on a Russian layout).
        assert_eq!(table.add(&typing("B", KeyCode::KeyB), true), Err(Refusal::Listed(b)));
        assert_eq!(table.add(&typing("\u{44e}", KeyCode::Period), true), Err(Refusal::Listed(ShowKey::Period)));
        assert_eq!(table.add(&named(NamedKey::Escape, KeyCode::Escape), true), Err(Refusal::Listed(ShowKey::Escape)));
        assert_eq!(table.add(&named(NamedKey::F5, KeyCode::F5), true), Err(Refusal::Listed(ShowKey::F5)));
        assert_eq!(table.listed(true).len(), 23);
        // Set to something, it keeps it, even set to Nothing again.
        table.set(b, KeyAction::ClearAll);
        assert_eq!(table.keys_for(KeyAction::ClearAll, false), [b, ShowKey::Escape, ShowKey::Period]);
        table.set(b, KeyAction::Nothing);
        assert!(table.is_added(b), "an added key at Nothing stays in the list");
        // Saved with the preferences, newest first, and read back the same.
        table.set(comma, KeyAction::Next);
        let saved = serde_json::to_value(&table).unwrap();
        assert_eq!(saved["added"], serde_json::json!(["comma", "b"]));
        assert_eq!(saved["changed"], serde_json::json!({"b": "nothing", "comma": "next"}));
        let again: KeyTable = serde_json::from_value(saved).unwrap();
        assert_eq!(again, table);
        // Its × takes it out, with what it was set to; a row can't be taken out.
        table.remove(comma);
        assert_eq!(table.listed(true)[..2], [b, ShowKey::Escape]);
        assert_eq!(table.action(comma), KeyAction::Nothing);
        table.remove(ShowKey::Escape);
        assert!(table.is_listed(ShowKey::Escape, false));
        // Insert can be added too (the design allows it).
        assert_eq!(table.add(&named(NamedKey::Insert, KeyCode::Insert), true), Ok(ShowKey::Other(KeyCode::Insert)));
        // Reset to Defaults sets the rows and takes the keys added out.
        table.set(ShowKey::Escape, KeyAction::ClearSlide);
        table.reset();
        assert_eq!(table.listed(true), rows(true));
        assert!(!table.is_added(b) && !table.is_listed(b, true));
        assert_eq!(table.action(ShowKey::Escape), KeyAction::ClearAll);
        assert!(!table.any_changed());
        assert_eq!(table, KeyTable::default());
    }

    #[test]
    fn choosing_a_layout_keeps_added_keys() {
        let mut table = KeyTable::default();
        let b = ShowKey::Other(KeyCode::KeyB);
        let m = ShowKey::Other(KeyCode::KeyM);
        table.add(&typing("b", KeyCode::KeyB), true).unwrap();
        table.add(&typing("m", KeyCode::KeyM), true).unwrap();
        table.set(b, KeyAction::ClearAll);
        table.set(ShowKey::F2, KeyAction::Nothing);
        table.set_style(KeyStyle::SoftProjector);
        // The layout's own rows: the change to F2 goes.
        assert_eq!(table.action(ShowKey::F2), KeyAction::AddToPlaylist);
        assert_eq!(table.action(ShowKey::Period), KeyAction::HideWords);
        // The keys added stay, where they were and as they were set.
        assert_eq!(table.listed(true)[..3], [m, b, ShowKey::Escape]);
        assert_eq!((table.action(b), table.action(m)), (KeyAction::ClearAll, KeyAction::Nothing));
        assert!(table.any_changed(), "keys added are still there to reset");
        table.set_style(KeyStyle::SoftPresenter);
        assert_eq!(table.action(b), KeyAction::ClearAll, "and back again");
    }

    #[test]
    fn an_f_key_added_on_a_mac_starts_at_its_layouts_action_and_is_read_there() {
        let mut table = KeyTable::default();
        table.set_style(KeyStyle::SoftProjector);
        let f1 = named(NamedKey::F1, KeyCode::F1);
        assert_eq!(table.row_of(&f1, false), None, "a Mac's F keys are the system's");
        assert_eq!(table.add(&f1, false), Ok(ShowKey::F1));
        assert_eq!(table.action(ShowKey::F1), KeyAction::ClearAll, "F1 = Clear All, like SoftProjector");
        assert_eq!(table.row_of(&f1, false), Some(ShowKey::F1));
        assert_eq!(table.listed(false)[..2], [ShowKey::F1, ShowKey::Escape]);
        assert_eq!(table.add(&f1, false), Err(Refusal::Listed(ShowKey::F1)));
        // Off a Mac it is a row anyway, listed once in its place, and does what it was set to.
        table.set(ShowKey::F1, KeyAction::ShowAgain);
        assert_eq!(table.listed(true), rows(true));
        assert_eq!(table.action(ShowKey::F1), KeyAction::ShowAgain);
        // A layout chosen leaves it as it was set.
        table.set_style(KeyStyle::SoftPresenter);
        assert_eq!(table.action(ShowKey::F1), KeyAction::ShowAgain);
        // A PC's file with an F row changed, and nothing added, reads no F key on a Mac.
        let from_a_pc: KeyTable = serde_json::from_value(serde_json::json!({"changed": {"f2": "hide_words"}})).unwrap();
        assert_eq!(from_a_pc.row_of(&named(NamedKey::F2, KeyCode::F2), false), None);
        assert_eq!(from_a_pc.listed(false), rows(false));
    }

    #[test]
    fn the_refused_keys_and_modifiers_are_refused_with_a_reason() {
        let mut table = KeyTable::default();
        let kept = [
            (named(NamedKey::Enter, KeyCode::Enter), Kept::Enter),
            (named(NamedKey::Enter, KeyCode::NumpadEnter), Kept::Enter),
            (named(NamedKey::Tab, KeyCode::Tab), Kept::Tab),
            (named(NamedKey::ArrowUp, KeyCode::ArrowUp), Kept::Up),
            (named(NamedKey::ArrowDown, KeyCode::ArrowDown), Kept::Down),
            (typing("/", KeyCode::Slash), Kept::Slash),
            // ? is Shift-/ on a US keyboard, Shift-ß on a German one: it opens the list of keys.
            (held(typing("?", KeyCode::Slash), ModifiersState::SHIFT), Kept::QuestionMark),
            (held(typing("?", KeyCode::Minus), ModifiersState::SHIFT), Kept::QuestionMark),
            // / is Shift-7 on a German keyboard.
            (held(typing("/", KeyCode::Digit7), ModifiersState::SHIFT), Kept::Slash),
            (named(NamedKey::Delete, KeyCode::Delete), Kept::Delete),
            (named(NamedKey::Backspace, KeyCode::Backspace), Kept::Backspace),
        ];
        for (pressed, why) in &kept {
            assert_eq!(table.add(pressed, true), Err(Refusal::Kept(*why)), "{pressed:?}");
        }
        // Held with ⌘, ⌃ or ⌥ (Ctrl or Alt): the menus'. Shift is no refusal.
        for modifier in [ModifiersState::SUPER, ModifiersState::CONTROL, ModifiersState::ALT] {
            assert_eq!(table.add(&held(typing("b", KeyCode::KeyB), modifier), true), Err(Refusal::Held));
        }
        // A modifier by itself is no key at all: nothing is said.
        for alone in [
            named(NamedKey::Shift, KeyCode::ShiftLeft),
            held(named(NamedKey::Control, KeyCode::ControlLeft), ModifiersState::CONTROL),
            held(named(NamedKey::Super, KeyCode::SuperRight), ModifiersState::SUPER),
            named(NamedKey::CapsLock, KeyCode::CapsLock),
        ] {
            assert_eq!(table.add(&alone, true), Err(Refusal::NoKey), "{alone:?}");
        }
        assert_eq!(table.listed(true), rows(true), "nothing was added");
        assert_eq!(Refusal::NoKey.reason(true), None);
        let said = |refusal: Refusal, mac: bool| with_language(Language::English, || refusal.reason(mac));
        let say = |words: &str| Some(words.to_owned());
        assert_eq!(
            said(Refusal::Kept(Kept::Enter), false),
            say("Enter always puts the picked slide on screen. Press another.")
        );
        assert_eq!(said(Refusal::Kept(Kept::Tab), false), say("Tab always moves between buttons. Press another."));
        assert_eq!(said(Refusal::Kept(Kept::Up), false), say("\u{2191} always goes to the item above. Press another."));
        assert_eq!(
            said(Refusal::Kept(Kept::Down), false),
            say("\u{2193} always goes to the item below. Press another.")
        );
        assert_eq!(said(Refusal::Kept(Kept::Slash), false), say("/ always opens the find box. Press another."));
        assert_eq!(
            said(Refusal::Kept(Kept::QuestionMark), false),
            say("? always opens the list of keys. Press another.")
        );
        assert_eq!(
            said(Refusal::Kept(Kept::Delete), false),
            say("Delete always deletes what is picked. Press another.")
        );
        assert_eq!(
            said(Refusal::Kept(Kept::Backspace), true),
            say("Delete always deletes what is picked. Press another.")
        );
        assert_eq!(
            said(Refusal::Kept(Kept::Backspace), false),
            say("Backspace always deletes what is picked. Press another.")
        );
        assert_eq!(
            said(Refusal::Held, true),
            say("Keys held with \u{2318}, \u{2303} or \u{2325} are the menus\u{2019}. Press a key by itself.")
        );
        assert_eq!(
            said(Refusal::Held, false),
            say("Keys held with Ctrl or Alt are the menus\u{2019}. Press a key by itself.")
        );
        assert_eq!(said(Refusal::Listed(ShowKey::PageDown), false), say("Page Down is in the list already."));
        assert_eq!(said(Refusal::Listed(ShowKey::Other(KeyCode::KeyB)), false), say("B is in the list already."));
        // Every reason is there in every language.
        for language in Language::ALL {
            for (_, why) in &kept {
                assert!(
                    with_language(language, || Refusal::Kept(*why).reason(false)).is_some_and(|said| !said.is_empty())
                );
            }
        }
    }

    #[test]
    fn keys_added_are_read_back_from_a_damaged_file_as_far_as_they_can_be() {
        let saved = serde_json::json!({
            "changed": {"b": "next", "f20": "clear_all"},
            "added": ["b", 7, "b", "something_newer", "f20"]
        });
        let table: KeyTable = serde_json::from_value(saved).unwrap();
        let b = ShowKey::Other(KeyCode::KeyB);
        let f20 = ShowKey::Other(KeyCode::F20);
        assert_eq!(table.listed(true)[..3], [b, f20, ShowKey::Escape], "a name not known is passed over");
        assert_eq!((table.action(b), table.action(f20)), (KeyAction::Next, KeyAction::ClearAll));
        // The name this copy doesn't know is kept for the copy that wrote it; a wrong shape isn't.
        assert_eq!(serde_json::to_value(&table).unwrap()["added"], serde_json::json!(["b", "something_newer", "f20"]));
        let not_a_list: KeyTable = serde_json::from_value(serde_json::json!({"added": "b"})).unwrap();
        assert_eq!(not_a_list, KeyTable::default());
    }

    #[test]
    fn the_row_of_a_key_goes_by_its_place_then_by_the_key_it_is() {
        let mut table = KeyTable::default();
        // By its place: "." on a Russian layout types ю, and the key at / there types ".".
        assert_eq!(table.row_of(&typing("\u{44e}", KeyCode::Period), true), Some(ShowKey::Period));
        assert_eq!(table.row_of(&typing(".", KeyCode::Slash), true), None);
        // A letter not added is no row: it is a hot key or a group's letter.
        assert_eq!(table.row_of(&typing("c", KeyCode::KeyC), true), None);
        table.add(&typing("c", KeyCode::KeyC), true).unwrap();
        let c = Some(ShowKey::Other(KeyCode::KeyC));
        assert_eq!(table.row_of(&typing("c", KeyCode::KeyC), true), c);
        assert_eq!(table.row_of(&typing("\u{441}", KeyCode::KeyC), true), c, "the Russian letter at C's place");
        assert_eq!(table.row_of(&held(typing("C", KeyCode::KeyC), ModifiersState::SHIFT), true), c);
        // Typed with ⌥ (Alt), a character is the window's: ⌥1 numbers a tab's things.
        table.add(&typing("1", KeyCode::Digit1), true).unwrap();
        assert_eq!(table.row_of(&held(typing("\u{a1}", KeyCode::Digit1), ModifiersState::ALT), true), None);
        assert_eq!(table.row_of(&held(typing("1", KeyCode::Digit1), ModifiersState::ALT), true), None);
        assert_eq!(table.row_of(&typing("1", KeyCode::Digit1), true), Some(ShowKey::Other(KeyCode::Digit1)));
        // / and ? are never a row, wherever the layout puts them (German: Shift-7, Shift-ß).
        table.add(&typing("7", KeyCode::Digit7), true).unwrap();
        table.add(&typing("\u{df}", KeyCode::Minus), true).unwrap();
        assert_eq!(table.row_of(&held(typing("/", KeyCode::Digit7), ModifiersState::SHIFT), true), None);
        assert_eq!(table.row_of(&held(typing("?", KeyCode::Minus), ModifiersState::SHIFT), true), None);
        // A place with no row goes by the key it is: the number pad's 6 with Num Lock off is →,
        // and a key whose place the system doesn't say is the key it reports.
        assert_eq!(table.row_of(&named(NamedKey::ArrowRight, KeyCode::Numpad6), true), Some(ShowKey::Right));
        assert_eq!(table.row_of(&Pressed::from(Key::Named(NamedKey::PageUp)), true), Some(ShowKey::PageUp));
        let unknown = PhysicalKey::Unidentified(winit::keyboard::NativeKeyCode::Unidentified);
        let space = Pressed { logical: Key::Named(NamedKey::Space), physical: unknown, held: ModifiersState::empty() };
        assert_eq!(table.row_of(&space, true), Some(ShowKey::Space));
        // Added, the number pad's 6 is its own row.
        table.add(&typing("6", KeyCode::Numpad6), true).unwrap();
        assert_eq!(
            table.row_of(&named(NamedKey::ArrowRight, KeyCode::Numpad6), true),
            Some(ShowKey::Other(KeyCode::Numpad6))
        );
        // The keys the app keeps have none.
        assert_eq!(table.row_of(&named(NamedKey::Enter, KeyCode::Enter), true), None);
        assert_eq!(table.row_of(&named(NamedKey::ArrowUp, KeyCode::ArrowUp), true), None);
    }

    #[test]
    fn a_changed_row_is_kept_and_choosing_its_default_again_forgets_it() {
        let mut table = KeyTable::default();
        table.set(ShowKey::Escape, KeyAction::ClearSlide);
        assert_eq!(table.action(ShowKey::Escape), KeyAction::ClearSlide);
        assert!(table.is_changed(ShowKey::Escape));
        assert!(table.any_changed());
        assert!(!table.is_changed(ShowKey::F1), "the other rows are as they came");
        assert_eq!(table.action(ShowKey::F1), KeyAction::ClearAll);
        table.set(ShowKey::Escape, KeyAction::ClearAll);
        assert!(!table.is_changed(ShowKey::Escape));
        assert!(!table.any_changed());
        assert_eq!(table, KeyTable::default(), "nothing is left of the change");
    }

    #[test]
    fn choosing_a_style_sets_every_row_and_drops_changes() {
        let mut table = KeyTable::default();
        table.set(ShowKey::F2, KeyAction::Nothing);
        table.set_style(KeyStyle::SoftProjector);
        assert_eq!(table.style, KeyStyle::SoftProjector);
        assert_eq!(table.action(ShowKey::F2), KeyAction::AddToPlaylist);
        assert!(!table.any_changed());
        // In this style Clear All on Esc is a change, and Reset goes back to this style's Esc.
        table.set(ShowKey::Escape, KeyAction::ClearAll);
        assert!(table.is_changed(ShowKey::Escape));
        table.reset();
        assert_eq!((table.style, table.action(ShowKey::Escape)), (KeyStyle::SoftProjector, KeyAction::HideWords));
        assert!(!table.any_changed());
    }

    #[test]
    fn keys_for_finds_every_key_doing_an_action() {
        let table = KeyTable::default();
        assert_eq!(table.keys_for(KeyAction::ClearAll, true), [ShowKey::Escape, ShowKey::F1, ShowKey::Period]);
        assert_eq!(table.keys_for(KeyAction::ClearAll, false), [ShowKey::Escape, ShowKey::Period]);
        assert_eq!(table.keys_for(KeyAction::Next, false), [ShowKey::Space, ShowKey::Right, ShowKey::PageDown]);
        assert!(table.keys_for(KeyAction::ShowAgain, true).is_empty(), "no key does it in this style");
        let mut table = table;
        table.set(ShowKey::F2, KeyAction::ClearAll);
        assert_eq!(
            table.keys_for(KeyAction::ClearAll, true),
            [ShowKey::Escape, ShowKey::F1, ShowKey::F2, ShowKey::Period]
        );
        assert_eq!(
            table.keys_for(KeyAction::ClearAll, false),
            [ShowKey::Escape, ShowKey::Period],
            "an F row only off a Mac"
        );
        // SoftProjector's style clears everything with F1 alone, so a Mac has no key for it there.
        table.set_style(KeyStyle::SoftProjector);
        assert_eq!(table.keys_for(KeyAction::ClearAll, true), [ShowKey::F1]);
        assert!(table.keys_for(KeyAction::ClearAll, false).is_empty());
        assert_eq!(table.keys_for(KeyAction::HideWords, false), [ShowKey::Escape, ShowKey::Period]);
    }

    #[test]
    fn every_action_is_saved_by_its_name_and_listed_in_its_group() {
        let names: Vec<&str> = KeyAction::ALL.iter().map(|action| action.name()).collect();
        assert_eq!(
            names,
            [
                "clear_all",
                "hide_words",
                "clear_slide",
                "clear_media",
                "clear_props",
                "clear_audio",
                "clear_messages",
                "show_selected",
                "show_again",
                "next",
                "previous",
                "first",
                "last",
                "add_to_playlist",
                "remove_from_playlist",
                "go_bible",
                "go_service",
                "nothing",
            ]
        );
        for action in KeyAction::ALL {
            assert_eq!(KeyAction::from_name(action.name()), Some(action));
        }
        assert_eq!(KeyAction::from_name("fly_away"), None);
        // ALL walks the groups in order, each group's actions together.
        let groups: Vec<ActionGroup> = KeyAction::ALL.iter().map(|action| action.group()).collect();
        let mut expected = vec![ActionGroup::Clear; 7];
        expected.extend([ActionGroup::Slides; 6]);
        expected.extend([ActionGroup::PlaylistViews; 4]);
        expected.push(ActionGroup::None);
        assert_eq!(groups, expected);
        let words: Vec<String> = with_language(Language::English, || KeyAction::ALL.map(KeyAction::words).to_vec());
        assert_eq!(
            words,
            [
                "Clear All",
                "Hide the Words",
                "Clear Slide",
                "Clear Media",
                "Clear Props",
                "Clear Audio",
                "Clear Messages",
                "Show Selected",
                "Show Again",
                "Next Slide",
                "Previous Slide",
                "First Slide",
                "Last Slide",
                "Add to Playlist",
                "Remove from Playlist",
                "Go to Bible",
                "Go to Service",
                "Nothing",
            ]
        );
    }

    #[test]
    fn a_show_key_is_known_by_its_place_or_the_named_key_it_is_and_saved_by_its_name() {
        let at = |code| ShowKey::of(&PhysicalKey::Code(code));
        let places = [
            (KeyCode::Escape, ShowKey::Escape),
            (KeyCode::F1, ShowKey::F1),
            (KeyCode::F8, ShowKey::F8),
            (KeyCode::F9, ShowKey::F9),
            (KeyCode::F12, ShowKey::F12),
            (KeyCode::Space, ShowKey::Space),
            (KeyCode::ArrowRight, ShowKey::Right),
            (KeyCode::ArrowLeft, ShowKey::Left),
            (KeyCode::PageDown, ShowKey::PageDown),
            (KeyCode::PageUp, ShowKey::PageUp),
            (KeyCode::Home, ShowKey::Home),
            (KeyCode::End, ShowKey::End),
            (KeyCode::Period, ShowKey::Period),
        ];
        for (code, show_key) in places {
            assert_eq!(at(code), Some(show_key), "{code:?}");
        }
        let named = |key| ShowKey::named(&Key::Named(key));
        let reported = [
            (NamedKey::Escape, ShowKey::Escape),
            (NamedKey::F1, ShowKey::F1),
            (NamedKey::F12, ShowKey::F12),
            (NamedKey::Space, ShowKey::Space),
            (NamedKey::ArrowRight, ShowKey::Right),
            (NamedKey::ArrowLeft, ShowKey::Left),
            (NamedKey::PageDown, ShowKey::PageDown),
            (NamedKey::PageUp, ShowKey::PageUp),
            (NamedKey::Home, ShowKey::Home),
            (NamedKey::End, ShowKey::End),
        ];
        for (key, show_key) in reported {
            assert_eq!(named(key), Some(show_key), "{key:?}");
        }
        for key in [NamedKey::F13, NamedKey::ArrowUp, NamedKey::ArrowDown, NamedKey::Enter, NamedKey::Insert] {
            assert_eq!(named(key), None, "{key:?} is no row");
        }
        assert_eq!(ShowKey::named(&Key::Character(".".into())), None, "a character goes by its place only");
    }
}
