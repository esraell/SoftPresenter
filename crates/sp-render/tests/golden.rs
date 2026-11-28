//! Image tests: draw through the real GPU path, read the pixels back and compare with stored
//! pictures in `tests/golden/`. Run with `PC_UPDATE_GOLDEN=1` to write the pictures; without
//! it a missing picture is a failure, so a deleted file cannot pass quietly.
//! They use the bundled font alone (SoftPresenter Sans), never the computer's own, so the
//! pictures are the same on every computer. They need a GPU.

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use sp_model::{Object, ObjectKind, Role, Slide, Theme};
use sp_render::vello::ScratchSizes;
use sp_render::vello::util::RenderContext;
use sp_render::{Fonts, Painter, build_scene, read_pixels, target_texture, wgpu};

/// An instance and a device that are never destroyed, made one at a time. Tests run on many
/// threads, and a driver does not have to stand many instances and devices being made and
/// destroyed at once: on Linux with an NVIDIA card (driver 595, RTX 2080 Ti) four such tests
/// hung inside the driver with every thread waiting, and two crashed in the Vulkan loader.
/// The app makes its one device once, and shares it between its threads, as these tests do.
fn lasting_context() -> &'static RenderContext {
    static MAKING: Mutex<()> = Mutex::new(());
    let _one_at_a_time = MAKING.lock().unwrap_or_else(|e| e.into_inner());
    let mut cx = if cfg!(target_os = "linux") {
        // A test build asks Vulkan for its debug names and checks. With many threads naming
        // pipelines at once the Vulkan loader (1.4.341) crashed in vkSetDebugUtilsObjectNameEXT,
        // on one device as on many. The app's own builds do not ask for them.
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            display: None,
            backends: wgpu::Backends::from_env().unwrap_or_default(),
            flags: wgpu::InstanceFlags::empty().with_env(),
            memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
            backend_options: wgpu::BackendOptions::from_env_or_default(),
        });
        RenderContext { instance, devices: Vec::new(), memory_hints: wgpu::MemoryHints::default(), refused: Vec::new() }
    } else {
        RenderContext::new()
    };
    pollster::block_on(cx.device(None)).expect("these tests need a GPU");
    Box::leak(Box::new(cx))
}

/// The instance and device nearly every test draws with: made when the first test asks.
fn context() -> &'static RenderContext {
    static CONTEXT: OnceLock<&'static RenderContext> = OnceLock::new();
    CONTEXT.get_or_init(lasting_context)
}

fn gpu() -> (&'static wgpu::Device, &'static wgpu::Queue) {
    let handle = &context().devices[0];
    (&handle.device, &handle.queue)
}

fn render(slide: Option<&Slide>, w: u32, h: u32) -> Vec<u8> {
    render_with(slide, &Theme::builtin(), w, h)
}

fn render_with(slide: Option<&Slide>, theme: &Theme, w: u32, h: u32) -> Vec<u8> {
    let (device, queue) = gpu();
    let mut painter = Painter::new(device).expect("painter");
    let built = build_scene(&mut Fonts::bundled_only(), slide, theme, w, h);
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
    // White main-language text and stone-grey translation text (#D2CEC6) are both really there.
    assert!(count_lit(&px) > 20_000, "expected white text");
    let stone =
        px.chunks(4).filter(|p| p[0].abs_diff(210) < 6 && p[1].abs_diff(206) < 6 && p[2].abs_diff(198) < 6).count();
    assert!(stone > 10_000, "expected stone-grey text, found {stone} pixels");
    assert_matches_golden("bilingual_1920x1080", &px, 1920, 1080);
}

/// How far down a slide the end of a song's mark may reach: 3% above its bottom.
const MARK_FLOOR: f32 = 1080.0 * 0.97;

/// A song's last slide of eight lines that fill a words' box over most of the slide, with the
/// end of a song marked: the words come out a little smaller, and the three stars sit under
/// them, below the box and above the lowest 3% of the slide.
#[test]
fn end_mark_on_a_full_last_slide() {
    let mut theme = Theme::builtin();
    for template in &mut theme.templates {
        for look in &mut template.boxes {
            if look.role == Role::primary() {
                look.rect = sp_model::SlideRect { x: 80.0, y: 60.0, w: 1760.0, h: 940.0 };
            }
        }
    }
    let words = "Now the evening light is falling\nOver field and over hill\nEvery road is turning homeward\n\
Every restless wind is still\nHere we stand and sing together\nOne more song before we part\n\
Grace has brought us safe thus far\nGrace will keep us, hand and heart";
    let full = slide(words, None);
    let marked = Slide { end_mark: Some(MARK_FLOOR), ..full.clone() };
    let (plain, px) = (render_with(Some(&full), &theme, 1920, 1080), render_with(Some(&marked), &theme, 1920, 1080));
    // Nothing is lit in the lowest 3% of the slide, and with the mark there is something lit
    // under where the unmarked words end, which there is not without it.
    let lit_from = |pixels: &[u8], top: u32| {
        (top..1080)
            .flat_map(|y| (0..1920).map(move |x| (x, y)))
            .filter(|(x, y)| pixel(pixels, 1920, *x, *y)[0] > 200)
            .count()
    };
    assert_eq!(lit_from(&px, 1049), 0, "the mark stays above the lowest 3%");
    let lowest_lit = |pixels: &[u8]| (0..1080).rev().find(|y| (0..1920).any(|x| pixel(pixels, 1920, x, *y)[0] > 200));
    let (words_end, mark_ends) = (lowest_lit(&plain).expect("words"), lowest_lit(&px).expect("words and mark"));
    assert!(mark_ends > words_end && mark_ends <= 1048, "the mark ends at {mark_ends}, the words alone at {words_end}");
    assert_matches_golden("end_mark_on_a_full_last_slide", &px, 1920, 1080);
}

/// A last slide in two languages in the built-in theme, with the end of a song marked under
/// the translation.
#[test]
fn end_mark_under_two_languages() {
    let s = slide(
        "Light rises over the quiet hills\nAnd the morning sings again",
        Some("Свет встаёт над тихими холмами\nІ ранок знову співає"),
    );
    let marked = Slide { end_mark: Some(MARK_FLOOR), ..s.clone() };
    let (plain, px) = (render(Some(&s), 1920, 1080), render(Some(&marked), 1920, 1080));
    assert!(count_lit(&px) > count_lit(&plain), "the mark adds white to the slide");
    assert_matches_golden("end_mark_under_two_languages", &px, 1920, 1080);
}

