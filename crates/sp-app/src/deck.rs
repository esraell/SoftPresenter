//! Slides made from a presentation file read directly (a `.pptx`, see [`crate::pptx`]), or from
//! pictures: the pages of a PDF, a presentation file turned into a PDF by the program that made
//! it (see [`crate::convert`]), or a folder of pictures exported from one. Each page or picture
//! becomes a slide showing it across the whole slide, in a new presentation named after the
//! file or folder. Drawing a deck's pages takes a few seconds, so it is done on a thread of its
//! own: the operator's window and keys go on working meanwhile, even mid-service.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, Sender, channel};

use sp_model::{Group, Object, Presentation, SlideRect, Uuid};

/// A PDF, whose pages become slides.
pub const PDF_ENDING: &str = "pdf";
/// Files presentation programs save, turned into a PDF by a program on this Mac first.
pub const DECK_ENDINGS: [&str; 6] = ["pptx", "ppt", "pps", "ppsx", "key", "odp"];
/// The most pages or pictures a deck makes slides of.
const MOST_SLIDES: usize = 300;
/// The largest PDF read.
const LARGEST_PDF: u64 = 512 * 1024 * 1024;
/// The size each page is drawn at, at most: a full 1080p screen.
const PAGE_SIZE: (u32, u32) = (1920, 1080);

fn has_ending(path: &Path, endings: &[&str]) -> bool {
    path.extension().is_some_and(|ext| endings.iter().any(|e| ext.eq_ignore_ascii_case(e)))
}

/// True for a PDF.
pub fn is_pdf(path: &Path) -> bool {
    has_ending(path, &[PDF_ENDING])
}

/// True for a file a presentation program saved (a `.pptx`, `.ppsx`, `.ppt`, `.pps`, `.key` or `.odp`).
pub fn is_presentation_file(path: &Path) -> bool {
    has_ending(path, &DECK_ENDINGS)
}

/// The pictures directly inside a folder, in the order people number them (Slide2 before
/// Slide10), or `None` when there are none or the folder holds songs, which go in as songs.
pub fn pictures_in(folder: &Path) -> Option<Vec<PathBuf>> {
    let mut pictures = Vec::new();
    for entry in std::fs::read_dir(folder).ok()?.flatten() {
        let path = entry.path();
        let hidden = path.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.'));
        if hidden || !path.is_file() {
            continue;
        }
        if crate::load::is_importable(&path) || crate::load::is_song_file(&path) {
            return None;
        }
        if has_ending(&path, &crate::media::PICTURES) {
            pictures.push(path);
        }
    }
    pictures.sort_by(|a, b| natural_order(&a.to_string_lossy(), &b.to_string_lossy()));
    (!pictures.is_empty()).then_some(pictures)
}

/// Compares names the way people number things: runs of digits by their value, so "Slide2"
/// comes before "Slide10"; letters without regard to case.
pub fn natural_order(a: &str, b: &str) -> std::cmp::Ordering {
    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (None, Some(_)) => return std::cmp::Ordering::Less,
            (Some(_), None) => return std::cmp::Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let number = |chars: &mut std::iter::Peekable<std::str::Chars>| {
                    let mut digits = String::new();
                    while let Some(c) = chars.peek().copied().filter(char::is_ascii_digit) {
                        digits.push(c);
                        chars.next();
                    }
                    digits.trim_start_matches('0').to_owned()
                };
                let (x, y) = (number(&mut a), number(&mut b));
                let by_value = x.len().cmp(&y.len()).then_with(|| x.cmp(&y));
                if by_value != std::cmp::Ordering::Equal {
                    return by_value;
                }
            }
            (Some(x), Some(y)) => {
                let by_letter = x.to_lowercase().cmp(y.to_lowercase());
                if by_letter != std::cmp::Ordering::Equal {
                    return by_letter;
                }
                a.next();
                b.next();
            }
        }
    }
}

/// A slide showing one picture across the whole slide.
fn picture_slide(file: String, name: String, width: u32, height: u32) -> sp_model::Slide {
    let mut picture = Object::image(file, name, width, height);
    picture.frame = Some(SlideRect { x: 0.0, y: 0.0, w: sp_model::SLIDE_WIDTH, h: sp_model::SLIDE_HEIGHT });
    sp_model::Slide::new(vec![picture])
}

/// A presentation of slides, named.
fn presentation(title: String, slides: Vec<sp_model::Slide>) -> Presentation {
    Presentation {
        id: Uuid::new_v4(),
        title,
        groups: vec![Group { id: Uuid::new_v4(), name: String::new(), color: sp_model::group_color(""), slides }],
        copyright: sp_model::Copyright::default(),
        theme: None,
        arrangements: Vec::new(),
        arrangement: None,
        loops: false,
        transition: None,
    }
}

