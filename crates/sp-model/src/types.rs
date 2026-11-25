use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::theme::{Shadow, SlideRect, TextStyle};

/// A colour with straight (not premultiplied) alpha.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const BLACK: Rgba = Rgba::rgb(0, 0, 0);
    pub const WHITE: Rgba = Rgba::rgb(255, 255, 255);

    pub const fn rgb(r: u8, g: u8, b: u8) -> Rgba {
        Rgba { r, g, b, a: 255 }
    }
}

/// The name a text box is addressed by. Themes style boxes by role.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Role(pub String);

impl Role {
    /// The line that says where a Bible verse is from.
    pub fn reference() -> Role {
        Role("reference".to_owned())
    }

    /// The role's name as people see it: "Main", "Translation", or the role's own name with
    /// a capital and spaces.
    pub fn label(&self) -> String {
        match self.0.as_str() {
            "primary" => "Main".to_owned(),
            "translation" => "Translation".to_owned(),
            other => {
                let spaced = other.replace('-', " ");
                let mut letters = spaced.chars();
                letters.next().map_or_else(String::new, |first| first.to_uppercase().chain(letters).collect())
            }
        }
    }

    /// The main language.
    pub fn primary() -> Role {
        Role("primary".to_owned())
    }

    /// The second language, shown with the main one.
    pub fn translation() -> Role {
        Role("translation".to_owned())
    }

    /// Where the translation of a Bible verse is from, when it is shown apart from the first.
    pub fn translation_reference() -> Role {
        Role("translation-reference".to_owned())
    }
}

/// What an object on a slide is. More kinds (pictures, video) are added here.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", from = "ObjectKindIn")]
pub enum ObjectKind {
    /// A block of text. Line breaks in `text` are kept. `style` holds what this object sets
    /// for itself; the rest of its look comes from the theme. `spans` holds the words that
    /// look different from the rest, by character (see [`crate::TextSpan`]); they are kept
    /// sound, and change with the words only through [`crate::edit_text`] (or
    /// [`Object::replace_text`] and [`Object::set_text`], which call it).
    Text {
        text: String,
        #[serde(default, skip_serializing_if = "TextStyle::is_default")]
        style: TextStyle,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        spans: Vec<crate::TextSpan>,
    },
    /// A drawn shape filling the object's place, in the object's fill colour.
    Shape { shape: Shape },
    /// A picture from a file, in the object's place. `file` is a name in the app's Images
    /// folder, or a whole path.
    Image {
        file: String,
        #[serde(default)]
        fit: ImageFit,
    },
    /// A live web page in the object's place, laid out at the object's size. `url` is an
    /// `https` address (the app opens nothing else).
    Web { url: String },
    /// A live NDI source in the object's place, received while the slide is on screen or being
    /// edited. `source` is the name NDI gives it (`COMPUTER (Source)`); empty until one is chosen.
    Ndi {
        #[serde(default)]
        source: String,
        /// How the picture sits in the box.
        #[serde(default)]
        scaling: ImageFit,
        /// The full stream, or the sender's lighter one.
        #[serde(default)]
        quality: NdiQuality,
        /// Whether the source's sound is wanted too. Kept as a setting; sound isn't played yet.
        #[serde(default, skip_serializing_if = "is_false")]
        sound: bool,
    },
    /// On a stage layout: what one of the audience screens shows, live, scaled into the
    /// object's place. Drawn only on a stage screen (anywhere else it shows nothing, or a
    /// stand-in in the operator's window).
    Screen {
        /// The screen, by its id, so renaming it changes nothing. `None` is the first audience
        /// screen, whichever that is; so is a screen that has gone, or is no longer an audience
        /// screen.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        screen: Option<Uuid>,
        /// How round its corners are, in slide units.
        #[serde(default, skip_serializing_if = "is_zero")]
        corner: f32,
        /// Words in a band across the top of the picture; none when empty.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        label: String,
    },
}

/// What a file holds for [`ObjectKind`]: the same shapes, read, then made sound (a file is not
/// to be trusted). A text object's spans are checked against its words here, so every way of
/// loading an object gets the same care.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ObjectKindIn {
    Text {
        text: String,
        #[serde(default)]
        style: TextStyle,
        #[serde(default, deserialize_with = "crate::TextSpan::lenient")]
        spans: Vec<crate::TextSpan>,
    },
    Shape {
        shape: Shape,
    },
    Image {
        file: String,
        #[serde(default)]
        fit: ImageFit,
    },
    Web {
        url: String,
    },
    Ndi {
        #[serde(default)]
        source: String,
        #[serde(default)]
        scaling: ImageFit,
        #[serde(default)]
        quality: NdiQuality,
        #[serde(default)]
        sound: bool,
    },
    Screen {
        #[serde(default)]
        screen: Option<Uuid>,
        #[serde(default)]
        corner: f32,
        #[serde(default)]
        label: String,
    },
}

impl From<ObjectKindIn> for ObjectKind {
    fn from(read: ObjectKindIn) -> ObjectKind {
        match read {
            ObjectKindIn::Text { text, style, spans } => {
                let spans =
                    if spans.is_empty() { spans } else { crate::TextSpan::normalize(spans, text.chars().count()) };
                ObjectKind::Text { text, style, spans }
            }
            ObjectKindIn::Shape { shape } => ObjectKind::Shape { shape },
            ObjectKindIn::Image { file, fit } => ObjectKind::Image { file, fit },
            ObjectKindIn::Web { url } => ObjectKind::Web { url },
            ObjectKindIn::Ndi { source, scaling, quality, sound } => {
                ObjectKind::Ndi { source, scaling, quality, sound }
            }
            ObjectKindIn::Screen { screen, corner, label } => ObjectKind::Screen { screen, corner, label },
        }
    }
}

/// How an item on a stage layout shows what it shows: which timer, the clock's form, how many
/// lines of the notes. Each part is for the item it names; on any other object it is left as it
/// is, which is how it starts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageShows {
    /// The timer a timer item shows, by its id, running or not. `None` shows the first timer
    /// that is running, and nothing while none is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timer: Option<Uuid>,
    /// The clock on a 24-hour clock, not a 12-hour one.
    #[serde(default, skip_serializing_if = "is_false")]
    pub hours_24: bool,
    /// The clock with its seconds.
    #[serde(default, skip_serializing_if = "is_false")]
    pub seconds: bool,
    /// How many lines of the slide's notes show, from the top; 0 shows them all.
    #[serde(default, skip_serializing_if = "is_zero_lines")]
    pub lines: u8,
}

impl StageShows {
    pub fn is_default(&self) -> bool {
        *self == StageShows::default()
    }
}

fn is_zero_lines(value: &u8) -> bool {
    *value == 0
}

/// How a picture sits in its object's place.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageFit {
    /// All of the picture shows, as large as fits, keeping its shape.
    #[default]
    Fit,
    /// The place is filled, keeping the picture's shape; what hangs over is cut off.
    Fill,
    /// The picture is stretched to the place's shape.
    Stretch,
}

impl ImageFit {
    pub const ALL: [ImageFit; 3] = [ImageFit::Fit, ImageFit::Fill, ImageFit::Stretch];

    pub fn name(self) -> &'static str {
        match self {
            ImageFit::Fit => "Fit",
            ImageFit::Fill => "Fill",
            ImageFit::Stretch => "Stretch",
        }
    }
}

/// Which of an NDI source's streams a slide receives.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NdiQuality {
    /// The full picture.
    #[default]
    Full,
    /// The sender's smaller picture: less of the network and of this computer.
    Light,
}

impl NdiQuality {
    pub const ALL: [NdiQuality; 2] = [NdiQuality::Full, NdiQuality::Light];

    pub fn name(self) -> &'static str {
        match self {
            NdiQuality::Full => "Full",
            NdiQuality::Light => "Light",
        }
    }
}