/// Russian and Ukrainian (with і, ї, є and ґ) in every style of the bundled typeface, one style
/// to a line: regular, bold, italic, bold italic.
#[test]
fn cyrillic_in_every_style() {
    use sp_model::{SpanStyle, TextSpan};
    let lines = [
        "Свет встаёт над холмами. Ґанок, їжак, є і",
        "Свет встаёт над холмами. Ґанок, їжак, є і",
        "Свет встаёт над холмами. Ґанок, їжак, є і",
        "Свет встаёт над холмами. Ґанок, їжак, є і",
    ];
    let styles = [(400, false), (700, false), (400, true), (700, true)];
    let mut s = slide(&lines.join("\n"), None);
    let mut start = 0;
    let mut spans = Vec::new();
    for (line, (weight, italic)) in lines.iter().zip(styles) {
        let end = start + line.chars().count();
        let style = SpanStyle { weight: Some(weight), italic: Some(italic), ..SpanStyle::default() };
        spans.push(TextSpan::new(start, end, style));
        start = end + 1;
    }
    let ObjectKind::Text { spans: own, .. } = &mut s.objects[0].kind else { panic!("a text object") };
    *own = spans;
    let px = render(Some(&s), 1920, 1080);
    assert!(count_lit(&px) > 20_000, "expected white text");
    assert_matches_golden("cyrillic_in_every_style", &px, 1920, 1080);
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
    let (device, queue) = gpu();
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
    let built = sp_render::build_layers(&mut Fonts::bundled_only(), &layers, &Theme::builtin(), w, h);
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

/// The owner, 27 November ("somewhat transparent to see whats behind"): on the Edit view's slide
/// a web page, NDI source or AirPlay box with nothing to show yet is a dark fill at 62% (30, 32,
/// 36) with a faint hatch (white at 7%, 45°, 6% stripes every 12%), so what is behind it shows
/// dimmed; words in front of it are at full strength. On the screens it shows nothing.
#[test]
fn an_empty_display_box_shows_what_is_behind_it_dimmed_on_the_edit_canvas() {
    let (w, h) = (640, 360);
    let mut red = Object::shape(sp_model::Shape::Rectangle);
    red.frame = Some(sp_model::WHOLE_SLIDE);
    red.fill = Some(sp_model::Rgba::rgb(255, 0, 0));
    red.stroke = None;
    for boxed in [Object::web(String::new()), Object::ndi(String::new()), Object::mirror()] {
        let kind = boxed.name.clone();
        let mut words = slide("IIIIIIII", None);
        words.objects.insert(0, boxed);
        words.objects.insert(0, red.clone());
        sp_render::show_placeholders(true);
        sp_render::shade_empty_boxes(true);
        let operator = render(Some(&words), w, h);
        sp_render::shade_empty_boxes(false);
        sp_render::show_placeholders(false);
        let screens = render(Some(&words), w, h);
        // A row across the top, clear of the outline, the tag and a web page's globe.
        let row: Vec<[u8; 4]> = (100..540).map(|x| pixel(&operator, w, x, 60)).collect();
        for p in &row {
            assert!(
                (110..=131).contains(&p[0]) && (14..=42).contains(&p[1]) && (16..=44).contains(&p[2]),
                "{kind}: red behind, dimmed by the dark fill and at most the hatch over it: {p:?}"
            );
        }
        // Dark fill alone: 255 × 0.38 + 30 × 0.62 = 115.5, 32 × 0.62 = 19.8, 36 × 0.62 = 22.3.
        assert!(row.iter().any(|p| p[1] <= 23), "{kind}: the dimmed red between the stripes");
        // A stripe: that, with white at 7% over it (125, 36, 39).
        assert!(row.iter().any(|p| p[1] >= 33), "{kind}: the hatch");
        assert!(count_lit(&operator) > 200, "{kind}: the words in front at full strength");
        assert!(is_about(pixel(&screens, w, 320, 60), [255, 0, 0]), "{kind}: on the screens, nothing over the red");
    }
}

/// Elsewhere in the operator's window (a thumbnail, the playlist) an empty box keeps its old
/// look: no shade and no hatch (the owner's hand-off: see-through "in the editor").
#[test]
fn an_empty_display_box_in_a_thumbnail_has_no_shade_and_no_hatch() {
    let (w, h) = (640, 360);
    let mut red = Object::shape(sp_model::Shape::Rectangle);
    red.frame = Some(sp_model::WHOLE_SLIDE);
    red.fill = Some(sp_model::Rgba::rgb(255, 0, 0));
    red.stroke = None;
    // A web page keeps its faint blue panel; an NDI source or AirPlay box only its dashed edge.
    for (boxed, inside) in [
        (Object::web(String::new()), [162, 24, 36]),
        (Object::ndi(String::new()), [255, 0, 0]),
        (Object::mirror(), [255, 0, 0]),
    ] {
        let kind = boxed.name.clone();
        let mut words = slide("IIIIIIII", None);
        words.objects.insert(0, boxed);
        words.objects.insert(0, red.clone());
        sp_render::show_placeholders(true);
        let thumbnail = render(Some(&words), w, h);
        let row: Vec<[u8; 4]> = (100..540).map(|x| pixel(&thumbnail, w, x, 60)).collect();
        for p in &row {
            assert!(is_about(*p, inside), "{kind}: {p:?}, not {inside:?}");
        }
        let greens = row.iter().map(|p| p[1]);
        assert!(greens.clone().max().unwrap() - greens.min().unwrap() <= 2, "{kind}: no hatch");
    }
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
    let (device, queue) = gpu();
    let mut painter = Painter::new(device).expect("painter");
    let black = build_scene(&mut Fonts::bundled_only(), None, &Theme::builtin(), w, h).scene;
    let mut white_theme = Theme::builtin();
    white_theme.background = Some(Rgba::WHITE);
    let white = build_scene(&mut Fonts::bundled_only(), Some(&Slide::new(Vec::new())), &white_theme, w, h).scene;
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
    let (device, queue) = gpu();
    let mut painter = Painter::new(device).expect("painter");
    let black = build_scene(&mut Fonts::bundled_only(), Some(&Slide::new(Vec::new())), &Theme::builtin(), w, h).scene;
    let mut white_theme = Theme::builtin();
    white_theme.background = Some(Rgba::WHITE);
    let white = build_scene(&mut Fonts::bundled_only(), Some(&Slide::new(Vec::new())), &white_theme, w, h).scene;
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
    let (device, queue) = gpu();
    let mut painter = Painter::new(device).expect("painter");
    let black = build_scene(&mut Fonts::bundled_only(), Some(&Slide::new(Vec::new())), &Theme::builtin(), w, h).scene;
    let mut white_theme = Theme::builtin();
    white_theme.background = Some(Rgba::WHITE);
    let white = build_scene(&mut Fonts::bundled_only(), Some(&Slide::new(Vec::new())), &white_theme, w, h).scene;
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
        let (device, queue) = gpu();
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
        let built = sp_render::build_layers(&mut Fonts::bundled_only(), &layers, &Theme::builtin(), 640, 360);
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
        let (device, queue) = gpu();
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
        let built = sp_render::build_layers(&mut Fonts::bundled_only(), &layers, &theme, 160, 90);
        let texture = target_texture(device, 160, 90);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        painter.paint(device, queue, &built.scene, &view, 160, 90).expect("paint");
        read_pixels(device, queue, &texture).expect("read back")
    };
    assert!(is_about(pixel(&draw(None), 160, 80, 45), [230, 30, 30]), "the theme's picture fills the slide");
    let green = flat(16, 9, [30, 200, 60]);
    assert!(is_about(pixel(&draw(Some(&green)), 160, 80, 45), [30, 200, 60]), "media covers it");
}

/// Hide the Words: the slide stays on screen without its words. The theme's colour, its
/// picture and the shapes of its layout are drawn as before, and so is the slide's own picture;
/// only the words go.
#[test]
fn a_slide_without_its_words_keeps_the_themes_background_and_picture() {
    use sp_model::{Rgba, SlideRect};
    sp_render::set_picture_loader(wide_red);
    let (w, h) = (320, 180);
    let mut theme = Theme::builtin();
    theme.background = Some(Rgba::rgb(20, 40, 160));
    // A green bar across the bottom of every layout.
    let mut bar = Object::shape(sp_model::Shape::Rectangle);
    bar.frame = Some(SlideRect { x: 0.0, y: 900.0, w: 1920.0, h: 180.0 });
    bar.fill = Some(Rgba::rgb(0, 200, 0));
    for template in &mut theme.templates {
        template.objects = vec![bar.clone()];
    }
    // Words over the middle, and the slide's own picture in its top left corner.
    let mut song = slide("WWWWWWWWWWWWWWWWWWWW\nWWWWWWWWWWWWWWWWWWWW", None);
    let mut picture = Object::image("red.png".to_owned(), "Picture".to_owned(), 200, 100);
    picture.frame = Some(SlideRect { x: 0.0, y: 0.0, w: 400.0, h: 200.0 });
    song.objects.insert(0, picture);
    let with_words = render_with(Some(&song), &theme, w, h);
    assert!(count_lit(&with_words) > 100, "with its words, they are drawn: {}", count_lit(&with_words));

    let hidden = Slide { words_hidden: true, ..song.clone() };
    let drawn = render_with(Some(&hidden), &theme, w, h);
    assert_eq!(count_lit(&drawn), 0, "no words");
    assert!(is_about(pixel(&drawn, w, 300, 60), [20, 40, 160]), "the theme's colour: {:?}", pixel(&drawn, w, 300, 60));
    assert!(is_about(pixel(&drawn, w, 160, 170), [0, 200, 0]), "its layout's bar: {:?}", pixel(&drawn, w, 160, 170));
    assert!(is_about(pixel(&drawn, w, 20, 10), [230, 30, 30]), "the slide's picture: {:?}", pixel(&drawn, w, 20, 10));
    // Where the words were, the theme's colour shows through.
    assert!(is_about(pixel(&drawn, w, 160, 90), [20, 40, 160]), "{:?}", pixel(&drawn, w, 160, 90));

    // A theme with a picture behind its slides: the picture stays, without the words.
    theme.background_picture = Some("church.png".to_owned());
    let drawn = render_with(Some(&hidden), &theme, w, h);
    assert_eq!(count_lit(&drawn), 0, "no words over the theme's picture");
    assert!(is_about(pixel(&drawn, w, 300, 60), [230, 30, 30]), "the theme's picture: {:?}", pixel(&drawn, w, 300, 60));
}

/// A box fitted to its words (Container up or down) keeps the size its words give it while
/// they are hidden: its own fill is drawn as large as with them, with no words on it.
#[test]
fn a_box_fitted_to_its_words_keeps_its_size_while_they_are_hidden() {
    use sp_model::{Rgba, Scaling, SlideRect};
    let (w, h) = (320, 180);
    let mut song = slide("Light rises\nover the quiet hills\nand the morning sings", None);
    let main = &mut song.objects[0];
    main.frame = Some(SlideRect { x: 160.0, y: 40.0, w: 1600.0, h: 1000.0 });
    main.fill = Some(Rgba::rgb(0, 200, 0));
    if let ObjectKind::Text { style, .. } = &mut main.kind {
        style.scaling = Some(Scaling::ContainerUpOrDown);
    }
    // The rows the box's green fill covers, top and bottom.
    let green_rows = |pixels: &[u8]| -> Option<(u32, u32)> {
        let green = |y: u32| (0..w).any(|x| is_about(pixel(pixels, w, x, y), [0, 200, 0]));
        let rows: Vec<u32> = (0..h).filter(|y| green(*y)).collect();
        Some((*rows.first()?, *rows.last()?))
    };
    let with_words = render_with(Some(&song), &Theme::builtin(), w, h);
    let (top, bottom) = green_rows(&with_words).expect("the box's fill");
    assert!(bottom - top < 150, "fitted to three lines, not its whole place: {top}..{bottom}");
    assert!(count_lit(&with_words) > 100, "its words");
    let hidden = render_with(Some(&Slide { words_hidden: true, ..song.clone() }), &Theme::builtin(), w, h);
    assert_eq!(green_rows(&hidden), Some((top, bottom)), "the same size without its words");
    assert_eq!(count_lit(&hidden), 0, "and no words");
}