/// How a presentation file comes in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportAs {
    /// Slides that can be edited, read from the file directly (`.pptx` and `.ppsx` only).
    Slides,
    /// Pictures of the slides, drawn by a program on this Mac that opens the file: the exact look.
    Pictures,
}

impl ImportAs {
    pub fn name(self) -> &'static str {
        match self {
            ImportAs::Slides => "slides",
            ImportAs::Pictures => "pictures",
        }
    }

    pub fn from_name(name: &str) -> Option<ImportAs> {
        match name.trim().to_lowercase().as_str() {
            "slides" => Some(ImportAs::Slides),
            "pictures" => Some(ImportAs::Pictures),
            _ => None,
        }
    }
}

/// The kind of presentation file this is, for remembering how its kind is brought in: `pptx`
/// (which `.ppsx` is too), `ppt` (and `.pps`), `odp` or `key`.
pub fn kind_of(path: &Path) -> &'static str {
    let ending = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    match ending.as_str() {
        "pptx" | "ppsx" => "pptx",
        "ppt" | "pps" => "ppt",
        "odp" => "odp",
        _ => "key",
    }
}

/// True when this file can be read directly into slides: `.pptx` and `.ppsx`. The others come
/// in as pictures only.
pub fn reads_as_slides(path: &Path) -> bool {
    kind_of(path) == "pptx"
}

/// What a deck is made from.
#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    /// A PDF, or a folder of pictures.
    File(PathBuf),
    /// Pictures chosen together, as slides in this order, under this title.
    Pictures { title: String, files: Vec<PathBuf> },
    /// A presentation file, brought in as slides or as pictures.
    Presentation { path: PathBuf, mode: ImportAs },
}

impl Source {
    /// What the deck is known by while it is being made.
    fn key(&self) -> PathBuf {
        match self {
            Source::File(path) | Source::Presentation { path, .. } => path.clone(),
            Source::Pictures { files, .. } => files.first().cloned().unwrap_or_default(),
        }
    }

    /// The name of the file or folder, for messages.
    pub fn name(&self) -> String {
        match self {
            Source::File(path) | Source::Presentation { path, .. } => {
                path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned())
            }
            Source::Pictures { title, .. } => title.clone(),
        }
    }
}

/// Pictures chosen together, as one deck: in the order people number them, named after their
/// folder.
pub fn chosen_pictures(mut files: Vec<PathBuf>) -> Source {
    files.sort_by(|a, b| natural_order(&a.to_string_lossy(), &b.to_string_lossy()));
    let title = files
        .first()
        .and_then(|f| f.parent())
        .and_then(Path::file_name)
        .map_or_else(|| "Slides".to_owned(), |n| n.to_string_lossy().into_owned());
    Source::Pictures { title, files }
}

/// A deck laid out: its presentation, what to say about it, and the work still to do.
pub struct Laid {
    pub presentation: Presentation,
    /// What to tell the person when it is not the usual "Made N slides from X" (a presentation
    /// read as slides names the ones to check).
    pub said: Option<String>,
    pending: Pending,
}

/// A deck made.
pub struct Finished {
    pub presentation: Presentation,
    pub said: Option<String>,
}

/// The pictures of a laid out deck that are yet to be made, each for a slide.
enum Pending {
    Nothing,
    Pdf { path: PathBuf, files: Vec<(Uuid, String)>, then_delete: Option<PathBuf> },
    Copies { files: Vec<(PathBuf, Uuid, String)> },
}

/// One slide's picture coming into being after its deck was laid out, or the end of that.
#[derive(Clone, Debug, PartialEq)]
pub enum Fill {
    /// This slide's picture is in its file now.
    Page { slide: Uuid, file: String },
    /// Every picture has been tried; this many could not be made.
    Done { key: PathBuf, name: String, failed: usize },
}

