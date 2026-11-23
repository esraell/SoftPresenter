//! Image tests: draw through the real GPU path, read the pixels back and compare with stored
//! pictures in `tests/golden/`. Run with `PC_UPDATE_GOLDEN=1` to write the pictures; without
//! it a missing picture is a failure, so a deleted file cannot pass quietly.
//! They use the system font and need a GPU.

use std::path::PathBuf;

use sp_model::{Object, ObjectKind, Role, Slide, Theme};
use sp_render::vello::ScratchSizes;
use sp_render::vello::util::RenderContext;
use sp_render::{Fonts, Painter, build_scene, read_pixels, target_texture, wgpu};

fn render(slide: Option<&Slide>, w: u32, h: u32) -> Vec<u8> {
    render_with(slide, &Theme::builtin(), w, h)
}

fn render_with(slide: Option<&Slide>, theme: &Theme, w: u32, h: u32) -> Vec<u8> {
    let mut cx = RenderContext::new();
    let id = pollster::block_on(cx.device(None)).expect("these tests need a GPU");
    let (device, queue) = (&cx.devices[id].device, &cx.devices[id].queue);
    let mut painter = Painter::new(device).expect("painter");
    let built = build_scene(&mut Fonts::new(), slide, theme, w, h);
    let texture = target_texture(device, w, h);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    painter.paint(device, queue, &built.scene, &view, w, h).expect("paint");
    read_pixels(device, queue, &texture).expect("read back")
}

fn slide(primary: &str, translation: Option<&str>) -> Slide {
    let mut boxes = vec![Object::text(Role::primary(), primary)];
    if let Some(t) = translation {
        boxes.push(Object::text(Role::translation(), t));
    }
    Slide::new(boxes)
}

fn golden_path(name: &str) -> PathBuf {
    // Testing aid: SOFTPRESENTER_TEST_ROOT=<a copy of the repository> finds the pictures there,
    // for test programs built on one computer and run on another (Windows from the Mac).
    let crate_dir = std::env::var_os("SOFTPRESENTER_TEST_ROOT")
        .map_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")), |root| PathBuf::from(root).join("crates/sp-render"));
    crate_dir.join("tests/golden").join(format!("{name}.png"))
}

fn write_png(path: &PathBuf, pixels: &[u8], w: u32, h: u32) {
    let file = std::fs::File::create(path).expect("create png");
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header().expect("header").write_image_data(pixels).expect("data");
}

fn read_png(path: &PathBuf) -> (Vec<u8>, u32, u32) {
    let decoder = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path).expect("open png")));
    let mut reader = decoder.read_info().expect("png info");
    let mut data = vec![0; reader.output_buffer_size().expect("png size")];
    let info = reader.next_frame(&mut data).expect("png frame");
    data.truncate(info.buffer_size());
    (data, info.width, info.height)
}

/// Passes when the pictures are nearly the same: small average difference and few pixels
/// that differ a lot.
fn assert_matches_golden(name: &str, pixels: &[u8], w: u32, h: u32) {
    let path = golden_path(name);
    if std::env::var_os("PC_UPDATE_GOLDEN").is_some() {
        write_png(&path, pixels, w, h);
        eprintln!("wrote {}", path.display());
        return;
    }
    assert!(
        path.exists(),
        "{name}: no stored picture at {}; run with PC_UPDATE_GOLDEN=1 and check it by eye",
        path.display()
    );
    let (expected, ew, eh) = read_png(&path);
    assert_eq!((ew, eh), (w, h), "{name}: size changed");
    assert_eq!(expected.len(), pixels.len(), "{name}: byte count changed");
    let mut total = 0u64;
    let mut far = 0usize;
    for (a, b) in expected.chunks(4).zip(pixels.chunks(4)) {
        let mut worst = 0u8;
        for (x, y) in a.iter().zip(b) {
            let d = x.abs_diff(*y);
            total += u64::from(d);
            worst = worst.max(d);
        }
        if worst > 32 {
            far += 1;
        }
    }
    let mean = total as f64 / pixels.len() as f64;
    let far_share = far as f64 / (pixels.len() / 4) as f64;
    // 0.05% of a 1080p frame is about a thousand pixels: a missing letter fails, GPU noise does not.
    if mean >= 1.0 || far_share >= 0.0005 {
        let actual = path.with_extension("actual.png");
        write_png(&actual, pixels, w, h);
        panic!(
            "{name}: differs from the stored picture (mean {mean:.2}, {:.2}% far off); see {}",
            far_share * 100.0,
            actual.display()
        );
    }
}

fn pixel(pixels: &[u8], w: u32, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * w + x) * 4) as usize;
    [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
}

fn count_lit(pixels: &[u8]) -> usize {
    pixels.chunks(4).filter(|p| p[0] > 200 && p[1] > 200 && p[2] > 200).count()
}

#[test]
fn bilingual_1920x1080() {
    let s = slide(
        "Light rises over the quiet hills\nAnd the morning sings again",
        Some("Свет встаёт над тихими холмами\nІ ранок знову співає: їжак, ґанок, є"),
    );
    let px = render(Some(&s), 1920, 1080);
    // White main-language text and orange translation text are both really there.
    assert!(count_lit(&px) > 20_000, "expected white text");
    let orange = px.chunks(4).filter(|p| p[0] > 230 && (120..180).contains(&p[1]) && p[2] < 80).count();
    assert!(orange > 10_000, "expected orange text, found {orange} pixels");
    assert_matches_golden("bilingual_1920x1080", &px, 1920, 1080);
}

#[test]
fn wrapping_long_line() {
    let line = "The river runs and the fields are wide and the road goes on beyond the last low wall ".repeat(2);
    let px = render(Some(&slide(line.trim(), None)), 1920, 1080);
    // Wrapped text never reaches the left or right edge of the slide.
    for y in (0..1080).step_by(7) {
        assert!(
            pixel(&px, 1920, 4, y)[0] < 40 && pixel(&px, 1920, 1915, y)[0] < 40,
            "text reached the edge at row {y}"
        );
    }
    assert_matches_golden("wrapping_long_line", &px, 1920, 1080);
}

#[test]
fn shrink_to_fit() {
    let text = ["A line of plain words"; 9].join("\n");
    let px = render(Some(&slide(&text, None)), 1920, 1080);
    // Shrunk to fit: nothing is drawn in the top or bottom 40 rows.
    for y in (0..40).chain(1040..1080) {
        for x in (0..1920).step_by(5) {
            assert!(pixel(&px, 1920, x, y)[0] < 40, "text spilled to row {y}");
        }
    }
    assert_matches_golden("shrink_to_fit", &px, 1920, 1080);
}

#[test]
fn solo_primary_centred() {
    let px = render(Some(&slide("One line alone", None)), 1920, 1080);
    let rows: Vec<u32> = (0..1080).filter(|y| (0..1920).any(|x| pixel(&px, 1920, x, *y)[0] > 200)).collect();
    let middle = (rows.first().expect("text") + rows.last().expect("text")) / 2;
    assert!((500..580).contains(&middle), "a lone box should sit mid-slide, found row {middle}");
    assert_matches_golden("solo_primary_centred", &px, 1920, 1080);
}

#[test]
fn letterbox_4_3() {
    let s = slide("Light rises", Some("Свет встаёт"));
    let px = render(Some(&s), 1600, 1200);
    for y in (0..150).chain(1050..1200) {
        for x in (0..1600).step_by(3) {
            assert_eq!(pixel(&px, 1600, x, y), [0, 0, 0, 255], "bar not black at {x},{y}");
        }
    }
    assert!(count_lit(&px) > 1_000);
    assert_matches_golden("letterbox_4_3", &px, 1600, 1200);
}

/// The built-in theme's outline and shadow are black on black, so they cannot be seen in the
/// other pictures. Here the outline is green and the shadow is switched off and on.
#[test]
fn outline_and_shadow_are_drawn() {
    use sp_model::Rgba;
    let s = slide("OUTLINE", None);
    let mut theme = Theme::builtin();
    for style in theme.templates.iter_mut().flat_map(|template| &mut template.boxes) {
        style.outline_color = Rgba::rgb(0, 255, 0);
        style.outline_width = 8.0;
        style.fill = Rgba::rgb(255, 0, 0);
        style.shadow = false;
    }
    let px = render_with(Some(&s), &theme, 1920, 1080);
    let green = px.chunks(4).filter(|p| p[0] < 60 && p[1] > 200 && p[2] < 60).count();
    let red = px.chunks(4).filter(|p| p[0] > 200 && p[1] < 60 && p[2] < 60).count();
    assert!(green > 5_000, "expected a green outline, found {green} pixels");
    assert!(red > 5_000, "expected red letters, found {red} pixels");

    // Without an outline the green goes away.
    for style in theme.templates.iter_mut().flat_map(|template| &mut template.boxes) {
        style.outline_width = 0.0;
    }
    let px = render_with(Some(&s), &theme, 1920, 1080);
    assert_eq!(px.chunks(4).filter(|p| p[0] < 60 && p[1] > 200 && p[2] < 60).count(), 0);

    // The shadow is drawn underneath the letters. With white letters on black, switching it on
    // must leave exactly the same white pixels.
    for style in theme.templates.iter_mut().flat_map(|template| &mut template.boxes) {
        style.outline_color = Rgba::rgb(255, 255, 255);
        style.outline_width = 2.0;
        style.fill = Rgba::rgb(255, 255, 255);
    }
    let without = render_with(Some(&s), &theme, 1920, 1080);
    for style in theme.templates.iter_mut().flat_map(|template| &mut template.boxes) {
        style.shadow = true;
    }
    let with = render_with(Some(&s), &theme, 1920, 1080);
    assert_eq!(count_lit(&without), count_lit(&with), "a shadow must not eat into the letters");
}

#[test]
fn cleared_is_black() {
    let px = render(None, 640, 360);
    assert!(px.chunks(4).all(|p| p == [0, 0, 0, 255]));
}

#[test]
fn odd_width_reads_back_without_padding() {
    // 333 pixels wide is not a multiple of the GPU's row alignment.
    let px = render(None, 333, 77);
    assert_eq!(px.len(), 333 * 77 * 4);
}

fn render_layers(media: Option<&sp_render::Picture>, slide: Option<&Slide>, w: u32, h: u32) -> Vec<u8> {
    let mut cx = RenderContext::new();
    let id = pollster::block_on(cx.device(None)).expect("these tests need a GPU");
    let (device, queue) = (&cx.devices[id].device, &cx.devices[id].queue);
    let mut painter = Painter::new(device).expect("painter");
    let layers = sp_render::Layers {
        media_fit: sp_model::ImageFit::Fill,
        media,
        slide,
        overlay: None,
        time: 0.0,
        backdrop: None,
        mask: [0.0; 4],
        announcement: None,
    };
    let built = sp_render::build_layers(&mut Fonts::new(), &layers, &Theme::builtin(), w, h);
    let texture = target_texture(device, w, h);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    painter.paint(device, queue, &built.scene, &view, w, h).expect("paint");
    read_pixels(device, queue, &texture).expect("read back")
}

/// A picture of one flat colour, of any shape.
fn flat(width: u32, height: u32, rgb: [u8; 3]) -> sp_render::Picture {
    let rgba = [rgb[0], rgb[1], rgb[2], 255].repeat((width * height) as usize);
    sp_render::Picture::from_rgba(width, height, rgba).expect("picture")
}

fn is_about(p: [u8; 4], rgb: [u8; 3]) -> bool {
    p[0].abs_diff(rgb[0]) < 12 && p[1].abs_diff(rgb[1]) < 12 && p[2].abs_diff(rgb[2]) < 12
}