#[test]
fn a_screens_colour_fills_the_empty_room_and_its_covered_edges() {
    use sp_model::Rgba;
    let (device, queue) = gpu();
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
    let built = sp_render::build_layers(&mut Fonts::bundled_only(), &layers, &Theme::builtin(), 160, 120);
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
    let (device, queue) = gpu();
    let mut painter = Painter::new(device).expect("painter");
    let coloured = |rgba: Rgba, words: &str| {
        let mut theme = Theme::builtin();
        theme.background = Some(rgba);
        build_scene(&mut Fonts::bundled_only(), Some(&slide(words, None)), &theme, w, h).scene
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
    let (device, queue) = gpu();
    let mut painter = Painter::new(device).expect("painter");
    let photo = flat(1600, 1200, [230, 30, 30]);
    let mut draw = |media: Option<&sp_render::Picture>| {
        let layers = sp_render::Layers { media_fit: sp_model::ImageFit::Fill, media, ..Default::default() };
        let built = sp_render::build_layers(&mut Fonts::bundled_only(), &layers, &Theme::builtin(), 160, 90);
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
    let editing = theme.templates[0].as_slide("Text");
    assert_eq!(editing.objects.iter().filter(|o| o.id == bar.id).count(), 1);
    assert!(sp_render::layout_objects(&editing, &theme).is_none(), "drawn once, as the slide's own");
    // Saving what the editor made keeps the bar.
    let saved = theme.templates[0].with_slide(&editing);
    assert_eq!(saved.objects, vec![bar]);
    assert_eq!(saved.boxes, theme.templates[0].boxes);
}

/// A layout's shape kept in front of the words (dragged above the text boxes in the Theme
/// Editor's Objects list) is drawn over them on every slide that uses the layout; behind them,
/// the words show over it.
#[test]
fn a_layouts_shape_in_front_of_the_words_covers_them() {
    let (w, h) = (320, 180);
    let mut theme = Theme::builtin();
    // A green block over the middle of the main box, where its words are.
    let mut block = Object::shape(sp_model::Shape::Rectangle);
    block.frame = Some(sp_model::SlideRect { x: 0.0, y: 0.0, w: 1920.0, h: 1080.0 });
    block.fill = Some(sp_model::Rgba::rgb(0, 200, 0));
    for template in &mut theme.templates {
        template.objects = vec![block.clone()];
    }
    let words = "WWWWWWWWWWWWWWWWWWWW\nWWWWWWWWWWWWWWWWWWWW";
    let song = slide(words, None);
    let white = |pixels: &[u8]| pixels.chunks(4).filter(|p| p[0] > 200 && p[1] > 200 && p[2] > 200).count();
    let behind = render_with(Some(&song), &theme, w, h);
    assert!(white(&behind) > 100, "behind the block, the words show: {}", white(&behind));
    for template in &mut theme.templates {
        template.in_front = vec![block.id];
    }
    let in_front = render_with(Some(&song), &theme, w, h);
    assert_eq!(white(&in_front), 0, "in front, the block covers them");
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
    sp_render::build_layers(&mut Fonts::bundled_only(), &layers, &Theme::builtin(), w, h).scene
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
    let (device, queue) = gpu();
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
    let plain = build_scene(&mut Fonts::bundled_only(), None, &Theme::builtin(), w, h).scene;
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
    let (device, queue) = gpu();
    let mut painter = Painter::new(device).expect("painter");
    let texture = target_texture(device, w, h);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let plain = build_scene(&mut Fonts::bundled_only(), None, &Theme::builtin(), w, h).scene;

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
    let (device, queue) = gpu();
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

/// A BGRA texture that is already on the GPU (a decoded video picture's own memory) and the
/// same pixels sent as rows from the CPU give pictures that are the same to the last byte: the
/// GPU's copy only turns the order of the bytes.
#[test]
fn a_texture_sent_on_the_gpu_draws_the_same_pixels_as_rows_sent_from_the_cpu() {
    use sp_render::PixelOrder;
    let (w, h) = (64, 36);
    let (sw, sh) = (13u32, 7u32);
    let mut cx = RenderContext::new();
    let id = pollster::block_on(cx.device(None)).expect("these tests need a GPU");
    let (device, queue) = (&cx.devices[id].device, &cx.devices[id].queue);
    let mut painter = Painter::new(device).expect("painter");
    // Every pixel different, in every byte, with the fourth byte full as a video's are.
    let bgra: Vec<u8> =
        (0..sw * sh).flat_map(|n| [(n * 7 + 3) as u8, (n * 19 + 101) as u8, (n * 37 + 200) as u8, 255]).collect();
    let bgra_texture = |width: u32, usage: wgpu::TextureUsages| {
        device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d { width, height: sh, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8Unorm,
            usage,
            view_formats: &[],
        })
    };
    let source = bgra_texture(sw, wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST);
    queue.write_texture(
        source.as_image_copy(),
        &bgra,
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(sw * 4), rows_per_image: None },
        wgpu::Extent3d { width: sw, height: sh, depth_or_array_layers: 1 },
    );
    // Each way has a painter of its own, so that both pictures sit in the same place in the
    // renderer's picture atlas (where a picture sits changes the last digit of how it is scaled).
    let mut cpu_painter = Painter::new(device).expect("painter");
    let from_cpu = cpu_painter.moving_picture(device, sw, sh, PixelOrder::Bgra).expect("moving picture");
    assert!(cpu_painter.show_rows(queue, &from_cpu, &bgra, sw as usize * 4));
    let from_gpu = painter.moving_picture(device, sw, sh, PixelOrder::Bgra).expect("moving picture");
    assert!(painter.show_texture(device, queue, &from_gpu, source.clone()));
    let texture = target_texture(device, w, h);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let drawn = |painter: &mut Painter, moving: &sp_render::MovingPicture| {
        painter.paint(device, queue, &media_scene(&moving.picture(), w, h), &view, w, h).expect("paint");
        read_pixels(device, queue, &texture).expect("read back")
    };
    let (cpu, gpu) = (drawn(&mut cpu_painter, &from_cpu), drawn(&mut painter, &from_gpu));
    let first = cpu.iter().zip(&gpu).position(|(a, b)| a != b);
    assert!(
        first.is_none(),
        "the two ways draw the same picture: first difference at {first:?}: {:?} vs {:?}",
        first.map(|i| &cpu[i / 4 * 4..i / 4 * 4 + 4]),
        first.map(|i| &gpu[i / 4 * 4..i / 4 * 4 + 4])
    );
    assert!(cpu.chunks(4).any(|pixel| pixel[0] != pixel[2]), "and it is not one flat colour");
    // A texture of another size or kind is refused, and nothing changes.
    let wrong = bgra_texture(sw + 1, wgpu::TextureUsages::TEXTURE_BINDING);
    assert!(!painter.show_texture(device, queue, &from_gpu, wrong));
    let rgba = painter.moving_picture(device, sw, sh, PixelOrder::Rgba).expect("moving picture");
    assert!(!painter.show_texture(device, queue, &rgba, source), "a picture made for RGBA bytes isn't sent BGRA");
    assert!(drawn(&mut painter, &from_gpu) == cpu);
}

/// A video's two planes, sent to the GPU to be turned into colours there, are drawn in the
/// colours the processor would have turned them into, for SD and HD video in limited and full
/// range, at an odd size (whose last column and row have colour samples of their own) and with
/// rows longer than their pixels.
#[test]
fn a_videos_planes_are_drawn_in_the_colours_the_processor_makes_of_them() {
    use sp_render::{PixelOrder, YuvColours, YuvMatrix, YuvPlanes};
    let (w, h) = (13u32, 7u32);
    let (cw, ch) = YuvPlanes::chroma_size(w, h);
    let (device, queue) = gpu();
    // Every pixel's brightness different, and colour samples spread over every byte (each
    // row padded with bytes nothing should read).
    let (luma_row, chroma_row) = (w as usize + 3, cw as usize * 2 + 4);
    let luma: Vec<u8> = (0..h as usize * luma_row).map(|n| (n * 37 % 251) as u8).collect();
    let chroma: Vec<u8> = (0..ch as usize * chroma_row).map(|n| (n * 101 % 256) as u8).collect();
    let texture = target_texture(device, w, h);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut cpu_painter = Painter::new(device).expect("painter");
    let mut painter = Painter::new(device).expect("painter");
    painter.prepare_video(device, queue, 1920, 1080).expect("readied for a 1080p video");
    let from_cpu = cpu_painter.moving_picture(device, w, h, PixelOrder::Rgba).expect("moving picture");
    let from_gpu = painter.moving_picture(device, w, h, PixelOrder::Rgba).expect("moving picture");
    let drawn = |painter: &mut Painter, moving: &sp_render::MovingPicture| {
        painter.paint(device, queue, &media_scene(&moving.picture(), w, h), &view, w, h).expect("paint");
        read_pixels(device, queue, &texture).expect("read back")
    };
    for matrix in [YuvMatrix::Bt601, YuvMatrix::Bt709] {
        for full_range in [false, true] {
            let colours = YuvColours { matrix, full_range };
            // The processor's way: each pixel's colours worked out, its square's colour sample
            // used as it is, and sent as rows.
            let rgba: Vec<u8> = (0..h as usize)
                .flat_map(|y| (0..w as usize).map(move |x| (x, y)))
                .flat_map(|(x, y)| {
                    let at = y / 2 * chroma_row + x / 2 * 2;
                    let [r, g, b] = colours.rgb(luma[y * luma_row + x], chroma[at], chroma[at + 1]);
                    [r, g, b, 255]
                })
                .collect();
            assert!(cpu_painter.show_rows(queue, &from_cpu, &rgba, w as usize * 4));
            let planes = YuvPlanes { width: w, height: h, luma: &luma, luma_row, chroma: &chroma, chroma_row, colours };
            assert!(painter.show_planes(device, queue, &from_gpu, &planes));
            let (cpu, gpu) = (drawn(&mut cpu_painter, &from_cpu), drawn(&mut painter, &from_gpu));
            let worst = cpu.iter().zip(&gpu).map(|(a, b)| a.abs_diff(*b)).max();
            let first = cpu.iter().zip(&gpu).position(|(a, b)| a.abs_diff(*b) > 1);
            assert!(
                first.is_none(),
                "{colours:?}: the GPU's colours are the processor's: first difference at pixel {:?}: {:?} vs {:?}",
                first.map(|i| i / 4),
                first.map(|i| &cpu[i / 4 * 4..i / 4 * 4 + 4]),
                first.map(|i| &gpu[i / 4 * 4..i / 4 * 4 + 4])
            );
            let shades: std::collections::HashSet<&[u8]> = cpu.chunks(4).collect();
            assert!(shades.len() > 40, "{colours:?}: a picture of many colours, not one flat one: {}", shades.len());
            eprintln!("{colours:?}: at most {worst:?} apart, {} colours", shades.len());
        }
    }
    // Planes short of bytes, of another size, or for a picture made for BGRA bytes are refused,
    // and nothing changes.
    let colours = YuvColours { matrix: YuvMatrix::Bt709, full_range: true };
    let planes = YuvPlanes { width: w, height: h, luma: &luma, luma_row, chroma: &chroma, chroma_row, colours };
    let before = drawn(&mut painter, &from_gpu);
    assert!(!painter.show_planes(
        device,
        queue,
        &from_gpu,
        &YuvPlanes { chroma: &chroma[..chroma.len() - 5], ..planes }
    ));
    assert!(!painter.show_planes(device, queue, &from_gpu, &YuvPlanes { width: w - 1, ..planes }));
    let bgra = painter.moving_picture(device, w, h, PixelOrder::Bgra).expect("moving picture");
    assert!(!painter.show_planes(device, queue, &bgra, &planes), "a picture made for BGRA bytes isn't sent planes");
    assert!(drawn(&mut painter, &from_gpu) == before);
}

/// Readying the painter for a video leaves nothing of its own behind: the clear picture it
/// paints is let go at the next painting, and pictures drawn after look as they always did.
#[test]
fn readying_for_a_video_leaves_no_picture_behind() {
    let (w, h) = (64, 36);
    let (device, queue) = gpu();
    let mut painter = Painter::new(device).expect("painter");
    painter.prepare_video(device, queue, 1920, 1080).expect("readied for a 1080p video");
    let texture = target_texture(device, w, h);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let plain = build_scene(&mut Fonts::bundled_only(), None, &Theme::builtin(), w, h).scene;
    painter.paint(device, queue, &plain, &view, w, h).expect("paint");
    assert_eq!(painter.moving_pictures(), 0, "the picture it painted is let go");
    let pixels = read_pixels(device, queue, &texture).expect("read back");
    assert_eq!(pixel(&pixels, w, 8, 18), [0, 0, 0, 255]);
}

/// A texture sent with `show_texture` is held by the painter until the GPU says it has finished
/// with it, and no more than a few at a time are held however far behind the GPU is.
#[test]
fn textures_sent_to_the_gpu_are_held_until_it_is_done_with_them() {
    use sp_render::PixelOrder;
    let mut cx = RenderContext::new();
    let id = pollster::block_on(cx.device(None)).expect("these tests need a GPU");
    let (device, queue) = (&cx.devices[id].device, &cx.devices[id].queue);
    let mut painter = Painter::new(device).expect("painter");
    let moving = painter.moving_picture(device, 8, 4, PixelOrder::Bgra).expect("moving picture");
    let make = || {
        device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d { width: 8, height: 4, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
    };
    assert_eq!(painter.textures_in_flight(), 0);
    assert!(painter.show_texture(device, queue, &moving, make()));
    assert_eq!(painter.textures_in_flight(), 1, "held at once, the GPU not yet known to be done");
    device.poll(wgpu::PollType::wait_indefinitely()).expect("the GPU finishes");
    // The next one's arrival lets the finished one go.
    assert!(painter.show_texture(device, queue, &moving, make()));
    assert_eq!(painter.textures_in_flight(), 1, "the first was let go once the GPU was done with it");
    // Far behind: never more than a few.
    for _ in 0..40 {
        assert!(painter.show_texture(device, queue, &moving, make()));
    }
    assert!(painter.textures_in_flight() <= 8, "{} held", painter.textures_in_flight());
    device.poll(wgpu::PollType::wait_indefinitely()).expect("the GPU finishes");
    let texture = target_texture(device, 8, 4);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    painter.paint(device, queue, &media_scene(&moving.picture(), 8, 4), &view, 8, 4).expect("paint");
    assert_eq!(painter.textures_in_flight(), 0, "all let go once the GPU said so");
}

/// A moving picture's fourth bytes are its alpha (a web page, a video with clear parts played
/// by FFmpeg): where they are 0, what is behind it shows.
#[test]
fn a_moving_picture_with_alpha_lets_what_is_behind_it_show() {
    use sp_render::PixelOrder;
    let (w, h) = (64, 36);
    let (device, queue) = gpu();
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

/// The heaviest pictures a service draws, by name (`sentences` is how many sentences of the
/// reading; the amounts the tests ask for are what SoftPresenter Sans, with its many curves,
/// needs to be as heavy as these tests mean, and no more): a full reading, two of them half-way through
/// a dissolve and a wipe, a slide of many shapes, and words over a picture.
fn heavy_scenes(w: u32, h: u32, sentences: usize) -> Vec<(&'static str, sp_render::vello::Scene)> {
    use sp_model::{Direction, TransitionKind};
    let theme = Theme::builtin();
    let mut fonts = Fonts::bundled_only();
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
        let (device, queue) = gpu();
        let (device, queue) = (device.clone(), queue.clone());
        let painter = Painter::new(&device).expect("painter");
        Easel { device, queue, painter }
    }

    /// A painter on a device no other test draws with, for a test that looks at when the GPU
    /// finishes its own work: another thread asking the shared device how far it has got would
    /// finish that work for it.
    fn on_a_device_of_its_own() -> Easel {
        let handle = &lasting_context().devices[0];
        let painter = Painter::new(&handle.device).expect("painter");
        Easel { device: handle.device.clone(), queue: handle.queue.clone(), painter }
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

    /// Lets the painter give memory back, and waits until the GPU has let it go: within five
    /// seconds of the GPU finishing what it was sent, however long a slow GPU takes over that.
    fn rest(&mut self) {
        use std::sync::atomic::{AtomicBool, Ordering};
        let finished = std::sync::Arc::new(AtomicBool::new(false));
        let said = finished.clone();
        self.queue.on_submitted_work_done(move || said.store(true, Ordering::Release));
        let mut idle_since = None;
        while !self.painter.rest(&self.device, &self.queue) {
            if finished.load(Ordering::Acquire) {
                let since = *idle_since.get_or_insert_with(std::time::Instant::now);
                assert!(since.elapsed() < std::time::Duration::from_secs(5), "the GPU lets the memory go");
            }
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
        for (name, scene) in heavy_scenes(w, h, 1) {
            let ours = sized.paint(&scene, w, h).expect("paint");
            let theirs = fixed.paint(&scene, w, h).expect("paint with fixed sizes");
            assert!(count_lit(&ours) > if w > 1_000 { 300 } else { 10 }, "{name} at {w}: something bright is drawn");
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
        for (name, scene) in heavy_scenes(w, h, 9) {
            let name = format!("{name} at {w}");
            let first = sized.paint(&scene, w, h).expect("paint");
            // (Small letters at thumbnail size, half faded in a dissolve, light up few pixels.)
            assert!(count_lit(&first) > if w > 1_000 { 1_000 } else { 50 }, "{name}: the letters are drawn");
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
    let (name, reading) = heavy_scenes(3840, 2160, 9).swap_remove(0);
    assert_eq!(name, "reading");
    assert!(fixed.paint(&reading, 3840, 2160).is_err(), "the fixed sizes are too small, and say so");
}

/// One painter draws a heavy 4K slide, then a thumbnail of a light one, then the heavy slide
/// again: each picture is right, the heavy one the same both times. When drawing stops, the
/// memory the heavy slide took is given back, and the light slide still draws after that.
#[test]
fn a_painter_goes_from_heavy_to_light_and_back_and_gives_memory_back() {
    let mut easel = Easel::new();
    let heavy = heavy_scenes(3840, 2160, 9).swap_remove(0).1;
    let light = heavy_scenes(480, 270, 1).swap_remove(4).1;
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
    let (name, over_photo) = heavy_scenes(w, h, 1).swap_remove(4);
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
    // The picture that fitted took as many runs as the GPU's room made it (one or more).
    let sent = easel.painter.runs() - runs;
    assert!(sent >= 1, "the picture that fitted was sent to the GPU");
    // The same once more, and the refusal is still remembered.
    assert_eq!(refuse(&mut easel), Err(sp_render::PaintError::TooHeavy), "still too heavy");
    assert_eq!(easel.painter.runs(), runs + sent, "only the picture that fitted was sent to the GPU");
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
    let mut fonts = Fonts::bundled_only();
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
    let mut fonts = Fonts::bundled_only();
    let light: Vec<Sized> = (0..10)
        .map(|seed| (480, 270, build_scene(&mut fonts, Some(&dense_passage(seed, 3)), &theme, 480, 270).scene))
        .collect();
    let heavy: Vec<Sized> = [(3840, 2160), (2560, 1440)]
        .into_iter()
        .flat_map(|(w, h)| heavy_scenes(w, h, 9).into_iter().map(move |(_, scene)| (w, h, scene)))
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
    // Each thread draws until the time is up, and past it until it has drawn the pictures it must
    // (a slow GPU draws few heavy ones in the time), for two minutes at most.
    let going = |drawn: usize, least: usize| {
        Instant::now() < until || (drawn < least && Instant::now() < until + Duration::from_secs(120))
    };
    let draw = |name: &'static str, least: usize, mut easel: Easel, scenes: &[Sized], expected: &[Vec<u8>]| {
        let (mut drawn, mut longest) = (0usize, Duration::ZERO);
        while going(drawn, least) {
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
        while going(drawn, 6) {
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
        let light_thread = threads.spawn(|| draw("light", 6, light_easel, &light, &light_expected));
        let heavy_thread = threads.spawn(|| draw("heavy", 3, heavy_easel, &heavy, &heavy_expected));
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
    let mut fonts = Fonts::bundled_only();
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
    let (name, reading) = heavy_scenes(w, h, 9).swap_remove(0);
    assert_eq!(name, "reading");
    let expected = Easel::kept_at(ROOMY).paint(&reading, w, h).expect("paint with plenty of room");

    let mut easel = Easel::on_a_device_of_its_own();
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
    let (name, over_photo) = heavy_scenes(w, h, 1).swap_remove(4);
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
    let mut easel = Easel::on_a_device_of_its_own();
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
    let scene = sp_render::build_layers(&mut Fonts::bundled_only(), &layers, &Theme::builtin(), w, h).scene;
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

/// The graphics are chosen by the app, not left to the order a system lists them in: of the
/// adapters there are, one of the best kind (a discrete GPU, then an integrated one), and on
/// Windows the Vulkan one before the DirectX 12 one of the same chip, which was about eighty
/// times slower to start in a service session and eight in the signed-in user's, on an Intel
/// UHD 770. Left alone when the person asked for a backend or an adapter themselves.
#[test]
fn the_graphics_are_chosen_deliberately() {
    if ["WGPU_BACKEND", "WGPU_ADAPTER_NAME", "WGPU_POWER_PREF"].iter().any(|name| std::env::var_os(name).is_some()) {
        return;
    }
    let cx = context();
    let adapters = pollster::block_on(cx.instance.enumerate_adapters(wgpu::Backends::all()));
    let chosen = cx.devices[0].adapter().get_info();
    eprintln!("chosen: {} ({:?}, {:?})", chosen.name, chosen.backend, chosen.device_type);
    for adapter in &adapters {
        let info = adapter.get_info();
        eprintln!("  there is {} ({:?}, {:?})", info.name, info.backend, info.device_type);
    }
    let kind = |info: &wgpu::AdapterInfo| match info.device_type {
        wgpu::DeviceType::DiscreteGpu => 0,
        wgpu::DeviceType::IntegratedGpu => 1,
        wgpu::DeviceType::VirtualGpu => 2,
        wgpu::DeviceType::Other => 3,
        wgpu::DeviceType::Cpu => 4,
    };
    let best = adapters.iter().map(|adapter| kind(&adapter.get_info())).min().expect("an adapter");
    assert_eq!(kind(&chosen), best, "an adapter of the best kind is chosen");
    if cfg!(windows) {
        let has_vulkan = adapters.iter().any(|adapter| {
            let info = adapter.get_info();
            kind(&info) == best && info.backend == wgpu::Backend::Vulkan
        });
        if has_vulkan {
            assert_eq!(chosen.backend, wgpu::Backend::Vulkan, "Vulkan is chosen before DirectX 12");
        }
    }
}

// ---- Fades drawn from textures of their two pictures ----------------------------------------

/// A busy picture to put behind the slides: diagonal bands and a bright blob, so a wrong shift
/// or blur shows.
fn busy_picture(w: u32, h: u32) -> sp_render::Picture {
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let band = ((x + y) / 40) % 2 == 0;
            let (dx, dy) = (f64::from(x) - f64::from(w) * 0.7, f64::from(y) - f64::from(h) * 0.3);
            let glow = (255.0 * (-(dx * dx + dy * dy) / 40_000.0).exp()) as u8;
            let base = if band { [30, 60, 120] } else { [90, 40, 70] };
            rgba.extend_from_slice(&[base[0].max(glow), base[1].max(glow), base[2], 255]);
        }
    }
    sp_render::Picture::from_rgba(w, h, rgba).expect("picture")
}

/// A theme whose words have a green outline and a shadow, so both are in the fade.
fn outlined_theme() -> Theme {
    use sp_model::Rgba;
    let mut theme = Theme::builtin();
    for style in theme.templates.iter_mut().flat_map(|template| &mut template.boxes) {
        style.outline_color = Rgba::rgb(0, 255, 0);
        style.outline_width = 6.0;
        style.fill = Rgba::rgb(250, 240, 200);
        style.shadow = true;
    }
    theme
}

fn layers_of<'a>(media: &'a sp_render::Picture, slide: &'a Slide) -> sp_render::Layers<'a> {
    sp_render::Layers {
        media_fit: sp_model::ImageFit::Fill,
        media: Some(media),
        slide: Some(slide),
        overlay: None,
        time: 0.0,
        backdrop: None,
        mask: [0.0; 4],
        announcement: None,
    }
}

fn paint_to_pixels(painter: &mut Painter, scene: &sp_render::vello::Scene, w: u32, h: u32) -> Vec<u8> {
    let (device, queue) = gpu();
    let texture = target_texture(device, w, h);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    painter.paint(device, queue, scene, &view, w, h).expect("paint");
    read_pixels(device, queue, &texture).expect("read back")
}

/// The mean difference of the colour bytes, the largest difference of any colour byte, and how
/// many pixels differ by more than `loose` anywhere.
fn compare(a: &[u8], b: &[u8], loose: u8) -> (f64, u8, usize) {
    let (mut total, mut worst, mut far) = (0u64, 0u8, 0usize);
    for (p, q) in a.chunks(4).zip(b.chunks(4)) {
        let d = (0..3).map(|i| p[i].abs_diff(q[i])).max().unwrap_or(0);
        total += (0..3).map(|i| u64::from(p[i].abs_diff(q[i]))).sum::<u64>();
        worst = worst.max(d);
        far += usize::from(d > loose);
    }
    (total as f64 / (a.len() / 4 * 3) as f64, worst, far)
}

const FADE_PROGRESS: [f32; 5] = [0.0, 0.25, 0.5, 0.75, 1.0];

/// Every kind of transition, at five points through it, drawn from textures of its two pictures
/// (each painted once) beside drawn from the pictures themselves (the way fades always were),
/// with outlined, shadowed words and a translation line over a picture. The kinds the app
/// caches (`fade_cacheable`: the fades and the clipped ones) are held to a mean difference of
/// 0.05 and no colour byte more than 2 off; the others, which move or scale a picture and so
/// sample the texture between its texels, are measured and reported (and drawn from the
/// slides in the app).
#[test]
fn a_fade_from_textures_matches_a_fade_drawn_from_the_slides() {
    use sp_model::{Direction, TransitionKind};
    let (w, h) = (960, 540);
    let (device, queue) = gpu();
    let mut painter = Painter::new(device).expect("painter");
    let mut fonts = Fonts::bundled_only();
    let theme = outlined_theme();
    let picture = busy_picture(w, h);
    let first = slide("Glory to God in the highest", Some("Слава Богу в вышних"));
    let second = slide("And on earth peace", Some("И на земле мир"));
    let from = sp_render::build_layers(&mut fonts, &layers_of(&picture, &first), &theme, w, h).scene;
    let to = sp_render::build_layers(&mut fonts, &layers_of(&picture, &second), &theme, w, h).scene;
    let cached_from = painter.cache_whole(device, queue, &from, w, h).expect("cache from");
    let cached_to = painter.cache_whole(device, queue, &to, w, h).expect("cache to");
    let mut report = String::new();
    let mut failures = Vec::new();
    for kind in TransitionKind::ALL.into_iter().filter(|kind| *kind != TransitionKind::Cut) {
        for direction in [Direction::FromLeft, Direction::FromTop] {
            for amount in FADE_PROGRESS {
                let plain = sp_render::transition(kind, direction, &from, &to, amount, w, h);
                let cached =
                    sp_render::transition(kind, direction, cached_from.scene(), cached_to.scene(), amount, w, h);
                let (a, b) =
                    (paint_to_pixels(&mut painter, &plain, w, h), paint_to_pixels(&mut painter, &cached, w, h));
                let (mean, worst, far) = compare(&a, &b, 24);
                report.push_str(&format!("{kind:?} {direction:?} {amount}: mean {mean:.3} worst {worst} far {far}\n"));
                if sp_render::fade_cacheable(kind) && (mean > 0.05 || worst > 2 || far > 0) {
                    failures.push(format!("{kind:?} {direction:?} at {amount}: mean {mean:.3}, {far} far"));
                }
            }
        }
    }
    eprintln!("{report}");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Not a check: times one dissolve step at 1920 by 1080, drawn from the slides (as fades always
/// were), from textures of them through the renderer (as the app now does), and from textures by
/// a plain two-texture blend of its own, which only does dissolves. Each picture is read back, so
/// the time is the GPU's as well as the processor's. Run it by name to see the numbers.
#[test]
#[ignore = "a timing, not a check: run with --ignored --nocapture"]
fn dissolve_step_cost() {
    use sp_model::{Direction, TransitionKind};
    use std::time::Instant;
    let (w, h) = (1920, 1080);
    let (device, queue) = gpu();
    let mut painter = Painter::new(device).expect("painter");
    let mut fonts = Fonts::bundled_only();
    let theme = outlined_theme();
    let picture = busy_picture(w, h);
    let first = slide("Glory to God in the highest", Some("Слава Богу в вышних"));
    let second = slide("And on earth peace", Some("И на земле мир"));
    let from = sp_render::build_layers(&mut fonts, &layers_of(&picture, &first), &theme, w, h).scene;
    let to = sp_render::build_layers(&mut fonts, &layers_of(&picture, &second), &theme, w, h).scene;
    let cached_from = painter.cache_whole(device, queue, &from, w, h).expect("cache from");
    let cached_to = painter.cache_whole(device, queue, &to, w, h).expect("cache to");
    let texture = target_texture(device, w, h);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let one_pixel = |texture: &wgpu::Texture| {
        // A readback of the whole texture is the sync point; it costs the same for each way.
        read_pixels(device, queue, texture).expect("read back")
    };
    let steps = 40;
    let mut time = |label: &str, make: &mut dyn FnMut(f32) -> sp_render::vello::Scene| {
        for step in 0..4 {
            let scene = make(step as f32 / 40.0);
            painter.paint(device, queue, &scene, &view, w, h).expect("paint");
        }
        one_pixel(&texture);
        let (mut scene_time, mut paint_time) = (0.0, 0.0);
        let began = Instant::now();
        for step in 0..steps {
            let a = Instant::now();
            let scene = make(step as f32 / steps as f32);
            scene_time += a.elapsed().as_secs_f64();
            let b = Instant::now();
            painter.paint(device, queue, &scene, &view, w, h).expect("paint");
            paint_time += b.elapsed().as_secs_f64();
        }
        one_pixel(&texture);
        let total = began.elapsed().as_secs_f64();
        eprintln!(
            "{label}: {:.2} ms a step in all ({:.2} ms building the scene, {:.2} ms in paint)",
            total * 1000.0 / steps as f64,
            scene_time * 1000.0 / steps as f64,
            paint_time * 1000.0 / steps as f64
        );
    };
    time("from the slides  ", &mut |a| {
        sp_render::transition(TransitionKind::Dissolve, Direction::FromLeft, &from, &to, a, w, h)
    });
    let (cf, ct) = (cached_from.scene().clone(), cached_to.scene().clone());
    time("from textures    ", &mut |a| {
        sp_render::transition(TransitionKind::Dissolve, Direction::FromLeft, &cf, &ct, a, w, h)
    });
    // A plain blend: one triangle, two textures, a mix.
    let pictures = [&from, &to].map(|scene| {
        let t = target_texture(device, w, h);
        let v = t.create_view(&wgpu::TextureViewDescriptor::default());
        painter.paint(device, queue, scene, &v, w, h).expect("paint");
        v
    });
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(
            "struct Out { @builtin(position) p: vec4<f32>, @location(0) uv: vec2<f32> }
@vertex fn vs(@builtin(vertex_index) i: u32) -> Out {
    var o: Out;
    let c = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    o.p = vec4<f32>(c * 2.0 - 1.0, 0.0, 1.0);
    o.uv = vec2<f32>(c.x, 1.0 - c.y);
    return o;
}
@group(0) @binding(0) var a: texture_2d<f32>;
@group(0) @binding(1) var b: texture_2d<f32>;
@group(0) @binding(2) var s: sampler;
struct Mixing { amount: f32 }
@group(0) @binding(3) var<uniform> m: Mixing;
@fragment fn fs(i: Out) -> @location(0) vec4<f32> {
    return mix(textureSample(a, s, i.uv), textureSample(b, s, i.uv), m.amount);
}"
            .into(),
        ),
    });
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor::default());
    let uniform = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 16,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let target_format = wgpu::TextureFormat::Rgba8Unorm;
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: None,
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(target_format.into())],
        }),
        multiview_mask: None,
        cache: None,
    });
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: target_format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let target_view = target.create_view(&wgpu::TextureViewDescriptor::default());
    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&pictures[0]) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&pictures[1]) },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&sampler) },
            wgpu::BindGroupEntry { binding: 3, resource: uniform.as_entire_binding() },
        ],
    });
    let blend_step = |amount: f32| {
        queue.write_buffer(&uniform, 0, &amount.to_le_bytes().repeat(4));
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.draw(0..3, 0..1);
        }
        queue.submit([encoder.finish()]);
    };
    for step in 0..4 {
        blend_step(step as f32 / 40.0);
    }
    one_pixel(&target);
    let began = Instant::now();
    for step in 0..steps {
        blend_step(step as f32 / steps as f32);
    }
    one_pixel(&target);
    eprintln!("two-texture blend : {:.2} ms a step in all", began.elapsed().as_secs_f64() * 1000.0 / steps as f64);
}

