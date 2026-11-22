//! The stage layouts in the Edit view, under Stage. A layout is edited with the slide editor,
//! as a slide of objects (see `stage.rs`): this keeps what is the stage's own, the list of
//! layouts down the left, the Add menu over the canvas, and the inspector's parts for a layout
//! and for what a stage object shows.

use egui::{Align2, FontId, Rect, Sense, pos2, vec2};
use sp_model::{Object, ObjectKind, Role, SlideRect, TextStyle};

use crate::editor::EditDraft;
use crate::home::HomeState;
use crate::stage::{ITEMS, STAGE_SLOT};
use crate::studio::{CARD, ROW_HOVER, TEXT, TEXT_DIM, TEXT_FAINT};

/// What the list of layouts asked for. It is carried out once the frame's change to the open
/// layout has gone back into it, so a layout added or deleted in front of it never takes it.
#[derive(Clone, Debug, PartialEq)]
pub enum ListRequest {
    Open(usize),
    New,
    Duplicate(usize),
    Delete(usize),
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
            let name = (1..).map(|n| format!("Layout {n}")).find(|name| !taken(name)).unwrap_or_default();
            state.stage_layouts.push(crate::stage::new_layout(name));
            state.stage_layouts_changed = true;
            state.stage_layouts.len() - 1
        }
        ListRequest::Duplicate(index) => {
            let Some(original) = state.stage_layouts.get(index) else { return };
            let copy = crate::stage::copy_of(original, format!("{} copy", original.name));
            state.stage_layouts.insert(index + 1, copy);
            state.stage_layouts_changed = true;
            index + 1
        }
        ListRequest::Delete(index) => {
            if state.stage_layouts.len() <= 1 || index >= state.stage_layouts.len() {
                return;
            }
            state.stage_layouts.remove(index);
            state.stage_layouts_changed = true;
            if open > index { open - 1 } else { open.min(state.stage_layouts.len() - 1) }
        }
    };
    state.stage_edit = (to, None);
    state.editing = draft(state, to);
}