/// Lays out the presentation for a deck, on the calling thread, quickly: its slides are there
/// and each picture's file has its name, but a PDF's pages or a folder's pictures are made
/// afterwards by [`fill`]. A presentation file read directly is whole at once.
pub fn lay_out(source: &Source) -> Result<Laid, String> {
    let (title, pictures) = match source {
        Source::Pictures { title, files } => (title.clone(), files.clone()),
        Source::File(path) if path.is_dir() => {
            let title = path.file_name().map_or_else(|| "Slides".to_owned(), |n| n.to_string_lossy().into_owned());
            let pictures = pictures_in(path).ok_or_else(|| format!("{title} has no pictures in it."))?;
            (title, pictures)
        }
        Source::Presentation { path, mode: ImportAs::Slides } => return lay_out_pptx(path),
        Source::Presentation { path, mode: ImportAs::Pictures } => return lay_out_by_program(path),
        Source::File(path) => {
            let title = path.file_stem().map_or_else(|| "Slides".to_owned(), |s| s.to_string_lossy().into_owned());
            return lay_out_pdf(path, title, None);
        }
    };
    let mut slides = Vec::new();
    let mut files = Vec::new();
    for picture in pictures.iter().take(MOST_SLIDES) {
        let (width, height) = crate::images::measure(picture)?;
        let ending =
            picture.extension().map_or_else(String::new, |e| format!(".{}", e.to_string_lossy().to_lowercase()));
        let file = format!("{}{ending}", Uuid::new_v4());
        let name = picture.file_stem().map_or_else(|| "Picture".to_owned(), |stem| stem.to_string_lossy().into_owned());
        let slide = picture_slide(file.clone(), name, width, height);
        files.push((picture.clone(), slide.id, file));
        slides.push(slide);
    }
    Ok(Laid { presentation: presentation(title, slides), said: None, pending: Pending::Copies { files } })
}

/// Makes the pictures a laid out deck waits for, as many at once as the computer has cores,
/// and says as each is done. Returns how many could not be made.
pub fn fill(laid_pending: LaidPending, key: PathBuf, name: &str, each: &(dyn Fn(Fill) + Sync)) {
    let folder = crate::images::folder().filter(|folder| std::fs::create_dir_all(folder).is_ok());
    let failed = match (laid_pending.0, folder) {
        (Pending::Nothing, _) => 0,
        (Pending::Pdf { path, files, then_delete }, folder) => {
            let failed = folder.map_or(files.len(), |folder| fill_pdf(&path, &files, &folder, each));
            if let Some(work) = then_delete {
                let _ = std::fs::remove_dir_all(work);
            }
            failed
        }
        (Pending::Copies { files }, folder) => folder.map_or(files.len(), |folder| fill_copies(&files, &folder, each)),
    };
    each(Fill::Done { key, name: name.to_owned(), failed });
}

/// What [`lay_out`] leaves for [`fill`].
pub struct LaidPending(Pending);

impl Laid {
    /// Splits the work still to do from the presentation and its note.
    pub fn split(self) -> (Finished, LaidPending) {
        (Finished { presentation: self.presentation, said: self.said }, LaidPending(self.pending))
    }
}

/// How many threads draw at once.
fn threads() -> usize {
    std::thread::available_parallelism().map_or(2, |n| n.get()).clamp(2, 8)
}

fn fill_pdf(path: &Path, files: &[(Uuid, String)], folder: &Path, each: &(dyn Fn(Fill) + Sync)) -> usize {
    let made = std::sync::atomic::AtomicUsize::new(0);
    let drawn = sp_video::pdf_draw(path, PAGE_SIZE.0, PAGE_SIZE.1, MOST_SLIDES, threads(), |number, frame| {
        let Some((slide, file)) = files.get(number - 1) else { return true };
        if save_png(&folder.join(file), &frame).is_ok() {
            made.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            each(Fill::Page { slide: *slide, file: file.clone() });
        }
        true
    });
    let _ = drawn;
    files.len().saturating_sub(made.load(std::sync::atomic::Ordering::Relaxed))
}

fn fill_copies(files: &[(PathBuf, Uuid, String)], folder: &Path, each: &(dyn Fn(Fill) + Sync)) -> usize {
    let next = std::sync::atomic::AtomicUsize::new(0);
    let failed = std::sync::atomic::AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..threads().min(files.len().max(1)) {
            scope.spawn(|| {
                use std::sync::atomic::Ordering::Relaxed;
                loop {
                    let at = next.fetch_add(1, Relaxed);
                    let Some((from, slide, file)) = files.get(at) else { return };
                    match crate::images::copy_into(from, folder, file) {
                        Ok(()) => each(Fill::Page { slide: *slide, file: file.clone() }),
                        Err(_) => {
                            failed.fetch_add(1, Relaxed);
                        }
                    }
                }
            });
        }
    });
    failed.load(std::sync::atomic::Ordering::Relaxed)
}

/// Makes the presentation for a deck whole, on the calling thread (every picture is made before
/// it returns). Each page or picture is saved in the Images folder, so the deck keeps working
/// when the file is moved or deleted.
pub fn make(source: &Source) -> Result<Finished, String> {
    let (finished, pending) = lay_out(source)?.split();
    fill(pending, source.key(), &source.name(), &|_| {});
    Ok(finished)
}

