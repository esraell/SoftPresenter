//! What the Edit view does to a slide, kept apart from how it is drawn: finding the object
//! under the pointer, moving and resizing it, adding and deleting objects, and undo.

use sp_engine::{Show, SlideRef};
use sp_model::{Object, ObjectKind, SLIDE_HEIGHT, SLIDE_WIDTH, Slide, SlideRect, TextStyle, Theme, Uuid};
use sp_render::object_frame;

use crate::i18n::{t, tg, tgf, tk};

/// An object cannot be made smaller than this on either side, in slide units.
pub const MIN_SIDE: f32 = 20.0;
/// How many changes can be undone.
const UNDO_LIMIT: usize = 200;

/// One of the eight places an object can be resized from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handle {
    TopLeft,
    Top,
    TopRight,
    Right,
    BottomRight,
    Bottom,
    BottomLeft,
    Left,
}

impl Handle {
    pub const ALL: [Handle; 8] = [
        Handle::TopLeft,
        Handle::Top,
        Handle::TopRight,
        Handle::Right,
        Handle::BottomRight,
        Handle::Bottom,
        Handle::BottomLeft,
        Handle::Left,
    ];

    /// Where the handle sits on a frame, in slide units.
    pub fn position(self, frame: SlideRect) -> (f32, f32) {
        let (left, right, mid_x) = (frame.x, frame.x + frame.w, frame.x + frame.w / 2.0);
        let (top, bottom, mid_y) = (frame.y, frame.y + frame.h, frame.y + frame.h / 2.0);
        match self {
            Handle::TopLeft => (left, top),
            Handle::Top => (mid_x, top),
            Handle::TopRight => (right, top),
            Handle::Right => (right, mid_y),
            Handle::BottomRight => (right, bottom),
            Handle::Bottom => (mid_x, bottom),
            Handle::BottomLeft => (left, bottom),
            Handle::Left => (left, mid_y),
        }
    }

    fn moves_left(self) -> bool {
        matches!(self, Handle::TopLeft | Handle::Left | Handle::BottomLeft)
    }

    fn moves_right(self) -> bool {
        matches!(self, Handle::TopRight | Handle::Right | Handle::BottomRight)
    }

    fn moves_top(self) -> bool {
        matches!(self, Handle::TopLeft | Handle::Top | Handle::TopRight)
    }

    fn moves_bottom(self) -> bool {
        matches!(self, Handle::BottomLeft | Handle::Bottom | Handle::BottomRight)
    }
}

/// The handle of a frame within `reach` of a point, if any.
/// The handle a press is on: the nearest within reach, so on a small box, where handles
/// crowd together, the one meant is the one taken.
pub fn handle_at(frame: SlideRect, point: (f32, f32), reach: f32) -> Option<Handle> {
    let distance = |handle: &Handle| {
        let (x, y) = handle.position(frame);
        (point.0 - x).abs().max((point.1 - y).abs())
    };
    Handle::ALL
        .into_iter()
        .filter(|handle| distance(handle) <= reach)
        .min_by(|a, b| distance(a).total_cmp(&distance(b)))
}

/// A frame moved by a distance.
pub fn moved(frame: SlideRect, by: (f32, f32)) -> SlideRect {
    SlideRect { x: frame.x + by.0, y: frame.y + by.1, ..frame }
}

/// A frame resized by dragging one handle a distance. The opposite side stays where it is,
/// and the frame never gets smaller than [`MIN_SIDE`].
/// A resized frame brought back to the proportions it started with, keeping the corner
/// opposite the handle where it is. Only corner handles do this; sides resize freely.
pub fn same_shape(from: SlideRect, handle: Handle, free: SlideRect) -> SlideRect {
    let corner = (handle.moves_left() || handle.moves_right()) && (handle.moves_top() || handle.moves_bottom());
    if !corner || from.w <= 0.0 || from.h <= 0.0 {
        return free;
    }
    // Follow whichever side was dragged further, in proportion.
    let scale = (free.w / from.w).max(free.h / from.h);
    let (w, h) = ((from.w * scale).max(MIN_SIDE), (from.h * scale).max(MIN_SIDE));
    let x = if handle.moves_left() { free.x + free.w - w } else { free.x };
    let y = if handle.moves_top() { free.y + free.h - h } else { free.y };
    SlideRect { x, y, w, h }
}

pub fn resized(frame: SlideRect, handle: Handle, by: (f32, f32)) -> SlideRect {
    let (mut left, mut top) = (frame.x, frame.y);
    let (mut right, mut bottom) = (frame.x + frame.w, frame.y + frame.h);
    if handle.moves_left() {
        left = (left + by.0).min(right - MIN_SIDE);
    }
    if handle.moves_right() {
        right = (right + by.0).max(left + MIN_SIDE);
    }
    if handle.moves_top() {
        top = (top + by.1).min(bottom - MIN_SIDE);
    }
    if handle.moves_bottom() {
        bottom = (bottom + by.1).max(top + MIN_SIDE);
    }
    SlideRect { x: left, y: top, w: right - left, h: bottom - top }
}

/// True when a point is inside a frame that has been turned about its centre.
fn contains(frame: SlideRect, rotation: f32, point: (f32, f32)) -> bool {
    let (cx, cy) = (frame.x + frame.w / 2.0, frame.y + frame.h / 2.0);
    // Turn the point back by the object's rotation and test against the upright frame.
    let (sin, cos) = (-rotation).to_radians().sin_cos();
    let (dx, dy) = (point.0 - cx, point.1 - cy);
    let (x, y) = (cx + dx * cos - dy * sin, cy + dx * sin + dy * cos);
    x >= frame.x && x <= frame.x + frame.w && y >= frame.y && y <= frame.y + frame.h
}

/// A text box, which on a theme's layout is one of its boxes for the words.
fn is_text(object: &Object) -> bool {
    matches!(object.kind, ObjectKind::Text { .. })
}

/// The topmost object under a point, in slide units.
pub fn object_at(slide: &Slide, theme: &Theme, point: (f32, f32)) -> Option<Uuid> {
    // Clicks pass through objects that are hidden or locked to whatever is behind them.
    slide
        .objects
        .iter()
        .rev()
        .filter(|o| !o.hidden && !o.locked)
        .find(|o| contains(object_frame(slide, o, theme), o.rotation, point))
        .map(|o| o.id)
}

/// A line across the slide that a dragged object is lined up with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Guide {
    /// A line down the slide at this x.
    Down(f32),
    /// A line across the slide at this y.
    Across(f32),
}

/// The safe edge: the margin kept clear round the slide, in slide units (a twentieth of its
/// width and height). Text placed inside it is never tight against the edge of a screen.
pub const SAFE_EDGE: (f32, f32) = (SLIDE_WIDTH * 0.05, SLIDE_HEIGHT * 0.05);

/// How near, in slide units, a dragged object must come to a line to be pulled onto it.
const SNAP: f32 = 14.0;

/// How far apart the grid's lines are, in slide units: 16 across and 9 down.
pub const GRID: f32 = 120.0;

/// The lines a box can be lined up with: the slide's edges and middle, the safe edge inside
/// them, and the edges and middles of the other objects. Down-lines (x) and across-lines (y).
fn lines(others: &[SlideRect], grid: bool) -> (Vec<f32>, Vec<f32>) {
    let mut down = vec![0.0, SAFE_EDGE.0, SLIDE_WIDTH / 2.0, SLIDE_WIDTH - SAFE_EDGE.0, SLIDE_WIDTH];
    let mut across = vec![0.0, SAFE_EDGE.1, SLIDE_HEIGHT / 2.0, SLIDE_HEIGHT - SAFE_EDGE.1, SLIDE_HEIGHT];
    // With the grid showing, its lines pull too.
    if grid {
        let steps = |size: f32| (1..(size / GRID) as usize).map(|n| n as f32 * GRID).collect::<Vec<f32>>();
        down.extend(steps(SLIDE_WIDTH));
        across.extend(steps(SLIDE_HEIGHT));
    }
    for other in others {
        down.extend([other.x, other.x + other.w / 2.0, other.x + other.w]);
        across.extend([other.y, other.y + other.h / 2.0, other.y + other.h]);
    }
    (down, across)
}

/// The line nearest a position, if one is near enough to pull onto.
fn near(lines: &[f32], at: f32) -> Option<f32> {
    lines
        .iter()
        .copied()
        .filter(|line| (line - at).abs() <= SNAP)
        .min_by(|a, b| (a - at).abs().total_cmp(&(b - at).abs()))
}

/// Pulls a frame being moved onto nearby lines. Returns the frame, moved if it was near one,
/// and the lines it now sits on.
#[cfg(test)]
pub fn snapped(frame: SlideRect, others: &[SlideRect]) -> (SlideRect, Vec<Guide>) {
    snapped_on(frame, others, false)
}

/// As `snapped`, with the grid's lines pulling too when `grid` is true.
pub fn snapped_on(frame: SlideRect, others: &[SlideRect], grid: bool) -> (SlideRect, Vec<Guide>) {
    let (down, across) = lines(others, grid);
    // The nearest line to any of the frame's own three (start, middle, end) along one way.
    let nearest = |lines: &[f32], start: f32, size: f32| -> Option<(f32, f32)> {
        [start, start + size / 2.0, start + size]
            .into_iter()
            .filter_map(|own| near(lines, own).map(|line| (line - own, line)))
            .min_by(|a, b| a.0.abs().total_cmp(&b.0.abs()))
    };
    let mut frame = frame;
    let mut guides = Vec::new();
    if let Some((by, line)) = nearest(&down, frame.x, frame.w) {
        frame.x += by;
        guides.push(Guide::Down(line));
    }
    if let Some((by, line)) = nearest(&across, frame.y, frame.h) {
        frame.y += by;
        guides.push(Guide::Across(line));
    }
    (frame, guides)
}

/// Pulls the edges a handle is dragging onto nearby lines, leaving the other edges where they
/// are. Returns the frame and the lines its dragged edges now sit on.
#[cfg(test)]
pub fn snapped_resize(frame: SlideRect, handle: Handle, others: &[SlideRect]) -> (SlideRect, Vec<Guide>) {
    snapped_resize_on(frame, handle, others, false)
}