/// The name of an NDI source without its computer's: `Camera 1` of `STREAM-PC (Camera 1)`. A name
/// not in that shape is its own. The last ` (` begins it, as a computer's own name may hold one
/// (`MacBook Pro (2) (Camera 1)`).
pub fn ndi_short_name(source: &str) -> &str {
    match (source.rfind(" ("), source.strip_suffix(')')) {
        (Some(open), Some(inside)) => &inside[open + 2..],
        _ => source,
    }
}

/// The computer of an NDI source's name: `STREAM-PC` of `STREAM-PC (Camera 1)`.
pub fn ndi_computer(source: &str) -> Option<&str> {
    match (source.rfind(" ("), source.ends_with(')')) {
        (Some(open), true) => Some(&source[..open]),
        _ => None,
    }
}

/// The shapes the editor can draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Shape {
    RoundedRect,
    Rectangle,
    Ellipse,
    Triangle,
    RightTriangle,
    Rhombus,
    Polygon,
    Star,
    DoubleArrow,
    Arrow,
}

impl Shape {
    pub const ALL: [Shape; 10] = [
        Shape::RoundedRect,
        Shape::Rectangle,
        Shape::Ellipse,
        Shape::Triangle,
        Shape::RightTriangle,
        Shape::Rhombus,
        Shape::Polygon,
        Shape::Star,
        Shape::DoubleArrow,
        Shape::Arrow,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Shape::RoundedRect => "Rounded Rect",
            Shape::Rectangle => "Rectangle",
            Shape::Ellipse => "Ellipse",
            Shape::Triangle => "Triangle",
            Shape::RightTriangle => "Right Triangle",
            Shape::Rhombus => "Rhombus",
            Shape::Polygon => "Polygon",
            Shape::Star => "Star",
            Shape::DoubleArrow => "Double Arrow",
            Shape::Arrow => "Arrow",
        }
    }

    /// The shape's outline as points in a box from (0, 0) to (1, 1), for the shapes made of
    /// straight lines. Rounded shapes are drawn as curves instead and give `None`.
    pub fn corners(self) -> Option<Vec<(f64, f64)>> {
        use std::f64::consts::PI;
        let ring = |count: usize, inner: Option<f64>| -> Vec<(f64, f64)> {
            let steps = if inner.is_some() { count * 2 } else { count };
            (0..steps)
                .map(|step| {
                    let radius = match inner {
                        Some(inner) if step % 2 == 1 => inner,
                        _ => 0.5,
                    };
                    let angle = -PI / 2.0 + 2.0 * PI * step as f64 / steps as f64;
                    (0.5 + radius * angle.cos(), 0.5 + radius * angle.sin())
                })
                .collect()
        };
        Some(match self {
            Shape::RoundedRect | Shape::Ellipse => return None,
            Shape::Rectangle => vec![(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)],
            Shape::Triangle => vec![(0.5, 0.0), (1.0, 1.0), (0.0, 1.0)],
            Shape::RightTriangle => vec![(0.0, 0.0), (1.0, 1.0), (0.0, 1.0)],
            Shape::Rhombus => vec![(0.5, 0.0), (1.0, 0.5), (0.5, 1.0), (0.0, 0.5)],
            Shape::Polygon => ring(5, None),
            Shape::Star => ring(5, Some(0.2)),
            Shape::Arrow => vec![(0.0, 0.3), (0.6, 0.3), (0.6, 0.0), (1.0, 0.5), (0.6, 1.0), (0.6, 0.7), (0.0, 0.7)],
            Shape::DoubleArrow => vec![
                (0.0, 0.5),
                (0.3, 0.0),
                (0.3, 0.3),
                (0.7, 0.3),
                (0.7, 0.0),
                (1.0, 0.5),
                (0.7, 1.0),
                (0.7, 0.7),
                (0.3, 0.7),
                (0.3, 1.0),
            ],
        })
    }
}

/// A line drawn round an object's box or shape.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct StrokeLook {
    /// In slide units.
    pub width: f32,
    pub color: Rgba,
}

