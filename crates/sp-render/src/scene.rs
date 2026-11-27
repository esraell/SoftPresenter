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

/// The backdrop, the theme's colour under the media, and the media (or the theme's picture).
fn paint_under(scene: &mut Scene, layers: &Layers, theme: &Theme, target_w: u32, target_h: u32) {
    let whole = Rect::new(0.0, 0.0, f64::from(target_w), f64::from(target_h));
    let backdrop = layers.backdrop.map_or(Color::BLACK, color);
    scene.fill(Fill::NonZero, Affine::IDENTITY, backdrop, None, &whole);
    let (x, y, w, _) = fit_rect(target_w, target_h);
    let to_target = Affine::translate((x, y)) * Affine::scale(w / f64::from(SLIDE_WIDTH));

    // The theme's background colour fills the slide, under the media. A cleared screen stays black.
    if let (Some(background), Some(_)) = (theme.background, layers.slide) {
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
/// fitted to, the middle of their box across the slide, and their first line's top and last
/// line's bottom.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Laid {
    pub size: f32,
    pub centre: f32,
    pub top: f32,
    pub bottom: f32,
}

/// The end of a song's mark as it is drawn: its letters' size, its top, and the place across
/// the slide it is centred on, in slide units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct EndMark {
    pub size: f32,
    pub top: f32,
    pub centre: f32,
}

/// The mark itself: three stars, kept together and apart by no-break spaces.
const END_MARK: &str = "*\u{a0}\u{a0}\u{a0}\u{a0}*\u{a0}\u{a0}\u{a0}\u{a0}*";

/// The mark's size beside the largest words', and how far under the lowest words its top is, in
/// lines of the largest words.
const END_MARK_SIZE: f32 = 0.8;
const END_MARK_GAP: f32 = 0.5;

/// Where the end of a song's mark goes under a slide's words as they were laid out (`words`,
/// each with the place of its object on the slide): at 80% of the size the largest words were
/// fitted to, its top half a line of theirs under the lowest words' last line, centred on that
/// box's middle. With it, which words are the largest: the mark takes their look. `None` when
/// the slide has no words, or the mark would pass `floor`: the words are never moved or made
/// smaller for it.
fn end_mark_place(words: &[(usize, Laid)], floor: f32) -> Option<(usize, EndMark)> {
    // The first of the largest, and the last of the lowest: as the slide lists its boxes.
    let (largest, sized) =
        words.iter().copied().reduce(|best, next| if next.1.size > best.1.size { next } else { best })?;
    let (_, lowest) =
        words.iter().copied().reduce(|best, next| if next.1.bottom >= best.1.bottom { next } else { best })?;
    let size = sized.size * END_MARK_SIZE;
    let mark = EndMark { size, top: lowest.bottom + sized.size * END_MARK_GAP, centre: lowest.centre };
    // The mark is one line as tall as its letters' size.
    (mark.top + size <= floor).then_some((largest, mark))
}

/// What drawing a slide's objects came to: whether any text overflowed its box, where each
/// box's words lie (with the place of its object on the slide), and the end of a song's mark,
/// if it was drawn.
pub(crate) struct Painted {
    pub overflow: bool,
    #[cfg_attr(not(test), expect(dead_code, reason = "the tests compare the words with the mark on and off"))]
    pub words: Vec<(usize, Laid)>,
    #[cfg_attr(not(test), expect(dead_code, reason = "the tests look at where the mark went"))]
    pub end_mark: Option<EndMark>,
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
    let mut words = Vec::new();
    // The slide is drawn with one of the theme's templates: the one it names, or the one
    // that suits what is on it.
    let template = theme.template_for(slide);
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
            let (spilt, laid) =
                draw_text_box(scene, fonts, placed, (text, spans), &style, rect, time, object.shown_paragraphs);
            overflow |= spilt;
            // Words in a box that is turned or flipped lie somewhere else on the slide than
            // their layout says: the mark is never put under those.
            let straight = object.rotation == 0.0 && !object.flip_h && !object.flip_v;
            words.extend(laid.filter(|_| straight).map(|laid| (at, laid)));
        }
        if faded {
            scene.pop_layer();
        }
    }
    // The end of a song's mark, on a layer of its own over the slide's objects: the words above
    // were laid out and drawn without it.
    let mut end_mark = None;
    if let (Some(floor), Some(template)) = (slide.end_mark, template)
        && !slide.words_hidden
        && let Some((largest, mark)) = end_mark_place(&words, floor)
        && let Some(object) = slide.objects.get(largest)
        && let ObjectKind::Text { style, .. } = &object.kind
        && let Some(look) = template.text_style(object.role.as_ref(), style)
    {
        draw_end_mark(scene, fonts, to_target, &look, mark);
        end_mark = Some(mark);
    }
    Painted { overflow, words, end_mark }
}