/// The stage layouts, under the Edit view's switch: a heading with + (a new layout), and a row
/// for each layout (click to edit it; ⋯ or a right-click to rename, duplicate or delete it).
/// Returns what was asked for.
pub fn list(ui: &mut egui::Ui, state: &mut HomeState) -> Option<ListRequest> {
    let open = state.editing.as_ref().filter(|draft| is_stage(draft)).map(|draft| draft.at.slide);
    let mut asked = None;
    let side = egui::Margin { left: 10, right: 10, top: 4, bottom: 8 };
    egui::Frame::new().inner_margin(side).show(ui, |ui| {
        let (heading, _) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::hover());
        crate::studio::cap(ui, heading.left_center() + vec2(8.0, -4.0), Align2::LEFT_CENTER, "Stage layouts", TEXT_DIM);
        let plus = Rect::from_min_size(pos2(heading.max.x - 36.0, heading.min.y), vec2(28.0, 28.0));
        let id = ui.scope_id().with("new stage layout");
        let add = crate::studio::icon_button(ui, plus, id, crate::icons::Icon::Plus, 1.0, false);
        if add.on_hover_text("A new stage layout").clicked() {
            asked = Some(ListRequest::New);
        }
        // The list scrolls when it is long, leaving the Objects shelf its room.
        let room = (ui.available_height() - 220.0).max(80.0);
        egui::ScrollArea::vertical().id_salt("stage layouts").max_height(room).show(ui, |ui| {
            let only_one = state.stage_layouts.len() <= 1;
            let mut renamed = None;
            for (index, layout) in state.stage_layouts.iter().enumerate() {
                let (row, response) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::click());
                let chosen = open == Some(index);
                let near = response.hovered() || ui.rect_contains_pointer(row);
                if chosen {
                    ui.painter().rect_filled(row, crate::studio::RADIUS, CARD);
                } else if near {
                    ui.painter().rect_filled(row, crate::studio::RADIUS, ROW_HOVER);
                }
                let mark = Rect::from_center_size(row.left_center() + vec2(16.0, 0.0), vec2(16.0, 16.0));
                crate::icons::paint(
                    ui.painter(),
                    crate::icons::Icon::Stage,
                    mark,
                    if chosen { TEXT } else { TEXT_DIM },
                );
                let more = Rect::from_min_size(pos2(row.max.x - 28.0, row.center().y - 14.0), vec2(28.0, 28.0));
                let name_area =
                    Rect::from_min_max(pos2(row.min.x + 34.0, row.min.y), pos2(more.min.x - 4.0, row.max.y));
                let renaming = state.stage_rename.as_ref().is_some_and(|(at, _)| *at == index);
                if renaming {
                    if let Some((_, typed)) = state.stage_rename.as_mut() {
                        let field = crate::studio::rename_field_at(
                            ui,
                            typed,
                            crate::studio::semibold(14.0),
                            TEXT,
                            crate::studio::ACCENT,
                            name_area.left_center(),
                            name_area.max.x,
                        );
                        if std::mem::take(&mut state.stage_rename_started) {
                            field.request_focus();
                        }
                        let (enter, escape) = ui
                            .input(|input| (input.key_pressed(egui::Key::Enter), input.key_pressed(egui::Key::Escape)));
                        if escape {
                            renamed = Some(None);
                        } else if enter || field.lost_focus() {
                            renamed = Some(Some((index, typed.clone())));
                        }
                    }
                } else {
                    ui.painter().with_clip_rect(name_area).text(
                        name_area.left_center(),
                        Align2::LEFT_CENTER,
                        layout.name.as_str(),
                        crate::studio::semibold(14.0),
                        if chosen { TEXT } else { TEXT_DIM },
                    );
                }
                let mut chose = None;
                let dots = crate::studio::more_button(ui, more, ui.scope_id().with(("stage layout more", index)), near);
                egui::Popup::menu(&dots).show(|ui| chose = layout_menu(ui, only_one).or(chose));
                response.context_menu(|ui| chose = layout_menu(ui, only_one).or(chose));
                if response.double_clicked() || chose == Some(Choice::Rename) {
                    state.stage_rename = Some((index, layout.name.clone()));
                    state.stage_rename_started = true;
                } else if chose == Some(Choice::Duplicate) {
                    asked = Some(ListRequest::Duplicate(index));
                } else if chose == Some(Choice::Delete) {
                    asked = Some(ListRequest::Delete(index));
                } else if response.clicked() && !chosen {
                    asked = Some(ListRequest::Open(index));
                }
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

/// What a layout's menu (its ⋯, or a right-click on its row) was asked for.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Choice {
    Rename,
    Duplicate,
    Delete,
}

/// A layout's menu: Rename, Duplicate, Delete (not for the only layout).
fn layout_menu(ui: &mut egui::Ui, only_one: bool) -> Option<Choice> {
    let mut chose = None;
    if ui.button("Rename").clicked() {
        chose = Some(Choice::Rename);
    }
    if ui.button("Duplicate").clicked() {
        chose = Some(Choice::Duplicate);
    }
    let delete = ui
        .add_enabled(!only_one, egui::Button::new("Delete"))
        .on_hover_text("Delete this layout; stage screens that use it show the first one");
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
pub const MAIN_SCREEN: &str = "Audience screens (main)";

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
    if ui.add(egui::Button::new("Text").shortcut_text("T")).clicked() {
        draft.add_text();
        ui.close();
    }
    ui.menu_button("Shape", |ui| {
        for (shape, key) in sp_model::Shape::ALL.into_iter().zip(crate::home::SHAPE_KEYS) {
            if ui.add(egui::Button::new(shape.name()).shortcut_text(key.to_string())).clicked() {
                draft.add_shape(shape);
                ui.close();
            }
        }
    });
    if ui.button("Picture or Video…").clicked() {
        wants_picture = true;
        ui.close();
    }
    if ui.button("Web Page").clicked() {
        draft.add_web();
        ui.close();
    }
    ui.separator();
    ui.label(egui::RichText::new("SHOWS LIVE").font(crate::studio::fixed_semibold(11.0)).color(TEXT_FAINT));
    for (role, name, _) in ITEMS {
        if ui.button(name).clicked() {
            draft.add(item_object(role));
            ui.close();
        }
    }
    ui.menu_button("Screen Picture", |ui| {
        let mut choices = vec![(None, MAIN_SCREEN.to_owned())];
        choices.extend(screens.iter().map(|(id, name)| (Some(*id), name.clone())));
        for (screen, name) in choices {
            if ui.button(name).on_hover_text("What this screen shows, live, in a box on the stage").clicked() {
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
        ui.label(egui::RichText::new("Name").size(12.5).strong().color(TEXT));
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
    let about = "Each object shows something live: the words on screen, the next slide, the clock, a timer, \
                 the notes, the stage message, or what an audience screen shows. Add more from Add above the \
                 stage. Click one to change it.";
    ui.label(egui::RichText::new(about).size(12.0).color(TEXT_FAINT));
    ui.add_space(8.0);
    let screens = if using.is_empty() {
        "No stage screen uses this layout. Each stage screen picks its layout in the Stage tab.".to_owned()
    } else {
        format!("Stage screens using this layout: {}.", using.join(", "))
    };
    ui.label(egui::RichText::new(screens).size(12.0).color(TEXT_FAINT));
}

/// The settings of the item a stage object shows, at the head of the inspector's Text tab:
/// which timer, the clock's form, how many lines of the notes.
pub fn item_settings(ui: &mut egui::Ui, object: &mut Object, timers: &[(sp_model::Uuid, String)]) {
    let Some(item) = crate::stage::item_of(object) else { return };
    let role = Role(item.to_owned());
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("SHOWS").font(crate::studio::fixed_semibold(11.0)).color(TEXT_FAINT));
        ui.label(egui::RichText::new(crate::stage::item_name(&role)).size(13.0).strong().color(TEXT));
    });
    let shows = &mut object.shows;
    match item {
        "timer" => {
            let chosen = shows.timer.and_then(|id| timers.iter().find(|(have, _)| *have == id));
            let shown = match (shows.timer, chosen) {
                (None, _) => "The first one running".to_owned(),
                (Some(_), Some((_, name))) => name.clone(),
                (Some(_), None) => "(a timer that has gone)".to_owned(),
            };
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Timer").size(12.5).color(TEXT_DIM));
                egui::ComboBox::from_id_salt("stage timer").selected_text(shown).width(ui.available_width()).show_ui(
                    ui,
                    |ui| {
                        if ui.add(egui::Button::selectable(shows.timer.is_none(), "The first one running")).clicked() {
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
                "Shows this timer's name and time, running or not."
            } else {
                "Shows the name and time of the first timer that is running, and nothing while none is."
            };
            ui.label(egui::RichText::new(note).size(11.5).color(TEXT_FAINT));
        }
        "clock" => {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Clock").size(12.5).color(TEXT_DIM));
                if ui.add(egui::Button::selectable(!shows.hours_24, "12-hour")).clicked() {
                    shows.hours_24 = false;
                }
                if ui.add(egui::Button::selectable(shows.hours_24, "24-hour")).clicked() {
                    shows.hours_24 = true;
                }
                ui.checkbox(&mut shows.seconds, "Seconds");
            });
        }
        "notes" => {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Lines").size(12.5).color(TEXT_DIM));
                ui.add(egui::DragValue::new(&mut shows.lines).range(0..=40));
                let said = if shows.lines == 0 { "all of them" } else { "from the top" };
                ui.label(egui::RichText::new(said).size(11.5).color(TEXT_FAINT));
            });
        }
        other => {
            let said = match other {
                "now" => "The words of the slide on screen, every box's.",
                "next" => "The words of the slide that Next would show.",
                "next-label" => "NEXT, while there is a next slide.",
                "song-title" => "The title of the song on screen.",
                "group" => "The group of the slide on screen.",
                _ => "The stage message, while it is showing; red unless its box has a fill.",
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
        None => MAIN_SCREEN.to_owned(),
    };
    ui.label(egui::RichText::new("SCREEN").font(crate::studio::fixed_semibold(11.0)).color(TEXT_FAINT));
    egui::ComboBox::from_id_salt("screen picture source").selected_text(shown).width(ui.available_width()).show_ui(
        ui,
        |ui| {
            if ui.add(egui::Button::selectable(screen.is_none(), MAIN_SCREEN)).clicked() {
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
            let why = if gone {
                "The screen it showed has been removed"
            } else {
                "The screen it showed is now a stage screen"
            };
            egui::RichText::new(format!("{why}, so it shows the main audience screen ({main}). Choose another above."))
                .color(crate::studio::CARE)
        }
        (true, None) | (false, None) => {
            egui::RichText::new("There is no audience screen to show. Add one in Settings › Screens.")
                .color(crate::studio::CARE)
        }
        (false, Some(main)) if was.is_none() => {
            egui::RichText::new(format!("Shows the first audience screen, now {main}, whichever that is."))
                .color(TEXT_FAINT)
        }
        (false, Some(_)) => {
            egui::RichText::new("Shows live what that screen shows, in its own theme.").color(TEXT_FAINT)
        }
    };
    ui.label(said.size(11.5));
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Corners").size(12.5).color(TEXT_DIM));
        ui.add(egui::DragValue::new(corner).range(0.0..=200.0).speed(0.5).max_decimals(0));
        ui.add_space(12.0);
        ui.label(egui::RichText::new("Label").size(12.5).color(TEXT_DIM));
        ui.add(egui::TextEdit::singleline(label).hint_text("None").desired_width(f32::INFINITY));
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
            egui::RichText::new("Here the keyboard edits the layout. Space and the arrows don’t run the show.")
                .font(crate::studio::semibold(12.0))
                .color(crate::studio::CARE),
        );
        ui.label(
            egui::RichText::new(
                "Click to pick · Shift-click picks more · drag to move · handles resize · arrows nudge, Shift for one pixel",
            )
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
}