fn full_opacity() -> f32 {
    1.0
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn is_zero(value: &f32) -> bool {
    *value == 0.0
}

fn is_full(value: &f32) -> bool {
    *value == 1.0
}

/// One thing on a slide. A slide's objects are drawn in order, the first at the bottom.
///
/// The theme decides where an object goes and how it looks unless the object says otherwise:
/// each field here that is `None` means "as the theme has it".
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Object {
    pub id: Uuid,
    /// The name shown in the editor's object list.
    pub name: String,
    /// Which theme style the object follows, and how other parts of the app find it (the main
    /// language, the translation). `None` for an object that belongs to no role.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    /// Where the object sits, in slide units. `None` uses the theme's place for its role.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame: Option<SlideRect>,
    /// Turned clockwise about its centre, in degrees.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub rotation: f32,
    /// From 0 (invisible) to 1 (solid).
    #[serde(default = "full_opacity", skip_serializing_if = "is_full")]
    pub opacity: f32,
    /// A hidden object is kept on the slide but not drawn.
    #[serde(default, skip_serializing_if = "is_false")]
    pub hidden: bool,
    /// A locked object cannot be selected or moved by clicking on the slide.
    #[serde(default, skip_serializing_if = "is_false")]
    pub locked: bool,
    /// The colour the object's box is filled with behind its content. `None` leaves it clear.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<Rgba>,
    /// The fill running from its colour to another.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gradient: Option<crate::Gradient>,
    /// A line round the object's box or shape. `None` draws none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke: Option<StrokeLook>,
    /// A shadow behind the object's box or shape. `None` draws none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow: Option<Shadow>,
    /// Mirrored left to right, and top to bottom, about its centre.
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip_h: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip_v: bool,
    /// How the object comes onto its slide, if it has a build of its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build_in: Option<crate::Build>,
    /// How the object leaves its slide on a click, if it does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build_out: Option<crate::Build>,
    /// For a text object: what it shows instead of its words, if it is linked to something.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<crate::TextLink>,
    /// On a stage layout, for an item such as the clock or a timer: how it shows what it shows.
    #[serde(default, skip_serializing_if = "StageShows::is_default")]
    pub shows: StageShows,
    /// While a build delivers a text object bullet by bullet: how many of its paragraphs show.
    /// Worked out as the slide is drawn, never saved.
    #[serde(skip)]
    pub shown_paragraphs: Option<usize>,
    #[serde(flatten)]
    pub kind: ObjectKind,
}

impl Object {
    /// A text object that follows the theme for its role.
    pub fn text(role: Role, text: impl Into<String>) -> Object {
        let name = role.label();
        Object {
            id: Uuid::new_v4(),
            name,
            role: Some(role),
            frame: None,
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
            shows: StageShows::default(),
            gradient: None,
            shown_paragraphs: None,
            kind: ObjectKind::Text { text: text.into(), style: TextStyle::default(), spans: Vec::new() },
        }
    }

    /// A shape in the middle of the slide, filled light blue as a new shape is.
    pub fn shape(shape: Shape) -> Object {
        Object {
            id: Uuid::new_v4(),
            name: shape.name().to_owned(),
            role: None,
            frame: Some(SlideRect { x: 660.0, y: 340.0, w: 600.0, h: 400.0 }),
            rotation: 0.0,
            opacity: 1.0,
            hidden: false,
            locked: false,
            fill: Some(Rgba::rgb(110, 170, 240)),
            stroke: None,
            shadow: None,
            flip_h: false,
            flip_v: false,
            build_in: None,
            build_out: None,
            link: None,
            shows: StageShows::default(),
            gradient: None,
            shown_paragraphs: None,
            kind: ObjectKind::Shape { shape },
        }
    }

    /// A picture from a file, sized to the picture's shape and centred on the slide.
    pub fn image(file: String, name: String, width: u32, height: u32) -> Object {
        let aspect = width.max(1) as f32 / height.max(1) as f32;
        let (w, h) = if aspect >= 16.0 / 9.0 { (960.0, 960.0 / aspect) } else { (540.0 * aspect, 540.0) };
        Object {
            name,
            frame: Some(SlideRect { x: (crate::SLIDE_WIDTH - w) / 2.0, y: (crate::SLIDE_HEIGHT - h) / 2.0, w, h }),
            fill: None,
            kind: ObjectKind::Image { file, fit: ImageFit::Fit },
            ..Object::shape(Shape::Rectangle)
        }
    }

    /// A web page box, two thirds of the slide, in the middle. With an empty address it shows
    /// nothing until one is typed.
    pub fn web(url: String) -> Object {
        let (w, h) = (crate::SLIDE_WIDTH * 2.0 / 3.0, crate::SLIDE_HEIGHT * 2.0 / 3.0);
        Object {
            name: "Web page".to_owned(),
            frame: Some(SlideRect { x: (crate::SLIDE_WIDTH - w) / 2.0, y: (crate::SLIDE_HEIGHT - h) / 2.0, w, h }),
            fill: None,
            kind: ObjectKind::Web { url },
            ..Object::shape(Shape::Rectangle)
        }
    }

    /// An NDI source box, 16:9, in the middle of the slide. With no source chosen it shows an
    /// empty box until one is.
    pub fn ndi(source: String) -> Object {
        let (w, h) = (1400.0, 788.0);
        Object {
            name: "NDI source".to_owned(),
            frame: Some(SlideRect { x: (crate::SLIDE_WIDTH - w) / 2.0, y: (crate::SLIDE_HEIGHT - h) / 2.0, w, h }),
            fill: None,
            kind: ObjectKind::Ndi { source, scaling: ImageFit::Fit, quality: NdiQuality::Full, sound: false },
            ..Object::shape(Shape::Rectangle)
        }
    }

    /// A picture of what an audience screen shows, for a stage layout: the first audience
    /// screen's, a third of the stage wide, in its top right corner.
    pub fn screen(screen: Option<Uuid>) -> Object {
        let (w, h) = (640.0, 360.0);
        Object {
            name: "Screen".to_owned(),
            frame: Some(SlideRect { x: crate::SLIDE_WIDTH - w - 40.0, y: 130.0, w, h }),
            fill: None,
            kind: ObjectKind::Screen { screen, corner: 0.0, label: String::new() },
            ..Object::shape(Shape::Rectangle)
        }
    }

    /// The object's own text settings, over its theme box.
    pub fn text_style(&self) -> Option<&crate::TextStyle> {
        match &self.kind {
            ObjectKind::Text { style, .. } => Some(style),
            ObjectKind::Shape { .. }
            | ObjectKind::Image { .. }
            | ObjectKind::Web { .. }
            | ObjectKind::Ndi { .. }
            | ObjectKind::Screen { .. } => None,
        }
    }

    /// The object's own text settings, to change; `None` for an object that is not text.
    pub fn text_style_mut(&mut self) -> Option<&mut crate::TextStyle> {
        match &mut self.kind {
            ObjectKind::Text { style, .. } => Some(style),
            ObjectKind::Shape { .. }
            | ObjectKind::Image { .. }
            | ObjectKind::Web { .. }
            | ObjectKind::Ndi { .. }
            | ObjectKind::Screen { .. } => None,
        }
    }

    /// The words of a text object that look different from the rest; none for other objects.
    pub fn spans(&self) -> &[crate::TextSpan] {
        match &self.kind {
            ObjectKind::Text { spans, .. } => spans,
            _ => &[],
        }
    }

    /// Replaces the characters from `start` to `end` of a text object's words with `with`,
    /// moving its spans to follow ([`crate::edit_text`]). Does nothing to other objects.
    pub fn replace_text(&mut self, start: usize, end: usize, with: &str) {
        if let ObjectKind::Text { text, spans, .. } = &mut self.kind {
            crate::edit_text(text, spans, start, end, with);
        }
    }

    /// Makes a text object's words `new`, by the smallest change that gets there (or, if it
    /// has as many characters as before, letter for letter), so the spans on the words that
    /// stay stay with them. Every writer of a text object's words that has
    /// the whole new text, not an edit, comes here. Does nothing to other objects.
    pub fn set_text(&mut self, new: &str) {
        let ObjectKind::Text { text, spans, .. } = &mut self.kind else { return };
        if spans.is_empty() {
            if text != new {
                new.clone_into(text);
            }
        } else if text.chars().count() == new.chars().count() {
            // Letters swapped for letters (capitals, a corrected word): each stays as styled as
            // it was.
            new.clone_into(text);
        } else if let Some((start, end, with)) = crate::smallest_edit(text, new) {
            crate::edit_text(text, spans, start, end, with);
        }
    }

    pub fn as_text(&self) -> Option<&str> {
        match &self.kind {
            ObjectKind::Text { text, .. } => Some(text),
            ObjectKind::Shape { .. }
            | ObjectKind::Image { .. }
            | ObjectKind::Web { .. }
            | ObjectKind::Ndi { .. }
            | ObjectKind::Screen { .. } => None,
        }
    }
}

impl Slide {
    /// Replaces the words of the main and translation text. The objects keep their ids, places
    /// and looks. An empty text removes that object; a missing one is added.
    pub fn set_words(&mut self, primary: &str, translation: &str) {
        // Main goes below the translation when it has to be added.
        for (position, role, text) in [(0, Role::primary(), primary), (usize::MAX, Role::translation(), translation)] {
            let text = crate::clean_box_text(text);
            let existing = self.objects.iter().position(|o| o.role.as_ref() == Some(&role));
            match (existing, text.is_empty()) {
                (Some(at), true) => {
                    self.objects.remove(at);
                }
                (Some(at), false) => {
                    // Only the words change; the object keeps the look it was given.
                    self.objects[at].set_text(&text);
                }
                (None, false) => {
                    let at = position.min(self.objects.len());
                    self.objects.insert(at, Object::text(role, text));
                }
                (None, true) => {}
            }
        }
    }
}

fn enabled_by_default() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Slide {
    pub id: Uuid,
    /// Everything on the slide, bottom first.
    pub objects: Vec<Object>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default = "enabled_by_default")]
    pub enabled: bool,
    /// The theme template the slide is drawn with, by its id. `None` lets the theme choose
    /// the template that suits what is on the slide.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<Uuid>,
    /// How the screens change to this slide; `None` uses the transition chosen for all slides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transition: Option<crate::Transition>,
    /// What the slide does when it goes on screen, in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<crate::ShowAction>,
    /// A letter or digit that puts this slide on screen in the Show view, before any group's
    /// letter. Kept in capitals.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hot_key: Option<char>,
    /// Goes on to the next slide by itself this many seconds after it is put on screen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub advance: Option<f32>,
    /// Why this slide, brought in from a presentation file, needs a look before it is used.
    /// Kept with the slide, so it survives a restart, until the person marks it as checked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check: Option<String>,
}

impl Slide {
    pub fn new(objects: Vec<Object>) -> Slide {
        Slide {
            id: Uuid::new_v4(),
            objects,
            label: None,
            notes: None,
            enabled: true,
            template: None,
            transition: None,
            actions: Vec::new(),
            hot_key: None,
            advance: None,
            check: None,
        }
    }

    /// A new slide laid out like this one (the same boxes, in the same places, with the same
    /// look) with no words in them, drawn with the same template. Shapes, pictures and web
    /// pages come as they are.
    pub fn empty_copy(&self) -> Slide {
        let objects = self
            .objects
            .iter()
            .map(|object| {
                let kind = match &object.kind {
                    ObjectKind::Text { style, .. } => {
                        ObjectKind::Text { text: String::new(), style: style.clone(), spans: Vec::new() }
                    }
                    other @ (ObjectKind::Shape { .. }
                    | ObjectKind::Image { .. }
                    | ObjectKind::Web { .. }
                    | ObjectKind::Ndi { .. }
                    | ObjectKind::Screen { .. }) => other.clone(),
                };
                Object { id: Uuid::new_v4(), kind, ..object.clone() }
            })
            .collect();
        Slide { template: self.template, ..Slide::new(objects) }
    }

    /// The text of the first text object with this role.
    pub fn text_for(&self, role: &Role) -> Option<&str> {
        self.objects.iter().find(|o| o.role.as_ref() == Some(role)).and_then(Object::as_text)
    }

