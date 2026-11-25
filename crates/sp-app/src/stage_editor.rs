//! The stage layouts in the Edit view, under Stage. A layout is edited with the slide editor,
//! as a slide of objects (see `stage.rs`): this keeps what is the stage's own, the list of
//! layouts down the left, the Add menu over the canvas, and the inspector's parts for a layout
//! and for what a stage object shows.

use egui::{FontId, vec2};
use sp_model::{Object, ObjectKind, Role, SlideRect, TextStyle};

use crate::editor::EditDraft;
use crate::home::HomeState;
use crate::i18n::{t, tf};
use crate::live_tabs::StagePictures;
use crate::stage::{ITEMS, STAGE_SLOT};
use crate::studio::{ListKey, ListPicture, TEXT, TEXT_DIM, TEXT_FAINT, Thumb};

/// What the list of layouts asked for. It is carried out once the frame's change to the open
/// layout has gone back into it, so a layout added or deleted in front of it never takes it.
#[derive(Clone, Debug, PartialEq)]
pub enum ListRequest {
    Open(usize),
    New,
    Duplicate(usize),
    Delete(usize),
    /// The layout one place up the list (true) or down; the one open stays open.
    Move(usize, bool),
    /// The layout at a place dragged to another; the one open stays open.
    MoveTo(usize, usize),
}

/// The editor's draft of a layout, by its place in the list.
pub fn draft(state: &HomeState, index: usize) -> Option<EditDraft> {
    let layout = state.stage_layouts.get(index)?;
    let mut draft = EditDraft::of_slide(crate::stage::as_slide(layout), index);
    draft.at.presentation = STAGE_SLOT;
    // Several objects may show one item (two clocks, the words twice): a copy shows it too.
    draft.keeps_roles = true;
    Some(draft)
}

/// True when the draft is of a stage layout.
pub fn is_stage(draft: &EditDraft) -> bool {
    draft.at.presentation == STAGE_SLOT
}

/// While Edit › Stage is open, makes sure a layout's draft is in the editor: the one chosen
/// (`stage_edit.0`), with the object `stage_edit.1` (its place in the Objects list, front
/// first) picked when it is first opened.
pub fn prepare(state: &mut HomeState) {
    if state.stage_layouts.is_empty() {
        state.stage_layouts = crate::stage::builtin_layouts();
    }
    let last = state.stage_layouts.len() - 1;
    state.stage_edit.0 = state.stage_edit.0.min(last);
    if let Some(open) = state.editing.as_ref().filter(|draft| is_stage(draft)) {
        // A layout that has gone (deleted from the command line, say): the one before it opens.
        if open.at.slide > last {
            state.stage_edit.0 = last;
            state.editing = draft(state, last);
        }
        return;
    }
    if !state.stage_editor_open || state.editing.is_some() || state.theme_edit.is_some() {
        return;
    }
    state.editing = draft(state, state.stage_edit.0);
    if let (Some(at), Some(draft)) = (state.stage_edit.1.take(), state.editing.as_mut()) {
        let picked = draft.slide.objects.iter().rev().nth(at).map(|object| object.id);
        draft.select_only(picked);
    }
}

/// Carries out what the list of layouts asked for.
pub fn apply(state: &mut HomeState, request: ListRequest) {
    let open = state.editing.as_ref().filter(|draft| is_stage(draft)).map_or(0, |draft| draft.at.slide);
    let to = match request {
        ListRequest::Open(index) => index,
        ListRequest::New => {
            let taken = |name: &str| state.stage_layouts.iter().any(|layout| layout.name == name);
            let name = (1..)
                .map(|n: u32| tf("editor.layout.new", "Layout {n}", &[("n", &n.to_string())]))
                .find(|name| !taken(name))
                .unwrap_or_default();
            state.stage_layouts.push(crate::stage::new_layout(name));
            state.stage_layouts_changed = true;
            state.stage_layouts.len() - 1
        }
        ListRequest::Duplicate(index) => {
            let Some(original) = state.stage_layouts.get(index) else { return };
            let copy =
                crate::stage::copy_of(original, tf("editor.layout.copy", "{name} copy", &[("name", &original.name)]));
            state.stage_layouts.insert(index + 1, copy);
            state.stage_layouts_changed = true;
            index + 1
        }
        // At once, asking nothing: Undo puts it back.
        ListRequest::Delete(index) => {
            if let Some(layout) = take_out(state, index) {
                state.removed.push(crate::removed::Removed::StageLayout { at: index, layout });
                state.note_deleted_in_editor();
            }
            return;
        }
        ListRequest::MoveTo(from, to) => {
            if from >= state.stage_layouts.len() || to >= state.stage_layouts.len() {
                return;
            }
            let layout = state.stage_layouts.remove(from);
            state.stage_layouts.insert(to, layout);
            state.stage_layouts_changed = true;
            state.stage_rename = None;
            let now = crate::home::place_after_move(open, from, to);
            state.stage_edit = (now, None);
            if let Some(draft) = state.editing.as_mut().filter(|draft| is_stage(draft)) {
                draft.at.slide = now;
            }
            return;
        }
        ListRequest::Move(index, up) => {
            let Some(to) = (if up { index.checked_sub(1) } else { Some(index + 1) }) else { return };
            if to >= state.stage_layouts.len() {
                return;
            }
            state.stage_layouts.swap(index, to);
            state.stage_layouts_changed = true;
            // The open layout keeps its draft, and the changes it can undo, wherever it went.
            let now = if open == index {
                to
            } else if open == to {
                index
            } else {
                open
            };
            state.stage_edit = (now, None);
            if let Some(draft) = state.editing.as_mut().filter(|draft| is_stage(draft)) {
                draft.at.slide = now;
            }
            return;
        }
    };
    state.stage_edit = (to, None);
    state.editing = draft(state, to);
}

