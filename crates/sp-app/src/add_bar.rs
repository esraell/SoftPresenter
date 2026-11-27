//! The editor's Add toolbar: a key for each kind of thing the open editor can put on what it
//! edits, in one rounded well that floats over the slide. Fitted, the well is centred over the
//! slide, [`OVER_SLIDE`] points above it, and the two are centred together in the canvas, as
//! much room over the well as under the slide; zoomed in, the well stays at the top left of the
//! canvas while the slide moves under it. One click adds; a kind marked ▾ opens its choice under
//! its key (which shape, which box, what the box shows), and Media asks for a picture or video
//! in a file window.
//!
//! Each key is flat, its picture beside its name ([`Kind::short`]) and as wide as they need.
//! Which [`Form`] the keys take depends only on the room and the language, never on a click:
//! with the caption ADD in a wide window, tighter and without it in a narrow one, and where
//! the names still don't all fit, every key is its picture alone, never some of each.
//!
//! The kinds come from one list ([`kinds`]) for the three editors: a song's slide (and a prop),
//! a theme's template, and a stage layout.

use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2, pos2, vec2};

use crate::editor::EditDraft;
use crate::home::HomeState;
use crate::i18n::t;
use crate::icons::Icon;

/// Which editor the toolbar is over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Editor {
    /// A song's slide, or a prop.
    Slide,
    /// A theme's template.
    Template,
    /// A stage layout.
    Stage,
}

/// A kind of thing the toolbar adds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Text,
    Shape,
    Picture,
    Web,
    Ndi,
    Mirror,
    /// A text box that shows something that changes: the time, a timer, the song's title; on a
    /// stage layout, one of the stage's items (the words on screen, the next slide…).
    Linked,
    /// A template's box for the words of one part of a slide: the main language, the
    /// translation, a reference.
    WordsBox,
    /// A stage layout's picture of what an audience screen shows.
    Screen,
}

impl Kind {
    /// Every kind, in the toolbar's order.
    pub const ALL: [Kind; 9] = [
        Kind::Text,
        Kind::Shape,
        Kind::Picture,
        Kind::Web,
        Kind::Ndi,
        Kind::Mirror,
        Kind::Linked,
        Kind::WordsBox,
        Kind::Screen,
    ];

    /// Its full name, in the language set: what a screen reader says for its key.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Text => t("editor.add.text", "Text"),
            Kind::Shape => t("editor.add.shape", "Shape"),
            Kind::Picture => t("editor.add.picture", "Picture or Video"),
            Kind::Web => t("editor.add.web", "Web Page"),
            Kind::Ndi => t("editor.add.ndi", "NDI Source"),
            Kind::Mirror => t("editor.add.mirror", "AirPlay"),
            Kind::Linked => t("editor.add.linked", "Linked Text"),
            Kind::WordsBox => t("editor.add.box", "A Box for Words"),
            Kind::Screen => t("editor.add.screen", "Screen Picture"),
        }
    }

    /// Its name on its key, in the language set.
    pub fn short(self) -> &'static str {
        match self {
            Kind::Text => t("editor.add.key.text", "Text"),
            Kind::Shape => t("editor.add.key.shape", "Shape"),
            Kind::Picture => t("editor.add.key.picture", "Media"),
            Kind::Web => t("editor.add.key.web", "Web Page"),
            Kind::Ndi => t("editor.add.key.ndi", "NDI Source"),
            Kind::Mirror => t("editor.add.key.mirror", "AirPlay"),
            Kind::Linked => t("editor.add.key.linked", "Linked Text"),
            Kind::WordsBox => t("editor.add.key.box", "Words Box"),
            Kind::Screen => t("editor.add.key.screen", "Screen"),
        }
    }

    /// What its key says under the pointer, in the language set.
    pub fn hint(self) -> &'static str {
        match self {
            Kind::Text => t("editor.add.tip.text", "Add text"),
            Kind::Shape => t("editor.add.tip.shape", "Add a shape: choose which"),
            Kind::Picture => t("editor.add.tip.picture", "Add a picture or video"),
            Kind::Web => t("editor.add.tip.web", "Add a web page"),
            Kind::Ndi => t("editor.add.tip.ndi", "Add an NDI source"),
            Kind::Mirror => t("editor.add.tip.mirror", "Add AirPlay"),
            Kind::Linked => t("editor.add.tip.linked", "Add linked text: choose which"),
            Kind::WordsBox => t("editor.add.tip.box", "Add a box for words: choose which"),
            Kind::Screen => t("editor.add.tip.screen", "Add a screen picture: choose which"),
        }
    }

    /// The picture on its key.
    fn icon(self) -> Icon {
        match self {
            Kind::Text => Icon::Letter,
            Kind::Shape => Icon::Shapes,
            Kind::Picture => Icon::Picture,
            Kind::Web => Icon::Globe,
            Kind::Ndi => Icon::Live,
            Kind::Mirror => Icon::Phone,
            Kind::Linked => Icon::Clock,
            Kind::WordsBox => Icon::WordsBox,
            Kind::Screen => Icon::Display,
        }
    }

    /// True when a click opens a choice under the key first (marked ▾): which shape, what
    /// linked text shows, which box for words, which screen. A picture or video is chosen in a
    /// file window, which is no choice under the key, so Media has no ▾.
    pub fn chooses(self) -> bool {
        !matches!(self, Kind::Text | Kind::Picture | Kind::Web | Kind::Ndi | Kind::Mirror)
    }
}