    /// Each text object's role and words, in order. Two slides with the same words give the
    /// same list whatever their ids.
    pub fn texts(&self) -> Vec<(Option<Role>, String)> {
        self.objects.iter().filter_map(|o| Some((o.role.clone(), o.as_text()?.to_owned()))).collect()
    }
}

/// A named run of slides, such as "Verse 1" or "Chorus". Names may repeat in a presentation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Group {
    pub id: Uuid,
    pub name: String,
    pub color: Rgba,
    pub slides: Vec<Slide>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Presentation {
    pub id: Uuid,
    pub title: String,
    pub groups: Vec<Group>,
    /// Who wrote the song and who holds its rights, for the copyright line.
    #[serde(default, skip_serializing_if = "Copyright::is_empty")]
    pub copyright: Copyright,
    /// The theme the song is drawn with, by its id; `None` uses the theme songs use.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<Uuid>,
    /// Orders of the song's groups to play it in, such as verse, chorus, verse, chorus.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub arrangements: Vec<Arrangement>,
    /// The arrangement the song is played in, by its id; `None` plays the groups as written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arrangement: Option<Uuid>,
    /// Next on the last slide goes back to the first, for a loop of announcements.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub loops: bool,
    /// How each slide gives way to the next, unless a slide has its own; `None` for the one
    /// chosen for all.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transition: Option<crate::Transition>,
    /// Where the song came from when it was copied out of the song catalog or another outside
    /// source (TimelyChurch); `None` for the church's own songs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalog: Option<Box<CatalogOrigin>>,
}

/// What a song copied out of the song catalog, or made from another outside source such as
/// TimelyChurch, remembers of where it came from, so a later update can tell a song nobody has
/// touched from one the church has changed. Whoever reads it checks `source` first.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogOrigin {
    /// The source it came from (`worshipleaderapp`, `timelychurch`).
    pub source: String,
    /// The source's key for the song (see `catalog::song_key`, `timelychurch::song_key`).
    pub key: String,
    /// The language the song is in at the source (`ru`, `uk`…); empty when the source doesn't
    /// say.
    pub language: String,
    /// The language of the translation that was put under the song's words, if one was.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub second: Option<String>,
    /// A fingerprint of the catalog's text the copy was made from.
    pub text_hash: String,
    /// A fingerprint of the song as the catalog last wrote it: the song is unchanged while it
    /// still has this fingerprint, and any edit gives it another.
    pub made_hash: String,
    /// When the copy was made, as a date (year-month-day).
    pub added: String,
    /// The fingerprint of a newer version of the song's text, which the catalog has and the
    /// church's changed copy has not taken.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub newer: Option<String>,
    /// The catalog no longer has the song.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub gone: bool,
}

/// An order to play a song's groups in. A group can come more than once, and a group left out
/// is not played.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Arrangement {
    pub id: Uuid,
    pub name: String,
    /// The groups, by their ids, in the order they are played.
    pub groups: Vec<Uuid>,
}

/// A piece of an arrangement: a group sung once, or several times running (shown as one pill
/// marked ×2, ×3…). It is stored as the group listed that many times.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Run {
    pub group: Uuid,
    /// Where it starts in the arrangement's list of groups, and how many times it is listed.
    pub start: usize,
    pub count: usize,
}

/// The most times running a group can be sung in one piece.
pub const MOST_TIMES: usize = 20;

impl Arrangement {
    /// Its pieces in order: each group listed several times running is one piece.
    pub fn runs(&self) -> Vec<Run> {
        let mut runs: Vec<Run> = Vec::new();
        for (at, group) in self.groups.iter().enumerate() {
            match runs.last_mut() {
                Some(last) if last.group == *group => last.count += 1,
                _ => runs.push(Run { group: *group, start: at, count: 1 }),
            }
        }
        runs
    }

    /// Sets how many times running the piece starting at `start` is sung (1 to 20).
    pub fn set_times(&mut self, start: usize, times: usize) {
        let Some(run) = self.runs().into_iter().find(|run| run.start == start) else { return };
        let times = times.clamp(1, MOST_TIMES);
        self.groups.splice(run.start..run.start + run.count, std::iter::repeat_n(run.group, times));
    }

    /// Takes the piece starting at `start` out.
    pub fn remove_run(&mut self, start: usize) {
        if let Some(run) = self.runs().into_iter().find(|run| run.start == start) {
            self.groups.drain(run.start..run.start + run.count);
        }
    }

    /// Moves the piece starting at `from` to just before (or, with `after`, just after) the piece
    /// starting at `to`. Two pieces of the same group that come together are one piece after.
    pub fn move_run(&mut self, from: usize, to: usize, after: bool) {
        let mut runs = self.runs();
        let Some(moving) = runs.iter().position(|run| run.start == from) else { return };
        if !runs.iter().any(|run| run.start == to) || from == to {
            return;
        }
        let piece = runs.remove(moving);
        let Some(target) = runs.iter().position(|run| run.start == to) else { return };
        runs.insert(target + usize::from(after), piece);
        self.groups = runs.iter().flat_map(|run| std::iter::repeat_n(run.group, run.count)).collect();
    }
}

/// One slide of a song as it is sung: its place among the song's slides, its place in the
/// order sung, and whether it was sung earlier in that order (the same slide, again).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sung {
    pub slide: usize,
    pub step: usize,
    pub again: bool,
}

/// Where the slide at a place in the order sung stands in its arrangement: the piece, and which
/// time through it (from 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pass {
    pub run: usize,
    pub time: usize,
}

impl Presentation {
    /// With an arrangement, the slides in the order sung (a group sung again comes again, as
    /// the same slides), and the slides of the groups it leaves out, in their written order.
    /// `None` when the song is sung as written.
    pub fn sung(&self) -> Option<(Vec<Sung>, Vec<usize>)> {
        self.active_arrangement()?;
        let order = self.order();
        let mut seen = vec![false; self.slide_count()];
        let sung = order
            .iter()
            .enumerate()
            .map(|(step, slide)| {
                let again = seen.get(*slide).copied().unwrap_or(false);
                if let Some(seen) = seen.get_mut(*slide) {
                    *seen = true;
                }
                Sung { slide: *slide, step, again }
            })
            .collect();
        let left_out = (0..self.slide_count()).filter(|slide| !seen[*slide]).collect();
        Some((sung, left_out))
    }

    /// The first place in the order sung of each piece of the arrangement in use, and how many
    /// places one time through it takes (its group's slides).
    pub fn run_steps(&self) -> Vec<(usize, usize)> {
        let Some(arrangement) = self.active_arrangement() else { return Vec::new() };
        let length = |group: &Uuid| self.groups.iter().find(|have| have.id == *group).map_or(0, |g| g.slides.len());
        let mut step = 0;
        arrangement
            .runs()
            .iter()
            .map(|run| {
                let len = length(&run.group);
                let first = step;
                step += len * run.count;
                (first, len)
            })
            .collect()
    }

    /// Which piece of the arrangement the place `step` in the order sung is in, and which time
    /// through it.
    pub fn pass_at(&self, step: usize) -> Option<Pass> {
        let runs = self.active_arrangement()?.runs();
        self.run_steps().iter().zip(&runs).enumerate().find_map(|(at, ((first, len), run))| {
            let end = first + len * run.count;
            (*len > 0 && step >= *first && step < end).then(|| Pass { run: at, time: (step - first) / len + 1 })
        })
    }
}

/// A song's copyright details, as a CCLI report asks for them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Copyright {
    pub artist: String,
    pub author: String,
    pub publisher: String,
    pub year: String,
    /// The song's number in the CCLI catalogue.
    pub ccli_song: String,
}

impl Copyright {
    pub fn is_empty(&self) -> bool {
        *self == Copyright::default()
    }

