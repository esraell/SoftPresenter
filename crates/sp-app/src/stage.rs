//! What a stage screen shows, in a layout of its own: the words on screen now, the next
//! slide's words, the time, a timer, the slide's notes, the song's title and group, the stage
//! message, and a picture of what an audience screen shows. Layouts are kept in
//! `stage-layouts.json`; each stage screen picks one.
//!
//! A layout is a list of objects, as a slide is, edited with the slide editor: each item (the
//! words now, the clock…) is a text object whose role says what it shows, beside shapes,
//! pictures, words of its own and screen pictures, in order from the back. It is kept as a
//! theme's template (`ThemeTemplate`): its `objects` are the layout's objects, each item with
//! its whole look and no words of its own; its `boxes` hold, for each item, the look and place
//! of its first object, as layouts saved before items were objects held them. A layout saved
//! then has boxes and no item objects: each of its items becomes an object with its box's look
//! and place, in the order they were drawn then, so it looks as it did.

use std::borrow::Cow;

use sp_model::{
    Align, BoxStyle, Object, ObjectKind, Rgba, Role, Slide, SlideRect, TextStyle, Theme, ThemeTemplate, Uuid, VAlign,
};

use crate::i18n::{t, tf};

/// Where the Edit view's draft of a stage layout says it is from: no song, and the layout by
/// its place in the list as the slide (as a prop's draft has `PROP_SLOT`).
pub const STAGE_SLOT: usize = usize::MAX - 2;

const NOW: &str = "now";
const NEXT_LABEL: &str = "next-label";
const NEXT: &str = "next";
const CLOCK: &str = "clock";
const TIMER: &str = "timer";
const MESSAGE: &str = "stage-message";
const NOTES: &str = "notes";
const TITLE: &str = "song-title";
const GROUP: &str = "group";

/// What a stage layout can show: each item's role, its name, and words to stand in for it in
/// the editor, in English (the command line's words); the window says them with [`item_name`]
/// and [`item_sample`].
pub const ITEMS: [(&str, &str, &str); 9] = [
    (NOW, "Current Slide", "The words on screen now"),
    (NEXT, "Next Slide", "The next slide's words"),
    (NEXT_LABEL, "Next Label", "NEXT"),
    (CLOCK, "Clock", "10:42"),
    (TIMER, "Timer", "Sermon  00:24:10"),
    (NOTES, "Slide Notes", "Notes for the slide on screen"),
    (TITLE, "Song Title", "Song Title"),
    (GROUP, "Group", "Chorus"),
    (MESSAGE, "Stage Message", "A note for the stage"),
];

/// An item's name, by its role, in the window's language.
pub fn item_name(role: &Role) -> &'static str {
    match role.0.as_str() {
        NOW => t("tools.stage.item.now", "Current Slide"),
        NEXT => t("tools.stage.item.next", "Next Slide"),
        NEXT_LABEL => t("tools.stage.item.next_label", "Next Label"),
        CLOCK => t("tools.stage.item.clock", "Clock"),
        TIMER => t("tools.stage.item.timer", "Timer"),
        NOTES => t("tools.stage.item.notes", "Slide Notes"),
        TITLE => t("tools.stage.item.title", "Song Title"),
        GROUP => t("tools.stage.item.group", "Group"),
        MESSAGE => t("tools.stage.item.message", "Stage Message"),
        _ => t("tools.stage.item.other", "Item"),
    }
}

/// An item's stand-in words for the editor, by its role, in the window's language.
pub fn item_sample(role: &Role) -> &'static str {
    match role.0.as_str() {
        NOW => t("tools.stage.sample.now", "The words on screen now"),
        NEXT => t("tools.stage.sample.next", "The next slide's words"),
        NEXT_LABEL => t("tools.stage.next_word", "NEXT"),
        TIMER => t("tools.stage.sample.timer", "Sermon  00:24:10"),
        NOTES => t("tools.stage.sample.notes", "Notes for the slide on screen"),
        TITLE => t("tools.stage.sample.title", "Song Title"),
        GROUP => t("tools.stage.sample.group", "Chorus"),
        MESSAGE => t("tools.stage.sample.message", "A note for the stage"),
        // The clock's stand-in is a time, the same in every language.
        _ => ITEMS.iter().find(|(have, _, _)| *have == role.0).map_or("", |(_, _, sample)| sample),
    }
}

/// A box for a stage item, in its place, at its size and colour.
pub fn item_box(role: &str, rect: SlideRect, size: f32, fill: Rgba, align: Align, valign: VAlign) -> BoxStyle {
    BoxStyle {
        role: Role(role.to_owned()),
        name: None,
        rect,
        size,
        min_size: (size * 0.4).max(18.0),
        fill,
        align,
        valign,
        outline_width: 0.0,
        shadow: false,
        ..base_box()
    }
}

/// The layouts SoftPresenter comes with.
pub fn builtin_layouts() -> Vec<ThemeTemplate> {
    let id = |n: u128| Uuid::from_u128(0x7374_6167_6500_0000_0000_0000_0000_0000 + n);
    let rect = |x: f32, y: f32, w: f32, h: f32| SlideRect { x, y, w, h };
    let (white, grey, yellow, green) =
        (Rgba::WHITE, Rgba::rgb(140, 140, 140), Rgba::rgb(255, 200, 60), Rgba::rgb(120, 220, 140));
    let clock = || item_box(CLOCK, rect(1360.0, 24.0, 520.0, 90.0), 64.0, white, Align::Right, VAlign::Middle);
    let timer = || item_box(TIMER, rect(60.0, 24.0, 900.0, 90.0), 64.0, green, Align::Left, VAlign::Middle);
    let message = || item_box(MESSAGE, rect(0.0, 120.0, 1920.0, 110.0), 64.0, white, Align::Center, VAlign::Middle);
    [
        ThemeTemplate {
            objects: Vec::new(),
            in_front: Vec::new(),
            songs: true,
            bible: false,
            id: id(2),
            name: t("tools.stage.layout.words_next", "Words and Next").to_owned(),
            boxes: vec![
                item_box(NOW, rect(60.0, 130.0, 1800.0, 560.0), 110.0, white, Align::Center, VAlign::Middle),
                BoxStyle {
                    weight: 600,
                    ..item_box(NEXT_LABEL, rect(60.0, 712.0, 600.0, 50.0), 34.0, grey, Align::Left, VAlign::Middle)
                },
                item_box(NEXT, rect(60.0, 770.0, 1800.0, 280.0), 64.0, yellow, Align::Center, VAlign::Top),
                clock(),
                timer(),
                message(),
            ],
        },
        ThemeTemplate {
            objects: Vec::new(),
            in_front: Vec::new(),
            songs: true,
            bible: false,
            id: id(3),
            name: t("tools.stage.layout.words_only", "Words Only").to_owned(),
            boxes: vec![
                item_box(NOW, rect(60.0, 130.0, 1800.0, 900.0), 140.0, white, Align::Center, VAlign::Middle),
                clock(),
                message(),
            ],
        },
        ThemeTemplate {
            objects: Vec::new(),
            in_front: Vec::new(),
            songs: true,
            bible: false,
            id: id(4),
            name: t("tools.stage.layout.words_notes", "Words and Notes").to_owned(),
            boxes: vec![
                item_box(GROUP, rect(60.0, 24.0, 900.0, 90.0), 56.0, grey, Align::Left, VAlign::Middle),
                item_box(NOW, rect(60.0, 130.0, 1800.0, 520.0), 100.0, white, Align::Center, VAlign::Middle),
                item_box(NOTES, rect(60.0, 680.0, 1800.0, 370.0), 56.0, yellow, Align::Left, VAlign::Top),
                clock(),
                message(),
            ],
        },
        ThemeTemplate {
            objects: Vec::new(),
            in_front: Vec::new(),
            songs: true,
            bible: false,
            id: id(5),
            name: t("tools.stage.layout.clock_timer", "Clock and Timer").to_owned(),
            boxes: vec![
                item_box(TITLE, rect(60.0, 24.0, 1800.0, 90.0), 56.0, grey, Align::Center, VAlign::Middle),
                item_box(CLOCK, rect(60.0, 220.0, 1800.0, 380.0), 300.0, white, Align::Center, VAlign::Middle),
                item_box(TIMER, rect(60.0, 640.0, 1800.0, 300.0), 160.0, green, Align::Center, VAlign::Middle),
                message(),
            ],
        },
    ]
    .iter()
    .map(normalized)
    .collect()
}