/// The plain two-texture blend (`FadeBlend`), which the app uses for the fades that are a sum of
/// the two pictures, gives the same pictures as drawing them from the slides: at the ends exactly
/// the old and the new picture (for every such kind at the end, which a window then keeps as its
/// picture), and between them within a level or two, at many points through the fade (the app's
/// own fades are eased, so any point may come).
#[test]
fn the_plain_blend_matches_a_fade_drawn_from_the_slides() {
    use sp_model::{Direction, TransitionKind};
    let (w, h) = (960, 540);
    let (device, queue) = gpu();
    let mut painter = Painter::new(device).expect("painter");
    let blend = sp_render::FadeBlend::new(device);
    let mut fonts = Fonts::bundled_only();
    let theme = outlined_theme();
    let picture = busy_picture(w, h);
    let first = slide("Glory to God in the highest", Some("Слава Богу в вышних"));
    let second = slide("And on earth peace", Some("И на земле мир"));
    let from = sp_render::build_layers(&mut fonts, &layers_of(&picture, &first), &theme, w, h).scene;
    let to = sp_render::build_layers(&mut fonts, &layers_of(&picture, &second), &theme, w, h).scene;
    let cached_from = painter.cache_whole(device, queue, &from, w, h).expect("cache from");
    let cached_to = painter.cache_whole(device, queue, &to, w, h).expect("cache to");
    let (old, new) = (paint_to_pixels(&mut painter, &from, w, h), paint_to_pixels(&mut painter, &to, w, h));
    let mut report = String::new();
    let mut failures = Vec::new();
    for kind in [TransitionKind::Dissolve, TransitionKind::Fade, TransitionKind::FadeBlack, TransitionKind::FadeWhite] {
        for step in 0..=20 {
            let amount = step as f32 / 20.0;
            let plain = sp_render::transition(kind, Direction::FromLeft, &from, &to, amount, w, h);
            let want = paint_to_pixels(&mut painter, &plain, w, h);
            let weights = sp_render::blend_weights(kind, amount).expect("a sum of the two");
            let texture = target_texture(device, w, h);
            assert!(blend.draw(device, queue, &cached_from, &cached_to, weights, &texture));
            let got = read_pixels(device, queue, &texture).expect("read back");
            let (mean, worst, _) = compare(&want, &got, 2);
            report.push_str(&format!("{kind:?} {amount}: mean {mean:.3} worst {worst}\n"));
            // The renderer rounds to a whole level after each layer, the blend once: one level off.
            if mean > 0.5 || worst > 1 {
                failures.push(format!("{kind:?} at {amount}: mean {mean:.3} worst {worst}"));
            }
            if step == 0 && kind == TransitionKind::Dissolve {
                assert_eq!(compare(&old, &got, 0).1, 0, "the first step is the old picture");
            }
            // The app's windows keep the last step as the new picture, not painted again.
            if step == 20 {
                assert_eq!(compare(&new, &got, 0).1, 0, "{kind:?}: the last step is the new picture, exactly");
            }
        }
    }
    eprintln!("{report}");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The side a fade starts from, copied from a window's picture (`Painter::cache_copy`) as the app
/// takes it when the window holds that picture, is the very side painting the slide again gives
/// (`cache_whole`): every step of the fade from it is the same, through the plain blend and
/// through the renderer.
#[test]
fn a_side_copied_from_a_painted_picture_is_the_side_painted_again() {
    use sp_model::{Direction, TransitionKind};
    let (w, h) = (960, 540);
    let (device, queue) = gpu();
    let mut painter = Painter::new(device).expect("painter");
    let blend = sp_render::FadeBlend::new(device);
    let mut fonts = Fonts::bundled_only();
    let theme = outlined_theme();
    let picture = busy_picture(w, h);
    let first = slide("Glory to God in the highest", Some("Слава Богу в вышних"));
    let second = slide("And on earth peace", Some("И на земле мир"));
    let from = sp_render::build_layers(&mut fonts, &layers_of(&picture, &first), &theme, w, h).scene;
    let to = sp_render::build_layers(&mut fonts, &layers_of(&picture, &second), &theme, w, h).scene;
    // The window's picture: the first slide painted as the screens paint a window's.
    let window = target_texture(device, w, h);
    let view = window.create_view(&wgpu::TextureViewDescriptor::default());
    painter.paint(device, queue, &from, &view, w, h).expect("paint");
    let copied = painter.cache_copy(device, queue, &window).expect("copy");
    let painted = painter.cache_whole(device, queue, &from, w, h).expect("paint again");
    let cached_to = painter.cache_whole(device, queue, &to, w, h).expect("to");
    for step in 0..=10 {
        let amount = step as f32 / 10.0;
        let weights = sp_render::blend_weights(TransitionKind::Dissolve, amount).expect("a sum of the two");
        let (a, b) = (target_texture(device, w, h), target_texture(device, w, h));
        assert!(blend.draw(device, queue, &copied, &cached_to, weights, &a));
        assert!(blend.draw(device, queue, &painted, &cached_to, weights, &b));
        let (a, b) = (read_pixels(device, queue, &a).expect("read"), read_pixels(device, queue, &b).expect("read"));
        assert_eq!(compare(&a, &b, 0).1, 0, "blended, step {step}");
        let push = |side: &sp_render::CachedSide| {
            sp_render::transition(
                TransitionKind::Push,
                Direction::FromLeft,
                side.scene(),
                cached_to.scene(),
                amount,
                w,
                h,
            )
        };
        let (a, b) =
            (paint_to_pixels(&mut painter, &push(&copied), w, h), paint_to_pixels(&mut painter, &push(&painted), w, h));
        assert_eq!(compare(&a, &b, 0).1, 0, "pushed, step {step}");
    }
}

/// Not a check: saves the middle of a dissolve drawn from the slides and drawn from textures as
/// pictures, into the folder named by SOFTPRESENTER_FADE_PICTURES, to look at.
#[test]
#[ignore = "saves pictures to look at: set SOFTPRESENTER_FADE_PICTURES=folder and run with --ignored"]
fn save_the_middle_of_a_dissolve() {
    use sp_model::{Direction, TransitionKind};
    let Some(folder) = std::env::var_os("SOFTPRESENTER_FADE_PICTURES").map(PathBuf::from) else { return };
    let (w, h) = (960, 540);
    let (device, queue) = gpu();
    let mut painter = Painter::new(device).expect("painter");
    let blend = sp_render::FadeBlend::new(device);
    let mut fonts = Fonts::bundled_only();
    let theme = outlined_theme();
    let picture = busy_picture(w, h);
    let first = slide("Glory to God in the highest", Some("Слава Богу в вышних"));
    let second = slide("And on earth peace", Some("И на земле мир"));
    let from = sp_render::build_layers(&mut fonts, &layers_of(&picture, &first), &theme, w, h).scene;
    let to = sp_render::build_layers(&mut fonts, &layers_of(&picture, &second), &theme, w, h).scene;
    let (a, b) = (
        painter.cache_whole(device, queue, &from, w, h).expect("cache"),
        painter.cache_whole(device, queue, &to, w, h).expect("cache"),
    );
    let plain = sp_render::transition(TransitionKind::Dissolve, Direction::FromLeft, &from, &to, 0.4, w, h);
    write_png(&folder.join("dissolve-plain.png"), &paint_to_pixels(&mut painter, &plain, w, h), w, h);
    let texture = target_texture(device, w, h);
    let weights = sp_render::blend_weights(TransitionKind::Dissolve, 0.4).expect("weights");
    assert!(blend.draw(device, queue, &a, &b, weights, &texture));
    write_png(&folder.join("dissolve-blend.png"), &read_pixels(device, queue, &texture).expect("read"), w, h);
}

/// A slide with outlined, shadowed words and a translation line, over a picture that changes
/// (a playing video's pictures), is the same drawn whole each time and drawn from a texture of
/// the slide above the picture laid over the picture's own: the way the app draws a video's
/// pictures behind a slide that nothing else moves on.
#[test]
fn a_slide_kept_in_a_texture_over_a_changing_picture_matches_the_slide_drawn_whole() {
    let (w, h) = (960, 540);
    let (device, queue) = gpu();
    let mut painter = Painter::new(device).expect("painter");
    let mut fonts = Fonts::bundled_only();
    let theme = outlined_theme();
    let moving = painter.moving_picture(device, w, h, sp_render::PixelOrder::Rgba).expect("moving picture");
    let video = moving.picture();
    let words = slide("Glory to God in the highest", Some("Слава Богу в вышних"));
    let layers = layers_of(&video, &words);
    let whole = sp_render::build_layers(&mut fonts, &layers, &theme, w, h).scene;
    let parts = sp_render::build_layer_parts(&mut fonts, &layers, &theme, w, h);
    // The two parts one after the other are the whole (to a level, which is all the GPU promises).
    let mut one_after_the_other = parts.under.clone();
    one_after_the_other.append(&parts.over, None);
    let frame_of = |shade: u8| {
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                let bright = (((x * 255 / w) as u8) / 2).saturating_add(shade);
                let stripe = if (x / 30 + y / 30) % 2 == 0 { 40 } else { 0 };
                rgba.extend_from_slice(&[bright, bright.saturating_sub(stripe), 255 - bright, 255]);
            }
        }
        rgba
    };
    let mut cached = painter.cache_split(device, queue, parts.under, &parts.over, w, h).expect("cache");
    assert!(cached.is_split());
    let mut report = String::new();
    let mut failures = Vec::new();
    for shade in [0u8, 60, 120, 200] {
        let rows = frame_of(shade);
        assert!(painter.show_rows(queue, &moving, &rows, (w * 4) as usize));
        let (a, b, c) = (
            paint_to_pixels(&mut painter, &whole, w, h),
            paint_to_pixels(&mut painter, cached.scene(), w, h),
            paint_to_pixels(&mut painter, &one_after_the_other, w, h),
        );
        assert!(compare(&a, &c, 0).1 <= 1, "the two parts one after the other are the whole");
        let (mean, worst, far) = compare(&a, &b, 3);
        report.push_str(&format!("picture {shade}: mean {mean:.4} worst {worst} off by more than 3: {far}\n"));
        if mean > 0.02 || worst > 2 || far > 0 {
            failures.push(format!("picture {shade}: mean {mean:.4} worst {worst} far {far}"));
        }
    }
    // The part below the slide is made again when the video is given another picture to draw.
    let other = sp_render::Picture::from_rgba(w, h, frame_of(30)).expect("picture");
    assert!(!other.same_as(&video) && video.same_as(&moving.picture()));
    let under = sp_render::build_under(&layers_of(&other, &words), &theme, w, h);
    cached.set_under(under);
    let after = paint_to_pixels(&mut painter, cached.scene(), w, h);
    let again = sp_render::build_layers(&mut fonts, &layers_of(&other, &words), &theme, w, h).scene;
    let (mean, worst, _) = compare(&paint_to_pixels(&mut painter, &again, w, h), &after, 3);
    report.push_str(&format!("another picture: mean {mean:.4} worst {worst}\n"));
    if mean > 0.02 || worst > 2 {
        failures.push(format!("another picture: mean {mean:.4} worst {worst}"));
    }
    eprintln!("{report}");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Not a check: times one picture of a video behind a slide at 1920 by 1080, drawn whole (as each
/// picture of the video was) and from the slide's texture laid over the video's. Run it by name.
#[test]
#[ignore = "a timing, not a check: run with --ignored --nocapture"]
fn video_picture_step_cost() {
    use std::time::Instant;
    let (w, h) = (1920, 1080);
    let (device, queue) = gpu();
    let mut painter = Painter::new(device).expect("painter");
    let mut fonts = Fonts::bundled_only();
    let theme = outlined_theme();
    let moving = painter.moving_picture(device, w, h, sp_render::PixelOrder::Rgba).expect("moving picture");
    let video = moving.picture();
    let words =
        slide("Glory to God in the highest, and on earth peace to people of good will", Some("Слава Богу в вышних"));
    let layers = layers_of(&video, &words);
    let whole = sp_render::build_layers(&mut fonts, &layers, &theme, w, h).scene;
    let parts = sp_render::build_layer_parts(&mut fonts, &layers, &theme, w, h);
    let cached = painter.cache_split(device, queue, parts.under, &parts.over, w, h).expect("cache");
    let rows = vec![90u8; (w * h * 4) as usize];
    let texture = target_texture(device, w, h);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut time = |label: &str, scene: &sp_render::vello::Scene| {
        for _ in 0..4 {
            painter.show_rows(queue, &moving, &rows, (w * 4) as usize);
            painter.paint(device, queue, scene, &view, w, h).expect("paint");
        }
        read_pixels(device, queue, &texture).expect("read back");
        let began = Instant::now();
        let steps = 40;
        for _ in 0..steps {
            painter.show_rows(queue, &moving, &rows, (w * 4) as usize);
            painter.paint(device, queue, scene, &view, w, h).expect("paint");
        }
        read_pixels(device, queue, &texture).expect("read back");
        eprintln!("{label}: {:.2} ms a picture", began.elapsed().as_secs_f64() * 1000.0 / steps as f64);
    };
    time("slide drawn whole each time   ", &whole);
    time("slide kept in a texture       ", cached.scene());
}

/// What one step of [`first_big_picture_cost`] took of the calling thread, and what the painter's
/// own time in it was made of.
fn say_step(
    name: &str,
    painter: &Painter,
    began: std::time::Instant,
    before: sp_render::vello::PaintWork,
    waited: std::time::Duration,
) {
    let ms = |d: std::time::Duration| d.as_secs_f64() * 1000.0;
    let (took, now) = (began.elapsed(), painter.work());
    eprintln!(
        "  {name:<44} {:>7.2} ms: GPU wait {:.2}, working out {:.2}, {} sent from memory ({} MB) {:.2}, {} copied on \
         the GPU, pictures' texture made x{} ({} MB) {:.2}, handing over {:.2}, commands {:.2}; pictures' texture \
         now {} px",
        ms(took),
        ms(painter.gpu_waited() - waited),
        ms(now.resolving - before.resolving),
        now.pictures_sent - before.pictures_sent,
        (now.bytes_sent - before.bytes_sent) >> 20,
        ms(now.sending - before.sending),
        now.pictures_copied - before.pictures_copied,
        now.atlases_made - before.atlases_made,
        (now.atlas_bytes_made - before.atlas_bytes_made) >> 20,
        ms(now.atlas_making - before.atlas_making),
        ms(now.handing_over - before.handing_over),
        ms(now.recording - before.recording),
        painter.pictures_texture_side(),
    );
}

/// Not a check: what the first drawing of a big picture costs the thread that draws it, part by
/// part, at 3840 by 2160 with a 3840 by 2160 picture behind words, from the state the screens'
/// painter is in when the app has started (readied for a 1080p video). Then the same slide again
/// (nothing to send), a second big picture never seen before (a fade between two pictures paints
/// the new one while the old is still kept), and the first again. Each step's time is the calling
/// thread's, until the call returns; the GPU's own work is waited for between steps and not
/// counted. Run it by name, in a release build:
/// `cargo test --release -p sp-render --test golden first_big_picture_cost -- --ignored --nocapture`.
#[test]
#[ignore = "a timing, not a check: run with --ignored --nocapture"]
fn first_big_picture_cost() {
    use std::time::Instant;
    let (w, h) = (3840, 2160);
    let (device, queue) = gpu();
    let mut fonts = Fonts::bundled_only();
    let theme = outlined_theme();
    let words =
        slide("Glory to God in the highest, and on earth peace to people of good will", Some("Слава Богу в вышних"));
    let texture = target_texture(device, w, h);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    for round in 0..3 {
        eprintln!("A painter new from the app's start, round {round}:");
        let mut painter = Painter::new(device).expect("painter");
        painter.prepare_video(device, queue, 1920, 1080).expect("readied");
        let (first, second) = (busy_picture(w, h), busy_picture(w, h));
        let mut step = |name: &str, picture: &sp_render::Picture| {
            let scene = sp_render::build_layers(&mut fonts, &layers_of(picture, &words), &theme, w, h).scene;
            let _ = device.poll(wgpu::PollType::wait_indefinitely());
            let (began, before, waited) = (Instant::now(), painter.work(), painter.gpu_waited());
            painter.paint(device, queue, &scene, &view, w, h).expect("paint");
            say_step(name, &painter, began, before, waited);
        };
        step("a big picture's first drawing", &first);
        step("the same again", &first);
        step("a second big picture's first drawing", &second);
        step("the first picture again", &first);
        step("the second again", &second);
    }
}

/// A window's picture for the close mark to go over, as the screens' windows have them (RGBA):
/// light grey to the left of `split`, black to its right, so one mark shows over both.
fn split_picture(w: u32, h: u32, split: u32) -> wgpu::Texture {
    let (device, queue) = gpu();
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("a window's picture"),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let pixels: Vec<u8> =
        (0..w * h).flat_map(|at| if at % w < split { [200, 200, 200, 255] } else { [0, 0, 0, 255] }).collect();
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &pixels,
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: Some(h) },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    texture
}