    /// The copyright line shown on the screens: "Title" by the author, the year and publisher,
    /// the CCLI song number, and the church's CCLI licence number. Parts not known are left out.
    pub fn line(&self, title: &str, licence: &str) -> String {
        let mut parts = vec![format!("\u{201c}{title}\u{201d}")];
        let writer = if self.author.is_empty() { &self.artist } else { &self.author };
        if !writer.is_empty() {
            parts.push(writer.clone());
        }
        let held =
            [self.year.as_str(), self.publisher.as_str()].iter().filter(|p| !p.is_empty()).copied().collect::<Vec<_>>();
        if !held.is_empty() {
            parts.push(format!("\u{a9} {}", held.join(" ")));
        }
        if !self.ccli_song.is_empty() {
            parts.push(format!("CCLI Song # {}", self.ccli_song));
        }
        if !licence.is_empty() {
            parts.push(format!("CCLI License # {licence}"));
        }
        parts.join("  \u{b7}  ")
    }
}

impl Presentation {
    pub fn slide_count(&self) -> usize {
        self.groups.iter().map(|g| g.slides.len()).sum()
    }

    /// The arrangement the song is played in, if one is chosen and still there.
    pub fn active_arrangement(&self) -> Option<&Arrangement> {
        let id = self.arrangement?;
        self.arrangements.iter().find(|arrangement| arrangement.id == id)
    }

    /// The slides, by their places, in the order they are played: the chosen arrangement's
    /// groups in turn, or the groups as written when no arrangement is chosen. A group the song
    /// no longer has is passed over; if none of the arrangement's groups is left, the groups
    /// are played as written.
    pub fn order(&self) -> Vec<usize> {
        let mut starts = Vec::with_capacity(self.groups.len());
        let mut at = 0;
        for group in &self.groups {
            starts.push(at);
            at += group.slides.len();
        }
        let written = || (0..at).collect::<Vec<usize>>();
        let Some(arrangement) = self.active_arrangement() else { return written() };
        let order: Vec<usize> = arrangement
            .groups
            .iter()
            .filter_map(|id| self.groups.iter().position(|group| group.id == *id))
            .flat_map(|index| starts[index]..starts[index] + self.groups[index].slides.len())
            .collect();
        if order.is_empty() { written() } else { order }
    }

    /// A new arrangement with the groups as written, named `named(n)` with the first number from
    /// 1 that no arrangement of the song has (the app gives the name in its language, such as
    /// "Arrangement 2").
    pub fn new_arrangement(&self, named: impl Fn(usize) -> String) -> Arrangement {
        let taken = |name: &str| self.arrangements.iter().any(|arrangement| arrangement.name == name);
        let name = (1..).map(named).find(|name| !taken(name)).unwrap_or_default();
        Arrangement { id: Uuid::new_v4(), name, groups: self.groups.iter().map(|group| group.id).collect() }
    }

    /// The slide at a flat index across all groups, in order, with its group.
    pub fn slide(&self, index: usize) -> Option<(&Group, &Slide)> {
        let mut remaining = index;
        for group in &self.groups {
            if let Some(slide) = group.slides.get(remaining) {
                return Some((group, slide));
            }
            remaining -= group.slides.len();
        }
        None
    }

    /// A copy with the words of one slide's main and translation text replaced. The slide and
    /// its objects keep their ids, places and everything else. An empty text removes that
    /// object; if the slide would be left with no objects, or there is no such slide, returns
    /// `None`.
    pub fn with_slide_text(&self, index: usize, primary: &str, translation: &str) -> Option<Presentation> {
        let mut copy = self.clone();
        let mut remaining = index;
        for group in &mut copy.groups {
            if let Some(slide) = group.slides.get_mut(remaining) {
                slide.set_words(primary, translation);
                return (!slide.objects.is_empty()).then_some(copy);
            }
            remaining -= group.slides.len();
        }
        None
    }

    /// The song rebuilt from its words as text (the song text format: `[Group]` lines, a blank
    /// line between slides, `--` before a translation), as the Reflow editor makes it. A slide
    /// at a place the song already had keeps everything but its words: its looks, label,
    /// notes, template and shapes. A group named as before keeps its colour.
    pub fn reflowed(&self, text: &str) -> Presentation {
        let parsed = crate::parse_song_text(&self.title, text);
        let before: Vec<&Slide> = self.slides().map(|(_, slide)| slide).collect();
        let mut index = 0;
        let mut groups = parsed.groups;
        // Each group takes the id of the first group before it of the same name not yet taken,
        // so arrangements keep their groups when one is added, removed or moved.
        let mut taken = vec![false; self.groups.len()];
        for group in &mut groups {
            if let Some(same) = self.groups.iter().find(|old| old.name == group.name) {
                group.color = same.color;
            }
            if let Some(at) = self.groups.iter().zip(&taken).position(|(old, used)| !used && old.name == group.name) {
                taken[at] = true;
                group.id = self.groups[at].id;
            }
            for slide in &mut group.slides {
                if let Some(old) = before.get(index) {
                    let words = |role: Role| slide.text_for(&role).unwrap_or("").to_owned();
                    let (primary, translation) = (words(Role::primary()), words(Role::translation()));
                    let mut kept = (*old).clone();
                    kept.set_words(&primary, &translation);
                    if !kept.objects.is_empty() {
                        *slide = kept;
                    }
                }
                index += 1;
            }
        }
        Presentation {
            id: self.id,
            title: self.title.clone(),
            groups,
            copyright: self.copyright.clone(),
            theme: self.theme,
            arrangements: self.arrangements.clone(),
            arrangement: self.arrangement,
            loops: self.loops,
            transition: self.transition,
            catalog: self.catalog.clone(),
        }
    }

    /// A copy with one slide replaced by an edited version of it. Returns `None` if there is no
    /// slide at that position.
    pub fn with_slide(&self, index: usize, slide: Slide) -> Option<Presentation> {
        let mut copy = self.clone();
        let mut remaining = index;
        for group in &mut copy.groups {
            if let Some(slot) = group.slides.get_mut(remaining) {
                *slot = slide;
                return Some(copy);
            }
            remaining -= group.slides.len();
        }
        None
    }

    /// The group a slide is in, and where in that group, by its place in the whole song.
    fn locate(&self, index: usize) -> Option<(usize, usize)> {
        let mut remaining = index;
        for (at, group) in self.groups.iter().enumerate() {
            if remaining < group.slides.len() {
                return Some((at, remaining));
            }
            remaining -= group.slides.len();
        }
        None
    }

    /// A copy with the slides at `moving` taken out and put, in the order they were in, just
    /// before the slide at `to` (or just after it, with `after`), in that slide's group. A group
    /// left with no slides goes, and so does its place in the arrangements. Returns the copy and,
    /// for each slide's place before, its place after. `None` if a place is not a slide, `to` is
    /// one of those moving, or nothing would move.
    pub fn with_slides_moved(&self, moving: &[usize], to: usize, after: bool) -> Option<(Presentation, Vec<usize>)> {
        let count = self.slide_count();
        if moving.is_empty() || to >= count || moving.contains(&to) || moving.iter().any(|at| *at >= count) {
            return None;
        }
        // Every slide, by its place before, in its group.
        let mut place = 0;
        let mut groups: Vec<Vec<usize>> = Vec::new();
        for group in &self.groups {
            groups.push((place..place + group.slides.len()).collect());
            place += group.slides.len();
        }
        let mut carried: Vec<usize> = moving.to_vec();
        carried.sort_unstable();
        carried.dedup();
        for group in &mut groups {
            group.retain(|at| !carried.contains(at));
        }
        let (target, at) = groups.iter().enumerate().find_map(|(g, slides)| {
            slides.iter().position(|have| *have == to).map(|at| (g, at + usize::from(after)))
        })?;
        groups[target].splice(at..at, carried.iter().copied());
        let order: Vec<usize> = groups.iter().flatten().copied().collect();
        if order.iter().enumerate().all(|(new, old)| new == *old) {
            return None;
        }
        // The slides themselves, group by group; a group left empty goes.
        let slides: Vec<&Slide> = self.groups.iter().flat_map(|group| &group.slides).collect();
        let mut copy = self.clone();
        let mut gone = Vec::new();
        for (group, places) in copy.groups.iter_mut().zip(&groups) {
            group.slides = places.iter().map(|old| slides[*old].clone()).collect();
            if group.slides.is_empty() {
                gone.push(group.id);
            }
        }
        copy.groups.retain(|group| !group.slides.is_empty());
        for arrangement in &mut copy.arrangements {
            arrangement.groups.retain(|id| !gone.contains(id));
        }
        let mut new_of_old = vec![0; count];
        for (new, old) in order.iter().enumerate() {
            new_of_old[*old] = new;
        }
        Some((copy, new_of_old))
    }