/// The order the items were drawn in before they were objects, back first: a layout saved
/// then has its items made into objects in this order, so they overlap as they did.
const DRAWN_BEFORE: [&str; 9] = [CLOCK, TIMER, NOW, NOTES, TITLE, GROUP, NEXT_LABEL, NEXT, MESSAGE];

/// True when a role is one of the items a stage layout can show.
pub fn is_item(role: &Role) -> bool {
    ITEMS.iter().any(|(have, _, _)| *have == role.0)
}

/// The item a text object shows, if it shows one.
pub fn item_of(object: &Object) -> Option<&'static str> {
    let role = object.role.as_ref().filter(|_| matches!(object.kind, ObjectKind::Text { .. }))?;
    ITEMS.iter().find(|(have, _, _)| *have == role.0).map(|(have, _, _)| *have)
}

/// An id for an item's object that is the same at every start until the layout is saved, so
/// a layout from before items were objects is the same each time it is read.
fn item_id(layout: Uuid, role: &str) -> Uuid {
    let mut hash: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
    for byte in role.bytes() {
        hash ^= u128::from(byte);
        hash = hash.wrapping_mul(0x0000_0000_0100_0000_0000_0000_0000_013b);
    }
    Uuid::from_u128(layout.as_u128() ^ hash)
}

/// The object an item's box stands for, as a layout from before items were objects has it:
/// the box's place and whole look, and no words.
fn item_object(layout: Uuid, style: &BoxStyle) -> Object {
    let mut object = Object::text(style.role.clone(), "");
    object.id = item_id(layout, &style.role.0);
    object.name = item_name(&style.role).to_owned();
    object.frame = Some(style.rect);
    if let ObjectKind::Text { style: own, .. } = &mut object.kind {
        *own = TextStyle::all(style);
    }
    object
}

/// A layout's objects, back first: those it keeps, then an object for each item it has a box
/// for and no object (a layout saved before items were objects), in the order they were drawn.
pub fn objects(layout: &ThemeTemplate) -> Cow<'_, [Object]> {
    let has_object = |role: &str| layout.objects.iter().any(|object| item_of(object) == Some(role));
    let missing: Vec<&BoxStyle> = DRAWN_BEFORE
        .iter()
        .filter(|role| !has_object(role))
        .filter_map(|role| layout.boxes.iter().find(|style| style.role.0 == *role))
        .collect();
    if missing.is_empty() {
        return Cow::Borrowed(&layout.objects);
    }
    let mut all = layout.objects.clone();
    all.extend(missing.into_iter().map(|style| item_object(layout.id, style)));
    Cow::Owned(all)
}

/// The layout with an object for each of its items (see [`objects`]).
pub fn normalized(layout: &ThemeTemplate) -> ThemeTemplate {
    match objects(layout) {
        Cow::Borrowed(_) => layout.clone(),
        Cow::Owned(objects) => ThemeTemplate { objects, ..layout.clone() },
    }
}

/// The look an item's words have in the editor before anything is changed: the plain stage
/// look, at the item's place. The editor keeps each object's own look as its difference from
/// this, which does not change while it is open, so undo gives back the look it took away.
fn editor_box(role: Role, rect: SlideRect, min_size: f32) -> BoxStyle {
    BoxStyle { role, rect, min_size, ..base_box() }
}

/// The theme the editor works with on a layout: the plain stage look for every item and for
/// words of its own, and nothing drawn behind (the layout's objects are all on the slide).
pub fn editor_theme(layout: &ThemeTemplate) -> Theme {
    let mut boxes = vec![base_box()];
    boxes.extend(layout.boxes.iter().map(|style| editor_box(style.role.clone(), style.rect, style.min_size)));
    Theme {
        id: Uuid::from_u128(0x7374_6167_6500_0000_0000_0000_0000_0002),
        name: "Stage".to_owned(),
        templates: vec![ThemeTemplate { boxes, objects: Vec::new(), ..layout.clone() }],
        background: None,
        background_picture: None,
        versions: Vec::new(),
    }
}

/// A layout as a slide for the editor: its objects, each item showing words that say what it
/// is.
pub fn as_slide(layout: &ThemeTemplate) -> Slide {
    let objects = objects(layout)
        .iter()
        .map(|object| {
            let mut object = object.clone();
            let (item, shows) = (item_of(&object), object.shows);
            if let Some(item) = item {
                object.set_text(&sample_for(item, &shows));
            }
            object
        })
        .collect();
    Slide { template: Some(layout.id), ..Slide::new(objects) }
}

/// The layout as the editor left that slide: its objects as they are on it, each text object
/// with its whole look (an item with no words of its own), and each item's box following its
/// first object, for anything that reads the boxes.
pub fn with_slide(layout: &ThemeTemplate, slide: &Slide) -> ThemeTemplate {
    let mut boxes: Vec<BoxStyle> = Vec::new();
    let mut objects = Vec::with_capacity(slide.objects.len());
    for object in &slide.objects {
        let mut kept = object.clone();
        kept.shown_paragraphs = None;
        let item = item_of(object);
        if let ObjectKind::Text { text, style, spans } = &mut kept.kind {
            let role = object.role.clone().unwrap_or_else(Role::primary);
            let rect = object.frame.unwrap_or(SlideRect { x: 0.0, y: 0.0, w: 1920.0, h: 1080.0 });
            let min_size = layout.style_for(&role).map_or(30.0, |style| style.min_size);
            let look = style.over(&editor_box(role.clone(), rect, min_size));
            *style = TextStyle::all(&look);
            if let Some(item) = item {
                sp_model::edit_text(text, spans, 0, usize::MAX, "");
                if !boxes.iter().any(|have| have.role.0 == item) {
                    let mut mirror = layout
                        .style_for(&role)
                        .cloned()
                        .unwrap_or_else(|| item_box(item, rect, look.size, look.fill, look.align, look.valign));
                    mirror = style.over(&mirror);
                    mirror.rect = rect;
                    boxes.push(mirror);
                }
            }
        }
        objects.push(kept);
    }
    // The boxes keep the order they had, new ones after; a box for something that is not an
    // item (none are made) is kept as it was.
    let place = |style: &BoxStyle| layout.boxes.iter().position(|have| have.role == style.role).unwrap_or(usize::MAX);
    boxes.sort_by_key(place);
    boxes.extend(layout.boxes.iter().filter(|style| !is_item(&style.role)).cloned());
    ThemeTemplate { boxes, objects, ..layout.clone() }
}

/// A layout as a theme, to draw a stage screen with: the layout with its objects (the stage's
/// slide carries each of them itself, so none is drawn behind it twice), each item's box, and
/// the plain stage look for words of its own.
pub fn theme_for(layout: &ThemeTemplate) -> Theme {
    let mut layout = normalized(layout);
    if layout.style_for(&Role::primary()).is_none() {
        layout.boxes.push(base_box());
    }
    Theme {
        id: Uuid::from_u128(0x7374_6167_6500_0000_0000_0000_0000_0001),
        name: "Stage".to_owned(),
        templates: vec![layout],
        background: None,
        background_picture: None,
        versions: Vec::new(),
    }
}