/// As `snapped_resize`, with the grid's lines pulling too when `grid` is true.
pub fn snapped_resize_on(
    frame: SlideRect,
    handle: Handle,
    others: &[SlideRect],
    grid: bool,
) -> (SlideRect, Vec<Guide>) {
    let (down, across) = lines(others, grid);
    let (mut left, mut top) = (frame.x, frame.y);
    let (mut right, mut bottom) = (frame.x + frame.w, frame.y + frame.h);
    let mut guides = Vec::new();
    if handle.moves_left()
        && let Some(line) = near(&down, left).filter(|line| right - line >= MIN_SIDE)
    {
        left = line;
        guides.push(Guide::Down(line));
    }
    if handle.moves_right()
        && let Some(line) = near(&down, right).filter(|line| line - left >= MIN_SIDE)
    {
        right = line;
        guides.push(Guide::Down(line));
    }
    if handle.moves_top()
        && let Some(line) = near(&across, top).filter(|line| bottom - line >= MIN_SIDE)
    {
        top = line;
        guides.push(Guide::Across(line));
    }
    if handle.moves_bottom()
        && let Some(line) = near(&across, bottom).filter(|line| line - top >= MIN_SIDE)
    {
        bottom = line;
        guides.push(Guide::Across(line));
    }
    (SlideRect { x: left, y: top, w: right - left, h: bottom - top }, guides)
}

/// What a drag on the canvas is doing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Drag {
    Move {
        id: Uuid,
        from: SlideRect,
        start: (f32, f32),
    },
    Resize {
        id: Uuid,
        handle: Handle,
        from: SlideRect,
        start: (f32, f32),
    },
    /// Dragging out a rectangle from empty slide: everything it touches is selected.
    Select {
        start: (f32, f32),
    },
}

/// What a shape is called (its key and its English), for the Add menus and a new shape's name.
fn shape_words(shape: sp_model::Shape) -> (&'static str, &'static str) {
    use sp_model::Shape;
    match shape {
        Shape::RoundedRect => tk("editor.shape.rounded_rect", "Rounded Rect"),
        Shape::Rectangle => tk("editor.shape.rectangle", "Rectangle"),
        Shape::Ellipse => tk("editor.shape.ellipse", "Ellipse"),
        Shape::Triangle => tk("editor.shape.triangle", "Triangle"),
        Shape::RightTriangle => tk("editor.shape.right_triangle", "Right Triangle"),
        Shape::Rhombus => tk("editor.shape.rhombus", "Rhombus"),
        Shape::Polygon => tk("editor.shape.polygon", "Polygon"),
        Shape::Star => tk("editor.shape.star", "Star"),
        Shape::DoubleArrow => tk("editor.shape.double_arrow", "Double Arrow"),
        Shape::Arrow => tk("editor.shape.arrow", "Arrow"),
    }
}

/// A shape's name in the language set, for the Add menus (sp-model keeps the English one).
pub fn shape_name(shape: sp_model::Shape) -> &'static str {
    let (key, english) = shape_words(shape);
    t(key, english)
}

/// The words a new text box says, and a template's boxes in the Theme Editor: sample text, never
/// a song's words. Given in the language set when the box is made, and kept as given.
const SAMPLE_WORDS: (&str, &str) = tk("editor.sample_text", "Text");

/// What a new text box is named: "Text 2", numbered by its place on the slide.
const NEW_TEXT_NAME: (&str, &str) = tk("editor.new_text.name", "Text {number}");

/// The sample words in the language set (see [`SAMPLE_WORDS`]): what a new text box says, and
/// what a template's boxes say while it is edited (they keep no words of their own).
pub fn sample_words() -> &'static str {
    crate::i18n::tg(SAMPLE_WORDS.0, SAMPLE_WORDS.1)
}

/// Whether a box's words are still the app's sample words, in whichever language the app was in
/// when it gave them ("Text", "Texto"), rather than words of its own.
pub fn is_sample_words(text: &str) -> bool {
    crate::i18n::every_wording(SAMPLE_WORDS).any(|sample| sample == text)
}

/// Whether `name` is the name the app gave a new text box ("Text 2", "Texto 2"), in any of its
/// languages, rather than one a church gave it.
pub fn is_new_text_name(name: &str) -> bool {
    crate::i18n::every_wording(NEW_TEXT_NAME).any(|numbered| {
        numbered.split_once("{number}").is_some_and(|(before, after)| {
            name.strip_prefix(before)
                .and_then(|rest| rest.strip_suffix(after))
                .is_some_and(|number| !number.is_empty() && number.chars().all(|c| c.is_ascii_digit()))
        })
    })
}

/// What a box's role is called in the window: the built-in roles (the main language, the
/// translation, a Bible verse's reference and its translation's) in the language set, any other
/// by its own name. sp-model keeps the role itself, and its English label for files and the
/// command line ([`sp_model::Role::label`]).
pub fn role_label(role: &sp_model::Role) -> String {
    use sp_model::Role;
    let words = if *role == Role::primary() {
        t("editor.role.main", "Main")
    } else if *role == Role::translation() {
        t("editor.role.translation", "Translation")
    } else if *role == Role::reference() {
        t("editor.role.reference", "Reference")
    } else if *role == Role::translation_reference() {
        t("editor.role.translation_reference", "Translation reference")
    } else {
        return role.label();
    };
    words.to_owned()
}

/// What an object is called where its name is shown as it is (the Theme Editor's and stage
/// layouts' Objects lists, a name being retyped, the Shape tab's Name): a box still named after
/// its role, as a template's boxes are ([`sp_model::ThemeTemplate::as_slide`]), is called what
/// its role is called in the language set.
pub fn object_name(object: &sp_model::Object) -> String {
    match &object.role {
        Some(role) if object.name == role.label() => role_label(role),
        _ => object.name.clone(),
    }
}

/// The name an object is given when `typed` is typed for it: typed as its role is called in the
/// window, it keeps its role's own label, which tells a template the box has no name of its
/// own ([`sp_model::ThemeTemplate::with_slide`]), so no translation is ever saved as a name.
pub fn typed_object_name(object: &sp_model::Object, typed: &str) -> String {
    match &object.role {
        Some(role) if typed == role_label(role) => role.label(),
        _ => typed.to_owned(),
    }
}

/// A number for each draft made, so a change of draft can be told from the same one.
fn next_draft_serial() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// The slide being edited: a working copy, what is selected, and the changes that can be undone.
#[derive(Clone, Debug)]
pub struct EditDraft {
    /// Different for every draft made: opening another slide, or the same one again, makes a
    /// new one.
    pub serial: u64,
    pub at: SlideRef,
    pub slide: Slide,
    pub selected: Option<Uuid>,
    /// Other objects selected along with `selected`, by Shift or Command click. The first one
    /// selected stays the one the inspector shows.
    pub also: Vec<Uuid>,
    /// Where the other selected objects were when a drag began, so they move with it.
    drag_others: Vec<(Uuid, SlideRect)>,
    /// The rectangle being dragged out to select with, as two opposite corners, for the canvas
    /// to draw; and where every object was, and what was selected, when it began.
    pub marquee: Option<((f32, f32), (f32, f32))>,
    marquee_frames: Vec<(Uuid, SlideRect)>,
    marquee_kept: Vec<Uuid>,
    /// The lines a dragged object is lined up with right now, for the canvas to draw.
    pub guides: Vec<Guide>,
    /// True while the grid shows: its lines pull boxes being moved or resized.
    pub grid: bool,
    /// The object being renamed in the Objects list, and the name as typed so far.
    pub renaming: Option<(Uuid, String)>,
    /// The text object whose words are being typed on the slide itself, and whether that has
    /// only just begun (so its field can take the keyboard once).
    pub typing: Option<Uuid>,
    pub typing_started: bool,
    /// The characters selected in the field being typed in (start, end; start is before end),
    /// kept while the field has the keyboard, so a control in the inspector can style them
    /// after it has taken the keyboard away. Empty or none: no words are selected.
    pub typing_range: Option<(usize, usize)>,
    /// Set when the field being typed in should take the keyboard back, and the selection with
    /// it, as after a control in the inspector was used.
    pub typing_refocus: bool,
    /// The words and spans of the box when typing in it began, for Escape.
    pub typing_backup: Option<(String, Vec<sp_model::TextSpan>)>,
    pub drag: Option<Drag>,
    undo: Vec<Slide>,
    redo: Vec<Slide>,
    /// When the last change was recorded for undo, so a run of typing or nudging is one step.
    last_recorded: f64,
    /// Set when undo or redo has just changed the slide, so that change is not itself
    /// recorded as something to undo.
    pub stepped: bool,
    /// True where several objects may share a role, as on a stage layout (two clocks): a copy
    /// keeps its original's. Elsewhere a copy belongs to no theme box.
    pub keeps_roles: bool,
    /// True on a theme's layout ([`EditDraft::of_template`]): its shapes, pictures and videos are
    /// either behind all its text boxes or in front of them all, as its slides draw them (the
    /// words there are the slide's own), so they move past the text boxes all at once, and a text
    /// box stays among the text boxes.
    pub layered: bool,
}

/// What a look set for the whole box means for the words: a setting the controls changed from
/// what they showed is the box's now, and every word takes it.
pub fn apply_to_whole_box(
    spans: &mut Vec<sp_model::TextSpan>,
    length: usize,
    shown: &sp_model::BoxStyle,
    target: &sp_model::BoxStyle,
    look: &mut sp_model::BoxStyle,
) {
    use sp_model::clear_in_spans as clear;
    if target.font_family != shown.font_family {
        look.font_family.clone_from(&target.font_family);
        clear(spans, length, |s| s.font_family = None);
    }
    if target.size != shown.size {
        look.size = target.size;
        clear(spans, length, |s| s.size = None);
    }
    if target.weight != shown.weight {
        look.weight = target.weight;
        clear(spans, length, |s| s.weight = None);
    }
    if target.italic != shown.italic {
        look.italic = target.italic;
        clear(spans, length, |s| s.italic = None);
    }
    if target.underline != shown.underline {
        look.underline = target.underline;
        clear(spans, length, |s| s.underline = None);
    }
    if target.strike != shown.strike {
        look.strike = target.strike;
        clear(spans, length, |s| s.strike = None);
    }
    if target.fill != shown.fill {
        look.fill = target.fill;
        clear(spans, length, |s| s.fill = None);
    }
}

/// A change made to the whole box: the settings it changed are no longer set word by word, so
/// they are taken out of every span.
pub fn clear_spans_for_changes(
    spans: &mut Vec<sp_model::TextSpan>,
    length: usize,
    before: &sp_model::BoxStyle,
    after: &sp_model::BoxStyle,
) {
    use sp_model::clear_in_spans as clear;
    if spans.is_empty() {
        return;
    }
    if after.font_family != before.font_family {
        clear(spans, length, |s| s.font_family = None);
    }
    if after.size != before.size {
        clear(spans, length, |s| s.size = None);
    }
    if after.weight != before.weight {
        clear(spans, length, |s| s.weight = None);
    }
    if after.italic != before.italic {
        clear(spans, length, |s| s.italic = None);
    }
    if after.underline != before.underline {
        clear(spans, length, |s| s.underline = None);
    }
    if after.strike != before.strike {
        clear(spans, length, |s| s.strike = None);
    }
    if after.fill != before.fill {
        clear(spans, length, |s| s.fill = None);
    }
}