/// The kinds `editor` can add, in the toolbar's order. A template's text boxes are its boxes
/// for words, which keep a look and a place but nothing linked, so a template has no Linked
/// Text; it has its boxes for words instead, and only a stage layout shows a screen's picture.
pub fn kinds(editor: Editor) -> Vec<Kind> {
    Kind::ALL
        .into_iter()
        .filter(|kind| match kind {
            Kind::Linked => editor != Editor::Template,
            Kind::WordsBox => editor == Editor::Template,
            Kind::Screen => editor == Editor::Stage,
            _ => true,
        })
        .collect()
}

/// How tall the well is.
pub const WELL_TALL: f32 = 34.0;
/// How tall a key is.
pub const KEY_TALL: f32 = 28.0;
/// The room between keys.
pub const KEY_GAP: f32 = 1.0;
/// The room between the foot of the well and the top of the slide.
pub const OVER_SLIDE: f32 = 34.0;
/// A window narrower than this has no caption in the well, and tighter keys.
pub const NARROW_WINDOW: f32 = 1250.0;
/// The room round the keys inside the well.
const WELL_PAD: f32 = 3.0;
/// The room before the caption, and between it and the first key.
const CAPTION_LEFT: f32 = 12.0;
const CAPTION_GAP: f32 = 6.0;
/// How big a key's picture is.
const PICTURE: f32 = 15.0;
/// How far the zoomed well stands in from the canvas's corner, clear of the rulers.
const PINNED_INSET: f32 = 8.0;

/// The caption before the keys, as the app's small capital labels are set.
fn caption_words() -> &'static str {
    t("editor.add.caption", "Add")
}

/// How the keys are drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Form {
    /// The caption ADD, then each key's picture beside its name.
    Wide,
    /// No caption, and less room in and between the keys' parts: a narrow window.
    Tight,
    /// Every key its picture alone, its name said under the pointer: the names don't all fit.
    Pictures,
}

impl Form {
    /// The room a key keeps either side of what is on it.
    fn pad(self) -> f32 {
        if self == Form::Wide { 9.0 } else { 7.0 }
    }

    /// The room between a key's picture and its name.
    fn between(self) -> f32 {
        if self == Form::Wide { 6.0 } else { 5.0 }
    }

    /// The face of a key's name.
    fn font(self) -> FontId {
        FontId::proportional(if self == Form::Wide { 12.5 } else { 12.0 })
    }
}