/// Lays the close mark over a split picture and reads it back.
fn with_close_mark(look: &sp_render::MarkLook, square: sp_render::Square, w: u32, h: u32, split: u32) -> Vec<u8> {
    let (device, queue) = gpu();
    let picture = split_picture(w, h, split);
    let mark = sp_render::CloseMark::new(device, wgpu::TextureFormat::Rgba8Unorm);
    assert_eq!(mark.format(), wgpu::TextureFormat::Rgba8Unorm);
    let view = picture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    mark.draw(queue, &mut encoder, &view, (w, h), square, look);
    queue.submit([encoder.finish()]);
    read_pixels(device, queue, &picture).expect("read back")
}

/// The X a screen's window shows over its picture when the pointer moves over it (the app's
/// `close_button`): a dark plate the picture shows through, a faint light ring that keeps it in
/// sight on black, and a light X in the middle. Nothing outside its round plate changes, and at
/// strength 0 nothing at all. The colours are the app's (its panel grey, its light text).
#[test]
fn the_close_mark_lies_over_the_picture_in_its_corner_and_nowhere_else() {
    let (w, h, split) = (400, 120, 340);
    let square = sp_render::Square { x: 304.0, y: 24.0, side: 72.0 };
    let look = sp_render::MarkLook {
        plate: [25.0 / 255.0, 25.0 / 255.0, 25.0 / 255.0, 0.72],
        ring: [243.0 / 255.0, 243.0 / 255.0, 239.0 / 255.0, 0.22],
        cross: [185.0 / 255.0, 185.0 / 255.0, 178.0 / 255.0, 0.95],
        strength: 1.0,
    };
    let before = {
        let (device, queue) = gpu();
        read_pixels(device, queue, &split_picture(w, h, split)).expect("read back")
    };
    let none = with_close_mark(&sp_render::MarkLook { strength: 0.0, ..look }, square, w, h, split);
    assert!(none == before, "at strength 0 nothing changes");
    let marked = with_close_mark(&look, square, w, h, split);
    assert_matches_golden("close_mark", &marked, w, h);
    // Outside the plate (the square's corners, and everything beyond it), the picture as it was.
    for (x, y) in [(10, 10), (302, 60), (378, 60), (340, 22), (340, 98), (307, 27), (372, 27), (307, 92), (372, 92)] {
        assert_eq!(pixel(&marked, w, x, y), pixel(&before, w, x, y), "({x}, {y}) is outside the plate");
    }
    // The middle of the X is light; the plate darkens the light picture between its arms, which
    // shows through it; the ring shows on black.
    let middle = pixel(&marked, w, 340, 60);
    assert!(middle[0] >= 170, "the X is light: {middle:?}");
    let plate = pixel(&marked, w, 316, 60);
    assert!(plate[0] < 120 && plate[0] > 40, "a dark plate the grey shows through: {plate:?}");
    let ring = pixel(&marked, w, 375, 60);
    assert!(ring[0] > 20, "the ring shows on black: {ring:?}");
    assert!(marked.chunks(4).all(|p| p[3] == 255), "the window stays solid");
    // A square partly outside the picture draws nothing (the app never asks for one).
    let outside = with_close_mark(&look, sp_render::Square { x: 360.0, ..square }, w, h, split);
    assert!(outside == before);
}