/// The layout a stage screen asks for, or the first one if it asks for none (or one deleted).
pub fn layout_for(layouts: &[ThemeTemplate], wanted: Option<Uuid>) -> ThemeTemplate {
    wanted
        .and_then(|id| layouts.iter().find(|layout| layout.id == id))
        .or(layouts.first())
        .cloned()
        .unwrap_or_else(|| builtin_layouts().remove(0))
}

/// A new layout, laid out like the first built-in one, with ids of its own.
pub fn new_layout(name: String) -> ThemeTemplate {
    copy_of(&builtin_layouts().remove(0), name)
}

/// A copy of a layout under another name, with ids of its own.
pub fn copy_of(layout: &ThemeTemplate, name: String) -> ThemeTemplate {
    let objects = objects(layout).iter().map(|object| Object { id: Uuid::new_v4(), ..object.clone() }).collect();
    ThemeTemplate { id: Uuid::new_v4(), name, objects, bible: false, ..layout.clone() }
}

/// Which audience screen a screen picture shows: the one it names while that is still an
/// audience screen, otherwise the first audience screen. The second part is true when it
/// names a screen it cannot show (gone, or now a stage screen) and shows the first instead.
/// `None` when there is no audience screen at all.
pub fn source_screen(wanted: Option<Uuid>, screens: &[crate::screens::Screen]) -> (Option<Uuid>, bool) {
    let audience = |screen: &&crate::screens::Screen| screen.kind == crate::screens::Kind::Audience;
    match wanted {
        Some(id) if screens.iter().filter(audience).any(|screen| screen.id == id) => (Some(id), false),
        wanted => (screens.iter().find(audience).map(|screen| screen.id), wanted.is_some()),
    }
}

/// The layout with each screen picture naming the audience screen it shows now (see
/// [`source_screen`]), for the screens' thread, which knows no screen's order or kind. With no
/// audience screen it names none of them (`Uuid::nil()`), which is drawn as the audience
/// picture with no look of its own.
pub fn with_sources(layout: &ThemeTemplate, screens: &[crate::screens::Screen]) -> ThemeTemplate {
    let mut layout = normalized(layout);
    for object in &mut layout.objects {
        if let ObjectKind::Screen { screen, .. } = &mut object.kind {
            *screen = Some(source_screen(*screen, screens).0.unwrap_or(Uuid::nil()));
        }
    }
    layout
}

/// The layout with one of its objects (by its place among [`objects`]) moved to `frame`, its
/// box following it when it is an item's first.
pub fn with_frame(layout: &ThemeTemplate, index: usize, frame: SlideRect) -> ThemeTemplate {
    let mut slide = as_slide(layout);
    if let Some(object) = slide.objects.get_mut(index) {
        object.frame = Some(frame);
    }
    with_slide(layout, &slide)
}

/// What one of a layout's objects shows, in words, for the command line.
pub fn describe(object: &Object, screens: &[crate::screens::Screen]) -> String {
    let name_of = |id: Option<Uuid>| screens.iter().find(|screen| Some(screen.id) == id).map(|s| s.name.clone());
    match (&object.kind, item_of(object)) {
        (_, Some(item)) => {
            // In English, as the command line speaks: `ITEMS` keeps each item's English name.
            let name = ITEMS.iter().find(|(have, _, _)| *have == item).map_or("item", |(_, name, _)| name);
            format!("shows {}", name.to_lowercase())
        }
        (ObjectKind::Screen { screen, .. }, None) => {
            let (shows, fell_back) = source_screen(*screen, screens);
            match (screen, shows.and_then(|id| name_of(Some(id)))) {
                (_, None) => "a screen picture, with no audience screen to show".to_owned(),
                (None, Some(main)) => format!("what the main audience screen ({main}) shows"),
                (Some(_), Some(shown)) if fell_back => {
                    format!("what the main audience screen ({shown}) shows, as the screen it named has gone")
                }
                (Some(_), Some(shown)) => format!("what {shown} shows"),
            }
        }
        (ObjectKind::Text { text, .. }, None) => format!("the words \u{201c}{}\u{201d}", text.replace('\n', " / ")),
        (ObjectKind::Shape { shape }, None) => format!("a shape ({})", shape.name().to_lowercase()),
        (ObjectKind::Image { file, .. }, None) => format!("a picture ({file})"),
        (ObjectKind::Web { url }, None) => format!("a web page ({url})"),
        (ObjectKind::Ndi { source, .. }, None) if source.is_empty() => "an NDI source (none chosen)".to_owned(),
        (ObjectKind::Ndi { source, .. }, None) => format!("an NDI source ({})", sp_model::ndi_short_name(source)),
        (ObjectKind::Mirror { .. }, None) => "an AirPlay screen".to_owned(),
    }
}

/// The objects of the layouts the stage screens show while they are on, each layout once: the
/// app opens their web pages and plays their videos, as it does a slide's on screen.
pub fn shown_objects(state: &crate::home::HomeState) -> Vec<Object> {
    if !state.stage_on {
        return Vec::new();
    }
    let mut seen = Vec::new();
    let mut shown = Vec::new();
    let stages = state.screens.iter().filter(|screen| screen.kind == crate::screens::Kind::Stage && !screen.is_off());
    for screen in stages {
        let layout = layout_for(&state.stage_layouts, screen.stage_layout);
        if !seen.contains(&layout.id) {
            seen.push(layout.id);
            shown.extend(objects(&layout).iter().filter(|object| !object.hidden).cloned());
        }
    }
    shown
}

/// The screens a layout's screen pictures show, each once, as [`with_sources`] left them.
pub fn sources(layout: &ThemeTemplate) -> Vec<Uuid> {
    let mut found = Vec::new();
    for object in layout.objects.iter().filter(|object| !object.hidden) {
        if let ObjectKind::Screen { screen: Some(id), .. } = &object.kind
            && !found.contains(id)
        {
            found.push(*id);
        }
    }
    found
}

fn file() -> Option<std::path::PathBuf> {
    crate::bible_store::data_dir().map(|dir| dir.join("stage-layouts.json"))
}

/// The stage layouts: the ones saved, or the built-in ones if none are; and a file there that
/// couldn't be read, kept aside (`whole_file`).
pub fn load_layouts() -> (Vec<ThemeTemplate>, Option<crate::whole_file::Unread>) {
    let (layouts, unread) = file().map_or((Vec::new(), None), |path| load_layouts_from(&path));
    (if layouts.is_empty() { builtin_layouts() } else { layouts }, unread)
}

fn load_layouts_from(path: &std::path::Path) -> (Vec<ThemeTemplate>, Option<crate::whole_file::Unread>) {
    crate::whole_file::read_or_default(path, crate::whole_file::What::StageLayouts, read_layouts)
}

/// Layouts from a file's words, each with an object for every item (none when it lists none).
/// The error, for the log, says why they can't be read.
fn read_layouts(text: &str) -> Result<Vec<ThemeTemplate>, String> {
    let layouts = serde_json::from_str::<Vec<ThemeTemplate>>(text).map_err(|e| e.to_string())?;
    Ok(layouts.iter().map(normalized).collect())
}

/// The stage layouts' file and what goes in it, for the library's writer to write (`writer`),
/// paced (`writer::Paced`). The error is a plain sentence.
pub fn layouts_to_save(layouts: &[ThemeTemplate]) -> Result<crate::writer::Save, String> {
    let path = file()
        .ok_or_else(|| t("media.no_place", "Could not find where to keep settings on this computer.").to_owned())?;
    Ok(layouts_to_save_at(path, layouts))
}