/// Takes a layout out of the list (never the last one; stage screens that use it show the first
/// one), keeping Edit › Stage on a layout when it is open: the one open, wherever it went, or
/// the one now in its place. Returns it.
pub fn take_out(state: &mut HomeState, index: usize) -> Option<sp_model::ThemeTemplate> {
    if state.stage_layouts.len() <= 1 || index >= state.stage_layouts.len() {
        return None;
    }
    let open = open_layout(state);
    let layout = state.stage_layouts.remove(index);
    state.stage_layouts_changed = true;
    if let Some(open) = open {
        let to = if open > index { open - 1 } else { open.min(state.stage_layouts.len() - 1) };
        state.stage_edit = (to, None);
        state.editing = draft(state, to);
    }
    Some(layout)
}

/// The layout open in the editor, by its place in the list.
fn open_layout(state: &HomeState) -> Option<usize> {
    state.editing.as_ref().filter(|draft| is_stage(draft)).map(|draft| draft.at.slide)
}

/// The top of the stage layouts' part of the Edit view's left column: the heading, and the keys
/// that act on the layout open (Add, Duplicate, Delete, at once, and ↑ and ↓ to move it).
/// `delete` is the Delete key pressed with nothing picked on the canvas: the Delete key here.
/// Returns what was asked for.
pub fn list_head(ui: &mut egui::Ui, state: &mut HomeState, delete: bool) -> Option<ListRequest> {
    let open = open_layout(state);
    let count = state.stage_layouts.len();
    let mut asked = None;
    egui::Frame::new().inner_margin(egui::Margin::symmetric(10, 0)).show(ui, |ui| {
        crate::studio::list_heading(ui, t("editor.layouts", "Stage layouts"));
        let can = [
            true,
            open.is_some(),
            open.is_some() && count > 1,
            open.is_some_and(|at| at > 0),
            open.is_some_and(|at| at + 1 < count),
        ];
        let hints = [
            t("editor.layouts.new.hint", "A new stage layout"),
            t("editor.layouts.copy.hint", "A copy of this layout"),
            t("editor.layouts.delete.hint", "Delete this layout; stage screens that use it show the first one"),
            t("editor.layouts.up.hint", "Move this layout up the list"),
            t("editor.layouts.down.hint", "Move this layout down the list"),
        ];
        let none_open = t("editor.layouts.none_open", "No stage layout is open");
        let why = match open {
            Some(_) => ["", none_open, crate::home::ONE_LAYOUT, crate::studio::AT_TOP, crate::studio::AT_BOTTOM],
            None => ["", none_open, none_open, none_open, none_open],
        };
        let pressed = crate::studio::list_keys(ui, "stage layout keys", can, hints, why)
            .or_else(|| crate::home::list_delete_key(delete, can, why, &mut state.notices));
        let Some(at) = open else {
            asked = (pressed == Some(ListKey::Add)).then_some(ListRequest::New);
            return;
        };
        asked = match pressed {
            Some(ListKey::Add) => Some(ListRequest::New),
            Some(ListKey::Copy) => Some(ListRequest::Duplicate(at)),
            Some(ListKey::Up) => Some(ListRequest::Move(at, true)),
            Some(ListKey::Down) => Some(ListRequest::Move(at, false)),
            Some(ListKey::Delete) => Some(ListRequest::Delete(at)),
            None => None,
        };
    });
    asked
}

