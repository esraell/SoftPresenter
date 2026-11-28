use parley::{Alignment, AlignmentOptions, FontWeight, Layout, LineHeight, PositionedLayoutItem, StyleProperty};
use sp_model::{
    Align, BoxStyle, ImageFit, Object, ObjectKind, Rgba, SLIDE_HEIGHT, SLIDE_WIDTH, Scaling, Shape, Slide, SlideRect,
    TextSpan, Theme, VAlign,
};
use vello::kurbo::{Affine, BezPath, CircleSegment, Ellipse, Join, Rect, RoundedRect, Shape as _, Stroke};
use vello::peniko::{Color, Fill, Mix};
use vello::{Glyph, Scene};

use crate::{Fonts, Picture};

/// The colour a span gives its words, carried through the text layout as its brush; none where
/// the words are the box's own colour (or gradient).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Ink(Option<Rgba>);

/// A scene ready to paint, and whether any text did not fit its box.
pub struct Built {
    pub scene: Scene,
    /// True when a box's text was still too tall at its smallest allowed size. The text is
    /// drawn anyway, spilling past its box, because cut-off words are worse than untidy ones.
    pub overflow: bool,
}

/// The largest 16:9 rectangle centred inside a target, as (x, y, width, height) in pixels.
pub fn fit_rect(target_w: u32, target_h: u32) -> (f64, f64, f64, f64) {
    let (tw, th) = (f64::from(target_w), f64::from(target_h));
    let (sw, sh) = (f64::from(SLIDE_WIDTH), f64::from(SLIDE_HEIGHT));
    // Compare by cross-multiplying so exact fits stay exact.
    let (w, h) = if tw * sh <= th * sw { (tw, tw * sh / sw) } else { (th * sw / sh, th) };
    ((tw - w) / 2.0, (th - h) / 2.0, w, h)
}

/// What is on screen, bottom layer first. A layer that is `None` is clear.
#[derive(Clone, Copy, Default)]
pub struct Layers<'a> {
    /// A picture behind the words, placed on the slide as `media_fit` says.
    pub media: Option<&'a Picture>,
    pub media_fit: ImageFit,
    pub slide: Option<&'a Slide>,
    /// Words over everything else, such as a message. Its objects carry their own places and
    /// looks.
    pub overlay: Option<&'a Slide>,
    /// Seconds since the slide went on screen, for text that scrolls; 0 shows it where it starts.
    pub time: f32,
    /// The colour where nothing is drawn, instead of black.
    pub backdrop: Option<Rgba>,
    /// How much of each edge of the slide to cover with the backdrop, in slide units: left, top,
    /// right, bottom. For screens whose edges are cut off or hidden.
    pub mask: [f32; 4],
    /// The Announcements layer: a slide of its own, drawn with its own theme over the slide and
    /// the media, and under the props and the message.
    pub announcement: Option<(&'a Slide, &'a Theme)>,
}

/// Draws a slide for a target of the given pixel size: black, with the slide fitted inside.
/// `None` draws black only, which is what a cleared layer looks like.
pub fn build_scene(fonts: &mut Fonts, slide: Option<&Slide>, theme: &Theme, target_w: u32, target_h: u32) -> Built {
    build_layers(
        fonts,
        &Layers {
            media_fit: sp_model::ImageFit::Fill,
            media: None,
            slide,
            overlay: None,
            time: 0.0,
            backdrop: None,
            mask: [0.0; 4],
            announcement: None,
        },
        theme,
        target_w,
        target_h,
    )
}

/// Draws every layer for a target of the given pixel size: black, then the media, then the
/// slide's words, all fitted inside the 16:9 slide area.
pub fn build_layers(fonts: &mut Fonts, layers: &Layers, theme: &Theme, target_w: u32, target_h: u32) -> Built {
    let mut scene = Scene::new();
    paint_under(&mut scene, layers, theme, target_w, target_h);
    let overflow = paint_over(&mut scene, fonts, layers, theme, target_w, target_h);
    Built { scene, overflow }
}

/// The layers of [`build_layers`] in two scenes that drawn one after the other make the same
/// picture: `under` is what lies below the slide's own objects (the backdrop, the theme's
/// colour and the media or theme picture), and `over` is everything above it. A fade can keep
/// a picture of `over` and draw `under` live, when the media is a playing video.
pub struct Parts {
    pub under: Scene,
    pub over: Scene,
    pub overflow: bool,
}

/// Draws every layer as [`build_layers`] does, but in the two parts of [`Parts`].
pub fn build_layer_parts(fonts: &mut Fonts, layers: &Layers, theme: &Theme, target_w: u32, target_h: u32) -> Parts {
    let under = build_under(layers, theme, target_w, target_h);
    let mut over = Scene::new();
    let overflow = paint_over(&mut over, fonts, layers, theme, target_w, target_h);
    Parts { under, over, overflow }
}

/// Only the lower part of [`Parts`].
pub fn build_under(layers: &Layers, theme: &Theme, target_w: u32, target_h: u32) -> Scene {
    let mut scene = Scene::new();
    paint_under(&mut scene, layers, theme, target_w, target_h);
    scene
}

/// What the Edit view draws behind the slide it is editing, in place of the black a screen has
/// there: one colour, or a checker of two in squares `square` slide units a side. It shows only
/// where the slide is see-through or plain black: a theme's own background colour (other than
/// plain black) or picture is drawn over it, as on a screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EditorBackdrop {
    Colour(Rgba),
    Checker { square: f32, colours: [Rgba; 2] },
}

/// True when a theme's background colour is the plain black a screen is anyway (or it has
/// none): the Edit view's backdrop shows through there.
pub fn plain_black(background: Option<Rgba>) -> bool {
    background.is_none_or(|colour| colour.a == 255 && (colour.r, colour.g, colour.b) == (0, 0, 0))
}

/// Draws every layer as [`build_layers`] does, for the Edit view's picture of the slide being
/// edited: over `backdrop` in place of the black base, and without the theme's background
/// colour where that is plain black. Nothing a screen draws goes through here.
pub fn build_layers_for_editor(
    fonts: &mut Fonts,
    layers: &Layers,
    theme: &Theme,
    backdrop: EditorBackdrop,
    target_w: u32,
    target_h: u32,
) -> Built {
    let mut scene = Scene::new();
    let whole = Rect::new(0.0, 0.0, f64::from(target_w), f64::from(target_h));
    match backdrop {
        EditorBackdrop::Colour(colour) => scene.fill(Fill::NonZero, Affine::IDENTITY, color(colour), None, &whole),
        EditorBackdrop::Checker { square, colours } => {
            scene.fill(Fill::NonZero, Affine::IDENTITY, color(colours[0]), None, &whole);
            let (x, y, w, _) = fit_rect(target_w, target_h);
            let to_target = Affine::translate((x, y)) * Affine::scale(w / f64::from(SLIDE_WIDTH));
            let square = f64::from(square.max(8.0));
            let across = (f64::from(SLIDE_WIDTH) / square).ceil() as usize;
            let down = (f64::from(SLIDE_HEIGHT) / square).ceil() as usize;
            let slide_area = Rect::new(0.0, 0.0, f64::from(SLIDE_WIDTH), f64::from(SLIDE_HEIGHT));
            scene.push_clip_layer(Fill::NonZero, to_target, &slide_area);
            // Every other square in the second colour, as one shape.
            let mut squares = BezPath::new();
            for row in 0..down {
                for column in (row % 2..across).step_by(2) {
                    let (left, top) = (column as f64 * square, row as f64 * square);
                    squares.extend(Rect::new(left, top, left + square, top + square).path_elements(0.1));
                }
            }
            scene.fill(Fill::NonZero, to_target, color(colours[1]), None, &squares);
            scene.pop_layer();
        }
    }
    paint_theme_and_media(&mut scene, layers, theme, !plain_black(theme.background), target_w, target_h);
    let overflow = paint_over(&mut scene, fonts, layers, theme, target_w, target_h);
    Built { scene, overflow }
}

/// The backdrop, the theme's colour under the media, and the media (or the theme's picture).
fn paint_under(scene: &mut Scene, layers: &Layers, theme: &Theme, target_w: u32, target_h: u32) {
    let whole = Rect::new(0.0, 0.0, f64::from(target_w), f64::from(target_h));
    let backdrop = layers.backdrop.map_or(Color::BLACK, color);
    scene.fill(Fill::NonZero, Affine::IDENTITY, backdrop, None, &whole);
    paint_theme_and_media(scene, layers, theme, true, target_w, target_h);
}

/// The theme's colour under the media (when `colour` says to draw it), and the media (or the
/// theme's picture), over whatever the base is.
fn paint_theme_and_media(
    scene: &mut Scene,
    layers: &Layers,
    theme: &Theme,
    colour: bool,
    target_w: u32,
    target_h: u32,
) {
    let (x, y, w, _) = fit_rect(target_w, target_h);
    let to_target = Affine::translate((x, y)) * Affine::scale(w / f64::from(SLIDE_WIDTH));

    // The theme's background colour fills the slide, under the media. A cleared screen stays black.
    if let (Some(background), Some(_), true) = (theme.background, layers.slide, colour) {
        let slide_area = Rect::new(0.0, 0.0, f64::from(SLIDE_WIDTH), f64::from(SLIDE_HEIGHT));
        scene.fill(Fill::NonZero, to_target, color(background), None, &slide_area);
    }

    // The media, or while none is showing, the theme's own picture behind its slides.
    let theme_picture = match (layers.media, layers.slide, theme.background_picture.as_deref()) {
        (None, Some(_), Some(file)) => crate::pictures::picture_at(file, w),
        _ => None,
    };
    if let Some(picture) = layers.media.or(theme_picture.as_ref()) {
        let (slide_w, slide_h) = (f64::from(SLIDE_WIDTH), f64::from(SLIDE_HEIGHT));
        let (pic_w, pic_h) = (f64::from(picture.width()), f64::from(picture.height()));
        // The theme's picture always covers the slide; media as it is set to.
        let fit = if layers.media.is_some() { layers.media_fit } else { ImageFit::Fill };
        let (scale_x, scale_y) = match fit {
            ImageFit::Fill => {
                let cover = (slide_w / pic_w).max(slide_h / pic_h);
                (cover, cover)
            }
            ImageFit::Fit => {
                let inside = (slide_w / pic_w).min(slide_h / pic_h);
                (inside, inside)
            }
            ImageFit::Stretch => (slide_w / pic_w, slide_h / pic_h),
        };
        let offset = ((slide_w - pic_w * scale_x) / 2.0, (slide_h - pic_h * scale_y) / 2.0);
        scene.push_clip_layer(Fill::NonZero, to_target, &Rect::new(0.0, 0.0, slide_w, slide_h));
        scene.draw_image(
            &picture.image,
            to_target * Affine::translate(offset) * Affine::scale_non_uniform(scale_x, scale_y),
        );
        scene.pop_layer();
    }
}

/// Everything above the media: the layout's objects, the slide's, the announcement, the props
/// and message, and the covered edges. True when a box's text did not fit.
fn paint_over(
    scene: &mut Scene,
    fonts: &mut Fonts,
    layers: &Layers,
    theme: &Theme,
    target_w: u32,
    target_h: u32,
) -> bool {
    let backdrop = layers.backdrop.map_or(Color::BLACK, color);
    let (x, y, w, _) = fit_rect(target_w, target_h);
    let to_target = Affine::translate((x, y)) * Affine::scale(w / f64::from(SLIDE_WIDTH));
    let mut overflow = false;
    if let Some(slide) = layers.slide {
        // The layout's own shapes, pictures and videos, behind the slide's words, and those it
        // puts in front of them.
        if let Some(behind) = layout_layer(slide, theme, false) {
            draw_objects(scene, fonts, to_target, &behind, theme, layers.time);
        }
        overflow |= draw_objects(scene, fonts, to_target, slide, theme, layers.time).overflow;
        if let Some(front) = layout_layer(slide, theme, true) {
            draw_objects(scene, fonts, to_target, &front, theme, layers.time);
        }
    }
    if let Some((announcement, own_theme)) = layers.announcement {
        // Its theme's background covers what is under it, as an announcement takes the screen.
        if let Some(background) = own_theme.background {
            let slide_area = Rect::new(0.0, 0.0, f64::from(SLIDE_WIDTH), f64::from(SLIDE_HEIGHT));
            scene.fill(Fill::NonZero, to_target, color(background), None, &slide_area);
        }
        if let Some(behind) = layout_layer(announcement, own_theme, false) {
            draw_objects(scene, fonts, to_target, &behind, own_theme, layers.time);
        }
        draw_objects(scene, fonts, to_target, announcement, own_theme, layers.time);
        if let Some(front) = layout_layer(announcement, own_theme, true) {
            draw_objects(scene, fonts, to_target, &front, own_theme, layers.time);
        }
    }
    if let Some(overlay) = layers.overlay {
        draw_objects(scene, fonts, to_target, overlay, theme, layers.time);
    }
    // The covered edges, over everything.
    let [left, top, right, bottom] = layers.mask.map(|edge| f64::from(edge.max(0.0)));
    let (w, h) = (f64::from(SLIDE_WIDTH), f64::from(SLIDE_HEIGHT));
    for edge in [
        Rect::new(0.0, 0.0, left, h),
        Rect::new(0.0, 0.0, w, top),
        Rect::new(w - right, 0.0, w, h),
        Rect::new(0.0, h - bottom, w, h),
    ] {
        if edge.area() > 0.0 {
            scene.fill(Fill::NonZero, to_target, backdrop, None, &edge);
        }
    }
    overflow
}

/// The shapes, pictures and videos a slide's layout puts behind its words and in front of them,
/// as a slide (for what they show: the videos to play, the web pages and NDI sources to take);
/// any the slide has itself (as the slide the theme editor works on does) are left out, so
/// they are drawn once. `None` when there are none.
pub fn layout_objects(slide: &Slide, theme: &Theme) -> Option<Slide> {
    layout_slide(slide, theme, None)
}

/// Those of [`layout_objects`] drawn behind the slide's words, or (`in_front`) over them.
fn layout_layer(slide: &Slide, theme: &Theme, in_front: bool) -> Option<Slide> {
    layout_slide(slide, theme, Some(in_front))
}

fn layout_slide(slide: &Slide, theme: &Theme, in_front: Option<bool>) -> Option<Slide> {
    let template = theme.template_for(slide)?;
    let objects: Vec<Object> = template
        .objects
        .iter()
        .filter(|object| in_front.is_none_or(|front| template.in_front.contains(&object.id) == front))
        .filter(|object| !object.hidden && !slide.objects.iter().any(|own| own.id == object.id))
        .cloned()
        .collect();
    if objects.is_empty() {
        return None;
    }
    let mut behind = Slide::new(objects);
    behind.template = Some(template.id);
    Some(behind)
}

/// Draws only a slide's objects, with nothing behind them, to lay over another picture of the
/// same size (a screen's name while screens are identified, say).
pub fn build_overlay(fonts: &mut Fonts, slide: &Slide, theme: &Theme, target_w: u32, target_h: u32) -> Scene {
    let mut scene = Scene::new();
    let (x, y, w, _) = fit_rect(target_w, target_h);
    let to_target = Affine::translate((x, y)) * Affine::scale(w / f64::from(SLIDE_WIDTH));
    draw_objects(&mut scene, fonts, to_target, slide, theme, 0.0);
    scene
}

/// Where a box's words came to lie when they were drawn, in slide units: the size they were
/// fitted to, the middle of their box across the slide, their first line's top and last line's
/// bottom, and how far left and right their lines reach.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Laid {
    pub size: f32,
    pub centre: f32,
    pub top: f32,
    pub bottom: f32,
    pub left: f32,
    pub right: f32,
}

/// The end of a song's mark as it is drawn: its letters' size, its top, and the place across
/// the slide it is centred on, in slide units; and how solid it is, from 0 to 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct EndMark {
    pub size: f32,
    pub top: f32,
    pub centre: f32,
    pub opacity: f32,
}

/// How far under the lowest words the mark's top is when it goes under them, in lines of the
/// largest words; and the least it is, on a full slide where the mark is pressed against the
/// lowest it may go.
const END_MARK_GAP: f32 = 0.5;
const END_MARK_LEAST_GAP: f32 = 0.25;

/// The smallest the mark is drawn, as a share of the largest words' size, where the words
/// can't be made smaller for it: the smallest size a theme can choose for it. With less room
/// than that it is left out.
const END_MARK_LEAST: f32 = 0.4;

/// A box of a slide's words as it was laid out: the place of its object on the slide, where
/// its words lie, and how solid the box is.
type Words = (usize, Laid, f32);

/// The largest of a slide's words (the first of them), which the mark takes its look and size
/// from, and the lowest (the last of them), which it goes under: as the slide lists its boxes.
fn largest_and_lowest(words: &[Words]) -> Option<(Words, Words)> {
    let largest = words.iter().copied().reduce(|best, next| if next.1.size > best.1.size { next } else { best })?;
    let lowest = words.iter().copied().reduce(|best, next| if next.1.bottom >= best.1.bottom { next } else { best })?;
    Some((largest, lowest))
}

/// The room for the mark under the lowest words (`lowest`), down to `floor`, the least gap kept,
/// and the size the theme wants it at beside the largest words (`sized`). Whether it fits and
/// where it goes are both worked out from these, so they never disagree by a rounding: words
/// that made room for the mark always get it.
fn end_mark_room(sized: Laid, lowest: Laid, floor: f32, look: &sp_model::EndLook) -> (f32, f32) {
    (floor - (lowest.bottom + sized.size * END_MARK_LEAST_GAP), sized.size * f32::from(look.size) / 100.0)
}

/// Whether the mark has room on the slide at the size the theme gives it: under the lowest
/// words with the least gap, it ends no lower than `floor`.
fn end_mark_fits(words: &[(usize, Laid, f32)], floor: f32, look: &sp_model::EndLook) -> bool {
    largest_and_lowest(words).is_some_and(|((_, sized, _), (_, lowest, _))| {
        let (room, wanted) = end_mark_room(sized, lowest, floor, look);
        room >= wanted
    })
}

/// Where the end of a song's mark goes under a slide's words as they were laid out (`words`,
/// each with the place of its object on the slide and how solid that box is), with the look the
/// theme gives it (`look`): at its share of the size the largest words were fitted to (80% as
/// it comes), centred on the lowest words' box, and no more solid than either box. With it,
/// which words are the largest: the mark takes their look.
///
/// Under the last line, its top is half a line of the largest words under the lowest words'
/// last line, wherever that leaves it above `floor`, the lowest it may reach (the bottom of the
/// screen, or the copyright line's top): it may lie below the words' box. On a full slide it
/// moves up against `floor`, the gap closing to a quarter line at the least. For a look that
/// puts it at the bottom of the screen, it lies against `floor`.
///
/// With no room for it at its size even so (the words make room for it first, where their box
/// lets them: [`end_mark_shorter_box`]), it is drawn smaller, in the room there
/// is, down to 40% of the largest words' size. `None` when the slide has no words, or there
/// isn't that much room: it never lies over the words, and never past `floor`.
fn end_mark_place(words: &[(usize, Laid, f32)], floor: f32, look: &sp_model::EndLook) -> Option<(usize, EndMark)> {
    let ((largest, sized, sized_opacity), (_, lowest, lowest_opacity)) = largest_and_lowest(words)?;
    // The room under the words, the least gap kept, as `end_mark_fits` has it.
    let (room, wanted) = end_mark_room(sized, lowest, floor, look);
    let size = if room >= wanted {
        wanted
    } else if room >= sized.size * END_MARK_LEAST {
        room.min(wanted)
    } else {
        return None;
    };
    // The mark is one line as tall as its letters' size: this is the lowest its top may be.
    let lowest_top = floor - size;
    let mark = EndMark {
        size,
        top: match look.place {
            sp_model::EndPlace::Under => (lowest.bottom + sized.size * END_MARK_GAP).min(lowest_top),
            sp_model::EndPlace::Bottom => lowest_top,
        },
        centre: lowest.centre,
        opacity: sized_opacity.min(lowest_opacity).clamp(0.0, 1.0),
    };
    Some((largest, mark))
}

/// A slide's box of the song's own words: its look, and its place as the slide or the theme
/// gives it (a box that grows round its words is fitted to them when it is drawn: see
/// [`drawn_place`]). `None` for anything else, and for words turned or flipped (they lie
/// somewhere else than their layout says: the mark is never put under those).
fn song_words_box<'a>(
    template: &sp_model::ThemeTemplate,
    at: usize,
    object: &'a Object,
) -> Option<crate::fonts::WordsBox<'a>> {
    let ObjectKind::Text { text, style, spans } = &object.kind else { return None };
    let straight = object.rotation == 0.0 && !object.flip_h && !object.flip_v;
    if object.hidden || !straight || !object.holds_song_words() {
        return None;
    }
    let style = template.text_style(object.role.as_ref(), style)?;
    let rect = object.frame.unwrap_or_else(|| template.place_for(object.role.as_ref()));
    let main = object.role.as_ref() == Some(&sp_model::Role::primary());
    Some(crate::fonts::WordsBox { at, main, text, spans, style, rect, paragraphs: object.shown_paragraphs })
}

/// The place a box of words is drawn in: its own, or for a box set to fit its text, that place
/// grown or shrunk about its middle to the text's height.
fn drawn_place(fonts: &mut Fonts, words: &crate::fonts::WordsBox) -> SlideRect {
    if words.style.scaling == Scaling::ContainerUpOrDown {
        fitted_container(fonts, words.text, words.spans, &words.style, words.rect)
    } else {
        words.rect
    }
}

/// How the end of a song's mark goes on a last slide: worked out once for a slide
/// ([`end_mark_plan`]) and remembered with the fonts, so a slide drawn again and again only
/// looks it up.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum MarkPlan {
    /// One mark, under the lowest of the song's words, as it always was: on a slide in one
    /// language, at the bottom of the screen, and where the theme asks for it under the last
    /// words only. With it, the box whose words are fitted to a shorter box than theirs to make
    /// room for it, and by how much ([`end_mark_shorter_box`]); `None` when none is.
    One(Option<(usize, f32)>),
    /// A mark under each language's words, on a slide with the main language's and the
    /// translation's, the lowest words' first.
    Each(Vec<LanguageMark>),
}

/// The mark under one language's words, on a slide in two languages.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LanguageMark {
    /// Under the main language's words (`true`), or the translation's.
    pub main: bool,
    /// How far down the slide it may reach: the slide's own floor, or for words with the other
    /// language's under them, a quarter of a line of its words over the first line of those.
    /// `None` when it is left out because under its words it would lie over the other
    /// language's (words beside them that reach lower, or found where it would be drawn). With
    /// a floor, it is still left out where there is no room for it even at its smallest.
    pub floor: Option<f32>,
    /// The box of its words fitted to a shorter box than theirs, to make room for it, and by how
    /// much; `None` when none is.
    pub cut: Option<(usize, f32)>,
}

impl MarkPlan {
    /// How much shorter the box of the object at `at` is taken to be while its words are fitted
    /// and placed, if it makes room for a mark.
    fn cut_for(&self, at: usize) -> Option<f32> {
        let cut = |made_room: Option<(usize, f32)>| made_room.filter(|(box_at, _)| *box_at == at).map(|(_, by)| by);
        match self {
            MarkPlan::One(made_room) => cut(*made_room),
            MarkPlan::Each(languages) => languages.iter().find_map(|language| cut(language.cut)),
        }
    }
}

/// Whether a slide's object holds the main language's words (and not the translation's).
fn main_words(slide: &Slide, at: usize) -> bool {
    slide.objects.get(at).is_some_and(|object| object.role.as_ref() == Some(&sp_model::Role::primary()))
}