#[test]
fn media_fills_the_slide_whatever_its_shape() {
    let green = [20, 200, 60];
    for (pw, ph) in [(64, 36), (40, 40), (120, 20), (20, 90)] {
        let px = render_layers(Some(&flat(pw, ph, green)), None, 640, 360);
        for (x, y) in [(2, 2), (637, 2), (2, 357), (637, 357), (320, 180)] {
            assert!(is_about(pixel(&px, 640, x, y), green), "{pw}x{ph} picture left a gap at {x},{y}");
        }
    }
}

#[test]
fn media_stays_inside_the_slide_when_the_screen_is_another_shape() {
    let px = render_layers(Some(&flat(100, 10, [255, 0, 0])), None, 400, 300);
    // 400x300 holds a 400x225 slide with bars above and below.
    for y in [2, 30, 270, 297] {
        assert_eq!(pixel(&px, 400, 200, y), [0, 0, 0, 255], "the picture spilled into the bar at row {y}");
    }
    assert!(is_about(pixel(&px, 400, 200, 150), [255, 0, 0]));
}

#[test]
fn words_are_drawn_over_the_media() {
    let blue = [0, 0, 200];
    let s = slide("WORDS", None);
    let px = render_layers(Some(&flat(16, 9, blue)), Some(&s), 1920, 1080);
    assert!(is_about(pixel(&px, 1920, 20, 20), blue), "the corner should show the picture");
    assert!(count_lit(&px) > 3_000, "the white words should be on top of the picture");
}

#[test]
fn a_moved_object_is_drawn_where_it_was_put() {
    use sp_model::SlideRect;
    let mut s = slide("X", None);
    // Bottom-right quarter of the slide.
    s.objects[0].frame = Some(SlideRect { x: 960.0, y: 540.0, w: 960.0, h: 540.0 });
    let px = render(Some(&s), 640, 360);
    let lit = |x0: u32, y0: u32| {
        (y0..y0 + 180)
            .flat_map(|y| (x0..x0 + 320).map(move |x| (x, y)))
            .filter(|(x, y)| pixel(&px, 640, *x, *y)[0] > 200)
            .count()
    };
    assert_eq!(lit(0, 0) + lit(320, 0) + lit(0, 180), 0, "nothing should be drawn outside the object's quarter");
    assert!(lit(320, 180) > 100, "the letter should be in the bottom-right quarter");
}

#[test]
fn a_faded_object_is_dimmer_and_a_turned_one_moves_its_ink() {
    let upright = slide("IIIIIIII", None);
    let px_upright = render(Some(&upright), 640, 360);
    let mut faded = slide("IIIIIIII", None);
    faded.objects[0].opacity = 0.3;
    let px_faded = render(Some(&faded), 640, 360);
    assert!(count_lit(&px_upright) > 200);
    assert_eq!(count_lit(&px_faded), 0, "at 30% no pixel should stay bright white");
    assert!(px_faded.chunks(4).any(|p| p[0] > 40), "but the letters are still there");

    let mut turned = slide("IIIIIIII", None);
    turned.objects[0].rotation = 90.0;
    let px_turned = render(Some(&turned), 640, 360);
    // Upright, the row of letters is wide and short; turned, it is tall and narrow.
    let extent = |px: &[u8]| {
        let lit: Vec<(u32, u32)> = (0..360)
            .flat_map(|y| (0..640).map(move |x| (x, y)))
            .filter(|(x, y)| pixel(px, 640, *x, *y)[0] > 200)
            .collect();
        let xs = lit.iter().map(|p| p.0);
        let ys = lit.iter().map(|p| p.1);
        (xs.clone().max().unwrap() - xs.min().unwrap(), ys.clone().max().unwrap() - ys.min().unwrap())
    };
    let (wide, short) = extent(&px_upright);
    let (narrow, tall) = extent(&px_turned);
    assert!(wide > short && tall > narrow, "upright {wide}x{short}, turned {narrow}x{tall}");
}

#[test]
fn an_objects_own_colour_alignment_and_size_are_drawn() {
    use sp_model::{Align, ObjectKind, Rgba};
    let mut s = slide("IIII", None);
    let plain = render(Some(&s), 640, 360);
    let ObjectKind::Text { style, .. } = &mut s.objects[0].kind else { panic!("a text object") };
    style.fill = Some(Rgba::rgb(255, 0, 0));
    style.align = Some(Align::Left);
    style.outline_width = Some(0.0);
    style.shadow = Some(false);
    let px = render(Some(&s), 640, 360);
    let red: Vec<u32> = (0..360)
        .flat_map(|y| (0..640).map(move |x| (x, y)))
        .filter(|(x, y)| is_about(pixel(&px, 640, *x, *y), [255, 0, 0]))
        .map(|p| p.0)
        .collect();
    assert!(red.len() > 100, "the letters should be red");
    assert_eq!(count_lit(&px), 0, "and no longer white");
    assert!(*red.iter().max().unwrap() < 320, "left-aligned letters stay in the left half");
    assert!(count_lit(&plain) > 100, "the same slide in the theme's style is white");

    // A much smaller size draws much less ink.
    let ObjectKind::Text { style, .. } = &mut s.objects[0].kind else { panic!("a text object") };
    style.size = Some(20.0);
    let small = render(Some(&s), 640, 360);
    let small_red = small.chunks(4).filter(|p| p[0] > 200 && p[1] < 60 && p[2] < 60).count();
    assert!(small_red * 4 < red.len(), "{small_red} pixels at size 20 against {}", red.len());
}

/// A fade half-way from a black screen to a white one is mid grey; at its ends it is one
/// picture or the other.
#[test]
fn a_cross_fade_blends_two_pictures() {
    use sp_model::Rgba;
    use sp_render::cross_fade;
    let (w, h) = (64, 36);
    let mut cx = RenderContext::new();
    let id = pollster::block_on(cx.device(None)).expect("these tests need a GPU");
    let (device, queue) = (&cx.devices[id].device, &cx.devices[id].queue);
    let mut painter = Painter::new(device).expect("painter");
    let black = build_scene(&mut Fonts::new(), None, &Theme::builtin(), w, h).scene;
    let mut white_theme = Theme::builtin();
    white_theme.background = Some(Rgba::WHITE);
    let white = build_scene(&mut Fonts::new(), Some(&Slide::new(Vec::new())), &white_theme, w, h).scene;
    let mut middle_of = |amount: f32| {
        let texture = target_texture(device, w, h);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        painter.paint(device, queue, &cross_fade(&black, &white, amount, w, h), &view, w, h).expect("paint");
        let pixels = read_pixels(device, queue, &texture).expect("read back");
        let at = ((h / 2 * w + w / 2) * 4) as usize;
        pixels[at]
    };
    assert_eq!(middle_of(0.0), 0);
    assert_eq!(middle_of(1.0), 255);
    let half = middle_of(0.5);
    assert!((110..=145).contains(&half), "half-way was {half}");
}

/// Half-way through a wipe or a push from the left, the left half shows the new picture and the
/// right half the old one; half-way through a fade through black, the screen is black.
#[test]
fn transitions_put_each_picture_where_it_belongs() {
    use sp_model::{Direction, Rgba, TransitionKind};
    let (w, h) = (64, 36);
    let mut cx = RenderContext::new();
    let id = pollster::block_on(cx.device(None)).expect("these tests need a GPU");
    let (device, queue) = (&cx.devices[id].device, &cx.devices[id].queue);
    let mut painter = Painter::new(device).expect("painter");
    let black = build_scene(&mut Fonts::new(), Some(&Slide::new(Vec::new())), &Theme::builtin(), w, h).scene;
    let mut white_theme = Theme::builtin();
    white_theme.background = Some(Rgba::WHITE);
    let white = build_scene(&mut Fonts::new(), Some(&Slide::new(Vec::new())), &white_theme, w, h).scene;
    let mut shade_at = |kind: TransitionKind, amount: f32, x: u32| {
        let texture = target_texture(device, w, h);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let scene = sp_render::transition(kind, Direction::FromLeft, &black, &white, amount, w, h);
        painter.paint(device, queue, &scene, &view, w, h).expect("paint");
        let pixels = read_pixels(device, queue, &texture).expect("read back");
        pixels[((h / 2 * w + x) * 4) as usize]
    };
    for kind in [TransitionKind::Wipe, TransitionKind::Push, TransitionKind::MoveIn] {
        assert_eq!(shade_at(kind, 0.5, w / 4), 255, "{kind:?}: the left is new");
        assert_eq!(shade_at(kind, 0.5, w * 3 / 4), 0, "{kind:?}: the right is old");
    }
    assert_eq!(shade_at(TransitionKind::FadeBlack, 0.5, w / 2), 0);
    assert_eq!(shade_at(TransitionKind::FadeWhite, 0.5, w / 2), 255);
    assert_eq!(shade_at(TransitionKind::Iris, 0.0, w / 2), 0);
    assert_eq!(shade_at(TransitionKind::Iris, 1.0, 1), 255);
}

#[test]
fn every_transition_starts_on_the_old_picture_and_ends_on_the_new() {
    use sp_model::{Direction, Rgba, TransitionKind};
    let (w, h) = (160, 90);
    let mut cx = RenderContext::new();
    let id = pollster::block_on(cx.device(None)).expect("these tests need a GPU");
    let (device, queue) = (&cx.devices[id].device, &cx.devices[id].queue);
    let mut painter = Painter::new(device).expect("painter");
    let black = build_scene(&mut Fonts::new(), Some(&Slide::new(Vec::new())), &Theme::builtin(), w, h).scene;
    let mut white_theme = Theme::builtin();
    white_theme.background = Some(Rgba::WHITE);
    let white = build_scene(&mut Fonts::new(), Some(&Slide::new(Vec::new())), &white_theme, w, h).scene;
    // Points in from the edges: the middle and near each corner.
    let points = [(w / 2, h / 2), (w / 8, h / 8), (w * 7 / 8, h / 8), (w / 8, h * 7 / 8), (w * 7 / 8, h * 7 / 8)];
    for direction in [Direction::FromLeft, Direction::FromTop] {
        for kind in TransitionKind::ALL.into_iter().filter(|kind| *kind != TransitionKind::Cut) {
            for (amount, want) in [(0.0, 0), (1.0, 255)] {
                let texture = target_texture(device, w, h);
                let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                let scene = sp_render::transition(kind, direction, &black, &white, amount, w, h);
                painter.paint(device, queue, &scene, &view, w, h).expect("paint");
                let pixels = read_pixels(device, queue, &texture).expect("read back");
                for (x, y) in points {
                    let red = pixels[((y * w + x) * 4) as usize];
                    assert_eq!(red, want, "{kind:?} {direction:?} at {amount}: ({x}, {y})");
                }
            }
        }
    }
}

/// A wide red picture for any file but "missing.png", which can't be read.
fn wide_red(file: &str) -> Option<sp_render::Picture> {
    (file != "missing.png").then(|| flat(200, 100, [230, 30, 30]))
}