/// The mark on a key that opens a choice, and its face: small and faint, after the name.
const CHOOSES: &str = "\u{25be}";
fn chooses_font() -> FontId {
    crate::studio::semibold(9.0)
}
/// The room between a key's name, or its picture, and its ▾.
const CHOOSES_GAP: f32 = 3.0;

/// How wide `words` are in `font`.
fn wide(ui: &egui::Ui, words: &str, font: FontId) -> f32 {
    ui.painter().layout_no_wrap(words.to_owned(), font, crate::studio::TEXT).size().x
}

/// How wide `kind`'s key is in `form`: as wide as what is on it. A key with only its picture
/// is a square, with room after it for ▾ where it opens a choice.
fn key_width(ui: &egui::Ui, kind: Kind, form: Form) -> f32 {
    let mark = if kind.chooses() { CHOOSES_GAP + wide(ui, CHOOSES, chooses_font()) } else { 0.0 };
    if form == Form::Pictures {
        return (KEY_TALL + mark).ceil();
    }
    (2.0 * form.pad() + PICTURE + form.between() + wide(ui, kind.short(), form.font()) + mark).ceil()
}

/// The toolbar's sizes for one set of kinds: the form its keys take, how wide its caption is
/// (when it has one) and how wide each key is.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub form: Form,
    pub caption: Option<f32>,
    pub keys: Vec<f32>,
}

impl Plan {
    /// The toolbar in `form`.
    fn in_form(ui: &egui::Ui, kinds: &[Kind], form: Form) -> Plan {
        let caption = (form == Form::Wide).then(|| {
            let look = crate::studio::Lettering::cap(crate::studio::TEXT_FAINT);
            crate::studio::spaced(ui, &caption_words().to_uppercase(), look.font, look.ink, look.spacing).size().x
        });
        Plan { form, caption, keys: kinds.iter().map(|kind| key_width(ui, *kind, form)).collect() }
    }

    /// The toolbar for `kinds` where it has `room` across, in a window `window` wide: with its
    /// caption in a wide window, tighter without it in a narrow one or where that is what
    /// fits, and every key its picture alone where the names don't all fit even so.
    pub fn of(ui: &egui::Ui, kinds: &[Kind], room: f32, window: f32) -> Plan {
        let forms: &[Form] = if window < NARROW_WINDOW { &[Form::Tight] } else { &[Form::Wide, Form::Tight] };
        forms
            .iter()
            .map(|form| Plan::in_form(ui, kinds, *form))
            .find(|plan| plan.size().x <= room + 0.01)
            .unwrap_or_else(|| Plan::in_form(ui, kinds, Form::Pictures))
    }

    /// How big the well is.
    pub fn size(&self) -> Vec2 {
        let keys: f32 = self.keys.iter().sum::<f32>() + KEY_GAP * self.keys.len().saturating_sub(1) as f32;
        let lead = self.caption.map_or(WELL_PAD, |caption| CAPTION_LEFT + caption + CAPTION_GAP);
        vec2(lead + keys + WELL_PAD, WELL_TALL)
    }

    /// The toolbar with its well's top left corner at `at`.
    pub fn lay_out(&self, at: Pos2) -> Bar {
        let well = Rect::from_min_size(at, self.size());
        let caption =
            self.caption.map(|wide| Rect::from_min_size(pos2(at.x + CAPTION_LEFT, at.y), vec2(wide, WELL_TALL)));
        let mut x = caption.map_or(at.x + WELL_PAD, |caption| caption.max.x + CAPTION_GAP);
        let top = at.y + (WELL_TALL - KEY_TALL) / 2.0;
        let keys = self
            .keys
            .iter()
            .map(|wide| {
                let key = Rect::from_min_size(pos2(x, top), vec2(*wide, KEY_TALL));
                x += wide + KEY_GAP;
                key
            })
            .collect();
        Bar { well, caption, keys, form: self.form }
    }
}

