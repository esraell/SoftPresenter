//! What each screen's window should do when the displays change: the decision, apart from the
//! windows themselves, so it can be tried with made-up displays.
//!
//! A screen that fills a display must never end up on the operator's. When a display goes (a
//! projector switched off, an HDMI signal lost) the system moves its windows to the main
//! display; the plan says "hide" for such a window, and the window comes back by itself when the
//! display does. Nothing here asks the system anything.

use std::collections::BTreeMap;

use sp_model::Uuid;

use crate::displays::{DisplayInfo, Rect};
use crate::screens::{Kind, Output, Placed, Screen};

/// How often a window may be put back on its display, when it is found somewhere else, before it
/// is hidden instead.
pub const TRIES: u8 = 3;

/// What a screen's window is like now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Seen {
    pub screen: Uuid,
    /// Not hidden.
    pub shown: bool,
    /// The display the system says it is on, as that display's rectangle.
    pub on: Option<Rect>,
    /// The display's rectangle it was last put on.
    pub put_on: Option<Rect>,
    /// How many times it has been put back since, after being found elsewhere.
    pub tries: u8,
    /// It has had time to get where it was put (the system may take a moment to say so).
    pub settled: bool,
}

/// Why a window is hidden.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    /// Its display is not connected.
    Disconnected,
    /// Its display is the main one, where the operator works.
    Operator,
    /// It would not stay on its display.
    Stray,
    /// The screen is switched off.
    Off,
}

impl Why {
    pub fn in_words(self) -> &'static str {
        match self {
            Why::Disconnected => "its display is not connected",
            Why::Operator => "its display is the main one",
            Why::Stray => "it would not stay on its display",
            Why::Off => "the screen is switched off",
        }
    }
}

/// What to do with one screen's window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// Nothing wanted, and no window.
    Idle,
    /// Make a window on this display's rectangle.
    Open(Rect),
    /// The window it has is where it should be and shown.
    Keep,
    /// Put the window it has on this display's rectangle, and show it.
    Place(Rect),
    /// Hide the window at once (kept, or closed, as the system suits).
    Hide(Why),
    /// Not a window on a display (an ordinary window, NDI, DeckLink): handled as before.
    Other,
    /// No window is wanted: close the one it has.
    Close,
}

/// One step per screen, in order. `placed` is where each screen goes (`screens::place_all`, and
/// quiet runs' changes), `on` whether its kind of screen is switched on, `windows` the windows
/// there are now.
pub fn plan(
    screens: &[Screen],
    placed: &[Placed],
    on: &[bool],
    displays: &[DisplayInfo],
    windows: &[Seen],
) -> Vec<Step> {
    let rect = |display: &DisplayInfo| Rect { x: display.x, y: display.y, w: display.w, h: display.h };
    screens
        .iter()
        .zip(placed)
        .zip(on)
        .map(|((screen, place), on)| {
            let window = windows.iter().find(|have| have.screen == screen.id);
            match place {
                Placed::Fullscreen(index) => {
                    let Some(display) = displays.iter().find(|d| d.index == *index) else {
                        return hide_or_idle(window, Why::Disconnected);
                    };
                    if display.main {
                        return hide_or_idle(window, Why::Operator);
                    }
                    let want = rect(display);
                    let Some(have) = window else { return Step::Open(want) };
                    if !have.shown {
                        return if have.tries >= TRIES && have.put_on == Some(want) {
                            Step::Hide(Why::Stray)
                        } else {
                            Step::Place(want)
                        };
                    }
                    if have.put_on != Some(want) {
                        return Step::Place(want);
                    }
                    match have.on {
                        Some(at) if at != want && have.settled => {
                            if have.tries < TRIES {
                                Step::Place(want)
                            } else {
                                Step::Hide(Why::Stray)
                            }
                        }
                        _ => Step::Keep,
                    }
                }
                Placed::Nothing(_) => {
                    let Output::Display(name) = &screen.output else {
                        return if window.is_some() { Step::Close } else { Step::Idle };
                    };
                    if !*on {
                        return hide_or_idle(window, Why::Off);
                    }
                    match displays.iter().find(|d| d.is_named(name)) {
                        None => hide_or_idle(window, Why::Disconnected),
                        Some(display) if display.main => hide_or_idle(window, Why::Operator),
                        // Another screen has the display, or the operator works on it (the only
                        // display, or the home window's where none is known as the main one):
                        // this one has no window.
                        Some(_) => {
                            if window.is_some() {
                                Step::Close
                            } else {
                                Step::Idle
                            }
                        }
                    }
                }
                Placed::Window | Placed::Ndi(_) | Placed::DeckLink(_) => Step::Other,
            }
        })
        .collect()
}