impl EditDraft {
    /// Starts editing a slide of the show.
    pub fn of(show: &Show, at: SlideRef) -> Option<EditDraft> {
        let (_, slide) = show.presentations().get(at.presentation)?.slide(at.slide)?;
        Some(EditDraft {
            at,
            slide: slide.clone(),
            selected: None,
            also: Vec::new(),
            drag_others: Vec::new(),
            marquee: None,
            marquee_frames: Vec::new(),
            marquee_kept: Vec::new(),
            guides: Vec::new(),
            grid: false,
            renaming: None,
            typing: None,
            typing_started: false,
            typing_range: None,
            typing_refocus: false,
            typing_backup: None,
            drag: None,
            undo: Vec::new(),
            serial: next_draft_serial(),
            redo: Vec::new(),
            last_recorded: f64::NEG_INFINITY,
            stepped: false,
            keeps_roles: false,
            layered: false,
        })
    }

    /// A draft of a slide that belongs to no song, such as one of a theme's layouts. `which`
    /// tells such drafts apart.
    pub fn of_slide(slide: Slide, which: usize) -> EditDraft {
        EditDraft {
            at: SlideRef { presentation: usize::MAX, slide: which },
            slide,
            selected: None,
            also: Vec::new(),
            drag_others: Vec::new(),
            marquee: None,
            marquee_frames: Vec::new(),
            marquee_kept: Vec::new(),
            guides: Vec::new(),
            grid: false,
            renaming: None,
            typing: None,
            typing_started: false,
            typing_range: None,
            typing_refocus: false,
            typing_backup: None,
            drag: None,
            undo: Vec::new(),
            serial: next_draft_serial(),
            redo: Vec::new(),
            last_recorded: f64::NEG_INFINITY,
            stepped: false,
            keeps_roles: false,
            layered: false,
        }
    }

    /// A draft of one of a theme's layouts, as the Theme Editor works on it (see `layered`).
    pub fn of_template(template: &sp_model::ThemeTemplate, which: usize) -> EditDraft {
        EditDraft { layered: true, ..EditDraft::of_slide(template.as_slide(sample_words()), which) }
    }

    pub fn selected_object(&self) -> Option<&Object> {
        let id = self.selected?;
        self.slide.objects.iter().find(|o| o.id == id)
    }

    pub fn selected_object_mut(&mut self) -> Option<&mut Object> {
        let id = self.selected?;
        self.slide.objects.iter_mut().find(|o| o.id == id)
    }

    /// Remembers the slide as it is now, so the change about to be made can be undone.
    pub fn record(&mut self) {
        self.record_slide(self.slide.clone());
    }