/// The stage layouts, under their heading and keys: each a picture of what the stage would show
/// in it now, with its number and name under it. A click opens one, a double click retypes its
/// name in place, a drag moves it to where the ink line is, and a right-click offers Rename,
/// Duplicate and Delete. Returns what was asked for.
pub fn list(
    ui: &mut egui::Ui,
    state: &mut HomeState,
    pictures: &StagePictures,
    thumb: &mut Thumb,
) -> Option<ListRequest> {
    let open = open_layout(state);
    let mut asked = None;
    let list = egui::Id::unique("stage layout pictures");
    egui::ScrollArea::vertical().id_salt("stage layouts").auto_shrink([false, false]).show(ui, |ui| {
        let side = egui::Margin { left: 20, right: 20, top: crate::studio::LIST_TOP as i8, bottom: 12 };
        egui::Frame::new().inner_margin(side).show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, crate::studio::LIST_BETWEEN);
            let only_one = state.stage_layouts.len() <= 1;
            let mut renamed = None;
            let (mut rows, mut dragging, mut dropped) = (Vec::with_capacity(state.stage_layouts.len()), None, None);
            for (index, layout) in state.stage_layouts.iter().enumerate() {
                let renaming = state.stage_rename.as_ref().is_some_and(|(at, _)| *at == index);
                let look = ListPicture {
                    number: index + 1,
                    name: &layout.name,
                    dot: None,
                    open: open == Some(index),
                    live: false,
                    renaming,
                };
                let (response, places) = crate::studio::list_picture(ui, list, index, &look);
                rows.push(response.rect);
                if response.dragged() {
                    dragging = Some(index);
                }
                if crate::studio::drag_let_go(&response) {
                    dropped = Some(index);
                }
                // Only the pictures in sight are made, drawn or asked for.
                if ui.is_rect_visible(places.picture) {
                    pictures.paint(ui, thumb, places.picture, crate::studio::LIST_PICTURE_CORNER, layout);
                }
                if renaming && let Some((_, typed)) = state.stage_rename.as_mut() {
                    let look = crate::studio::list_name_look(TEXT);
                    let field = crate::studio::rename_field_at(ui, typed, &look, places.name, places.name_right);
                    if std::mem::take(&mut state.stage_rename_started) {
                        field.request_focus();
                    }
                    let (enter, escape) =
                        ui.input(|input| (input.key_pressed(egui::Key::Enter), input.key_pressed(egui::Key::Escape)));
                    if escape {
                        renamed = Some(None);
                    } else if enter || field.lost_focus() {
                        renamed = Some(Some((index, typed.clone())));
                    }
                }
                let mut chose = None;
                response.context_menu(|ui| chose = layout_menu(ui, only_one).or(chose));
                if response.double_clicked() || chose == Some(Choice::Rename) {
                    state.stage_rename = Some((index, layout.name.clone()));
                    state.stage_rename_started = true;
                } else if chose == Some(Choice::Duplicate) {
                    asked = Some(ListRequest::Duplicate(index));
                } else if chose == Some(Choice::Delete) {
                    asked = Some(ListRequest::Delete(index));
                } else if response.clicked() && open != Some(index) {
                    asked = Some(ListRequest::Open(index));
                }
            }
            let moving: Vec<usize> = dragging.or(dropped).into_iter().collect();
            let words = dragging
                .and_then(|at| state.stage_layouts.get(at))
                .map(|layout| layout.name.clone())
                .unwrap_or_default();
            if let (Some(from), Some(y)) =
                (dropped, crate::home::list_drag(ui, &rows, (dragging, dropped), &moving, &words))
                && let Some(to) = crate::home::dragged_to(&rows, from, y)
            {
                asked = Some(ListRequest::MoveTo(from, to));
            }
            match renamed {
                Some(Some((index, name))) => {
                    let name = name.trim();
                    if let Some(layout) = state.stage_layouts.get_mut(index)
                        && !name.is_empty()
                        && layout.name != name
                    {
                        layout.name = name.to_owned();
                        state.stage_layouts_changed = true;
                    }
                    state.stage_rename = None;
                }
                Some(None) => state.stage_rename = None,
                None => {}
            }
        });
    });
    asked
}

/// What a layout's menu (a right-click on its picture) was asked for.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Choice {
    Rename,
    Duplicate,
    Delete,
}

/// A layout's menu: Rename, Duplicate, Delete (at once, as Undo puts it back; not for the only
/// layout).
fn layout_menu(ui: &mut egui::Ui, only_one: bool) -> Option<Choice> {
    let mut chose = None;
    if ui.button(t("editor.layout.rename", "Rename")).clicked() {
        chose = Some(Choice::Rename);
    }
    if ui.button(t("editor.layout.duplicate", "Duplicate")).clicked() {
        chose = Some(Choice::Duplicate);
    }
    let delete = ui
        .add_enabled(!only_one, egui::Button::new(t("editor.layout.delete", "Delete")))
        .on_hover_text(t(
            "editor.layouts.delete.hint",
            "Delete this layout; stage screens that use it show the first one",
        ))
        .on_disabled_hover_text(crate::home::ONE_LAYOUT);
    if delete.clicked() {
        chose = Some(Choice::Delete);
    }
    if chose.is_some() {
        ui.close();
    }
    chose
}

/// The audience screens a screen picture can show, by id and name, the first (the main one)
/// first. A stage screen is never one of them: no screen shows itself.
pub fn audience_screens(state: &HomeState) -> Vec<(sp_model::Uuid, String)> {
    state
        .screens
        .iter()
        .filter(|screen| screen.kind == crate::screens::Kind::Audience)
        .map(|screen| (screen.id, screen.name.clone()))
        .collect()
}