#[test]
fn a_picture_object_sits_in_its_place_as_its_fit_says() {
    use sp_model::{ImageFit, Object, ObjectKind, SlideRect};
    sp_render::set_picture_loader(wide_red);
    let (w, h) = (192, 108);
    // A square place in the middle of the slide; the picture is twice as wide as it is tall.
    let place = SlideRect { x: 660.0, y: 240.0, w: 600.0, h: 600.0 };
    let draw = |file: &str, fit: ImageFit| {
        let mut object = Object::image(file.to_owned(), "Picture".to_owned(), 200, 100);
        object.frame = Some(place);
        object.kind = ObjectKind::Image { file: file.to_owned(), fit };
        render_layers(None, Some(&Slide::new(vec![object])), w, h)
    };
    // The square's middle, a point near its top (above a fitted picture), and outside it.
    let (middle, near_top, outside) = ((96, 54), (96, 28), (40, 54));
    let red = [230, 30, 30];
    let fit = draw("logo.png", ImageFit::Fit);
    assert!(is_about(pixel(&fit, w, middle.0, middle.1), red));
    assert!(is_about(pixel(&fit, w, near_top.0, near_top.1), [0, 0, 0]), "a fitted picture leaves room above");
    assert!(is_about(pixel(&fit, w, outside.0, outside.1), [0, 0, 0]));
    for filled in [ImageFit::Fill, ImageFit::Stretch] {
        let pixels = draw("logo.png", filled);
        assert!(is_about(pixel(&pixels, w, near_top.0, near_top.1), red), "{filled:?} fills the place");
        assert!(is_about(pixel(&pixels, w, outside.0, outside.1), [0, 0, 0]), "{filled:?} stays inside it");
    }
    // A picture that can't be read is a grey box, not nothing.
    let missing = draw("missing.png", ImageFit::Fit);
    let grey = pixel(&missing, w, near_top.0, near_top.1);
    assert!(grey[0] > 60 && grey[0].abs_diff(grey[2]) < 20, "{grey:?}");
}

#[test]
fn a_text_background_is_drawn_behind_the_lines_or_across_the_box() {
    use sp_model::{LineFill, ObjectKind, Rgba};
    let blue = [20, 40, 220];
    let band = |full_width: bool| {
        let mut s = slide("II", None);
        let ObjectKind::Text { style, .. } = &mut s.objects[0].kind else { panic!("a text object") };
        style.line_fill = Some(LineFill { on: true, color: Rgba::rgb(blue[0], blue[1], blue[2]), full_width });
        style.shadow = Some(false);
        render(Some(&s), 640, 360)
    };
    let plain = render(Some(&slide("II", None)), 640, 360);
    let blue_count = |px: &[u8]| px.chunks(4).filter(|p| is_about([p[0], p[1], p[2], p[3]], blue)).count();
    assert_eq!(blue_count(&plain), 0);
    let lines_only = band(false);
    let full = band(true);
    assert!(blue_count(&lines_only) > 200, "a band sits behind the short line");
    assert!(blue_count(&full) > blue_count(&lines_only) * 3, "across the box is much wider");
}

#[test]
fn scrolling_text_starts_in_its_box_and_moves_with_time() {
    use sp_model::{ObjectKind, Scroll};
    let mut s = slide("IIIIIIII", None);
    let ObjectKind::Text { style, .. } = &mut s.objects[0].kind else { panic!("a text object") };
    style.scroll = Some(Scroll::Left);
    style.shadow = Some(false);
    style.outline_width = Some(0.0);
    let draw = |time: f32| {
        let mut cx = RenderContext::new();
        let id = pollster::block_on(cx.device(None)).expect("these tests need a GPU");
        let (device, queue) = (&cx.devices[id].device, &cx.devices[id].queue);
        let mut painter = Painter::new(device).expect("painter");
        let layers = sp_render::Layers {
            media_fit: sp_model::ImageFit::Fill,
            media: None,
            slide: Some(&s),
            overlay: None,
            time,
            backdrop: None,
            mask: [0.0; 4],
            announcement: None,
        };
        let built = sp_render::build_layers(&mut Fonts::new(), &layers, &Theme::builtin(), 640, 360);
        let texture = target_texture(device, 640, 360);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        painter.paint(device, queue, &built.scene, &view, 640, 360).expect("paint");
        read_pixels(device, queue, &texture).expect("read back")
    };
    // Where the white letters are, across the screen.
    let lit_columns =
        |px: &[u8]| -> Vec<u32> { (0..640).filter(|x| (0..360).any(|y| pixel(px, 640, *x, y)[0] > 200)).collect() };
    let start = lit_columns(&draw(0.0));
    let later = lit_columns(&draw(1.0));
    assert!(!start.is_empty() && !later.is_empty());
    assert!(later[0] < start[0], "moving left: {} then {}", start[0], later[0]);
    assert!(sp_render::scrolls(&s, &Theme::builtin()));
    assert!(!sp_render::scrolls(&slide("Still", None), &Theme::builtin()));
}

#[test]
fn a_themes_picture_is_behind_its_slides_until_media_shows() {
    sp_render::set_picture_loader(wide_red);
    let mut theme = Theme::builtin();
    theme.background_picture = Some("church.png".to_owned());
    let draw = |media: Option<&sp_render::Picture>| {
        let mut cx = RenderContext::new();
        let id = pollster::block_on(cx.device(None)).expect("these tests need a GPU");
        let (device, queue) = (&cx.devices[id].device, &cx.devices[id].queue);
        let mut painter = Painter::new(device).expect("painter");
        let empty = Slide::new(Vec::new());
        let layers = sp_render::Layers {
            media_fit: sp_model::ImageFit::Fill,
            media,
            slide: Some(&empty),
            overlay: None,
            time: 0.0,
            backdrop: None,
            mask: [0.0; 4],
            announcement: None,
        };
        let built = sp_render::build_layers(&mut Fonts::new(), &layers, &theme, 160, 90);
        let texture = target_texture(device, 160, 90);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        painter.paint(device, queue, &built.scene, &view, 160, 90).expect("paint");
        read_pixels(device, queue, &texture).expect("read back")
    };
    assert!(is_about(pixel(&draw(None), 160, 80, 45), [230, 30, 30]), "the theme's picture fills the slide");
    let green = flat(16, 9, [30, 200, 60]);
    assert!(is_about(pixel(&draw(Some(&green)), 160, 80, 45), [30, 200, 60]), "media covers it");
}

#[test]
fn a_screens_colour_fills_the_empty_room_and_its_covered_edges() {
    use sp_model::Rgba;
    let mut cx = RenderContext::new();
    let id = pollster::block_on(cx.device(None)).expect("these tests need a GPU");
    let (device, queue) = (&cx.devices[id].device, &cx.devices[id].queue);
    let mut painter = Painter::new(device).expect("painter");
    let white = flat(16, 9, [255, 255, 255]);
    let teal = Rgba::rgb(0, 128, 128);
    // A 4:3 target: the slide is letterboxed, with room above and below it.
    let layers = sp_render::Layers {
        media_fit: sp_model::ImageFit::Fill,
        media: Some(&white),
        slide: None,
        overlay: None,
        time: 0.0,
        backdrop: Some(teal),
        mask: [480.0, 0.0, 0.0, 0.0],
        announcement: None,
    };
    let built = sp_render::build_layers(&mut Fonts::new(), &layers, &Theme::builtin(), 160, 120);
    let texture = target_texture(device, 160, 120);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    painter.paint(device, queue, &built.scene, &view, 160, 120).expect("paint");
    let px = read_pixels(device, queue, &texture).expect("read back");
    assert!(is_about(pixel(&px, 160, 80, 4), [0, 128, 128]), "the room above the slide is the screen's colour");
    assert!(is_about(pixel(&px, 160, 10, 60), [0, 128, 128]), "the covered left edge is too");
    assert!(is_about(pixel(&px, 160, 120, 60), [255, 255, 255]), "the rest of the slide shows");
}

/// Writes each transition 40% of the way through, from a red slide to a blue one, to the folder
/// SHEET_DIR names, to look at by hand:
/// `SHEET_DIR=/tmp/sheet cargo test -p sp-render --test golden transition_sheet -- --ignored`.
#[test]
#[ignore = "writes files, for looking at by hand"]
fn transition_sheet() {
    use sp_model::{Direction, Rgba, TransitionKind};
    let Ok(dir) = std::env::var("SHEET_DIR") else { return };
    let (w, h) = (320, 180);
    let mut cx = RenderContext::new();
    let id = pollster::block_on(cx.device(None)).expect("these tests need a GPU");
    let (device, queue) = (&cx.devices[id].device, &cx.devices[id].queue);
    let mut painter = Painter::new(device).expect("painter");
    let coloured = |rgba: Rgba, words: &str| {
        let mut theme = Theme::builtin();
        theme.background = Some(rgba);
        build_scene(&mut Fonts::new(), Some(&slide(words, None)), &theme, w, h).scene
    };
    let from = coloured(Rgba { r: 200, g: 40, b: 40, a: 255 }, "Old");
    let to = coloured(Rgba { r: 40, g: 80, b: 220, a: 255 }, "New");
    for kind in TransitionKind::ALL {
        let texture = target_texture(device, w, h);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let scene = sp_render::transition(kind, Direction::FromLeft, &from, &to, 0.4, w, h);
        painter.paint(device, queue, &scene, &view, w, h).expect("paint");
        let pixels = read_pixels(device, queue, &texture).expect("read back");
        write_png(&PathBuf::from(&dir).join(format!("{kind:?}.png")), &pixels, w, h);
    }
}

/// The lowest and highest rows with white (text) pixels, or `None` when there are none.
fn lit_rows(pixels: &[u8], w: u32, h: u32) -> Option<(u32, u32)> {
    let lit = |y: u32| (0..w).any(|x| pixel(pixels, w, x, y)[0] > 200 && pixel(pixels, w, x, y)[2] > 200);
    let top = (0..h).find(|y| lit(*y))?;
    let bottom = (0..h).rev().find(|y| lit(*y))?;
    Some((top, bottom))
}

#[test]
fn paragraph_spacing_baseline_and_paragraph_builds_move_and_hide_lines() {
    use sp_model::{SlideRect, TextStyle, VAlign};
    let (w, h) = (480, 270);
    let with = |style: TextStyle, shown: Option<usize>| {
        let mut s = slide("One\nTwo", None);
        s.objects[0].frame = Some(SlideRect { x: 0.0, y: 0.0, w: 1920.0, h: 1080.0 });
        if let ObjectKind::Text { style: own, .. } = &mut s.objects[0].kind {
            *own = TextStyle { valign: Some(VAlign::Top), size: Some(120.0), ..style };
        }
        s.objects[0].shown_paragraphs = shown;
        lit_rows(&render(Some(&s), w, h), w, h)
    };
    let (top, bottom) = with(TextStyle::default(), None).expect("text is drawn");
    // 200 points of paragraph spacing push the second line about 50 pixels lower at a quarter size.
    let (spaced_top, spaced_bottom) =
        with(TextStyle { paragraph_spacing: Some(200.0), ..TextStyle::default() }, None).expect("drawn");
    assert_eq!(spaced_top, top, "the first line stays put");
    assert!((spaced_bottom as i32 - bottom as i32 - 50).abs() <= 3, "{bottom} -> {spaced_bottom}");
    // A baseline of 40 points raises the whole block by about 10 pixels.
    let (raised_top, raised_bottom) =
        with(TextStyle { baseline: Some(40.0), ..TextStyle::default() }, None).expect("drawn");
    assert!((top as i32 - raised_top as i32 - 10).abs() <= 2 && (bottom as i32 - raised_bottom as i32 - 10).abs() <= 2);
    // Only the first paragraph of two shows while a build has reached one.
    let (_, first_only_bottom) = with(TextStyle::default(), Some(1)).expect("drawn");
    assert!(first_only_bottom < (top + bottom) / 2 + 10, "{first_only_bottom} vs {top}..{bottom}");
}