// ---- The early stages on the processor (Windows' software graphics, an Intel Mac's) ----------

/// A painter that runs the renderer's early stages on the processor, as the app's does on
/// Windows' software graphics and an Intel Mac's, here whatever this computer's graphics are.
fn easel_with_early_stages_on_processor() -> Easel {
    let mut easel = Easel::new();
    easel.painter = Painter::with_early_stages_on_processor(&easel.device).expect("painter");
    easel
}

/// With the early stages on the processor the pictures are the ones the GPU works out by
/// itself: words, a dissolve and a wipe half-way through, shapes, and words over a picture.
#[test]
fn the_early_stages_on_the_processor_draw_the_same_pictures() {
    let mut on_gpu = Easel::new();
    let mut on_processor = easel_with_early_stages_on_processor();
    for (w, h) in [(1280, 720), (1920, 1080)] {
        for (name, scene) in heavy_scenes(w, h, 1) {
            let expected = on_gpu.paint(&scene, w, h).expect("paint");
            let got = on_processor.paint(&scene, w, h).expect("paint with the early stages on the processor");
            assert!(count_lit(&got) > 1_000 || name == "shapes", "{name} at {w}x{h} is drawn");
            assert_same_picture(&format!("{name} at {w}x{h}"), &got, &expected);
        }
    }
    // The GPU memory the last stage's buffers take is used again by the next picture, not
    // taken anew: over many pictures it never grows (what the lighter pictures above left spare
    // is given back meanwhile).
    let (_, scene) = heavy_scenes(1920, 1080, 1).swap_remove(0);
    on_processor.paint(&scene, 1920, 1080).expect("paint");
    let kept = on_processor.painter.spare_bytes();
    for _ in 0..10 {
        on_processor.paint(&scene, 1920, 1080).expect("paint");
        let now = on_processor.painter.spare_bytes();
        assert!(now <= kept, "no more memory after more pictures: {now} bytes, {kept} before");
    }
    eprintln!("kept between pictures with the early stages on the processor: {} MB", kept >> 20);
}