/// What the main audience screen is called in a screen picture's choice.
pub fn main_screen() -> &'static str {
    t("editor.screen.main", "Audience screens (main)")
}

/// The name a screen picture is given in the Objects list for what it shows.
pub fn screen_object_name(screen: Option<sp_model::Uuid>, screens: &[(sp_model::Uuid, String)]) -> String {
    match screen.and_then(|id| screens.iter().find(|(have, _)| *have == id)) {
        Some((_, name)) => format!("Screen: {name}"),
        None => "Screen: main audience".to_owned(),
    }
}

/// A new object showing one of the stage's items, in the middle of the stage, with the plain
/// stage look at 72 points.
pub fn item_object(role: &str) -> Object {
    let rect = SlideRect { x: 560.0, y: 440.0, w: 800.0, h: 200.0 };
    let role = Role(role.to_owned());
    let mut object = Object::text(role.clone(), crate::stage::item_sample(&role));
    object.name = crate::stage::item_name(&role).to_owned();
    object.frame = Some(rect);
    if let ObjectKind::Text { style, .. } = &mut object.kind {
        *style = TextStyle { size: Some(72.0), ..TextStyle::default() };
    }
    object
}

/// The canvas's Add menu on a stage layout: words of its own, a shape, a picture or video, a
/// web page; then each item the stage can show live, and a picture of an audience screen.
/// Returns true when the menu asked for a picture or video from a file.
pub fn add_menu(ui: &mut egui::Ui, draft: &mut EditDraft, screens: &[(sp_model::Uuid, String)]) -> bool {
    let mut wants_picture = false;
    if ui.add(egui::Button::new(t("editor.add.text", "Text")).shortcut_text("T")).clicked() {
        draft.add_text();
        ui.close();
    }
    ui.menu_button(t("editor.add.shape", "Shape"), |ui| {
        for (shape, key) in sp_model::Shape::ALL.into_iter().zip(crate::home::SHAPE_KEYS) {
            let name = crate::editor::shape_name(shape);
            if ui.add(egui::Button::new(name).shortcut_text(key.to_string())).clicked() {
                draft.add_shape(shape);
                ui.close();
            }
        }
    });
    if ui.button(t("editor.add.picture", "Picture or Video…")).clicked() {
        wants_picture = true;
        ui.close();
    }
    if ui.button(t("editor.add.web", "Web Page")).clicked() {
        draft.add_web();
        ui.close();
    }
    if ui.button(t("editor.add.ndi", "NDI Source")).clicked() {
        draft.add_ndi();
        ui.close();
    }
    ui.separator();
    ui.label(
        egui::RichText::new(t("editor.add.shows_live", "SHOWS LIVE"))
            .font(crate::studio::fixed_semibold(11.0))
            .color(TEXT_FAINT),
    );
    for (role, name, _) in ITEMS {
        if ui.button(name).clicked() {
            draft.add(item_object(role));
            ui.close();
        }
    }
    ui.menu_button(t("editor.add.screen", "Screen Picture"), |ui| {
        let mut choices = vec![(None, main_screen().to_owned())];
        choices.extend(screens.iter().map(|(id, name)| (Some(*id), name.clone())));
        for (screen, name) in choices {
            let hint = t("editor.add.screen.hint", "What this screen shows, live, in a box on the stage");
            if ui.button(name).on_hover_text(hint).clicked() {
                let mut object = Object::screen(screen);
                object.name = screen_object_name(screen, screens);
                draft.add(object);
                ui.close();
            }
        }
    });
    wants_picture
}

/// The inspector with nothing picked on a layout: its name, which stage screens use it, and
/// what its objects can be.
pub fn layout_panel(ui: &mut egui::Ui, state: &mut HomeState) {
    let Some(index) = state.editing.as_ref().filter(|draft| is_stage(draft)).map(|draft| draft.at.slide) else {
        return;
    };
    let using: Vec<String> = state
        .screens
        .iter()
        .filter(|screen| screen.kind == crate::screens::Kind::Stage)
        .filter(|screen| {
            crate::stage::layout_for(&state.stage_layouts, screen.stage_layout).id == state.stage_layouts[index].id
        })
        .map(|screen| screen.name.clone())
        .collect();
    let Some(layout) = state.stage_layouts.get_mut(index) else { return };
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(t("editor.layout.name", "Name")).size(12.5).strong().color(TEXT));
        // A layout always has a name: emptied, it keeps the one it had, as a theme's does.
        let before = layout.name.clone();
        let field = ui.add(egui::TextEdit::singleline(&mut layout.name).desired_width(f32::INFINITY));
        if field.changed() {
            if layout.name.trim().is_empty() {
                layout.name = before;
            } else {
                state.stage_layouts_changed = true;
            }
        }
    });
    ui.add_space(8.0);
    let about = t(
        "editor.layout.about",
        "Each object shows something live: the words on screen, the next slide, the clock, a timer, the notes, the stage message, or what an audience screen shows. Add more from Add above the stage. Click one to change it.",
    );
    ui.label(egui::RichText::new(about).size(12.0).color(TEXT_FAINT));
    ui.add_space(8.0);
    let screens = if using.is_empty() {
        t(
            "editor.layout.unused",
            "No stage screen uses this layout. Each stage screen picks its layout in the Stage tab.",
        )
        .to_owned()
    } else {
        tf("editor.layout.used_by", "Stage screens using this layout: {screens}.", &[("screens", &using.join(", "))])
    };
    ui.label(egui::RichText::new(screens).size(12.0).color(TEXT_FAINT));
}