/// How the end of a song's mark goes on a last slide due one (`floor`: how far down it may
/// reach), with the look the theme gives it (`look`).
///
/// One mark, as it always was ([`MarkPlan::One`]), unless the slide has the song's words in two
/// languages and the theme puts the mark under the last line, under each language's words:
/// then a mark under each ([`MarkPlan::Each`], [`each_language_marks`]).
///
/// Worked out once for a slide: what it came to is remembered with the fonts
/// (`Fonts::mark_room`), by the words, their looks and places, `floor`, and the mark's look
/// (all of it but its colour, which moves nothing), so a slide drawn again and again (behind a
/// playing video, in the Program preview) only looks it up. Everything the plan comes to
/// follows from those, the floor of one language's mark over the other language's words among
/// it (where those words are drawn is their box, look and place, and their own mark's look and
/// floor). The words are laid out in the slide's own units, so one plan serves a screen of
/// any size.
fn end_mark_plan(
    fonts: &mut Fonts,
    slide: &Slide,
    template: &sp_model::ThemeTemplate,
    look: &sp_model::EndLook,
    floor: f32,
    time: f32,
) -> MarkPlan {
    let boxes: Vec<crate::fonts::WordsBox> = slide
        .objects
        .iter()
        .enumerate()
        .filter_map(|(at, object)| song_words_box(template, at, object))
        // Words that scroll lie nowhere for long: the mark is never put under them.
        .filter(|words| words.style.scroll == sp_model::Scroll::None)
        .collect();
    if boxes.is_empty() {
        return MarkPlan::One(None);
    }
    if let Some(known) = fonts.mark_room(&boxes, floor, look) {
        return known;
    }
    let plan = work_out_marks(fonts, slide, &boxes, look, floor, time);
    fonts.remember_mark_room(&boxes, floor, look, plan.clone());
    plan
}

/// A box of a slide's song words, measured: the box, the place its words are drawn in, and
/// whether they spill out of it.
type Measured<'a, 'b> = (&'b crate::fonts::WordsBox<'a>, SlideRect, bool);

/// What [`end_mark_plan`] remembers: the words measured as they will be drawn, with nothing
/// drawn, and where the marks go worked out.
fn work_out_marks(
    fonts: &mut Fonts,
    slide: &Slide,
    boxes: &[crate::fonts::WordsBox],
    look: &sp_model::EndLook,
    floor: f32,
    time: f32,
) -> MarkPlan {
    let mut places: Vec<Measured> = Vec::new();
    let mut words = Vec::new();
    for b in boxes {
        let rect = drawn_place(fonts, b);
        let (spilt, laid) = measure_text_box(fonts, (b.text, b.spans), &b.style, rect, time, b.paragraphs);
        let Some(laid) = laid else { continue };
        places.push((b, rect, spilt));
        words.push((b.at, laid, slide.objects.get(b.at).map_or(1.0, |object| object.opacity)));
    }
    let each = look.place == sp_model::EndPlace::Under && look.under == sp_model::EndUnder::EachLanguage;
    let languages = [true, false].map(|main| {
        let at: Vec<usize> = (0..words.len()).filter(|n| main_words(slide, words[*n].0) == main).collect();
        (main, at)
    });
    if !each || languages.iter().any(|(_, at)| at.is_empty()) {
        return MarkPlan::One(end_mark_shorter_box(fonts, &places, &words, look, floor, time));
    }
    each_language_marks(fonts, &places, &words, languages, look, floor, time)
}

/// How wide the end of a song's mark is drawn at `size` in the look of the words it follows.
fn end_mark_width(fonts: &mut Fonts, words: &BoxStyle, size: f32, mark: sp_model::EndMark) -> f32 {
    let style = end_mark_style(words, size, None);
    lay_out_within(fonts, &mark.text(), &[], &style, size, None).width()
}

/// A mark under each language's words ([`MarkPlan::Each`]), each placed, sized and fitted as the
/// one mark always is ([`end_mark_place`], [`end_mark_shorter_box`]), with its own words alone:
/// the lowest language's first, against the slide's floor, as one mark would be.
///
/// Words with the other language's under them (stacked, the one over the other) have their mark
/// between the two, over a floor a quarter of a line of their words above the other's first line
/// as it is drawn (after that language's words made room for their own mark): their words make
/// room for it as one mark's do, and never the other language's. Words beside the other
/// language's (side by side) each have their mark under them, against the slide's floor: the
/// other's are no floor to a mark that clears them across the slide. Words with the other
/// language's beside them and reaching lower, where the mark would lie, get none.
///
/// Last, each mark is checked where it will be drawn, at its size and width, against the box
/// round the other language's lines (from the first line's top to the last line's bottom, as
/// far across as the widest reaches): one that would lie in it is left out. A mark is never
/// drawn over words.
fn each_language_marks(
    fonts: &mut Fonts,
    places: &[Measured],
    words: &[Words],
    languages: [(bool, Vec<usize>); 2],
    look: &sp_model::EndLook,
    floor: f32,
    time: f32,
) -> MarkPlan {
    // The lowest words' language first.
    let lowest = |at: &[usize]| at.iter().map(|n| words[*n].1.bottom).fold(f32::MIN, f32::max);
    let mut languages = languages;
    languages.sort_by(|(_, a), (_, b)| lowest(b).total_cmp(&lowest(a)));
    // Each language's words as they will be drawn, and the look of its largest words.
    let mut drawn: Vec<(bool, Vec<Words>, BoxStyle)> = Vec::new();
    let mut plan = Vec::new();
    for (main, at) in &languages {
        let theirs: Vec<Words> = at.iter().map(|n| words[*n]).collect();
        let their_places: Vec<Measured> = at.iter().map(|n| places[*n]).collect();
        let Some(((largest, sized, _), (_, low, _))) = largest_and_lowest(&theirs) else { continue };
        let Some(style) = their_places.iter().find(|(b, _, _)| b.at == largest).map(|(b, _, _)| b.style.clone()) else {
            continue;
        };
        // Across the slide, the mark at its size reaches this far either side of its middle,
        // and a quarter of a line more, for outlines and shadows: the lines it may meet.
        let clear = sized.size * END_MARK_LEAST_GAP;
        let reach = end_mark_width(fonts, &style, sized.size * f32::from(look.size) / 100.0, look.mark) / 2.0 + clear;
        let mut own_floor = Some(floor);
        for (_, other, _) in drawn.iter().flat_map(|(_, words, _)| words) {
            let across = other.left < low.centre + reach && other.right > low.centre - reach;
            if !across || other.bottom <= low.bottom {
                continue;
            }
            own_floor =
                if other.top >= low.bottom { own_floor.map(|floor| floor.min(other.top - clear)) } else { None };
        }
        let cut = own_floor.and_then(|floor| end_mark_shorter_box(fonts, &their_places, &theirs, look, floor, time));
        // Its words as they will be drawn: in a shorter box where they make room.
        let mut theirs = theirs;
        if let Some((cut_at, by)) = cut
            && let Some(n) = theirs.iter().position(|(at, _, _)| *at == cut_at)
            && let Some((b, rect, _)) = their_places.iter().find(|(b, _, _)| b.at == cut_at)
        {
            let shorter = SlideRect { h: (rect.h - by).max(1.0), ..*rect };
            if let (_, Some(laid)) = measure_text_box(fonts, (b.text, b.spans), &b.style, shorter, time, b.paragraphs) {
                theirs[n].1 = laid;
            }
        }
        drawn.push((*main, theirs, style));
        plan.push(LanguageMark { main: *main, floor: own_floor, cut });
    }
    // Where each mark will be drawn, checked against the other language's lines.
    for (n, (_, theirs, style)) in drawn.iter().enumerate() {
        let Some(floor) = plan[n].floor else { continue };
        let Some((_, mark)) = end_mark_place(theirs, floor, look) else { continue };
        let Some(((_, sized, _), _)) = largest_and_lowest(theirs) else { continue };
        let reach = end_mark_width(fonts, style, mark.size, look.mark) / 2.0 + sized.size * END_MARK_LEAST_GAP;
        let over = drawn.iter().enumerate().filter(|(other, _)| *other != n).flat_map(|(_, (_, words, _))| words).any(
            |(_, other, _)| {
                other.left < mark.centre + reach
                    && other.right > mark.centre - reach
                    && other.top < mark.top + mark.size
                    && other.bottom > mark.top
            },
        );
        if over {
            plan[n].floor = None;
        }
    }
    MarkPlan::Each(plan)
}

/// On a last slide too full for the end of a song's mark at its size, even pressed against
/// `floor`: how much shorter, at its bottom, the lowest words' box is taken to be while its
/// words are fitted and placed, so that they leave the mark its room. With it, which object
/// that box is. In the shorter box the words are fitted as their box fits them: where the box
/// had room to spare they only move up in it, at their size; where it had none they come out a
/// little smaller, if the theme makes words smaller to fit (or bigger to fill). `words` are the
/// words the mark goes under, as they were measured, and `places` their boxes, in the same order.
///
/// `None` when there is room as it is, and when no shorter box gives it without the words
/// spilling out of their box: words that keep their size in a box they fill, words already at
/// their least size, a box that grows round its words. The mark is then drawn smaller
/// ([`end_mark_place`]).
///
/// The least cut that gives the mark its room is looked for. It is worked out once for a slide,
/// with the rest of its plan ([`end_mark_plan`]).
fn end_mark_shorter_box(
    fonts: &mut Fonts,
    places: &[Measured],
    words: &[Words],
    look: &sp_model::EndLook,
    floor: f32,
    time: f32,
) -> Option<(usize, f32)> {
    if end_mark_fits(words, floor, look) {
        return None;
    }
    let ((_, sized, _), (lowest, low, _)) = largest_and_lowest(words)?;
    let place = words.iter().position(|(at, _, _)| *at == lowest)?;
    let (b, rect, spilt) = places[place];
    // Words that spill out of their box already, or whose box is made to fit them, have
    // nothing to give.
    if spilt || b.style.scaling == Scaling::ContainerUpOrDown {
        return None;
    }
    // How far the mark, at its size and with the least gap, passes the floor as things are.
    let short_by = low.bottom + sized.size * END_MARK_LEAST_GAP + sized.size * f32::from(look.size) / 100.0 - floor;
    // The trials' sizes are not remembered: only the one the slide is then drawn at is.
    fonts.trying = true;
    // Whether the mark has its room with the box this much shorter, the words still inside it.
    let mut fits_with = |cut: f32| {
        let shorter = SlideRect { h: (rect.h - cut).max(1.0), ..rect };
        let (spilt, laid) = measure_text_box(fonts, (b.text, b.spans), &b.style, shorter, time, b.paragraphs);
        let Some(laid) = laid.filter(|_| !spilt) else { return false };
        let mut trial = words.to_vec();
        trial[place].1 = laid;
        end_mark_fits(&trial, floor, look)
    };
    // The cuts tried, least first. Words in the middle of a box with room to spare move up by
    // half of what the box loses, and words at its bottom by all of it, so the shortfall and
    // twice the shortfall come first; then a sixteenth of the box at a time, for words that
    // must come out smaller, which they do in steps. As the box gets shorter the words' last
    // line only ever moves up, until they would spill out of it: so the first cut that gives
    // the room is near the least that does, and halving between it and the one before finds
    // that. A small cut is never passed over for a large one.
    let mut cuts: Vec<f32> =
        [short_by, short_by * 2.0].into_iter().chain((1..=14).map(|n| rect.h * n as f32 / 16.0)).collect();
    cuts.retain(|cut| *cut > 0.0 && *cut < rect.h);
    cuts.sort_by(f32::total_cmp);
    let mut too_little = 0.0;
    let mut enough = None;
    for cut in cuts {
        if fits_with(cut) {
            enough = Some(cut);
            break;
        }
        too_little = cut;
    }
    if let Some(enough) = enough.as_mut() {
        for _ in 0..8 {
            let middle = (too_little + *enough) / 2.0;
            if fits_with(middle) {
                *enough = middle;
            } else {
                too_little = middle;
            }
        }
    }
    fonts.trying = false;
    enough.map(|cut| (b.at, cut))
}

/// Why a slide due the end of a song's mark was drawn without it (or without one of its two).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkLeftOut {
    /// There was no room for it under the words, even at its smallest.
    NoRoom,
    /// Under one language's words it would have lain over the other language's words beside
    /// them, so it was left out on purpose.
    OverTheOtherLanguage,
}

thread_local! {
    /// The slide last drawn on this thread that was due the end of a song's mark and got none
    /// (or not every one), and why, until it is asked for ([`take_end_mark_left_out`]) or the
    /// same slide is drawn with every mark.
    static END_MARK_LEFT_OUT: std::cell::Cell<Option<(sp_model::Uuid, MarkLeftOut)>> =
        const { std::cell::Cell::new(None) };
}

/// The slide (by its id) that was drawn on this thread since the last call due the end of a
/// song's mark and without it (or one of its two), and why: for the app to say so in its log,
/// once, of that slide, whichever drawing found it (a screen's, the Program preview's, an NDI
/// picture's).
pub fn take_end_mark_left_out() -> Option<(sp_model::Uuid, MarkLeftOut)> {
    END_MARK_LEFT_OUT.with(|left_out| left_out.replace(None))
}

/// What drawing a slide's objects came to: whether any text overflowed its box; where the
/// song's words lie, box by box (with the place of its object on the slide and how solid the
/// box is), which is looked at only for a slide that asks for the end of a song's mark (and in
/// the tests); the mark under the lowest words, if it was drawn; and every mark drawn, the
/// lowest words' first, each with the object whose words' look it took (the largest words it
/// is under).
pub(crate) struct Painted {
    pub overflow: bool,
    #[cfg_attr(not(test), expect(dead_code, reason = "the tests compare the words with the mark on and off"))]
    pub words: Vec<(usize, Laid, f32)>,
    #[cfg_attr(not(test), expect(dead_code, reason = "the tests look at where the mark went"))]
    pub end_mark: Option<EndMark>,
    #[cfg_attr(not(test), expect(dead_code, reason = "the tests look at where each mark went"))]
    pub end_marks: Vec<(usize, EndMark)>,
}

/// Draws a slide's objects, and under its words the end of a song's mark when the slide asks
/// for it ([`Slide::end_mark`]).
fn draw_objects(
    scene: &mut Scene,
    fonts: &mut Fonts,
    to_target: Affine,
    slide: &Slide,
    theme: &Theme,
    time: f32,
) -> Painted {
    let mut overflow = false;
    // Where the words lie is kept only for a slide that gets the mark.
    let marking = (slide.end_mark.is_some() && !slide.words_hidden) || cfg!(test);
    let mut words = Vec::new();
    // The slide is drawn with one of the theme's templates: the one it names, or the one
    // that suits what is on it.
    let template = theme.template_for(slide);
    // Where the marks go. A last slide too full for one: the words it goes under are fitted to
    // a shorter box, so they move up in theirs, or come out a little smaller, and leave it room.
    let plan = match (slide.end_mark, template) {
        (Some(floor), Some(template)) if !slide.words_hidden => {
            Some(end_mark_plan(fonts, slide, template, &theme.end_look, floor, time))
        }
        _ => None,
    };
    for (at, object) in slide.objects.iter().enumerate().filter(|(_, object)| !object.hidden) {
        let Some(template) = template else { continue };
        let rect = object.frame.unwrap_or_else(|| template.place_for(object.role.as_ref()));
        // A text box set to fit its text grows or shrinks about its middle to the text's height.
        let rect = match &object.kind {
            ObjectKind::Text { text, style, spans } => match template.text_style(object.role.as_ref(), style) {
                Some(look) if look.scaling == Scaling::ContainerUpOrDown => {
                    fitted_container(fonts, text, spans, &look, rect)
                }
                _ => rect,
            },
            ObjectKind::Shape { .. }
            | ObjectKind::Image { .. }
            | ObjectKind::Web { .. }
            | ObjectKind::Ndi { .. }
            | ObjectKind::Mirror { .. }
            | ObjectKind::Screen { .. } => rect,
        };
        // Turn the object about its own centre.
        let centre = (f64::from(rect.x + rect.w / 2.0), f64::from(rect.y + rect.h / 2.0));
        let mirror = Affine::translate(centre)
            * Affine::scale_non_uniform(if object.flip_h { -1.0 } else { 1.0 }, if object.flip_v { -1.0 } else { 1.0 })
            * Affine::translate((-centre.0, -centre.1));
        let placed = to_target * Affine::rotate_about(f64::from(object.rotation).to_radians(), centre) * mirror;
        let faded = object.opacity < 1.0;
        if faded {
            let whole_slide = Rect::new(0.0, 0.0, f64::from(SLIDE_WIDTH), f64::from(SLIDE_HEIGHT));
            scene.push_layer(Fill::NonZero, Mix::Normal, object.opacity.clamp(0.0, 1.0), to_target, &whole_slide);
        }
        // The object's outline: its shape, or the box a text object sits in.
        let outline = match &object.kind {
            ObjectKind::Shape { shape } => shape_path(*shape, rect),
            ObjectKind::Screen { corner, .. } if *corner > 0.0 => {
                let (x, y) = (f64::from(rect.x), f64::from(rect.y));
                let radius = f64::from(corner.min(rect.w.min(rect.h) / 2.0));
                RoundedRect::new(x, y, x + f64::from(rect.w), y + f64::from(rect.h), radius).to_path(0.1)
            }
            ObjectKind::Text { .. }
            | ObjectKind::Image { .. }
            | ObjectKind::Web { .. }
            | ObjectKind::Ndi { .. }
            | ObjectKind::Mirror { .. }
            | ObjectKind::Screen { .. } => {
                let (x, y) = (f64::from(rect.x), f64::from(rect.y));
                Rect::new(x, y, x + f64::from(rect.w), y + f64::from(rect.h)).to_path(0.1)
            }
        };
        // A soft shadow under the box or shape: several faint, widening layers, offset.
        if let Some(look) = object.shadow.filter(|look| look.opacity > 0.0) {
            let fall = f64::from(look.angle).to_radians();
            let offset = (f64::from(look.offset) * fall.cos(), -f64::from(look.offset) * fall.sin());
            let layer_alpha = 1.0 - (1.0 - look.opacity.clamp(0.0, 1.0)).powf(1.0 / 6.0);
            let mut shade = look.color;
            shade.a = (f32::from(shade.a) * layer_alpha).round() as u8;
            let shadowed = placed * Affine::translate(offset);
            let spread = f64::from(look.blur.max(0.0)) / 3.0;
            for step in 1..=6 {
                scene.fill(Fill::NonZero, shadowed, color(shade), None, &outline);
                if spread > 0.0 {
                    let edge = Stroke::new(f64::from(step) * spread).with_join(Join::Round);
                    scene.stroke(&edge, shadowed, color(shade), None, &outline);
                }
            }
        }
        if let Some(fill) = object.fill {
            match object.gradient.filter(sp_model::Gradient::on) {
                Some(gradient) => {
                    let (x, y) = (f64::from(rect.x), f64::from(rect.y));
                    let area = Rect::new(x, y, x + f64::from(rect.w), y + f64::from(rect.h));
                    scene.fill(Fill::NonZero, placed, &gradient_brush(&gradient, fill, area), None, &outline);
                }
                None => scene.fill(Fill::NonZero, placed, color(fill), None, &outline),
            }
        }
        if let Some(line) = object.stroke.filter(|line| line.width > 0.0) {
            scene.stroke(&Stroke::new(f64::from(line.width)), placed, color(line.color), None, &outline);
        }
        if let ObjectKind::Image { file, fit } = &object.kind {
            draw_picture(scene, placed, file, *fit, rect, &outline);
        }
        if let ObjectKind::Web { .. } = &object.kind {
            draw_web(scene, placed, object_web_url(object), rect, &outline);
        }
        if let ObjectKind::Ndi { source, scaling, .. } = &object.kind {
            draw_ndi(scene, fonts, placed, source, *scaling, template, rect, &outline);
        }
        if let ObjectKind::Mirror { scaling } = &object.kind {
            draw_mirror(scene, fonts, placed, *scaling, template, rect, &outline);
        }
        if let ObjectKind::Screen { screen, label, .. } = &object.kind {
            draw_screen(scene, placed, *screen, rect, &outline);
            if !label.trim().is_empty() {
                draw_screen_label(scene, fonts, placed, label, template, rect, &outline);
            }
        }
        // While the slide's words are hidden, the box (its place, worked out from its words when
        // it is fitted to them, and its own fill, outline and shadow) is drawn as ever, and its
        // words are not: their fill, outline and shadow, and the bands behind their lines.
        if let ObjectKind::Text { text, style, spans } = &object.kind
            && !slide.words_hidden
        {
            // The theme's style for the object's role, with the object's own settings over it.
            let Some(style) = template.text_style(object.role.as_ref(), style) else { continue };
            // The words that make room for the mark are fitted to a shorter box than theirs
            // (the box itself, drawn above, is as ever).
            let words_rect = match plan.as_ref().and_then(|plan| plan.cut_for(at)) {
                Some(cut) => SlideRect { h: (rect.h - cut).max(1.0), ..rect },
                None => rect,
            };
            let (spilt, laid) =
                draw_text_box(scene, fonts, placed, (text, spans), &style, words_rect, time, object.shown_paragraphs);
            overflow |= spilt;
            // The mark goes under the song's own words (the main language's and the
            // translation's), never under a caption, a clock or a deck's text box. Words in a
            // box that is turned or flipped lie somewhere else on the slide than their layout
            // says: the mark is never put under those either.
            let straight = object.rotation == 0.0 && !object.flip_h && !object.flip_v;
            if marking && straight && object.holds_song_words() {
                words.extend(laid.map(|laid| (at, laid, object.opacity)));
            }
        }
        if faded {
            scene.pop_layer();
        }
    }
    // The end of a song's mark, on a layer of its own over the slide's objects: the words above
    // were laid out and drawn without it. Each mark takes the look of the largest words it is
    // under.
    let mut end_mark = None;
    let mut end_marks = Vec::new();
    // Whether a mark was due under words that were drawn and left out, and why (the first found).
    let mut left_out = None;
    let mut draw_mark = |scene: &mut Scene, fonts: &mut Fonts, largest: usize, mark: EndMark| -> bool {
        let Some(template) = template else { return false };
        let Some(object) = slide.objects.get(largest) else { return false };
        let ObjectKind::Text { style, .. } = &object.kind else { return false };
        let Some(look) = template.text_style(object.role.as_ref(), style) else { return false };
        draw_end_mark(scene, fonts, to_target, &look, mark, &theme.end_look);
        end_marks.push((largest, mark));
        true
    };
    match (slide.end_mark, &plan) {
        (Some(floor), Some(MarkPlan::One(_))) => {
            if let Some((largest, mark)) = end_mark_place(&words, floor, &theme.end_look)
                && draw_mark(scene, fonts, largest, mark)
            {
                end_mark = Some(mark);
            }
            if !words.is_empty() && end_mark.is_none() {
                left_out = Some(MarkLeftOut::NoRoom);
            }
        }
        (Some(_), Some(MarkPlan::Each(languages))) => {
            for (n, language) in languages.iter().enumerate() {
                let theirs: Vec<Words> =
                    words.iter().copied().filter(|(at, _, _)| main_words(slide, *at) == language.main).collect();
                if theirs.is_empty() {
                    continue;
                }
                let Some(floor) = language.floor else {
                    left_out.get_or_insert(MarkLeftOut::OverTheOtherLanguage);
                    continue;
                };
                match end_mark_place(&theirs, floor, &theme.end_look) {
                    Some((largest, mark)) if draw_mark(scene, fonts, largest, mark) => {
                        if n == 0 {
                            end_mark = Some(mark);
                        }
                    }
                    _ => {
                        left_out.get_or_insert(MarkLeftOut::NoRoom);
                    }
                }
            }
        }
        _ => {}
    }
    // Due, with words to go under, and left out: the app's log says so, and why, of this slide.
    // Drawn with every mark after all (another theme's screen, another floor), it has nothing
    // to say.
    if slide.end_mark.is_some() && !slide.words_hidden && !words.is_empty() {
        END_MARK_LEFT_OUT.with(|noted| {
            if let Some(why) = left_out {
                noted.set(Some((slide.id, why)));
            } else if noted.get().is_some_and(|(id, _)| id == slide.id) {
                noted.set(None);
            }
        });
    }
    Painted { overflow, words, end_mark, end_marks }
}

