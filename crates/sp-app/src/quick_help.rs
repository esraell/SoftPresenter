//! Quick Help: what each control does, in one plain sentence under it while the pointer rests on
//! it, once it is asked for. The **?** key at the top right of the top bar switches it on and off;
//! it is off on a new install and remembered after that (`Prefs::quick_help`). The list of every
//! key stays the keyboard's ? and, on a Mac, the Help menu.
//!
//! Off, only three hovers show anywhere:
//! - why a greyed control is greyed ([`Hint::reason`], [`Hint::greyed`]);
//! - the whole of a name that is cut short ([`Hint::full_name`]);
//! - the ? key's own hint ([`Hint::always`]), since it has to explain itself.
//!
//! Every hover in the app goes through [`Hint`], a few methods on a control's `Response`. egui's
//! own `on_hover_text`, `on_hover_ui` and `on_disabled_hover_text` are called here and nowhere
//! else (`no_hover_skips_quick_help` checks the source). Every control that can be pointed at
//! gives one: `every_control_has_a_hint` draws the windows and checks each control the toolkit
//! knows has said what it does.
//!
//! Whether it is on lives in each window's context (the operator's window, Screens and Settings
//! each have their own), written at the top of each window's drawing from
//! `HomeState::quick_help` ([`set_on`]).
//!
//! A hint is also the control's description for a screen reader, on or off, and its name where it
//! has none (an icon key), as egui's own hover text made them; a reason is said that way only
//! while its control is greyed.

use egui::{Response, WidgetText};

fn on_id() -> egui::Id {
    egui::Id::unique("quick help on")
}

/// Says whether Quick Help is on in this window, for its drawing from now on.
pub fn set_on(ctx: &egui::Context, on: bool) {
    ctx.data_mut(|data| data.insert_temp(on_id(), on));
}

/// True while Quick Help is on in this window.
pub fn is_on(ctx: &egui::Context) -> bool {
    ctx.data(|data| data.get_temp(on_id())).unwrap_or(false)
}

/// The hovers a control can have. Each takes the `Response` and gives it back, as egui's own do.
pub trait Hint: Sized {
    /// What the control does, shown under it while Quick Help is on.
    fn hint(self, what: impl Into<WidgetText>) -> Self;
    /// Why the control is greyed, shown always while it is greyed (egui's disabled): a greyed key
    /// gives no other clue.
    fn reason(self, why: impl Into<WidgetText>) -> Self;
    /// Why a control the app draws greyed itself (one that still takes the pointer) is greyed,
    /// shown always. The caller gives it only while the control is greyed.
    fn greyed(self, why: impl Into<WidgetText>) -> Self;
    /// What the control does, or, while `why` gives a reason, why it is greyed (shown always).
    fn hint_or_greyed(self, what: impl Into<WidgetText>, why: Option<&str>) -> Self;
    /// The whole of a name drawn cut short, shown always. The caller gives it only while the
    /// name is cut short, or for a place that always cuts.
    fn full_name(self, name: impl Into<WidgetText>) -> Self;
    /// What the control does, under the whole of its name while `whole` gives it (the name is
    /// drawn cut short): the name shows always, and what it does too while Quick Help is on.
    fn full_name_and_hint(self, what: impl Into<WidgetText>, whole: Option<&str>) -> Self;
    /// A key drawn as a picture alone: its `name` (the label it doesn't have) shows always;
    /// while Quick Help is on, what it does shows in its place (the name, when it says nothing).
    fn name_or_hint(self, name: &str, what: &str) -> Self;
    /// More about something shown (a readout, a row's state, where an item came from): not a
    /// control's hint, nor a reason or a name. It follows Quick Help, as hints do.
    fn status(self, more: impl Into<WidgetText>) -> Self;
    /// A hint shown always: only the ? key's own, which says how to switch Quick Help on.
    fn always(self, what: impl Into<WidgetText>) -> Self;
    /// Something that takes the pointer but is no control of its own (a page's background that
    /// takes a click to let go of a pick, the room a drag is dropped in), so it says nothing.
    fn no_hint(self) -> Self;
}