#[test]
fn a_gradient_fill_runs_from_one_colour_to_the_other() {
    use sp_model::{Gradient, GradientKind, Rgba, Shape, SlideRect};
    let (w, h) = (192, 108);
    let mut object = Object::shape(Shape::Rectangle);
    object.frame = Some(SlideRect { x: 0.0, y: 0.0, w: 1920.0, h: 1080.0 });
    object.fill = Some(Rgba { r: 255, g: 0, b: 0, a: 255 });
    object.gradient =
        Some(Gradient { kind: GradientKind::Linear, to: Rgba { r: 0, g: 0, b: 255, a: 255 }, angle: 0.0 });
    let px = render(Some(&Slide::new(vec![object])), w, h);
    let (left, right) = (pixel(&px, w, 4, h / 2), pixel(&px, w, w - 5, h / 2));
    assert!(left[0] > 200 && left[2] < 60, "the left is red: {left:?}");
    assert!(right[2] > 200 && right[0] < 60, "the right is blue: {right:?}");
}

/// A blank drawing between two drawings of the same picture must not lose the picture (vello
/// 0.12 threw its picture atlas away on a scene of plain fills only).
#[test]
fn a_picture_still_shows_after_a_blank_drawing() {
    let mut cx = RenderContext::new();
    let id = pollster::block_on(cx.device(None)).expect("these tests need a GPU");
    let (device, queue) = (&cx.devices[id].device, &cx.devices[id].queue);
    let mut painter = Painter::new(device).expect("painter");
    let photo = flat(1600, 1200, [230, 30, 30]);
    let mut draw = |media: Option<&sp_render::Picture>| {
        let layers = sp_render::Layers { media_fit: sp_model::ImageFit::Fill, media, ..Default::default() };
        let built = sp_render::build_layers(&mut Fonts::new(), &layers, &Theme::builtin(), 160, 90);
        let texture = target_texture(device, 160, 90);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        painter.paint(device, queue, &built.scene, &view, 160, 90).expect("paint");
        pixel(&read_pixels(device, queue, &texture).expect("read back"), 160, 80, 45)
    };
    assert!(is_about(draw(Some(&photo)), [230, 30, 30]));
    assert!(is_about(draw(None), [0, 0, 0]), "the blank drawing is black");
    assert!(is_about(draw(Some(&photo)), [230, 30, 30]), "the picture is still there");
}

/// A theme's layout can hold shapes (and pictures and videos): they are drawn behind the words
/// of every slide that uses the layout, and the slide the theme editor works on, which holds
/// them itself, draws each once.
#[test]
fn a_layouts_shapes_are_drawn_behind_the_words_of_its_slides() {
    let (w, h) = (320, 180);
    let mut theme = Theme::builtin();
    // A green bar across the bottom of every layout.
    let mut bar = Object::shape(sp_model::Shape::Rectangle);
    bar.frame = Some(sp_model::SlideRect { x: 0.0, y: 900.0, w: 1920.0, h: 180.0 });
    bar.fill = Some(sp_model::Rgba::rgb(0, 200, 0));
    for template in &mut theme.templates {
        template.objects = vec![bar.clone()];
    }
    let at = |pixels: &[u8], x: u32, y: u32| {
        let i = ((y * w + x) * 4) as usize;
        (pixels[i], pixels[i + 1], pixels[i + 2])
    };
    let song = slide("Words", None);
    let drawn = render_with(Some(&song), &theme, w, h);
    let (r, g, b) = at(&drawn, 10, h - 8);
    assert!(g > 150 && r < 60 && b < 60, "the layout's bar shows under a song's slide: {:?}", (r, g, b));
    let (r, g, b) = at(&drawn, 10, 8);
    assert!(r < 30 && g < 30 && b < 30, "and nowhere else");
    // The theme editor's slide is the layout itself, its bar among its objects.
    let editing = theme.templates[0].as_slide();
    assert_eq!(editing.objects.iter().filter(|o| o.id == bar.id).count(), 1);
    assert!(sp_render::layout_objects(&editing, &theme).is_none(), "drawn once, as the slide's own");
    // Saving what the editor made keeps the bar.
    let saved = theme.templates[0].with_slide(&editing);
    assert_eq!(saved.objects, vec![bar]);
    assert_eq!(saved.boxes, theme.templates[0].boxes);
}

/// The scene of a picture behind an empty slide, filling it.
fn media_scene(media: &sp_render::Picture, w: u32, h: u32) -> sp_render::vello::Scene {
    let layers = sp_render::Layers {
        media_fit: sp_model::ImageFit::Fill,
        media: Some(media),
        slide: None,
        overlay: None,
        time: 0.0,
        backdrop: None,
        mask: [0.0; 4],
        announcement: None,
    };
    sp_render::build_layers(&mut Fonts::new(), &layers, &Theme::builtin(), w, h).scene
}

/// Rows of a picture eight pixels by four, its left half one colour and its right half another,
/// each pixel's four bytes as given, with four bytes of padding after each row.
fn two_halves(left: [u8; 4], right: [u8; 4]) -> (Vec<u8>, usize) {
    let row: Vec<u8> = [left.repeat(4), right.repeat(4), vec![0xEE; 4]].concat();
    (row.repeat(4), 36)
}

/// A playing video's pixels go to the GPU as the system's player decodes them: blue first, in
/// rows with padding. They are drawn in their true colours; pixels sent later take their place;
/// and a scene kept after the video has gone still shows its last picture.
#[test]
fn a_moving_picture_shows_the_pixels_sent_last_in_their_true_colours() {
    use sp_render::PixelOrder;
    let (w, h) = (64, 36);
    let mut cx = RenderContext::new();
    let id = pollster::block_on(cx.device(None)).expect("these tests need a GPU");
    let (device, queue) = (&cx.devices[id].device, &cx.devices[id].queue);
    let mut painter = Painter::new(device).expect("painter");
    let texture = target_texture(device, w, h);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let paint = |painter: &mut Painter, scene: &sp_render::vello::Scene| {
        painter.paint(device, queue, scene, &view, w, h).expect("paint");
        let pixels = read_pixels(device, queue, &texture).expect("read back");
        (pixel(&pixels, w, 8, 18), pixel(&pixels, w, 56, 18))
    };
    let (red, blue, green) = ([200, 40, 10], [10, 40, 220], [20, 200, 60]);
    let bgra = |rgb: [u8; 3]| [rgb[2], rgb[1], rgb[0], 255];

    let moving = painter.moving_picture(device, 8, 4, PixelOrder::Bgra).expect("moving picture");
    assert!(moving.fits(8, 4, PixelOrder::Bgra));
    assert!(!moving.fits(8, 4, PixelOrder::Rgba) && !moving.fits(8, 2, PixelOrder::Bgra));
    let (rows, stride) = two_halves(bgra(red), bgra(blue));
    assert!(!painter.show_rows(queue, &moving, &rows[..rows.len() - 8], stride), "too few bytes");
    assert!(painter.show_rows(queue, &moving, &rows, stride));
    let scene = media_scene(&moving.picture(), w, h);
    let (left, right) = paint(&mut painter, &scene);
    assert!(is_about(left, red) && is_about(right, blue), "first picture: {left:?} {right:?}");

    // New pixels, the same picture: a scene built before they came shows them too.
    let (rows, stride) = two_halves(bgra(green), bgra(red));
    assert!(painter.show_rows(queue, &moving, &rows, stride));
    let (left, right) = paint(&mut painter, &scene);
    assert!(is_about(left, green) && is_about(right, red), "second picture: {left:?} {right:?}");

    // A scene of plain colour in between (the renderer's own picture texture is thrown away
    // by such a scene) doesn't leave the video black.
    let plain = build_scene(&mut Fonts::new(), None, &Theme::builtin(), w, h).scene;
    let (left, _) = paint(&mut painter, &plain);
    assert_eq!(left, [0, 0, 0, 255]);
    let (left, right) = paint(&mut painter, &scene);
    assert!(is_about(left, green) && is_about(right, red), "after a plain scene: {left:?} {right:?}");

    // The video goes, but a scene drawn with it is kept (a fade starts from it): it shows
    // the last picture for as long as it is kept, with other scenes drawn in between.
    drop(moving);
    for _ in 0..3 {
        let (left, right) = paint(&mut painter, &scene);
        assert!(is_about(left, green) && is_about(right, red), "kept scene: {left:?} {right:?}");
        paint(&mut painter, &plain);
    }
    assert_eq!(painter.moving_pictures(), 1, "its texture stays while a scene shows it");
    // Once nothing shows it, its texture is let go.
    drop(scene);
    for _ in 0..3 {
        paint(&mut painter, &plain);
    }
    assert_eq!(painter.moving_pictures(), 0, "and goes when nothing does");
}

/// A moving picture made and let go without ever being painted leaves nothing behind, and a
/// scene drawn with a picture whose texture has gone (which the app never keeps) is drawn
/// clear, not as a failure.
#[test]
fn a_moving_picture_let_go_leaves_no_texture_behind() {
    use sp_render::PixelOrder;
    let (w, h) = (64, 36);
    let mut cx = RenderContext::new();
    let id = pollster::block_on(cx.device(None)).expect("these tests need a GPU");
    let (device, queue) = (&cx.devices[id].device, &cx.devices[id].queue);
    let mut painter = Painter::new(device).expect("painter");
    let texture = target_texture(device, w, h);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let plain = build_scene(&mut Fonts::new(), None, &Theme::builtin(), w, h).scene;

    let never_painted = painter.moving_picture(device, 8, 4, PixelOrder::Bgra).expect("moving picture");
    assert_eq!(painter.moving_pictures(), 1);
    painter.paint(device, queue, &plain, &view, w, h).expect("paint");
    assert_eq!(painter.moving_pictures(), 1, "kept while more pixels may come");
    let picture = never_painted.picture();
    drop(never_painted);
    painter.paint(device, queue, &plain, &view, w, h).expect("paint");
    assert_eq!(painter.moving_pictures(), 1, "and while its picture is held");
    let late = media_scene(&picture, w, h);
    drop(picture);
    drop(late);
    painter.paint(device, queue, &plain, &view, w, h).expect("paint");
    assert_eq!(painter.moving_pictures(), 0);

    // Another painter (the operator's window has its own) draws a moving picture clear.
    let moving = painter.moving_picture(device, 8, 4, PixelOrder::Bgra).expect("moving picture");
    let (rows, stride) = two_halves([10, 40, 200, 255], [10, 40, 200, 255]);
    assert!(painter.show_rows(queue, &moving, &rows, stride));
    let mut other = Painter::new(device).expect("painter");
    other.paint(device, queue, &media_scene(&moving.picture(), w, h), &view, w, h).expect("paint");
    let pixels = read_pixels(device, queue, &texture).expect("read back");
    assert_eq!(pixel(&pixels, w, 8, 18), [0, 0, 0, 255]);
}

/// Two videos at once (one fading out as the next comes in) each keep their own pictures.
#[test]
fn two_moving_pictures_keep_their_own_pixels() {
    use sp_render::PixelOrder;
    let (w, h) = (64, 36);
    let mut cx = RenderContext::new();
    let id = pollster::block_on(cx.device(None)).expect("these tests need a GPU");
    let (device, queue) = (&cx.devices[id].device, &cx.devices[id].queue);
    let mut painter = Painter::new(device).expect("painter");
    let (red, blue, green) = ([200, 40, 10], [10, 40, 220], [20, 200, 60]);
    let bgra = |rgb: [u8; 3]| [rgb[2], rgb[1], rgb[0], 255];
    let old = painter.moving_picture(device, 8, 4, PixelOrder::Bgra).expect("moving picture");
    let new = painter.moving_picture(device, 8, 4, PixelOrder::Bgra).expect("moving picture");
    let (rows, stride) = two_halves(bgra(red), bgra(red));
    assert!(painter.show_rows(queue, &old, &rows, stride));
    let (rows, stride) = two_halves(bgra(blue), bgra(green));
    assert!(painter.show_rows(queue, &new, &rows, stride));
    // Half-way through a wipe from the left, the new video is on the left and the old on the right.
    let (from, to) = (media_scene(&old.picture(), w, h), media_scene(&new.picture(), w, h));
    let wipe =
        sp_render::transition(sp_model::TransitionKind::Wipe, sp_model::Direction::FromLeft, &from, &to, 0.5, w, h);
    let texture = target_texture(device, w, h);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    painter.paint(device, queue, &wipe, &view, w, h).expect("paint");
    let pixels = read_pixels(device, queue, &texture).expect("read back");
    let (left, right) = (pixel(&pixels, w, 8, 18), pixel(&pixels, w, 56, 18));
    assert!(is_about(left, blue) && is_about(right, red), "half-way: {left:?} {right:?}");
}