/// The look the end of a song's mark is drawn in: the face, weight, outline and shadow of the
/// words it follows (`words`), in their colour unless the theme chose one for it (`colour`), at
/// `size`, on one line of its own that nothing else of their look changes.
fn end_mark_style(words: &BoxStyle, size: f32, colour: Option<Rgba>) -> BoxStyle {
    let plain = plain_look(words.clone());
    BoxStyle {
        size,
        min_size: size,
        align: Align::Center,
        valign: VAlign::Top,
        scaling: Scaling::None,
        line_height: 1.0,
        fill: colour.unwrap_or(plain.fill),
        ..plain
    }
}

/// Draws the end of a song's mark as the theme has it (`end`: which mark, and its colour if it
/// has one of its own), in the look of the words it follows (`look`).
fn draw_end_mark(
    scene: &mut Scene,
    fonts: &mut Fonts,
    to_target: Affine,
    look: &BoxStyle,
    mark: EndMark,
    end: &sp_model::EndLook,
) {
    let style = end_mark_style(look, mark.size, end.colour);
    // Room for the three marks whatever the face: they are centred in it and never wrap.
    let room = mark.size * 20.0;
    let rect = SlideRect { x: mark.centre - room / 2.0, y: mark.top, w: room, h: mark.size };
    // As solid as the words it follows, no more.
    let faded = mark.opacity < 1.0;
    if faded {
        let whole_slide = Rect::new(0.0, 0.0, f64::from(SLIDE_WIDTH), f64::from(SLIDE_HEIGHT));
        scene.push_layer(Fill::NonZero, Mix::Normal, mark.opacity, to_target, &whole_slide);
    }
    draw_text_box(scene, fonts, to_target, (&end.mark.text(), &[]), &style, rect, 0.0, None);
    if faded {
        scene.pop_layer();
    }
}

/// A box's look with everything that changes its words or moves them taken off: no capitals,
/// list marks, bands, scrolling, margins, spacing or baseline shift, and no underline. Its face,
/// weight, slant, colour, outline and shadow stay.
fn plain_look(look: BoxStyle) -> BoxStyle {
    BoxStyle {
        caps: sp_model::Caps::None,
        list: sp_model::ListStyle::None,
        line_transform: sp_model::LineTransform::None,
        line_fill: sp_model::LineFill { on: false, ..look.line_fill },
        scroll: sp_model::Scroll::None,
        underline: false,
        strike: false,
        letter_spacing: 0.0,
        paragraph_spacing: 0.0,
        baseline: 0.0,
        margins: [0.0; 4],
        ..look
    }
}