/// Where the toolbar's well, caption and keys go.
#[derive(Clone, Debug, PartialEq)]
pub struct Bar {
    pub well: Rect,
    pub caption: Option<Rect>,
    /// Each key, in order, left to right.
    pub keys: Vec<Rect>,
    pub form: Form,
}

/// The toolbar's well, `bar` big, and the slide fitted to the canvas room `room` (the room
/// inside its margins, under any name over it): the well's top left corner, the slide's middle
/// and the slide's width. Well and slide are one block, centred across and down the room: the
/// well centred over the slide, [`OVER_SLIDE`] above it.
pub fn fit(room: Rect, bar: Vec2) -> (Pos2, Pos2, f32) {
    let width = room.width().min((room.height() - bar.y - OVER_SLIDE) * 16.0 / 9.0).max(1.0);
    let tall = bar.y + OVER_SLIDE + width * 9.0 / 16.0;
    let top = room.min.y + ((room.height() - tall) / 2.0).max(0.0);
    let centre = pos2(room.center().x, top + bar.y + OVER_SLIDE + width * 9.0 / 32.0);
    (pos2(room.center().x - bar.x / 2.0, top), centre, width)
}

/// Where the well's top left corner is while the slide is zoomed: just inside the top left
/// corner `corner` of the slide's room (inside the rulers, under any name over the canvas).
/// Where the slide has been moved to makes no difference.
pub fn pinned(corner: Pos2) -> Pos2 {
    corner + Vec2::splat(PINNED_INSET)
}

/// For the tests that measure a drawn frame: where the frame's toolbar put its well and each
/// key, kept under [`well_id`].
#[cfg(test)]
#[derive(Clone, Debug, PartialEq)]
pub struct Drawn {
    pub well: Rect,
    pub keys: Vec<(Kind, Rect)>,
}

/// The id of the well: one toolbar is shown at a time.
pub fn well_id() -> egui::Id {
    egui::Id::unique("add bar well")
}

/// Draws the toolbar `bar` for `editor` and does what its keys ask: adds to the draft being
/// edited, or asks for a picture from a file. The well takes presses, so none between its keys
/// falls through to the slide under it. Keep it inside the Edit view's frame, so what it adds
/// is handed on with the rest.
pub fn show(ui: &mut egui::Ui, state: &mut HomeState, editor: Editor, bar: &Bar) {
    #[cfg(test)]
    ui.data_mut(|data| {
        let keys = kinds(editor).into_iter().zip(bar.keys.iter().copied()).collect();
        data.insert_temp(well_id(), Drawn { well: bar.well, keys });
    });
    ui.interact(bar.well, well_id(), Sense::click_and_drag());
    let painter = ui.painter();
    // It floats: a soft shadow under it, and a thin line round it.
    let shadow = egui::Shadow { offset: [0, 6], blur: 18, spread: 0, color: Color32::from_black_alpha(89) };
    painter.add(shadow.as_shape(bar.well, 10));
    painter.rect_filled(bar.well, 10.0, crate::studio::CARD);
    painter.rect_stroke(bar.well, 10.0, Stroke::new(1.0, crate::studio::LINE), StrokeKind::Inside);
    if let Some(caption) = bar.caption {
        crate::studio::cap(ui, caption.left_center(), Align2::LEFT_CENTER, caption_words(), crate::studio::TEXT_FAINT);
    }
    for (kind, rect) in kinds(editor).into_iter().zip(bar.keys.iter().copied()) {
        let key = flat_key(ui, rect, kind, bar.form);
        // A key that is only its picture says its name; one with its name says what it does.
        let key = key.on_hover_text(if bar.form == Form::Pictures { kind.name() } else { kind.hint() });
        key.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, kind.name()));
        match kind {
            Kind::Shape => {
                egui::Popup::menu(&key).show(|ui| shape_menu(ui, state.editing.as_mut()));
            }
            Kind::Linked if editor == Editor::Stage => {
                egui::Popup::menu(&key).show(|ui| stage_items_menu(ui, state.editing.as_mut()));
            }
            Kind::Linked => {
                let timers: Vec<(sp_model::Uuid, String)> =
                    state.timers.iter().map(|timer| (timer.id, timer.name.clone())).collect();
                egui::Popup::menu(&key).show(|ui| linked_menu(ui, state.editing.as_mut(), &timers));
            }
            Kind::WordsBox => {
                egui::Popup::menu(&key).show(|ui| words_box_menu(ui, state.editing.as_mut()));
            }
            Kind::Screen => {
                let screens = crate::stage_editor::audience_screens(state);
                egui::Popup::menu(&key).show(|ui| screen_menu(ui, state.editing.as_mut(), &screens));
            }
            // Today's picker for a picture or video: the file dialog, which puts what is chosen
            // on the slide (`picture_to_add`).
            Kind::Picture if key.clicked() => state.picture_request = true,
            Kind::Text | Kind::Web | Kind::Ndi | Kind::Mirror if key.clicked() => {
                add(ui, state, kind);
            }
            _ => {}
        }
    }
}