/// A `.pptx` read directly into slides.
fn lay_out_pptx(path: &Path) -> Result<Laid, String> {
    let name = path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let title = path.file_stem().map_or_else(|| "Slides".to_owned(), |s| s.to_string_lossy().into_owned());
    let size = std::fs::metadata(path).map_err(|e| format!("{name} could not be read ({e})."))?.len();
    if size > crate::pptx::LARGEST_FILE {
        return Err(format!("{name} is larger than any presentation, so it wasn't read."));
    }
    let bytes = std::fs::read(path).map_err(|e| format!("{name} could not be read ({e})."))?;
    let folder = crate::images::folder().ok_or("Could not find where to keep pictures on this computer.")?;
    std::fs::create_dir_all(&folder).map_err(|e| format!("Could not make the Images folder: {e}"))?;
    let mut save = |bytes: &[u8], ending: &str, crop: Option<[f32; 4]>| keep_picture(&folder, bytes, ending, crop);
    let read = crate::pptx::read(&bytes, &name, MOST_SLIDES, &mut save)?;
    let mut slides = read.slides;
    mark_to_check(&mut slides, &read.check);
    let said = Some(came_in_note(&title, slides.len(), &read.check));
    Ok(Laid { presentation: presentation(title, slides), said, pending: Pending::Nothing })
}

/// Gives each slide named its reasons to be looked at, kept with it until it is marked as checked.
pub fn mark_to_check(slides: &mut [sp_model::Slide], check: &[(usize, Vec<String>)]) {
    for (number, reasons) in check {
        if let Some(slide) = number.checked_sub(1).and_then(|at| slides.get_mut(at)) {
            slide.check = Some(reasons.join("; "));
        }
    }
}

/// What the notice says when a presentation came in as slides: how many, and which of them to
/// check, with why: "“Sermon” came in as 24 slides. Check slides 3 and 7: a chart came in as
/// its picture; a text box had mixed sizes; it took its main look." Reasons are named once
/// however many slides have them; the slide's own tag says which it has.
pub fn came_in_note(title: &str, count: usize, check: &[(usize, Vec<String>)]) -> String {
    let plural = if count == 1 { "" } else { "s" };
    let mut said = format!("\u{201c}{title}\u{201d} came in as {count} slide{plural}.");
    if check.is_empty() {
        return said;
    }
    const NAMED: usize = 8;
    let numbers: Vec<String> = check.iter().take(NAMED).map(|(number, _)| number.to_string()).collect();
    let more = check.len().saturating_sub(NAMED);
    let which = match (numbers.as_slice(), more) {
        ([one], 0) => one.clone(),
        (numbers, 0) => format!("{} and {}", numbers[..numbers.len() - 1].join(", "), numbers[numbers.len() - 1]),
        (numbers, more) => format!("{} and {more} more", numbers.join(", ")),
    };
    let mut reasons: Vec<&str> = Vec::new();
    for reason in check.iter().flat_map(|(_, why)| why.iter()) {
        if !reasons.contains(&reason.as_str()) {
            reasons.push(reason);
        }
    }
    let plural = if check.len() == 1 { "" } else { "s" };
    said.push_str(&format!(" Check slide{plural} {which}: {}.", reasons.join("; ")));
    said
}

/// The most pixels a picture taken from a presentation may have.
const MOST_PIXELS: u64 = 64 * 1024 * 1024;

/// Keeps a picture from a presentation file in the Images folder, optionally cropped to the
/// part the slide shows, and says its name and size. A picture with too many pixels, or in a
/// form the app doesn't read, is refused.
fn keep_picture(
    folder: &Path,
    bytes: &[u8],
    ending: &str,
    crop: Option<[f32; 4]>,
) -> Result<(String, u32, u32), String> {
    let ending = match ending {
        "jpeg" => "jpg",
        "png" | "jpg" | "gif" | "bmp" | "webp" => ending,
        _ => return Err("not a picture format the app reads".to_owned()),
    };
    let reader =
        image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format().map_err(|e| e.to_string())?;
    let (width, height) = reader.into_dimensions().map_err(|e| e.to_string())?;
    if u64::from(width) * u64::from(height) > MOST_PIXELS {
        return Err("too many pixels".to_owned());
    }
    let id = Uuid::new_v4();
    let Some([left, top, right, bottom]) = crop else {
        let file = format!("{id}.{ending}");
        std::fs::write(folder.join(&file), bytes).map_err(|e| e.to_string())?;
        return Ok((file, width, height));
    };
    let picture = image::load_from_memory(bytes).map_err(|e| e.to_string())?;
    let (x, y) = ((left * width as f32) as u32, (top * height as f32) as u32);
    let (w, h) = (
        (((1.0 - left - right) * width as f32) as u32).max(1).min(width - x.min(width - 1)),
        (((1.0 - top - bottom) * height as f32) as u32).max(1).min(height - y.min(height - 1)),
    );
    let cropped = picture.crop_imm(x.min(width - 1), y.min(height - 1), w, h);
    let file = format!("{id}.png");
    cropped.save(folder.join(&file)).map_err(|e| e.to_string())?;
    Ok((file, cropped.width(), cropped.height()))
}