    /// A copy without one slide. A group left empty goes too. `None` if there is no such slide.
    pub fn without_slide(&self, index: usize) -> Option<Presentation> {
        let (group, at) = self.locate(index)?;
        let mut copy = self.clone();
        copy.groups[group].slides.remove(at);
        if copy.groups[group].slides.is_empty() {
            copy.groups.remove(group);
        }
        Some(copy)
    }

    /// A copy with a slide put just after the one at `index`, in the same group, given new ids
    /// so it is a slide of its own. `None` if there is no slide there.
    pub fn with_slide_after(&self, index: usize, slide: &Slide) -> Option<Presentation> {
        let (group, at) = self.locate(index)?;
        let mut copy = self.clone();
        let mut fresh = slide.clone();
        fresh.id = Uuid::new_v4();
        for object in &mut fresh.objects {
            object.id = Uuid::new_v4();
        }
        copy.groups[group].slides.insert(at + 1, fresh);
        Some(copy)
    }

    /// A copy with slides put, in order, just after the one at `index`, in the same group, each
    /// given new ids so it is a slide of its own. `None` if there is no slide there.
    pub fn with_slides_after(&self, index: usize, slides: &[Slide]) -> Option<Presentation> {
        let mut copy = self.clone();
        for (at, slide) in slides.iter().enumerate() {
            copy = copy.with_slide_after(index + at, slide)?;
        }
        Some(copy)
    }

    /// A copy where these slides (by their places) are a group of this name, as Group does for
    /// several slides picked together: one slide alone is [`with_group_starting`]; with several,
    /// each run of them in a row becomes a group of that name, and the slide after a run goes on
    /// in the group it was in. `None` if a place has no slide.
    ///
    /// [`with_group_starting`]: Self::with_group_starting
    pub fn with_group_over(&self, slides: &[usize], name: &str) -> Option<Presentation> {
        let mut places = slides.to_vec();
        places.sort_unstable();
        places.dedup();
        if let [one] = places.as_slice() {
            return self.with_group_starting(*one, name);
        }
        let mut copy = self.clone();
        // Runs of slides in a row, from the last, so each split leaves the earlier places as
        // they were.
        let mut runs: Vec<(usize, usize)> = Vec::new();
        for at in places {
            match runs.last_mut() {
                Some((_, end)) if *end + 1 == at => *end = at,
                _ => runs.push((at, at)),
            }
        }
        for (start, end) in runs.into_iter().rev() {
            let (group, _) = copy.locate(end)?;
            let was = copy.groups[group].name.clone();
            if copy.locate(end + 1).is_some_and(|(after, _)| after == group) {
                copy = copy.with_group_starting(end + 1, &was)?;
            }
            // The run's first slide, and each slide in it that starts a group of its own.
            for at in (start..=end).rev() {
                if at == start || copy.locate(at)?.1 == 0 {
                    copy = copy.with_group_starting(at, name)?;
                }
            }
        }
        Some(copy)
    }

    /// A copy where a group with this name starts at the slide at `index`: the slide and the
    /// rest of its group become a group of that name, in its colour. A slide that already
    /// starts its group renames the group. `None` if there is no slide there.
    pub fn with_group_starting(&self, index: usize, name: &str) -> Option<Presentation> {
        let (group, at) = self.locate(index)?;
        let mut copy = self.clone();
        let color = crate::group_color(name);
        if at == 0 {
            let renamed = &mut copy.groups[group];
            renamed.name = name.to_owned();
            renamed.color = color;
        } else {
            let tail = copy.groups[group].slides.split_off(at);
            let started = Group { id: Uuid::new_v4(), name: name.to_owned(), color, slides: tail };
            // Wherever an arrangement plays the group that was split, it plays the new one after.
            let (split, new) = (copy.groups[group].id, started.id);
            for arrangement in &mut copy.arrangements {
                arrangement.groups = arrangement
                    .groups
                    .iter()
                    .flat_map(|id| if *id == split { vec![*id, new] } else { vec![*id] })
                    .collect();
            }
            copy.groups.insert(group + 1, started);
        }
        Some(copy)
    }

    /// A copy with a new, empty slide at the end, and that slide's position. The new slide is
    /// laid out like the one before it (the same boxes, in the same places, with the same
    /// look) with no words in them, and joins the last group. A song with no slides gets a
    /// first one with a main-language box.
    pub fn with_slide_added(&self) -> (Presentation, usize) {
        let mut copy = self.clone();
        let slide = match copy.groups.iter().rev().find_map(|group| group.slides.last()) {
            Some(last) => last.empty_copy(),
            None => Slide::new(vec![Object::text(Role::primary(), "")]),
        };
        if copy.groups.is_empty() {
            copy.groups.push(Group {
                id: Uuid::new_v4(),
                name: String::new(),
                color: Rgba::rgb(128, 128, 128),
                slides: Vec::new(),
            });
        }
        if let Some(group) = copy.groups.last_mut() {
            group.slides.push(slide);
        }
        let index = copy.slide_count() - 1;
        (copy, index)
    }

    /// Every slide with its group, in order.
    pub fn slides(&self) -> impl Iterator<Item = (&Group, &Slide)> {
        self.groups.iter().flat_map(|g| g.slides.iter().map(move |s| (g, s)))
    }
}

/// A picture or video that can be shown behind the words, and how it is shown. Only what
/// identifies it is kept here; the pixels are loaded by the app.
#[derive(Clone, Debug, PartialEq)]
pub struct MediaItem {
    pub id: Uuid,
    /// The file's name, shown under its thumbnail.
    pub name: String,
    pub path: std::path::PathBuf,
    /// How it fills the screen: cropped to fill it (the usual), fitted inside it, or stretched.
    pub fit: ImageFit,
    /// A video starts again at its end; one that doesn't stays on its last picture.
    pub looping: bool,
    /// Which of the sample backgrounds the app comes with this is, by the file name it came as
    /// ("Olive.jpg"); `None` for any other file. It stays when the file is renamed.
    pub sample: Option<String>,
}