/// A moving picture's fourth bytes are its alpha (a web page, a video with clear parts played
/// by FFmpeg): where they are 0, what is behind it shows.
#[test]
fn a_moving_picture_with_alpha_lets_what_is_behind_it_show() {
    use sp_render::PixelOrder;
    let (w, h) = (64, 36);
    let mut cx = RenderContext::new();
    let id = pollster::block_on(cx.device(None)).expect("these tests need a GPU");
    let (device, queue) = (&cx.devices[id].device, &cx.devices[id].queue);
    let mut painter = Painter::new(device).expect("painter");
    let moving = painter.moving_picture(device, 8, 4, PixelOrder::Rgba).expect("moving picture");
    let (rows, stride) = two_halves([200, 40, 10, 255], [10, 40, 220, 0]);
    assert!(painter.show_rows(queue, &moving, &rows, stride));
    let texture = target_texture(device, w, h);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    painter.paint(device, queue, &media_scene(&moving.picture(), w, h), &view, w, h).expect("paint");
    let pixels = read_pixels(device, queue, &texture).expect("read back");
    let (left, right) = (pixel(&pixels, w, 8, 18), pixel(&pixels, w, 56, 18));
    assert!(is_about(left, [200, 40, 10]), "the opaque half: {left:?}");
    assert!(is_about(right, [0, 0, 0]), "the clear half shows the black behind: {right:?}");
}

/// A video in a box on a slide is copied as the system's player decodes it, blue first, and
/// drawn in its true colours, opaque: at its full size, and as the small copy a thumbnail draws.
#[test]
fn a_picture_copied_blue_first_is_drawn_in_its_true_colours() {
    use sp_render::{Picture, PixelOrder};
    let (red, blue) = ([200, 40, 10], [10, 40, 220]);
    let bgr0 = |rgb: [u8; 3]| [rgb[2], rgb[1], rgb[0], 0];
    let (rows, stride) = two_halves(bgr0(red), bgr0(blue));
    let picture = Picture::from_rows(8, 4, &rows, stride, PixelOrder::Bgra, true).expect("picture");
    for (name, drawn) in [("full size", picture.clone()), ("small", picture.shrunk(4))] {
        let px = render_layers(Some(&drawn), None, 64, 36);
        let (left, right) = (pixel(&px, 64, 8, 18), pixel(&px, 64, 56, 18));
        assert!(is_about(left, red) && is_about(right, blue), "{name}: {left:?} {right:?}");
    }
}

/// A long made-up reading in two languages, set small so the slide is full of letters.
fn dense_passage(seed: usize, sentences: usize) -> Slide {
    let english = [
        "The keeper of the orchard rose before the light and walked the long rows counting what the wind had left.",
        "He set his ladder by the oldest tree and spoke kindly to it, as his father had done.",
        "When the carts came up from the valley the baskets were already full and the gate stood open.",
        "No one who asked for fruit that day was sent away, and what remained was laid by for the winter.",
        "At evening the children ran between the trees and the keeper sat down, glad of his work.",
    ];
    let russian = [
        "Хранитель сада встал до рассвета и прошёл по длинным рядам, считая то, что оставил ветер.",
        "Он поставил лестницу у самого старого дерева и ласково говорил с ним, как делал его отец.",
        "Когда из долины поднялись телеги, корзины были уже полны, а ворота стояли открытыми.",
        "Никого, кто просил в тот день плодов, не отослали прочь, а остаток сберегли на зиму.",
        "Вечером дети бегали между деревьями, а хранитель сел отдохнуть, радуясь своему труду.",
    ];
    let reading = |lines: &[&str]| -> String {
        (0..sentences)
            .map(|i| format!("{} {}", i + 1 + seed, lines[(i + seed) % lines.len()]))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let mut s = slide(&reading(&english), Some(&reading(&russian)));
    for object in &mut s.objects {
        if let Some(style) = object.text_style_mut() {
            style.size = Some(30.0);
        }
    }
    s
}

/// A slide of four hundred shapes of every kind, each in its own colour, with words on top.
fn many_shapes() -> Slide {
    use sp_model::{Rgba, Shape, SlideRect};
    let kinds = [
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
    let mut s = slide("Shapes behind the words", None);
    for i in 0..400u32 {
        let (col, row) = (i % 25, i / 25);
        let mut shape = Object::shape(kinds[i as usize % kinds.len()]);
        shape.frame = Some(SlideRect { x: 10.0 + 76.0 * col as f32, y: 8.0 + 67.0 * row as f32, w: 70.0, h: 60.0 });
        shape.fill = Some(Rgba::rgb((i * 37 % 256) as u8, (i * 91 % 256) as u8, (i * 53 % 256) as u8));
        shape.rotation = (i % 7) as f32 * 9.0;
        s.objects.insert(0, shape);
    }
    s
}

/// A picture that changes from pixel to pixel, so a misplaced or missing part shows.
fn patterned(width: u32, height: u32) -> sp_render::Picture {
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let check = ((x / 40 + y / 40) % 2 * 120) as u8;
            rgba.extend_from_slice(&[(x * 255 / width) as u8, (y * 255 / height) as u8, check, 255]);
        }
    }
    sp_render::Picture::from_rgba(width, height, rgba).expect("picture")
}

/// The heaviest pictures a service draws, by name: a full reading, two of them half-way through
/// a dissolve and a wipe, a slide of many shapes, and words over a picture.
fn heavy_scenes(w: u32, h: u32, sentences: usize) -> Vec<(&'static str, sp_render::vello::Scene)> {
    use sp_model::{Direction, TransitionKind};
    let theme = Theme::builtin();
    let mut fonts = Fonts::new();
    let first = build_scene(&mut fonts, Some(&dense_passage(0, sentences)), &theme, w, h).scene;
    let second = build_scene(&mut fonts, Some(&dense_passage(3, sentences)), &theme, w, h).scene;
    let dissolve = sp_render::cross_fade(&first, &second, 0.5, w, h);
    let wipe = sp_render::transition(TransitionKind::Wipe, Direction::FromLeft, &first, &second, 0.5, w, h);
    let shapes = build_scene(&mut fonts, Some(&many_shapes()), &theme, w, h).scene;
    let photo = patterned(1920, 1080);
    let layers = sp_render::Layers {
        media_fit: sp_model::ImageFit::Fill,
        media: Some(&photo),
        slide: Some(&dense_passage(1, sentences)),
        ..Default::default()
    };
    let over_photo = sp_render::build_layers(&mut fonts, &layers, &theme, w, h).scene;
    vec![("reading", first), ("dissolve", dissolve), ("wipe", wipe), ("shapes", shapes), ("photo", over_photo)]
}

/// A painter with a GPU device, its own or one it shares with another painter as the app's two
/// drawing threads do.
struct Easel {
    device: wgpu::Device,
    queue: wgpu::Queue,
    painter: Painter,
}

impl Easel {
    fn new() -> Easel {
        let mut cx = RenderContext::new();
        let id = pollster::block_on(cx.device(None)).expect("these tests need a GPU");
        let (device, queue) = (cx.devices[id].device.clone(), cx.devices[id].queue.clone());
        let painter = Painter::new(&device).expect("painter");
        Easel { device, queue, painter }
    }

    /// Another painter drawing with the same device.
    fn sharing_the_device_of(other: &Easel) -> Easel {
        let painter = Painter::new(&other.device).expect("painter");
        Easel { device: other.device.clone(), queue: other.queue.clone(), painter }
    }

    /// A painter whose working memory stays at these sizes, as the renderer's did before it
    /// was sized from the picture.
    fn kept_at(sizes: ScratchSizes) -> Easel {
        let mut easel = Easel::new();
        easel.painter.keep_scratch_at(Some(sizes));
        easel
    }

    fn paint(&mut self, scene: &sp_render::vello::Scene, w: u32, h: u32) -> Result<Vec<u8>, String> {
        let texture = target_texture(&self.device, w, h);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.painter.paint(&self.device, &self.queue, scene, &view, w, h)?;
        read_pixels(&self.device, &self.queue, &texture)
    }