/// One key of the well: no fill until the pointer is on it or its menu is open, then the key's
/// highlight; its picture, its name beside it unless the keys are pictures only, and a small
/// faint ▾ last where a click opens a choice.
fn flat_key(ui: &egui::Ui, rect: Rect, kind: Kind, form: Form) -> egui::Response {
    let response = ui.interact(rect, key_id(kind), Sense::click());
    let open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&response));
    let lit = response.hovered() || response.is_pointer_button_down_on() || open;
    let painter = ui.painter();
    if lit {
        painter.rect_filled(rect, 8.0, crate::studio::BUTTON_HOVER);
    }
    let middle = rect.center().y;
    let pad = if form == Form::Pictures { (KEY_TALL - PICTURE) / 2.0 } else { form.pad() };
    let picture = Rect::from_min_size(pos2(rect.min.x + pad, middle - PICTURE / 2.0), Vec2::splat(PICTURE));
    crate::icons::paint(painter, kind.icon(), picture, crate::studio::TEXT);
    let mut after = picture.max.x;
    if form != Form::Pictures {
        let ink = if lit { crate::studio::TEXT } else { crate::studio::TEXT_DIM };
        let at = pos2(after + form.between(), middle);
        after = painter.text(at, Align2::LEFT_CENTER, kind.short(), form.font(), ink).max.x;
    }
    if kind.chooses() {
        let at = pos2(after + CHOOSES_GAP, middle + 1.0);
        painter.text(at, Align2::LEFT_CENTER, CHOOSES, chooses_font(), crate::studio::TEXT_FAINT);
    }
    response
}

/// The id of `kind`'s key: one toolbar is shown at a time.
pub fn key_id(kind: Kind) -> egui::Id {
    egui::Id::unique(("add bar key", kind as u8))
}

/// Adds a kind that needs no choice, selected, and readies the inspector for what it needs.
fn add(ui: &egui::Ui, state: &mut HomeState, kind: Kind) {
    let Some(draft) = state.editing.as_mut() else { return };
    match kind {
        Kind::Text => draft.add_text(),
        Kind::Web => {
            draft.add_web();
            // The box's own tab in the inspector, where the address field is, and it takes the
            // keyboard, ready for the address.
            state.inspector.tab = crate::inspector::InspectorTab::Own;
            ui.data_mut(|data| data.insert_temp(egui::Id::unique("web address focus"), true));
        }
        Kind::Ndi => {
            draft.add_ndi();
            // The box's own tab in the inspector, where the Source field is, and it takes the keyboard,
            // ready for a choice.
            state.inspector.tab = crate::inspector::InspectorTab::Own;
            ui.data_mut(|data| data.insert_temp(egui::Id::unique("ndi source focus"), true));
        }
        Kind::Mirror => {
            draft.add_mirror();
            state.inspector.tab = crate::inspector::InspectorTab::Own;
        }
        _ => {}
    }
}