impl MediaItem {
    /// A file in the media bin, filling the screen and, for a video, looping.
    pub fn new(name: String, path: std::path::PathBuf) -> MediaItem {
        MediaItem { id: Uuid::new_v4(), name, path, fit: ImageFit::Fill, looping: true, sample: None }
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn several_slides_paste_in_order_after_one_as_slides_of_their_own() {
        let song = crate::parse_song_text("Song", "One\n\nTwo\n\nThree\n");
        let copies: Vec<Slide> = song.slides().take(2).map(|(_, slide)| slide.clone()).collect();
        let pasted = song.with_slides_after(2, &copies).unwrap();
        let words: Vec<&str> = pasted.slides().map(|(_, s)| s.text_for(&Role::primary()).unwrap_or("")).collect();
        assert_eq!(words, ["One", "Two", "Three", "One", "Two"]);
        assert_ne!(pasted.slide(3).unwrap().1.id, song.slide(0).unwrap().1.id, "new ids");
        assert!(song.with_slides_after(9, &copies).is_none());
    }

    #[test]
    fn slides_move_into_the_group_of_the_slide_they_are_dropped_by() {
        let words = |text: &str| Slide::new(vec![Object::text(Role::primary(), text.to_owned())]);
        let group = |name: &str, texts: &[&str]| Group {
            id: Uuid::new_v4(),
            name: name.to_owned(),
            color: crate::group_color(name),
            slides: texts.iter().map(|t| words(t)).collect(),
        };
        let mut song = Presentation {
            id: Uuid::new_v4(),
            title: "Song".to_owned(),
            groups: vec![group("Verse 1", &["a", "b"]), group("Chorus", &["c"]), group("Verse 2", &["d", "e"])],
            copyright: Copyright::default(),
            theme: None,
            arrangements: Vec::new(),
            arrangement: None,
            loops: false,
            transition: None,
            catalog: None,
        };
        let verse = song.groups[0].id;
        let chorus = song.groups[1].id;
        song.arrangements =
            vec![Arrangement { id: Uuid::new_v4(), name: "As sung".to_owned(), groups: vec![verse, chorus, verse] }];
        let texts = |song: &Presentation| -> Vec<(String, String)> {
            song.groups
                .iter()
                .flat_map(|g| {
                    g.slides
                        .iter()
                        .map(move |s| (g.name.clone(), s.text_for(&Role::primary()).unwrap_or("").to_owned()))
                })
                .collect()
        };
        // One slide, after another in a later group: it joins that group.
        let (moved, places) = song.with_slides_moved(&[0], 3, true).unwrap();
        assert_eq!(
            texts(&moved),
            [("Verse 1", "b"), ("Chorus", "c"), ("Verse 2", "d"), ("Verse 2", "a"), ("Verse 2", "e")]
                .map(|(g, t)| (g.to_owned(), t.to_owned()))
        );
        assert_eq!(places, [3, 0, 1, 2, 4]);
        // Two picked slides together, before the first: they keep their order.
        let (moved, places) = song.with_slides_moved(&[4, 2], 0, false).unwrap();
        assert_eq!(texts(&moved).iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>(), ["c", "e", "a", "b", "d"]);
        assert_eq!(places, [2, 3, 0, 4, 1]);
        // The Chorus emptied goes, from the groups and from the arrangement.
        assert_eq!(moved.groups.len(), 2);
        assert_eq!(moved.arrangements[0].groups, [verse, verse]);
        // Nothing to do: dropped where it already is, or onto itself.
        assert!(song.with_slides_moved(&[1], 2, false).is_none());
        assert!(song.with_slides_moved(&[1], 1, true).is_none());
        assert!(song.with_slides_moved(&[9], 1, true).is_none());
    }

    use super::*;

    #[test]
    fn a_reflowed_song_keeps_each_slides_look_and_takes_the_new_words() {
        let mut song = crate::parse_song_text("Song", "[Verse 1]\nA\n--\nА\n\nB\n\n[Chorus]\nC\n");
        song.groups[0].slides[0].label = Some("Repeat".to_owned());
        let crate::ObjectKind::Text { style, .. } = &mut song.groups[0].slides[0].objects[0].kind else { panic!() };
        style.size = Some(40.0);
        let text =
            crate::to_song_text(&song).replace("A\n", "A changed\n").replace("[Chorus]\nC\n", "[Chorus]\nC\n\nD\n");
        let again = song.reflowed(&text);
        assert_eq!(again.slide_count(), 4);
        let (_, first) = again.slide(0).unwrap();
        assert_eq!(first.text_for(&Role::primary()), Some("A changed"));
        assert_eq!(first.label.as_deref(), Some("Repeat"));
        assert_eq!(first.id, song.slide(0).unwrap().1.id, "the slide keeps its id");
        let crate::ObjectKind::Text { style, .. } = &first.objects[0].kind else { panic!() };
        assert_eq!(style.size, Some(40.0), "and its look");
        assert_eq!(again.slide(3).unwrap().1.text_for(&Role::primary()), Some("D"));
        // Unchanged text gives the same song.
        assert_eq!(song.reflowed(&crate::to_song_text(&song)), song);
    }

    #[test]
    fn the_copyright_line_leaves_out_what_is_not_known() {
        let full = Copyright {
            artist: "The Band".to_owned(),
            author: "A. Writer".to_owned(),
            publisher: "Songs Ltd".to_owned(),
            year: "2024".to_owned(),
            ccli_song: "1234567".to_owned(),
        };
        assert_eq!(
            full.line("Steady Ground", "998877"),
            "\u{201c}Steady Ground\u{201d}  \u{b7}  A. Writer  \u{b7}  \u{a9} 2024 Songs Ltd  \u{b7}  CCLI Song # 1234567  \u{b7}  CCLI License # 998877"
        );
        let little = Copyright { artist: "The Band".to_owned(), ..Copyright::default() };
        assert_eq!(little.line("Song", ""), "\u{201c}Song\u{201d}  \u{b7}  The Band");
        assert!(Copyright::default().is_empty());
    }

    #[test]
    fn slides_are_removed_copied_in_and_start_groups() {
        let song = crate::parse_song_text("Song", "[Verse 1]\nA\n\nB\n\nC\n\n[Chorus]\nD\n");
        let words = |song: &Presentation| {
            song.slides()
                .map(|(g, s)| format!("{}:{}", g.name, s.objects[0].as_text().unwrap_or("")))
                .collect::<Vec<_>>()
        };
        // Removing the chorus's only slide removes the chorus.
        let fewer = song.without_slide(3).unwrap();
        assert_eq!(words(&fewer), ["Verse 1:A", "Verse 1:B", "Verse 1:C"]);
        assert!(song.without_slide(9).is_none());
        // A copied slide goes in after another, in its group, with ids of its own.
        let copied = song.slide(0).unwrap().1.clone();
        let more = song.with_slide_after(1, &copied).unwrap();
        assert_eq!(words(&more), ["Verse 1:A", "Verse 1:B", "Verse 1:A", "Verse 1:C", "Chorus:D"]);
        assert_ne!(more.slide(2).unwrap().1.id, copied.id);
        // Marking B as the start of a Pre-chorus splits the verse there.
        let split = song.with_group_starting(1, "Pre-chorus").unwrap();
        assert_eq!(words(&split), ["Verse 1:A", "Pre-chorus:B", "Pre-chorus:C", "Chorus:D"]);
        assert_eq!(split.groups[1].color, crate::group_color("Pre-chorus"));
        // On a group's first slide it renames the group.
        let renamed = song.with_group_starting(3, "Bridge").unwrap();
        assert_eq!(words(&renamed), ["Verse 1:A", "Verse 1:B", "Verse 1:C", "Bridge:D"]);
        // Several picked together: just they become the group, and the verse goes on after.
        let over = song.with_group_over(&[1], "Pre-chorus").unwrap();
        assert_eq!(words(&over), words(&split), "one alone starts the group, as before");
        let over = song.with_group_over(&[1, 0], "Tag").unwrap();
        assert_eq!(words(&over), ["Tag:A", "Tag:B", "Verse 1:C", "Chorus:D"]);
        let over = song.with_group_over(&[0, 2, 3], "Tag").unwrap();
        assert_eq!(words(&over), ["Tag:A", "Verse 1:B", "Tag:C", "Tag:D"]);
        assert!(song.with_group_over(&[1, 9], "Tag").is_none());
    }

    #[test]
    fn a_slide_added_at_the_end_is_laid_out_like_the_one_before_with_no_words() {
        let song = crate::parse_song_text("Song", "[Verse 1]\nOne\n--\nОдин\n\n[Chorus]\nTwo\n--\nДва\n");
        let (longer, at) = song.with_slide_added();
        assert_eq!((longer.slide_count(), at), (3, 2));
        let (group, added) = longer.slide(at).unwrap();
        assert_eq!(group.name, "Chorus");
        let (_, before) = longer.slide(1).unwrap();
        assert_eq!(added.objects.len(), before.objects.len());
        for (new, old) in added.objects.iter().zip(&before.objects) {
            assert_eq!((new.role.clone(), new.frame, new.as_text()), (old.role.clone(), old.frame, Some("")));
            assert_ne!(new.id, old.id);
        }
        // A song with nothing in it gets a first slide with a main box.
        let empty = Presentation {
            id: Uuid::new_v4(),
            title: "Empty".to_owned(),
            groups: Vec::new(),
            copyright: Copyright::default(),
            theme: None,
            arrangements: Vec::new(),
            arrangement: None,
            loops: false,
            transition: None,
            catalog: None,
        };
        let (first, at) = empty.with_slide_added();
        assert_eq!((first.slide_count(), at), (1, 0));
        assert_eq!(first.slide(0).unwrap().1.objects[0].role, Some(Role::primary()));
    }
}

#[cfg(test)]
mod arrangement_tests {
    use super::*;