/// A presentation file's slides as pictures, through a PDF made by the program that opens it
/// (without showing it). The PDF is deleted once its pages are drawn; the slides' pictures stay
/// in the Images folder.
fn lay_out_by_program(path: &Path) -> Result<Laid, String> {
    let title = path.file_stem().map_or_else(|| "Slides".to_owned(), |s| s.to_string_lossy().into_owned());
    let work = crate::bible_store::data_dir()
        .ok_or("Could not find where to work on this computer.")?
        .join("Converting")
        .join(Uuid::new_v4().to_string());
    match crate::convert::to_pdf(path, &work) {
        Ok((pdf, _)) => lay_out_pdf(&pdf, title, Some(work.clone())).inspect_err(|_| {
            let _ = std::fs::remove_dir_all(&work);
        }),
        Err(problem) => {
            let _ = std::fs::remove_dir_all(&work);
            Err(problem)
        }
    }
}

/// A PDF's pages as slides, in a presentation with this title; their pictures are drawn by
/// [`fill`]. `then_delete` is a folder removed once they are.
fn lay_out_pdf(path: &Path, title: String, then_delete: Option<PathBuf>) -> Result<Laid, String> {
    let size = std::fs::metadata(path).map_err(|e| format!("{title} could not be read ({e}).")).map(|m| m.len())?;
    if size > LARGEST_PDF {
        return Err(format!("{title} is too large to make slides of."));
    }
    let sizes = sp_video::pdf_sizes(path, PAGE_SIZE.0, PAGE_SIZE.1, MOST_SLIDES)?;
    let mut slides = Vec::new();
    let mut files = Vec::new();
    for (at, (width, height)) in sizes.iter().enumerate() {
        let file = format!("{}.png", Uuid::new_v4());
        let slide = picture_slide(file.clone(), format!("{title} {}", at + 1), *width, *height);
        files.push((slide.id, file));
        slides.push(slide);
    }
    Ok(Laid {
        presentation: presentation(title, slides),
        said: None,
        pending: Pending::Pdf { path: path.to_owned(), files, then_delete },
    })
}

/// Saves a drawn page as a PNG, without its alpha (a page is opaque), compressed quickly.
fn save_png(path: &Path, frame: &sp_video::Frame) -> Result<(), String> {
    use image::ImageEncoder;
    let rgb: Vec<u8> = frame.rgba.as_chunks::<4>().0.iter().flat_map(|&[r, g, b, _]| [r, g, b]).collect();
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let encoder = image::codecs::png::PngEncoder::new_with_quality(
        std::io::BufWriter::new(file),
        image::codecs::png::CompressionType::Fast,
        image::codecs::png::FilterType::Adaptive,
    );
    encoder.write_image(&rgb, frame.width, frame.height, image::ExtendedColorType::Rgb8).map_err(|e| e.to_string())
}

/// What one deck being made came to.
pub type Made = (PathBuf, Result<Finished, String>);

/// Decks being made on threads of their own: each is laid out and handed over at once, and its
/// pictures then fill in, a slide at a time, as they are drawn.
pub struct Decks {
    tx: Sender<Made>,
    rx: Receiver<Made>,
    fills_tx: Sender<Fill>,
    fills_rx: Receiver<Fill>,
    busy: Vec<PathBuf>,
}

impl Default for Decks {
    fn default() -> Self {
        let (tx, rx) = channel();
        let (fills_tx, fills_rx) = channel();
        Decks { tx, rx, fills_tx, fills_rx, busy: Vec::new() }
    }
}