/// The settings of the item a stage object shows, at the head of the inspector's Text tab:
/// which timer, the clock's form, how many lines of the notes.
pub fn item_settings(ui: &mut egui::Ui, object: &mut Object, timers: &[(sp_model::Uuid, String)]) {
    let Some(item) = crate::stage::item_of(object) else { return };
    let role = Role(item.to_owned());
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(t("editor.item.shows", "SHOWS"))
                .font(crate::studio::fixed_semibold(11.0))
                .color(TEXT_FAINT),
        );
        ui.label(egui::RichText::new(crate::stage::item_name(&role)).size(13.0).strong().color(TEXT));
    });
    let shows = &mut object.shows;
    match item {
        "timer" => {
            let chosen = shows.timer.and_then(|id| timers.iter().find(|(have, _)| *have == id));
            let shown = match (shows.timer, chosen) {
                (None, _) => t("editor.item.timer.first", "The first one running").to_owned(),
                (Some(_), Some((_, name))) => name.clone(),
                (Some(_), None) => t("editor.item.timer.gone", "(a timer that has gone)").to_owned(),
            };
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(t("editor.item.timer", "Timer")).size(12.5).color(TEXT_DIM));
                egui::ComboBox::from_id_salt("stage timer").selected_text(shown).width(ui.available_width()).show_ui(
                    ui,
                    |ui| {
                        let first = t("editor.item.timer.first", "The first one running");
                        if ui.add(egui::Button::selectable(shows.timer.is_none(), first)).clicked() {
                            shows.timer = None;
                        }
                        for (id, name) in timers {
                            if ui.add(egui::Button::selectable(shows.timer == Some(*id), name.as_str())).clicked() {
                                shows.timer = Some(*id);
                            }
                        }
                    },
                );
            });
            let note = if shows.timer.is_some() {
                t("editor.item.timer.this", "Shows this timer's name and time, running or not.")
            } else {
                t(
                    "editor.item.timer.first.about",
                    "Shows the name and time of the first timer that is running, and nothing while none is.",
                )
            };
            ui.label(egui::RichText::new(note).size(11.5).color(TEXT_FAINT));
        }
        "clock" => {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(t("editor.item.clock", "Clock")).size(12.5).color(TEXT_DIM));
                if ui.add(egui::Button::selectable(!shows.hours_24, t("editor.item.clock.12", "12-hour"))).clicked() {
                    shows.hours_24 = false;
                }
                if ui.add(egui::Button::selectable(shows.hours_24, t("editor.item.clock.24", "24-hour"))).clicked() {
                    shows.hours_24 = true;
                }
                ui.checkbox(&mut shows.seconds, t("editor.item.clock.seconds", "Seconds"));
            });
        }
        "notes" => {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(t("editor.item.notes.lines", "Lines")).size(12.5).color(TEXT_DIM));
                ui.add(egui::DragValue::new(&mut shows.lines).range(0..=40));
                let said = if shows.lines == 0 {
                    t("editor.item.notes.all", "all of them")
                } else {
                    t("editor.item.notes.top", "from the top")
                };
                ui.label(egui::RichText::new(said).size(11.5).color(TEXT_FAINT));
            });
        }
        other => {
            let said = match other {
                "now" => t("editor.item.now", "The words of the slide on screen, every box's."),
                "next" => t("editor.item.next", "The words of the slide that Next would show."),
                "next-label" => t("editor.item.next_label", "NEXT, while there is a next slide."),
                "song-title" => t("editor.item.song_title", "The title of the song on screen."),
                "group" => t("editor.item.group", "The group of the slide on screen."),
                _ => t("editor.item.message", "The stage message, while it is showing; red unless its box has a fill."),
            };
            ui.label(egui::RichText::new(said).size(11.5).color(TEXT_FAINT));
        }
    }
    ui.add_space(4.0);
}