    /// Lets the painter give memory back, and waits until the GPU has let it go.
    fn rest(&mut self) {
        let started = std::time::Instant::now();
        while !self.painter.rest(&self.device, &self.queue) {
            assert!(started.elapsed() < std::time::Duration::from_secs(5), "the GPU lets the memory go");
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
}

/// Passes when two drawings of one scene are the same picture. The GPU adds up the edges
/// crossing a pixel in whatever order its threads reach them, so a pixel may come out a step
/// or so differently from run to run: one step of 255 on up to ten thousand pixels and never
/// more than a hundredth of them, so that a small picture all one step out does not pass (two
/// readings half-way through a dissolve on a 1440p screen came out one step apart on about
/// 4 900 of them, before and after the painter's memory was sized from the picture), or
/// three steps on up to a thousandth of them. Anything missing or misplaced differs by far
/// more: a single missing letter of small text is hundreds of pixels of full difference.
fn assert_same_picture(name: &str, a: &[u8], b: &[u8]) {
    assert_eq!(a.len(), b.len(), "{name}: sizes differ");
    let mut differing = 0usize;
    let mut worst = 0u8;
    for (x, y) in a.chunks(4).zip(b.chunks(4)) {
        if x != y {
            differing += 1;
            worst = worst.max(x.iter().zip(y).map(|(p, q)| p.abs_diff(*q)).max().unwrap_or(0));
        }
    }
    let share = differing as f64 / (a.len() / 4) as f64;
    assert!(
        (worst <= 1 && differing <= 10_000 && share < 0.01) || (worst <= 3 && share < 0.001),
        "{name}: {differing} pixels differ ({:.3}%), by up to {worst} of 255",
        share * 100.0
    );
}

/// Room for the heaviest pictures in these tests, kept whatever is drawn: four times the lines
/// and segments the renderer's old fixed sizes had.
const ROOMY: ScratchSizes = ScratchSizes {
    lines: 1 << 23,
    bin_data: 1 << 18,
    tiles: 1 << 21,
    seg_counts: 1 << 23,
    segments: 1 << 23,
    blend_spill: 1 << 20,
    ptcl: 1 << 24,
};

/// Slides of a few hundred letters, shapes, and words over a picture, on a 4K screen and as
/// thumbnails: sizing the working memory from the picture draws exactly what the renderer's
/// own fixed sizes drew.
#[test]
fn pictures_are_the_same_with_working_memory_sized_from_them() {
    let mut sized = Easel::new();
    let mut fixed = Easel::kept_at(ScratchSizes::UPSTREAM);
    for (w, h) in [(3840, 2160), (480, 270)] {
        for (name, scene) in heavy_scenes(w, h, 3) {
            let ours = sized.paint(&scene, w, h).expect("paint");
            let theirs = fixed.paint(&scene, w, h).expect("paint with fixed sizes");
            assert!(count_lit(&ours) > 300, "{name} at {w}: something bright is drawn");
            assert_same_picture(&format!("{name} at {w}"), &ours, &theirs);
        }
    }
}

/// A slide full of small outlined letters makes more lines than the renderer's fixed sizes had
/// room for, and it then drew nothing, without saying so. Now the painter finds the room: the
/// reading, two of them in a dissolve and a wipe, and one over a picture are all drawn, the
/// same as by a painter given plenty of room from the start, on a 4K screen and as thumbnails.
#[test]
fn a_slide_too_heavy_for_the_old_fixed_sizes_is_drawn_in_full() {
    let mut sized = Easel::new();
    let mut roomy = Easel::kept_at(ROOMY);
    for (w, h) in [(3840, 2160), (480, 270)] {
        for (name, scene) in heavy_scenes(w, h, 14) {
            let name = format!("{name} at {w}");
            let first = sized.paint(&scene, w, h).expect("paint");
            assert!(count_lit(&first) > 1_000, "{name}: the letters are drawn");
            let reruns = sized.painter.reruns();
            let again = sized.paint(&scene, w, h).expect("paint again");
            assert_eq!(sized.painter.reruns(), reruns, "{name}: the second drawing knows the room it needs");
            assert_same_picture(&name, &first, &again);
            let reference = roomy.paint(&scene, w, h).expect("paint with plenty of room");
            assert_same_picture(&name, &first, &reference);
        }
    }
    assert!(sized.painter.reruns() > 0, "the heavy slides needed more than the resting room");

    // The old sizes cannot hold the 4K reading: that is now an error, not an empty picture.
    let mut fixed = Easel::kept_at(ScratchSizes::UPSTREAM);
    let (name, reading) = heavy_scenes(3840, 2160, 14).swap_remove(0);
    assert_eq!(name, "reading");
    assert!(fixed.paint(&reading, 3840, 2160).is_err(), "the fixed sizes are too small, and say so");
}

/// One painter draws a heavy 4K slide, then a thumbnail of a light one, then the heavy slide
/// again: each picture is right, the heavy one the same both times. When drawing stops, the
/// memory the heavy slide took is given back, and the light slide still draws after that.
#[test]
fn a_painter_goes_from_heavy_to_light_and_back_and_gives_memory_back() {
    let mut easel = Easel::new();
    let heavy = heavy_scenes(3840, 2160, 14).swap_remove(0).1;
    let light = heavy_scenes(480, 270, 3).swap_remove(4).1;
    let light_alone = Easel::new().paint(&light, 480, 270).expect("paint");

    let heavy_first = easel.paint(&heavy, 3840, 2160).expect("paint");
    assert!(count_lit(&heavy_first) > 100_000, "the 4K reading is drawn");
    let light_after_heavy = easel.paint(&light, 480, 270).expect("paint");
    assert_same_picture("light after heavy", &light_after_heavy, &light_alone);
    let heavy_again = easel.paint(&heavy, 3840, 2160).expect("paint");
    assert_same_picture("heavy again", &heavy_again, &heavy_first);

    let kept = easel.painter.spare_bytes();
    assert!(kept > 100 << 20, "the heavy slide's memory is kept while it is being drawn: {kept} bytes");
    easel.rest();
    assert_eq!(easel.painter.spare_bytes(), 0, "and given back when drawing stops");
    let reruns = easel.painter.reruns();
    let heavy_after_rest = easel.paint(&heavy, 3840, 2160).expect("paint");
    assert_eq!(easel.painter.reruns(), reruns, "the room it needs is still remembered");
    assert_same_picture("heavy after a rest", &heavy_after_rest, &heavy_first);

    // A painter that only ever drew light slides keeps well under the old fixed 165 MB.
    let mut small = Easel::new();
    small.paint(&light, 480, 270).expect("paint");
    small.rest();
    let resting = small.painter.spare_bytes();
    assert!(resting > 0 && resting < 48 << 20, "a light slide's memory stays ready, and is small: {resting} bytes");
    assert_eq!(small.painter.reruns(), 0, "a light slide fits the resting room");
}

/// Draws `count` rectangles over a scene, each covering the whole picture, each in a colour of
/// its own. Every one of them touches every tile, so a few hundred need far more room for
/// tiles and command lists than text ever does.
fn cover_many_times(scene: &mut sp_render::vello::Scene, count: u32, w: u32, h: u32) {
    use sp_render::vello::{kurbo, peniko};
    let whole = kurbo::Rect::new(0.0, 0.0, f64::from(w), f64::from(h));
    for i in 0..count {
        let colour = cover_colour(i);
        let colour = peniko::Color::from_rgb8(colour[0], colour[1], colour[2]);
        scene.fill(peniko::Fill::NonZero, kurbo::Affine::IDENTITY, colour, None, &whole);
    }
}

/// The colour of the `i`th rectangle of [`cover_many_times`].
fn cover_colour(i: u32) -> [u8; 3] {
    [(40 + i * 37 % 200) as u8, (40 + i * 91 % 200) as u8, (40 + i * 53 % 200) as u8]
}

/// Room for a picture covered a few hundred times, kept whatever is drawn: 430 MB.
const ROOM_FOR_COVERS: ScratchSizes = ScratchSizes {
    lines: 1 << 19,
    bin_data: 1 << 18,
    tiles: 1 << 24,
    seg_counts: 1 << 19,
    segments: 1 << 19,
    blend_spill: 1 << 12,
    ptcl: 1 << 26,
};

/// A picture covered three hundred times over needs many times the room a painter starts with,
/// in stage after stage. It is drawn all the same, exactly as by a painter given the room from
/// the start, and never comes back as drawn while the screen stays empty.
#[test]
fn a_picture_that_outgrows_its_room_many_times_over_is_still_drawn() {
    let (w, h) = (3840, 2160);
    let mut scene = sp_render::vello::Scene::new();
    cover_many_times(&mut scene, 300, w, h);
    let mut fresh = Easel::new();
    let drawn = fresh.paint(&scene, w, h).expect("the painter finds the room");
    let top = cover_colour(299);
    for (x, y) in [(0, 0), (w / 2, h / 2), (w - 1, h - 1), (17, h - 40)] {
        assert!(is_about(pixel(&drawn, w, x, y), top), "the top rectangle shows at {x},{y}");
    }
    assert!(fresh.painter.reruns() > 0, "it needed more than the room a painter starts with");
    let roomy = Easel::kept_at(ROOM_FOR_COVERS).paint(&scene, w, h).expect("paint with plenty of room");
    assert_same_picture("covered 300 times", &drawn, &roomy);
    // Drawn again, and again after a rest, it is the same picture.
    let again = fresh.paint(&scene, w, h).expect("paint again");
    assert_same_picture("covered 300 times, again", &again, &roomy);
    fresh.rest();
    let rested = fresh.paint(&scene, w, h).expect("paint after a rest");
    assert_same_picture("covered 300 times, after a rest", &rested, &roomy);
}

/// A picture covered twelve thousand times over needs more memory than one picture may take.
/// It is refused, every time it is asked for; it is never reported as drawn. A picture that can
/// be drawn, asked for next, is drawn in full, its photograph included.
#[test]
fn a_picture_too_heavy_to_draw_is_refused_and_the_next_one_is_drawn() {
    let (w, h) = (3840, 2160);
    let (name, over_photo) = heavy_scenes(w, h, 3).swap_remove(4);
    assert_eq!(name, "photo");
    let mut too_heavy = over_photo.clone();
    cover_many_times(&mut too_heavy, 12_000, w, h);
    let expected = Easel::new().paint(&over_photo, w, h).expect("paint");
    assert!(count_lit(&expected) > 1_000, "the words over the photograph are drawn");

    // The first thing a new painter is asked for, photograph and all.
    let mut easel = Easel::new();
    let texture = target_texture(&easel.device, w, h);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let refuse = |easel: &mut Easel| easel.painter.paint(&easel.device, &easel.queue, &too_heavy, &view, w, h);
    assert_eq!(refuse(&mut easel), Err(sp_render::PaintError::TooHeavy), "too heavy, and says so");
    // Asked for again and again, as for a slide with a video playing behind it, it is refused
    // without being sent to the GPU, and takes no memory.
    let (runs, spare) = (easel.painter.runs(), easel.painter.spare_bytes());
    let started = std::time::Instant::now();
    for attempt in 0..200 {
        assert_eq!(refuse(&mut easel), Err(sp_render::PaintError::TooHeavy), "attempt {attempt}");
    }
    assert_eq!(easel.painter.runs(), runs, "known to be too heavy: not tried again");
    assert_eq!(easel.painter.spare_bytes(), spare, "and nothing is kept for it");
    eprintln!("a refusal already known takes {:?}", started.elapsed() / 200);
    assert!(read_pixels(&easel.device, &easel.queue, &texture).expect("read back").iter().all(|byte| *byte == 0));

    let after = easel.paint(&over_photo, w, h).expect("a picture that fits is drawn after one that did not");
    assert_same_picture("after a refusal", &after, &expected);
    // The same once more, and the refusal is still remembered.
    assert_eq!(refuse(&mut easel), Err(sp_render::PaintError::TooHeavy), "still too heavy");
    assert_eq!(easel.painter.runs(), runs + 1, "only the picture that fitted was sent to the GPU");
    let after = easel.paint(&over_photo, w, h).expect("paint");
    assert_same_picture("after a second refusal", &after, &expected);
    easel.rest();
    assert!(easel.painter.spare_bytes() < 64 << 20, "a refused picture leaves little behind");
}

/// Twelve different slides drawn in turn, three times round: more than a painter keeps in mind
/// as known to fit. Each is the right picture every time.
#[test]
fn more_slides_than_a_painter_keeps_in_mind_are_each_drawn_right() {
    let (w, h) = (480, 270);
    let theme = Theme::builtin();
    let mut fonts = Fonts::new();
    let scenes: Vec<_> =
        (0..12).map(|seed| build_scene(&mut fonts, Some(&dense_passage(seed, 3)), &theme, w, h).scene).collect();
    let mut reference = Easel::kept_at(ROOMY);
    let expected: Vec<_> = scenes.iter().map(|scene| reference.paint(scene, w, h).expect("paint")).collect();
    let mut easel = Easel::new();
    for round in 0..3 {
        for (i, scene) in scenes.iter().enumerate() {
            let drawn = easel.paint(scene, w, h).expect("paint");
            assert_same_picture(&format!("slide {i}, round {round}"), &drawn, &expected[i]);
        }
    }
    assert_eq!(easel.painter.reruns(), 0, "thumbnails of short readings fit the room a painter starts with");
}

/// A scene with the size it is drawn at.
type Sized = (u32, u32, sp_render::vello::Scene);

/// The app's two drawing threads, each with a painter of its own on one shared device: one
/// draws thumbnails of light slides, the other heavy slides on 4K and 1440p screens, and both
/// ask the device to tidy up after every picture, as the app does. Two more threads make it
/// harder than the app does: a third painter drawing tiny pictures without a pause, and a
/// thread that only sends the device empty work and asks it to tidy up. No thread panics and
/// every picture looked at is the right one. More slides go round on each thread than a
/// painter keeps in mind, so every picture waits for the GPU to say whether it fitted.
fn threads_draw_with_one_device(for_how_long: std::time::Duration) {
    use std::time::{Duration, Instant};
    let theme = Theme::builtin();
    let mut fonts = Fonts::new();
    let light: Vec<Sized> = (0..10)
        .map(|seed| (480, 270, build_scene(&mut fonts, Some(&dense_passage(seed, 3)), &theme, 480, 270).scene))
        .collect();
    let heavy: Vec<Sized> = [(3840, 2160), (2560, 1440)]
        .into_iter()
        .flat_map(|(w, h)| heavy_scenes(w, h, 14).into_iter().map(move |(_, scene)| (w, h, scene)))
        .collect();
    // What each should look like, from a painter with plenty of room on a device of its own.
    let mut reference = Easel::kept_at(ROOMY);
    let mut expect = |scenes: &[Sized]| -> Vec<Vec<u8>> {
        scenes.iter().map(|(w, h, scene)| reference.paint(scene, *w, *h).expect("paint the reference")).collect()
    };
    let (light_expected, heavy_expected) = (expect(&light), expect(&heavy));

    let light_easel = Easel::new();
    let heavy_easel = Easel::sharing_the_device_of(&light_easel);
    let until = Instant::now() + for_how_long;
    let draw = |name: &'static str, mut easel: Easel, scenes: &[Sized], expected: &[Vec<u8>]| {
        let (mut drawn, mut longest) = (0usize, Duration::ZERO);
        while Instant::now() < until {
            let i = drawn % scenes.len();
            let (w, h, scene) = &scenes[i];
            let texture = target_texture(&easel.device, *w, *h);
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let started = Instant::now();
            easel.painter.paint(&easel.device, &easel.queue, scene, &view, *w, *h).expect("paint");
            longest = longest.max(started.elapsed());
            let pixels = read_pixels(&easel.device, &easel.queue, &texture).expect("read back");
            assert_same_picture(&format!("{name} picture {i}, the {drawn}th drawn"), &pixels, &expected[i]);
            let _ = easel.device.poll(wgpu::PollType::Poll);
            drawn += 1;
        }
        (drawn, longest, easel.painter.reruns())
    };
    // A third painter draws tiny pictures as fast as it can, so that some painter is waiting
    // for the GPU at every moment; every hundredth of its pictures is looked at.
    let tiny: Vec<Sized> = (0..10)
        .map(|seed| (64, 36, build_scene(&mut fonts, Some(&dense_passage(seed, 1)), &theme, 64, 36).scene))
        .collect();
    let tiny_expected = expect(&tiny);
    let mut tiny_easel = Easel::sharing_the_device_of(&light_easel);
    let scribble = move || {
        let mut drawn = 0usize;
        let texture = target_texture(&tiny_easel.device, 64, 36);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        while Instant::now() < until {
            let i = drawn % tiny.len();
            tiny_easel.painter.paint(&tiny_easel.device, &tiny_easel.queue, &tiny[i].2, &view, 64, 36).expect("paint");
            if drawn.is_multiple_of(100) {
                let pixels = read_pixels(&tiny_easel.device, &tiny_easel.queue, &texture).expect("read back");
                assert_same_picture(&format!("tiny picture {i}, the {drawn}th drawn"), &pixels, &tiny_expected[i]);
            }
            drawn += 1;
        }
        drawn
    };
    // A fourth thread does nothing but send the device empty work and ask it to tidy up, as fast
    // as it can: the device is looked at from two threads at every moment a painter waits.
    let (device, queue) = (light_easel.device.clone(), light_easel.queue.clone());
    let pester = move || {
        let mut asked = 0u64;
        while Instant::now() < until {
            queue.submit([device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default()).finish()]);
            let _ = device.poll(wgpu::PollType::Poll);
            asked += 1;
        }
        asked
    };
    let (light_done, heavy_done, scribbled, asked) = std::thread::scope(|threads| {
        let light_thread = threads.spawn(|| draw("light", light_easel, &light, &light_expected));
        let heavy_thread = threads.spawn(|| draw("heavy", heavy_easel, &heavy, &heavy_expected));
        let scribbling = threads.spawn(scribble);
        let pestering = threads.spawn(pester);
        (light_thread.join(), heavy_thread.join(), scribbling.join(), pestering.join())
    });
    let light_done = light_done.expect("the thread drawing light slides did not panic");
    let heavy_done = heavy_done.expect("the thread drawing heavy slides did not panic");
    let scribbled = scribbled.expect("the thread drawing tiny pictures did not panic");
    let asked = asked.expect("the thread pestering the device did not panic");
    eprintln!(
        "threads sharing a device for {for_how_long:?}: light drew {} (longest paint {:?}, {} run again), heavy \
         drew {} (longest paint {:?}, {} run again), {scribbled} tiny pictures were drawn, and the device was sent \
         empty work and looked at {asked} times meanwhile",
        light_done.0, light_done.1, light_done.2, heavy_done.0, heavy_done.1, heavy_done.2
    );
    assert!(light_done.0 > 5 && heavy_done.0 > 2 && scribbled > 5, "every thread kept drawing");
}