/// The painter keeps the stages' large working buffers from one picture to the next there, made
/// zero again only as far as the stages' own counters say they wrote. So a light picture after
/// a heavy one, one after a rest (which lets the buffers go) and the same one twice running are
/// still the pictures the GPU works out by itself: nothing of an earlier picture is left in them.
#[test]
fn pictures_one_after_another_on_the_processor_leave_nothing_behind() {
    let mut on_gpu = Easel::new();
    let mut on_processor = easel_with_early_stages_on_processor();
    let (w, h) = (2560, 1440);
    let scenes = heavy_scenes(w, h, 2);
    let expected: Vec<Vec<u8>> = scenes.iter().map(|(_, scene)| on_gpu.paint(scene, w, h).expect("paint")).collect();
    // Heaviest first (a dissolve of two readings), then lighter and heavier ones in turn.
    for (step, at) in [1, 3, 1, 1, 4, 0, 3, 2, 3].into_iter().enumerate() {
        if step == 5 {
            while !on_processor.painter.rest(&on_processor.device, &on_processor.queue) {
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        }
        let (name, scene) = &scenes[at];
        let got = on_processor.paint(scene, w, h).expect("paint with the early stages on the processor");
        assert_same_picture(&format!("{name}, picture {step} of the run"), &got, &expected[at]);
    }
}

/// A slide that outgrows the fixed working memory the processor's stages have is refused, and
/// says so, where it would otherwise end the thread that draws it; asked for again and again
/// it is soon refused at once; and the painter draws the next slides, and their picture, as if
/// nothing had happened.
#[test]
fn a_slide_too_heavy_for_the_processor_stages_is_refused_and_the_next_one_is_drawn() {
    let mut on_gpu = Easel::new();
    let mut on_processor = easel_with_early_stages_on_processor();
    let light = heavy_scenes(1280, 720, 1);
    let expected: Vec<Vec<u8>> =
        light.iter().map(|(_, scene)| on_gpu.paint(scene, 1280, 720).expect("paint")).collect();
    // The picture is in the painter's atlas before the heavy slide is asked for.
    let before = on_processor.paint(&light[4].1, 1280, 720).expect("paint");
    assert_same_picture("photo before", &before, &expected[4]);

    let (w, h) = (3840, 2160);
    // A full reading over a photograph: its picture is on its way to the painter's atlas when
    // the stages run out of room.
    let (_, too_heavy) = heavy_scenes(w, h, 9).swap_remove(4);
    let texture = target_texture(&on_processor.device, w, h);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let refuse = |easel: &mut Easel| easel.painter.paint(&easel.device, &easel.queue, &too_heavy, &view, w, h);
    let asked = std::time::Instant::now();
    assert_eq!(refuse(&mut on_processor), Err(sp_render::PaintError::TooHeavy), "too heavy, and says so");
    let worked_out = asked.elapsed();
    // Its picture has a new place in the atlas after that (the pictures were forgotten), so
    // it is a scene not seen before once more, and no more than once.
    assert_eq!(refuse(&mut on_processor), Err(sp_render::PaintError::TooHeavy), "still too heavy");
    let asked_again = std::time::Instant::now();
    assert_eq!(refuse(&mut on_processor), Err(sp_render::PaintError::TooHeavy), "and again");
    let at_once = asked_again.elapsed();

    for ((name, scene), expected) in light.iter().zip(&expected) {
        let got = on_processor.paint(scene, 1280, 720).expect("paint after a slide that was refused");
        assert_same_picture(&format!("{name} after the refused slide"), &got, expected);
    }
    // Not worked out again and again: that ends in a panic that is caught, which the program's
    // log hears of each time.
    assert!(at_once < worked_out / 2, "refused at once by then: {at_once:?} after {worked_out:?}");
}