/// A screen picture's settings, at the head of the inspector's Shape tab: which screen it shows
/// (the main audience screen, or one by name, kept by its id), its corners and its label.
pub fn screen_settings(ui: &mut egui::Ui, object: &mut Object, screens: &[crate::screens::Screen]) {
    let listed: Vec<(sp_model::Uuid, String)> = screens
        .iter()
        .filter(|screen| screen.kind == crate::screens::Kind::Audience)
        .map(|screen| (screen.id, screen.name.clone()))
        .collect();
    let auto_name = |screen: Option<sp_model::Uuid>| screen_object_name(screen, &listed);
    let name_before = object.name.clone();
    let ObjectKind::Screen { screen, corner, label } = &mut object.kind else { return };
    let was = *screen;
    let (_, fell_back) = crate::stage::source_screen(*screen, screens);
    let shown = match screen.and_then(|id| listed.iter().find(|(have, _)| *have == id)) {
        Some((_, name)) => name.clone(),
        None => main_screen().to_owned(),
    };
    ui.label(
        egui::RichText::new(t("editor.screen", "SCREEN")).font(crate::studio::fixed_semibold(11.0)).color(TEXT_FAINT),
    );
    egui::ComboBox::from_id_salt("screen picture source").selected_text(shown).width(ui.available_width()).show_ui(
        ui,
        |ui| {
            if ui.add(egui::Button::selectable(screen.is_none(), main_screen())).clicked() {
                *screen = None;
            }
            for (id, name) in &listed {
                if ui.add(egui::Button::selectable(*screen == Some(*id), name.as_str())).clicked() {
                    *screen = Some(*id);
                }
            }
        },
    );
    let main_name = listed.first().map(|(_, name)| name.as_str());
    let said = match (fell_back, main_name) {
        (true, Some(main)) => {
            let gone = screens.iter().all(|have| Some(have.id) != was);
            let said = if gone {
                tf(
                    "editor.screen.removed",
                    "The screen it showed has been removed, so it shows the main audience screen ({main}). Choose another above.",
                    &[("main", main)],
                )
            } else {
                tf(
                    "editor.screen.now_stage",
                    "The screen it showed is now a stage screen, so it shows the main audience screen ({main}). Choose another above.",
                    &[("main", main)],
                )
            };
            egui::RichText::new(said).color(crate::studio::CARE)
        }
        (true, None) | (false, None) => egui::RichText::new(t(
            "editor.screen.none",
            "There is no audience screen to show. Add one in Settings › Screens.",
        ))
        .color(crate::studio::CARE),
        (false, Some(main)) if was.is_none() => egui::RichText::new(tf(
            "editor.screen.first",
            "Shows the first audience screen, now {main}, whichever that is.",
            &[("main", main)],
        ))
        .color(TEXT_FAINT),
        (false, Some(_)) => {
            egui::RichText::new(t("editor.screen.live", "Shows live what that screen shows, in its own theme."))
                .color(TEXT_FAINT)
        }
    };
    ui.label(said.size(11.5));
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(t("editor.screen.corners", "Corners")).size(12.5).color(TEXT_DIM));
        ui.add(egui::DragValue::new(corner).range(0.0..=200.0).speed(0.5).max_decimals(0));
        ui.add_space(12.0);
        ui.label(egui::RichText::new(t("editor.screen.label", "Label")).size(12.5).color(TEXT_DIM));
        let none = t("editor.screen.label.none", "None");
        ui.add(egui::TextEdit::singleline(label).hint_text(none).desired_width(f32::INFINITY));
    });
    // A name that said which screen it showed follows the new choice.
    if *screen != was && name_before == auto_name(was) {
        object.name = auto_name(object_screen(object));
    }
}

fn object_screen(object: &Object) -> Option<sp_model::Uuid> {
    match &object.kind {
        ObjectKind::Screen { screen, .. } => *screen,
        _ => None,
    }
}