thread_local! {
    /// Whether a web page box with no picture yet is drawn as a placeholder, on this thread.
    static PLACEHOLDERS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Whether such placeholders are the Edit view's see-through ones (see
    /// [`shade_empty_boxes`]), on this thread.
    static SHADED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// The screens' pictures a screen-picture object may show while a scene is built, on this
    /// thread (see [`with_screen_pictures`]).
    static SCREEN_PICTURES: std::cell::RefCell<Vec<ScreenPicture>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// What one screen shows, for a screen-picture object to show too: the very scene that screen
/// is drawn with, for a target of `width` by `height` pixels.
pub struct ScreenPicture {
    pub screen: sp_model::Uuid,
    pub scene: Scene,
    pub width: u32,
    pub height: u32,
}

/// Builds a scene (whatever `build` does) with these screens' pictures at hand: each
/// screen-picture object that names one of them shows its scene, scaled into its place. Any
/// other screen-picture object shows nothing (a stand-in in the operator's window). The
/// pictures are let go when `build` returns.
pub fn with_screen_pictures<R>(pictures: Vec<ScreenPicture>, build: impl FnOnce() -> R) -> R {
    let earlier = SCREEN_PICTURES.with(|kept| std::mem::replace(&mut *kept.borrow_mut(), pictures));
    let built = build();
    SCREEN_PICTURES.with(|kept| *kept.borrow_mut() = earlier);
    built
}

/// A screen-picture object: the screen's own scene, fitted into the object's place keeping its
/// shape and clipped to the object's outline. Without it, nothing on the screens, and a dark
/// panel with a screen drawn in it in the operator's window.
fn draw_screen(scene: &mut Scene, placed: Affine, screen: Option<sp_model::Uuid>, rect: SlideRect, outline: &BezPath) {
    let drawn = screen.is_some_and(|wanted| {
        SCREEN_PICTURES.with(|pictures| {
            let pictures = pictures.borrow();
            let Some(picture) = pictures.iter().find(|picture| picture.screen == wanted) else { return false };
            let (w, h) = (f64::from(picture.width.max(1)), f64::from(picture.height.max(1)));
            let scale = (f64::from(rect.w) / w).min(f64::from(rect.h) / h);
            let offset = (
                f64::from(rect.x) + (f64::from(rect.w) - w * scale) / 2.0,
                f64::from(rect.y) + (f64::from(rect.h) - h * scale) / 2.0,
            );
            // Black where a screen of another shape leaves room, as a screen is.
            scene.fill(Fill::NonZero, placed, Color::BLACK, None, outline);
            scene.push_clip_layer(Fill::NonZero, placed, outline);
            scene.append(&picture.scene, Some(placed * Affine::translate(offset) * Affine::scale(scale)));
            scene.pop_layer();
            true
        })
    });
    if drawn || !PLACEHOLDERS.with(std::cell::Cell::get) {
        return;
    }
    let (x, y, w, h) = (f64::from(rect.x), f64::from(rect.y), f64::from(rect.w), f64::from(rect.h));
    scene.fill(Fill::NonZero, placed, Color::from_rgba8(28, 30, 34, 255), None, outline);
    scene.stroke(&Stroke::new(3.0), placed, Color::from_rgba8(90, 94, 102, 255), None, outline);
    // A screen on its stand, in the middle.
    let side = (w.min(h * 16.0 / 9.0) * 0.32).clamp(24.0, 360.0);
    let (sw, sh) = (side, side * 9.0 / 16.0);
    let (cx, cy) = (x + w / 2.0, y + h / 2.0 - sh * 0.1);
    let ink = Color::from_rgba8(130, 136, 146, 255);
    let line = Stroke::new((side / 28.0).max(2.0)).with_join(Join::Round);
    let face = RoundedRect::new(cx - sw / 2.0, cy - sh / 2.0, cx + sw / 2.0, cy + sh / 2.0, side / 30.0);
    scene.stroke(&line, placed, ink, None, &face);
    let mut stand = BezPath::new();
    stand.move_to((cx, cy + sh / 2.0));
    stand.line_to((cx, cy + sh / 2.0 + sh * 0.18));
    stand.move_to((cx - sw * 0.18, cy + sh / 2.0 + sh * 0.18));
    stand.line_to((cx + sw * 0.18, cy + sh / 2.0 + sh * 0.18));
    scene.stroke(&line, placed, ink, None, &stand);
}

/// A screen-picture object's label: its words, small and white, in a dark band across the top
/// of the picture, inside its outline.
fn draw_screen_label(
    scene: &mut Scene,
    fonts: &mut Fonts,
    placed: Affine,
    label: &str,
    template: &sp_model::ThemeTemplate,
    rect: SlideRect,
    outline: &BezPath,
) {
    let Some(base) = template.box_for(None) else { return };
    let size = (rect.h * 0.075).clamp(20.0, 72.0);
    let band = SlideRect { h: (size * 1.6).min(rect.h), ..rect };
    let look = BoxStyle {
        size,
        min_size: size * 0.5,
        weight: 600,
        italic: false,
        fill: Rgba::WHITE,
        align: sp_model::Align::Left,
        valign: VAlign::Middle,
        outline_width: 0.0,
        shadow: false,
        caps: sp_model::Caps::None,
        underline: false,
        strike: false,
        scaling: Scaling::TextDown,
        line_transform: sp_model::LineTransform::None,
        letter_spacing: 0.0,
        margins: [size * 0.6, 0.0, size * 0.6, 0.0],
        line_fill: sp_model::LineFill::default(),
        list: sp_model::ListStyle::None,
        scroll: sp_model::Scroll::None,
        paragraph_spacing: 0.0,
        baseline: 0.0,
        gradient: sp_model::Gradient::default(),
        ..base.clone()
    };
    let (x, y) = (f64::from(band.x), f64::from(band.y));
    scene.push_clip_layer(Fill::NonZero, placed, outline);
    let strip = Rect::new(x, y, x + f64::from(band.w), y + f64::from(band.h));
    scene.fill(Fill::NonZero, placed, Color::from_rgba8(0, 0, 0, 170), None, &strip);
    let words = label.lines().next().unwrap_or_default();
    draw_text_box(scene, fonts, placed, (words, &[]), &look, band, 0.0, None);
    scene.pop_layer();
}

/// Says whether this thread draws a placeholder for a web page box whose page has no picture
/// yet: the operator's window (thumbnails, the editor) does; the screens never do, so the
/// audience sees nothing until the page has loaded and drawn.
pub fn show_placeholders(on: bool) {
    PLACEHOLDERS.with(|placeholders| placeholders.set(on));
}

/// Says whether this thread is drawing the Edit view's slide, where a web page, NDI source or
/// AirPlay box with nothing to show is drawn see-through ([`see_through_placeholder`]; the
/// owner's hand-off, 27 November: "in the editor"). Elsewhere in the operator's window (the
/// thumbnails, the playlist) such a box keeps its plain placeholder: a faint blue panel for a
/// web page, a dashed edge for the others. Only with [`show_placeholders`] on.
pub fn shade_empty_boxes(on: bool) {
    SHADED.with(|shaded| shaded.set(on));
}

/// What a web page box shows in the operator's window in place of its page, when the app says
/// it has a stand-in (a VoxLive screen's free test pattern for its live display, which costs).
static WEB_STAND_IN: std::sync::OnceLock<fn(&str) -> Option<String>> = std::sync::OnceLock::new();

/// Says which address stands in for a web page box's own in the operator's window (thumbnails,
/// the editor); the screens and the Program preview always show the box's own page. Set once.
pub fn set_web_stand_in(stand_in: fn(&str) -> Option<String>) {
    let _ = WEB_STAND_IN.set(stand_in);
}

/// The words of an NDI box's tag in the operator's window, in the app's language: given the
/// source's own name (`None` while none is chosen) and whether it has stopped sending. Without
/// them, English ("NDI · choose a source", "NDI · Camera 1 · not sending").
static NDI_TAG_WORDS: std::sync::OnceLock<fn(Option<&str>, bool) -> String> = std::sync::OnceLock::new();

/// Says how an NDI box's tag is worded in the operator's window (the screens never show it).
/// Set once.
pub fn set_ndi_tag_words(words: fn(Option<&str>, bool) -> String) {
    let _ = NDI_TAG_WORDS.set(words);
}

fn object_web_url(object: &sp_model::Object) -> &str {
    match &object.kind {
        ObjectKind::Web { url } => url,
        _ => "",
    }
}

/// A web page box: the page's newest picture, laid out at the box's size so it fills it; before
/// there is one, nothing on the screens and a placeholder with a globe in the operator's window
/// (a faint blue panel; in the Edit view the see-through one of [`see_through_placeholder`]).
fn draw_web(scene: &mut Scene, placed: Affine, url: &str, rect: SlideRect, outline: &BezPath) {
    let operator = PLACEHOLDERS.with(std::cell::Cell::get);
    // In the operator's window, a stand-in's picture first (its own may cost to open).
    let stand_in = operator.then(|| WEB_STAND_IN.get().and_then(|stand_in| stand_in(url))).flatten();
    for address in stand_in.iter().map(String::as_str).chain([url]) {
        if let Some(picture) = crate::pictures::shown(&crate::web_key(address, rect.w, rect.h)) {
            paint_picture(scene, placed, &picture, ImageFit::Stretch, rect, outline);
            return;
        }
    }
    if !operator {
        return;
    }
    let (x, y, w, h) = (f64::from(rect.x), f64::from(rect.y), f64::from(rect.w), f64::from(rect.h));
    if SHADED.with(std::cell::Cell::get) {
        see_through_placeholder(scene, placed, rect, outline, false);
    } else {
        scene.fill(Fill::NonZero, placed, Color::from_rgba8(40, 56, 84, 110), None, outline);
    }
    scene.stroke(&Stroke::new(3.0), placed, Color::from_rgba8(90, 110, 140, 255), None, outline);
    // A globe in the middle: a circle, its equator and a meridian.
    let radius = (w.min(h) * 0.18).clamp(12.0, 120.0);
    let centre = (x + w / 2.0, y + h / 2.0);
    let ink = Color::from_rgba8(120, 150, 190, 255);
    let line = Stroke::new((radius / 10.0).max(2.0));
    scene.stroke(&line, placed, ink, None, &vello::kurbo::Circle::new(centre, radius));
    scene.stroke(&line, placed, ink, None, &Ellipse::new(centre, (radius * 0.45, radius), 0.0));
    let mut equator = BezPath::new();
    equator.move_to((centre.0 - radius, centre.1));
    equator.line_to((centre.0 + radius, centre.1));
    scene.stroke(&line, placed, ink, None, &equator);
}

/// An NDI source box. On the screens: the source's newest picture, fitted as asked, whole while it
/// is sending and held for two seconds after it stops, then faded out over half a second (and
/// faded back in when it returns); before it has sent anything, nothing. No tag. In the
/// operator's window (the editor, thumbnails): its newest picture with a small tag at its foot;
/// a dashed box where there is none yet (see-through in the Edit view,
/// [`see_through_placeholder`]); an amber one, tagged "not sending", where the source is not
/// there.
#[allow(clippy::too_many_arguments)]
fn draw_ndi(
    scene: &mut Scene,
    fonts: &mut Fonts,
    placed: Affine,
    source: &str,
    fit: ImageFit,
    template: &sp_model::ThemeTemplate,
    rect: SlideRect,
    outline: &BezPath,
) {
    let operator = PLACEHOLDERS.with(std::cell::Cell::get);
    let shown = crate::pictures::ndi_shown(source);
    if !operator {
        if let Some(picture) = shown.picture.filter(|_| shown.visible > 0.0) {
            if shown.visible < 1.0 {
                scene.push_layer(Fill::NonZero, Mix::Normal, shown.visible, placed, outline);
                paint_picture(scene, placed, &picture, fit, rect, outline);
                scene.pop_layer();
            } else {
                paint_picture(scene, placed, &picture, fit, rect, outline);
            }
        }
        return;
    }
    let name = sp_model::ndi_short_name(source);
    let amber = Color::from_rgba8(255, 196, 0, 255);
    let stopped = !source.is_empty() && shown.stopped;
    let chosen = (!source.is_empty()).then_some(name);
    let tag = match NDI_TAG_WORDS.get() {
        Some(words) => words(chosen, stopped),
        None => match chosen {
            None => "NDI \u{b7} choose a source".to_owned(),
            Some(name) if stopped => format!("NDI \u{b7} {name} \u{b7} not sending"),
            Some(name) => format!("NDI \u{b7} {name}"),
        },
    };
    let picture = shown.picture.filter(|_| !stopped);
    match picture {
        Some(picture) => paint_picture(scene, placed, &picture, fit, rect, outline),
        None => {
            let ink = if stopped { amber } else { Color::from_rgba8(150, 150, 156, 255) };
            if SHADED.with(std::cell::Cell::get) {
                see_through_placeholder(scene, placed, rect, outline, stopped);
            } else if stopped {
                scene.fill(Fill::NonZero, placed, Color::from_rgba8(60, 44, 0, 150), None, outline);
            }
            let dashed = Stroke::new(4.0).with_dashes(0.0, [18.0, 12.0]);
            scene.stroke(&dashed, placed, ink, None, outline);
        }
    }
    draw_ndi_tag(scene, fonts, placed, &tag, stopped, template, rect, outline);
}

/// An AirPlay box: on the screens, the mirrored screen's newest picture (held and
/// faded as an NDI source's is when its sender goes) or nothing; in the operator's window, the
/// picture while somebody is showing one, else a dashed box (see-through in the Edit view,
/// [`see_through_placeholder`]), with a tag saying what the box is.
fn draw_mirror(
    scene: &mut Scene,
    fonts: &mut Fonts,
    placed: Affine,
    fit: ImageFit,
    template: &sp_model::ThemeTemplate,
    rect: SlideRect,
    outline: &BezPath,
) {
    let operator = PLACEHOLDERS.with(std::cell::Cell::get);
    let shown = crate::pictures::ndi_shown(crate::pictures::MIRROR_SOURCE);
    if !operator {
        if let Some(picture) = shown.picture.filter(|_| shown.visible > 0.0) {
            if shown.visible < 1.0 {
                scene.push_layer(Fill::NonZero, Mix::Normal, shown.visible, placed, outline);
                paint_picture(scene, placed, &picture, fit, rect, outline);
                scene.pop_layer();
            } else {
                paint_picture(scene, placed, &picture, fit, rect, outline);
            }
        }
        return;
    }
    match shown.picture.filter(|_| !shown.stopped) {
        Some(picture) => paint_picture(scene, placed, &picture, fit, rect, outline),
        None => {
            if SHADED.with(std::cell::Cell::get) {
                see_through_placeholder(scene, placed, rect, outline, false);
            }
            let dashed = Stroke::new(4.0).with_dashes(0.0, [18.0, 12.0]);
            scene.stroke(&dashed, placed, Color::from_rgba8(150, 150, 156, 255), None, outline);
        }
    }
    draw_ndi_tag(scene, fonts, placed, "AirPlay", false, template, rect, outline);
}

/// A web page, NDI source or AirPlay box with nothing to show yet, in the Edit view (the owner,
/// 27 November: "somewhat transparent to see whats behind"): a dark fill at 62% (30, 32,
/// 36), so what is behind it in the layers shows dimmed and what is in front of it is at full
/// strength, with a faint hatch over it as the design draws it: white at 7%, as CSS's
/// `repeating-linear-gradient(45deg, … 0 6%, transparent 6% 12%)`. `stopped` is an NDI source
/// that isn't sending: amber at 10% over the dark fill in place of the hatch.
fn see_through_placeholder(scene: &mut Scene, placed: Affine, rect: SlideRect, outline: &BezPath, stopped: bool) {
    scene.fill(Fill::NonZero, placed, Color::from_rgba8(30, 32, 36, 158), None, outline);
    if stopped {
        scene.fill(Fill::NonZero, placed, Color::from_rgba8(255, 196, 0, 26), None, outline);
        return;
    }
    // The gradient runs from the box's bottom left corner towards its top right, along a line
    // through its middle that is (w + h) / √2 long; its stripes cross that line.
    let (w, h) = (f64::from(rect.w), f64::from(rect.h));
    let length = (w + h) * std::f64::consts::FRAC_1_SQRT_2;
    let middle = (f64::from(rect.x) + w / 2.0, f64::from(rect.y) + h / 2.0);
    let along = Affine::translate(middle)
        * Affine::rotate(-std::f64::consts::FRAC_PI_4)
        * Affine::translate((-length / 2.0, 0.0));
    let (stripe, every) = (length * 0.06, length * 0.12);
    let mut stripes = BezPath::new();
    let mut at = 0.0;
    while at < length {
        stripes.move_to((at, -length));
        stripes.line_to((at + stripe, -length));
        stripes.line_to((at + stripe, length));
        stripes.line_to((at, length));
        stripes.close_path();
        at += every;
    }
    scene.push_clip_layer(Fill::NonZero, placed, outline);
    scene.fill(Fill::NonZero, placed * along, Color::from_rgba8(255, 255, 255, 18), None, &stripes);
    scene.pop_layer();
}

/// A small tag at the foot of an NDI box, on a dark strip, amber when the source is not sending.
#[allow(clippy::too_many_arguments)]
fn draw_ndi_tag(
    scene: &mut Scene,
    fonts: &mut Fonts,
    placed: Affine,
    tag: &str,
    amber: bool,
    template: &sp_model::ThemeTemplate,
    rect: SlideRect,
    outline: &BezPath,
) {
    let Some(base) = template.box_for(None) else { return };
    let size = (rect.h * 0.032).clamp(18.0, 30.0);
    let margin = size * 0.5;
    let width = (tag.chars().count() as f32 * size * 0.62 + size * 1.2).min(rect.w - margin * 2.0);
    let height = size * 1.6;
    let band = SlideRect { x: rect.x + margin, y: rect.y + rect.h - height - margin, w: width, h: height };
    let look = BoxStyle {
        size,
        min_size: size * 0.5,
        weight: 500,
        italic: false,
        fill: if amber { Rgba { r: 255, g: 196, b: 0, a: 255 } } else { Rgba::WHITE },
        align: sp_model::Align::Left,
        valign: VAlign::Middle,
        outline_width: 0.0,
        shadow: false,
        caps: sp_model::Caps::None,
        underline: false,
        strike: false,
        scaling: Scaling::TextDown,
        line_transform: sp_model::LineTransform::None,
        letter_spacing: 0.0,
        margins: [size * 0.5, 0.0, size * 0.5, 0.0],
        line_fill: sp_model::LineFill::default(),
        list: sp_model::ListStyle::None,
        scroll: sp_model::Scroll::None,
        paragraph_spacing: 0.0,
        baseline: 0.0,
        gradient: sp_model::Gradient::default(),
        ..base.clone()
    };
    let (x, y) = (f64::from(band.x), f64::from(band.y));
    scene.push_clip_layer(Fill::NonZero, placed, outline);
    let strip = RoundedRect::new(x, y, x + f64::from(band.w), y + f64::from(band.h), f64::from(size * 0.3));
    scene.fill(Fill::NonZero, placed, Color::from_rgba8(0, 0, 0, 190), None, &strip);
    draw_text_box(scene, fonts, placed, (tag, &[]), &look, band, 0.0, None);
    scene.pop_layer();
}

/// A picture object's picture in its place, fitted as it says. A picture that can't be read is
/// drawn as a grey box with a cross, so it can still be seen, moved and deleted.
fn draw_picture(scene: &mut Scene, placed: Affine, file: &str, fit: ImageFit, rect: SlideRect, outline: &BezPath) {
    let (x, y, w, h) = (f64::from(rect.x), f64::from(rect.y), f64::from(rect.w), f64::from(rect.h));
    // How large it is drawn, in pixels, says whether a small copy will do.
    let [a, b, ..] = placed.as_coeffs();
    let drawn = w.max(h) * a.hypot(b);
    let Some(picture) = crate::pictures::picture_at(file, drawn) else {
        // One being read for the screens shows nothing until it is there.
        if crate::pictures::coming(file) {
            return;
        }
        scene.fill(Fill::NonZero, placed, Color::from_rgba8(90, 90, 96, 255), None, outline);
        let mut cross = BezPath::new();
        cross.move_to((x, y));
        cross.line_to((x + w, y + h));
        cross.move_to((x + w, y));
        cross.line_to((x, y + h));
        scene.stroke(&Stroke::new(4.0), placed, Color::from_rgba8(150, 150, 156, 255), None, &cross);
        return;
    };
    paint_picture(scene, placed, &picture, fit, rect, outline);
}

/// A picture drawn in a place, fitted as asked and clipped to the outline.
fn paint_picture(
    scene: &mut Scene,
    placed: Affine,
    picture: &Picture,
    fit: ImageFit,
    rect: SlideRect,
    outline: &BezPath,
) {
    let (x, y, w, h) = (f64::from(rect.x), f64::from(rect.y), f64::from(rect.w), f64::from(rect.h));
    let (pic_w, pic_h) = (f64::from(picture.width()), f64::from(picture.height()));
    let (scale_x, scale_y) = match fit {
        ImageFit::Fit => {
            let scale = (w / pic_w).min(h / pic_h);
            (scale, scale)
        }
        ImageFit::Fill => {
            let scale = (w / pic_w).max(h / pic_h);
            (scale, scale)
        }
        ImageFit::Stretch => (w / pic_w, h / pic_h),
    };
    let offset = (x + (w - pic_w * scale_x) / 2.0, y + (h - pic_h * scale_y) / 2.0);
    scene.push_clip_layer(Fill::NonZero, placed, outline);
    scene.draw_image(&picture.image, placed * Affine::translate(offset) * Affine::scale_non_uniform(scale_x, scale_y));
    scene.pop_layer();
}

/// A box made as tall as its text at the text's own size (inside its margins), about its middle.
fn fitted_container(fonts: &mut Fonts, text: &str, spans: &[TextSpan], style: &BoxStyle, rect: SlideRect) -> SlideRect {
    let drawn = style.drawn(text, spans, false);
    let (text, spans) = (&*drawn.text, &*drawn.spans);
    let [left, top, right, bottom] = style.margins.map(|margin| margin.max(0.0));
    let width = (rect.w - left - right).max(1.0);
    let height = lay_out(fonts, text, spans, style, style.size, width).height()
        + gaps_total(text, style, style.size)
        + top
        + bottom;
    SlideRect { y: rect.y + (rect.h - height) / 2.0, h: height.max(1.0), ..rect }
}

/// A shape filling a place on the slide, as a path in slide units.
fn shape_path(shape: Shape, rect: SlideRect) -> BezPath {
    let (x, y, w, h) = (f64::from(rect.x), f64::from(rect.y), f64::from(rect.w), f64::from(rect.h));
    let bounds = Rect::new(x, y, x + w, y + h);
    match (shape, shape.corners()) {
        (_, Some(points)) => {
            let mut path = BezPath::new();
            for (index, (px, py)) in points.into_iter().enumerate() {
                let point = (x + px * w, y + py * h);
                if index == 0 {
                    path.move_to(point);
                } else {
                    path.line_to(point);
                }
            }
            path.close_path();
            path
        }
        (Shape::Ellipse, None) => Ellipse::from_rect(bounds).to_path(0.1),
        (_, None) => RoundedRect::from_rect(bounds, w.min(h) * 0.12).to_path(0.1),
    }
}

/// One picture fading into another: `from` with `to` over it, `amount` of the way in (0 is
/// all `from`, 1 is all `to`). Both must have been built for a target of this size.
pub fn cross_fade(from: &Scene, to: &Scene, amount: f32, target_w: u32, target_h: u32) -> Scene {
    let mut scene = from.clone();
    let whole = Rect::new(0.0, 0.0, f64::from(target_w), f64::from(target_h));
    scene.push_layer(Fill::NonZero, Mix::Normal, amount.clamp(0.0, 1.0), Affine::IDENTITY, &whole);
    scene.append(to, None);
    scene.pop_layer();
    scene
}

/// Whether a transition of this kind may be drawn from textures of its two pictures (see
/// [`Painter::cache_whole`](crate::Painter::cache_whole)), blended by [`transition`]. The kinds
/// that only fade, or show one picture through a clip, put each texel where the picture had it;
/// the ones that move or scale a picture sample the texture between texels, a little softer than
/// drawing the slide again at the new place.
pub fn fade_cacheable(kind: sp_model::TransitionKind) -> bool {
    use sp_model::TransitionKind as K;
    matches!(
        kind,
        K::Dissolve
            | K::Fade
            | K::FadeBlack
            | K::FadeWhite
            | K::Wipe
            | K::Split
            | K::Iris
            | K::Clock
            | K::Blinds
            | K::Checkerboard
    )
}

/// One picture giving way to another by a transition, `amount` of the way through (0 is all
/// `from`, 1 is all `to`). Both must have been built for a target of this size.
pub fn transition(
    kind: sp_model::TransitionKind,
    direction: sp_model::Direction,
    from: &Scene,
    to: &Scene,
    amount: f32,
    target_w: u32,
    target_h: u32,
) -> Scene {
    use sp_model::TransitionKind as K;
    let a = f64::from(amount.clamp(0.0, 1.0));
    let (w, h) = (f64::from(target_w), f64::from(target_h));
    let whole = Rect::new(0.0, 0.0, w, h);
    let (dx, dy) = direction.travel();
    // Where the new picture starts, off the screen on the side it comes from.
    let away = (-dx * w, -dy * h);
    let mut scene = Scene::new();
    scene.fill(Fill::NonZero, Affine::IDENTITY, Color::BLACK, None, &whole);
    match kind {
        K::Cut => scene.append(to, None),
        K::Dissolve => return cross_fade(from, to, amount, target_w, target_h),
        K::FadeBlack | K::FadeWhite => {
            let through = if kind == K::FadeBlack { Color::BLACK } else { Color::WHITE };
            if a < 0.5 {
                scene.append(from, None);
                scene.push_layer(Fill::NonZero, Mix::Normal, (a * 2.0) as f32, Affine::IDENTITY, &whole);
                scene.fill(Fill::NonZero, Affine::IDENTITY, through, None, &whole);
            } else {
                scene.fill(Fill::NonZero, Affine::IDENTITY, through, None, &whole);
                scene.push_layer(Fill::NonZero, Mix::Normal, ((a - 0.5) * 2.0) as f32, Affine::IDENTITY, &whole);
                scene.append(to, None);
            }
            scene.pop_layer();
        }
        K::Fade => {
            // Each fades on its own over black, so the middle is darker than a dissolve.
            scene.push_layer(Fill::NonZero, Mix::Normal, (1.0 - a) as f32, Affine::IDENTITY, &whole);
            scene.append(from, None);
            scene.pop_layer();
            scene.push_layer(Fill::NonZero, Mix::Normal, a as f32, Affine::IDENTITY, &whole);
            scene.append(to, None);
            scene.pop_layer();
        }
        K::Push => {
            scene.append(from, Some(Affine::translate((dx * w * a, dy * h * a))));
            scene.append(to, Some(Affine::translate((away.0 * (1.0 - a), away.1 * (1.0 - a)))));
        }
        K::MoveIn => {
            scene.append(from, None);
            scene.append(to, Some(Affine::translate((away.0 * (1.0 - a), away.1 * (1.0 - a)))));
        }
        K::Reveal => {
            scene.append(to, None);
            scene.append(from, Some(Affine::translate((dx * w * a, dy * h * a))));
        }
        K::FlyIn => {
            // In from the side, growing from small to full size as it lands.
            scene.append(from, None);
            let size = about_middle(w, h, Affine::scale(0.5 + 0.5 * a));
            let travel = Affine::translate((away.0 * (1.0 - a), away.1 * (1.0 - a)));
            scene.append(to, Some(travel * size));
        }
        K::ZoomIn => {
            // Grows from the middle of the side it comes from as it fades in.
            scene.append(from, None);
            let anchor = (w / 2.0 - dx * w / 2.0, h / 2.0 - dy * h / 2.0);
            let grow =
                Affine::translate(anchor) * Affine::scale(0.6 + 0.4 * a) * Affine::translate((-anchor.0, -anchor.1));
            scene.push_layer(Fill::NonZero, Mix::Normal, a as f32, Affine::IDENTITY, &whole);
            scene.append(to, Some(grow));
            scene.pop_layer();
        }
        K::Swap => {
            // The old steps back and away; the new comes forward from the other side.
            let back = Affine::translate((dx * w * 0.5 * a, dy * h * 0.5 * a))
                * about_middle(w, h, Affine::scale(1.0 - 0.3 * a));
            scene.push_layer(Fill::NonZero, Mix::Normal, (1.0 - a) as f32, Affine::IDENTITY, &whole);
            scene.append(from, Some(back));
            scene.pop_layer();
            let forward = Affine::translate((away.0 * 0.5 * (1.0 - a), away.1 * 0.5 * (1.0 - a)))
                * about_middle(w, h, Affine::scale(0.7 + 0.3 * a));
            scene.push_layer(Fill::NonZero, Mix::Normal, a as f32, Affine::IDENTITY, &whole);
            scene.append(to, Some(forward));
            scene.pop_layer();
        }
        K::Wipe => {
            scene.append(from, None);
            // The edge sweeps the way the new picture travels.
            let uncovered = match direction {
                sp_model::Direction::FromLeft => Rect::new(0.0, 0.0, w * a, h),
                sp_model::Direction::FromRight => Rect::new(w * (1.0 - a), 0.0, w, h),
                sp_model::Direction::FromTop => Rect::new(0.0, 0.0, w, h * a),
                sp_model::Direction::FromBottom => Rect::new(0.0, h * (1.0 - a), w, h),
            };
            scene.push_clip_layer(Fill::NonZero, Affine::IDENTITY, &uncovered);
            scene.append(to, None);
            scene.pop_layer();
        }
        K::Iris => {
            scene.append(from, None);
            let reach = (w * w + h * h).sqrt() / 2.0;
            let circle = Ellipse::new((w / 2.0, h / 2.0), (reach * a, reach * a), 0.0);
            scene.push_clip_layer(Fill::NonZero, Affine::IDENTITY, &circle);
            scene.append(to, None);
            scene.pop_layer();
        }
        K::Door => {
            scene.append(to, None);
            let across = dx != 0.0;
            let (first, second, apart) = if across {
                (Rect::new(0.0, 0.0, w / 2.0, h), Rect::new(w / 2.0, 0.0, w, h), (w / 2.0 * a, 0.0))
            } else {
                (Rect::new(0.0, 0.0, w, h / 2.0), Rect::new(0.0, h / 2.0, w, h), (0.0, h / 2.0 * a))
            };
            for (half, sign) in [(first, -1.0), (second, 1.0)] {
                let moved = Affine::translate((apart.0 * sign, apart.1 * sign));
                scene.push_clip_layer(Fill::NonZero, moved, &half);
                scene.append(from, Some(moved));
                scene.pop_layer();
            }
        }
        K::Split => {
            scene.append(from, None);
            let open = if dx != 0.0 {
                Rect::new(w / 2.0 * (1.0 - a), 0.0, w / 2.0 * (1.0 + a), h)
            } else {
                Rect::new(0.0, h / 2.0 * (1.0 - a), w, h / 2.0 * (1.0 + a))
            };
            scene.push_clip_layer(Fill::NonZero, Affine::IDENTITY, &open);
            scene.append(to, None);
            scene.pop_layer();
        }
        K::Flip => {
            // Edge-on halfway: the old picture narrows to a line, then the new one widens from it.
            let (picture, open) = if a < 0.5 { (from, 1.0 - a * 2.0) } else { (to, a * 2.0 - 1.0) };
            let squeeze = if dx != 0.0 {
                Affine::scale_non_uniform(open.max(0.0001), 1.0)
            } else {
                Affine::scale_non_uniform(1.0, open.max(0.0001))
            };
            scene.append(picture, Some(about_middle(w, h, squeeze)));
        }
        K::Clock => {
            scene.append(from, None);
            let reach = (w * w + h * h).sqrt();
            let hand = CircleSegment::new(
                (w / 2.0, h / 2.0),
                reach,
                0.0,
                -std::f64::consts::FRAC_PI_2,
                a * std::f64::consts::TAU,
            );
            scene.push_clip_layer(Fill::NonZero, Affine::IDENTITY, &hand.to_path(0.1));
            scene.append(to, None);
            scene.pop_layer();
        }
        K::Blinds | K::Checkerboard => {
            scene.append(from, None);
            let uncovered = strips(kind == K::Blinds, direction, a, w, h);
            scene.push_clip_layer(Fill::NonZero, Affine::IDENTITY, &uncovered);
            scene.append(to, None);
            scene.pop_layer();
        }
    }
    scene
}

/// A change made about the middle of a target of this size, not its corner.
fn about_middle(w: f64, h: f64, change: Affine) -> Affine {
    Affine::translate((w / 2.0, h / 2.0)) * change * Affine::translate((-w / 2.0, -h / 2.0))
}

/// The part uncovered so far, `a` of the way through: by blinds (strips side by side, each
/// opening the way the picture travels), or by a checkerboard (every other square opening in
/// the first half of the time, the rest in the second).
fn strips(blinds: bool, direction: sp_model::Direction, a: f64, w: f64, h: f64) -> BezPath {
    let (dx, dy) = direction.travel();
    let across = dx != 0.0;
    let forward = if across { dx > 0.0 } else { dy > 0.0 };
    let mut path = BezPath::new();
    // A rectangle `t` of the way to covering `cell`, growing the way the picture travels.
    let mut grow = |cell: Rect, t: f64| {
        let t = t.clamp(0.0, 1.0);
        if t <= 0.0 {
            return;
        }
        let part = match (across, forward) {
            (true, true) => Rect::new(cell.x0, cell.y0, cell.x0 + cell.width() * t, cell.y1),
            (true, false) => Rect::new(cell.x1 - cell.width() * t, cell.y0, cell.x1, cell.y1),
            (false, true) => Rect::new(cell.x0, cell.y0, cell.x1, cell.y0 + cell.height() * t),
            (false, false) => Rect::new(cell.x0, cell.y1 - cell.height() * t, cell.x1, cell.y1),
        };
        path.extend(part.path_elements(0.1));
    };
    if blinds {
        const COUNT: usize = 8;
        for at in 0..COUNT {
            let (f, g) = (at as f64 / COUNT as f64, (at + 1) as f64 / COUNT as f64);
            let cell = if across { Rect::new(w * f, 0.0, w * g, h) } else { Rect::new(0.0, h * f, w, h * g) };
            grow(cell, a);
        }
    } else {
        const COLUMNS: usize = 8;
        const ROWS: usize = 6;
        for row in 0..ROWS {
            for column in 0..COLUMNS {
                let cell = Rect::new(
                    w * column as f64 / COLUMNS as f64,
                    h * row as f64 / ROWS as f64,
                    w * (column + 1) as f64 / COLUMNS as f64,
                    h * (row + 1) as f64 / ROWS as f64,
                );
                let second = (row + column) % 2 == 1;
                grow(cell, if second { a * 2.0 - 1.0 } else { a * 2.0 });
            }
        }
    }
    path
}

/// Where an object sits on its slide, in slide units: its own place if it has one, otherwise
/// the theme's place for its role. The editor uses this to find and outline objects.
pub fn object_frame(slide: &Slide, object: &Object, theme: &Theme) -> SlideRect {
    object.frame.unwrap_or_else(|| theme.place_on(slide, object.role.as_ref()))
}

pub(crate) fn lay_out(
    fonts: &mut Fonts,
    text: &str,
    spans: &[TextSpan],
    style: &BoxStyle,
    size: f32,
    width: f32,
) -> Layout<Ink> {
    lay_out_within(fonts, text, spans, style, size, Some(width))
}

/// Lays text out at a size, wrapping it at `width`, or on lines as long as they are with `None`.
/// `spans` (counted in the characters of `text`) set their own look over the box's; their sizes
/// and the spacing between letters grow and shrink with `size` as the box's own do.
fn lay_out_within(
    fonts: &mut Fonts,
    text: &str,
    spans: &[TextSpan],
    style: &BoxStyle,
    size: f32,
    width: Option<f32>,
) -> Layout<Ink> {
    fonts.starting_layout();
    let mut builder = fonts.layout_cx.ranged_builder(&mut fonts.font_cx, text, 1.0, true);
    builder.push_default(StyleProperty::FontSize(size));
    builder.push_default(StyleProperty::FontWeight(FontWeight::new(f32::from(style.weight))));
    if style.italic {
        builder.push_default(StyleProperty::FontStyle(parley::FontStyle::Italic));
    }
    builder.push_default(StyleProperty::Underline(style.underline));
    builder.push_default(StyleProperty::Strikethrough(style.strike));
    builder.push_default(StyleProperty::LineHeight(LineHeight::FontSizeRelative(style.line_height.clamp(0.5, 4.0))));
    // Letter spacing is set at the box's font size; text shrunk to fit keeps it in proportion.
    if style.letter_spacing != 0.0 {
        builder.push_default(StyleProperty::LetterSpacing(style.letter_spacing * size / style.size.max(1.0)));
    }
    builder.push_default(StyleProperty::FontFamily(style.font_family.as_str().into()));
    let scale = size / style.size.max(1.0);
    for (range, own) in sp_model::byte_ranges(text, spans) {
        if let Some(family) = &own.font_family {
            builder.push(StyleProperty::FontFamily(family.as_str().into()), range.clone());
        }
        if let Some(own_size) = own.size {
            builder.push(StyleProperty::FontSize(own_size * scale), range.clone());
        }
        if let Some(weight) = own.weight {
            builder.push(StyleProperty::FontWeight(FontWeight::new(f32::from(weight))), range.clone());
        }
        if let Some(italic) = own.italic {
            let slant = if italic { parley::FontStyle::Italic } else { parley::FontStyle::Normal };
            builder.push(StyleProperty::FontStyle(slant), range.clone());
        }
        if let Some(underline) = own.underline {
            builder.push(StyleProperty::Underline(underline), range.clone());
        }
        if let Some(strike) = own.strike {
            builder.push(StyleProperty::Strikethrough(strike), range.clone());
        }
        if own.fill.is_some() {
            builder.push(StyleProperty::Brush(Ink(own.fill)), range);
        }
    }
    let mut layout: Layout<Ink> = builder.build(text);
    let alignment = match style.align {
        Align::Left => Alignment::Left,
        Align::Center => Alignment::Center,
        Align::Right => Alignment::Right,
        Align::Justify => Alignment::Justify,
    };
    // Lines wider than the box (one very long word) stay centred instead of jumping left.
    layout.set_alignment(alignment, AlignmentOptions { align_when_overflowing: true, ..AlignmentOptions::default() });
    layout.break_all_lines(width);
    layout
}

/// How fast scrolling text moves, in slide units a second.
const SCROLL_SPEED: f32 = 160.0;

/// Where scrolling text starts along its way, `time` seconds after it was put on: it moves by
/// `SCROLL_SPEED`, and once it has gone right out of the box it comes back in from the other
/// side. `room` is the box's length that way, `length` the text's. At time 0 it is where it
/// starts, at the box's start.
fn scrolled(way: sp_model::Scroll, time: f32, room: f32, length: f32) -> f32 {
    let lap = (room + length).max(1.0);
    let gone = (time.max(0.0) * SCROLL_SPEED) % lap;
    match way {
        // Moving towards the start: out past the start, then back in at the end.
        sp_model::Scroll::Left | sp_model::Scroll::Up => {
            if gone <= length {
                -gone
            } else {
                room - (gone - length)
            }
        }
        // Moving towards the end: out past the end, then back in at the start.
        sp_model::Scroll::Right | sp_model::Scroll::Down | sp_model::Scroll::None => {
            if gone <= room {
                gone
            } else {
                -(lap - gone)
            }
        }
    }
}

/// True when a slide has text that scrolls, so it is drawn again at each refresh.
pub fn scrolls(slide: &Slide, theme: &Theme) -> bool {
    slide.objects.iter().filter(|object| !object.hidden).any(|object| match &object.kind {
        ObjectKind::Text { style, .. } => theme
            .text_style_on(slide, object.role.as_ref(), style)
            .is_some_and(|look| look.scroll != sp_model::Scroll::None),
        _ => false,
    })
}

/// Lays the text out at the style's size, shrinking in small steps until it fits the box's
/// height or reaches the smallest allowed size. Returns the layout and whether it still
/// overflows.
fn fit(
    fonts: &mut Fonts,
    text: &str,
    spans: &[TextSpan],
    style: &BoxStyle,
    rect: SlideRect,
) -> (Layout<Ink>, bool, f32) {
    let too_big = |layout: &Layout<Ink>, size: f32| {
        layout.height() + gaps_total(text, style, size) > rect.h || layout.width() > rect.w + 0.5
    };
    let (shrinks, grows) = match style.scaling {
        Scaling::None => (false, false),
        Scaling::TextDown => (true, false),
        Scaling::TextUp => (false, true),
        Scaling::TextUpOrDown => (true, true),
        // The box is fitted to the text before it gets here.
        Scaling::ContainerUpOrDown => (false, false),
    };
    // Fitted before (the same slide drawn again): laid out once, at the size it came to.
    let box_size = (rect.w, rect.h);
    if let Some(size) = fonts.fitted_size(text, spans, style, box_size) {
        let layout = lay_out(fonts, text, spans, style, size, rect.w);
        let overflow = too_big(&layout, size);
        return (layout, overflow, size);
    }
    let floor = style.min_size.min(style.size).max(1.0);
    let mut size = style.size.max(floor);
    let mut layout = lay_out(fonts, text, spans, style, size, rect.w);
    // A written line a little too long for the box is made a little smaller, so it stays on one
    // row, as lyrics are written; one much too long (a paragraph, a Bible verse) wraps instead.
    if shrinks && wraps(&layout, text) {
        let lowest = (style.size * KEEP_LINES_DOWN_TO).max(floor);
        let mut trial = size;
        while trial > lowest {
            trial = (trial * 0.96).max(lowest);
            let smaller = lay_out(fonts, text, spans, style, trial, rect.w);
            if !wraps(&smaller, text) {
                (size, layout) = (trial, smaller);
                break;
            }
        }
    }
    if too_big(&layout, size) {
        while shrinks && too_big(&layout, size) && size > floor {
            size = (size * 0.96).max(floor);
            layout = lay_out(fonts, text, spans, style, size, rect.w);
        }
    } else if grows && !text.trim().is_empty() {
        // Grow in small steps and keep the last size that still fitted.
        while size < LARGEST_GROWN {
            let bigger = lay_out(fonts, text, spans, style, size * 1.04, rect.w);
            if too_big(&bigger, size * 1.04) {
                break;
            }
            size *= 1.04;
            layout = bigger;
        }
    }
    let overflow = too_big(&layout, size);
    fonts.remember_fit(text, spans, style, box_size, size);
    (layout, overflow, size)
}

/// How far below its size text is made smaller to keep each written line on one row, before
/// it is left to wrap.
const KEEP_LINES_DOWN_TO: f32 = 0.7;

/// Whether a written line of the text runs onto more than one row.
fn wraps(layout: &Layout<Ink>, text: &str) -> bool {
    layout.len() > text.trim_end().split('\n').count()
}

/// The paragraph spacing at a size: set at the box's font size, kept in proportion when the
/// text is shrunk or grown to fit.
fn spacing_at(style: &BoxStyle, size: f32) -> f32 {
    style.paragraph_spacing.max(0.0) * size / style.size.max(1.0)
}

/// How much paragraph spacing adds to the text's height: one gap for each line break.
fn gaps_total(text: &str, style: &BoxStyle, size: f32) -> f32 {
    spacing_at(style, size) * text.trim_end().matches('\n').count() as f32
}

/// Which paragraph a line of the layout starts in (0 for the first): the line breaks before it.
fn paragraph_of(text: &str, line_start: usize) -> usize {
    text.get(..line_start).map_or(0, |before| before.matches('\n').count())
}

/// A gradient over a box, as a brush, from `from` to the gradient's own colour.
fn gradient_brush(gradient: &sp_model::Gradient, from: Rgba, area: Rect) -> vello::peniko::Brush {
    let (from, to) = (color(from), color(gradient.to));
    let centre = area.center();
    let made = match gradient.kind {
        sp_model::GradientKind::Radial => {
            let radius = (area.width().max(area.height()) / 2.0) as f32;
            vello::peniko::Gradient::new_radial(centre, radius.max(1.0))
        }
        _ => {
            let angle = f64::from(gradient.angle).to_radians();
            let (dx, dy) = (angle.cos(), angle.sin());
            // Half the box's extent along the way the gradient runs.
            let half = (area.width() * dx.abs() + area.height() * dy.abs()) / 2.0;
            vello::peniko::Gradient::new_linear(
                (centre.x - dx * half, centre.y - dy * half),
                (centre.x + dx * half, centre.y + dy * half),
            )
        }
    };
    vello::peniko::Brush::Gradient(made.with_stops([from, to]))
}

/// Text made bigger to fill its box stops growing here, in slide units.
const LARGEST_GROWN: f32 = 1200.0;

fn color(c: Rgba) -> Color {
    Color::from_rgba8(c.r, c.g, c.b, c.a)
}

/// Draws one box: soft shadow, then outline, then fill. Returns whether the text overflowed,
/// and where the words it drew lie: `None` when it drew none (no words, or none of their
/// paragraphs shown yet), or when they scroll and so lie nowhere for long.
#[allow(clippy::too_many_arguments)]
fn draw_text_box(
    scene: &mut Scene,
    fonts: &mut Fonts,
    to_target: Affine,
    words: (&str, &[TextSpan]),
    style: &BoxStyle,
    rect: SlideRect,
    time: f32,
    paragraphs: Option<usize>,
) -> (bool, Option<Laid>) {
    text_box(Some(scene), fonts, to_target, words, style, rect, time, paragraphs)
}

/// Whether a box's words would overflow, and where they would lie, if it were drawn in `rect`,
/// drawing nothing: the same fitting and placing as [`draw_text_box`], which it is.
fn measure_text_box(
    fonts: &mut Fonts,
    words: (&str, &[TextSpan]),
    style: &BoxStyle,
    rect: SlideRect,
    time: f32,
    paragraphs: Option<usize>,
) -> (bool, Option<Laid>) {
    text_box(None, fonts, Affine::IDENTITY, words, style, rect, time, paragraphs)
}

/// [`draw_text_box`], or with no scene only its fitting and placing ([`measure_text_box`]).
#[allow(clippy::too_many_arguments)]
fn text_box(
    scene: Option<&mut Scene>,
    fonts: &mut Fonts,
    to_target: Affine,
    words: (&str, &[TextSpan]),
    style: &BoxStyle,
    rect: SlideRect,
    time: f32,
    paragraphs: Option<usize>,
) -> (bool, Option<Laid>) {
    // Text moving sideways runs on one long line.
    let drawn = style.drawn(words.0, words.1, style.scroll.sideways());
    let (text, spans) = (&*drawn.text, &*drawn.spans);
    // The text is laid out inside the box's margins.
    let [left, top, right, bottom] = style.margins.map(|margin| margin.max(0.0));
    let rect = SlideRect {
        x: rect.x + left,
        y: rect.y + top,
        w: (rect.w - left - right).max(1.0),
        h: (rect.h - top - bottom).max(1.0),
    };
    // Scrolling text keeps its size: it moves instead of being made to fit.
    let scroll = style.scroll;
    let (layout, overflow, size) = match scroll {
        sp_model::Scroll::None => fit(fonts, text, spans, style, rect),
        way if way.sideways() => (lay_out_within(fonts, text, spans, style, style.size, None), false, style.size),
        _ => (lay_out(fonts, text, spans, style, style.size, rect.w), false, style.size),
    };
    // Paragraph spacing moves each line down by a gap for each line break above it.
    let spacing = f64::from(spacing_at(style, size));
    let block_height = layout.height() + gaps_total(text, style, size);
    let gap_before = |line_start: usize| spacing * paragraph_of(text, line_start) as f64;
    // A build delivered bullet by bullet shows only the paragraphs it has reached.
    let shows = |line_start: usize| paragraphs.is_none_or(|shown| paragraph_of(text, line_start) < shown);
    // Place the text block in its box (in the middle it may spill both ways when it overflows).
    let spare = f64::from(rect.h) - f64::from(block_height);
    let mut left = f64::from(rect.x);
    let mut top = f64::from(rect.y)
        + match style.valign {
            VAlign::Top => 0.0,
            VAlign::Middle => spare / 2.0,
            VAlign::Bottom => spare,
        };
    if scroll.sideways() {
        left += f64::from(scrolled(scroll, time, rect.w, layout.width()));
    } else if scroll != sp_model::Scroll::None {
        top = f64::from(rect.y) + f64::from(scrolled(scroll, time, rect.h, block_height));
    }
    // The baseline raises (or lowers) the whole block, in proportion to the size it is drawn at.
    top -= f64::from(style.baseline * size / style.size.max(1.0));
    // Where the words lie: from the first line shown to the last, each with the gaps above it.
    let mut laid: Option<Laid> = None;
    if scroll == sp_model::Scroll::None && !text.trim().is_empty() {
        for line in layout.lines().filter(|line| shows(line.text_range().start)) {
            let (metrics, gap) = (line.metrics(), gap_before(line.text_range().start));
            let line_top = (top + f64::from(metrics.block_min_coord) + gap) as f32;
            let bottom = (top + f64::from(metrics.block_max_coord) + gap) as f32;
            let top = laid.map_or(line_top, |above| above.top);
            // The line's reach across: from where it starts to where its last letter ends (a
            // justified line, spread over the box, its whole width).
            let (start, end) = if style.align == Align::Justify {
                (metrics.inline_min_coord, metrics.inline_max_coord)
            } else {
                (metrics.offset, metrics.offset + metrics.advance)
            };
            let (start, end) = ((left + f64::from(start)) as f32, (left + f64::from(end)) as f32);
            let (start, end) = laid.map_or((start, end), |above| (above.left.min(start), above.right.max(end)));
            laid = Some(Laid { size, centre: rect.x + rect.w / 2.0, top, bottom, left: start, right: end });
        }
    }
    // Only measured: where the words lie is all that was asked.
    let Some(scene) = scene else { return (overflow, laid) };
    // Scrolling text shows only inside its box.
    let clipped = scroll != sp_model::Scroll::None;
    if clipped {
        let (x, y) = (f64::from(rect.x), f64::from(rect.y));
        scene.push_clip_layer(Fill::NonZero, to_target, &Rect::new(x, y, x + f64::from(rect.w), y + f64::from(rect.h)));
    }
    let origin = to_target * Affine::translate((left, top));
    let outline_width = f64::from(style.outline_width);
    let outline = Stroke::new(outline_width * 2.0).with_join(Join::Round);
    // The text's colour, or a gradient over the whole block of text.
    let block = Rect::new(0.0, 0.0, f64::from(rect.w), f64::from(block_height));
    let fill: vello::peniko::Brush =
        if style.gradient.on() { gradient_brush(&style.gradient, style.fill, block) } else { color(style.fill).into() };
    let outline_color = color(style.outline_color);
    let look = style.shadow_look;
    let fall = f64::from(look.angle).to_radians();
    // The angle is counted anticlockwise, and the slide's y runs downwards.
    let shadow_offset = (f64::from(look.offset) * fall.cos(), -f64::from(look.offset) * fall.sin());
    // Six faint layers add up to the wanted opacity where they all overlap.
    let layer_alpha = 1.0 - (1.0 - look.opacity.clamp(0.0, 1.0)).powf(1.0 / 6.0);
    let spread = f64::from(look.blur.max(0.0)) / 3.0;

    // A band of colour behind each line: under its words, or across the box.
    if style.line_fill.on {
        let pad_x = f64::from(style.size) * 0.2;
        let pad_y = f64::from(style.size) * 0.06;
        let corner = f64::from(style.size) * 0.12;
        for line in layout.lines() {
            let m = line.metrics();
            if m.advance - m.hanging_advance <= 0.0 || !shows(line.text_range().start) {
                continue;
            }
            let origin = origin * Affine::translate((0.0, gap_before(line.text_range().start)));
            let (left, right) = if style.line_fill.full_width {
                (0.0, f64::from(rect.w))
            } else {
                (f64::from(m.offset) - pad_x, f64::from(m.offset + m.advance - m.hanging_advance) + pad_x)
            };
            let band = RoundedRect::new(
                left,
                f64::from(m.content_block_min_coord) - pad_y,
                right,
                f64::from(m.content_block_max_coord) + pad_y,
                if style.line_fill.full_width { 0.0 } else { corner },
            );
            scene.fill(Fill::NonZero, origin, color(style.line_fill.color), None, &band);
        }
    }

    for pass in [Pass::Shadow, Pass::Outline, Pass::Fill] {
        if (pass == Pass::Shadow && !style.shadow) || (pass == Pass::Outline && outline_width <= 0.0) {
            continue;
        }
        for line in layout.lines() {
            let start = line.text_range().start;
            if !shows(start) {
                continue;
            }
            let gap = gap_before(start);
            let origin = origin * Affine::translate((0.0, gap));
            // The gradient stays put over the whole block, whichever line it is drawn on.
            let brush_place = Some(Affine::translate((0.0, -gap)));
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(run) = item else {
                    continue;
                };
                let font = run.run().font();
                let size = run.run().font_size();
                // A span's own colour is a flat one, over any gradient the box has.
                let own_ink: Option<vello::peniko::Brush> = run.style().brush.0.map(|ink| color(ink).into());
                let run_fill = own_ink.as_ref().unwrap_or(&fill);
                let coords: Vec<i16> = run.run().normalized_coords().iter().map(|c| c.to_bits()).collect();
                let glyphs: Vec<Glyph> = run.positioned_glyphs().map(|g| Glyph { id: g.id, x: g.x, y: g.y }).collect();
                match pass {
                    Pass::Shadow => {
                        // The renderer cannot blur glyphs, so a soft shadow is several faint,
                        // widening strokes, offset down and to the right.
                        let shadow = origin * Affine::translate(shadow_offset);
                        for step in 1..=6 {
                            let stroke =
                                Stroke::new(outline_width * 2.0 + f64::from(step) * spread).with_join(Join::Round);
                            scene
                                .draw_glyphs(font)
                                .font_size(size)
                                .transform(shadow)
                                .normalized_coords(&coords)
                                .brush(color(look.color))
                                .brush_alpha(layer_alpha)
                                .draw(&stroke, glyphs.iter().copied());
                        }
                    }
                    Pass::Outline => scene
                        .draw_glyphs(font)
                        .font_size(size)
                        .transform(origin)
                        .normalized_coords(&coords)
                        .brush(outline_color)
                        .draw(&outline, glyphs.iter().copied()),
                    Pass::Fill => scene
                        .draw_glyphs(font)
                        .font_size(size)
                        .transform(origin)
                        .brush_transform(brush_place)
                        .normalized_coords(&coords)
                        .brush(run_fill)
                        .draw(Fill::NonZero, glyphs.iter().copied()),
                }
                // Underline and strike-through are bars across the run, in the text's colour.
                if pass == Pass::Fill {
                    let metrics = run.run().font_metrics();
                    let (left, baseline) = (f64::from(run.offset()), f64::from(run.baseline()));
                    let right = left + f64::from(run.advance());
                    let bars = [
                        (run.style().underline.is_some(), metrics.underline_offset, metrics.underline_size),
                        (run.style().strikethrough.is_some(), metrics.strikethrough_offset, metrics.strikethrough_size),
                    ];
                    for (wanted, offset, thickness) in bars {
                        if wanted {
                            let y = baseline - f64::from(offset);
                            let bar = Rect::new(left, y, right, y + f64::from(thickness.max(1.0)));
                            scene.fill(Fill::NonZero, origin, run_fill, brush_place, &bar);
                        }
                    }
                }
            }
        }
    }
    if clipped {
        scene.pop_layer();
    }
    (overflow, laid)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pass {
    Shadow,
    Outline,
    Fill,
}

#[cfg(test)]
mod tests {
    use sp_model::parse_song_text;

    use super::*;
    use sp_model::Role;

    #[test]
    fn the_editors_backdrop_shows_only_through_no_background_or_plain_black() {
        // No background colour, or the plain black a screen is anyway: the backdrop shows.
        assert!(plain_black(None));
        assert!(plain_black(Some(Rgba { r: 0, g: 0, b: 0, a: 255 })));
        // A theme's own colour, even a dark one or a see-through black, is drawn as on screen.
        assert!(!plain_black(Some(Rgba { r: 10, g: 20, b: 30, a: 255 })));
        assert!(!plain_black(Some(Rgba { r: 0, g: 0, b: 1, a: 255 })));
        assert!(!plain_black(Some(Rgba { r: 0, g: 0, b: 0, a: 128 })));
        // The editor's picture draws the same words as a screen's, over either backdrop, and
        // a screen's own scene is built without any of it.
        let mut fonts = Fonts::new();
        let theme = Theme::builtin();
        let slide = slide("We lift our voices", None);
        let layers = Layers {
            media_fit: ImageFit::Fill,
            media: None,
            slide: Some(&slide),
            overlay: None,
            time: 0.0,
            backdrop: None,
            mask: [0.0; 4],
            announcement: None,
        };
        let on_screen = build_layers(&mut fonts, &layers, &theme, 1920, 1080);
        let greys = [Rgba { r: 28, g: 28, b: 28, a: 255 }, Rgba { r: 42, g: 42, b: 42, a: 255 }];
        for backdrop in [
            EditorBackdrop::Colour(Rgba { r: 0xb9, g: 0xb9, b: 0xb9, a: 255 }),
            EditorBackdrop::Checker { square: 80.0, colours: greys },
        ] {
            let edited = build_layers_for_editor(&mut fonts, &layers, &theme, backdrop, 1920, 1080);
            assert_eq!(edited.overflow, on_screen.overflow, "{backdrop:?}");
        }
    }

    fn slide(primary: &str, translation: Option<&str>) -> Slide {
        let mut boxes = vec![Object::text(Role::primary(), primary)];
        if let Some(t) = translation {
            boxes.push(Object::text(Role::translation(), t));
        }
        Slide::new(boxes)
    }

    fn overflows(slide: &Slide) -> bool {
        build_scene(&mut Fonts::new(), Some(slide), &Theme::builtin(), 1920, 1080).overflow
    }

    #[test]
    fn a_layouts_shapes_are_drawn_behind_the_words_or_in_front_as_its_template_keeps_them() {
        let mut theme = Theme::builtin();
        let (band, logo) = (Object::shape(sp_model::Shape::Rectangle), Object::shape(sp_model::Shape::Ellipse));
        theme.templates[0].objects = vec![band.clone(), logo.clone()];
        theme.templates[0].in_front = vec![logo.id];
        let song = slide("Words", Some("Слова"));
        let ids = |layer: Option<Slide>| layer.map(|s| s.objects.iter().map(|o| o.id).collect::<Vec<_>>());
        assert_eq!(ids(layout_layer(&song, &theme, false)), Some(vec![band.id]));
        assert_eq!(ids(layout_layer(&song, &theme, true)), Some(vec![logo.id]));
        // Every one of them, for what they show.
        assert_eq!(ids(layout_objects(&song, &theme)), Some(vec![band.id, logo.id]));
        // None kept in front: none drawn there.
        theme.templates[0].in_front.clear();
        assert_eq!(ids(layout_layer(&song, &theme, true)), None);
    }

    #[test]
    fn fit_rect_same_shape() {
        assert_eq!(fit_rect(1920, 1080), (0.0, 0.0, 1920.0, 1080.0));
    }

    #[test]
    fn fit_rect_4_3_letterboxes() {
        let (x, y, w, h) = fit_rect(1600, 1200);
        assert_eq!((x, w), (0.0, 1600.0));
        assert_eq!(h, 900.0);
        assert_eq!(y, 150.0);
    }

    #[test]
    fn fit_rect_wide_pillarboxes() {
        let (x, _, w, h) = fit_rect(3840, 1080);
        assert_eq!((w, h), (1920.0, 1080.0));
        assert_eq!(x, 960.0);
    }

    #[test]
    fn fit_rect_zero_target_does_not_panic() {
        assert_eq!(fit_rect(0, 0), (0.0, 0.0, 0.0, 0.0));
    }

    #[test]
    fn short_text_does_not_overflow() {
        assert!(!overflows(&slide("Light rises\nover the hills", Some("Свет встаёт\nнад холмами"))));
    }

    #[test]
    fn many_lines_shrink_and_fit() {
        assert!(!overflows(&slide(&["A line of words"; 7].join("\n"), None)));
    }

    #[test]
    fn huge_text_overflows_not_panics() {
        assert!(overflows(&slide(&["A line of words"; 40].join("\n"), Some("перевод"))));
        assert!(overflows(&slide(&"W".repeat(400), None)));
    }

    #[test]
    fn every_text_setting_draws() {
        use sp_model::{Caps, TextStyle};
        let mut object = Object::text(Role::primary(), "Light rises\nover the hills");
        object.fill = Some(Rgba { r: 0, g: 0, b: 0, a: 160 });
        let ObjectKind::Text { style, .. } = &mut object.kind else { panic!("a text object") };
        *style = TextStyle {
            valign: Some(VAlign::Bottom),
            caps: Some(Caps::AllCaps),
            underline: Some(true),
            strike: Some(true),
            ..TextStyle::default()
        };
        assert!(!overflows(&Slide::new(vec![object])));
    }

    #[test]
    fn scaling_decides_whether_text_shrinks_grows_or_spills() {
        use sp_model::TextStyle;
        let with = |text: &str, scaling: Scaling| {
            let mut object = Object::text(Role::primary(), text);
            let ObjectKind::Text { style, .. } = &mut object.kind else { panic!("a text object") };
            *style = TextStyle { scaling: Some(scaling), ..TextStyle::default() };
            Slide::new(vec![object])
        };
        let long = ["A line of words"; 9].join("\n");
        assert!(!overflows(&with(&long, Scaling::TextDown)));
        assert!(overflows(&with(&long, Scaling::None)));
        assert!(overflows(&with(&long, Scaling::TextUp)));
        assert!(!overflows(&with(&long, Scaling::TextUpOrDown)));
        assert!(!overflows(&with("Short", Scaling::TextUp)));
    }

    #[test]
    fn a_line_a_little_too_long_stays_on_one_row_and_a_paragraph_wraps() {
        let mut fonts = Fonts::new();
        let style = Theme::builtin().templates[0].boxes[0].clone();
        let rect = SlideRect { x: 0.0, y: 0.0, w: 1728.0, h: 700.0 };
        // At the box's size this line takes two rows; made a little smaller it fits on one.
        let line = "Filled with His goodness, lost in His love";
        let full = lay_out(&mut fonts, line, &[], &style, style.size, rect.w);
        assert!(full.len() > 1, "the test needs a line that wraps at full size");
        let (layout, _, size) =
            fit(&mut fonts, &format!("{line}\nThis is my story, this is my song"), &[], &style, rect);
        assert_eq!(layout.len(), 2, "each written line on one row");
        assert!(size < style.size && size >= style.size * KEEP_LINES_DOWN_TO);
        // A paragraph far too long for one row wraps at its size, as before.
        let paragraph = "For God so loved the world, that he gave his only begotten Son, that whosoever believeth in him should not perish";
        let (layout, _, size) = fit(&mut fonts, paragraph, &[], &style, rect);
        assert!(layout.len() > 1);
        assert_eq!(size, style.size);
    }

    #[test]
    fn text_fitted_again_comes_out_the_same_from_memory() {
        let mut fonts = Fonts::new();
        let style = Theme::builtin().templates[0].boxes[0].clone();
        let rect = SlideRect { x: 0.0, y: 0.0, w: 900.0, h: 300.0 };
        let text = "A long line of words that has to be made smaller to fit inside a small box on the slide";
        let (first, first_over, first_size) = fit(&mut fonts, text, &[], &style, rect);
        assert!(first_size < style.size, "the test needs text that is shrunk");
        assert_eq!(fonts.fitted_size(text, &[], &style, (rect.w, rect.h)), Some(first_size));
        let (again, again_over, again_size) = fit(&mut fonts, text, &[], &style, rect);
        assert_eq!((again_size, again_over, again.len()), (first_size, first_over, first.len()));
        // Another box size, or other words, is fitted afresh.
        assert_eq!(fonts.fitted_size(text, &[], &style, (rect.w, rect.h + 1.0)), None);
        assert_eq!(fonts.fitted_size("Other words", &[], &style, (rect.w, rect.h)), None);
    }

    #[test]
    fn a_hidden_object_is_not_drawn() {
        let mut slide = slide(&["A line of words"; 40].join("\n"), None);
        assert!(overflows(&slide));
        slide.objects[0].hidden = true;
        assert!(!overflows(&slide));
    }

    #[test]
    fn empty_slide_builds() {
        let mut fonts = Fonts::new();
        let theme = Theme::builtin();
        assert!(!build_scene(&mut fonts, None, &theme, 1920, 1080).overflow);
        assert!(!build_scene(&mut fonts, Some(&Slide::new(vec![])), &theme, 1920, 1080).overflow);
        assert!(!build_scene(&mut fonts, None, &theme, 0, 0).overflow);
    }

    #[test]
    fn a_role_the_theme_does_not_know_is_placed_like_the_main_language() {
        let s = Slide::new(vec![Object::text(Role("notes".to_owned()), "Hello")]);
        let theme = Theme::builtin();
        assert_eq!(object_frame(&s, &s.objects[0], &theme), theme.place_on(&s, Some(&Role::primary())));
    }

    /// A missing letter is drawn as an empty box, which the picture tests could mistake for
    /// text. Glyph 0 is the font's "no such letter" box.
    #[test]
    fn latin_russian_and_ukrainian_letters_all_have_glyphs() {
        let text =
            "ABCXYZ abcxyz 0123456789 АБВГДЕЁЖЗИЙКЛМНОПРСТУФХЦЧШЩЪЫЬЭЮЯ абвгдеёжзийклмнопрстуфхцчшщъыьэюя ҐґЄєІіЇї";
        let theme = Theme::builtin();
        let layout = lay_out(&mut Fonts::new(), text, &[], &theme.templates[0].boxes[0], 40.0, 100_000.0);
        let mut glyphs = 0;
        for line in layout.lines() {
            for item in line.items() {
                if let PositionedLayoutItem::GlyphRun(run) = item {
                    for glyph in run.glyphs() {
                        glyphs += 1;
                        assert_ne!(glyph.id, 0, "a letter has no glyph in the font that was chosen");
                    }
                }
            }
        }
        assert!(glyphs >= text.chars().filter(|c| !c.is_whitespace()).count());
    }

    #[test]
    fn an_objects_own_place_wins_over_the_themes() {
        let theme = Theme::builtin();
        let mut s = slide("A", Some("Б"));
        assert_eq!(object_frame(&s, &s.objects[0], &theme), theme.templates[0].boxes[0].rect);
        let moved = SlideRect { x: 10.0, y: 20.0, w: 300.0, h: 200.0 };
        s.objects[0].frame = Some(moved);
        assert_eq!(object_frame(&s, &s.objects[0], &theme), moved);
        assert_eq!(object_frame(&s, &s.objects[1], &theme), theme.templates[0].boxes[1].rect);
        // A lone main-language object sits in the theme's centred place.
        let alone = slide("A", None);
        assert_eq!(object_frame(&alone, &alone.objects[0], &theme), theme.templates[1].boxes[0].rect);
    }

    #[test]
    fn spacing_widens_letters_spreads_lines_and_margins_squeeze_the_text() {
        let mut fonts = Fonts::new();
        let theme = Theme::builtin();
        let base = theme.templates[0].boxes[0].clone();
        let text = "Light rises\nover the hills";
        let plain = lay_out(&mut fonts, text, &[], &base, 60.0, 10_000.0);
        let spaced = BoxStyle { letter_spacing: 10.0, ..base.clone() };
        assert!(lay_out(&mut fonts, text, &[], &spaced, 60.0, 10_000.0).width() > plain.width() + 50.0);
        let tall = BoxStyle { line_height: 2.0, ..base.clone() };
        assert!(lay_out(&mut fonts, text, &[], &tall, 60.0, 10_000.0).height() > plain.height() * 1.4);
        // Text that fits its box no longer fits once wide margins are kept clear inside it.
        let fixed = |margins| {
            let look = BoxStyle { margins, scaling: Scaling::None, ..base.clone() };
            let mut object = Object::text(Role::primary(), "Light rises over the hills");
            let ObjectKind::Text { style, .. } = &mut object.kind else { panic!("a text object") };
            *style = sp_model::TextStyle::all(&look);
            object.frame = Some(SlideRect { x: 100.0, y: 100.0, w: 1700.0, h: 300.0 });
            build_scene(&mut Fonts::new(), Some(&Slide::new(vec![object])), &theme, 1920, 1080).overflow
        };
        assert!(!fixed([0.0; 4]));
        assert!(fixed([600.0, 120.0, 600.0, 120.0]));
    }

    /// What each run of a layout is drawn with: size, font, how the font is set, slant, and
    /// whether it is underlined or has its own colour.
    type Drawn = (f32, u64, Vec<i16>, bool, bool, bool, Option<Rgba>);

    fn runs_of(layout: &Layout<Ink>) -> Vec<Drawn> {
        let mut runs = Vec::new();
        for line in layout.lines() {
            for item in line.items() {
                if let PositionedLayoutItem::GlyphRun(run) = item {
                    let synthesis = run.run().synthesis();
                    runs.push((
                        run.run().font_size(),
                        run.run().font().data.id() * 1000 + u64::from(run.run().font().index),
                        run.run().normalized_coords().iter().map(|c| c.to_bits()).collect(),
                        synthesis.embolden(),
                        synthesis.skew().is_some(),
                        run.style().underline.is_some(),
                        run.style().brush.0,
                    ));
                }
            }
        }
        runs
    }

    #[test]
    fn a_styled_word_is_drawn_differently_from_the_words_beside_it() {
        let mut fonts = Fonts::new();
        let base = BoxStyle { weight: 400, ..Theme::builtin().templates[0].boxes[0].clone() };
        let text = "plain word plain";
        let look = |own: sp_model::SpanStyle| vec![sp_model::TextSpan::new(6, 10, own)];
        let plain = lay_out(&mut fonts, text, &[], &base, base.size, 10_000.0);
        let red = Rgba::rgb(255, 0, 0);
        // Weight and slant need a face for them on this computer; a computer with one face of
        // the theme's font cannot show the difference, so those are tried only where it can.
        let faces = |own: sp_model::SpanStyle| {
            let layout = lay_out(&mut Fonts::new(), text, &look(own), &base, base.size, 10_000.0);
            let runs = runs_of(&layout);
            runs.len() == 3 && runs[1] != runs[0]
        };
        let bold = sp_model::SpanStyle { weight: Some(700), ..Default::default() };
        let slanted = sp_model::SpanStyle { italic: Some(true), ..Default::default() };
        let mut tries = vec![
            ("size", sp_model::SpanStyle { size: Some(base.size * 2.0), ..Default::default() }),
            ("underline", sp_model::SpanStyle { underline: Some(true), ..Default::default() }),
            ("colour", sp_model::SpanStyle { fill: Some(red), ..Default::default() }),
        ];
        for (name, own) in [("bold", bold), ("italic", slanted)] {
            if faces(own.clone()) {
                tries.push((name, own));
            } else {
                // A Mac has them for the theme's font; elsewhere the font may have one face.
                #[cfg(target_os = "macos")]
                panic!("{name} draws like the words beside it");
            }
        }
        for (name, own) in tries {
            let layout = lay_out(&mut fonts, text, &look(own), &base, base.size, 10_000.0);
            let runs = runs_of(&layout);
            assert_eq!(runs.len(), 3, "{name}: before, the word, after: {runs:?}");
            assert_ne!(runs[1], runs[0], "{name}: the word looks like what is before it");
            assert_ne!(runs[1], runs[2], "{name}: the word looks like what is after it");
            assert_eq!(runs[0], runs[2], "{name}: the neighbours differ");
            // The words beside it are drawn as they are with no spans at all.
            assert_eq!(runs[0], runs_of(&plain)[0], "{name}");
        }
        // Bigger letters make the line wider.
        let big = sp_model::SpanStyle { size: Some(base.size * 2.0), ..Default::default() };
        assert!(lay_out(&mut fonts, text, &look(big), &base, base.size, 10_000.0).width() > plain.width() * 1.2);
    }

    #[test]
    fn a_spans_size_shrinks_with_the_box_and_the_rest_follows_the_theme() {
        let mut fonts = Fonts::new();
        let base = Theme::builtin().templates[0].boxes[0].clone();
        let text = "plain word plain";
        let own = sp_model::SpanStyle { size: Some(base.size * 1.5), ..Default::default() };
        let spans = vec![sp_model::TextSpan::new(6, 10, own)];
        // Drawn at half size (as when the text is shrunk to fit), both halve.
        let layout = lay_out(&mut fonts, text, &spans, &base, base.size / 2.0, 10_000.0);
        let sizes: Vec<f32> = runs_of(&layout).iter().map(|run| run.0).collect();
        assert_eq!(sizes, vec![base.size / 2.0, base.size * 0.75, base.size / 2.0]);
        // The theme makes its text bigger: the plain words follow; the word that was given a
        // size by hand keeps it.
        let bigger = BoxStyle { size: base.size * 2.0, ..base.clone() };
        let layout = lay_out(&mut fonts, text, &spans, &bigger, bigger.size, 10_000.0);
        let sizes: Vec<f32> = runs_of(&layout).iter().map(|run| run.0).collect();
        assert_eq!(sizes, vec![base.size * 2.0, base.size * 1.5, base.size * 2.0]);
    }

    #[test]
    fn a_slide_with_styled_words_builds_fits_and_fits_again_the_same() {
        let theme = Theme::builtin();
        let mut object = Object::text(Role::primary(), ["A line of words in a verse"; 9].join("\n"));
        if let ObjectKind::Text { spans, .. } = &mut object.kind {
            let red =
                sp_model::SpanStyle { fill: Some(Rgba::rgb(255, 0, 0)), underline: Some(true), ..Default::default() };
            *spans = vec![
                sp_model::TextSpan::new(0, 6, sp_model::SpanStyle { weight: Some(800), ..Default::default() }),
                sp_model::TextSpan::new(10, 20, red),
            ];
        }
        let slide = Slide::new(vec![object]);
        let mut fonts = Fonts::new();
        let first = build_scene(&mut fonts, Some(&slide), &theme, 1920, 1080);
        let again = build_scene(&mut fonts, Some(&slide), &theme, 1920, 1080);
        assert_eq!(first.overflow, again.overflow);
        assert!(!first.overflow, "nine lines shrink to fit");
        // Different spans are not served the size another set was fitted to.
        let mut other = slide.clone();
        if let ObjectKind::Text { spans, .. } = &mut other.objects[0].kind {
            spans[0].style.size = Some(400.0);
        }
        assert!(build_scene(&mut fonts, Some(&other), &theme, 1920, 1080).overflow, "huge words do not fit nine lines");
    }

    #[test]
    fn spans_that_cut_through_clusters_or_run_past_the_words_do_not_panic() {
        // A letter with a combining mark, a family emoji joined by zero-width joiners, and a
        // Cyrillic word; spans start and stop inside them, and past the end.
        let text = "e\u{301}x \u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467} слово";
        let length = text.chars().count();
        let base = Theme::builtin().templates[0].boxes[0].clone();
        let style = sp_model::SpanStyle { size: Some(40.0), weight: Some(700), ..Default::default() };
        let mut fonts = Fonts::new();
        for start in 0..=length + 2 {
            for end in start..=length + 4 {
                let spans = vec![sp_model::TextSpan::new(start, end, style.clone())];
                lay_out(&mut fonts, text, &spans, &base, base.size, 800.0);
            }
        }
        // Spans that were never normalized (out of order, overlapping) still lay out.
        let loose = vec![
            sp_model::TextSpan::new(5, 9, style.clone()),
            sp_model::TextSpan::new(1, 7, style.clone()),
            sp_model::TextSpan::new(3, 3, style),
        ];
        lay_out(&mut fonts, text, &loose, &base, base.size, 800.0);
    }

    #[test]
    fn a_box_set_to_fit_its_text_grows_about_its_middle() {
        let mut fonts = Fonts::new();
        let look = BoxStyle { scaling: Scaling::ContainerUpOrDown, ..Theme::builtin().templates[0].boxes[0].clone() };
        let place = SlideRect { x: 100.0, y: 400.0, w: 1600.0, h: 100.0 };
        let grown = fitted_container(&mut fonts, "One\nTwo\nThree", &[], &look, place);
        assert!(grown.h > 300.0, "{grown:?}");
        assert!((grown.y + grown.h / 2.0 - 450.0).abs() < 0.5, "about its middle");
        assert_eq!((grown.x, grown.w), (place.x, place.w));
    }

    /// A song's last slide as an audience screen draws it with the end of a song marked, the
    /// mark free to reach `floor`; what drawing it came to.
    fn drawn_with_mark(slide: &Slide, theme: &Theme, floor: Option<f32>) -> Painted {
        let slide = Slide { end_mark: floor, ..slide.clone() };
        draw_objects(&mut Scene::new(), &mut Fonts::new(), Affine::IDENTITY, &slide, theme, 0.0)
    }

    const FLOOR: f32 = SLIDE_HEIGHT * 0.97;

    #[test]
    fn the_mark_sits_half_a_line_under_the_lowest_words_at_80_percent() {
        let theme = Theme::builtin();
        // Lines enough that the words are fitted smaller than the box's own size.
        let words = "One\nTwo\nThree\nFour\nFive\nSix\nSeven\nEight";
        let drawn = drawn_with_mark(&slide(words, None), &theme, Some(FLOOR));
        let [(_, laid, _)] = drawn.words[..] else { panic!("one box of words: {:?}", drawn.words) };
        let nominal = theme.templates[0].boxes[0].size;
        assert!(laid.size < nominal, "fitted to {} from {nominal}", laid.size);
        let mark = drawn.end_mark.expect("room under the words");
        assert!((mark.size - 0.8 * laid.size).abs() < 0.01, "80% of the fitted size: {mark:?} under {laid:?}");
        assert!((mark.top - (laid.bottom + 0.5 * laid.size)).abs() < 0.01, "half a line under: {mark:?}, {laid:?}");
        let place = theme.templates[0].place_for(Some(&Role::primary()));
        assert!((mark.centre - (place.x + place.w / 2.0)).abs() < 0.01, "centred on the box: {mark:?} in {place:?}");
        assert_eq!(mark.centre, laid.centre);

        // Two languages, under the last words only: the look of the largest words, under the
        // lower block.
        let last = Theme { end_look: last_words_only(), ..theme.clone() };
        let drawn = drawn_with_mark(&slide("One\nTwo", Some("Uno\nDos")), &last, Some(FLOOR));
        let [(_, first, _), (_, second, _)] = drawn.words[..] else { panic!("two boxes: {:?}", drawn.words) };
        let (largest, lowest) = (first.size.max(second.size), first.bottom.max(second.bottom));
        assert!(first.size != second.size && first.bottom != second.bottom, "{first:?} {second:?}");
        let mark = drawn.end_mark.expect("room under the words");
        assert!((mark.size - 0.8 * largest).abs() < 0.01, "{mark:?}");
        assert!((mark.top - (lowest + 0.5 * largest)).abs() < 0.01, "{mark:?}");

        // A box of its own place: the mark is centred on that box, not on the slide.
        let mut aside = slide("One\nTwo", None);
        aside.objects[0].frame = Some(SlideRect { x: 100.0, y: 100.0, w: 800.0, h: 500.0 });
        let mark = drawn_with_mark(&aside, &theme, Some(FLOOR)).end_mark.expect("room");
        assert!((mark.centre - 500.0).abs() < 0.01, "{mark:?}");
    }

    #[test]
    fn the_mark_goes_under_the_songs_words_only_and_is_as_solid_as_they_are() {
        // One mark, under the last words only, as the mark always was.
        let theme = Theme { end_look: last_words_only(), ..Theme::builtin() };
        let song = slide("One\nTwo", Some("Uno\nDos"));
        let plain = drawn_with_mark(&song, &theme, Some(FLOOR)).end_mark.expect("room");
        assert_eq!(plain.opacity, 1.0);
        // A caption in a box of its own, lower than the words and larger than them: the mark
        // stays under the song's words, in their size.
        let mut footed = song.clone();
        let mut caption = Object::text(Role::primary(), "12:45");
        caption.role = None;
        caption.frame = Some(SlideRect { x: 60.0, y: 930.0, w: 600.0, h: 100.0 });
        if let Some(style) = caption.text_style_mut() {
            style.size = Some(200.0);
        }
        footed.objects.push(caption);
        let drawn = drawn_with_mark(&footed, &theme, Some(FLOOR));
        assert_eq!(drawn.words.len(), 2, "the song's two boxes: {:?}", drawn.words);
        assert_eq!(drawn.end_mark, Some(plain));
        // A slide of text boxes that are no song's words (a deck's) gets none.
        let mut deck = slide("Point one\nPoint two", None);
        deck.objects[0].role = None;
        deck.objects[0].frame = Some(SlideRect { x: 100.0, y: 100.0, w: 1700.0, h: 500.0 });
        let drawn = drawn_with_mark(&deck, &theme, Some(FLOOR));
        assert!(drawn.words.is_empty() && drawn.end_mark.is_none(), "{:?}", drawn.words);
        // Words drawn part solid: the mark no more solid than they are, where it was.
        let mut faint = song.clone();
        faint.objects[1].opacity = 0.4;
        let mark = drawn_with_mark(&faint, &theme, Some(FLOOR)).end_mark.expect("room");
        assert_eq!(mark, EndMark { opacity: 0.4, ..plain });
        faint.objects[0].opacity = 0.25;
        assert_eq!(drawn_with_mark(&faint, &theme, Some(FLOOR)).end_mark.map(|mark| mark.opacity), Some(0.25));
        // Under each language's words: each mark as solid as the words it is under.
        let each = drawn_with_mark(&faint, &Theme::builtin(), Some(FLOOR));
        let solid: Vec<(usize, f32)> = each.end_marks.iter().map(|(at, mark)| (*at, mark.opacity)).collect();
        assert_eq!(solid, [(1, 0.4), (0, 0.25)], "the translation's, lowest, first");
        // And a caption is no language: still the song's two marks.
        let each = drawn_with_mark(&footed, &Theme::builtin(), Some(FLOOR));
        assert_eq!(each.end_marks.iter().map(|(at, _)| *at).collect::<Vec<_>>(), [1, 0]);
    }

    #[test]
    fn the_words_keep_their_size_and_place_with_the_mark_on() {
        let theme = Theme::builtin();
        for words in ["One\nTwo", "One\nTwo\nThree\nFour\nFive\nSix\nSeven\nEight"] {
            for translation in [None, Some("Uno\nDos")] {
                let slide = slide(words, translation);
                let off = drawn_with_mark(&slide, &theme, None);
                let on = drawn_with_mark(&slide, &theme, Some(FLOOR));
                assert!(off.end_mark.is_none() && on.end_mark.is_some(), "{words:?}");
                assert!(!off.words.is_empty());
                assert_eq!(on.words, off.words, "{words:?}: the words' size and place");
                assert_eq!(on.overflow, off.overflow);
            }
        }
    }

    /// The built-in theme with its song words' box over most of the slide (60 to 1000 down it),
    /// as many churches set theirs, and the words fitted as `scaling` says.
    fn tall(scaling: Scaling) -> Theme {
        let mut theme = Theme::builtin();
        for template in &mut theme.templates {
            for look in &mut template.boxes {
                if look.role == Role::primary() {
                    look.rect = SlideRect { x: 80.0, y: 60.0, w: 1760.0, h: 940.0 };
                    look.scaling = scaling;
                }
            }
        }
        theme
    }

    /// A last slide of `n` written lines.
    fn lines(n: usize) -> String {
        (1..=n).map(|at| format!("Line {at} of the last verse sung")).collect::<Vec<_>>().join("\n")
    }

    /// The copyright line's top, the floor the app gives while that line shows.
    const OVER_THE_COPYRIGHT_LINE: f32 = 1020.0;

    #[test]
    fn the_mark_moves_down_as_far_as_the_floor_before_anything_is_made_smaller() {
        let theme = Theme::builtin();
        let short = slide("One\nTwo", None);
        let off = drawn_with_mark(&short, &theme, None);
        let (_, laid, _) = off.words[0];
        let size = 0.8 * laid.size;
        // Room for half a line's gap, to a hair: where it always was.
        let roomy = laid.bottom + 0.5 * laid.size + size;
        let mark = drawn_with_mark(&short, &theme, Some(roomy + 0.01)).end_mark.expect("room");
        assert!((mark.top - (laid.bottom + 0.5 * laid.size)).abs() < 0.01 && mark.size == size, "{mark:?}");
        // Less than that, down to a quarter line's gap: the mark moves up against the floor at
        // its own size, and the words stay as they are.
        for gap in [0.49, 0.4, 0.26] {
            let floor = laid.bottom + gap * laid.size + size;
            let on = drawn_with_mark(&short, &theme, Some(floor));
            let mark = on.end_mark.unwrap_or_else(|| panic!("a gap of {gap} of a line"));
            assert_eq!(on.words, off.words, "{gap}: the words are not touched");
            assert!(mark.size == size && (mark.top + mark.size - floor).abs() < 0.01, "{gap}: {mark:?} over {floor}");
        }
        // It may lie below the words' box, which is no limit to it: only the floor is.
        let mut boxed = slide("One\nTwo", None);
        boxed.objects[0].frame = Some(SlideRect { x: 60.0, y: 600.0, w: 1800.0, h: 260.0 });
        let on = drawn_with_mark(&boxed, &theme, Some(FLOOR));
        let (mark, (_, laid, _)) = (on.end_mark.expect("room under the box"), on.words[0]);
        assert!(mark.top + mark.size > 860.0, "{mark:?} ends under the box, which ends at 860");
        assert!((mark.top - (laid.bottom + 0.5 * laid.size)).abs() < 0.01, "{mark:?} under {laid:?}");
    }

    #[test]
    fn a_full_last_slide_makes_its_words_a_little_smaller_for_the_mark() {
        let usual = sp_model::EndLook::default();
        let theme = tall(Scaling::TextDown);
        for n in [7, 8, 10] {
            let song = slide(&lines(n), None);
            let off = drawn_with_mark(&song, &theme, None);
            let (_, before, _) = off.words[0];
            for floor in [FLOOR, OVER_THE_COPYRIGHT_LINE] {
                for place in [sp_model::EndPlace::Under, sp_model::EndPlace::Bottom] {
                    for mark in sp_model::EndMark::ALL {
                        for size in [40, 80, 160] {
                            let look = sp_model::EndLook { mark, size, place, ..usual };
                            let said = format!("{n} lines, floor {floor}, {look:?}");
                            let themed = Theme { end_look: look, ..theme.clone() };
                            let on = drawn_with_mark(&song, &themed, Some(floor));
                            let [(_, laid, _)] = on.words[..] else { panic!("{said}: one box of words") };
                            let drawn = on.end_mark.unwrap_or_else(|| panic!("{said}: no mark; words {laid:?}"));
                            // At the theme's own size beside the words as they came out.
                            let wanted = laid.size * f32::from(size) / 100.0;
                            assert!((drawn.size - wanted).abs() < 0.01, "{said}: {drawn:?} of words at {}", laid.size);
                            // On the screen, clear of the floor, and never over the words: a
                            // quarter of a line under them at the least.
                            assert!(drawn.top + drawn.size <= floor + 0.01, "{said}: {drawn:?} passes the floor");
                            assert!(drawn.top >= laid.bottom + 0.25 * laid.size - 0.01, "{said}: {drawn:?}, {laid:?}");
                            // The words are no bigger than without it, never under their
                            // least size, and still start no higher than their box.
                            let least = themed.templates[0].boxes[0].min_size;
                            assert!(laid.size <= before.size + 0.01 && laid.size >= least - 0.01, "{said}: {laid:?}");
                            assert!(laid.top >= 60.0 - 0.01 && !on.overflow, "{said}: {laid:?}");
                            // No smaller, and no higher, than they need be: the room left over
                            // is no more than one step of their size frees (4% of their block).
                            if laid != before {
                                let spare = floor - (laid.bottom + 0.25 * laid.size + drawn.size);
                                let step = 0.04 * (laid.bottom - laid.top) + 2.0;
                                assert!(
                                    (-0.01..step).contains(&spare),
                                    "{said}: {spare} to spare, a step frees {step}"
                                );
                            }
                            if place == sp_model::EndPlace::Bottom {
                                assert!((drawn.top + drawn.size - floor).abs() < 0.01, "{said}: {drawn:?}");
                            }
                        }
                    }
                }
            }
            // The usual mark on an 8-line slide: the words do get smaller, and not by much.
            if n == 8 {
                let on = drawn_with_mark(&song, &theme, Some(FLOOR));
                let (_, laid, _) = on.words[0];
                assert!(
                    laid.size < before.size && laid.size > 0.75 * before.size,
                    "{} from {}",
                    laid.size,
                    before.size
                );
            }
        }
        // A slide with room as it is keeps its words exactly, in the same theme.
        let roomy = slide(&lines(4), None);
        assert_eq!(drawn_with_mark(&roomy, &theme, Some(FLOOR)).words, drawn_with_mark(&roomy, &theme, None).words);
    }

    #[test]
    fn at_the_bottom_of_the_screen_the_mark_shows_wherever_it_clears_the_words() {
        // Seven lines in a tall box whose words keep their size: no room for the usual gap
        // under them, but the mark at the bottom is clear of them. It was left out.
        let theme = tall(Scaling::None);
        let low =
            Theme { end_look: sp_model::EndLook { place: sp_model::EndPlace::Bottom, ..Default::default() }, ..theme };
        let song = slide(&lines(4), None);
        let off = drawn_with_mark(&song, &low, None);
        let (_, laid, _) = off.words[0];
        let size = 0.8 * laid.size;
        let floor = laid.bottom + 0.3 * laid.size + size;
        let on = drawn_with_mark(&song, &low, Some(floor));
        let mark = on.end_mark.expect("the bottom clears the words");
        assert_eq!(on.words, off.words);
        assert!(mark.size == size && (mark.top + mark.size - floor).abs() < 0.01, "{mark:?}");
        assert!(mark.top >= laid.bottom + 0.25 * laid.size, "{mark:?} over {laid:?}");
    }

    /// The song's slide with its words' box drawn close round the words as `theme` lays them
    /// out: a box they fill, with no room to spare in it.
    fn hugged(song: &Slide, theme: &Theme) -> Slide {
        let (_, laid, _) = drawn_with_mark(song, theme, None).words[0];
        let mut hugged = song.clone();
        hugged.objects[0].frame =
            Some(SlideRect { x: 80.0, y: laid.top - 0.25, w: 1760.0, h: laid.bottom - laid.top + 0.5 });
        hugged
    }

    #[test]
    fn words_with_room_to_spare_in_their_box_move_up_in_it_at_their_size() {
        // Four lines, which keep their size, in the middle of a tall box: with the floor close
        // under them the box is taken shorter, and they sit higher in it, no smaller.
        for scaling in [Scaling::None, Scaling::TextDown] {
            let theme = tall(scaling);
            let song = slide(&lines(4), None);
            let off = drawn_with_mark(&song, &theme, None);
            let (_, laid, _) = off.words[0];
            let floor = laid.bottom + 0.25 * laid.size + 0.8 * laid.size - 40.0;
            let on = drawn_with_mark(&song, &theme, Some(floor));
            let (mark, (_, moved, _)) = (on.end_mark.unwrap_or_else(|| panic!("{scaling:?}")), on.words[0]);
            assert_eq!(moved.size, laid.size, "{scaling:?}: no smaller");
            assert!(moved.top < laid.top && moved.top >= 60.0, "{scaling:?}: higher, inside the box: {moved:?}");
            assert!((mark.size - 0.8 * laid.size).abs() < 0.01, "{scaling:?}: {mark:?}");
            assert!(mark.top + mark.size <= floor + 0.01 && mark.top >= moved.bottom + 0.25 * moved.size - 0.01);
            assert!(!on.overflow && take_end_mark_left_out().is_none());
        }
    }

    #[test]
    fn words_that_keep_their_size_get_a_smaller_mark_and_below_its_least_none() {
        // Words that fill their box and can't be made smaller: a theme that keeps their size,
        // one whose box grows round them, and words already at their least size.
        let mut at_least = tall(Scaling::TextDown);
        for template in &mut at_least.templates {
            for look in &mut template.boxes {
                look.min_size = look.size;
            }
        }
        for (said, theme) in [
            ("their size kept", tall(Scaling::None)),
            ("the box grows round them", tall(Scaling::ContainerUpOrDown)),
            ("at their least size", at_least),
        ] {
            let song = hugged(&slide(&lines(4), None), &theme);
            let off = drawn_with_mark(&song, &theme, None);
            let (_, laid, _) = off.words[0];
            let quarter = 0.25 * laid.size;
            // Room for the mark at 60% of the words' size, not at its own 80%: drawn at 60%,
            // against the floor, a quarter line under the words, which stay as they are.
            let floor = laid.bottom + quarter + 0.6 * laid.size;
            let on = drawn_with_mark(&song, &theme, Some(floor));
            let mark = on.end_mark.unwrap_or_else(|| panic!("{said}: a smaller mark"));
            assert_eq!(on.words, off.words, "{said}: the words keep their size and place");
            assert!((mark.size - 0.6 * laid.size).abs() < 0.01, "{said}: {mark:?}");
            assert!((mark.top - (laid.bottom + quarter)).abs() < 0.01, "{said}: {mark:?}");
            assert!(take_end_mark_left_out().is_none());
            // Down to 40% of the words' size, to a hair; less room than that, and it is left
            // out, and that is noted for the log.
            let least = laid.bottom + quarter + 0.4 * laid.size;
            let on = drawn_with_mark(&song, &theme, Some(least + 0.01));
            assert!(on.end_mark.is_some_and(|mark| (mark.size - 0.4 * laid.size).abs() < 0.02), "{said}");
            let on = drawn_with_mark(&song, &theme, Some(least - 0.01));
            assert_eq!((on.end_mark, &on.words), (None, &off.words), "{said}");
            assert_eq!(take_end_mark_left_out(), Some((song.id, MarkLeftOut::NoRoom)), "{said}: noted, of this slide");
            assert_eq!(take_end_mark_left_out(), None, "{said}: noted once");
            // Drawn again with room, before anyone asked: it has nothing to say any more.
            drawn_with_mark(&song, &theme, Some(least - 0.01));
            drawn_with_mark(&song, &theme, Some(FLOOR + 400.0));
            assert_eq!(take_end_mark_left_out(), None, "{said}: it fits after all");
            // A theme that asks for a mark of 40% keeps it at that where there is the room.
            let small = Theme { end_look: sp_model::EndLook { size: 40, ..Default::default() }, ..theme.clone() };
            let on = drawn_with_mark(&song, &small, Some(least + 0.01));
            assert!(on.end_mark.is_some_and(|mark| (mark.size - 0.4 * laid.size).abs() < 0.01), "{said}");
        }
    }

    #[test]
    fn a_small_cut_is_found_where_a_large_one_would_spill() {
        // Eight lines that fill a tall box, in a theme whose words may get only a little
        // smaller: one step of 4%, and not two. That step gives the mark its room; a box cut
        // by a quarter would need the words far smaller than they may be, and spill.
        let song = slide(&lines(8), None);
        let (_, before, _) = drawn_with_mark(&song, &tall(Scaling::TextDown), None).words[0];
        let mut theme = tall(Scaling::TextDown);
        for template in &mut theme.templates {
            for look in &mut template.boxes {
                look.min_size = before.size * 0.95;
            }
        }
        let on = drawn_with_mark(&song, &theme, Some(FLOOR));
        let (mark, (_, laid, _)) = (on.end_mark.expect("the mark"), on.words[0]);
        assert!((laid.size - 0.96 * before.size).abs() < 0.01, "one step smaller: {} from {}", laid.size, before.size);
        assert!((mark.size - 0.8 * laid.size).abs() < 0.01, "at its own size, not a smaller one: {mark:?}");
        assert!(!on.overflow && mark.top + mark.size <= FLOOR + 0.01);
        // Words with room to spare move up by just what is wanting, to a hair: no step of
        // sixteenths is taken where less does.
        let theme = tall(Scaling::None);
        let short = slide(&lines(4), None);
        let (_, before, _) = drawn_with_mark(&short, &theme, None).words[0];
        for wanting in [3.0, 17.0, 41.0, 90.0] {
            let floor = before.bottom + 0.25 * before.size + 0.8 * before.size - wanting;
            let on = drawn_with_mark(&short, &theme, Some(floor));
            let (mark, (_, laid, _)) = (on.end_mark.expect("the mark"), on.words[0]);
            let moved = before.bottom - laid.bottom;
            assert!(
                laid.size == before.size && moved >= wanting - 0.01 && moved < wanting + 1.0,
                "{wanting}: moved {moved}"
            );
            assert!((mark.size - 0.8 * laid.size).abs() < 0.01 && mark.top + mark.size <= floor + 0.01, "{mark:?}");
        }
    }

    #[test]
    fn how_the_words_make_room_is_worked_out_once_and_never_stale() {
        let theme = tall(Scaling::TextDown);
        let song = Slide { end_mark: Some(FLOOR), ..slide(&lines(8), None) };
        let mut fonts = Fonts::new();
        let draw = |fonts: &mut Fonts, slide: &Slide, theme: &Theme| {
            let painted = draw_objects(&mut Scene::new(), fonts, Affine::IDENTITY, slide, theme, 0.0);
            (painted.words, painted.end_mark)
        };
        // Drawn again, it comes to the same, with nothing fitted anew (no trial is left
        // switched on, and the answer is the remembered one).
        let first = draw(&mut fonts, &song, &theme);
        let rooms = fonts.mark_rooms_kept();
        assert_eq!(rooms, 1);
        for _ in 0..3 {
            assert_eq!(draw(&mut fonts, &song, &theme), first);
        }
        assert!(!fonts.trying && fonts.mark_rooms_kept() == rooms, "looked up, not worked out again");
        // Another floor, another size of mark, other words, another look: each its own answer,
        // the same as fonts that never drew anything give.
        let fresh = |slide: &Slide, theme: &Theme| draw(&mut Fonts::new(), slide, theme);
        let lower = Slide { end_mark: Some(OVER_THE_COPYRIGHT_LINE), ..song.clone() };
        let big = Theme { end_look: sp_model::EndLook { size: 160, ..Default::default() }, ..theme.clone() };
        let other = Slide { end_mark: Some(FLOOR), ..slide(&lines(10), None) };
        let fixed = tall(Scaling::None);
        for (slide, theme) in [(&lower, &theme), (&song, &big), (&other, &theme), (&song, &fixed), (&song, &theme)] {
            assert_eq!(draw(&mut fonts, slide, theme), fresh(slide, theme));
        }
        assert_ne!(draw(&mut fonts, &lower, &theme), first, "a lower floor is another answer");
        // Many themes and slides: the oldest answers are let go, and what is drawn is right.
        for n in 5..40 {
            let slide = Slide { end_mark: Some(FLOOR), ..slide(&lines(8 + n % 5), None) };
            let mut theme = tall(Scaling::TextDown);
            for template in &mut theme.templates {
                for look in &mut template.boxes {
                    look.rect.y += n as f32;
                }
            }
            assert_eq!(draw(&mut fonts, &slide, &theme), fresh(&slide, &theme), "theme {n}");
        }
        assert!(fonts.mark_rooms_kept() <= 16);
        assert_eq!(draw(&mut fonts, &song, &theme), first, "worked out again after it was let go");
    }

    /// How long drawing a steady last slide takes with the mark and without, the same slide
    /// drawn again and again as behind a playing video. Run by hand, with the optimiser:
    /// `cargo test --release -p sp-render --lib the_marks_cost -- --ignored --nocapture`.
    #[test]
    #[ignore = "a measurement, not a check: it prints times"]
    fn the_marks_cost_on_a_steady_slide() {
        let two = "Light rises over the quiet hills\nAnd the morning sings again\nEvery field is bright with rain\nEvery road is new";
        let cases = [
            ("8 lines, tall box", slide(&lines(8), None), tall(Scaling::TextDown)),
            ("two languages, built-in", slide(two, Some(two)), Theme::builtin()),
            ("8 lines, built-in (room as it is)", slide(&lines(8), None), Theme::builtin()),
        ];
        for (name, song, theme) in cases {
            let mut fonts = Fonts::new();
            let mut time = |floor: Option<f32>| {
                let slide = Slide { end_mark: floor, ..song.clone() };
                for _ in 0..50 {
                    draw_objects(&mut Scene::new(), &mut fonts, Affine::IDENTITY, &slide, &theme, 0.0);
                }
                let started = std::time::Instant::now();
                for _ in 0..2000 {
                    draw_objects(&mut Scene::new(), &mut fonts, Affine::IDENTITY, &slide, &theme, 0.0);
                }
                started.elapsed().as_secs_f64() * 1000.0 / 2000.0
            };
            let (without, with) = (time(None), time(Some(OVER_THE_COPYRIGHT_LINE)));
            println!("{name}: {without:.3} ms without the mark, {with:.3} ms with it");
        }
    }

    #[test]
    fn the_built_in_theme_draws_every_last_slide_as_it_did() {
        let theme = Theme::builtin();
        for n in [2, 4, 6, 7, 8, 9, 10, 12] {
            for floor in [FLOOR, OVER_THE_COPYRIGHT_LINE] {
                let song = slide(&lines(n), None);
                let (off, on) = (drawn_with_mark(&song, &theme, None), drawn_with_mark(&song, &theme, Some(floor)));
                assert_eq!(on.words, off.words, "{n} lines: the words' size and place");
                let (mark, (_, laid, _)) = (on.end_mark.unwrap_or_else(|| panic!("{n} lines")), on.words[0]);
                assert!((mark.size - 0.8 * laid.size).abs() < 0.01, "{n} lines: {mark:?}");
                assert!((mark.top - (laid.bottom + 0.5 * laid.size)).abs() < 0.01, "{n} lines: {mark:?}");
            }
        }
    }

    /// The built-in theme with a look of its own for the end of a song's mark.
    fn themed(look: sp_model::EndLook) -> Theme {
        Theme { end_look: look, ..Theme::builtin() }
    }

    #[test]
    fn the_end_look_as_it_comes_draws_the_mark_as_it_always_was() {
        // A theme that never chose a look: three stars at 80%, half a line under the words, in
        // the words' own colour.
        let (theme, song) = (Theme::builtin(), slide("One\nTwo", None));
        assert!(theme.end_look.is_usual());
        let drawn = drawn_with_mark(&song, &theme, Some(FLOOR));
        let [(_, laid, _)] = drawn.words[..] else { panic!("one box of words") };
        let mark = drawn.end_mark.expect("room");
        assert!(
            (mark.size - 0.8 * laid.size).abs() < 0.01 && (mark.top - (laid.bottom + 0.5 * laid.size)).abs() < 0.01
        );
        assert_eq!(theme.end_look.mark.text(), "*\u{a0}\u{a0}\u{a0}\u{a0}*\u{a0}\u{a0}\u{a0}\u{a0}*");
        let words = theme.templates[0].boxes[0].clone();
        assert_eq!(end_mark_style(&words, mark.size, theme.end_look.colour).fill, words.fill);
    }

    #[test]
    fn end_look_each_mark_draws_at_the_themes_size_in_its_colour() {
        let song = slide("One\nTwo", None);
        let usual = sp_model::EndLook::default();
        let plain = build_scene(&mut Fonts::new(), Some(&song), &Theme::builtin(), 1920, 1080);
        let runs = |built: &Built| built.scene.encoding().resources.patches.len();
        for mark in sp_model::EndMark::ALL {
            let theme = themed(sp_model::EndLook { mark, ..usual });
            assert!(drawn_with_mark(&song, &theme, Some(FLOOR)).end_mark.is_some(), "{mark:?}");
            // Its three marks are in the scene: more is drawn than without it.
            let marked = Slide { end_mark: Some(FLOOR), ..song.clone() };
            let built = build_scene(&mut Fonts::new(), Some(&marked), &theme, 1920, 1080);
            assert!(runs(&built) > runs(&plain), "{mark:?}: {} and {}", runs(&built), runs(&plain));
        }
        // The size is the theme's share of the largest words' fitted size, from 40% to 160%.
        for size in [40, 80, 120, 160] {
            let drawn = drawn_with_mark(&song, &themed(sp_model::EndLook { size, ..usual }), Some(FLOOR));
            let [(_, laid, _)] = drawn.words[..] else { panic!("one box of words") };
            let mark = drawn.end_mark.unwrap_or_else(|| panic!("room at {size}%"));
            assert!((mark.size - laid.size * f32::from(size) / 100.0).abs() < 0.01, "{size}%: {mark:?}");
            // Under the words it starts where it always did, whatever its size.
            assert!((mark.top - (laid.bottom + 0.5 * laid.size)).abs() < 0.01, "{size}%: {mark:?}");
        }
        // Its colour is the words' unless the theme chose one; their face, outline and shadow
        // are the mark's either way.
        let words = BoxStyle { outline_width: 4.0, shadow: true, ..Theme::builtin().templates[0].boxes[0].clone() };
        let yellow = Rgba::rgb(0xff, 0xc4, 0x00);
        let (own, chosen) = (end_mark_style(&words, 50.0, None), end_mark_style(&words, 50.0, Some(yellow)));
        assert_eq!((own.fill, chosen.fill), (words.fill, yellow));
        for style in [&own, &chosen] {
            assert_eq!(
                (&style.font_family, style.outline_width, style.shadow, &style.shadow_look, style.size),
                (&words.font_family, 4.0, true, &words.shadow_look, 50.0)
            );
        }
    }

    #[test]
    fn end_look_at_the_bottom_the_mark_is_as_low_as_it_may_go_and_never_over_the_words() {
        let song = slide("One\nTwo", None);
        let usual = sp_model::EndLook::default();
        let low = themed(sp_model::EndLook { place: sp_model::EndPlace::Bottom, ..usual });
        // 3% above the slide's bottom: its one line ends there.
        let mark = drawn_with_mark(&song, &low, Some(FLOOR)).end_mark.expect("room");
        assert!((mark.top + mark.size - FLOOR).abs() < 0.01, "{mark:?}");
        // The copyright line showing: just over that, the floor the app gives then.
        let over_line = FLOOR - 120.0;
        let mark = drawn_with_mark(&song, &low, Some(over_line)).end_mark.expect("room");
        assert!((mark.top + mark.size - over_line).abs() < 0.01, "{mark:?}");
        // Centred on the words' box, as under them; the words are laid out the same.
        let under = drawn_with_mark(&song, &Theme::builtin(), Some(FLOOR));
        let bottom = drawn_with_mark(&song, &low, Some(FLOOR));
        assert_eq!(under.words, bottom.words, "the words keep their place");
        assert_eq!(under.end_mark.map(|mark| mark.centre), bottom.end_mark.map(|mark| mark.centre));
        // One rule for both places: with too little room under the words even for a quarter
        // line's gap, the words are made a little smaller, and the mark never lies over them.
        let [(_, laid, _)] = under.words[..] else { panic!("one box of words") };
        let tight = laid.bottom + 0.25 * laid.size + 0.8 * laid.size - 30.0;
        for theme in [&Theme::builtin(), &low] {
            let on = drawn_with_mark(&song, theme, Some(tight));
            let (mark, (_, smaller, _)) = (on.end_mark.expect("room made"), on.words[0]);
            // (Two lines in a box with room to spare: they move up in it, at their size.)
            assert!(smaller.size <= laid.size && smaller.bottom < laid.bottom, "{smaller:?} from {laid:?}");
            assert!((mark.size - 0.8 * smaller.size).abs() < 0.01, "{mark:?}");
            assert!(mark.top + mark.size <= tight + 0.01 && mark.top >= smaller.bottom + 0.25 * smaller.size - 0.01);
        }
    }

    #[test]
    fn the_mark_is_left_out_with_hidden_words_no_words_and_words_that_lie_elsewhere() {
        let theme = Theme::builtin();
        let song = slide("One\nTwo", None);
        assert!(drawn_with_mark(&song, &theme, Some(FLOOR)).end_mark.is_some());
        // Hide the Words: the mark goes with them.
        assert!(drawn_with_mark(&Slide { words_hidden: true, ..song.clone() }, &theme, Some(FLOOR)).end_mark.is_none());
        // A picture slide, or a box with no words in it.
        assert!(
            drawn_with_mark(&Slide::new(vec![Object::shape(Shape::Rectangle)]), &theme, Some(FLOOR)).end_mark.is_none()
        );
        assert!(drawn_with_mark(&slide("  ", None), &theme, Some(FLOOR)).end_mark.is_none());
        // Words that scroll, or in a box that is turned.
        let mut turned = song.clone();
        turned.objects[0].rotation = 90.0;
        assert!(drawn_with_mark(&turned, &theme, Some(FLOOR)).end_mark.is_none());
        let mut moving = song.clone();
        if let Some(style) = moving.objects[0].text_style_mut() {
            style.scroll = Some(sp_model::Scroll::Up);
        }
        assert!(drawn_with_mark(&moving, &theme, Some(FLOOR)).end_mark.is_none());
        // A build that has shown one paragraph of two: under that one.
        let mut building = song.clone();
        building.objects[0].shown_paragraphs = Some(1);
        let whole = drawn_with_mark(&song, &theme, Some(FLOOR)).end_mark.expect("room");
        let part = drawn_with_mark(&building, &theme, Some(FLOOR)).end_mark.expect("room");
        assert!(part.top < whole.top, "{part:?} above {whole:?}");
        // The scene with the mark has more in it than the scene without, and builds whole.
        let plain = build_scene(&mut Fonts::new(), Some(&song), &theme, 1920, 1080);
        let marked =
            build_scene(&mut Fonts::new(), Some(&Slide { end_mark: Some(FLOOR), ..song.clone() }), &theme, 1920, 1080);
        let runs = |built: &Built| built.scene.encoding().resources.patches.len();
        assert!(runs(&marked) > runs(&plain), "{} and {}", runs(&marked), runs(&plain));
    }

    /// The usual look for the end of a song's mark, under the last words only.
    fn last_words_only() -> sp_model::EndLook {
        sp_model::EndLook { under: sp_model::EndUnder::LastWords, ..Default::default() }
    }

    /// What a scene draws, to compare two scenes: its paths and draw data, and its glyphs.
    fn drawn_scene(slide: &Slide, theme: &Theme) -> (Vec<u32>, Vec<u32>, String) {
        let built = build_scene(&mut Fonts::new(), Some(slide), theme, 1920, 1080);
        let encoding = built.scene.encoding();
        (encoding.path_data.clone(), encoding.draw_data.clone(), format!("{:?}", encoding.resources.glyphs))
    }

    /// The built-in theme with its two languages' boxes where `main` and `translation` say, the
    /// words fitted as `scaling` says and placed in their box as `valign` says.
    fn two_boxes(main: SlideRect, translation: SlideRect, scaling: Scaling, valign: VAlign) -> Theme {
        let mut theme = Theme::builtin();
        for template in &mut theme.templates {
            for look in &mut template.boxes {
                if look.role == Role::primary() {
                    look.rect = main;
                } else if look.role == Role::translation() {
                    look.rect = translation;
                } else {
                    continue;
                }
                look.scaling = scaling;
                look.valign = valign;
            }
        }
        theme
    }

    /// The main language's box right over the translation's, with no room between them.
    fn stacked_close(scaling: Scaling) -> Theme {
        two_boxes(
            SlideRect { x: 120.0, y: 170.0, w: 1680.0, h: 400.0 },
            SlideRect { x: 120.0, y: 575.0, w: 1680.0, h: 400.0 },
            scaling,
            VAlign::Top,
        )
    }

    /// The main language at the left, the translation at the right.
    fn side_by_side() -> Theme {
        two_boxes(
            SlideRect { x: 60.0, y: 140.0, w: 870.0, h: 760.0 },
            SlideRect { x: 990.0, y: 140.0, w: 870.0, h: 760.0 },
            Scaling::TextDown,
            VAlign::Middle,
        )
    }

    /// Where a drawn mark lies, in the look of the words of the object at `at` on `slide`:
    /// left, top, right and bottom.
    fn mark_rect(slide: &Slide, theme: &Theme, at: usize, mark: &EndMark) -> (f32, f32, f32, f32) {
        let template = theme.template_for(slide).expect("a template");
        let object = &slide.objects[at];
        let ObjectKind::Text { style, .. } = &object.kind else { panic!("words") };
        let look = template.text_style(object.role.as_ref(), style).expect("a look");
        let half = end_mark_width(&mut Fonts::new(), &look, mark.size, theme.end_look.mark) / 2.0;
        (mark.centre - half, mark.top, mark.centre + half, mark.top + mark.size)
    }

    #[test]
    fn a_slide_in_one_language_is_drawn_the_same_under_each_language_and_the_last_words() {
        // The last words only is the mark as it always was (one mark, under the lowest words):
        // a slide with no translation draws exactly that under each language's too, its words,
        // its mark and every part of the scene, whatever the theme, floor, place and size.
        let usual = sp_model::EndLook::default();
        let empty_translation = slide(&lines(3), Some("   "));
        for theme in [Theme::builtin(), tall(Scaling::TextDown), tall(Scaling::None), stacked_close(Scaling::TextDown)]
        {
            for song in
                [slide("One\nTwo", None), slide(&lines(8), None), slide(&lines(12), None), empty_translation.clone()]
            {
                for floor in [FLOOR, OVER_THE_COPYRIGHT_LINE, 700.0] {
                    for place in [sp_model::EndPlace::Under, sp_model::EndPlace::Bottom] {
                        for size in [40, 80, 160] {
                            let look = sp_model::EndLook { size, place, ..usual };
                            let each = Theme { end_look: look, ..theme.clone() };
                            let last = Theme {
                                end_look: sp_model::EndLook { under: sp_model::EndUnder::LastWords, ..look },
                                ..theme.clone()
                            };
                            let said = format!("{look:?} at {floor}");
                            let (a, b) = (
                                drawn_with_mark(&song, &each, Some(floor)),
                                drawn_with_mark(&song, &last, Some(floor)),
                            );
                            assert_eq!(
                                (&a.words, a.end_mark, &a.end_marks),
                                (&b.words, b.end_mark, &b.end_marks),
                                "{said}"
                            );
                            assert!(a.end_marks.len() <= 1, "{said}: one mark");
                            let marked = Slide { end_mark: Some(floor), ..song.clone() };
                            assert!(drawn_scene(&marked, &each) == drawn_scene(&marked, &last), "{said}: the scene");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn each_language_gets_a_mark_under_its_words_in_their_look_and_size() {
        // The built-in theme: the main language over the translation, with room under each.
        let theme = Theme::builtin();
        let song = slide("One\nTwo", Some("Uno\nDos"));
        let off = drawn_with_mark(&song, &theme, None);
        let on = drawn_with_mark(&song, &theme, Some(FLOOR));
        assert_eq!(on.words, off.words, "the words keep their size and place");
        assert!(!on.overflow);
        let [(lower_at, lower), (upper_at, upper)] = on.end_marks[..] else { panic!("two marks: {:?}", on.end_marks) };
        assert_eq!((lower_at, upper_at), (1, 0), "the translation's (the lowest words') first");
        assert_eq!(on.end_mark, Some(lower), "the one under the lowest words");
        let laid = |at: usize| on.words.iter().find(|(have, _, _)| *have == at).expect("its words").1;
        let template = theme.template_for(&song).expect("a template");
        for (at, mark) in [(lower_at, lower), (upper_at, upper)] {
            let words = laid(at);
            // At 80% of its own words' size, half a line of them under their last line, centred
            // on their box.
            assert!((mark.size - 0.8 * words.size).abs() < 0.01, "{at}: {mark:?} under {words:?}");
            assert!((mark.top - (words.bottom + 0.5 * words.size)).abs() < 0.01, "{at}: {mark:?} under {words:?}");
            assert_eq!(mark.centre, words.centre);
            // In its own words' look: the main language's white, the translation's warm grey.
            let object = &song.objects[at];
            let ObjectKind::Text { style, .. } = &object.kind else { panic!("words") };
            let look = template.text_style(object.role.as_ref(), style).expect("a look");
            assert_eq!(end_mark_style(&look, mark.size, None).fill, look.fill);
        }
        assert!(laid(0).size > laid(1).size, "two sizes of words, two sizes of mark");
        let fills: Vec<Rgba> = [0, 1]
            .map(|at| {
                let object = &song.objects[at];
                let ObjectKind::Text { style, .. } = &object.kind else { panic!("words") };
                template.text_style(object.role.as_ref(), style).expect("a look").fill
            })
            .to_vec();
        assert_eq!(fills, [Rgba::WHITE, Rgba::rgb(210, 206, 198)]);
        // The upper mark lies between the two languages, touching neither.
        let (_, top, _, bottom) = mark_rect(&song, &theme, 0, &upper);
        assert!(top >= laid(0).bottom + 0.25 * laid(0).size && bottom < laid(1).top, "{upper:?} between");
        // A colour of its own: both marks take it, in their own words' face and size.
        let yellow = Rgba::rgb(0xff, 0xc4, 0x00);
        let coloured = Theme { end_look: sp_model::EndLook { colour: Some(yellow), ..Default::default() }, ..theme };
        let painted = drawn_with_mark(&song, &coloured, Some(FLOOR));
        assert_eq!(painted.end_marks, on.end_marks, "the same marks in the same places");
        assert!(take_end_mark_left_out().is_none());
    }

    #[test]
    fn the_last_words_only_and_the_bottom_of_the_screen_have_one_mark_as_it_always_was() {
        let song = slide("One\nTwo", Some("Uno\nDos"));
        let largest_and_lowest = |painted: &Painted| {
            let largest =
                painted.words.iter().map(|(_, laid, _)| *laid).reduce(|a, b| if b.size > a.size { b } else { a });
            let lowest =
                painted.words.iter().map(|(_, laid, _)| *laid).reduce(|a, b| if b.bottom >= a.bottom { b } else { a });
            (largest.expect("words"), lowest.expect("words"))
        };
        // Under the last words only: one mark, at 80% of the largest words, under the lowest.
        let last = Theme { end_look: last_words_only(), ..Theme::builtin() };
        let on = drawn_with_mark(&song, &last, Some(FLOOR));
        let (largest, lowest) = largest_and_lowest(&on);
        let [(at, mark)] = on.end_marks[..] else { panic!("one mark: {:?}", on.end_marks) };
        assert_eq!((at, on.end_mark), (0, Some(mark)), "the main language's look, the largest");
        assert!(
            (mark.size - 0.8 * largest.size).abs() < 0.01
                && (mark.top - (lowest.bottom + 0.5 * largest.size)).abs() < 0.01
        );
        assert_eq!(mark.centre, lowest.centre);
        // At the bottom of the screen: one mark, whichever "under", the largest words' look.
        for under in [sp_model::EndUnder::EachLanguage, sp_model::EndUnder::LastWords] {
            let look = sp_model::EndLook { place: sp_model::EndPlace::Bottom, under, ..Default::default() };
            let low = Theme { end_look: look, ..Theme::builtin() };
            let on = drawn_with_mark(&song, &low, Some(FLOOR));
            let [(at, mark)] = on.end_marks[..] else { panic!("{under:?}: one mark: {:?}", on.end_marks) };
            assert_eq!(at, 0, "{under:?}");
            assert!((mark.top + mark.size - FLOOR).abs() < 0.01, "{under:?}: {mark:?}");
        }
    }

    #[test]
    fn stacked_close_the_upper_words_make_room_for_their_mark_and_the_lower_never_move() {
        // Four lines of the main language fill their box, which the translation's box is right
        // under: no room between them for the main language's mark as things are.
        let song = slide(&lines(4), Some("Uno\nDos"));
        let theme = stacked_close(Scaling::TextDown);
        let off = drawn_with_mark(&song, &theme, None);
        let (upper_off, lower_off) = (off.words[0].1, off.words[1].1);
        let wanted = upper_off.bottom + 0.25 * upper_off.size + 0.8 * upper_off.size;
        assert!(wanted > lower_off.top - 0.25 * upper_off.size, "no room as things are: {upper_off:?} {lower_off:?}");
        let on = drawn_with_mark(&song, &theme, Some(FLOOR));
        let (upper, lower) = (on.words[0].1, on.words[1].1);
        assert_eq!(lower, lower_off, "the translation never moves or shrinks for the mark over it");
        // The main language's words come out smaller, inside their box, and leave room: by
        // about a fifth, for the 1.3 lines of theirs the mark and its two gaps take.
        assert!(upper.size < upper_off.size && upper.size > 0.7 * upper_off.size, "{upper:?} from {upper_off:?}");
        assert!(upper.top >= 170.0 - 0.01 && upper.bottom <= upper_off.bottom && !on.overflow, "{upper:?}");
        let [(1, low_mark), (0, high_mark)] = on.end_marks[..] else { panic!("two marks: {:?}", on.end_marks) };
        assert!((high_mark.size - 0.8 * upper.size).abs() < 0.01, "at its own size: {high_mark:?}");
        let (_, top, _, bottom) = mark_rect(&song, &theme, 0, &high_mark);
        assert!(top >= upper.bottom + 0.25 * upper.size - 0.01, "{high_mark:?} under {upper:?}");
        assert!(bottom <= lower.top - 0.25 * upper.size + 0.01, "{high_mark:?} over {lower:?}");
        assert!((low_mark.top - (lower.bottom + 0.5 * lower.size)).abs() < 0.01, "{low_mark:?} under {lower:?}");
        assert!(take_end_mark_left_out().is_none());
        // Words that keep their size, in a box drawn close round them, with the translation's
        // first line `gap` lines of them under their last: the mark between is drawn smaller,
        // down to 40% of their size, against its floor a quarter line over the translation; with
        // less room it is left out, and that is noted. The translation and its mark are as they
        // were, and no words move.
        let song = slide("One\nTwo", Some("Uno\nDos"));
        let kept = stacked_close(Scaling::None);
        let off = drawn_with_mark(&song, &kept, None);
        let (main, translation) = (off.words[0].1, off.words[1].1);
        let below_its_box = translation.top - 575.0;
        for (gap, room) in [(1.1, Some(0.6)), (0.8, None)] {
            let mut close = kept.clone();
            for template in &mut close.templates {
                for look in &mut template.boxes {
                    if look.role == Role::primary() {
                        look.rect = SlideRect { y: main.top, h: main.bottom - main.top + 0.5, ..look.rect };
                    } else if look.role == Role::translation() {
                        look.rect.y = main.bottom + gap * main.size - below_its_box;
                    }
                }
            }
            let song = Slide { id: sp_model::Uuid::new_v4(), ..song.clone() };
            let off = drawn_with_mark(&song, &close, None);
            let on = drawn_with_mark(&song, &close, Some(FLOOR));
            let (upper, lower) = (on.words[0].1, on.words[1].1);
            assert_eq!(on.words, off.words, "{gap}: no words move");
            assert!((lower.top - (upper.bottom + gap * upper.size)).abs() < 0.01, "{gap}: {upper:?} {lower:?}");
            assert_eq!(on.end_marks[0].0, 1, "{gap}: the translation's mark");
            assert!((on.end_marks[0].1.top - (lower.bottom + 0.5 * lower.size)).abs() < 0.01, "{gap}");
            match (room, &on.end_marks[1..]) {
                (Some(room), [(0, mark)]) => {
                    assert!((mark.size - room * upper.size).abs() < 0.01, "{gap}: {mark:?}");
                    assert!((mark.top - (upper.bottom + 0.25 * upper.size)).abs() < 0.01, "{gap}: {mark:?}");
                    let (_, _, _, bottom) = mark_rect(&song, &close, 0, mark);
                    assert!(bottom <= lower.top - 0.25 * upper.size + 0.01, "{gap}: {mark:?}");
                    assert!(take_end_mark_left_out().is_none(), "{gap}");
                }
                (None, []) => {
                    assert_eq!(
                        take_end_mark_left_out(),
                        Some((song.id, MarkLeftOut::NoRoom)),
                        "{gap}: noted, of this slide"
                    );
                }
                (_, marks) => panic!("{gap}: {marks:?}"),
            }
        }
    }

    #[test]
    fn side_by_side_each_column_has_its_mark_under_it_against_the_slides_floor() {
        // Two lines of the main language at the left, six of the translation at the right: the
        // short column's mark is under its own words, not held up by the long column beside it.
        let song = slide("One\nTwo", Some(&lines(6)));
        let theme = side_by_side();
        let off = drawn_with_mark(&song, &theme, None);
        let on = drawn_with_mark(&song, &theme, Some(FLOOR));
        assert_eq!(on.words, off.words);
        let (left, right) = (on.words[0].1, on.words[1].1);
        assert!(left.bottom < right.bottom && left.right < right.left, "{left:?} beside {right:?}");
        let [(1, right_mark), (0, left_mark)] = on.end_marks[..] else { panic!("two marks: {:?}", on.end_marks) };
        for (at, mark, words) in [(0, left_mark, left), (1, right_mark, right)] {
            assert_eq!(mark.centre, words.centre, "{at}: under its own column");
            assert!((mark.size - 0.8 * words.size).abs() < 0.01, "{at}: {mark:?}");
            assert!((mark.top - (words.bottom + 0.5 * words.size)).abs() < 0.01, "{at}: {mark:?} under {words:?}");
        }
        // Neither lies over the other column's words.
        let (from, _, to, _) = mark_rect(&song, &theme, 0, &left_mark);
        assert!(to < right.left && from > 60.0, "{left_mark:?} beside {right:?}");
        let (from, _, _, _) = mark_rect(&song, &theme, 1, &right_mark);
        assert!(from > left.right, "{right_mark:?}");
    }

    #[test]
    fn words_with_the_other_languages_beside_them_and_reaching_lower_get_no_mark_over_those() {
        // Both boxes across the slide: the main language's two lines at its top, the
        // translation's eight down the middle of its own, beside them and reaching lower. A mark
        // under the main language would lie over the translation: it is left out, and noted.
        let theme = two_boxes(
            SlideRect { x: 120.0, y: 200.0, w: 1680.0, h: 700.0 },
            SlideRect { x: 120.0, y: 100.0, w: 1680.0, h: 860.0 },
            Scaling::TextDown,
            VAlign::Top,
        );
        let mut theme = theme;
        for template in &mut theme.templates {
            for look in &mut template.boxes {
                if look.role == Role::translation() {
                    look.valign = VAlign::Middle;
                }
            }
        }
        let song = slide("One\nTwo", Some(&lines(8)));
        let off = drawn_with_mark(&song, &theme, None);
        let (main, translation) = (off.words[0].1, off.words[1].1);
        assert!(translation.top < main.bottom && translation.bottom > main.bottom, "{main:?} {translation:?}");
        let on = drawn_with_mark(&song, &theme, Some(FLOOR));
        assert_eq!(on.words, off.words, "nothing moves");
        assert_eq!(on.end_marks.iter().map(|(at, _)| *at).collect::<Vec<_>>(), [1], "the translation's only");
        assert_eq!(take_end_mark_left_out(), Some((song.id, MarkLeftOut::OverTheOtherLanguage)), "on purpose");
    }

    #[test]
    fn no_mark_lies_over_words_past_its_floor_and_words_never_grow_or_move_down() {
        let layouts = [
            ("built-in", Theme::builtin()),
            ("stacked close", stacked_close(Scaling::TextDown)),
            ("stacked close, kept size", stacked_close(Scaling::None)),
            ("side by side", side_by_side()),
            ("tall over built-in translation", tall(Scaling::TextDown)),
        ];
        let usual = sp_model::EndLook::default();
        for (name, theme) in layouts {
            for (main_lines, translation_lines) in [(2, 2), (4, 2), (6, 4), (8, 3), (2, 8)] {
                let song = slide(&lines(main_lines), Some(&lines(translation_lines)));
                let off = drawn_with_mark(&song, &theme, None);
                for floor in [FLOOR, OVER_THE_COPYRIGHT_LINE] {
                    for mark in [sp_model::EndMark::Star, sp_model::EndMark::Square] {
                        for size in [40, 80, 160] {
                            let look = sp_model::EndLook { mark, size, ..usual };
                            let themed = Theme { end_look: look, ..theme.clone() };
                            let said =
                                format!("{name}, {main_lines}+{translation_lines} lines, floor {floor}, {look:?}");
                            let on = drawn_with_mark(&song, &themed, Some(floor));
                            assert!(!on.overflow || off.overflow, "{said}: nothing new spills");
                            for ((at, now, _), (_, was, _)) in on.words.iter().zip(&off.words) {
                                assert!(now.size <= was.size + 0.01 && now.bottom <= was.bottom + 0.01, "{said}: {at}");
                            }
                            for (at, drawn) in &on.end_marks {
                                let (left, top, right, bottom) = mark_rect(&song, &themed, *at, drawn);
                                assert!(bottom <= floor + 0.01, "{said}: {drawn:?} passes the floor");
                                for (other, words, _) in &on.words {
                                    let clear = right <= words.left
                                        || left >= words.right
                                        || bottom <= words.top
                                        || top >= words.bottom;
                                    assert!(
                                        clear,
                                        "{said}: the mark of {at} {drawn:?} over the words of {other} {words:?}"
                                    );
                                }
                            }
                            let _ = take_end_mark_left_out();
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn where_the_marks_go_is_worked_out_once_and_never_stale() {
        let theme = stacked_close(Scaling::TextDown);
        let song = Slide { end_mark: Some(FLOOR), ..slide(&lines(4), Some("Uno\nDos")) };
        let mut fonts = Fonts::new();
        let draw = |fonts: &mut Fonts, slide: &Slide, theme: &Theme| {
            let painted = draw_objects(&mut Scene::new(), fonts, Affine::IDENTITY, slide, theme, 0.0);
            (painted.words, painted.end_marks)
        };
        let first = draw(&mut fonts, &song, &theme);
        assert_eq!((first.1.len(), fonts.mark_rooms_kept()), (2, 1));
        for _ in 0..3 {
            assert_eq!(draw(&mut fonts, &song, &theme), first);
        }
        assert!(!fonts.trying && fonts.mark_rooms_kept() == 1, "looked up, not worked out again");
        // The mark's colour moves nothing: the same plan serves it.
        let yellow = Rgba::rgb(0xff, 0xc4, 0x00);
        let coloured =
            Theme { end_look: sp_model::EndLook { colour: Some(yellow), ..Default::default() }, ..theme.clone() };
        assert_eq!(draw(&mut fonts, &song, &coloured), first);
        assert_eq!(fonts.mark_rooms_kept(), 1);
        // The setting, another mark, another place, a lower translation (another floor for the
        // mark over it), other words: each its own answer, the same as new fonts give.
        let fresh = |slide: &Slide, theme: &Theme| draw(&mut Fonts::new(), slide, theme);
        let last = Theme { end_look: last_words_only(), ..theme.clone() };
        let squares = Theme {
            end_look: sp_model::EndLook { mark: sp_model::EndMark::Square, ..Default::default() },
            ..theme.clone()
        };
        let bottom = Theme {
            end_look: sp_model::EndLook { place: sp_model::EndPlace::Bottom, ..Default::default() },
            ..theme.clone()
        };
        let mut lower = theme.clone();
        for template in &mut lower.templates {
            for look in &mut template.boxes {
                if look.role == Role::translation() {
                    look.rect.y += 30.0;
                }
            }
        }
        let other = Slide { end_mark: Some(FLOOR), ..slide(&lines(5), Some("Uno\nDos")) };
        for (slide, theme) in
            [(&song, &last), (&song, &squares), (&song, &bottom), (&song, &lower), (&other, &theme), (&song, &theme)]
        {
            assert_eq!(draw(&mut fonts, slide, theme), fresh(slide, theme));
        }
        assert_ne!(draw(&mut fonts, &song, &last).1, first.1, "the last words only is another answer");
        // Two slides alike but for which language a box holds (a template with no box for the
        // translation draws it in the main language's look): each its own answer.
        let mut one = theme.clone();
        for template in &mut one.templates {
            template.boxes.retain(|look| look.role != Role::translation());
        }
        let mut two_languages = song.clone();
        for (object, y) in two_languages.objects.iter_mut().zip([170.0, 600.0]) {
            object.frame = Some(SlideRect { x: 120.0, y, w: 1680.0, h: 400.0 });
        }
        let mut one_language = two_languages.clone();
        one_language.objects[1].role = Some(Role::primary());
        let (two, single) = (draw(&mut fonts, &two_languages, &one), draw(&mut fonts, &one_language, &one));
        assert_eq!((two.1.len(), single.1.len()), (2, 1), "{two:?} {single:?}");
        assert_eq!(single, fresh(&one_language, &one));
        assert_ne!(draw(&mut fonts, &song, &lower).0[0], first.0[0], "a lower translation: less room made");
    }

    /// The built-in theme with its main language's box in the translation's look (64 high,
    /// warm grey), at the box 120, 560, 1680 by 450, its words at its bottom.
    fn low_translation_look() -> Theme {
        let mut theme = Theme::builtin();
        let look = theme.templates[0].style_for(&Role::translation()).cloned().expect("the translation's look");
        for template in &mut theme.templates {
            for main in template.boxes.iter_mut().filter(|style| style.role == Role::primary()) {
                *main = BoxStyle {
                    role: Role::primary(),
                    rect: SlideRect { x: 120.0, y: 560.0, w: 1680.0, h: 450.0 },
                    valign: VAlign::Bottom,
                    ..look.clone()
                };
            }
        }
        theme
    }

    #[test]
    fn words_that_make_room_for_the_mark_always_get_it() {
        // At 40%, whether the mark fitted and where it went were worked out by sums that round
        // apart: the words moved up to make room for it, and then no mark was drawn (and it was
        // logged as left out). Two lines at the foot of their box, the floor close under them.
        let forty = sp_model::EndLook { size: 40, ..Default::default() };
        let theme = Theme { end_look: forty, ..low_translation_look() };
        let song = slide("Uno\nDos", None);
        let (_, before, _) = drawn_with_mark(&song, &theme, None).words[0];
        assert!((before.bottom - 1010.0).abs() < 0.5, "{before:?}");
        for floor in [1047.6, 1020.0] {
            let on = drawn_with_mark(&song, &theme, Some(floor));
            let (_, laid, _) = on.words[0];
            assert!(laid.bottom < before.bottom, "{floor}: the words make room: {laid:?}");
            let mark = on.end_mark.unwrap_or_else(|| panic!("{floor}: the words moved up and no mark: {laid:?}"));
            assert!((mark.size - 0.4 * laid.size).abs() < 0.01 && mark.top + mark.size <= floor + 0.01, "{mark:?}");
            assert_eq!(take_end_mark_left_out(), None, "{floor}");
        }
        // The same over many floors and sizes, in one language and two: words that move for a
        // mark, or come out smaller, always have it.
        let mut cases: Vec<(Slide, Theme, f32)> = Vec::new();
        for step in 0..48 {
            let floor = 1000.0 + step as f32 * 1.03;
            cases.push((song.clone(), low_translation_look(), floor));
            cases.push((slide(&lines(8), Some(&lines(3))), tall(Scaling::TextDown), floor));
            // The translation a little lower each time under a main language's full box: the
            // floor of the mark between the two moves by as much.
            let mut lower = stacked_close(Scaling::TextDown);
            for template in &mut lower.templates {
                for look in template.boxes.iter_mut().filter(|style| style.role == Role::translation()) {
                    look.rect.y += step as f32 * 1.03;
                }
            }
            cases.push((slide(&lines(4), Some("Uno\nDos")), lower, FLOOR));
        }
        for (song, theme, floor) in cases {
            let off = drawn_with_mark(&song, &theme, None);
            for size in [40, 50, 80] {
                let themed = Theme { end_look: sp_model::EndLook { size, ..Default::default() }, ..theme.clone() };
                let on = drawn_with_mark(&song, &themed, Some(floor));
                for ((at, now, _), (_, was, _)) in on.words.iter().zip(&off.words) {
                    if now == was {
                        continue;
                    }
                    let theirs =
                        on.end_marks.iter().any(|(under, _)| main_words(&song, *under) == main_words(&song, *at));
                    assert!(theirs, "{size}% at {floor}: the words of {at} moved ({was:?} to {now:?}) and got no mark");
                }
                let _ = take_end_mark_left_out();
            }
        }
    }

    #[test]
    fn every_shape_is_drawn_inside_its_place() {
        for shape in Shape::ALL {
            let rect = SlideRect { x: 100.0, y: 200.0, w: 400.0, h: 300.0 };
            let bounds = shape_path(shape, rect).bounding_box();
            assert!(bounds.width() > 300.0 && bounds.height() > 200.0, "{shape:?} is too small: {bounds:?}");
            assert!(bounds.x0 >= 99.9 && bounds.y0 >= 199.9 && bounds.x1 <= 500.1 && bounds.y1 <= 500.1, "{shape:?}");
        }
        // A slide with a shape and a stroked text box builds.
        let mut boxed = Object::text(Role::primary(), "Words");
        boxed.stroke = Some(sp_model::StrokeLook { width: 6.0, color: Rgba::WHITE });
        let mut star = Object::shape(Shape::Star);
        star.shadow = Some(sp_model::Shadow::default());
        star.flip_h = true;
        let slide = Slide::new(vec![star, boxed]);
        assert!(!build_scene(&mut Fonts::new(), Some(&slide), &Theme::builtin(), 1920, 1080).overflow);
    }

    #[test]
    fn every_transition_builds_at_every_stage() {
        let theme = Theme::builtin();
        let mut fonts = Fonts::new();
        let from = build_scene(&mut fonts, Some(&slide("Old", None)), &theme, 640, 360).scene;
        let to = build_scene(&mut fonts, Some(&slide("New", None)), &theme, 640, 360).scene;
        for kind in sp_model::TransitionKind::ALL {
            for direction in sp_model::Direction::ALL {
                for amount in [0.0, 0.3, 0.5, 0.7, 1.0] {
                    let _ = transition(kind, direction, &from, &to, amount, 640, 360);
                }
            }
        }
    }

    #[test]
    fn turned_and_faded_objects_build() {
        let mut s = slide("Turned", Some("Faded"));
        s.objects[0].rotation = 30.0;
        s.objects[1].opacity = 0.4;
        assert!(!build_scene(&mut Fonts::new(), Some(&s), &Theme::builtin(), 640, 360).overflow);
    }

    #[test]
    fn parsed_song_slides_build() {
        let p = parse_song_text("T", "[V]\nA\n--\nБ\n\nC\n");
        let mut fonts = Fonts::new();
        for (_, s) in p.slides() {
            assert!(!build_scene(&mut fonts, Some(s), &Theme::builtin(), 480, 270).overflow);
        }
    }

    #[test]
    fn a_screen_picture_shows_the_scene_it_is_given_and_nothing_else() {
        let mut fonts = Fonts::new();
        let screen = sp_model::Uuid::new_v4();
        let with = |object: Object| Slide::new(vec![object]);
        let paths = |built: &Built| built.scene.encoding().n_paths;
        // What the screen shows: here, three squares.
        let mut source = Scene::new();
        for at in [0.0, 100.0, 200.0] {
            source.fill(Fill::NonZero, Affine::IDENTITY, Color::WHITE, None, &Rect::new(at, 0.0, at + 50.0, 50.0));
        }
        let picture = || vec![ScreenPicture { screen, scene: source.clone(), width: 1920, height: 1080 }];
        let theme = Theme::builtin();
        let empty = paths(&build_scene(&mut fonts, Some(&with(Object::shape(Shape::Rectangle))), &theme, 960, 540));
        let alone = build_scene(&mut fonts, Some(&with(Object::screen(Some(screen)))), &theme, 960, 540);
        let shown = with_screen_pictures(picture(), || {
            build_scene(&mut fonts, Some(&with(Object::screen(Some(screen)))), &theme, 960, 540)
        });
        // Without its picture it draws nothing (no stand-in on the screens' thread); with it,
        // the three squares, clipped to its place.
        assert!(paths(&alone) < empty, "a rectangle draws its fill; a bare screen picture nothing");
        assert!(paths(&shown) >= paths(&alone) + 3, "{} and {}", paths(&shown), paths(&alone));
        // A picture of another screen, or one given only while another scene was built, is not shown.
        let other = with_screen_pictures(picture(), || {
            build_scene(&mut fonts, Some(&with(Object::screen(Some(sp_model::Uuid::new_v4())))), &theme, 960, 540)
        });
        assert_eq!(paths(&other), paths(&alone));
        let after = build_scene(&mut fonts, Some(&with(Object::screen(Some(screen)))), &theme, 960, 540);
        assert_eq!(paths(&after), paths(&alone));
        // In the operator's window it has a stand-in, so it can be seen and moved.
        show_placeholders(true);
        let stand_in = build_scene(&mut fonts, Some(&with(Object::screen(Some(screen)))), &theme, 960, 540);
        show_placeholders(false);
        assert!(paths(&stand_in) > paths(&alone));
    }
}