impl Hint for Response {
    fn hint(self, what: impl Into<WidgetText>) -> Self {
        let what = what.into();
        // Nothing to say shows no empty tip (and leaves the control one that says nothing).
        if what.text().is_empty() {
            return self;
        }
        said(&self, what.text());
        if is_on(&self.ctx) { self.on_hover_text(what) } else { self }
    }

    fn reason(self, why: impl Into<WidgetText>) -> Self {
        let why = why.into();
        // Said for a screen reader only while it is greyed, when it is the reason.
        if !why.text().is_empty() {
            noted(&self);
            if !self.enabled() {
                describe(&self, why.text());
            }
        }
        self.on_disabled_hover_text(why)
    }

    fn greyed(self, why: impl Into<WidgetText>) -> Self {
        let why = why.into();
        if why.text().is_empty() {
            return self;
        }
        said(&self, why.text());
        self.on_hover_text(why)
    }

    fn hint_or_greyed(self, what: impl Into<WidgetText>, why: Option<&str>) -> Self {
        match why {
            Some(why) => self.greyed(why.to_owned()),
            None => self.hint(what),
        }
    }

    fn full_name(self, name: impl Into<WidgetText>) -> Self {
        let name = name.into();
        if name.text().is_empty() {
            return self;
        }
        said(&self, name.text());
        self.on_hover_text(name)
    }

    fn name_or_hint(self, name: &str, what: &str) -> Self {
        // A screen reader is told what it does, on or off, when there is a sentence.
        if !what.is_empty() {
            describe(&self, what);
        }
        if what.is_empty() || !is_on(&self.ctx) { self.full_name(name.to_owned()) } else { self.hint(what.to_owned()) }
    }

    fn full_name_and_hint(self, what: impl Into<WidgetText>, whole: Option<&str>) -> Self {
        let Some(whole) = whole else { return self.hint(what) };
        let what = what.into();
        if what.text().is_empty() {
            return self.full_name(whole.to_owned());
        }
        said(&self, what.text());
        if is_on(&self.ctx) {
            let both = format!("{whole}\n{}", what.text());
            self.on_hover_text(both)
        } else {
            self.on_hover_text(whole.to_owned())
        }
    }

    fn status(self, more: impl Into<WidgetText>) -> Self {
        self.hint(more)
    }

    fn always(self, what: impl Into<WidgetText>) -> Self {
        let what = what.into();
        if what.text().is_empty() {
            return self;
        }
        said(&self, what.text());
        self.on_hover_text(what)
    }

    fn no_hint(self) -> Self {
        #[cfg(test)]
        tests::saw(self.id);
        self
    }
}

/// Gives the control `words` as its description for a screen reader (when it has none yet), and as
/// its name when it has none (an icon key, a picture), as egui's own hover text does; and, in the
/// tests, notes that it said something.
fn said(response: &Response, words: &str) {
    // Words left empty say nothing, and the control stays one that says nothing.
    if words.is_empty() {
        return;
    }
    noted(response);
    describe(response, words);
}

/// Notes, in the tests, that the control said something.
fn noted(_response: &Response) {
    #[cfg(test)]
    tests::saw(_response.id);
}

/// The control's description and (when it has none) name for a screen reader.
fn describe(response: &Response, words: &str) {
    if !response.ctx.has_accesskit_node(response.id) {
        return;
    }
    response.ctx.accesskit_node_builder(response.id, |node| {
        if node.description().is_none() {
            node.set_description(words);
        }
        // A label is named by its words, and one named by another node already has a name.
        if node.role() != egui::accesskit::Role::Label
            && node.labelled_by().is_empty()
            && node.label().is_none_or(str::is_empty)
        {
            node.set_label(words);
        }
    });
}

#[cfg(test)]
mod coverage;

#[cfg(test)]
pub mod tests {
    use std::cell::RefCell;
    use std::collections::HashSet;

    use egui::accesskit::{Action, Node, Role};
    use egui::{Rect, Sense, pos2, vec2};

    use super::*;

    thread_local! {
        /// The controls that said what they do (or that they are no control) in this thread's
        /// drawing since [`forget`].
        static SEEN: RefCell<HashSet<u64>> = RefCell::new(HashSet::new());
    }