fn layouts_to_save_at(path: std::path::PathBuf, layouts: &[ThemeTemplate]) -> crate::writer::Save {
    let text = serde_json::to_string_pretty(layouts).unwrap_or_default();
    crate::whole_file::settings_save(path, crate::whole_file::What::StageLayouts, text.into_bytes())
}

/// What the stage screens are told about the song on screen besides its slides.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct About {
    pub title: Option<String>,
    pub group: Option<String>,
}

/// The look every stage box starts from: bold, white, no outline or shadow.
fn base_box() -> BoxStyle {
    BoxStyle {
        role: Role::primary(),
        name: None,
        rect: SlideRect { x: 0.0, y: 0.0, w: 1920.0, h: 1080.0 },
        font_family: sp_model::DEFAULT_FONT_FAMILY.to_owned(),
        weight: 700,
        italic: false,
        size: 96.0,
        min_size: 30.0,
        fill: Rgba::WHITE,
        align: Align::Center,
        outline_width: 0.0,
        outline_color: Rgba::BLACK,
        shadow: false,
        shadow_look: sp_model::Shadow::default(),
        valign: VAlign::Middle,
        caps: sp_model::Caps::None,
        underline: false,
        strike: false,
        scaling: sp_model::Scaling::TextDown,
        line_transform: sp_model::LineTransform::None,
        line_height: sp_model::LINE_HEIGHT,
        letter_spacing: 0.0,
        margins: [0.0; 4],
        line_fill: sp_model::LineFill::default(),
        list: Default::default(),
        scroll: Default::default(),
        paragraph_spacing: 0.0,
        baseline: 0.0,
        gradient: sp_model::Gradient::default(),
    }
}