#[test]
fn threads_drawing_with_one_device_do_not_trip_each_other() {
    threads_draw_with_one_device(std::time::Duration::from_secs(3));
}

#[test]
#[ignore = "takes most of a minute: the test above, for long enough to meet timing that is rare"]
fn threads_drawing_with_one_device_do_not_trip_each_other_for_forty_seconds() {
    threads_draw_with_one_device(std::time::Duration::from_secs(40));
}

/// One painter draws tiny pictures as fast as it can while another thread sends the same device
/// empty work and asks it to tidy up, as fast as it can: tens of thousands of waits for the GPU,
/// each with the device being looked at from another thread. The painter's wait must never trip
/// the GPU library's own checks (a wait with a time limit did, and took the thread down).
fn a_painter_waits_while_the_device_is_pestered(for_how_long: std::time::Duration) {
    use std::time::Instant;
    let (w, h) = (64, 36);
    let theme = Theme::builtin();
    let mut fonts = Fonts::new();
    let scenes: Vec<_> =
        (0..10).map(|seed| build_scene(&mut fonts, Some(&dense_passage(seed, 1)), &theme, w, h).scene).collect();
    let mut reference = Easel::kept_at(ROOMY);
    let expected: Vec<_> = scenes.iter().map(|scene| reference.paint(scene, w, h).expect("paint")).collect();
    let mut easel = Easel::new();
    let (device, queue) = (easel.device.clone(), easel.queue.clone());
    let until = Instant::now() + for_how_long;
    let (drawn, asked) = std::thread::scope(|threads| {
        let pestering = threads.spawn(move || {
            let mut asked = 0u64;
            while Instant::now() < until {
                queue.submit([device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default()).finish()]);
                let _ = device.poll(wgpu::PollType::Poll);
                asked += 1;
            }
            asked
        });
        let painting = threads.spawn(move || {
            let texture = target_texture(&easel.device, w, h);
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let mut drawn = 0usize;
            while Instant::now() < until {
                let i = drawn % scenes.len();
                easel.painter.paint(&easel.device, &easel.queue, &scenes[i], &view, w, h).expect("paint");
                if drawn.is_multiple_of(500) {
                    let pixels = read_pixels(&easel.device, &easel.queue, &texture).expect("read back");
                    assert_same_picture(&format!("picture {i}, the {drawn}th drawn"), &pixels, &expected[i]);
                }
                drawn += 1;
            }
            drawn
        });
        (painting.join(), pestering.join())
    });
    let drawn = drawn.expect("the painting thread did not panic");
    let asked = asked.expect("the pestering thread did not panic");
    eprintln!("{drawn} pictures waited for in {for_how_long:?}, the device pestered {asked} times meanwhile");
    assert!(drawn > 10, "the painter kept drawing: {drawn}");
}

#[test]
fn a_painter_waiting_for_the_gpu_is_not_tripped_by_another_thread_asking_it() {
    a_painter_waits_while_the_device_is_pestered(std::time::Duration::from_secs(2));
}

#[test]
#[ignore = "takes most of a minute: the test above, for long enough to meet timing that is rare"]
fn a_painter_waiting_for_the_gpu_is_not_tripped_by_another_thread_asking_it_for_forty_seconds() {
    a_painter_waits_while_the_device_is_pestered(std::time::Duration::from_secs(40));
}

/// A GPU too slow to work a picture out in the time a painter waits (here no time at all, so
/// every picture is one): the picture is given up as worth asking for again, nothing of it is
/// drawn, and nothing is learnt from it, right or wrong. Asked for again with time to wait, the
/// same picture is drawn in full; and one that outgrows its room many times over, given up
/// part-way again and again with a little more time each try, is drawn right in the end.
#[test]
fn a_picture_the_gpu_is_too_slow_with_is_given_up_and_drawn_when_asked_again() {
    use std::time::Duration;
    let (w, h) = (3840, 2160);
    let (name, reading) = heavy_scenes(w, h, 14).swap_remove(0);
    assert_eq!(name, "reading");
    let expected = Easel::kept_at(ROOMY).paint(&reading, w, h).expect("paint with plenty of room");

    let mut easel = Easel::new();
    let texture = target_texture(&easel.device, w, h);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    easel.painter.wait_at_most(Some(Duration::ZERO));
    for attempt in 0..3 {
        let given_up = easel.painter.paint(&easel.device, &easel.queue, &reading, &view, w, h);
        let again = given_up.expect_err("no time to wait").worth_trying_again();
        assert!(again, "attempt {attempt}: too slow is not too heavy");
    }
    let untouched = read_pixels(&easel.device, &easel.queue, &texture).expect("read back");
    assert!(untouched.iter().all(|byte| *byte == 0), "nothing of a picture given up is drawn");
    // With time to wait, the picture is drawn; it was not taken for one that fits, or for one
    // too heavy.
    easel.painter.wait_at_most(Some(Duration::from_secs(5)));
    let runs = easel.painter.runs();
    let drawn = easel.paint(&reading, w, h).expect("paint, with time to wait");
    assert!(easel.painter.runs() > runs, "it was sent to the GPU and waited for");
    assert_same_picture("after being given up", &drawn, &expected);

    // A picture that needs to be worked out several times, given a little more time at each
    // try: whichever run each try is given up in, what is left behind does no harm to the next.
    let mut covered = sp_render::vello::Scene::new();
    cover_many_times(&mut covered, 300, w, h);
    let expected = Easel::kept_at(ROOM_FOR_COVERS).paint(&covered, w, h).expect("paint with plenty of room");
    let mut easel = Easel::new();
    easel.painter.wait_at_most(Some(Duration::from_micros(1)));
    let mut tries = 0;
    let drawn = loop {
        tries += 1;
        assert!(tries < 200, "with a tenth of a second and more to wait, it is drawn");
        match easel.paint(&covered, w, h) {
            Ok(drawn) => break drawn,
            Err(error) => assert_ne!(error, sp_render::PaintError::TooHeavy.to_string(), "try {tries}"),
        }
        easel.painter.wait_at_most(Some(Duration::from_millis(tries)));
    };
    assert_same_picture("after being given up between runs", &drawn, &expected);
}

/// A picture the GPU drops (as macOS drops the pictures of an app whose GPU work faulted) takes
/// its photograph with it: nothing of that picture reached the GPU. The next picture drawn with
/// the same photograph sends it again, and shows it.
#[test]
fn a_photograph_in_a_picture_the_gpu_dropped_is_sent_again() {
    let (w, h) = (1920, 1080);
    let (name, over_photo) = heavy_scenes(w, h, 3).swap_remove(4);
    assert_eq!(name, "photo");
    let expected = Easel::new().paint(&over_photo, w, h).expect("paint");
    // Dropped as the first picture a painter draws, and as a later one.
    let mut easel = Easel::new();
    for round in 0..2 {
        easel.painter.drop_pictures(1);
        let dropped = easel.paint(&over_photo, w, h);
        assert!(dropped.is_err(), "round {round}: a dropped picture is not reported as drawn");
        let drawn = easel.paint(&over_photo, w, h).expect("paint after a dropped picture");
        assert_same_picture(&format!("after a dropped picture, round {round}"), &drawn, &expected);
    }
}