    /// Remembers an earlier state of the slide as one undo step.
    pub fn record_slide(&mut self, before: Slide) {
        if self.undo.last() == Some(&before) {
            return;
        }
        self.undo.push(before);
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// Like [`record_slide`](Self::record_slide), but changes that follow each other within
    /// a second count as one step, so typing a word is undone as a word and not letter by letter.
    pub fn record_run(&mut self, before: Slide, now: f64) {
        if now - self.last_recorded > 1.0 {
            self.record_slide(before);
        }
        self.last_recorded = now;
    }

    /// How many steps the draft can undo.
    pub fn undo_depth(&self) -> usize {
        self.undo.len()
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Goes back one step. Returns true if anything changed.
    pub fn undo(&mut self) -> bool {
        let Some(earlier) = self.undo.pop() else { return false };
        self.redo.push(std::mem::replace(&mut self.slide, earlier));
        self.after_history();
        true
    }

    /// Goes forward one step again. Returns true if anything changed.
    pub fn redo(&mut self) -> bool {
        let Some(later) = self.redo.pop() else { return false };
        self.undo.push(std::mem::replace(&mut self.slide, later));
        self.after_history();
        true
    }

    fn after_history(&mut self) {
        self.stepped = true;
        self.drag = None;
        self.last_recorded = f64::NEG_INFINITY;
        if self.selected_object().is_none() {
            self.selected = None;
        }
    }

    /// Adds a text object in the middle of the slide, on top, and selects it. Its name and its
    /// sample words are given in the language set, and kept.
    pub fn add_text(&mut self) {
        self.record();
        // Three quarters of the slide, in the middle.
        let (w, h) = (SLIDE_WIDTH * 0.75, SLIDE_HEIGHT * 0.75);
        let number = self.slide.objects.len() + 1;
        let object = Object {
            id: Uuid::new_v4(),
            name: tgf(NEW_TEXT_NAME.0, NEW_TEXT_NAME.1, &[("number", &number.to_string())]),
            role: None,
            frame: Some(SlideRect { x: (SLIDE_WIDTH - w) / 2.0, y: (SLIDE_HEIGHT - h) / 2.0, w, h }),
            rotation: 0.0,
            opacity: 1.0,
            hidden: false,
            locked: false,
            fill: None,
            stroke: None,
            shadow: None,
            flip_h: false,
            flip_v: false,
            build_in: None,
            build_out: None,
            link: None,
            shows: sp_model::StageShows::default(),
            gradient: None,
            shown_paragraphs: None,
            kind: ObjectKind::Text { text: sample_words().to_owned(), style: TextStyle::default(), spans: Vec::new() },
        };
        self.selected = Some(object.id);
        self.slide.objects.push(object);
    }

    /// Adds a shape in the middle of the slide, in front of everything, and selects it.
    pub fn add_shape(&mut self, shape: sp_model::Shape) {
        self.record();
        let mut object = Object::shape(shape);
        let same = self
            .slide
            .objects
            .iter()
            .filter(|o| matches!(o.kind, ObjectKind::Shape { shape: s } if s == shape))
            .count();
        // Named in the language set, and kept: sp-model names it in English.
        let (key, english) = shape_words(shape);
        let named = tg(key, english);
        object.name = if same > 0 {
            tgf("editor.shape.numbered", "{shape} {n}", &[("shape", named), ("n", &(same + 1).to_string())])
        } else {
            named.to_owned()
        };
        self.selected = Some(object.id);
        self.also.clear();
        self.slide.objects.push(object);
    }

    /// Puts a picture on the slide, selected.
    pub fn add_image(&mut self, picture: &crate::images::Imported) {
        self.record();
        let object = Object::image(picture.file.clone(), picture.name.clone(), picture.width, picture.height);
        self.selected = Some(object.id);
        self.also.clear();
        self.slide.objects.push(object);
    }

    /// Puts an object on the slide, in front of everything, selected.
    pub fn add(&mut self, object: Object) {
        self.record();
        self.selected = Some(object.id);
        self.also.clear();
        self.slide.objects.push(object);
    }

    /// Puts a web page box on the slide, selected, its address to be typed.
    pub fn add_web(&mut self) {
        self.record();
        let same = self.slide.objects.iter().filter(|o| matches!(o.kind, ObjectKind::Web { .. })).count();
        let mut object = Object::web(String::new());
        object.name = if same > 0 {
            tgf("editor.web.numbered", "Web page {n}", &[("n", &(same + 1).to_string())])
        } else {
            tg("editor.web.name", "Web page").to_owned()
        };
        self.selected = Some(object.id);
        self.also.clear();
        self.slide.objects.push(object);
    }

    /// Puts an NDI source box on the slide, selected, its source to be chosen.
    pub fn add_ndi(&mut self) {
        self.record();
        let same = self.slide.objects.iter().filter(|o| matches!(o.kind, ObjectKind::Ndi { .. })).count();
        let mut object = Object::ndi(String::new());
        object.name = if same > 0 {
            tgf("editor.ndi.numbered", "NDI source {n}", &[("n", &(same + 1).to_string())])
        } else {
            tg("editor.ndi.name", "NDI source").to_owned()
        };
        self.selected = Some(object.id);
        self.also.clear();
        self.slide.objects.push(object);
    }

    /// Puts a Phone or iPad Screen box on the slide, selected: whoever is showing their screen
    /// to this computer shows in it.
    pub fn add_mirror(&mut self) {
        self.record();
        let same = self.slide.objects.iter().filter(|o| matches!(o.kind, ObjectKind::Mirror { .. })).count();
        let mut object = Object::mirror();
        object.name = if same > 0 {
            tgf("editor.mirror.numbered", "Phone or iPad Screen {n}", &[("n", &(same + 1).to_string())])
        } else {
            tg("editor.mirror.name", "Phone or iPad Screen").to_owned()
        };
        self.selected = Some(object.id);
        self.also.clear();
        self.slide.objects.push(object);
    }

    /// Every selected object: the first selected, then the rest.
    pub fn selection(&self) -> Vec<Uuid> {
        self.selected.into_iter().chain(self.also.iter().copied()).collect()
    }

    pub fn is_selected(&self, id: Uuid) -> bool {
        self.selected == Some(id) || self.also.contains(&id)
    }

    /// Selects one object and nothing else.
    pub fn select_only(&mut self, id: Option<Uuid>) {
        self.selected = id;
        self.also.clear();
    }

    /// Adds an object to the selection, or takes it out if it is already in. This is what a
    /// Shift or Command click does.
    pub fn toggle_selected(&mut self, id: Uuid) {
        if self.selected == Some(id) {
            // The next one selected becomes the one the inspector shows.
            self.selected = if self.also.is_empty() { None } else { Some(self.also.remove(0)) };
        } else if let Some(at) = self.also.iter().position(|have| *have == id) {
            self.also.remove(at);
        } else if self.selected.is_none() {
            self.selected = Some(id);
        } else {
            self.also.push(id);
        }
    }

    /// Selects every object on the slide that can be clicked.
    pub fn select_all(&mut self) {
        let mut ids = self.slide.objects.iter().filter(|o| !o.hidden && !o.locked).map(|o| o.id);
        self.selected = ids.next();
        self.also = ids.collect();
    }

    /// Removes every selected object. Returns true if there was one.
    pub fn delete_selected(&mut self) -> bool {
        let ids = self.selection();
        if !self.slide.objects.iter().any(|o| ids.contains(&o.id)) {
            return false;
        }
        self.record();
        self.slide.objects.retain(|o| !ids.contains(&o.id));
        self.select_only(None);
        self.drag = None;
        true
    }

    /// Copies of the selected objects, in their order on the slide, with their places written
    /// out so a copy lands where the original is even if the original followed the theme.
    pub fn copied(&self, theme: &Theme) -> Vec<Object> {
        let ids = self.selection();
        self.slide
            .objects
            .iter()
            .filter(|o| ids.contains(&o.id))
            .map(|o| Object { frame: Some(object_frame(&self.slide, o, theme)), ..o.clone() })
            .collect()
    }

    /// Puts copies of objects on the slide, on top, a little down and right of where they
    /// were copied from, and selects them.
    pub fn paste(&mut self, objects: &[Object]) {
        if objects.is_empty() {
            return;
        }
        self.record();
        let mut ids = Vec::new();
        // A screen picture belongs on a stage layout: anywhere else it would show nothing.
        let fits = |object: &&Object| self.keeps_roles || !matches!(object.kind, ObjectKind::Screen { .. });
        for object in objects.iter().filter(fits) {
            let mut copy = object.clone();
            copy.id = Uuid::new_v4();
            // A copy belongs to no theme box: two boxes with one role would fight over it.
            if !self.keeps_roles {
                copy.role = None;
            }
            if let Some(frame) = copy.frame.as_mut() {
                frame.x += 40.0;
                frame.y += 40.0;
            }
            ids.push(copy.id);
            self.slide.objects.push(copy);
        }
        self.selected = ids.first().copied();
        self.also = ids.into_iter().skip(1).collect();
    }

    /// Makes the change just made to the first selected object to every other selected one:
    /// the same change of look, of opacity, turn and fill, and the same move and change of
    /// size. `before` is the first selected object as it was. Words and names are not shared.
    pub fn share_change(&mut self, theme: &Theme, before: &Object) {
        let Some(after) = self.selected_object().cloned() else { return };
        if self.also.is_empty() || after == *before {
            return;
        }
        let looks = |object: &Object| theme.text_style(object.role.as_ref(), object.text_style()?);
        let (look_before, look_after) = (looks(before), looks(&after));
        let (was, now) = (object_frame(&self.slide, before, theme), object_frame(&self.slide, &after, theme));
        let moved_by = (now.x - was.x, now.y - was.y, now.w - was.w, now.h - was.h);
        for id in self.also.clone() {
            let frame = self.slide.objects.iter().find(|o| o.id == id).map(|o| object_frame(&self.slide, o, theme));
            let Some(object) = self.slide.objects.iter_mut().find(|o| o.id == id) else { continue };
            if before.opacity != after.opacity {
                object.opacity = after.opacity;
            }
            if before.rotation != after.rotation {
                object.rotation = after.rotation;
            }
            if before.fill != after.fill {
                object.fill = after.fill;
            }
            if before.stroke != after.stroke {
                object.stroke = after.stroke;
            }
            if before.shadow != after.shadow {
                object.shadow = after.shadow;
            }
            if before.flip_h != after.flip_h {
                object.flip_h = after.flip_h;
            }
            if before.flip_v != after.flip_v {
                object.flip_v = after.flip_v;
            }
            if moved_by != (0.0, 0.0, 0.0, 0.0)
                && let Some(frame) = frame
            {
                object.frame = Some(SlideRect {
                    x: frame.x + moved_by.0,
                    y: frame.y + moved_by.1,
                    w: (frame.w + moved_by.2).max(MIN_SIDE),
                    h: (frame.h + moved_by.3).max(MIN_SIDE),
                });
            }
            if let (Some(look_before), Some(look_after)) = (&look_before, &look_after)
                && look_before != look_after
            {
                let role = object.role.clone();
                if let ObjectKind::Text { style, .. } = &mut object.kind
                    && let Some(base) = theme.text_style(role.as_ref(), &TextStyle::default())
                {
                    let mut look = style.over(&base);
                    look.follow_change(look_before, look_after);
                    *style = TextStyle::between(&base, &look);
                }
            }
        }
    }

    /// Moves every selected object by an amount, as the arrow keys do.
    pub fn nudge(&mut self, theme: &Theme, by: (f32, f32)) {
        for id in self.selection() {
            let frame = self.slide.objects.iter().find(|o| o.id == id).map(|o| object_frame(&self.slide, o, theme));
            if let (Some(frame), Some(object)) = (frame, self.slide.objects.iter_mut().find(|o| o.id == id)) {
                object.frame = Some(moved(frame, by));
            }
        }
    }

    /// Moves an object to a place in the Objects list, which runs from the front of the slide to
    /// the back: `slot` is how many rows of the list will be above it (on a theme's layout, the
    /// nearest place it may go: see [`stack_slot`](Self::stack_slot)). One undo step. Returns
    /// true if it moved.
    pub fn move_in_stack(&mut self, id: Uuid, slot: usize) -> bool {
        let slot = self.stack_slot(id, slot);
        let mut list: Vec<Object> = self.slide.objects.iter().rev().cloned().collect();
        let Some(from) = list.iter().position(|object| object.id == id) else { return false };
        let to = if slot > from { slot - 1 } else { slot }.min(list.len() - 1);
        if to == from {
            return false;
        }
        self.record();
        let object = list.remove(from);
        list.insert(to, object);
        list.reverse();
        self.slide.objects = list;
        true
    }

    /// Where in the Objects list an object taken to `slot` (how many rows will be above it)
    /// goes. Anywhere, but on a theme's layout (`layered`): a text box among the text boxes, and
    /// a shape, picture or video above them all (in front of the words) or below them all, at
    /// whichever is nearer.
    pub fn stack_slot(&self, id: Uuid, slot: usize) -> usize {
        let list: Vec<&Object> = self.slide.objects.iter().rev().collect();
        let (Some(first), Some(last)) = (list.iter().position(|o| is_text(o)), list.iter().rposition(|o| is_text(o)))
        else {
            return slot;
        };
        match list.iter().find(|object| object.id == id) {
            Some(_) if !self.layered => slot,
            Some(object) if is_text(object) => slot.clamp(first, last + 1),
            Some(_) if slot > first && slot <= last => {
                if slot - first <= last + 1 - slot {
                    first
                } else {
                    last + 1
                }
            }
            _ => slot,
        }
    }

    /// On a theme's layout, puts the objects in the order its template keeps them (see
    /// `layered`): the shapes, pictures and videos behind the words, the text boxes, then those
    /// in front of the words, which are those with a text box behind them. One `is_new` says the
    /// layout hasn't had yet (just added or pasted) starts behind the words, as before.
    pub fn keep_layers(&mut self, is_new: impl Fn(Uuid) -> bool) {
        let first_text = self.slide.objects.iter().position(is_text);
        let (mut behind, mut texts, mut front) = (Vec::new(), Vec::new(), Vec::new());
        for (at, object) in std::mem::take(&mut self.slide.objects).into_iter().enumerate() {
            if is_text(&object) {
                texts.push(object);
            } else if first_text.is_some_and(|first| at > first) && !is_new(object.id) {
                front.push(object);
            } else {
                behind.push(object);
            }
        }
        self.slide.objects = behind.into_iter().chain(texts).chain(front).collect();
    }

    /// Changes the look of every selected text object the same way. Each keeps only what differs
    /// from its theme box, so it still follows the theme in everything else. Returns true if
    /// anything changed.
    pub fn restyle(&mut self, theme: &Theme, change: impl Fn(&mut sp_model::BoxStyle)) -> bool {
        let ids = self.selection();
        let Some(template) = theme.template_for(&self.slide).cloned() else { return false };
        let selected_words = self.selected_words();
        let mut changed = false;
        for object in self.slide.objects.iter_mut().filter(|object| ids.contains(&object.id)) {
            let Some(base) = template.box_for(object.role.as_ref()).cloned() else { continue };
            let ObjectKind::Text { text, style, spans } = &mut object.kind else { continue };
            let before_spans = spans.clone();
            let box_look = style.over(&base);
            let mut look = box_look.clone();
            change(&mut look);
            let length = text.chars().count();
            match selected_words.filter(|(id, ..)| *id == object.id) {
                // Words are selected: the settings a word can have go to those words, the rest
                // (alignment, shadow) to the box.
                Some((_, from, to)) => {
                    sp_model::restyle_range(spans, from, to, length, &box_look, &change);
                    let keep = |look: &mut sp_model::BoxStyle| {
                        look.font_family.clone_from(&box_look.font_family);
                        (look.size, look.weight, look.italic) = (box_look.size, box_look.weight, box_look.italic);
                        (look.underline, look.strike, look.fill) = (box_look.underline, box_look.strike, box_look.fill);
                    };
                    keep(&mut look);
                }
                // A whole box: what it changes in the box's look is no longer set word by word.
                None => {
                    clear_spans_for_changes(spans, length, &box_look, &look);
                    let (shown, _) = sp_model::look_of_range(spans, 0, length, &box_look);
                    let mut target = shown.clone();
                    change(&mut target);
                    apply_to_whole_box(spans, length, &shown, &target, &mut look);
                }
            }
            let own = TextStyle::between(&base, &look);
            if own != *style || *spans != before_spans {
                *style = own;
                changed = true;
            }
        }
        if changed {
            // One step of its own: not folded into the typing just before it.
            self.last_recorded = f64::NEG_INFINITY;
        }
        changed
    }

    /// The text object being typed in, and the first and last character of the words selected
    /// in it, if any are.
    pub fn selected_words(&self) -> Option<(Uuid, usize, usize)> {
        let (from, to) = self.typing_range.filter(|(from, to)| from < to)?;
        let id = self.typing?;
        self.slide.objects.iter().any(|o| o.id == id && o.as_text().is_some()).then_some((id, from, to))
    }

    /// The look the controls show for the selected text object: the look of the selected words
    /// if any are selected (and which settings differ among them), else the box's.
    pub fn look_shown(&self, theme: &Theme) -> Option<(sp_model::BoxStyle, sp_model::SpanMix)> {
        let object = self.selected_object()?;
        let box_look = theme.text_style_on(&self.slide, object.role.as_ref(), object.text_style()?)?;
        Some(match self.selected_words().filter(|(id, ..)| *id == object.id) {
            Some((_, from, to)) => sp_model::look_of_range(object.spans(), from, to, &box_look),
            None => {
                sp_model::look_of_range(object.spans(), 0, object.as_text().map_or(0, |t| t.chars().count()), &box_look)
            }
        })
    }

    /// Starts a new undo step with the next change, whatever came just before it.
    pub fn break_run(&mut self) {
        self.last_recorded = f64::NEG_INFINITY;
    }

    /// Changes the words of every selected text object. Returns true if any changed.
    pub fn retext(&mut self, change: impl Fn(&str) -> String) -> bool {
        let ids = self.selection();
        let mut changed = false;
        for object in self.slide.objects.iter_mut().filter(|object| ids.contains(&object.id)) {
            let ObjectKind::Text { text, .. } = &object.kind else { continue };
            let new = change(text);
            if new != *text {
                // Words that stay, stay styled: only the part that changed is replaced.
                object.set_text(&new);
                changed = true;
            }
        }
        changed
    }

    /// Moves the selected object to the front of the slide, or to the back.
    pub fn move_to_end_of_stack(&mut self, front: bool) -> bool {
        let Some(id) = self.selected else { return false };
        let slot = if front { 0 } else { self.slide.objects.len() };
        self.move_in_stack(id, slot)
    }

    /// Lines the selected objects up with the slide's sides or middle: 0 left, 1 centre, 2
    /// right across; 3 top, 4 middle, 5 bottom down. Locked objects stay put.
    pub fn align_to_slide(&mut self, theme: &Theme, edge: u8) -> bool {
        let ids = self.selection();
        if ids.is_empty() {
            return false;
        }
        self.record();
        let slide = self.slide.clone();
        let mut changed = false;
        for object in self.slide.objects.iter_mut().filter(|o| ids.contains(&o.id) && !o.locked) {
            let mut frame = object_frame(&slide, object, theme);
            match edge {
                0 => frame.x = 0.0,
                1 => frame.x = (SLIDE_WIDTH - frame.w) / 2.0,
                2 => frame.x = SLIDE_WIDTH - frame.w,
                3 => frame.y = 0.0,
                4 => frame.y = (SLIDE_HEIGHT - frame.h) / 2.0,
                _ => frame.y = SLIDE_HEIGHT - frame.h,
            }
            changed |= object.frame != Some(frame);
            object.frame = Some(frame);
        }
        changed
    }

    /// Mirrors the selected objects left to right, or top to bottom.
    pub fn flip(&mut self, across: bool) -> bool {
        let ids = self.selection();
        if ids.is_empty() {
            return false;
        }
        self.record();
        for object in self.slide.objects.iter_mut().filter(|o| ids.contains(&o.id) && !o.locked) {
            if across {
                object.flip_h = !object.flip_h;
            } else {
                object.flip_v = !object.flip_v;
            }
        }
        true
    }

    /// Locks the selected objects, or unlocks every object on the slide.
    pub fn lock(&mut self, lock: bool) -> bool {
        let ids = self.selection();
        let mut changed = false;
        for object in &mut self.slide.objects {
            let wanted = if lock { object.locked || ids.contains(&object.id) } else { false };
            changed |= object.locked != wanted;
            object.locked = wanted;
        }
        if lock {
            self.select_only(None);
        }
        changed
    }

    /// Spreads the selected objects out evenly, leaving the first and last where they are:
    /// across (equal gaps between them, left to right) or down. Needs three or more objects.
    pub fn distribute(&mut self, theme: &Theme, across: bool) -> bool {
        let ids = self.selection();
        let mut placed: Vec<(Uuid, SlideRect)> = self
            .slide
            .objects
            .iter()
            .filter(|object| ids.contains(&object.id))
            .map(|object| (object.id, object_frame(&self.slide, object, theme)))
            .collect();
        if placed.len() < 3 {
            return false;
        }
        let start = |rect: &SlideRect| if across { rect.x } else { rect.y };
        let length = |rect: &SlideRect| if across { rect.w } else { rect.h };
        placed.sort_by(|a, b| start(&a.1).total_cmp(&start(&b.1)));
        let first = placed[0].1;
        let last = placed[placed.len() - 1].1;
        let span = start(&last) + length(&last) - start(&first);
        let filled: f32 = placed.iter().map(|(_, rect)| length(rect)).sum();
        let gap = (span - filled) / (placed.len() - 1) as f32;
        let mut at = start(&first);
        let mut changed = false;
        for (id, rect) in placed {
            let mut moved = rect;
            if across {
                moved.x = at;
            } else {
                moved.y = at;
            }
            at += length(&rect) + gap;
            if let Some(object) = self.slide.objects.iter_mut().find(|object| object.id == id)
                && moved != rect
            {
                object.frame = Some(moved);
                changed = true;
            }
        }
        changed
    }

    /// Moves the selected object one place up (`true`) or down the stack. On a theme's layout
    /// (`layered`) a shape, picture or video next to the text boxes goes past them all, and a
    /// text box goes no further than they do. Returns true if it moved.
    pub fn restack(&mut self, forward: bool) -> bool {
        let Some(id) = self.selected else { return false };
        let objects = &self.slide.objects;
        let Some(index) = objects.iter().position(|o| o.id == id) else { return false };
        let target = if forward { index + 1 } else { index.wrapping_sub(1) };
        if target >= objects.len() {
            return false;
        }
        if self.layered && is_text(&objects[index]) != is_text(&objects[target]) {
            if is_text(&objects[index]) {
                return false;
            }
            // The far side of the text boxes: after the last of them, or before the first.
            let to = if forward { objects.iter().rposition(is_text) } else { objects.iter().position(is_text) };
            let Some(to) = to else { return false };
            self.record();
            let object = self.slide.objects.remove(index);
            self.slide.objects.insert(to, object);
            return true;
        }
        self.record();
        self.slide.objects.swap(index, target);
        true
    }

    /// Gives the selected object back to the theme: its own place, turn and fade are dropped.
    pub fn reset_to_theme(&mut self) -> bool {
        let Some(object) = self.selected_object() else { return false };
        if object.frame.is_none() && object.rotation == 0.0 && object.opacity == 1.0 {
            return false;
        }
        self.record();
        if let Some(object) = self.selected_object_mut() {
            object.frame = None;
            object.rotation = 0.0;
            object.opacity = 1.0;
        }
        true
    }

    /// A press on the slide at a point. It grabs a handle of the selected object, or selects
    /// the object under the point and gets ready to move the selection. With `extend` (Shift
    /// or Command held) it adds the object to the selection, or takes it out, and moves nothing.
    pub fn press(&mut self, theme: &Theme, point: (f32, f32), handle_reach: f32, extend: bool) {
        self.drag = None;
        self.drag_others.clear();
        // The selected object's handles come first: half of each lies outside the object, and
        // a press there resizes it rather than starting a rectangle or picking what is behind.
        if !extend
            && self.also.is_empty()
            && let Some(object) = self.selected_object()
            && object.rotation == 0.0
        {
            let from = object_frame(&self.slide, object, theme);
            if let Some(handle) = handle_at(from, point, handle_reach) {
                self.drag = Some(Drag::Resize { id: object.id, handle, from, start: point });
                return;
            }
        }
        let hit = object_at(&self.slide, theme, point);
        // A press on empty slide starts dragging out a rectangle. With Shift or Command held
        // what is already selected is kept, and the rectangle adds to it.
        if hit.is_none() {
            self.marquee_kept = if extend { self.selection() } else { Vec::new() };
            if !extend {
                self.select_only(None);
            }
            self.marquee_frames = self
                .slide
                .objects
                .iter()
                .filter(|o| !o.hidden && !o.locked)
                .map(|o| (o.id, object_frame(&self.slide, o, theme)))
                .collect();
            self.drag = Some(Drag::Select { start: point });
            return;
        }
        if extend {
            if let Some(id) = hit {
                self.toggle_selected(id);
            }
            return;
        }
        // A press on something already selected keeps the whole selection, so it can be
        // dragged together; a press anywhere else selects just what is there.
        match hit {
            Some(id) if self.is_selected(id) => {}
            other => self.select_only(other),
        }
        let frame_of = |draft: &EditDraft, id: Uuid| {
            draft.slide.objects.iter().find(|o| o.id == id).map(|o| object_frame(&draft.slide, o, theme))
        };
        // The object under the pointer leads the drag; the rest follow it.
        if let Some(lead) = hit.or(self.selected)
            && let Some(from) = frame_of(self, lead)
        {
            self.drag = Some(Drag::Move { id: lead, from, start: point });
            let followers: Vec<Uuid> = self.selection().into_iter().filter(|id| *id != lead).collect();
            self.drag_others = followers.into_iter().filter_map(|id| Some((id, frame_of(self, id)?))).collect();
        }
    }

    /// Moves the drag to a point. `keep_shape` (Shift held) keeps a box's proportions while a
    /// corner is dragged. `snap` pulls a moved object onto nearby lines: the slide's edges
    /// and middle, and the other objects'.
    pub fn drag_to(&mut self, point: (f32, f32), keep_shape: bool, snap: bool) -> bool {
        let Some(drag) = self.drag else { return false };
        self.guides.clear();
        if let Drag::Select { start } = drag {
            // Everything the rectangle touches is selected, along with what was kept.
            let (left, right) = (start.0.min(point.0), start.0.max(point.0));
            let (top, bottom) = (start.1.min(point.1), start.1.max(point.1));
            let touched = self
                .marquee_frames
                .iter()
                .filter(|(_, f)| f.x < right && f.x + f.w > left && f.y < bottom && f.y + f.h > top)
                .map(|(id, _)| *id);
            let mut ids = self.marquee_kept.clone();
            for id in touched {
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
            self.selected = ids.first().copied();
            self.also = ids.into_iter().skip(1).collect();
            self.marquee = Some((start, point));
            return false;
        }
        let (id, frame) = match drag {
            Drag::Move { id, from, start } => {
                let free = moved(from, (point.0 - start.0, point.1 - start.1));
                if snap {
                    // Everything that is not being moved is something to line up with.
                    let moving: Vec<Uuid> = self.drag_others.iter().map(|(id, _)| *id).chain([id]).collect();
                    let still: Vec<SlideRect> = self
                        .slide
                        .objects
                        .iter()
                        .filter(|o| !moving.contains(&o.id) && !o.hidden)
                        .filter_map(|o| o.frame)
                        .collect();
                    let (frame, guides) = snapped_on(free, &still, self.grid);
                    self.guides = guides;
                    (id, frame)
                } else {
                    (id, free)
                }
            }
            Drag::Select { .. } => return false,
            Drag::Resize { id, handle, from, start } => {
                let free = resized(from, handle, (point.0 - start.0, point.1 - start.1));
                if keep_shape {
                    // Keeping the box's shape decides both edges; lines are not followed.
                    (id, same_shape(from, handle, free))
                } else if snap {
                    let still: Vec<SlideRect> =
                        self.slide.objects.iter().filter(|o| o.id != id && !o.hidden).filter_map(|o| o.frame).collect();
                    let (frame, guides) = snapped_resize_on(free, handle, &still, self.grid);
                    self.guides = guides;
                    (id, frame)
                } else {
                    (id, free)
                }
            }
        };
        let Some(index) = self.slide.objects.iter().position(|o| o.id == id) else { return false };
        if self.slide.objects[index].frame == Some(frame) {
            return false;
        }
        // The first movement of a drag is the moment to remember where things were.
        let before = self.slide.clone();
        self.slide.objects[index].frame = Some(frame);
        let (Drag::Move { from, .. } | Drag::Resize { from, .. }) = drag else { return false };
        // The rest of the selection moves by exactly as much as the object being dragged.
        if matches!(drag, Drag::Move { .. }) {
            let by = (frame.x - from.x, frame.y - from.y);
            for (other, was) in self.drag_others.clone() {
                if let Some(object) = self.slide.objects.iter_mut().find(|o| o.id == other) {
                    object.frame = Some(moved(was, by));
                }
            }
        }
        if before.objects[index].frame.is_none() || before.objects[index].frame == Some(from) {
            self.record_slide(before);
        }
        true
    }

    pub fn release(&mut self) {
        self.drag = None;
        self.drag_others.clear();
        self.guides.clear();
        self.marquee = None;
        self.marquee_frames.clear();
        self.marquee_kept.clear();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sp_model::parse_song_text;

    #[test]
    fn a_boxs_role_is_shown_in_the_language_set_and_its_name_is_never_saved_translated() {
        use crate::i18n::{Language, with_language};
        use sp_model::{Object, Role};
        let roles = [Role::primary(), Role::translation(), Role::reference(), Role::translation_reference()];
        // In English the window's words are sp-model's labels, which the files keep.
        for role in &roles {
            assert_eq!(with_language(Language::English, || super::role_label(role)), role.label());
        }
        let spanish: Vec<String> = with_language(Language::Spanish, || roles.iter().map(super::role_label).collect());
        assert_eq!(spanish, ["Principal", "Traducción", "Referencia", "Referencia de la traducción"]);
        // A role of a theme's own is called by its name.
        assert_eq!(
            with_language(Language::Spanish, || super::role_label(&Role("verse-text".to_owned()))),
            "Verse text"
        );
        // A box named after its role is shown as its role is called; typed back so, it keeps
        // the English label (no name of its own); a name of its own stays as typed.
        let mut object = Object::text(Role::primary(), "Text");
        object.name = Role::primary().label();
        with_language(Language::Spanish, || {
            assert_eq!(super::object_name(&object), "Principal");
            assert_eq!(super::typed_object_name(&object, "Principal"), "Main");
            assert_eq!(super::typed_object_name(&object, "Coro"), "Coro");
        });
    }

    #[test]
    fn new_boxes_are_named_and_say_their_sample_words_in_the_language_set_and_keep_them() {
        use crate::i18n::{Language, with_language};
        let mut d = draft();
        let before = d.slide.objects.len();
        with_language(Language::Spanish, || {
            d.add_text();
            d.add_shape(sp_model::Shape::Star);
            d.add_shape(sp_model::Shape::Star);
            d.add_web();
            d.add_ndi();
        });
        let added: Vec<(String, Option<String>)> = d.slide.objects[before..]
            .iter()
            .map(|object| (object.name.clone(), object.as_text().map(str::to_owned)))
            .collect();
        let number = (before + 1).to_string();
        let expected = [
            (format!("Texto {number}"), Some("Texto".to_owned())),
            ("Estrella".to_owned(), None),
            ("Estrella 2".to_owned(), None),
            ("Página web".to_owned(), None),
            ("Fuente NDI".to_owned(), None),
        ];
        assert_eq!(added, expected);
        // Kept as given: in English the Spanish box is still the app's, by its name and its words.
        with_language(Language::English, || {
            assert!(super::is_new_text_name(&format!("Texto {number}")) && super::is_new_text_name("Text 12"));
            assert!(super::is_sample_words("Texto") && super::is_sample_words("Text"));
            assert_eq!(super::sample_words(), "Text");
        });
        // A church's names and words are its own, in any language.
        for (name, words) in [("Texto final", "Textos"), ("Text", "Text 2"), ("Texto 2a", "texto"), ("Texto ", "")] {
            assert!(!super::is_new_text_name(name), "{name}");
            assert!(!super::is_sample_words(words), "{words}");
        }
    }

    use super::*;

    const F: SlideRect = SlideRect { x: 100.0, y: 200.0, w: 400.0, h: 300.0 };

    fn draft() -> EditDraft {
        let show = Show::new(vec![Arc::new(parse_song_text("S", "Main words\n--\nПеревод\n"))]);
        EditDraft::of(&show, SlideRef { presentation: 0, slide: 0 }).unwrap()
    }

    #[test]
    fn an_ndi_source_box_is_added_as_a_web_page_box_is_named_picked_and_in_front() {
        let mut d = draft();
        let before = d.slide.objects.len();
        d.add_ndi();
        let first = d.selected_object().unwrap();
        assert_eq!(first.name, "NDI source");
        assert!(matches!(&first.kind, ObjectKind::Ndi { source, scaling, quality, sound }
            if source.is_empty() && *scaling == sp_model::ImageFit::Fit && *quality == sp_model::NdiQuality::Full && !sound));
        let frame = first.frame.unwrap();
        assert_eq!((frame.w, frame.h), (1400.0, 788.0), "16:9");
        assert_eq!(
            (frame.x + frame.w / 2.0, frame.y + frame.h / 2.0),
            (sp_model::SLIDE_WIDTH / 2.0, sp_model::SLIDE_HEIGHT / 2.0)
        );
        assert_eq!(d.slide.objects.len(), before + 1);
        assert_eq!(d.slide.objects.last().map(|o| o.id), d.selected, "in front, and the one picked");
        d.add_ndi();
        assert_eq!(d.selected_object().unwrap().name, "NDI source 2");
        // A web page box beside them is counted on its own.
        d.add_web();
        assert_eq!(d.selected_object().unwrap().name, "Web page");
        // One step back takes the last one away.
        d.undo();
        assert_eq!(d.slide.objects.len(), before + 2);
    }

    #[test]
    fn handles_sit_on_the_corners_and_the_middles_of_the_sides() {
        assert_eq!(Handle::TopLeft.position(F), (100.0, 200.0));
        assert_eq!(Handle::Right.position(F), (500.0, 350.0));
        assert_eq!(Handle::Bottom.position(F), (300.0, 500.0));
        assert_eq!(handle_at(F, (503.0, 497.0), 6.0), Some(Handle::BottomRight));
        assert_eq!(handle_at(F, (300.0, 350.0), 6.0), None);
    }

    #[test]
    fn a_press_just_outside_a_selected_box_on_its_handle_resizes_it() {
        let mut d = draft();
        let theme = Theme::builtin();
        d.press(&theme, (960.0, 300.0), 5.0, false);
        d.release();
        let frame = object_frame(&d.slide, d.selected_object().unwrap(), &theme);
        // Outside the box, past its top-left corner, but within the handle's reach.
        d.press(&theme, (frame.x - 6.0, frame.y - 6.0), 10.0, false);
        d.drag_to((frame.x - 46.0, frame.y - 26.0), false, false);
        d.release();
        let now = object_frame(&d.slide, d.selected_object().unwrap(), &theme);
        assert_eq!((now.x, now.y, now.w), (frame.x - 40.0, frame.y - 20.0, frame.w + 40.0));
        // On a tiny box the nearest handle is the one taken.
        let small = SlideRect { x: 0.0, y: 0.0, w: 12.0, h: 12.0 };
        assert_eq!(handle_at(small, (12.0, 13.0), 10.0), Some(Handle::BottomRight));
    }

    #[test]
    fn resizing_keeps_the_opposite_side_still() {
        assert_eq!(resized(F, Handle::Right, (50.0, 999.0)), SlideRect { w: 450.0, ..F });
        assert_eq!(resized(F, Handle::TopLeft, (10.0, 20.0)), SlideRect { x: 110.0, y: 220.0, w: 390.0, h: 280.0 });
        assert_eq!(resized(F, Handle::Bottom, (999.0, -100.0)), SlideRect { h: 200.0, ..F });
    }

    #[test]
    fn a_corner_dragged_with_shift_keeps_the_boxs_shape() {
        // F is 400 x 300: four to three.
        let free = resized(F, Handle::BottomRight, (200.0, 30.0));
        let kept = same_shape(F, Handle::BottomRight, free);
        assert_eq!((kept.x, kept.y, kept.w, kept.h), (F.x, F.y, 600.0, 450.0));
        // The opposite corner stays put when the top-left is dragged.
        let free = resized(F, Handle::TopLeft, (-200.0, 0.0));
        let kept = same_shape(F, Handle::TopLeft, free);
        assert_eq!((kept.x + kept.w, kept.y + kept.h), (F.x + F.w, F.y + F.h));
        assert_eq!((kept.w, kept.h), (600.0, 450.0));
        // A side handle is left alone.
        let free = resized(F, Handle::Right, (200.0, 0.0));
        assert_eq!(same_shape(F, Handle::Right, free), free);
    }

    #[test]
    fn a_dragged_box_is_pulled_onto_the_slides_middle_and_other_boxes() {
        // A 400 x 300 box whose middle is 6 units left of the slide's middle and 5 above it.
        let near = SlideRect { x: SLIDE_WIDTH / 2.0 - 206.0, y: SLIDE_HEIGHT / 2.0 - 155.0, w: 400.0, h: 300.0 };
        let (frame, guides) = snapped(near, &[]);
        assert_eq!((frame.x + frame.w / 2.0, frame.y + frame.h / 2.0), (SLIDE_WIDTH / 2.0, SLIDE_HEIGHT / 2.0));
        assert_eq!(guides, [Guide::Down(SLIDE_WIDTH / 2.0), Guide::Across(SLIDE_HEIGHT / 2.0)]);
        // Far from every line it is left alone.
        let far = SlideRect { x: 333.0, y: 177.0, w: 400.0, h: 300.0 };
        assert_eq!(snapped(far, &[]), (far, Vec::new()));
        // Its left edge 8 units from another box's left edge: pulled to it.
        let other = SlideRect { x: 341.0, y: 600.0, w: 100.0, h: 100.0 };
        let (frame, guides) = snapped(far, &[other]);
        assert_eq!((frame.x, guides), (341.0, vec![Guide::Down(341.0)]));
        // Near the safe edge it is pulled onto that.
        // With the grid on, a box near one of its lines is pulled onto it.
        let by_grid = SlideRect { x: 245.0, y: 610.0, w: 300.0, h: 100.0 };
        assert_eq!(snapped(by_grid, &[]).0, by_grid);
        assert_eq!(snapped_on(by_grid, &[], true).0, SlideRect { x: 240.0, y: 600.0, ..by_grid });
        let (frame, guides) = snapped(SlideRect { x: SAFE_EDGE.0 + 9.0, y: 177.0, w: 400.0, h: 300.0 }, &[]);
        assert_eq!((frame.x, guides), (SAFE_EDGE.0, vec![Guide::Down(SAFE_EDGE.0)]));
    }

    #[test]
    fn a_dragged_edge_is_pulled_onto_lines_and_the_other_edges_stay() {
        // The right edge, 6 units short of the slide's middle, is pulled onto it.
        let frame = SlideRect { x: 300.0, y: 200.0, w: SLIDE_WIDTH / 2.0 - 306.0, h: 300.0 };
        let (pulled, guides) = snapped_resize(frame, Handle::Right, &[]);
        assert_eq!((pulled.x, pulled.x + pulled.w, pulled.y, pulled.h), (300.0, SLIDE_WIDTH / 2.0, 200.0, 300.0));
        assert_eq!(guides, [Guide::Down(SLIDE_WIDTH / 2.0)]);
        // A corner follows a line each way: here the safe edge at the top left.
        let frame = SlideRect { x: SAFE_EDGE.0 + 5.0, y: SAFE_EDGE.1 - 7.0, w: 400.0, h: 300.0 };
        let (pulled, guides) = snapped_resize(frame, Handle::TopLeft, &[]);
        assert_eq!((pulled.x, pulled.y), SAFE_EDGE);
        assert_eq!((pulled.x + pulled.w, pulled.y + pulled.h), (frame.x + 400.0, frame.y + 300.0));
        assert_eq!(guides.len(), 2);
        // The edges not being dragged are never moved, however near a line they are.
        let frame = SlideRect { x: 3.0, y: 200.0, w: 500.0, h: 300.0 };
        assert_eq!(snapped_resize(frame, Handle::Bottom, &[]).0, frame);
    }

    #[test]
    fn several_objects_are_selected_moved_copied_and_deleted_together() {
        let mut d = draft();
        let theme = Theme::builtin();
        let (main, translation) = (d.slide.objects[0].id, d.slide.objects[1].id);
        // A click selects one; a click with Shift held adds another, and again takes it out.
        d.press(&theme, (960.0, 300.0), 5.0, false);
        d.release();
        d.press(&theme, (960.0, 700.0), 5.0, true);
        assert_eq!(d.selection(), [main, translation]);
        d.press(&theme, (960.0, 700.0), 5.0, true);
        assert_eq!(d.selection(), [main]);
        d.press(&theme, (960.0, 700.0), 5.0, true);

        // Dragging one of them moves both by the same amount.
        let before: Vec<SlideRect> = d.slide.objects.iter().map(|o| object_frame(&d.slide, o, &theme)).collect();
        d.press(&theme, (960.0, 300.0), 5.0, false);
        assert_eq!(d.selection().len(), 2);
        d.drag_to((1000.0, 333.0), false, false);
        d.release();
        for (object, was) in d.slide.objects.iter().zip(&before) {
            let now = object.frame.unwrap();
            assert_eq!((now.x - was.x, now.y - was.y), (40.0, 33.0));
        }

        // Copies land on top, offset, selected, and tied to no theme box.
        let copies = d.copied(&theme);
        d.paste(&copies);
        assert_eq!(d.slide.objects.len(), 4);
        assert_eq!(d.selection().len(), 2);
        assert!(d.slide.objects[2..].iter().all(|o| o.role.is_none() && d.is_selected(o.id)));
        assert_eq!(d.slide.objects[2].frame.unwrap().x, d.slide.objects[0].frame.unwrap().x + 40.0);

        // Delete removes every selected object, and one undo brings them back.
        assert!(d.delete_selected());
        assert_eq!(d.slide.objects.len(), 2);
        assert!(d.undo());
        assert_eq!(d.slide.objects.len(), 4);

        d.select_all();
        assert_eq!(d.selection().len(), 4);
    }

    #[test]
    fn dragging_a_rectangle_over_objects_selects_them() {
        let mut d = draft();
        let theme = Theme::builtin();
        let (main, translation) = (d.slide.objects[0].id, d.slide.objects[1].id);
        // From empty slide at the top left, down across the main box only.
        d.press(&theme, (10.0, 10.0), 5.0, false);
        d.drag_to((400.0, 300.0), false, true);
        assert_eq!(d.selection(), [main]);
        assert!(d.marquee.is_some());
        // Further down it reaches the translation too; drawing back lets it go again.
        d.drag_to((400.0, 800.0), false, true);
        assert_eq!(d.selection(), [main, translation]);
        d.drag_to((400.0, 300.0), false, true);
        assert_eq!(d.selection(), [main]);
        d.release();
        assert!(d.marquee.is_none());
        assert_eq!(d.selection(), [main]);
        // Nothing moved, and nothing was recorded to undo.
        assert!(!d.can_undo());
        // A plain click on empty slide still clears the selection.
        d.press(&theme, (10.0, 10.0), 5.0, false);
        d.release();
        assert!(d.selection().is_empty());
    }

    #[test]
    fn a_change_to_the_first_selected_object_reaches_the_others() {
        let mut d = draft();
        let theme = Theme::builtin();
        d.select_all();
        let before = d.selected_object().unwrap().clone();
        // Make the first italic, half faded, and 30 units lower.
        let frame = object_frame(&d.slide, &before, &theme);
        let first = d.selected_object_mut().unwrap();
        first.opacity = 0.5;
        first.frame = Some(SlideRect { y: frame.y + 30.0, ..frame });
        let ObjectKind::Text { style, .. } = &mut first.kind else { panic!("a text object") };
        style.italic = Some(true);
        let other_before = object_frame(&d.slide, &d.slide.objects[1], &theme);
        d.share_change(&theme, &before);
        let other = &d.slide.objects[1];
        let ObjectKind::Text { style, .. } = &other.kind else { panic!("a text object") };
        assert_eq!((style.italic, other.opacity), (Some(true), 0.5));
        assert_eq!(other.frame.unwrap().y, other_before.y + 30.0);
        // Its own colour and size were not touched.
        assert_eq!((style.fill, style.size), (None, None));
    }

    #[test]
    fn an_object_dragged_in_the_list_moves_in_the_stack() {
        let mut d = draft();
        let names = |d: &EditDraft| d.slide.objects.iter().rev().map(|o| o.name.clone()).collect::<Vec<_>>();
        for (index, object) in d.slide.objects.iter_mut().enumerate() {
            object.name = format!("o{index}");
        }
        // The list runs front to back, so the last object is listed first.
        assert_eq!(names(&d), ["o1", "o0"]);
        let back = d.slide.objects[0].id;
        assert!(d.move_in_stack(back, 0));
        assert_eq!(names(&d), ["o0", "o1"]);
        // Dropped where it already is, nothing changes.
        assert!(!d.move_in_stack(back, 0));
        assert!(!d.move_in_stack(back, 1));
        assert!(d.move_in_stack(back, 2));
        assert_eq!(names(&d), ["o1", "o0"]);
        // Each move is a step of its own.
        assert_eq!(d.undo_depth(), 2);
    }

    #[test]
    fn on_a_themes_layout_a_shape_goes_past_the_text_boxes_all_at_once() {
        let theme = Theme::builtin();
        let mut d = EditDraft::of_template(&theme.templates[0], 0);
        assert!(d.layered);
        let names = |d: &EditDraft| d.slide.objects.iter().rev().map(|o| o.name.clone()).collect::<Vec<_>>();
        // A new shape, behind the words as the theme keeps it.
        d.add_shape(sp_model::Shape::Rectangle);
        let shape = d.selected.expect("picked");
        d.keep_layers(|id| id == shape);
        assert_eq!(names(&d), ["Reference", "Translation", "Main", "Rectangle"]);
        // In the list: between two boxes is nearest the top or the bottom of them; a box can't
        // leave them.
        assert_eq!((d.stack_slot(shape, 1), d.stack_slot(shape, 2), d.stack_slot(shape, 4)), (0, 3, 4));
        let main = d.slide.objects[1].id;
        assert_eq!((d.stack_slot(main, 0), d.stack_slot(main, 4)), (0, 3));
        assert!(d.move_in_stack(shape, 1));
        assert_eq!(names(&d), ["Rectangle", "Reference", "Translation", "Main"]);
        assert!(d.move_in_stack(main, 0), "to the top of the boxes");
        assert_eq!(names(&d), ["Rectangle", "Main", "Reference", "Translation"]);
        // Bring Forward and Send Backward: past every box at once, and a box no further.
        d.select_only(Some(main));
        assert!(!d.restack(true), "the shape is in front of it");
        d.select_only(Some(shape));
        assert!(d.restack(false));
        assert_eq!(names(&d), ["Main", "Reference", "Translation", "Rectangle"]);
        assert!(!d.restack(false), "at the back");
        assert!(d.restack(true));
        assert_eq!(names(&d).first().map(String::as_str), Some("Rectangle"));
        // To the back and to the front, likewise; a box to the front of the boxes only.
        assert!(d.move_to_end_of_stack(false));
        assert_eq!(names(&d).last().map(String::as_str), Some("Rectangle"));
        d.select_only(Some(main));
        assert!(d.move_to_end_of_stack(false));
        assert_eq!(names(&d), ["Reference", "Translation", "Main", "Rectangle"]);
        // A shape with a box behind it is in front of the words; a new one starts behind them.
        let mut logo = sp_model::Object::shape(sp_model::Shape::Ellipse);
        logo.name = "Logo".to_owned();
        let logo_id = logo.id;
        d.slide.objects.insert(2, logo);
        d.keep_layers(|_| false);
        assert_eq!(names(&d), ["Logo", "Reference", "Translation", "Main", "Rectangle"]);
        assert_eq!(d.slide.objects.last().map(|o| o.id), Some(logo_id));
        let mut star = sp_model::Object::shape(sp_model::Shape::Star);
        star.name = "Star".to_owned();
        let star_id = star.id;
        d.slide.objects.push(star);
        d.keep_layers(|id| id == star_id);
        assert_eq!(names(&d), ["Logo", "Reference", "Translation", "Main", "Star", "Rectangle"]);
        // A song's slide has no such rule.
        let song = draft();
        let back = song.slide.objects[0].id;
        assert_eq!(song.stack_slot(back, 1), 1);
    }

    #[test]
    fn menu_changes_restyle_retext_restack_and_lock_the_selection() {
        let theme = Theme::builtin();
        let mut d = draft();
        let (back, front) = (d.slide.objects[0].id, d.slide.objects[1].id);
        d.select_only(Some(back));
        assert!(d.restyle(&theme, |look| look.weight = 400));
        let ObjectKind::Text { style, .. } = &d.slide.objects[0].kind else { panic!("a text object") };
        assert_eq!((style.weight, style.size), (Some(400), None), "only what changed is kept");
        // Setting it back to the theme's own look leaves nothing of its own.
        assert!(d.restyle(&theme, |look| look.weight = 700));
        let ObjectKind::Text { style, .. } = &d.slide.objects[0].kind else { panic!("a text object") };
        assert!(style.is_default());
        assert!(d.retext(str::to_uppercase));
        assert!(d.slide.objects[0].as_text().is_some_and(|text| text == text.to_uppercase()));
        assert!(d.move_to_end_of_stack(true));
        assert_eq!(d.slide.objects.last().map(|o| o.id), Some(back));
        assert!(d.move_to_end_of_stack(false));
        assert_eq!(d.slide.objects[0].id, back);
        assert!(d.lock(true));
        assert!(d.slide.objects.iter().any(|o| o.id == back && o.locked));
        assert!(d.slide.objects.iter().any(|o| o.id == front && !o.locked));
        assert!(d.lock(false));
        assert!(d.slide.objects.iter().all(|o| !o.locked));
    }

    #[test]
    fn controls_style_the_selected_words_or_the_whole_box_and_each_is_one_undo_step() {
        let theme = Theme::builtin();
        let mut d = draft();
        let id = d.slide.objects[0].id;
        d.select_only(Some(id));
        // "Main words": words 5..10 selected while typing in the box.
        d.typing = Some(id);
        d.typing_range = Some((5, 10));
        let before = d.slide.clone();
        d.record();
        assert!(d.restyle(&theme, |look| look.italic = true));
        let ObjectKind::Text { style, spans, .. } = &d.slide.objects[0].kind else { panic!("a text object") };
        assert!(style.is_default(), "the box is not touched");
        assert_eq!(spans.len(), 1);
        assert_eq!((spans[0].start, spans[0].end, spans[0].style.italic), (5, 10, Some(true)));
        // What the controls show for those words, and for a range that is only partly italic.
        let (look, mixed) = d.look_shown(&theme).unwrap();
        assert!(look.italic && !mixed.italic);
        d.typing_range = Some((0, 10));
        let (_, mixed) = d.look_shown(&theme).unwrap();
        assert!(mixed.italic, "some of the words are italic and some are not");
        // One Undo takes the styling back.
        assert!(d.undo());
        assert_eq!(d.slide, before);
        // Only a caret: the whole box, and every word takes it.
        d.redo();
        d.typing_range = Some((3, 3));
        d.record();
        assert!(d.restyle(&theme, |look| look.italic = true));
        let ObjectKind::Text { style, spans, .. } = &d.slide.objects[0].kind else { panic!("a text object") };
        assert!(spans.is_empty(), "the words gave the look up");
        assert_eq!(style.italic, Some(true));
    }

    #[test]
    fn the_selection_lines_up_with_the_slide_and_flips() {
        let mut d = draft();
        d.add_shape(sp_model::Shape::Star);
        let theme = Theme::builtin();
        let frame = |d: &EditDraft| d.selected_object().and_then(|o| o.frame).unwrap();
        assert!(d.align_to_slide(&theme, 2));
        assert_eq!(frame(&d).x + frame(&d).w, SLIDE_WIDTH);
        assert!(d.align_to_slide(&theme, 3));
        assert_eq!(frame(&d).y, 0.0);
        assert!(!d.align_to_slide(&theme, 3), "already there");
        d.flip(true);
        assert!(d.selected_object().is_some_and(|o| o.flip_h && !o.flip_v));
        d.undo();
        assert!(d.selected_object().is_some_and(|o| !o.flip_h));
    }

    #[test]
    fn a_shape_is_added_in_front_selected_and_named_after_its_kind() {
        let mut d = draft();
        d.add_shape(sp_model::Shape::Star);
        d.add_shape(sp_model::Shape::Star);
        let last = d.slide.objects.last().unwrap();
        assert_eq!((last.name.as_str(), d.selected), ("Star 2", Some(last.id)));
        assert!(d.undo());
        assert_eq!(d.slide.objects.last().unwrap().name, "Star");
    }

    #[test]
    fn three_objects_are_spread_out_evenly() {
        let theme = Theme::builtin();
        let mut d = draft();
        d.slide.objects.clear();
        for x in [0.0, 300.0, 1000.0] {
            let mut shape = Object::shape(sp_model::Shape::Rectangle);
            shape.frame = Some(SlideRect { x, y: 100.0, w: 100.0, h: 100.0 });
            d.slide.objects.push(shape);
        }
        d.select_all();
        assert!(d.distribute(&theme, true));
        let xs: Vec<f32> = d.slide.objects.iter().map(|o| o.frame.unwrap().x).collect();
        assert_eq!(xs, [0.0, 500.0, 1000.0]);
        assert!(!d.distribute(&theme, true), "already even");
    }

    #[test]
    fn locked_and_hidden_objects_let_clicks_through() {
        let mut d = draft();
        let theme = Theme::builtin();
        let (main, translation) = (d.slide.objects[0].id, d.slide.objects[1].id);
        assert_eq!(object_at(&d.slide, &theme, (960.0, 300.0)), Some(main));
        d.slide.objects[0].locked = true;
        assert_eq!(object_at(&d.slide, &theme, (960.0, 300.0)), None);
        d.slide.objects[1].hidden = true;
        assert_eq!(object_at(&d.slide, &theme, (960.0, 700.0)), None);
        let _ = translation;
    }

    #[test]
    fn resizing_stops_at_the_smallest_size_without_flipping() {
        let squeezed = resized(F, Handle::Left, (5000.0, 0.0));
        assert_eq!(squeezed, SlideRect { x: 500.0 - MIN_SIDE, w: MIN_SIDE, ..F });
        let squeezed = resized(F, Handle::Top, (0.0, 5000.0));
        assert_eq!(squeezed, SlideRect { y: 500.0 - MIN_SIDE, h: MIN_SIDE, ..F });
        let squeezed = resized(F, Handle::BottomRight, (-5000.0, -5000.0));
        assert_eq!(squeezed, SlideRect { w: MIN_SIDE, h: MIN_SIDE, ..F });
    }

    #[test]
    fn a_turned_frame_is_hit_where_it_is_drawn() {
        // A wide, flat frame turned a quarter stands tall and narrow about the same centre.
        let flat = SlideRect { x: 100.0, y: 300.0, w: 400.0, h: 40.0 };
        assert!(contains(flat, 0.0, (120.0, 320.0)));
        assert!(!contains(flat, 90.0, (120.0, 320.0)));
        assert!(contains(flat, 90.0, (300.0, 150.0)));
        assert!(!contains(flat, 0.0, (300.0, 150.0)));
    }

    #[test]
    fn the_topmost_object_under_the_pointer_is_found() {
        let theme = Theme::builtin();
        let mut d = draft();
        let main = d.slide.objects[0].id;
        let translation = d.slide.objects[1].id;
        assert_eq!(object_at(&d.slide, &theme, (960.0, 300.0)), Some(main));
        assert_eq!(object_at(&d.slide, &theme, (960.0, 700.0)), Some(translation));
        assert_eq!(object_at(&d.slide, &theme, (5.0, 5.0)), None);
        // Where two overlap, the one drawn last is on top.
        d.slide.objects[1].frame = Some(SlideRect { x: 0.0, y: 0.0, w: 1920.0, h: 1080.0 });
        assert_eq!(object_at(&d.slide, &theme, (960.0, 300.0)), Some(translation));
    }

    #[test]
    fn dragging_an_object_moves_it_and_is_one_undo_step() {
        let theme = Theme::builtin();
        let mut d = draft();
        let original = d.slide.clone();
        d.press(&theme, (960.0, 300.0), 8.0, false);
        assert_eq!(d.selected, Some(d.slide.objects[0].id));
        assert!(d.drag_to((970.0, 310.0), false, false));
        assert!(d.drag_to((1000.0, 350.0), false, false));
        d.release();
        let themed = theme.templates[0].boxes[0].rect;
        assert_eq!(d.slide.objects[0].frame, Some(SlideRect { x: themed.x + 40.0, y: themed.y + 50.0, ..themed }));
        assert!(d.undo());
        assert_eq!(d.slide, original, "one undo puts the whole drag back");
        assert!(!d.undo());
        assert!(d.redo());
        assert_eq!(d.slide.objects[0].frame.map(|f| f.x), Some(themed.x + 40.0));
    }

    #[test]
    fn dragging_a_handle_resizes_the_selected_object() {
        let theme = Theme::builtin();
        let mut d = draft();
        d.press(&theme, (960.0, 300.0), 8.0, false);
        d.release();
        let themed = theme.templates[0].boxes[0].rect;
        let corner = (themed.x + themed.w, themed.y + themed.h);
        d.press(&theme, corner, 8.0, false);
        assert!(matches!(d.drag, Some(Drag::Resize { handle: Handle::BottomRight, .. })));
        d.drag_to((corner.0 - 100.0, corner.1 + 60.0), false, false);
        assert_eq!(d.slide.objects[0].frame, Some(SlideRect { w: themed.w - 100.0, h: themed.h + 60.0, ..themed }));
    }

    #[test]
    fn pressing_empty_slide_clears_the_selection_and_a_press_without_a_drag_changes_nothing() {
        let theme = Theme::builtin();
        let mut d = draft();
        let original = d.slide.clone();
        d.press(&theme, (960.0, 300.0), 8.0, false);
        d.release();
        assert!(d.selected.is_some());
        assert_eq!(d.slide, original);
        assert!(!d.can_undo());
        d.press(&theme, (5.0, 5.0), 8.0, false);
        assert_eq!(d.selected, None);
        assert!(!d.drag_to((50.0, 50.0), false, false));
    }

    #[test]
    fn adding_deleting_and_restacking() {
        let mut d = draft();
        d.add_text();
        assert_eq!(d.slide.objects.len(), 3);
        assert_eq!(d.selected, Some(d.slide.objects[2].id));
        assert!(d.slide.objects[2].frame.is_some() && d.slide.objects[2].role.is_none());
        assert!(d.restack(false));
        assert_eq!(d.selected, Some(d.slide.objects[1].id));
        assert!(d.restack(false));
        assert!(!d.restack(false), "already at the bottom");
        assert!(d.delete_selected());
        assert_eq!(d.slide.objects.len(), 2);
        assert!(!d.delete_selected(), "nothing is selected now");
        // Undo walks back through delete, the two moves and the add.
        for _ in 0..4 {
            assert!(d.undo());
        }
        assert_eq!(d.slide.objects.len(), 2);
        assert!(!d.can_undo());
    }

    #[test]
    fn reset_gives_the_object_back_to_the_theme() {
        let theme = Theme::builtin();
        let mut d = draft();
        d.press(&theme, (960.0, 300.0), 8.0, false);
        assert!(!d.reset_to_theme(), "nothing to reset yet");
        d.drag_to((900.0, 300.0), false, false);
        d.release();
        d.selected_object_mut().unwrap().rotation = 12.0;
        assert!(d.reset_to_theme());
        let object = d.selected_object().unwrap();
        assert_eq!((object.frame, object.rotation, object.opacity), (None, 0.0, 1.0));
    }

    #[test]
    fn a_run_of_quick_changes_is_one_undo_step() {
        let mut d = draft();
        let first = d.slide.clone();
        for (i, now) in [10.0, 10.3, 10.6, 10.9].into_iter().enumerate() {
            let before = d.slide.clone();
            d.slide.objects[0].kind =
                ObjectKind::Text { text: "a".repeat(i + 1), style: TextStyle::default(), spans: Vec::new() };
            d.record_run(before, now);
        }
        // A pause, then more typing: a second step.
        let before = d.slide.clone();
        d.slide.objects[0].kind =
            ObjectKind::Text { text: "later".to_owned(), style: TextStyle::default(), spans: Vec::new() };
        d.record_run(before, 13.0);
        assert!(d.undo());
        assert_eq!(d.slide.objects[0].as_text(), Some("aaaa"));
        assert!(d.undo());
        assert_eq!(d.slide, first);
        assert!(!d.undo());
    }
}