    pub(super) fn saw(id: egui::Id) {
        SEEN.with(|seen| seen.borrow_mut().insert(id.value()));
    }

    /// Forgets which controls said what they do, before a drawing.
    pub fn forget() {
        SEEN.with(|seen| seen.borrow_mut().clear());
    }

    /// A control the toolkit knows of that said nothing: its name or words for a screen reader,
    /// where it is, and the words drawn inside it.
    #[derive(Debug)]
    pub struct Silent {
        pub name: String,
        pub rect: Rect,
        pub words: Vec<String>,
    }

    /// Every control in `output` (each thing the toolkit made focusable: a click or drag
    /// sensed, as every key, field, switch, tab, row and card is) that said nothing since
    /// [`forget`]. `output` is a drawing of a context with `enable_accesskit` on.
    pub fn silent(output: &egui::FullOutput) -> Vec<Silent> {
        fn walk(shape: &egui::Shape, into: &mut Vec<(String, Rect)>) {
            match shape {
                egui::Shape::Text(text) => into.push((text.galley.text().to_owned(), text.visual_bounding_rect())),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| walk(shape, into)),
                _ => {}
            }
        }
        let mut texts = Vec::new();
        output.shapes.iter().for_each(|clipped| walk(&clipped.shape, &mut texts));
        let update = output.platform_output.accesskit_update.as_ref().expect("the drawing has accesskit on");
        let seen = SEEN.with(|seen| seen.borrow().clone());
        let control =
            |node: &Node| node.supports_action(Action::Focus) && !matches!(node.role(), Role::Window | Role::ScrollBar);
        update
            .nodes
            .iter()
            .filter(|(id, node)| control(node) && !seen.contains(&id.0))
            .map(|(_, node)| {
                let rect = node.bounds().map_or(Rect::NOTHING, |b| {
                    Rect::from_min_max(pos2(b.x0 as f32, b.y0 as f32), pos2(b.x1 as f32, b.y1 as f32))
                });
                let name = node.label().or(node.value()).unwrap_or_default().to_owned();
                let words = texts
                    .iter()
                    .filter(|(_, at)| rect.expand(1.0).contains_rect(*at))
                    .map(|(text, _)| text.clone())
                    .collect();
                Silent { name, rect, words }
            })
            .collect()
    }

    /// Says each silent control, one to a line, for a test's failure.
    pub fn list(silent: &[Silent]) -> String {
        silent
            .iter()
            .map(|one| {
                format!(
                    "  {:?} at ({:.0}, {:.0})-({:.0}, {:.0}), words inside: {:?}",
                    one.name, one.rect.min.x, one.rect.min.y, one.rect.max.x, one.rect.max.y, one.words
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn off_only_the_three_always_hovers_show_and_on_every_hint_does() {
        // A control under the pointer, hinted one way, a few frames (the tip shows on the second).
        let shown = |on: bool, how: &dyn Fn(Response) -> Response, enabled: bool| {
            let ctx = egui::Context::default();
            ctx.all_styles_mut(|style| style.interaction.tooltip_delay = 0.0);
            let mut texts = Vec::new();
            for frame in 0..3 {
                let raw = egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 300.0))),
                    time: Some(f64::from(frame) * 0.5),
                    events: vec![egui::Event::PointerMoved(pos2(50.0, 50.0))],
                    ..egui::RawInput::default()
                };
                let output = ctx.run_ui(raw, |ui| {
                    set_on(ui.ctx(), on);
                    ui.add_enabled_ui(enabled, |ui| {
                        let rect = Rect::from_min_size(pos2(20.0, 20.0), vec2(60.0, 60.0));
                        how(ui.interact(rect, egui::Id::unique("control"), Sense::click()));
                    });
                });
                texts.clear();
                fn walk(shape: &egui::Shape, into: &mut Vec<String>) {
                    match shape {
                        egui::Shape::Text(text) => into.push(text.galley.text().to_owned()),
                        egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| walk(shape, into)),
                        _ => {}
                    }
                }
                output.shapes.iter().for_each(|clipped| walk(&clipped.shape, &mut texts));
                output.drop_without_applying_deltas();
            }
            texts.iter().any(|text| text == "Words")
        };
        assert!(!shown(false, &|r| r.hint("Words"), true), "off, a hint stays hidden");
        assert!(shown(true, &|r| r.hint("Words"), true), "on, it shows");
        assert!(shown(false, &|r| r.always("Words"), true), "the ? key's own shows off too");
        assert!(shown(false, &|r| r.full_name("Words"), true), "so does a whole name");
        assert!(shown(false, &|r| r.greyed("Words"), true), "and a greyed control's reason");
        assert!(shown(false, &|r| r.reason("Words"), false), "greyed by the toolkit too");
        assert!(!shown(true, &|r| r.reason("Words"), true), "a reason says nothing while not greyed");
        assert!(shown(false, &|r| r.hint_or_greyed("Other", Some("Words")), true));
        assert!(!shown(false, &|r| r.hint_or_greyed("Words", None), true));
        assert!(shown(true, &|r| r.full_name_and_hint("", Some("Words")), true), "a whole name with no hint");
        assert!(shown(false, &|r| r.full_name_and_hint("Other", Some("Words")), true), "off: the name alone");
        assert!(shown(false, &|r| r.name_or_hint("Words", "Other"), true), "a picture key's name, off");
        assert!(shown(true, &|r| r.name_or_hint("Other", "Words"), true), "on: what it does in its place");
        assert!(!shown(true, &|r| r.name_or_hint("Words", "Other"), true), "and not its name as well");
        assert!(shown(true, &|r| r.name_or_hint("Words", ""), true), "on, with nothing to say: its name");
    }

    #[test]
    fn the_check_finds_a_control_that_says_nothing_and_passes_one_that_does() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        forget();
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let at = |y: f32| Rect::from_min_size(pos2(10.0, y), vec2(80.0, 20.0));
            ui.interact(at(10.0), egui::Id::unique("hinted"), Sense::click()).hint("Does a thing.");
            ui.interact(at(40.0), egui::Id::unique("silent"), Sense::click());
            ui.interact(at(70.0), egui::Id::unique("only hover"), Sense::hover());
            ui.interact(at(100.0), egui::Id::unique("background"), Sense::click()).no_hint();
            let _ = ui.button("Unhinted button");
        });
        let silent = silent(&output);
        output.drop_without_applying_deltas();
        assert_eq!(silent.len(), 2, "{}", list(&silent));
        assert!(silent.iter().any(|one| one.rect.min.y == 40.0), "{}", list(&silent));
        assert!(silent.iter().any(|one| one.name == "Unhinted button"), "{}", list(&silent));
    }

    #[test]
    fn a_hint_is_the_controls_description_for_a_screen_reader_on_or_off() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let rect = Rect::from_min_size(pos2(10.0, 10.0), vec2(80.0, 20.0));
            ui.interact(rect, egui::Id::unique("hinted"), Sense::click()).hint("Does a thing.");
        });
        let update = output.platform_output.accesskit_update.as_ref().expect("accesskit is on");
        let id = egui::Id::unique("hinted").accesskit_id();
        let node = update.nodes.iter().find(|(at, _)| *at == id).map(|(_, node)| node).expect("a node");
        assert_eq!(node.description(), Some("Does a thing."));
        assert_eq!(node.label(), Some("Does a thing."), "and its name, as it has none");
        output.drop_without_applying_deltas();
    }

    /// The texts of a drawing.
    fn texts(output: &egui::FullOutput) -> Vec<(String, Rect)> {
        fn walk(shape: &egui::Shape, into: &mut Vec<(String, Rect)>) {
            match shape {
                egui::Shape::Text(text) => into.push((text.galley.text().to_owned(), text.visual_bounding_rect())),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| walk(shape, into)),
                _ => {}
            }
        }
        let mut into = Vec::new();
        output.shapes.iter().for_each(|clipped| walk(&clipped.shape, &mut into));
        into
    }

    /// A drawing of the operator's window with these events, giving the texts drawn.
    type Frame<'a> = dyn FnMut(&mut crate::home::HomeState, Vec<egui::Event>) -> Vec<(String, Rect)> + 'a;

    #[test]
    fn the_top_bars_question_mark_switches_quick_help_which_is_kept_and_starts_off() {
        use crate::home::{HomeState, Preview};
        let show = sp_engine::Show::new(vec![std::sync::Arc::new(sp_model::parse_song_text("Song", "Words\n"))]);
        let ctx = egui::Context::default();
        crate::studio::apply_style(&ctx);
        ctx.all_styles_mut(|style| style.interaction.tooltip_delay = 0.0);
        let mut time = 0.0;
        let mut frame = |state: &mut HomeState, events: Vec<egui::Event>| {
            time += 0.5;
            let raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1440.0, 900.0))),
                time: Some(time),
                events,
                ..egui::RawInput::default()
            };
            let output = ctx.run_ui(raw, |ui| {
                crate::home::home_ui(
                    ui,
                    &show,
                    state,
                    &sp_model::Theme::builtin(),
                    &mut |_| None,
                    Preview::default(),
                    None,
                );
            });
            let found = texts(&output);
            output.drop_without_applying_deltas();
            found
        };
        let mut state = HomeState::default();
        assert!(!state.quick_help, "off on a new install");
        frame(&mut state, vec![]);
        let drawn = frame(&mut state, vec![]);
        let key = drawn.iter().find(|(text, at)| text == "?" && at.max.y < 60.0).map(|(_, at)| at.center());
        let key = key.expect("the top bar's ?");
        let press = |pressed: bool| egui::Event::PointerButton {
            pos: key,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(&mut state, vec![egui::Event::PointerMoved(key), press(true)]);
        frame(&mut state, vec![press(false)]);
        assert!(state.quick_help, "a click switches it on");
        assert!(!state.key_sheet, "and opens no list of keys");
        // Kept with the preferences; a file from before Quick Help reads as off.
        let prefs = state.prefs();
        assert!(prefs.quick_help);
        let mut fresh = HomeState::default();
        fresh.apply_prefs(&prefs);
        assert!(fresh.quick_help);
        let mut value = serde_json::to_value(crate::service::Prefs::default()).expect("written");
        value.as_object_mut().expect("an object").remove("quick_help");
        let before: crate::service::Prefs = serde_json::from_value(value).expect("read");
        assert!(!before.quick_help);
        // On, the gear says what it does under the pointer; off, it doesn't.
        let gear = key + vec2(39.0, 0.0);
        let hovered = |state: &mut HomeState, frame: &mut Frame| {
            frame(state, vec![egui::Event::PointerMoved(gear)]);
            frame(state, vec![]);
            frame(state, vec![])
                .iter()
                .any(|(text, _)| text.starts_with(crate::i18n::t("topbar.settings", "Settings")) && text.len() > 9)
        };
        assert!(hovered(&mut state, &mut frame), "on: the gear's hint shows");
        state.quick_help = false;
        frame(&mut state, vec![egui::Event::PointerMoved(pos2(700.0, 500.0))]);
        assert!(!hovered(&mut state, &mut frame), "off: nothing shows");
    }

    #[test]
    fn settings_says_what_its_rows_do_only_with_quick_help_on() {
        use crate::home::HomeState;
        let show = sp_engine::Show::new(vec![std::sync::Arc::new(sp_model::parse_song_text("Song", "Words\n"))]);
        let hint = |on: bool| {
            let ctx = egui::Context::default();
            crate::studio::apply_style(&ctx);
            ctx.all_styles_mut(|style| style.interaction.tooltip_delay = 0.0);
            let mut state = HomeState { quick_help: on, ..HomeState::default() };
            let mut said = false;
            let mut row = None;
            for n in 0..6 {
                let events = row.map(|at| vec![egui::Event::PointerMoved(at)]).unwrap_or_default();
                let raw = egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1100.0, 800.0))),
                    time: Some(f64::from(n) * 0.5),
                    events,
                    ..egui::RawInput::default()
                };
                let output = ctx.run_ui(raw, |ui| crate::home::settings_ui(ui, &mut state, &show));
                let drawn = texts(&output);
                output.drop_without_applying_deltas();
                let groups = crate::i18n::t("settings.pane.groups", "Groups");
                row = row.or_else(|| drawn.iter().find(|(text, _)| text == groups).map(|(_, at)| at.center()));
                said |= drawn.iter().any(|(text, _)| text == "Open this page of Settings.");
            }
            said
        };
        assert!(hint(true), "on, a row of the list says what it does");
        assert!(!hint(false), "off, it doesn't");
    }

    #[test]
    fn a_greyed_undo_in_the_inspector_says_why_with_quick_help_off() {
        use crate::home::{HomeState, Preview};
        let show = sp_engine::Show::new(vec![std::sync::Arc::new(sp_model::parse_song_text("Song", "Words\n"))]);
        let ctx = egui::Context::default();
        crate::studio::apply_style(&ctx);
        ctx.all_styles_mut(|style| style.interaction.tooltip_delay = 0.0);
        let mut editing = crate::editor::EditDraft::of(&show, sp_engine::SlideRef { presentation: 0, slide: 0 });
        if let Some(draft) = editing.as_mut() {
            draft.select_only(draft.slide.objects.first().map(|object| object.id));
        }
        let mut state = HomeState { editing, ..HomeState::default() };
        state.inspector.tab = crate::inspector::InspectorTab::Box;
        assert!(!state.quick_help);
        let mut time = 0.0;
        let mut frame = |state: &mut HomeState, events: Vec<egui::Event>| {
            time += 0.5;
            let raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1440.0, 900.0))),
                time: Some(time),
                events,
                ..egui::RawInput::default()
            };
            let output = ctx.run_ui(raw, |ui| {
                crate::home::home_ui(
                    ui,
                    &show,
                    state,
                    &sp_model::Theme::builtin(),
                    &mut |_| None,
                    Preview::default(),
                    None,
                );
            });
            let found = texts(&output);
            output.drop_without_applying_deltas();
            found
        };
        frame(&mut state, vec![]);
        let undo = crate::i18n::t("inspector.undo", "Undo");
        let drawn = frame(&mut state, vec![]);
        let at = drawn.iter().filter(|(text, at)| text == undo && at.min.x > 1000.0).map(|(_, at)| at.center()).next();
        let at = at.expect("the inspector's Undo");
        frame(&mut state, vec![egui::Event::PointerMoved(at)]);
        frame(&mut state, vec![]);
        let said = frame(&mut state, vec![]);
        let why = crate::home::nothing_to(false);
        assert!(said.iter().any(|(text, _)| text == why), "{why:?} under the pointer: {said:?}");
    }

    /// The hovers egui shows by itself, which skip Quick Help: none is called outside this file.
    #[test]
    fn no_hover_skips_quick_help() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let banned = [
            ".on_hover_text(",
            ".on_hover_text_at_pointer(",
            ".on_hover_ui(",
            ".on_hover_ui_at_pointer(",
            ".on_disabled_hover_text(",
            ".on_disabled_hover_ui(",
            "show_tooltip",
            "Tooltip::",
        ];
        let mut found = Vec::new();
        fn visit(dir: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
            for entry in std::fs::read_dir(dir).expect("the source").flatten() {
                let path = entry.path();
                if path.is_dir() {
                    visit(&path, files);
                } else if path.extension().is_some_and(|ext| ext == "rs") {
                    files.push(path);
                }
            }
        }
        let mut files = Vec::new();
        visit(&src, &mut files);
        assert!(files.len() > 50, "the scan found only {} files", files.len());
        for path in files.iter().filter(|path| !path.ends_with("quick_help.rs")) {
            let text = std::fs::read_to_string(path).expect("readable");
            for (number, line) in text.lines().enumerate() {
                if banned.iter().any(|call| line.contains(call)) {
                    found.push(format!("{}:{}: {}", path.display(), number + 1, line.trim()));
                }
            }
        }
        assert!(found.is_empty(), "hovers that skip Quick Help (use quick_help::Hint):\n{}", found.join("\n"));
    }
}