/// A picture that is still asking for more room when the painter has worked it out as often as
/// it will in one go (here once, where it needs several) is given up as worth asking for again,
/// not refused as too heavy; and each time it is asked for it goes on from the room found, so
/// it is drawn within a few tries.
#[test]
fn a_picture_not_settled_in_one_go_is_not_refused_and_settles_over_a_few() {
    let (w, h) = (3840, 2160);
    let mut covered = sp_render::vello::Scene::new();
    cover_many_times(&mut covered, 300, w, h);
    let expected = Easel::kept_at(ROOM_FOR_COVERS).paint(&covered, w, h).expect("paint with plenty of room");
    let mut easel = Easel::new();
    easel.painter.work_out_at_most(Some(1));
    let texture = target_texture(&easel.device, w, h);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut tries = 0;
    loop {
        tries += 1;
        match easel.painter.paint(&easel.device, &easel.queue, &covered, &view, w, h) {
            Ok(()) => break,
            Err(error) => assert!(error.worth_trying_again(), "try {tries}: {error}"),
        }
        assert!(tries < 10, "every try gets further than the one before");
    }
    assert!(tries > 1, "it needed more than one go");
    let drawn = read_pixels(&easel.device, &easel.queue, &texture).expect("read back");
    assert_same_picture("settled over a few tries", &drawn, &expected);
}

/// A picture worked out again has already given its outgrown memory back by the time it is
/// drawn. The GPU still holds that memory until the device is looked at after the picture has
/// left it, so the painter must not say it has rested before then: not on the call that finds
/// the memory was given back, and not on the calls after it while the GPU is still busy. Here
/// the GPU is kept busy with thirty more 4K pictures, sent without waiting, each a few
/// milliseconds of work; the calls to rest follow within microseconds.
#[test]
fn a_painter_has_not_rested_while_the_gpu_holds_memory_a_picture_gave_back() {
    let (w, h) = (3840, 2160);
    let mut heavy = sp_render::vello::Scene::new();
    cover_many_times(&mut heavy, 300, w, h);
    let mut easel = Easel::new();
    let texture = target_texture(&easel.device, w, h);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    easel.painter.paint(&easel.device, &easel.queue, &heavy, &view, w, h).expect("paint");
    assert!(easel.painter.reruns() > 0, "the picture was worked out again, with more room");
    assert_eq!(easel.painter.spare_bytes(), 0, "and has given back the room it outgrew");
    // The same picture again and again: known to fit, so each is sent without a wait.
    let runs = easel.painter.runs();
    for _ in 0..30 {
        easel.painter.paint(&easel.device, &easel.queue, &heavy, &view, w, h).expect("paint");
    }
    assert_eq!(easel.painter.runs(), runs + 30);
    // The painter finds that the picture gave memory back, and asks the device to say when it
    // has left the GPU. That call never says the painter has rested, however fast the GPU is:
    // the device is first looked at on the call after.
    let rested = easel.painter.rest(&easel.device, &easel.queue);
    assert!(!rested, "the GPU may still hold what the picture gave back");
    for call in 0..3 {
        let rested = easel.painter.rest(&easel.device, &easel.queue);
        assert!(!rested, "call {call}: the GPU has thirty pictures still to draw");
    }
    easel.rest();
    assert!(easel.painter.rest(&easel.device, &easel.queue), "and then it has rested");
}

impl Easel {
    /// Paints a scene on clear, as a see-through screen is, and reads it back.
    fn paint_see_through(&mut self, scene: &sp_render::vello::Scene, w: u32, h: u32) -> (wgpu::Texture, Vec<u8>) {
        let texture = target_texture(&self.device, w, h);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.painter.paint_see_through(&self.device, &self.queue, scene, &view, w, h).expect("paint");
        let pixels = read_pixels(&self.device, &self.queue, &texture).expect("read back");
        (texture, pixels)
    }

    /// A picture painted see-through, as a window shows it through a solid pass: drawn into a
    /// texture of a window's format (here RGBA, as the screens' windows may be) and read back.
    fn shown_solid(&self, picture: &wgpu::Texture, shown: sp_render::Solid) -> Vec<u8> {
        let (w, h) = (picture.width(), picture.height());
        let window = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("a window's picture"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let pass = sp_render::SolidPass::new(&self.device, wgpu::TextureFormat::Rgba8Unorm, shown);
        assert_eq!((pass.shown(), pass.format()), (shown, wgpu::TextureFormat::Rgba8Unorm));
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        let source = picture.create_view(&wgpu::TextureViewDescriptor::default());
        let target = window.create_view(&wgpu::TextureViewDescriptor::default());
        pass.draw(&self.device, &mut encoder, &source, &target);
        self.queue.submit([encoder.finish()]);
        read_pixels(&self.device, &self.queue, &window).expect("read back")
    }
}

/// A see-through screen is painted on clear. The renderer works with premultiplied colours but
/// writes each pixel's colour as it is, with its alpha beside it (straight): a half-solid white
/// comes back white, half solid, not grey. Where nothing is drawn it is clear. So a picture
/// sent straight (NDI's own way) needs no change, and only premultiplied does.
#[test]
fn painting_on_clear_leaves_straight_colours_and_clear_where_nothing_is_drawn() {
    use sp_render::vello::kurbo::{Affine, Rect};
    use sp_render::vello::peniko::{Color, Fill};
    let (w, h) = (64, 36);
    let mut scene = sp_render::vello::Scene::new();
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(255, 255, 255, 128),
        None,
        &Rect::new(0.0, 0.0, 32.0, 36.0),
    );
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(200, 100, 50, 255),
        None,
        &Rect::new(32.0, 0.0, 48.0, 36.0),
    );
    let mut easel = Easel::new();
    let (_, pixels) = easel.paint_see_through(&scene, w, h);
    let half = pixel(&pixels, w, 10, 18);
    assert!(half[..3].iter().all(|c| *c >= 254) && half[3].abs_diff(128) <= 1, "half-solid white: {half:?}");
    assert_eq!(pixel(&pixels, w, 40, 18), [200, 100, 50, 255], "solid is as drawn");
    assert_eq!(pixel(&pixels, w, 56, 18), [0, 0, 0, 0], "nothing drawn is clear");
    // The same scene painted the usual way is on black, solid everywhere.
    let solid = easel.paint(&scene, w, h).expect("paint");
    assert_eq!(pixel(&solid, w, 56, 18), [0, 0, 0, 255]);
    assert!(pixel(&solid, w, 10, 18)[0].abs_diff(128) <= 1, "half white over black is grey");
}

/// A see-through screen's slide: the words, solid, with soft edges, and clear everywhere else.
/// Shown in a window, its fill is solid (straight: the colours as they are; premultiplied: as
/// over black), and its key is how solid each pixel of that same picture is, as a grey.
#[test]
fn a_see_through_screen_and_its_key_come_from_one_picture() {
    use sp_model::Rgba;
    use sp_render::Solid;
    let (w, h) = (480, 270);
    // As a see-through screen draws it: nothing where nothing is drawn (the screen's colour
    // clear), and the theme without its background (the built-in has none).
    let words = slide("Light rises over the quiet hills", None);
    let layers = sp_render::Layers {
        slide: Some(&words),
        backdrop: Some(Rgba { r: 0, g: 0, b: 0, a: 0 }),
        ..Default::default()
    };
    let scene = sp_render::build_layers(&mut Fonts::new(), &layers, &Theme::builtin(), w, h).scene;
    let mut easel = Easel::new();
    let (picture, fill) = easel.paint_see_through(&scene, w, h);
    let alphas: Vec<u8> = fill.chunks(4).map(|p| p[3]).collect();
    let clear = alphas.iter().filter(|a| **a == 0).count();
    let solid = alphas.iter().filter(|a| **a == 255).count();
    let between = alphas.len() - clear - solid;
    assert_eq!(pixel(&fill, w, 2, 2), [0, 0, 0, 0], "the corner is clear");
    assert!(clear > alphas.len() / 2, "mostly clear: {clear} of {}", alphas.len());
    assert!(solid > 500, "the words are solid: {solid}");
    assert!(between > 100, "with soft edges: {between}");
    // The key: each pixel's alpha as a grey, solid.
    let key = easel.shown_solid(&picture, Solid::Key);
    let (straight, premultiplied) =
        (easel.shown_solid(&picture, Solid::Straight), easel.shown_solid(&picture, Solid::Premultiplied));
    for ((fill, key), (straight, premultiplied)) in
        fill.chunks(4).zip(key.chunks(4)).zip(straight.chunks(4).zip(premultiplied.chunks(4)))
    {
        assert!(key[..3].iter().all(|grey| grey.abs_diff(fill[3]) <= 1) && key[3] == 255, "key {key:?} of {fill:?}");
        assert!(straight[..3].iter().zip(&fill[..3]).all(|(a, b)| a.abs_diff(*b) <= 1) && straight[3] == 255);
        let times = |colour: u8| (f32::from(colour) * f32::from(fill[3]) / 255.0).round() as u8;
        assert!(
            premultiplied[..3].iter().zip(&fill[..3]).all(|(a, b)| a.abs_diff(times(*b)) <= 1)
                && premultiplied[3] == 255,
            "premultiplied {premultiplied:?} of {fill:?}"
        );
    }
    // How the key and the fill look, for an eye to check.
    let shown: Vec<u8> = fill.chunks(4).flat_map(|p| [p[0], p[1], p[2], p[3]]).collect();
    assert_matches_golden("see_through_fill", &shown, w, h);
    assert_matches_golden("see_through_key", &key, w, h);
}

/// Half-way through a transition on a see-through screen, a pixel clear in both pictures is
/// still clear and one solid in both is still solid; one only in the old, or only in the new,
/// is half solid. (On a solid screen a dissolve lays the new over the old, which on clear would
/// leave the old words at full strength until the end.) A fade through black goes through
/// clear: the camera alone.
#[test]
fn a_see_through_transition_cross_fades_clear_and_solid_truly() {
    use sp_model::TransitionKind;
    use sp_render::vello::kurbo::{Affine, Rect};
    use sp_render::vello::peniko::{Color, Fill};
    let (w, h) = (64, 36);
    // The old: white on the left and in the middle; the new: white in the middle and on the right.
    let boxes = |rects: &[(f64, f64)]| {
        let mut scene = sp_render::vello::Scene::new();
        for (left, right) in rects {
            scene.fill(Fill::NonZero, Affine::IDENTITY, Color::WHITE, None, &Rect::new(*left, 0.0, *right, 36.0));
        }
        scene
    };
    let from = boxes(&[(0.0, 16.0), (24.0, 40.0)]);
    let to = boxes(&[(24.0, 40.0), (48.0, 64.0)]);
    let mut easel = Easel::new();
    let mut alpha_at = |kind: TransitionKind, amount: f32| {
        let scene = sp_render::see_through_transition(kind, &from, &to, amount, w, h);
        let (_, pixels) = easel.paint_see_through(&scene, w, h);
        [8, 20, 32, 56].map(|x| pixel(&pixels, w, x, 18)[3])
    };
    for kind in [TransitionKind::Dissolve, TransitionKind::Push, TransitionKind::Wipe, TransitionKind::Fade] {
        let [old, neither, both, new] = alpha_at(kind, 0.5);
        assert!(old.abs_diff(128) <= 2 && new.abs_diff(128) <= 2, "{kind:?}: half and half: {old} {new}");
        assert_eq!((neither, both), (0, 255), "{kind:?}: clear stays clear, solid stays solid");
        assert_eq!(alpha_at(kind, 0.0), [255, 0, 255, 0], "{kind:?} starts on the old");
        assert_eq!(alpha_at(kind, 1.0), [0, 0, 255, 255], "{kind:?} ends on the new");
    }
    assert_eq!(alpha_at(TransitionKind::FadeBlack, 0.5), [0, 0, 0, 0], "through clear, not black");
    assert_eq!(alpha_at(TransitionKind::Cut, 0.3), [0, 0, 255, 255]);
}