fn hide_or_idle(window: Option<&Seen>, why: Why) -> Step {
    match window {
        Some(have) if have.shown => Step::Hide(why),
        _ => Step::Idle,
    }
}

/// The screens that are on and meant for a display that is not connected, each with what to tell
/// the operator. The words stay the same while it is away, so the same notice is never said
/// twice.
pub fn away(screens: &[Screen], on: &[bool], displays: &[DisplayInfo]) -> BTreeMap<Uuid, String> {
    screens
        .iter()
        .zip(on)
        .filter_map(|(screen, on)| {
            let Output::Display(name) = &screen.output else { return None };
            if !*on || displays.iter().any(|d| d.is_named(name)) {
                return None;
            }
            let kind = match screen.kind {
                Kind::Audience => "audience",
                Kind::Stage => "stage",
            };
            Some((
                screen.id,
                format!(
                    "The {kind} display ({name}) was disconnected. The {kind} screen will come back when it returns."
                ),
            ))
        })
        .collect()
}

/// What to do with the notices when the screens that are away change: those to say, and those
/// to take away (the display came back, or the screen was switched off or pointed elsewhere).
pub fn notice_changes(before: &BTreeMap<Uuid, String>, now: &BTreeMap<Uuid, String>) -> (Vec<String>, Vec<String>) {
    let said: Vec<String> =
        now.iter().filter(|(id, text)| before.get(id) != Some(text)).map(|(_, text)| text.clone()).collect();
    let gone: Vec<String> =
        before.iter().filter(|(id, text)| now.get(id) != Some(text)).map(|(_, text)| text.clone()).collect();
    (said, gone)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn display(index: usize, name: &str, main: bool, x: i32) -> DisplayInfo {
        DisplayInfo { index, name: name.to_owned(), old_name: String::new(), main, x, y: 0, w: 1920, h: 1080 }
    }

    fn rect_of(display: &DisplayInfo) -> Rect {
        Rect { x: display.x, y: display.y, w: display.w, h: display.h }
    }

    fn audience(on_display: &str) -> Screen {
        Screen::new("Audience".to_owned(), Output::Display(on_display.to_owned()))
    }

    /// What the app does each time: place the screens, then plan the windows.
    fn run(screens: &[Screen], displays: &[DisplayInfo], windows: &[Seen]) -> Vec<Step> {
        let placed = crate::screens::place_all(screens, displays, &[0]);
        let on = vec![true; screens.len()];
        plan(screens, &placed, &on, displays, windows)
    }

    fn shown_on(screen: &Screen, display: &DisplayInfo) -> Seen {
        Seen {
            screen: screen.id,
            shown: true,
            on: Some(rect_of(display)),
            put_on: Some(rect_of(display)),
            tries: 0,
            settled: true,
        }
    }

    #[test]
    fn a_screen_opens_on_its_display() {
        let (main, tv) = (display(0, "Built-in", true, 0), display(1, "3DTV", false, 1920));
        let screen = audience("3DTV");
        let steps = run(std::slice::from_ref(&screen), &[main, tv.clone()], &[]);
        assert_eq!(steps, [Step::Open(rect_of(&tv))]);
    }

    #[test]
    fn unplugging_hides_the_window_and_replugging_puts_it_back() {
        let (main, tv) = (display(0, "Built-in", true, 0), display(1, "3DTV", false, 1920));
        let screen = audience("3DTV");
        let one = std::slice::from_ref(&screen);
        let mut window = shown_on(&screen, &tv);
        // The display goes; the system has moved the window onto the main display.
        window.on = Some(rect_of(&main));
        let steps = run(one, std::slice::from_ref(&main), &[window]);
        assert_eq!(steps, [Step::Hide(Why::Disconnected)]);
        // Hidden: nothing more to do while the display is away, however often it is looked at.
        window.shown = false;
        assert_eq!(run(one, std::slice::from_ref(&main), &[window]), [Step::Idle]);
        // The display returns: the window is put back on it and shown.
        let steps = run(one, &[main, tv.clone()], &[window]);
        assert_eq!(steps, [Step::Place(rect_of(&tv))]);
    }

    #[test]
    fn a_window_found_on_another_display_is_put_back_a_few_times_then_hidden() {
        let (main, tv) = (display(0, "Built-in", true, 0), display(1, "3DTV", false, 1920));
        let screen = audience("3DTV");
        let one = std::slice::from_ref(&screen);
        let displays = [main.clone(), tv.clone()];
        let mut window = shown_on(&screen, &tv);
        window.on = Some(rect_of(&main));
        // Not yet settled: the system may not have caught up.
        window.settled = false;
        assert_eq!(run(one, &displays, &[window]), [Step::Keep]);
        window.settled = true;
        assert_eq!(run(one, &displays, &[window]), [Step::Place(rect_of(&tv))]);
        window.tries = TRIES;
        assert_eq!(run(one, &displays, &[window]), [Step::Hide(Why::Stray)]);
        // Hidden as a stray it stays so (no flapping) until the display's place changes.
        window.shown = false;
        assert_eq!(run(one, &displays, &[window]), [Step::Hide(Why::Stray)]);
        let moved = display(1, "3DTV", false, 2560);
        assert_eq!(run(one, &[main, moved.clone()], &[window]), [Step::Place(rect_of(&moved))]);
    }

    #[test]
    fn the_displays_changing_order_moves_the_window_by_name() {
        let screen = audience("3DTV");
        let one = std::slice::from_ref(&screen);
        let before = [display(0, "Built-in", true, 0), display(1, "3DTV", false, 1920)];
        let window = shown_on(&screen, &before[1]);
        // The list now has the projector first, and to the left of the main display.
        let after = [display(0, "3DTV", false, -1920), display(1, "Built-in", true, 0)];
        let steps = run(one, &after, &[window]);
        assert_eq!(steps, [Step::Place(rect_of(&after[0]))]);
    }

    #[test]
    fn the_main_display_changing_hides_a_screen_that_is_now_on_it() {
        let screen = audience("3DTV");
        let one = std::slice::from_ref(&screen);
        let before = [display(0, "Built-in", true, 0), display(1, "3DTV", false, 1920)];
        let window = shown_on(&screen, &before[1]);
        // The projector is made the main display (menu bar and all).
        let after = [display(0, "Built-in", false, 0), display(1, "3DTV", true, 1920)];
        assert_eq!(run(one, &after, &[window]), [Step::Hide(Why::Operator)]);
        // And back again: shown on it once more.
        let steps = run(one, &before, &[Seen { shown: false, ..window }]);
        assert_eq!(steps, [Step::Place(rect_of(&before[1]))]);
    }

    #[test]
    fn two_displays_with_the_same_name_are_told_apart_by_their_numbers() {
        // "HP 27f" and "HP 27f (2)", as the app names them: a screen set up for the second
        // goes to the second, and leaves when only the first is plugged.
        let main = display(0, "Built-in", true, 0);
        let (one_tv, two_tv) = (display(1, "HP 27f", false, 1920), display(2, "HP 27f (2)", false, 3840));
        let screen = audience("HP 27f (2)");
        let one = std::slice::from_ref(&screen);
        let steps = run(one, &[main.clone(), one_tv.clone(), two_tv.clone()], &[]);
        assert_eq!(steps, [Step::Open(rect_of(&two_tv))]);
        let window = shown_on(&screen, &two_tv);
        let steps = run(one, &[main, one_tv], &[window]);
        assert_eq!(steps, [Step::Hide(Why::Disconnected)]);
    }

    #[test]
    fn a_screen_switched_off_hides_and_one_with_no_display_set_does_not() {
        let (main, tv) = (display(0, "Built-in", true, 0), display(1, "3DTV", false, 1920));
        let screen = audience("3DTV");
        let one = std::slice::from_ref(&screen);
        let displays = [main, tv.clone()];
        let window = shown_on(&screen, &tv);
        let placed = vec![Placed::Nothing("Switched off".to_owned())];
        assert_eq!(plan(one, &placed, &[false], &displays, &[window]), [Step::Hide(Why::Off)]);
        // A window screen is the old way's.
        let plain = Screen::new("Window".to_owned(), Output::Window);
        let steps = plan(std::slice::from_ref(&plain), &[Placed::Window], &[true], &displays, &[]);
        assert_eq!(steps, [Step::Other]);
    }

    #[test]
    fn the_notice_is_said_once_and_taken_away_when_the_display_returns() {
        let (main, tv) = (display(0, "Built-in", true, 0), display(1, "3DTV", false, 1920));
        let screen = audience("3DTV");
        let one = std::slice::from_ref(&screen);
        let none = away(one, &[true], &[main.clone(), tv.clone()]);
        assert!(none.is_empty());
        let gone = away(one, &[true], std::slice::from_ref(&main));
        let (said, taken) = notice_changes(&none, &gone);
        assert_eq!(
            said,
            ["The audience display (3DTV) was disconnected. The audience screen will come back when it returns."]
        );
        assert!(taken.is_empty());
        // Looked at again with the display still away: nothing new to say.
        let (said, taken) = notice_changes(&gone, &away(one, &[true], std::slice::from_ref(&main)));
        assert!(said.is_empty() && taken.is_empty());
        // The display returns: the notice goes.
        let (said, taken) = notice_changes(&gone, &away(one, &[true], &[main.clone(), tv]));
        assert!(said.is_empty());
        assert_eq!(taken.len(), 1);
        // A screen switched off says nothing about a display that is away.
        assert!(away(one, &[false], &[main]).is_empty());
    }
}