/// The two lines at the foot of the stage's canvas: that the keyboard edits here, and how.
pub fn hint(ui: &mut egui::Ui) {
    ui.vertical_centered(|ui| {
        ui.label(
            egui::RichText::new(t(
                "editor.hint.keyboard",
                "Here the keyboard edits the layout. Space and the arrows don’t run the show.",
            ))
                .font(crate::studio::semibold(12.0))
                .color(crate::studio::CARE),
        );
        ui.label(
            egui::RichText::new(t(
                "editor.hint.mouse",
                "Click to pick · Shift-click picks more · drag to move · handles resize · arrows nudge, Shift for one pixel",
            ))
            .font(FontId::proportional(12.0))
            .color(TEXT_FAINT),
        );
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_item_has_a_name_and_stand_in_words() {
        for (role, name, sample) in super::ITEMS {
            let role = sp_model::Role(role.to_owned());
            assert_eq!(crate::stage::item_name(&role), name);
            assert_eq!(crate::stage::item_sample(&role), sample);
            assert!(!sample.is_empty());
        }
    }

    #[test]
    fn a_stage_layout_renamed_in_the_list_keeps_its_name_where_and_as_it_was() {
        use egui::Event;

        use crate::studio::tests::{drawn, key, letters, same_letters, styled};
        let ctx = styled();
        let mut state = crate::home::HomeState { stage_layouts: crate::stage::builtin_layouts(), ..Default::default() };
        let name = state.stage_layouts[0].name.clone();
        let list = |state: &mut crate::home::HomeState, events: Vec<Event>| {
            drawn(&ctx, events, |ui| {
                super::list(ui, state, &super::StagePictures::default(), &mut |_| None);
            })
        };
        // The strip under the picture shows the name in capitals, and so does the field.
        let shown = name.to_uppercase();
        let before = letters(list(&mut state, vec![]), &shown);
        state.stage_rename = Some((0, name.clone()));
        state.stage_rename_started = true;
        list(&mut state, vec![]).drop_without_applying_deltas();
        same_letters(&before, &letters(list(&mut state, vec![]), &shown), "stage layout");
        list(&mut state, vec![Event::Text(" Two".to_owned())]).drop_without_applying_deltas();
        list(&mut state, vec![key(egui::Key::Enter)]).drop_without_applying_deltas();
        assert_eq!(state.stage_layouts[0].name, format!("{name} Two"), "Return keeps what was typed");
        assert!(state.stage_rename.is_none());
    }

    /// The built-in layouts, the one at `open` open in the editor.
    fn opened(open: usize) -> crate::home::HomeState {
        let mut state = crate::home::HomeState { stage_layouts: crate::stage::builtin_layouts(), ..Default::default() };
        state.stage_editor_open = true;
        state.editing = super::draft(&state, open);
        state
    }

    /// One frame of the layouts' part of the left column, as the Edit view draws it: what it
    /// drew, and what it asked for.
    fn column(
        ctx: &egui::Context,
        state: &mut crate::home::HomeState,
        events: Vec<egui::Event>,
    ) -> (egui::FullOutput, Option<super::ListRequest>) {
        let mut asked = None;
        // As wide as the left column, and as tall as the window.
        let column = egui::Rect::from_min_size(egui::pos2(10.0, 0.0), egui::vec2(crate::studio::LEFT_WIDTH, 800.0));
        let output = crate::studio::tests::drawn(ctx, events, |ui| {
            ui.scope_builder(egui::UiBuilder::new().max_rect(column), |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
                let head = super::list_head(ui, state, false);
                let list = super::list(ui, state, &super::StagePictures::default(), &mut |_| None);
                asked = head.or(list);
            });
        });
        (output, asked)
    }

    /// Clicks `words` where the column draws them (with the right button for its menu), and
    /// returns what was asked for on the way, and what the last frame drew.
    fn click(
        ctx: &egui::Context,
        state: &mut crate::home::HomeState,
        words: &str,
        button: egui::PointerButton,
    ) -> (Option<super::ListRequest>, egui::FullOutput) {
        let (output, _) = column(ctx, state, vec![]);
        let at = crate::studio::tests::place_on_top(&output, words).expect(words);
        output.drop_without_applying_deltas();
        let mut asked = None;
        let mut last = None;
        for events in crate::studio::tests::click_frames(at, button) {
            let (output, now) = column(ctx, state, events);
            asked = asked.or(now);
            if let Some(old) = last.replace(output) {
                egui::FullOutput::drop_without_applying_deltas(old);
            }
        }
        (asked, last.expect("drawn"))
    }

    #[test]
    fn a_stage_layout_dragged_in_the_list_moves_there_and_the_one_open_stays_open() {
        let ctx = crate::studio::tests::styled();
        let mut state = opened(1);
        let (output, _) = column(&ctx, &mut state, vec![]);
        let first = crate::studio::tests::place_of(&output, "WORDS AND NEXT").expect("drawn");
        let second = crate::studio::tests::place_of(&output, "WORDS ONLY").expect("drawn");
        output.drop_without_applying_deltas();
        let mut asked = None;
        for events in crate::studio::tests::drag_frames(first, second) {
            let (output, now) = column(&ctx, &mut state, events);
            output.drop_without_applying_deltas();
            asked = asked.or(now);
        }
        assert_eq!(asked, Some(super::ListRequest::MoveTo(0, 1)));
        super::apply(&mut state, super::ListRequest::MoveTo(0, 1));
        assert_eq!(state.stage_layouts[0].name, "Words Only");
        assert_eq!(state.stage_layouts[1].name, "Words and Next");
        assert_eq!(state.editing.as_ref().map(|draft| draft.at.slide), Some(0), "Words Only stays open");
        assert!(state.stage_layouts_changed);
    }

    #[test]
    fn a_stage_layouts_right_click_menu_offers_what_its_more_button_did() {
        use egui::PointerButton::{Primary, Secondary};
        let ctx = crate::studio::tests::styled();
        let mut state = opened(0);
        // Its picture's right-click menu: Rename, Duplicate, and Delete, which goes at once.
        click(&ctx, &mut state, "WORDS ONLY", Secondary).1.drop_without_applying_deltas();
        // A menu is drawn from its second frame, once it knows its size.
        let (menu, _) = column(&ctx, &mut state, vec![]);
        for offered in ["Rename", "Duplicate", "Delete"] {
            assert!(crate::studio::tests::place_of(&menu, offered).is_some(), "{offered} is offered");
        }
        // The menu's Duplicate, not the key over the list.
        let at = crate::studio::tests::last_place_of(&menu, "Duplicate").expect("the menu's");
        menu.drop_without_applying_deltas();
        let mut asked = None;
        for events in crate::studio::tests::click_frames(at, Primary) {
            let (output, now) = column(&ctx, &mut state, events);
            asked = asked.or(now);
            output.drop_without_applying_deltas();
        }
        assert_eq!(asked, Some(super::ListRequest::Duplicate(1)));
        super::apply(&mut state, asked.expect("asked"));
        assert_eq!(state.stage_layouts[2].name, "Words Only copy");
        assert_eq!(state.editing.as_ref().map(|draft| draft.at.slide), Some(2), "the copy opens");
        // Rename puts a field where the name is; Delete deletes that layout.
        click(&ctx, &mut state, "WORDS AND NEXT", Secondary).1.drop_without_applying_deltas();
        click(&ctx, &mut state, "Rename", Primary).1.drop_without_applying_deltas();
        assert_eq!(state.stage_rename, Some((0, "Words and Next".to_owned())));
        state.stage_rename = None;
        click(&ctx, &mut state, "WORDS AND NOTES", Secondary).1.drop_without_applying_deltas();
        let (asked, output) = click(&ctx, &mut state, "Delete", Primary);
        output.drop_without_applying_deltas();
        assert_eq!(asked, Some(super::ListRequest::Delete(3)));
    }

    #[test]
    fn the_keys_over_the_stage_layouts_act_on_the_one_open() {
        use egui::PointerButton::Primary;
        let ctx = crate::studio::tests::styled();
        let mut state = opened(1);
        let open = |state: &crate::home::HomeState| state.editing.as_ref().map(|draft| draft.at.slide);
        // Duplicate: a copy of the open layout, just after it, and it opens.
        let (asked, output) = click(&ctx, &mut state, "Duplicate", Primary);
        output.drop_without_applying_deltas();
        assert_eq!(asked, Some(super::ListRequest::Duplicate(1)));
        super::apply(&mut state, asked.expect("asked"));
        assert_eq!((state.stage_layouts[2].name.as_str(), open(&state)), ("Words Only copy", Some(2)));
        // Up: it goes one place up, and stays open in the same draft.
        let serial = state.editing.as_ref().map(|draft| draft.serial);
        let (asked, output) = click(&ctx, &mut state, "\u{2191}", Primary);
        output.drop_without_applying_deltas();
        assert_eq!(asked, Some(super::ListRequest::Move(2, true)));
        super::apply(&mut state, asked.expect("asked"));
        assert_eq!((state.stage_layouts[1].name.as_str(), open(&state)), ("Words Only copy", Some(1)));
        assert_eq!(state.editing.as_ref().map(|draft| draft.serial), serial, "the same draft");
        assert!(state.stage_layouts_changed);
        // Down takes it back.
        let (asked, output) = click(&ctx, &mut state, "\u{2193}", Primary);
        output.drop_without_applying_deltas();
        super::apply(&mut state, asked.expect("asked"));
        assert_eq!((state.stage_layouts[2].name.as_str(), open(&state)), ("Words Only copy", Some(2)));
        // Delete goes at once, asking nothing: the layout goes and the one after it opens.
        let (asked, output) = click(&ctx, &mut state, "Delete", Primary);
        output.drop_without_applying_deltas();
        let copy = state.stage_layouts[2].clone();
        assert_eq!(asked, Some(super::ListRequest::Delete(2)));
        super::apply(&mut state, asked.expect("asked"));
        assert!(!state.alert_open());
        assert!(state.stage_layouts.iter().all(|layout| layout.id != copy.id));
        assert_eq!((state.stage_layouts.len(), open(&state)), (4, Some(2)));
        assert_eq!(state.stage_layouts[2].name, "Words and Notes", "the one after it opens");
        // Undo puts it back where it was, open again, and Command-Z in the editor asks for it.
        let gone = state.removed.pop().expect("kept for Undo");
        assert_eq!(gone, crate::removed::Removed::StageLayout { at: 2, layout: copy.clone() });
        assert_eq!(state.edit_steps.last(), Some(&crate::home::EditStep::Deleted));
        assert!(gone.redo(&mut state, true));
        assert_eq!((state.stage_layouts[2].id, open(&state)), (copy.id, Some(2)));
        // Redo deletes it again.
        assert!(gone.redo(&mut state, false));
        assert_eq!((state.stage_layouts.len(), open(&state)), (4, Some(2)));
        // The first can't go up, the last can't go down, and the only one can't go.
        let mut state = opened(0);
        state.stage_layouts.truncate(1);
        for key in ["\u{2191}", "\u{2193}", "Delete"] {
            let (asked, output) = click(&ctx, &mut state, key, Primary);
            output.drop_without_applying_deltas();
            assert_eq!(asked, None, "{key}");
        }
    }
}