/// Which shape: each with the key that adds it.
fn shape_menu(ui: &mut egui::Ui, draft: Option<&mut EditDraft>) {
    let Some(draft) = draft else { return };
    for (shape, key) in sp_model::Shape::ALL.into_iter().zip(crate::home::SHAPE_KEYS) {
        let name = crate::editor::shape_name(shape);
        if ui.add(egui::Button::new(name).shortcut_text(key.to_string())).clicked() {
            draft.add_shape(shape);
            ui.close();
        }
    }
}

/// What a new linked text box shows: each thing linked text can show, as the inspector offers
/// them, then each timer by name.
fn linked_menu(ui: &mut egui::Ui, draft: Option<&mut EditDraft>, timers: &[(sp_model::Uuid, String)]) {
    let Some(draft) = draft else { return };
    let mut chosen = None;
    for link in sp_model::TextLink::SIMPLE {
        if ui.button(crate::inspector::link_name(link)).clicked() {
            chosen = Some((link, crate::inspector::link_name(link).to_owned(), None));
        }
    }
    for (id, name) in timers {
        let words = crate::inspector::timer_link_name(name);
        if ui.button(words.as_str()).clicked() {
            chosen = Some((sp_model::TextLink::Timer { id: *id }, words, Some(name.as_str())));
        }
    }
    if let Some((link, name, timer)) = chosen {
        add_linked(draft, link, name, timer);
        ui.close();
    }
}

/// Adds a text box linked to `link`, named `name` and saying what it is linked to, as the
/// inspector's Linked Text does to a box, selected.
fn add_linked(draft: &mut EditDraft, link: sp_model::TextLink, name: String, timer: Option<&str>) {
    draft.add_text();
    if let Some(object) = draft.selected_object_mut() {
        object.name = name;
        object.link = Some(link);
        if let sp_model::ObjectKind::Text { text, spans, .. } = &mut object.kind {
            sp_model::edit_text(text, spans, 0, usize::MAX, &crate::links::placeholder(link, timer));
        }
    }
}

/// What a new stage object shows live: each of the stage's items.
fn stage_items_menu(ui: &mut egui::Ui, draft: Option<&mut EditDraft>) {
    let Some(draft) = draft else { return };
    for (role, _, _) in crate::stage::ITEMS {
        if ui.button(crate::stage::item_name(&sp_model::Role(role.to_owned()))).clicked() {
            draft.add(crate::stage_editor::item_object(role));
            ui.close();
        }
    }
}

/// Which box for words, of those a template's slides fill: each that the template has no box
/// for yet (two boxes cannot share one: the first would keep it), greyed when it has, saying so.
fn words_box_menu(ui: &mut egui::Ui, draft: Option<&mut EditDraft>) {
    let Some(draft) = draft else { return };
    for role in crate::editor::word_roles() {
        let taken = draft.slide.objects.iter().any(|object| object.role.as_ref() == Some(&role));
        let button = ui.add_enabled(!taken, egui::Button::new(crate::editor::role_label(&role)));
        if button.on_disabled_hover_text(t("editor.add.box.taken", "This template has one already")).clicked() {
            add_words_box(draft, role);
            ui.close();
        }
    }
}

/// Adds a template's box for the words of `role`, named after it, selected.
fn add_words_box(draft: &mut EditDraft, role: sp_model::Role) {
    draft.add_text();
    if let Some(object) = draft.selected_object_mut() {
        object.name = role.label();
        object.role = Some(role);
    }
}