    /// Verse 1 (two slides), Chorus (one), Verse 2 (one).
    fn song() -> Presentation {
        crate::parse_song_text("Song", "[Verse 1]\nA\n\nB\n\n[Chorus]\nC\n\n[Verse 2]\nD\n")
    }

    #[test]
    fn an_arrangement_plays_its_groups_in_its_order_and_repeats_them() {
        let mut song = song();
        assert_eq!(song.order(), [0, 1, 2, 3], "no arrangement: as written");
        let (v1, chorus, v2) = (song.groups[0].id, song.groups[1].id, song.groups[2].id);
        let mut arrangement = song.new_arrangement(|n| format!("Arrangement {n}"));
        assert_eq!(arrangement.groups, [v1, chorus, v2]);
        arrangement.groups = vec![v1, chorus, v2, chorus];
        song.arrangement = Some(arrangement.id);
        song.arrangements.push(arrangement);
        assert_eq!(song.order(), [0, 1, 2, 3, 2]);
        // A group the song no longer has is passed over.
        let without_v2 = song.without_slide(3).unwrap();
        assert_eq!(without_v2.order(), [0, 1, 2, 2]);
        // An arrangement with none of its groups left plays the song as written.
        song.arrangements[0].groups = vec![Uuid::new_v4()];
        assert_eq!(song.order(), [0, 1, 2, 3]);
        assert_eq!(song.new_arrangement(|n| format!("Arrangement {n}")).name, "Arrangement 2");
    }

    /// The song with an arrangement in use: Verse 1, Chorus ×3, Verse 2, Chorus.
    fn arranged() -> (Presentation, [Uuid; 3]) {
        let mut song = song();
        let ids = [song.groups[0].id, song.groups[1].id, song.groups[2].id];
        let [v1, chorus, v2] = ids;
        let arrangement = Arrangement {
            id: Uuid::new_v4(),
            name: "As sung".to_owned(),
            groups: vec![v1, chorus, chorus, chorus, v2, chorus],
        };
        song.arrangement = Some(arrangement.id);
        song.arrangements.push(arrangement);
        (song, ids)
    }

    #[test]
    fn a_group_sung_several_times_running_is_one_piece_stored_as_the_group_listed_that_often() {
        let (song, [v1, chorus, v2]) = arranged();
        let arrangement = song.active_arrangement().unwrap();
        let runs: Vec<(Uuid, usize)> = arrangement.runs().iter().map(|run| (run.group, run.count)).collect();
        assert_eq!(runs, [(v1, 1), (chorus, 3), (v2, 1), (chorus, 1)]);
        // Saved and read back, it is still a plain list of groups.
        let saved = serde_json::to_string(&song).unwrap();
        assert!(
            saved.contains(&format!(
                "\"groups\":[\"{v1}\",\"{chorus}\",\"{chorus}\",\"{chorus}\",\"{v2}\",\"{chorus}\"]"
            ))
        );
        let read: Presentation = serde_json::from_str(&saved).unwrap();
        assert_eq!(read, song);
        assert_eq!(read.active_arrangement().unwrap().runs()[1].count, 3);
        // Once, ×4, More (up to 20).
        let mut changed = arrangement.clone();
        changed.set_times(1, 1);
        assert_eq!(changed.groups, [v1, chorus, v2, chorus]);
        changed.set_times(1, 4);
        assert_eq!(changed.groups.iter().filter(|g| **g == chorus).count(), 5);
        changed.set_times(1, 99);
        assert_eq!(changed.runs()[1].count, MOST_TIMES);
    }

    #[test]
    fn pieces_move_left_and_right_and_come_out_whole() {
        let (song, [v1, chorus, v2]) = arranged();
        let mut arrangement = song.active_arrangement().unwrap().clone();
        // The ending chorus to the front, before Verse 1.
        arrangement.move_run(5, 0, false);
        assert_eq!(arrangement.groups, [chorus, v1, chorus, chorus, chorus, v2]);
        // Verse 2 after the last piece.
        let mut arrangement = song.active_arrangement().unwrap().clone();
        arrangement.move_run(4, 5, true);
        assert_eq!(arrangement.groups, [v1, chorus, chorus, chorus, chorus, v2]);
        assert_eq!(arrangement.runs().len(), 3, "the two choruses meet, and are one piece ×4");
        let mut arrangement = song.active_arrangement().unwrap().clone();
        arrangement.remove_run(1);
        assert_eq!(arrangement.groups, [v1, v2, chorus]);
    }

    #[test]
    fn with_an_arrangement_the_slides_stand_in_the_order_sung_and_the_rest_wait_underneath() {
        let (mut song, [v1, chorus, _]) = arranged();
        let (sung, left_out) = song.sung().unwrap();
        let slides: Vec<usize> = sung.iter().map(|one| one.slide).collect();
        assert_eq!(slides, [0, 1, 2, 2, 2, 3, 2]);
        let again: Vec<bool> = sung.iter().map(|one| one.again).collect();
        assert_eq!(again, [false, false, false, true, true, false, true], "the same slide, sung again");
        assert_eq!(sung[4].step, 4);
        assert!(left_out.is_empty());
        // The chorus's second time through, and Verse 2.
        assert_eq!(song.pass_at(3), Some(Pass { run: 1, time: 2 }));
        assert_eq!(song.pass_at(5), Some(Pass { run: 2, time: 1 }));
        assert_eq!(song.run_steps(), [(0, 2), (2, 1), (5, 1), (6, 1)]);
        // Verse 2 left out: it waits underneath.
        song.arrangements[0].groups = vec![v1, chorus];
        let (sung, left_out) = song.sung().unwrap();
        assert_eq!(sung.len(), 3);
        assert_eq!(left_out, [3]);
        // As written: none of this.
        song.arrangement = None;
        assert_eq!(song.sung(), None);
    }

    #[test]
    fn splitting_a_group_or_reflowing_keeps_arrangements_whole() {
        let mut song = song();
        let (v1, chorus) = (song.groups[0].id, song.groups[1].id);
        let mut arrangement = song.new_arrangement(|n| format!("Arrangement {n}"));
        arrangement.groups = vec![v1, chorus, v1];
        song.arrangement = Some(arrangement.id);
        song.arrangements.push(arrangement);
        // Verse 1's second slide starts a new group: it is played after each Verse 1.
        let split = song.with_group_starting(1, "Pre-Chorus").unwrap();
        assert_eq!(split.order(), [0, 1, 2, 0, 1]);
        // A group added before the others in Reflow leaves the rest with their ids.
        let text = "[Intro]\nI\n\n[Verse 1]\nA\n\nB\n\n[Chorus]\nC\n\n[Verse 2]\nD\n";
        let reflowed = song.reflowed(text);
        assert_eq!(reflowed.groups[1].id, v1);
        assert_eq!(reflowed.groups[2].id, chorus);
        assert_eq!(reflowed.order(), [1, 2, 3, 1, 2]);
        assert_eq!(reflowed.arrangements, song.arrangements);
    }
}

#[cfg(test)]
mod ndi_name_tests {
    use super::*;

    #[test]
    fn an_ndi_name_splits_into_its_source_and_its_computer_even_when_the_computer_has_brackets() {
        assert_eq!(ndi_short_name("STREAM-PC (Camera 1)"), "Camera 1");
        assert_eq!(ndi_computer("STREAM-PC (Camera 1)"), Some("STREAM-PC"));
        assert_eq!(ndi_short_name("MacBook Pro (2) (Camera 1)"), "Camera 1");
        assert_eq!(ndi_computer("MacBook Pro (2) (Camera 1)"), Some("MacBook Pro (2)"));
        assert_eq!(ndi_short_name("plain"), "plain");
        assert_eq!(ndi_computer("plain"), None);
    }
}