impl Decks {
    /// Starts making a deck; `wake` is called when it is laid out and as each of its pictures is
    /// made, to redraw the window. A deck already being made isn't started twice.
    pub fn start(&mut self, source: Source, wake: impl Fn() + Send + Sync + 'static) {
        let path = source.key();
        if self.busy.contains(&path) {
            return;
        }
        self.busy.push(path.clone());
        let (tx, fills) = (self.tx.clone(), Mutex::new(self.fills_tx.clone()));
        let work = {
            let path = path.clone();
            move || match lay_out(&source) {
                Ok(laid) => {
                    let (finished, pending) = laid.split();
                    let _ = tx.send((path.clone(), Ok(finished)));
                    wake();
                    fill(pending, path.clone(), &source.name(), &|made| {
                        if let Ok(fills) = fills.lock() {
                            let _ = fills.send(made);
                        }
                        wake();
                    });
                }
                Err(problem) => {
                    let _ = tx.send((path, Err(problem)));
                    wake();
                }
            }
        };
        if std::thread::Builder::new().name("making slides".to_owned()).spawn(work).is_err() {
            self.busy.retain(|busy| *busy != path);
        }
    }

    /// The decks laid out since the last look.
    pub fn finished(&mut self) -> Vec<Made> {
        let done: Vec<Made> = self.rx.try_iter().collect();
        for (path, made) in &done {
            // A deck that came to nothing is over; one laid out is still filling in, and is
            // over when its fill says so.
            if made.is_err() {
                self.busy.retain(|busy| busy != path);
            }
        }
        done
    }