/// Which audience screen a stage's screen picture shows: the main one, then each by name.
fn screen_menu(ui: &mut egui::Ui, draft: Option<&mut EditDraft>, screens: &[(sp_model::Uuid, String)]) {
    let Some(draft) = draft else { return };
    let mut choices = vec![(None, crate::stage_editor::main_screen().to_owned())];
    choices.extend(screens.iter().map(|(id, name)| (Some(*id), name.clone())));
    for (screen, name) in choices {
        let hint = t("editor.add.screen.hint", "What this screen shows, live, in a box on the stage");
        if ui.button(name).on_hover_text(hint).clicked() {
            let mut object = sp_model::Object::screen(screen);
            object.name = crate::stage_editor::screen_object_name(screen, screens);
            draft.add(object);
            ui.close();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{Language, with_language};

    #[test]
    fn each_editor_has_its_kinds_in_the_toolbars_order() {
        use Kind::*;
        let common = [Text, Shape, Picture, Web, Ndi, Mirror];
        assert_eq!(kinds(Editor::Slide), [&common[..], &[Linked]].concat());
        assert_eq!(kinds(Editor::Template), [&common[..], &[WordsBox]].concat());
        assert_eq!(kinds(Editor::Stage), [&common[..], &[Linked, Screen]].concat());
        // ▾ on the kinds that ask which one first.
        let choosing: Vec<Kind> = Kind::ALL.into_iter().filter(|kind| kind.chooses()).collect();
        assert_eq!(choosing, [Shape, Linked, WordsBox, Screen], "Media's file window is no choice under the key");
    }

    /// The toolbar's words in each language: there in every table, each on one line.
    #[test]
    fn the_toolbars_words_are_in_every_language() {
        for language in [Language::Spanish, Language::Russian, Language::Ukrainian] {
            let lacking: Vec<String> = crate::i18n::missing(language)
                .into_iter()
                .filter(|(key, _)| key.starts_with("editor.add."))
                .map(|(key, _)| key)
                .collect();
            assert!(lacking.is_empty(), "{language:?} lacks {lacking:?}");
        }
        for language in Language::ALL {
            with_language(language, || {
                for kind in Kind::ALL {
                    let (name, short, hint) = (kind.name(), kind.short(), kind.hint());
                    assert!(!name.is_empty() && !short.is_empty() && !hint.is_empty());
                    assert!(!short.contains('\n'), "{language:?} {kind:?}: one line");
                    if language != Language::English && kind != Kind::Mirror {
                        let english = with_language(Language::English, || kind.hint());
                        assert_ne!(hint, english, "{language:?} {kind:?}");
                    }
                }
            });
        }
    }

    /// Runs `check` with a ui to measure words in.
    fn measuring(check: impl FnMut(&mut egui::Ui)) {
        let ctx = crate::studio::tests::styled();
        ctx.run_ui(egui::RawInput::default(), check).drop_without_applying_deltas();
    }

    /// The form follows the room and the window, for seven keys and eight, in every language:
    /// names with the caption where they fit a wide window, tighter without it in a narrow
    /// one, and pictures alone, every key, where the names don't all fit. Never some of each.
    #[test]
    fn the_keys_all_have_their_names_or_all_are_pictures() {
        measuring(|ui| {
            for language in Language::ALL {
                with_language(language, || {
                    for editor in [Editor::Slide, Editor::Template, Editor::Stage] {
                        let kinds = kinds(editor);
                        let at = format!("{language:?} {editor:?}");
                        let wide = Plan::in_form(ui, &kinds, Form::Wide);
                        let tight = Plan::in_form(ui, &kinds, Form::Tight);
                        let pictures = Plan::in_form(ui, &kinds, Form::Pictures);
                        assert!(wide.caption.is_some() && tight.caption.is_none() && pictures.caption.is_none());
                        assert!(tight.size().x < wide.size().x && pictures.size().x < tight.size().x, "{at}");
                        // All the room there could be: the names, with the caption only in a
                        // wide window.
                        assert_eq!(Plan::of(ui, &kinds, 4000.0, 1440.0), wide, "{at}");
                        assert_eq!(Plan::of(ui, &kinds, 4000.0, 1100.0), tight, "{at}");
                        // A wide window whose room only the tighter keys fit.
                        assert_eq!(Plan::of(ui, &kinds, wide.size().x - 1.0, 1440.0), tight, "{at}");
                        // One point short of the names: every key its picture.
                        for window in [1100.0, 1440.0] {
                            assert_eq!(Plan::of(ui, &kinds, tight.size().x - 1.0, window), pictures, "{at}");
                        }
                        // A picture's key is a square, with room for ▾ where it opens a choice;
                        // a key with its name is as wide as its name needs, not one shared width.
                        for (kind, key) in kinds.iter().zip(&pictures.keys) {
                            assert_eq!(*key > KEY_TALL, kind.chooses(), "{at} {kind:?}: {key}");
                            assert!(*key < KEY_TALL + 12.0, "{at} {kind:?}: {key}");
                        }
                        let named: std::collections::BTreeSet<i32> = wide.keys.iter().map(|key| *key as i32).collect();
                        assert!(named.len() > 3, "{at}: each as wide as its name: {:?}", wide.keys);
                        // The well at the smallest window's canvas (384 across with its margins'
                        // 56 beside) holds every key.
                        assert!(pictures.size().x <= 384.0, "{at}: {}", pictures.size().x);
                    }
                });
            }
        });
    }

    #[test]
    fn the_keys_stand_in_the_well_in_order_one_point_apart() {
        measuring(|ui| {
            for form in [Form::Wide, Form::Tight, Form::Pictures] {
                let plan = Plan::in_form(ui, &kinds(Editor::Stage), form);
                let bar = plan.lay_out(pos2(10.0, 20.0));
                assert_eq!(bar.well, Rect::from_min_size(pos2(10.0, 20.0), plan.size()));
                assert_eq!(bar.well.height(), WELL_TALL);
                assert_eq!(bar.keys.len(), 8);
                for pair in bar.keys.windows(2) {
                    assert!((pair[1].min.x - pair[0].max.x - KEY_GAP).abs() < 0.01, "{form:?}");
                }
                for key in &bar.keys {
                    assert_eq!(key.height(), KEY_TALL);
                    assert!(bar.well.shrink(WELL_PAD - 0.01).contains_rect(*key), "{form:?}: {key:?}");
                }
                assert_eq!(bar.caption.is_some(), form == Form::Wide);
                if let Some(caption) = bar.caption {
                    assert!(caption.max.x < bar.keys[0].min.x && caption.min.x > bar.well.min.x);
                }
            }
        });
    }

    /// Fitted to rooms of every shape, the well sits centred over the slide, 34 points above
    /// it, the two centred together in the room (as much room over the well as under the
    /// slide) and the slide inside it.
    #[test]
    fn fitted_the_well_is_centred_over_the_slide() {
        for (width, height) in [(384.0, 520.0), (708.0, 738.0), (708.0, 300.0), (1500.0, 500.0), (900.0, 900.0)] {
            for bar in [vec2(238.0, WELL_TALL), vec2(691.0, WELL_TALL)] {
                let room = Rect::from_min_size(pos2(100.0, 50.0), vec2(width, height));
                let (at, centre, slide_width) = fit(room, bar);
                let slide = Rect::from_center_size(centre, vec2(slide_width, slide_width * 9.0 / 16.0));
                let well = Rect::from_min_size(at, bar);
                let said = format!("{bar:?} in {room:?}");
                assert!((well.center().x - slide.center().x).abs() < 0.01, "{said}");
                assert!((slide.min.y - well.max.y - OVER_SLIDE).abs() < 0.01, "{said}");
                assert!(room.expand(0.01).contains_rect(slide), "{said}: {slide:?}");
                let (above, below) = (well.min.y - room.min.y, room.max.y - slide.max.y);
                assert!((above - below).abs() < 0.01, "{said}: centred down the room, {above} and {below}");
            }
        }
    }
}