/// All the words on a slide, each box's on its own lines.
fn words(slide: &Slide) -> String {
    slide
        .texts()
        .into_iter()
        .map(|(_, text)| text)
        .filter(|text| !text.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// The time on a clock item, in the form it asks for: on a 12-hour clock without seconds, as
/// stage screens always showed it, unless it says otherwise.
fn clock_text(shows: &sp_model::StageShows) -> String {
    let form = match (shows.hours_24, shows.seconds) {
        (false, false) => "%-I:%M",
        (false, true) => "%-I:%M:%S",
        (true, false) => "%H:%M",
        (true, true) => "%H:%M:%S",
    };
    chrono::Local::now().format(form).to_string()
}

/// The words an item stands in with in the editor.
fn sample_for(item: &str, shows: &sp_model::StageShows) -> String {
    match item {
        CLOCK => match (shows.hours_24, shows.seconds) {
            (false, false) => "10:42",
            (false, true) => "10:42:15",
            (true, false) => "22:42",
            (true, true) => "22:42:15",
        }
        .to_owned(),
        NOTES if shows.lines > 0 => (1..=shows.lines)
            .map(|line| tf("tools.stage.sample.notes_line", "Line {line} of the notes", &[("line", &line.to_string())]))
            .collect::<Vec<_>>()
            .join("\n"),
        other => item_sample(&Role(other.to_owned())).to_owned(),
    }
}

/// The first `lines` lines of some words, or all of them for 0.
fn first_lines(text: &str, lines: u8) -> String {
    if lines == 0 {
        return text.to_owned();
    }
    text.lines().take(usize::from(lines)).collect::<Vec<_>>().join("\n")
}

/// What an item shows now, or `None` when it has nothing to show (no timer running, no next
/// slide…): then it is left off the stage, fill and all.
fn item_value(
    item: &str,
    shows: &sp_model::StageShows,
    now: Option<&Slide>,
    next: Option<&Slide>,
    overlay: &crate::output::Overlay,
    about: &About,
) -> Option<String> {
    match item {
        CLOCK => Some(clock_text(shows)),
        TIMER => {
            let timer = match shows.timer {
                Some(id) => overlay.timers.iter().find(|timer| timer.id == id),
                None => overlay.timers.iter().find(|timer| timer.running()),
            }?;
            let value = timer.text(std::time::Instant::now(), crate::timers::seconds_into_day());
            Some(format!("{}  {value}", timer.name))
        }
        NOW => now.map(words),
        NOTES => now
            .and_then(|now| now.notes.as_deref())
            .filter(|notes| !notes.trim().is_empty())
            .map(|notes| first_lines(notes, shows.lines)),
        TITLE => about.title.clone(),
        GROUP => about.group.clone().filter(|group| !group.trim().is_empty()),
        NEXT_LABEL => next.map(|_| t("tools.stage.next_word", "NEXT").to_owned()),
        NEXT => next.map(words),
        MESSAGE => overlay.stage_message.clone().filter(|message| !message.trim().is_empty()),
        _ => None,
    }
}

/// The stage screen's picture as a slide: the layout's objects, back first, each item showing
/// what it shows now (the time, the words on screen now, the next slide's words, a timer, the
/// slide's notes, the song's title and group, the stage message); an item with nothing to show
/// is hidden, box and fill and all (it is still on the slide, so the layout's copy of it, drawn
/// behind a slide's objects, is not drawn either). Linked words of the layout's own (the date,
/// say) are filled in too.
pub fn slide(
    now: Option<&Slide>,
    next: Option<&Slide>,
    overlay: &crate::output::Overlay,
    about: &About,
    layout: &ThemeTemplate,
) -> Slide {
    let values = crate::links::Values {
        timers: &overlay.timers,
        title: about.title.as_deref(),
        group: about.group.as_deref(),
        next,
        stage_message: overlay.stage_message.as_deref(),
        copyright: overlay.copyright.as_deref(),
    };
    let empty = Slide::new(Vec::new());
    let mut shown = Vec::new();
    for object in objects(layout).iter() {
        let mut object = object.clone();
        match item_of(&object).filter(|_| !object.hidden) {
            Some(item) => {
                let value = item_value(item, &object.shows, now, next, overlay, about);
                object.hidden = value.is_none();
                object.set_text(&value.unwrap_or_default());
                // The stage message stands out in red, unless its box has a fill of its own.
                if item == MESSAGE && object.fill.is_none() {
                    object.fill = Some(Rgba::rgb(190, 30, 30));
                }
            }
            None => {
                if let Some(link) = object.link {
                    object.set_text(&crate::links::value(link, now.unwrap_or(&empty), &values));
                }
            }
        }
        shown.push(object);
    }
    Slide { template: Some(layout.id), ..Slide::new(shown) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stage_layouts_file_that_cant_be_read_is_kept_aside_and_never_saved_over() {
        let good = serde_json::to_string_pretty(&builtin_layouts()).unwrap();
        crate::whole_file::tests::check_loader(
            "stage-layouts.json",
            crate::whole_file::What::StageLayouts,
            good.as_bytes(),
            |path| load_layouts_from(path).1,
            |path| crate::whole_file::tests::written(layouts_to_save_at(path.to_owned(), &[])),
        );
    }

    /// Draws a stage screen and saves it where `STAGE_PNG` says, to look at. Run by hand:
    /// `STAGE_PNG=/tmp/stage.png cargo test -p sp-app stage_picture -- --ignored`.
    #[test]
    #[ignore = "needs a GPU, and is for looking at"]
    fn stage_picture() {
        use sp_render::vello::util::RenderContext;
        use sp_render::{Fonts, Painter, build_scene, read_pixels, target_texture, wgpu};
        let Ok(path) = std::env::var("STAGE_PNG") else { return };
        let song = sp_model::parse_song_text(
            "S",
            "We lift our voices, we lift our hands\nWe sing together across the lands\n--\nМы поднимаем голоса и руки\n\nOne song, one heart\n--\nОдна песня, одно сердце\n",
        );
        let built = slide(
            song.slide(0).map(|(_, s)| s),
            song.slide(1).map(|(_, s)| s),
            &crate::output::Overlay::default(),
            &About::default(),
            &builtin_layouts()[0],
        );
        let (w, h) = (1920, 1080);
        let mut cx = RenderContext::new();
        let id = pollster::block_on(cx.device(None)).expect("a GPU");
        let (device, queue) = (&cx.devices[id].device, &cx.devices[id].queue);
        let scene = build_scene(&mut Fonts::new(), Some(&built), &theme_for(&builtin_layouts()[0]), w, h).scene;
        let texture = target_texture(device, w, h);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Painter::new(device).expect("painter").paint(device, queue, &scene, &view, w, h).expect("paint");
        let pixels = read_pixels(device, queue, &texture).expect("read back");
        let mut png = png::Encoder::new(std::fs::File::create(path).expect("file"), w, h);
        png.set_color(png::ColorType::Rgba);
        png.set_depth(png::BitDepth::Eight);
        png.write_header().and_then(|mut writer| writer.write_image_data(&pixels)).expect("png");
    }

    /// How long the screens' thread takes to draw a stage screen, with and without a picture of
    /// the audience screen on it, as it draws them: the stage's scene built (the audience's
    /// scene taken from its window, as a copy), painted at 1920 × 1080, and waited for until the
    /// GPU has finished. Prints the figures. Run by hand:
    /// `cargo test -p sp-app stage_drawing_time -- --ignored --nocapture`.
    #[test]
    #[ignore = "needs a GPU, and is for measuring"]
    fn stage_drawing_time() {
        use sp_render::vello::util::RenderContext;
        use sp_render::{Fonts, Layers, Painter, build_layers, target_texture, wgpu};
        let song = sp_model::parse_song_text(
            "S",
            "We lift our voices, we lift our hands\nWe sing together across the lands\n--\nМы поднимаем голоса и руки\nМы поём вместе по всей земле\n\nOne song, one heart\n--\nОдна песня, одно сердце\n",
        );
        let (now, next) = (song.slide(0).map(|(_, s)| s), song.slide(1).map(|(_, s)| s));
        let (w, h) = (1920, 1080);
        let mut cx = RenderContext::new();
        let id = pollster::block_on(cx.device(None)).expect("a GPU");
        let (device, queue) = (&cx.devices[id].device, &cx.devices[id].queue);
        let mut painter = Painter::new(device).expect("painter");
        let mut fonts = Fonts::new();
        let texture = target_texture(device, w, h);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        // The audience screen: the song's slide over a photograph-like picture, as its window
        // holds it.
        let pixels: Vec<u8> =
            (0..w * h).flat_map(|i| [(i % 251) as u8, (i % 241) as u8, (i % 239) as u8, 255]).collect();
        let media = sp_render::Picture::from_rgba(w, h, pixels).expect("a picture");
        let audience_theme = Theme::builtin();
        let audience = build_layers(
            &mut fonts,
            &Layers { media: Some(&media), slide: now, ..Default::default() },
            &audience_theme,
            w,
            h,
        )
        .scene;
        let screen = Uuid::from_u128(1);
        let plain = builtin_layouts().remove(0);
        let mut with_picture = plain.clone();
        let mut object = Object::screen(Some(screen));
        object.frame = Some(SlideRect { x: 1240.0, y: 130.0, w: 640.0, h: 360.0 });
        with_picture.objects.push(object);
        let overlay = crate::output::Overlay::default();
        let mut draw = |layout: &ThemeTemplate, picture: bool| {
            let started = std::time::Instant::now();
            let stage = slide(now, next, &overlay, &About::default(), layout);
            let theme = theme_for(layout);
            let pictures = if picture {
                vec![sp_render::ScreenPicture { screen, scene: audience.clone(), width: w, height: h }]
            } else {
                Vec::new()
            };
            let layers = Layers { slide: Some(&stage), ..Default::default() };
            let scene =
                sp_render::with_screen_pictures(pictures, || build_layers(&mut fonts, &layers, &theme, w, h)).scene;
            let built = started.elapsed();
            painter.paint(device, queue, &scene, &view, w, h).expect("paint");
            let _ = device.poll(wgpu::PollType::wait_indefinitely());
            (built, started.elapsed())
        };
        // Taken in turns, so a busy computer slows both alike.
        let rounds = 300;
        let mut sums = [(std::time::Duration::ZERO, std::time::Duration::ZERO); 2];
        for round in 0..rounds + 20 {
            for (at, (layout, picture)) in [(&plain, false), (&with_picture, true)].into_iter().enumerate() {
                let (built, total) = draw(layout, picture);
                if round >= 20 {
                    sums[at].0 += built;
                    sums[at].1 += total;
                }
            }
        }
        for (name, (built, total)) in ["without", "with"].into_iter().zip(sums) {
            println!(
                "stage screen {name} a screen picture: {:.2} ms each ({:.2} ms building the scene), over {rounds}",
                total.as_secs_f64() * 1000.0 / f64::from(rounds),
                built.as_secs_f64() * 1000.0 / f64::from(rounds)
            );
        }
    }

    #[test]
    fn a_stage_screen_shows_the_time_the_words_now_and_the_next_words() {
        let song = sp_model::parse_song_text("S", "One\n--\nОдин\n\nTwo\n");
        let (now, next) = (song.slide(0).unwrap().1, song.slide(1).unwrap().1);
        let none = About::default();
        let layout = &builtin_layouts()[0];
        let built = slide(Some(now), Some(next), &crate::output::Overlay::default(), &none, layout);
        let text = |role: &str| {
            built
                .objects
                .iter()
                .find(|o| o.role == Some(Role(role.to_owned())))
                .and_then(|o| o.as_text())
                .map(str::to_owned)
        };
        assert_eq!(text(NOW).as_deref(), Some("One\nОдин"));
        assert_eq!(text(NEXT).as_deref(), Some("Two"));
        assert!(text(CLOCK).is_some_and(|time| time.contains(':')));
        // Nothing on screen and nothing next: only the time.
        assert_eq!(
            slide(None, None, &crate::output::Overlay::default(), &none, layout)
                .objects
                .iter()
                .filter(|o| !o.hidden)
                .count(),
            1
        );
        // A running timer and a stage message show too.
        let mut timer = crate::timers::Timer::new("Sermon".to_owned());
        timer.start(std::time::Instant::now());
        let overlay = crate::output::Overlay {
            timers: vec![timer],
            stage_message: Some("Wrap up".to_owned()),
            ..crate::output::Overlay::default()
        };
        let with = slide(None, None, &overlay, &none, layout);
        assert!(with.objects.iter().any(|o| o.as_text().is_some_and(|t| t.starts_with("Sermon  00:0"))));
        assert!(with.objects.iter().any(|o| o.as_text() == Some("Wrap up") && o.fill.is_some()));
        // Every box the slide uses is in the first layout.
        let theme = theme_for(&builtin_layouts()[0]);
        for object in &built.objects {
            assert!(theme.templates[0].style_for(object.role.as_ref().unwrap()).is_some());
        }
    }

    #[test]
    fn layouts_show_notes_title_and_group_and_a_screen_falls_back_to_the_first() {
        let mut song = sp_model::parse_song_text("S", "[Chorus]\nOne\n");
        song.groups[0].slides[0].notes = Some("Key change".to_owned());
        let now = song.slide(0).unwrap().1;
        let about = About { title: Some("Song".to_owned()), group: Some("Chorus".to_owned()) };
        let rect = SlideRect { x: 0.0, y: 0.0, w: 100.0, h: 100.0 };
        let everything = ThemeTemplate {
            id: Uuid::new_v4(),
            name: "All".to_owned(),
            objects: Vec::new(),
            in_front: Vec::new(),
            songs: true,
            bible: false,
            boxes: ITEMS
                .iter()
                .map(|(role, _, _)| item_box(role, rect, 40.0, Rgba::WHITE, Align::Center, VAlign::Middle))
                .collect(),
        };
        let built = slide(Some(now), None, &crate::output::Overlay::default(), &about, &everything);
        for (role, words) in [(NOTES, "Key change"), (TITLE, "Song"), (GROUP, "Chorus")] {
            assert!(built.objects.iter().any(|o| o.role == Some(Role(role.to_owned())) && o.as_text() == Some(words)));
        }
        // A layout without a box for an item leaves it out, rather than drawing it in its first box.
        let words_only = &builtin_layouts()[1];
        let mut timer = crate::timers::Timer::new("Sermon".to_owned());
        timer.start(std::time::Instant::now());
        let overlay = crate::output::Overlay { timers: vec![timer], ..crate::output::Overlay::default() };
        let trimmed = slide(Some(now), None, &overlay, &about, words_only);
        assert!(trimmed.objects.iter().all(|o| words_only.style_for(o.role.as_ref().unwrap()).is_some()));
        assert!(!trimmed.objects.iter().any(|o| o.role == Some(Role(TIMER.to_owned()))));
        // Each built-in layout has a box only for items it knows, each item at most once.
        let layouts = builtin_layouts();
        for layout in &layouts {
            for (index, item) in layout.boxes.iter().enumerate() {
                assert!(ITEMS.iter().any(|(role, _, _)| *role == item.role.0), "{}", item.role.0);
                assert!(layout.boxes[index + 1..].iter().all(|other| other.role != item.role));
            }
        }
        assert_eq!(layout_for(&layouts, Some(layouts[2].id)).name, "Words and Notes");
        assert_eq!(layout_for(&layouts, Some(Uuid::new_v4())).id, layouts[0].id);
        assert_eq!(layout_for(&[], None).id, layouts[0].id);
        let made = new_layout("Mine".to_owned());
        assert_ne!(made.id, layouts[0].id);
        assert_eq!(
            serde_json::from_str::<Vec<ThemeTemplate>>(&serde_json::to_string(&layouts).unwrap()).unwrap(),
            layouts
        );
    }
}

/// Stage layouts as objects: old layouts, the editor's work on them, and screen pictures.
#[cfg(test)]
mod object_tests {
    use super::*;
    use crate::editor::EditDraft;
    use crate::screens::{Output, Screen};

    /// The stage layouts as the app saved them before items were objects (the four built in
    /// and one changed by hand, with a shape behind and the stage message moved to the back of
    /// its boxes), written by that version's own code.
    const SAVED_BEFORE: &str = include_str!("testdata/stage-layouts-before-objects.json");

    /// What that version's stage screens were given for each of those layouts, with a slide on
    /// screen and one next, then with neither: each item's role, words and fill, in the order
    /// drawn. Taken from that version's `slide`; the time and the timer's count stand as TIME
    /// and TIMER.
    const SHOWN_BEFORE: &str = r#"Words and Next: clock=TIME/None | timer=Some("Sermon  TIMER")/None | now=Some("One\nОдин")/None | next-label=Some("NEXT")/None | next=Some("Two")/None | stage-message=Some("Wrap up")/Some(Rgba { r: 190, g: 30, b: 30, a: 255 })
Words and Next: clock=TIME/None | timer=Some("Sermon  TIMER")/None | stage-message=Some("Wrap up")/Some(Rgba { r: 190, g: 30, b: 30, a: 255 })
Words Only: clock=TIME/None | now=Some("One\nОдин")/None | stage-message=Some("Wrap up")/Some(Rgba { r: 190, g: 30, b: 30, a: 255 })
Words Only: clock=TIME/None | stage-message=Some("Wrap up")/Some(Rgba { r: 190, g: 30, b: 30, a: 255 })
Words and Notes: clock=TIME/None | now=Some("One\nОдин")/None | notes=Some("Key change")/None | group=Some("Chorus")/None | stage-message=Some("Wrap up")/Some(Rgba { r: 190, g: 30, b: 30, a: 255 })
Words and Notes: clock=TIME/None | group=Some("Chorus")/None | stage-message=Some("Wrap up")/Some(Rgba { r: 190, g: 30, b: 30, a: 255 })
Clock and Timer: clock=TIME/None | timer=Some("Sermon  TIMER")/None | song-title=Some("Song")/None | stage-message=Some("Wrap up")/Some(Rgba { r: 190, g: 30, b: 30, a: 255 })
Clock and Timer: clock=TIME/None | timer=Some("Sermon  TIMER")/None | song-title=Some("Song")/None | stage-message=Some("Wrap up")/Some(Rgba { r: 190, g: 30, b: 30, a: 255 })
Band: clock=TIME/None | timer=Some("Sermon  TIMER")/None | now=Some("One\nОдин")/None | notes=Some("Key change")/None | next-label=Some("NEXT")/None | next=Some("Two")/None | stage-message=Some("Wrap up")/Some(Rgba { r: 190, g: 30, b: 30, a: 255 })
Band: clock=TIME/None | timer=Some("Sermon  TIMER")/None | stage-message=Some("Wrap up")/Some(Rgba { r: 190, g: 30, b: 30, a: 255 })"#;

    fn song() -> sp_model::Presentation {
        let mut song = sp_model::parse_song_text("S", "[Chorus]\nOne\n--\nОдин\n\nTwo\n");
        song.groups[0].slides[0].notes = Some("Key change".to_owned());
        song
    }

    fn overlay() -> crate::output::Overlay {
        let mut timer = crate::timers::Timer::new("Sermon".to_owned());
        timer.start(std::time::Instant::now());
        crate::output::Overlay { timers: vec![timer], stage_message: Some("Wrap up".to_owned()), ..Default::default() }
    }

    fn about() -> About {
        About { title: Some("Song".to_owned()), group: Some("Chorus".to_owned()) }
    }

    /// One line of what a stage slide shows, as `SHOWN_BEFORE` has it.
    fn line(name: &str, shown: &Slide) -> String {
        let parts: Vec<String> = shown
            .objects
            .iter()
            .filter(|object| item_of(object).is_some() && !object.hidden)
            .map(|object| {
                let role = &object.role.as_ref().expect("an item").0;
                let words = match object.as_text() {
                    Some(text) if role == CLOCK && text.contains(':') => "TIME".to_owned(),
                    Some(text) if role == TIMER => format!("{:?}", Some(text.replace("00:05:00", "TIMER"))),
                    other => format!("{other:?}"),
                };
                format!("{role}={words}/{:?}", object.fill)
            })
            .collect();
        format!("{name}: {}", parts.join(" | "))
    }

    #[test]
    fn layouts_saved_before_items_were_objects_open_and_look_the_same() {
        let saved: Vec<ThemeTemplate> = serde_json::from_str(SAVED_BEFORE).unwrap();
        let loaded = read_layouts(SAVED_BEFORE).expect("they read");
        assert_eq!(loaded.len(), saved.len());
        let song = song();
        let (now, next) = (song.slide(0).unwrap().1, song.slide(1).unwrap().1);
        let mut lines = Vec::new();
        for (was, layout) in saved.iter().zip(&loaded) {
            // Nothing the file had is lost or changed: its boxes, its other objects, its id.
            assert_eq!((&layout.id, &layout.name, &layout.boxes), (&was.id, &was.name, &was.boxes));
            assert_eq!(&layout.objects[..was.objects.len()], &was.objects[..]);
            // Each item is an object in its box's place, with its box's whole look.
            let theme = theme_for(layout);
            for style in &was.boxes {
                let made: Vec<&Object> =
                    layout.objects.iter().filter(|o| o.role.as_ref() == Some(&style.role)).collect();
                assert_eq!(made.len(), 1, "{}", style.role.0);
                assert_eq!(made[0].frame, Some(style.rect));
                let drawn = theme.text_style(made[0].role.as_ref(), made[0].text_style().unwrap()).unwrap();
                assert_eq!(&drawn, style, "{} is drawn with its box's look", style.role.0);
            }
            for (on, after) in [(Some(now), Some(next)), (None, None)] {
                let shown = slide(on, after, &overlay(), &about(), layout);
                // The layout's own shape is drawn behind every item, as it was.
                if let Some(shape) = was.objects.first() {
                    assert_eq!(shown.objects[0], *shape);
                }
                lines.push(line(&layout.name, &shown));
            }
        }
        // Read again, they are the same: the ids made for the items stay put.
        assert_eq!(read_layouts(SAVED_BEFORE).unwrap(), loaded);
        assert_eq!(lines.join("\n"), SHOWN_BEFORE);
        // The built-in layouts are those, with their items as objects, and since then set in the
        // default typeface (the file names the one they were set in, and keeps it above).
        for (made, was) in builtin_layouts().iter().zip(&saved) {
            let mut was = was.clone();
            was.boxes.iter_mut().for_each(|style| style.font_family = sp_model::DEFAULT_FONT_FAMILY.to_owned());
            assert_eq!(made.boxes, was.boxes);
            assert_eq!(normalized(&was), *made);
        }
    }

    /// A draft of a layout, as the Edit view makes one, and the theme it works with.
    fn draft_of(layout: &ThemeTemplate) -> (EditDraft, Theme) {
        let mut draft = EditDraft::of_slide(as_slide(layout), 0);
        draft.at.presentation = STAGE_SLOT;
        draft.keeps_roles = true;
        (draft, editor_theme(layout))
    }

    fn object<'a>(layout: &'a ThemeTemplate, role: &str) -> &'a Object {
        layout.objects.iter().find(|o| o.role.as_ref().is_some_and(|r| r.0 == role)).unwrap()
    }

    fn drawn_look(layout: &ThemeTemplate, role: &str) -> BoxStyle {
        let object = object(layout, role);
        theme_for(layout).text_style(object.role.as_ref(), object.text_style().unwrap()).unwrap()
    }

    #[test]
    fn stage_objects_move_resize_line_up_restack_and_undo_as_slide_objects_do() {
        let first = builtin_layouts().remove(0);
        let mut layout = first.clone();
        // Taken into the editor and straight back, a layout is unchanged.
        let (mut draft, theme) = draft_of(&layout);
        assert_eq!(with_slide(&layout, &draft.slide), layout);
        let clock = object(&layout, CLOCK).clone();
        let frame = clock.frame.unwrap();
        let look_before = drawn_look(&layout, CLOCK);
        // A press on the clock picks it; a drag moves it, as one undo step.
        let centre = (frame.x + frame.w / 2.0, frame.y + frame.h / 2.0);
        draft.press(&theme, centre, 8.0, false);
        assert_eq!(draft.selected, Some(clock.id));
        draft.drag_to((centre.0 - 300.0, centre.1 + 200.0), false, false);
        draft.release();
        layout = with_slide(&layout, &draft.slide);
        let moved = SlideRect { x: frame.x - 300.0, y: frame.y + 200.0, ..frame };
        assert_eq!(object(&layout, CLOCK).frame, Some(moved));
        assert_eq!(layout.style_for(&Role(CLOCK.to_owned())).unwrap().rect, moved, "its box follows it");
        // Its bottom-right handle resizes it.
        let theme = editor_theme(&layout);
        let corner = (moved.x + moved.w, moved.y + moved.h);
        draft.press(&theme, corner, 8.0, false);
        draft.drag_to((corner.0 + 40.0, corner.1 + 10.0), false, false);
        draft.release();
        layout = with_slide(&layout, &draft.slide);
        assert_eq!(object(&layout, CLOCK).frame.map(|f| (f.w, f.h)), Some((moved.w + 40.0, moved.h + 10.0)));
        // Lined up with the stage's right edge.
        assert!(draft.align_to_slide(&theme, 2));
        layout = with_slide(&layout, &draft.slide);
        let lined = object(&layout, CLOCK).frame.unwrap();
        assert_eq!(lined.x + lined.w, sp_model::SLIDE_WIDTH);
        // Brought to the front (it starts at the back): drawn last on the stage.
        assert_eq!(layout.objects[0].id, clock.id);
        let before = draft.slide.clone();
        assert!(draft.move_to_end_of_stack(true));
        draft.record_slide(before);
        layout = with_slide(&layout, &draft.slide);
        assert_eq!(layout.objects.last().map(|o| o.id), Some(clock.id));
        let shown = slide(None, None, &crate::output::Overlay::default(), &About::default(), &layout);
        assert_eq!(shown.objects.last().map(|o| o.id), Some(clock.id));
        // Made smaller and green: the stage draws it so, and its box follows.
        let before = draft.slide.clone();
        assert!(draft.restyle(&theme, |look| {
            look.size = 40.0;
            look.fill = sp_model::Rgba::rgb(0, 200, 0);
        }));
        draft.record_slide(before);
        layout = with_slide(&layout, &draft.slide);
        let drawn = drawn_look(&layout, CLOCK);
        assert_eq!((drawn.size, drawn.fill), (40.0, sp_model::Rgba::rgb(0, 200, 0)));
        assert_eq!(layout.style_for(&Role(CLOCK.to_owned())).unwrap().size, 40.0, "its box follows it");
        // Undo goes back a step at a time, the look included, to the layout as it was.
        while draft.undo() {
            layout = with_slide(&layout, &draft.slide);
        }
        assert_eq!(object(&layout, CLOCK).frame, Some(frame));
        assert_eq!(drawn_look(&layout, CLOCK), look_before);
        assert_eq!(layout, first, "everything is as it was");
    }

    #[test]
    fn stage_objects_are_duplicated_hidden_locked_and_deleted() {
        let layout = builtin_layouts().remove(0);
        let (mut draft, theme) = draft_of(&layout);
        let clock = object(&layout, CLOCK).id;
        // A copy of the clock is a second clock: both show the time.
        draft.select_only(Some(clock));
        let copies = draft.copied(&theme);
        draft.paste(&copies);
        let layout = with_slide(&layout, &draft.slide);
        let overlay = crate::output::Overlay::default();
        let shown = slide(None, None, &overlay, &About::default(), &layout);
        let clocks: Vec<&Object> = shown.objects.iter().filter(|o| o.role == Some(Role(CLOCK.to_owned()))).collect();
        assert_eq!(clocks.len(), 2);
        assert!(clocks.iter().all(|o| o.as_text().is_some_and(|t| t.contains(':'))));
        assert_eq!(layout.boxes.iter().filter(|b| b.role.0 == CLOCK).count(), 1, "one box, for older versions");
        // A hidden one is not drawn; a locked one is passed over by a click.
        let copy = draft.selected.unwrap();
        draft.slide.objects.iter_mut().find(|o| o.id == copy).unwrap().hidden = true;
        let layout = with_slide(&layout, &draft.slide);
        assert_eq!(
            slide(None, None, &overlay, &About::default(), &layout).objects.iter().filter(|o| !o.hidden).count(),
            1
        );
        let place = object(&layout, CLOCK).frame.unwrap();
        draft.slide.objects.iter_mut().find(|o| o.id == clock).unwrap().locked = true;
        assert_ne!(crate::editor::object_at(&draft.slide, &theme, (place.x + 5.0, place.y + 5.0)), Some(clock));
        // Deleting every clock takes its box too, so an older version shows none either.
        draft.slide.objects.retain(|o| o.role != Some(Role(CLOCK.to_owned())));
        let layout = with_slide(&layout, &draft.slide);
        assert!(layout.style_for(&Role(CLOCK.to_owned())).is_none());
        assert!(slide(None, None, &overlay, &About::default(), &layout).objects.iter().all(|o| o.hidden));
    }

    #[test]
    fn items_show_the_timer_clock_and_notes_they_are_set_to() {
        let mut layout = builtin_layouts().remove(2);
        let sermon = crate::timers::Timer::new("Sermon".to_owned());
        let mut walk_in = crate::timers::Timer::new("Walk in".to_owned());
        walk_in.start(std::time::Instant::now());
        let overlay = crate::output::Overlay { timers: vec![walk_in.clone(), sermon.clone()], ..Default::default() };
        let mut song = song();
        song.groups[0].slides[0].notes = Some("First\nSecond\nThird".to_owned());
        let now = song.slide(0).unwrap().1;
        let text = |layout: &ThemeTemplate, role: &str| {
            slide(Some(now), None, &overlay, &About::default(), layout)
                .objects
                .iter()
                .find(|o| o.role == Some(Role(role.to_owned())))
                .and_then(|o| o.as_text().map(str::to_owned))
        };
        // A timer item: the first running one, or the one it names, running or not.
        layout.objects.push(crate::stage_editor::item_object(TIMER));
        assert!(text(&layout, TIMER).is_some_and(|t| t.starts_with("Walk in  ")));
        layout.objects.last_mut().unwrap().shows.timer = Some(sermon.id);
        assert!(text(&layout, TIMER).is_some_and(|t| t.starts_with("Sermon  ")));
        // Notes: all of them, or the first lines.
        assert_eq!(text(&layout, NOTES).as_deref(), Some("First\nSecond\nThird"));
        layout.objects.iter_mut().find(|o| o.role == Some(Role(NOTES.to_owned()))).unwrap().shows.lines = 2;
        assert_eq!(text(&layout, NOTES).as_deref(), Some("First\nSecond"));
        // The clock: twelve hours without seconds unless it says otherwise.
        let clock = text(&layout, CLOCK).unwrap();
        assert_eq!(clock.matches(':').count(), 1);
        let item = layout.objects.iter_mut().find(|o| o.role == Some(Role(CLOCK.to_owned()))).unwrap();
        item.shows.hours_24 = true;
        item.shows.seconds = true;
        let clock = text(&layout, CLOCK).unwrap();
        assert_eq!((clock.len(), clock.matches(':').count()), (8, 2), "{clock}");
        // Saved and read again, the settings stay; an object that has none writes none.
        let read: ThemeTemplate = serde_json::from_str(&serde_json::to_string(&layout).unwrap()).unwrap();
        assert_eq!(read, layout);
        assert!(!serde_json::to_string(&builtin_layouts()).unwrap().contains("\"shows\""));
    }

    #[test]
    fn a_screen_picture_shows_an_audience_screen_and_falls_back_to_the_first() {
        let main = Screen::new("Main".to_owned(), Output::Automatic);
        let hdmi = Screen::new("HDMI main".to_owned(), Output::Window);
        let band = Screen::stage("Band".to_owned(), Output::Window);
        let screens = vec![band.clone(), main.clone(), hdmi.clone()];
        // The main one is the first audience screen, wherever it is in the list.
        assert_eq!(source_screen(None, &screens), (Some(main.id), false));
        assert_eq!(source_screen(Some(hdmi.id), &screens), (Some(hdmi.id), false));
        // A screen that has gone, or is now a stage screen, falls back to the main one, and says so.
        assert_eq!(source_screen(Some(sp_model::Uuid::new_v4()), &screens), (Some(main.id), true));
        assert_eq!(source_screen(Some(band.id), &screens), (Some(main.id), true));
        let mut changed = screens.clone();
        changed[2].kind = crate::screens::Kind::Stage;
        assert_eq!(source_screen(Some(hdmi.id), &changed), (Some(main.id), true));
        // Renamed, it is still the one shown: it is kept by its id.
        let mut renamed = screens.clone();
        renamed[2].name = "Projector".to_owned();
        assert_eq!(source_screen(Some(hdmi.id), &renamed), (Some(hdmi.id), false));
        // With no audience screen at all there is nothing to show.
        assert_eq!(source_screen(None, std::slice::from_ref(&band)), (None, false));
        // The screens' thread is told the screen each picture shows, never a stage screen.
        let mut layout = builtin_layouts().remove(0);
        for wanted in [None, Some(hdmi.id), Some(band.id), Some(sp_model::Uuid::new_v4())] {
            layout.objects.push(Object::screen(wanted));
        }
        let sent = with_sources(&layout, &screens);
        assert_eq!(sources(&sent), [main.id, hdmi.id]);
        assert!(!sources(&with_sources(&layout, &changed)).contains(&hdmi.id));
        assert_eq!(sources(&with_sources(&layout, std::slice::from_ref(&band))), [sp_model::Uuid::nil()]);
        // The layout keeps what was chosen: only the copy sent is told.
        assert!(matches!(layout.objects.last().unwrap().kind, ObjectKind::Screen { screen: Some(_), .. }));
        assert!(matches!(layout.objects[layout.objects.len() - 4].kind, ObjectKind::Screen { screen: None, .. }));
        // On the stage it is drawn where it is in the layout, in front of what comes before it.
        let shown = slide(None, None, &crate::output::Overlay::default(), &About::default(), &sent);
        assert!(matches!(shown.objects.last().unwrap().kind, ObjectKind::Screen { .. }));
        // The editor's Add menu offers only audience screens.
        let state = crate::home::HomeState { screens, ..Default::default() };
        let offered = crate::stage_editor::audience_screens(&state);
        assert_eq!(offered.iter().map(|(id, _)| *id).collect::<Vec<_>>(), [main.id, hdmi.id]);
    }

    #[test]
    fn the_command_line_says_what_objects_show_and_moves_them() {
        let main = Screen::new("Main".to_owned(), Output::Automatic);
        let hdmi = Screen::new("HDMI main".to_owned(), Output::Window);
        let screens = vec![main.clone(), hdmi.clone()];
        let mut layout = builtin_layouts().remove(0);
        layout.objects.push(Object::screen(Some(hdmi.id)));
        layout.objects.push(Object::screen(None));
        let said: Vec<String> = objects(&layout).iter().map(|o| describe(o, &screens)).collect();
        assert_eq!(said[0], "shows clock");
        assert_eq!(said[said.len() - 2], "what HDMI main shows");
        assert_eq!(said[said.len() - 1], "what the main audience screen (Main) shows");
        let gone = describe(&layout.objects[layout.objects.len() - 2], std::slice::from_ref(&main));
        assert_eq!(gone, "what the main audience screen (Main) shows, as the screen it named has gone");
        // Moving the clock moves its box with it; moving a screen picture moves only it.
        let place = SlideRect { x: 10.0, y: 20.0, w: 300.0, h: 100.0 };
        let moved = with_frame(&layout, 0, place);
        assert_eq!(moved.objects[0].frame, Some(place));
        assert_eq!(moved.style_for(&Role(CLOCK.to_owned())).unwrap().rect, place);
        let last = moved.objects.len() - 1;
        let moved = with_frame(&moved, last, place);
        assert_eq!(moved.objects[last].frame, Some(place));
        assert_eq!(moved.boxes, with_frame(&layout, 0, place).boxes);
    }
}