    /// The pictures made since the last look, and the decks that are now complete.
    pub fn filled(&mut self) -> Vec<Fill> {
        let fills: Vec<Fill> = self.fills_rx.try_iter().collect();
        for fill in &fills {
            if let Fill::Done { key, .. } = fill {
                self.busy.retain(|busy| busy != key);
            }
        }
        fills
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sp_model::ObjectKind;

    #[test]
    fn names_are_put_in_the_order_people_number_them() {
        let mut names = vec!["Slide10.png", "slide2.png", "Slide1.png", "Slide02b.png", "Intro.png"];
        names.sort_by(|a, b| natural_order(a, b));
        assert_eq!(names, ["Intro.png", "Slide1.png", "slide2.png", "Slide02b.png", "Slide10.png"]);
    }

    #[test]
    fn presentation_files_and_pdfs_are_told_apart() {
        assert!(is_presentation_file(Path::new("Sermon.PPTX")) && is_presentation_file(Path::new("Talk.key")));
        assert!(!is_presentation_file(Path::new("Sermon.pdf")) && is_pdf(Path::new("Sermon.PDF")));
    }

    #[test]
    fn a_folder_of_pictures_is_a_deck_but_one_with_songs_is_not() {
        let folder = std::env::temp_dir().join(format!("softpresenter-deck-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        for name in ["Slide10.png", "Slide2.png", ".hidden.png", "notes.md"] {
            std::fs::write(folder.join(name), b"x").unwrap();
        }
        let found = pictures_in(&folder).unwrap();
        let names: Vec<_> = found.iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(names, ["Slide2.png", "Slide10.png"]);
        std::fs::write(folder.join("song.txt"), b"words").unwrap();
        assert!(pictures_in(&folder).is_none(), "a folder with songs in it goes in as songs");
        let _ = std::fs::remove_dir_all(&folder);
    }

    fn temp(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("softpresenter-deck-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    fn png(width: u32, height: u32, depth: png::BitDepth, color: png::ColorType, data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(color);
        encoder.set_depth(depth);
        encoder.write_header().unwrap().write_image_data(data).unwrap();
        out
    }

    fn rgb_png(width: u32, height: u32) -> Vec<u8> {
        png(width, height, png::BitDepth::Eight, png::ColorType::Rgb, &vec![120; (width * height * 3) as usize])
    }

    /// A PDF with a page of each size (in points), each with a rectangle on it.
    #[cfg(target_os = "macos")]
    fn pdf_of(sizes: &[(u32, u32)]) -> Vec<u8> {
        let mut objects: Vec<Vec<u8>> = vec![Vec::new()];
        let mut kids = Vec::new();
        for (at, (w, h)) in sizes.iter().enumerate() {
            let drawing = format!("0.2 0.4 0.8 rg 10 10 {} {} re f", w / 2, h / 2);
            objects.push(format!("<< /Length {} >>\nstream\n{drawing}\nendstream", drawing.len()).into_bytes());
            let content = objects.len();
            objects.push(
                format!("<< /Type /Page /Parent 1 0 R /MediaBox [0 0 {w} {h}] /Contents {content} 0 R >>").into_bytes(),
            );
            kids.push(format!("{} 0 R", objects.len()));
            let _ = at;
        }
        objects[0] = format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids.join(" "), sizes.len()).into_bytes();
        objects.push(b"<< /Type /Catalog /Pages 1 0 R >>".to_vec());
        let mut out = b"%PDF-1.4\n".to_vec();
        let mut at = Vec::new();
        for (n, body) in objects.iter().enumerate() {
            at.push(out.len());
            out.extend(format!("{} 0 obj\n", n + 1).bytes());
            out.extend(body);
            out.extend(b"\nendobj\n");
        }
        let xref = out.len();
        out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).bytes());
        for place in at {
            out.extend(format!("{place:010} 00000 n \n").bytes());
        }
        out.extend(
            format!(
                "trailer\n<< /Size {} /Root {} 0 R >>\nstartxref\n{xref}\n%%EOF\n",
                objects.len() + 1,
                objects.len()
            )
            .bytes(),
        );
        out
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_pdf_is_laid_out_at_once_and_every_page_is_drawn_in_its_place() {
        let folder = temp("pdf");
        // Twelve pages, alternately wide, tall and square, so a page in the wrong place shows.
        let sizes: Vec<(u32, u32)> = (0..12).map(|n| [(960, 540), (540, 960), (600, 600)][n % 3]).collect();
        let pdf = folder.join("Twelve.pdf");
        std::fs::write(&pdf, pdf_of(&sizes)).unwrap();
        let laid = lay_out_pdf(&pdf, "Twelve".to_owned(), None).unwrap();
        let slides: Vec<_> = laid.presentation.slides().map(|(_, slide)| slide.clone()).collect();
        assert_eq!(slides.len(), 12, "the whole deck is there before any page is drawn");
        assert!(!folder.join("Images").exists());
        let Pending::Pdf { path, files, .. } = &laid.pending else { panic!("a PDF waits for its pages") };
        let images = folder.join("Images");
        std::fs::create_dir_all(&images).unwrap();
        let told = Mutex::new(Vec::new());
        let failed = fill_pdf(path, files, &images, &|fill| told.lock().unwrap().push(fill));
        assert_eq!(failed, 0);
        let told = told.into_inner().unwrap();
        assert_eq!(told.len(), 12);
        for (at, slide) in slides.iter().enumerate() {
            let ObjectKind::Image { file, .. } = &slide.objects[0].kind else { panic!("a picture slide") };
            let (w, h) = image::image_dimensions(images.join(file)).unwrap();
            // Fitted into 1920 x 1080 as the page's own shape says.
            let scale = (1920.0 / f64::from(sizes[at].0)).min(1080.0 / f64::from(sizes[at].1));
            assert_eq!(
                (w, h),
                ((f64::from(sizes[at].0) * scale).round() as u32, (f64::from(sizes[at].1) * scale).round() as u32),
                "page {} is in slide {}",
                at + 1,
                at + 1
            );
            assert!(told.contains(&Fill::Page { slide: slide.id, file: file.clone() }));
            assert_eq!(slide.objects[0].name, format!("Twelve {}", at + 1));
        }
        assert!(w_and_h_match(&laid.presentation, &images));
        let _ = std::fs::remove_dir_all(&folder);
    }

    /// The size each slide's picture says it has is the picture's.
    #[cfg(target_os = "macos")]
    fn w_and_h_match(deck: &Presentation, images: &Path) -> bool {
        deck.slides().all(|(_, slide)| {
            let ObjectKind::Image { file, .. } = &slide.objects[0].kind else { return false };
            image::image_dimensions(images.join(file)).is_ok()
        })
    }

    #[test]
    fn a_folder_of_pictures_is_laid_out_at_once_and_copied_in_parallel_in_order() {
        let folder = temp("pictures");
        let from = folder.join("Slides");
        std::fs::create_dir_all(&from).unwrap();
        for n in 1..=12_u32 {
            std::fs::write(from.join(format!("Slide{n}.png")), rgb_png(10 + n, 6)).unwrap();
        }
        let laid = lay_out(&Source::File(from)).unwrap();
        let slides: Vec<_> = laid.presentation.slides().map(|(_, slide)| slide.clone()).collect();
        assert_eq!(slides.len(), 12);
        let Pending::Copies { files } = &laid.pending else { panic!("pictures wait to be copied") };
        let to = folder.join("Images");
        std::fs::create_dir_all(&to).unwrap();
        assert_eq!(fill_copies(files, &to, &|_| {}), 0);
        for (at, slide) in slides.iter().enumerate() {
            let ObjectKind::Image { file, .. } = &slide.objects[0].kind else { panic!("a picture slide") };
            let (w, _) = image::image_dimensions(to.join(file)).unwrap();
            assert_eq!(w, 11 + at as u32, "Slide{} is the slide {}", at + 1, at + 1);
        }
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_picture_from_a_presentation_is_cropped_and_a_huge_or_odd_one_is_refused() {
        let folder = temp("keep");
        let whole = rgb_png(4, 4);
        let (file, w, h) = keep_picture(&folder, &whole, "png", None).unwrap();
        assert_eq!((w, h), (4, 4));
        assert_eq!(std::fs::read(folder.join(&file)).unwrap(), whole, "kept as it came");
        let (file, w, h) = keep_picture(&folder, &whole, "png", Some([0.5, 0.0, 0.0, 0.25])).unwrap();
        assert_eq!((w, h), (2, 3));
        assert_eq!(image::image_dimensions(folder.join(&file)).unwrap(), (2, 3));
        // A picture that says it has more pixels than the limit is refused before it is decoded.
        let huge = png(10_000, 10_000, png::BitDepth::One, png::ColorType::Grayscale, &vec![0; 1250 * 10_000]);
        assert!(huge.len() < 100_000, "{} bytes", huge.len());
        assert!(keep_picture(&folder, &huge, "png", None).unwrap_err().contains("too many pixels"));
        assert!(keep_picture(&folder, &whole, "emf", None).is_err());
        assert!(keep_picture(&folder, b"not a picture", "png", None).is_err());
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn how_a_kind_of_file_comes_in_is_remembered_and_only_a_pptx_can_be_slides() {
        assert_eq!(ImportAs::from_name("Slides"), Some(ImportAs::Slides));
        assert_eq!(ImportAs::from_name("pictures"), Some(ImportAs::Pictures));
        assert_eq!(ImportAs::from_name("photos"), None);
        assert!(reads_as_slides(Path::new("a.PPTX")) && reads_as_slides(Path::new("a.ppsx")));
        assert!(
            !reads_as_slides(Path::new("a.key"))
                && !reads_as_slides(Path::new("a.odp"))
                && !reads_as_slides(Path::new("a.ppt"))
        );
        assert_eq!(
            (kind_of(Path::new("a.ppsx")), kind_of(Path::new("a.pps")), kind_of(Path::new("a.key"))),
            ("pptx", "ppt", "key")
        );

        let mut state = crate::home::HomeState::default();
        assert_eq!(state.import_choice("pptx"), ImportAs::Slides, "slides until told otherwise");
        state.remember_import("pptx", ImportAs::Pictures);
        // It is kept in the settings and comes back with them, for its own kind only.
        let saved = serde_json::to_string(&state.prefs()).unwrap();
        let mut next = crate::home::HomeState::default();
        next.apply_prefs(&serde_json::from_str(&saved).unwrap());
        assert_eq!(next.import_choice("pptx"), ImportAs::Pictures);
        assert_eq!(next.import_choice("odp"), ImportAs::Slides);
    }

    #[test]
    fn the_slides_to_check_are_named_in_a_sentence() {
        let none = came_in_note("Sermon", 24, &[]);
        assert_eq!(none, "\u{201c}Sermon\u{201d} came in as 24 slides.");
        let one = came_in_note("Talk", 1, &[(1, vec!["a chart came in as its picture".to_owned()])]);
        assert_eq!(one, "\u{201c}Talk\u{201d} came in as 1 slide. Check slide 1: a chart came in as its picture.");
        let chart = "a chart came in as its picture".to_owned();
        let mixed = vec!["a text box had mixed sizes; it took its main look".to_owned()];
        let said = came_in_note("Sermon", 24, &[(3, vec![chart.clone()]), (7, mixed.clone())]);
        assert_eq!(
            said,
            "\u{201c}Sermon\u{201d} came in as 24 slides. Check slides 3 and 7: a chart came in as its picture; \
             a text box had mixed sizes; it took its main look."
        );
        let many: Vec<(usize, Vec<String>)> = (1..=10).map(|n| (n, vec![chart.clone()])).collect();
        let said = came_in_note("Deck", 12, &many);
        assert!(
            said.contains("Check slides 1, 2, 3, 4, 5, 6, 7, 8 and 2 more: a chart came in as its picture."),
            "{said}"
        );
    }

    /// Testing aid: `SP_TEST_PDF=<file>` and `SOFTPRESENTER_DATA=<empty folder>`.
    #[test]
    #[ignore = "measures how long a PDF takes to become slides; needs SP_TEST_PDF"]
    fn time_a_pdf() {
        let Some(pdf) = std::env::var_os("SP_TEST_PDF") else { return };
        let started = std::time::Instant::now();
        let made = make(&Source::File(PathBuf::from(pdf))).unwrap();
        eprintln!("{} slides in {} ms", made.presentation.slide_count(), started.elapsed().as_millis());
    }
}