/// Draws the end of a song's mark in the face, weight, colour, outline and shadow of the words
/// it follows (`look`), on one line of its own that nothing else of their look changes.
fn draw_end_mark(scene: &mut Scene, fonts: &mut Fonts, to_target: Affine, look: &BoxStyle, mark: EndMark) {
    let style = BoxStyle {
        size: mark.size,
        min_size: mark.size,
        align: Align::Center,
        valign: VAlign::Top,
        scaling: Scaling::None,
        line_height: 1.0,
        ..plain_look(look.clone())
    };
    // Room for the three stars whatever the face: they are centred in it and never wrap.
    let room = mark.size * 20.0;
    let rect = SlideRect { x: mark.centre - room / 2.0, y: mark.top, w: room, h: mark.size };
    draw_text_box(scene, fonts, to_target, (END_MARK, &[]), &style, rect, 0.0, None);
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
            laid = Some(Laid { size, centre: rect.x + rect.w / 2.0, top, bottom });
        }
    }
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
        let slide = floor.map_or_else(|| slide.clone(), |floor| slide.with_end_mark(floor));
        draw_objects(&mut Scene::new(), &mut Fonts::new(), Affine::IDENTITY, &slide, theme, 0.0)
    }

    const FLOOR: f32 = SLIDE_HEIGHT * 0.97;

    #[test]
    fn the_mark_sits_half_a_line_under_the_lowest_words_at_80_percent() {
        let theme = Theme::builtin();
        // Lines enough that the words are fitted smaller than the box's own size.
        let words = "One\nTwo\nThree\nFour\nFive\nSix\nSeven\nEight";
        let drawn = drawn_with_mark(&slide(words, None), &theme, Some(FLOOR));
        let [(_, laid)] = drawn.words[..] else { panic!("one box of words: {:?}", drawn.words) };
        let nominal = theme.templates[0].boxes[0].size;
        assert!(laid.size < nominal, "fitted to {} from {nominal}", laid.size);
        let mark = drawn.end_mark.expect("room under the words");
        assert!((mark.size - 0.8 * laid.size).abs() < 0.01, "80% of the fitted size: {mark:?} under {laid:?}");
        assert!((mark.top - (laid.bottom + 0.5 * laid.size)).abs() < 0.01, "half a line under: {mark:?}, {laid:?}");
        let place = theme.templates[0].place_for(Some(&Role::primary()));
        assert!((mark.centre - (place.x + place.w / 2.0)).abs() < 0.01, "centred on the box: {mark:?} in {place:?}");
        assert_eq!(mark.centre, laid.centre);

        // Two languages: the look of the largest words, under the lower block.
        let drawn = drawn_with_mark(&slide("One\nTwo", Some("Uno\nDos")), &theme, Some(FLOOR));
        let [(_, first), (_, second)] = drawn.words[..] else { panic!("two boxes: {:?}", drawn.words) };
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

    #[test]
    fn no_mark_when_the_words_fill_to_the_bottom_or_meet_the_copyright_line() {
        let theme = Theme::builtin();
        let short = slide("One\nTwo", None);
        let laid = drawn_with_mark(&short, &theme, None).words[0].1;
        let bottom = laid.bottom + 0.5 * laid.size + 0.8 * laid.size;
        // Room to a hair, and none a hair less: the slide's own 3%, or the copyright line's top.
        assert!(drawn_with_mark(&short, &theme, Some(bottom + 0.01)).end_mark.is_some());
        assert!(drawn_with_mark(&short, &theme, Some(bottom - 0.01)).end_mark.is_none());
        // Words that fill their box to the foot of the slide leave it no room, and stay as they are.
        let mut full = slide("One\nTwo\nThree\nFour\nFive\nSix\nSeven\nEight", None);
        full.objects[0].frame = Some(SlideRect { x: 60.0, y: 40.0, w: 1800.0, h: 1000.0 });
        let (off, on) = (drawn_with_mark(&full, &theme, None), drawn_with_mark(&full, &theme, Some(FLOOR)));
        assert!(on.end_mark.is_none(), "{:?} under {:?}", on.end_mark, on.words);
        assert_eq!(on.words, off.words);
    }

    #[test]
    fn the_mark_is_left_out_with_hidden_words_no_words_and_words_that_lie_elsewhere() {
        let theme = Theme::builtin();
        let song = slide("One\nTwo", None);
        assert!(drawn_with_mark(&song, &theme, Some(FLOOR)).end_mark.is_some());
        // Hide the Words: the mark goes with them.
        assert!(drawn_with_mark(&song.without_words(), &theme, Some(FLOOR)).end_mark.is_none());
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
        let marked = build_scene(&mut Fonts::new(), Some(&song.with_end_mark(FLOOR)), &theme, 1920, 1080);
        let runs = |built: &Built| built.scene.encoding().resources.patches.len();
        assert!(runs(&marked) > runs(&plain), "{} and {}", runs(&marked), runs(&plain));
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
