//! Every control has a hint: the windows drawn in the states a person meets, with Quick Help off,
//! and every control the toolkit knows of in each (everything it made focusable: each key, field,
//! switch, tab, slide, row and card that takes a click or a drag) must have said what it does,
//! why it is greyed or its whole name through `quick_help::Hint`, or that it is no control.
//!
//! A closed menu is never drawn, so its items (which need no hint) drop out by themselves. A
//! state a person meets that isn't drawn here isn't checked: a new view, page or sheet adds its
//! case.

use std::sync::Arc;

use egui::{Rect, pos2, vec2};
use sp_engine::{Command, Show, SlideRef};

use super::tests::{Silent, forget, list, silent};
use crate::home::{HomeState, Pane, Preview};

/// Draws `body` a few frames in a window `size` large (later frames lay out what the first
/// measured), and gives the controls that said nothing in the last.
fn draw(size: egui::Vec2, mut body: impl FnMut(&mut egui::Ui)) -> Vec<Silent> {
    let ctx = egui::Context::default();
    crate::studio::apply_style(&ctx);
    ctx.enable_accesskit();
    let mut last = Vec::new();
    for frame in 0..3 {
        forget();
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), size)),
            time: Some(f64::from(frame) * 0.1),
            ..egui::RawInput::default()
        };
        let output = ctx.run_ui(raw, &mut body);
        if frame == 2 {
            last = silent(&output);
        }
        output.drop_without_applying_deltas();
    }
    last
}

/// The operator's window, 1440 × 900, in `state`.
fn home(show: &Show, state: &mut HomeState) -> Vec<Silent> {
    draw(vec2(1440.0, 900.0), |ui| {
        crate::home::home_ui(ui, show, state, &sp_model::Theme::builtin(), &mut |_| None, Preview::default(), None);
    })
}

/// The Settings window at `pane`, tall enough for a whole page to lay out.
fn settings(show: &Show, state: &mut HomeState, pane: Pane) -> Vec<Silent> {
    state.preferences_pane = pane;
    draw(vec2(1100.0, 3200.0), |ui| crate::home::settings_ui(ui, state, show))
}

/// The Screens window.
fn screens(state: &mut HomeState) -> Vec<Silent> {
    draw(vec2(1100.0, 1600.0), |ui| {
        crate::home::screens_ui(ui, state, true);
    })
}

/// Three songs of three slides (two in a group), the first open with its second slide on screen, and two pictures
/// in the media bin.
fn sample() -> Show {
    let songs = ["Morning Song", "Evening Song", "A Third Song"]
        .iter()
        .map(|title| {
            Arc::new(sp_model::parse_song_text(title, "[Verse 1]\nWords one\n\nWords two\n\n[Chorus]\nWords three\n"))
        })
        .collect();
    let mut show = Show::new(songs);
    for name in ["a.png", "b.png"] {
        show.add_media(sp_model::MediaItem::new(name.into(), name.into()));
    }
    show.apply(Command::GoTo(SlideRef { presentation: 0, slide: 1 }));
    show
}

/// Every case: its name and the controls in it that said nothing.
fn cases() -> Vec<(String, Vec<Silent>)> {
    let show = sample();
    let mut cases = Vec::new();
    let mut case = |name: &str, silent: Vec<Silent>| cases.push((name.to_owned(), silent));

    // The Show view, each tool tab of the right column in turn.
    for tab in 0..7 {
        let mut state = HomeState { live_tab: tab, ..HomeState::default() };
        case(&format!("Show view, tool tab {tab}"), home(&show, &mut state));
    }
    // The left column listing a playlist.
    let mut state = HomeState { playlist_open: true, ..HomeState::default() };
    case("Show view, a playlist", home(&show, &mut state));
    // The Bible view.
    let mut state = HomeState { bible_open: true, ..HomeState::default() };
    case("Bible view", home(&show, &mut state));
    // Edit: a song's slide.
    let editing = crate::editor::EditDraft::of(&show, SlideRef { presentation: 0, slide: 0 });
    let mut state = HomeState { editing, ..HomeState::default() };
    case("Edit, a slide", home(&show, &mut state));
    // Edit: a box picked on the slide, each tab of the inspector.
    use crate::inspector::InspectorTab;
    for tab in [InspectorTab::Own, InspectorTab::Box, InspectorTab::Animation] {
        let mut editing = crate::editor::EditDraft::of(&show, SlideRef { presentation: 0, slide: 0 });
        if let Some(draft) = editing.as_mut() {
            draft.select_only(draft.slide.objects.first().map(|object| object.id));
        }
        let mut state = HomeState { editing, ..HomeState::default() };
        state.inspector.tab = tab;
        case(&format!("Edit, a box picked, {tab:?}"), home(&show, &mut state));
    }
    // Edit › Themes.
    let mut state = HomeState {
        theme_edit: Some(crate::home::ThemeEdit::of(0, sp_model::Theme::builtin())),
        ..HomeState::default()
    };
    case("Edit, Themes", home(&show, &mut state));
    // Edit › Stage and Edit › Props.
    let mut state = HomeState { stage_editor_open: true, ..HomeState::default() };
    case("Edit, Stage", home(&show, &mut state));
    let mut state = HomeState { props_editor_open: true, ..HomeState::default() };
    case("Edit, Props", home(&show, &mut state));
    // The list of every key.
    let mut state = HomeState { key_sheet: true, ..HomeState::default() };
    case("The list of keys", home(&show, &mut state));
    // Each step of the first-time setup.
    use crate::setup::Step;
    for step in [Step::Welcome, Step::Languages, Step::Songs, Step::Bibles, Step::Screens, Step::Ready] {
        let mut setup = crate::setup::Setup::new();
        setup.step = step;
        let mut state = HomeState { setup: Some(setup), ..HomeState::default() };
        case(&format!("Setup, {step:?}"), home(&show, &mut state));
    }
    // Every page of Settings.
    let panes = [
        Pane::Screens,
        Pane::PhoneScreens,
        Pane::Slides,
        Pane::Groups,
        Pane::Language,
        Pane::Integrations,
        Pane::AiAssistants,
        Pane::PhoneRemote,
        Pane::Keys,
        Pane::CommandLine,
        Pane::SongFiles,
        Pane::About,
    ];
    for pane in panes {
        let mut state = HomeState::default();
        case(&format!("Settings, {pane:?}"), settings(&show, &mut state, pane));
    }
    // Settings' window asking for an integration.
    let mut state = HomeState::default();
    state.integration_request.open();
    case("Settings, Request an Integration", settings(&show, &mut state, Pane::Integrations));
    // The Screens window.
    let mut state = HomeState::default();
    case("Screens window", screens(&mut state));
    cases
}

#[test]
fn every_control_has_a_hint() {
    let cases = cases();
    let report: Vec<String> = cases
        .iter()
        .filter(|(_, silent)| !silent.is_empty())
        .map(|(name, silent)| format!("{name}: {} without a hint\n{}", silent.len(), list(silent)))
        .collect();
    let total: usize = cases.iter().map(|(_, silent)| silent.len()).sum();
    assert!(report.is_empty(), "{total} controls say nothing (quick_help::Hint):\n{}", report.join("\n"));
}
