//! Loading pictures and videos for the media bin.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::SystemTime;

use crate::i18n::{t, tf};
use sp_model::Uuid;
use sp_render::Picture;

/// Pictures larger than this on their longer side are scaled down when loaded, however large
/// the screens are. A phone photo kept at full size costs tens of megabytes.
pub const MAX_SIDE: u32 = 2560;

/// The largest a full picture is loaded, for a screen of 4K: a picture across a 3840 pixel wide
/// output needs that many. Larger screens are shown a picture scaled up from this.
const MOST_SIDE: u32 = 3840;

/// The longer side of the largest output opened so far, at most [`MOST_SIDE`]; never below
/// [`MAX_SIDE`], so a picture is never softer than before there was a 4K output to think of.
static SCREEN_SIDE: AtomicU32 = AtomicU32::new(MAX_SIDE);

/// Says how many pixels an output is, so pictures loaded from now on are sharp on it (up to
/// 4K). Pictures already kept stay as they are until they are read again.
pub fn note_screen(width: u32, height: u32) {
    raise(&SCREEN_SIDE, width, height);
}

fn raise(side: &AtomicU32, width: u32, height: u32) {
    side.fetch_max(width.max(height).min(MOST_SIDE), Ordering::Relaxed);
}

/// How large a picture for a screen is loaded: the longer side of the largest output so far.
pub fn screen_side() -> u32 {
    SCREEN_SIDE.load(Ordering::Relaxed)
}

/// A video's still is shown only until its first picture comes (and in thumbnails), so it is
/// kept smaller.
const VIDEO_STILL_SIDE: u32 = 1280;

/// How big the copy of a picture kept for thumbnails and the Program preview is. The media bin
/// keeps this copy of each item; the full-size picture is read when the item goes on screen
/// (see `bin_pictures`).
pub const SMALL_SIDE: u32 = 960;

/// Pictures decoded here.
const OWN_PICTURES: [&str; 6] = ["png", "jpg", "jpeg", "webp", "bmp", "gif"];

/// Pictures the computer's own system reads: iPhone photos, TIFF, Photoshop and others.
const SYSTEM_PICTURES: [&str; 7] = ["heic", "heif", "tif", "tiff", "psd", "avif", "jp2"];

/// Every picture's file ending, as the file chooser lists them.
pub const PICTURES: [&str; 13] =
    ["png", "jpg", "jpeg", "webp", "bmp", "gif", "heic", "heif", "tif", "tiff", "psd", "avif", "jp2"];

/// The file endings of everything the media bin takes, as the file chooser lists them.
pub const PICTURE_ENDINGS: [&str; 30] = [
    "png", "jpg", "jpeg", "webp", "bmp", "gif", "heic", "heif", "tif", "tiff", "psd", "avif", "jp2", "mp4", "mov",
    "m4v", "mkv", "webm", "avi", "wmv", "flv", "mpg", "mpeg", "m2ts", "mts", "ts", "ogv", "3gp", "vob", "divx",
];

fn list_file() -> Option<PathBuf> {
    crate::bible_store::data_dir().map(|dir| dir.join("media.json"))
}

/// A file in the media bin and how it is shown.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Entry {
    /// Kept between runs, so slide actions that show the item find it again.
    #[serde(default = "sp_model::Uuid::new_v4")]
    pub id: sp_model::Uuid,
    pub path: PathBuf,
    #[serde(default = "filled")]
    pub fit: sp_model::ImageFit,
    #[serde(default = "yes")]
    pub looping: bool,
    /// Brought into the bin by a media playlist following a folder (not added by hand): it
    /// leaves the bin when its file leaves the folder, unless something else uses it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub followed: bool,
    /// Which of the sample backgrounds it is, by its file name when it came ("Olive.jpg"); a
    /// file renamed later is still that sample. Missing from every other item, and from a list
    /// written before the samples.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample: Option<String>,
}

fn filled() -> sp_model::ImageFit {
    sp_model::ImageFit::Fill
}

fn yes() -> bool {
    true
}

/// A named group of the media bin's files, such as "Backgrounds" or "Announcements". A file
/// can be in any number of them.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Playlist {
    pub id: sp_model::Uuid,
    pub name: String,
    pub files: Vec<PathBuf>,
    /// A folder the playlist follows: the pictures and videos in it are its files, kept up to
    /// date as files come, go and change (such as an announcements video exported from Canva).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<PathBuf>,
    /// The one that gathers the sample backgrounds (see `samples`); adding them again puts the
    /// missing ones back into it, whatever it has been renamed.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub sample: bool,
}

impl Playlist {
    pub fn new(name: String) -> Playlist {
        Playlist { id: sp_model::Uuid::new_v4(), name, files: Vec::new(), folder: None, sample: false }
    }

    pub fn has(&self, path: &Path) -> bool {
        self.files.iter().any(|have| have == path)
    }

    /// Its order can be changed and files taken out of it here: not while it follows a folder,
    /// whose files (in name order) are what it holds.
    pub fn arranged_by_hand(&self) -> bool {
        self.folder.is_none()
    }

    /// Puts a file in, or takes it out if it is in.
    pub fn toggle(&mut self, path: &Path) {
        if self.has(path) {
            self.files.retain(|have| have != path);
        } else {
            self.files.push(path.to_owned());
        }
    }
}

/// The media bin places of a media playlist's files, in the playlist's order: the order it plays
/// in, and the order its own view in the media strip shows. Files no longer in the bin are left
/// out.
pub fn playlist_places(files: &[PathBuf], bin: &[sp_model::MediaItem]) -> Vec<usize> {
    files.iter().filter_map(|file| bin.iter().position(|item| &item.path == file)).collect()
}

/// A media playlist's files once `moving` (some of `shown`, the files its view shows, in order)
/// have been dragged to `slot` among `shown` (how many shown files are before the ink line).
/// They go there together, in the order they had; files not shown stay at the end.
pub fn moved_files(files: &[PathBuf], shown: &[PathBuf], moving: &[PathBuf], slot: usize) -> Vec<PathBuf> {
    let slot = slot.min(shown.len());
    let before = shown[..slot].iter().filter(|file| moving.contains(file)).count();
    let mut order: Vec<PathBuf> = shown.iter().filter(|file| !moving.contains(file)).cloned().collect();
    let carried: Vec<PathBuf> = shown.iter().filter(|file| moving.contains(file)).cloned().collect();
    let at = slot - before;
    order.splice(at..at, carried);
    order.extend(files.iter().filter(|file| !shown.contains(file)).cloned());
    order
}

/// Where a playing media playlist goes next, as a place among `items` (its bin places in its
/// order): after `shown`, the one on screen, wherever it is now; if it has been taken out, the
/// one that took its place (`at`, where it was).
pub fn next_in_cycle(items: &[usize], at: usize, shown: usize) -> usize {
    if items.is_empty() {
        return 0;
    }
    match items.iter().position(|item| *item == shown) {
        Some(now) => (now + 1) % items.len(),
        None => at % items.len(),
    }
}

/// Whether a playing media playlist is done with the video it shows, and goes on to its next
/// item: once the video has played to its end, once no picture of it has come within the most
/// a video's first picture is waited for (`video_wait`), or once it is known it can't play, so
/// a video that won't play never holds the playlist up. Only with `others` to go on to: one
/// alone loops, and is never done.
pub fn video_done(others: bool, ended: bool, overdue: bool, refused: bool) -> bool {
    others && (ended || overdue || refused)
}

/// A name no media playlist has: "New Media Playlist", "New Media Playlist 2", and so on
/// ("Playlist" alone is the running order).
pub fn new_playlist_name(playlists: &[Playlist]) -> String {
    let taken = |name: &str| playlists.iter().any(|playlist| playlist.name == name);
    let base = t("media.new_playlist.name", "New Media Playlist");
    std::iter::once(base.to_owned())
        .chain((2..).map(|n| format!("{base} {n}")))
        .find(|name| !taken(name))
        .unwrap_or_default()
}

/// The name a media item goes by where it is named in the media strip, a playlist's row and the
/// heading over it: the file's name without its ending ("Evening Hills"; the VIDEO badge or the
/// row's mark says what kind it is). Two files in the bin with the same name in two formats
/// ("Logo.png" and "Logo.jpg", compared ignoring case) keep their endings, to be told apart.
/// Menus and sheets, where a file is chosen, show the whole name.
pub fn label(name: &str, bin: &[sp_model::MediaItem]) -> String {
    let stem = |name: &str| -> String {
        match Path::new(name).file_stem() {
            Some(stem) => stem.to_string_lossy().into_owned(),
            None => name.to_owned(),
        }
    };
    let mine = stem(name).to_lowercase();
    let shared =
        bin.iter().any(|other| !other.name.eq_ignore_ascii_case(name) && stem(&other.name).to_lowercase() == mine);
    if shared { name.to_owned() } else { stem(name) }
}

/// `text` as it is if it fits (as `fits` says), else cut to what leaves room for "…" at the end:
/// one line, never cut more than it must.
pub fn ellipsize(text: &str, fits: impl Fn(&str) -> bool) -> String {
    if fits(text) {
        return text.to_owned();
    }
    for (end, _) in text.char_indices().rev() {
        let cut = format!("{}\u{2026}", text[..end].trim_end());
        if fits(&cut) {
            return cut;
        }
    }
    "\u{2026}".to_owned()
}

/// Where the media bin item at `at` is once the one at `removed` has been taken out: one place
/// up if it was after it, nowhere if it was that one.
pub fn place_after_removal(at: usize, removed: usize) -> Option<usize> {
    match at.cmp(&removed) {
        std::cmp::Ordering::Less => Some(at),
        std::cmp::Ordering::Equal => None,
        std::cmp::Ordering::Greater => Some(at - 1),
    }
}

/// The pictures and videos in a folder, by name, with when each last changed. Hidden files
/// (such as one still being downloaded) are left out. `None` when the folder can't be read (a
/// drive not plugged in, a share not connected), which is not the same as an empty folder.
pub fn folder_files(folder: &Path) -> Option<Vec<(PathBuf, SystemTime)>> {
    let entries = std::fs::read_dir(folder).ok()?;
    let mut files: Vec<(PathBuf, SystemTime)> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| is_media(path))
        .filter(|path| path.file_name().is_some_and(|name| !name.to_string_lossy().starts_with('.')))
        .filter_map(|path| {
            let changed = std::fs::metadata(&path).and_then(|meta| meta.modified()).ok()?;
            Some((path, changed))
        })
        .collect();
    files.sort_by(|a, b| a.0.cmp(&b.0));
    Some(files)
}

/// A look at a followed folder: the media playlist following it, the folder, and the files in
/// it (`None` when it couldn't be read).
pub type Listing = (Uuid, PathBuf, Option<Vec<(PathBuf, SystemTime)>>);

/// What a look at the followed folders asks for.
#[derive(Debug, Default, PartialEq)]
pub struct FolderChanges {
    /// Files new to the bin, to read before they go in, with when each last changed.
    pub read_new: Vec<(PathBuf, SystemTime)>,
    /// Bin items whose file was written over, to read again.
    pub read_again: Vec<Uuid>,
    /// Followed playlists whose files are now these.
    pub playlists: Vec<(Uuid, Vec<PathBuf>)>,
    /// Items a folder brought in whose file has left it, which nothing else uses: they leave the
    /// bin.
    pub leave: Vec<Uuid>,
}

/// What the app knows of the files in followed folders between looks.
#[derive(Debug, Default)]
pub struct FolderNotes {
    /// Each file in the bin with when it last changed, as last seen.
    pub seen: HashMap<PathBuf, SystemTime>,
    /// Files that couldn't be read, as they were then: tried again only once they change (a file
    /// still being written changes as it grows), never every few seconds.
    pub refused: HashMap<PathBuf, SystemTime>,
    /// Files new to the bin being read, by the id they will have, with when each last changed.
    pub coming: HashMap<Uuid, (PathBuf, SystemTime)>,
}

impl FolderNotes {
    /// Works out what a look at the followed folders changes. `bin` is the media bin's items,
    /// `brought` those a folder brought in, and `in_use` those used other than by a playlist (on
    /// a slide, in a playlist of the service, in a macro, or on screen). A folder that couldn't be
    /// read changes nothing, so a drive not plugged in doesn't empty its playlist or the bin.
    pub fn look(
        &mut self,
        listings: &[Listing],
        bin: &[(Uuid, PathBuf)],
        playlists: &[Playlist],
        brought: &HashSet<Uuid>,
        in_use: &HashSet<Uuid>,
    ) -> FolderChanges {
        let mut changes = FolderChanges::default();
        let coming: HashSet<&Path> = self.coming.values().map(|(path, _)| path.as_path()).collect();
        for (playlist, _, files) in listings {
            let Some(files) = files else { continue };
            for (path, changed) in files {
                match bin.iter().find(|(_, have)| have == path) {
                    Some((item, _)) => {
                        if self.seen.get(path).is_some_and(|seen| seen != changed) && !changes.read_again.contains(item)
                        {
                            changes.read_again.push(*item);
                        }
                        self.seen.insert(path.clone(), *changed);
                    }
                    None if self.refused.get(path) == Some(changed)
                        || coming.contains(path.as_path())
                        || changes.read_new.iter().any(|(new, _)| new == path) => {}
                    None => changes.read_new.push((path.clone(), *changed)),
                }
            }
            let now: Vec<PathBuf> = files.iter().map(|(path, _)| path.clone()).collect();
            if playlists.iter().any(|p| p.id == *playlist && p.files != now)
                && !changes.playlists.iter().any(|(id, _)| id == playlist)
            {
                changes.playlists.push((*playlist, now));
            }
        }
        // The playlists as they will be.
        let files_of = |playlist: &Playlist| {
            changes
                .playlists
                .iter()
                .find(|(id, _)| *id == playlist.id)
                .map_or(&playlist.files, |(_, files)| files)
                .clone()
        };
        let listed: Vec<Vec<PathBuf>> = playlists.iter().map(files_of).collect();
        // The files of each folder read just now.
        let folder_of = |path: &Path| {
            let parent = path.parent()?;
            listings.iter().find_map(|(_, folder, files)| files.as_ref().filter(|_| folder == parent))
        };
        for (item, path) in bin {
            if !brought.contains(item) || in_use.contains(item) {
                continue;
            }
            let Some(files) = folder_of(path) else { continue };
            if files.iter().any(|(have, _)| have == path) || listed.iter().any(|files| files.contains(path)) {
                continue;
            }
            changes.leave.push(*item);
        }
        // Notes of files gone from the folders read are let go; a folder that couldn't be read
        // keeps its notes until it can be.
        let gone = |path: &PathBuf| folder_of(path).is_some_and(|files| !files.iter().any(|(have, _)| have == path));
        self.seen.retain(|path, _| !gone(path));
        self.refused.retain(|path, _| !gone(path));
        changes
    }
}

/// What `media.json` keeps: the files in the media bin with how each is shown, and the
/// media playlists.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct List {
    pub entries: Vec<Entry>,
    #[serde(default)]
    pub playlists: Vec<Playlist>,
}

impl List {
    /// Reads the file's words. A file from before media settings is a plain list of files. The
    /// error, for the log, says why it isn't a media list.
    pub fn from_json(text: &str) -> Result<List, String> {
        if let Ok(paths) = serde_json::from_str::<Vec<PathBuf>>(text) {
            let entries = paths
                .into_iter()
                .map(|path| Entry {
                    id: sp_model::Uuid::new_v4(),
                    path,
                    fit: filled(),
                    looping: true,
                    followed: false,
                    sample: None,
                })
                .collect();
            return Ok(List { entries, playlists: Vec::new() });
        }
        serde_json::from_str(text).map_err(|e| e.to_string())
    }
}

/// The media bin as it was when the app last closed; and a file there that couldn't be read,
/// kept aside (`whole_file`).
pub fn load_list() -> (List, Option<crate::whole_file::Unread>) {
    list_file().map_or_else(|| (List::default(), None), |path| load_list_from(&path))
}

fn load_list_from(path: &Path) -> (List, Option<crate::whole_file::Unread>) {
    crate::whole_file::read_or_default(path, crate::whole_file::What::MediaList, List::from_json)
}

/// The media bin's file and what goes in it, for the library's writer to write (`writer`), so
/// the bin is as it was at the next start.
pub fn list_to_save(list: &List) -> Result<crate::writer::Save, String> {
    let path = list_file()
        .ok_or_else(|| t("media.no_place", "Could not find where to keep settings on this computer.").to_owned())?;
    Ok(list_to_save_at(path, list))
}

fn list_to_save_at(path: PathBuf, list: &List) -> crate::writer::Save {
    let text = serde_json::to_string_pretty(list).unwrap_or_default();
    let mut save =
        crate::writer::Save::new(path, crate::writer::Kind::MediaList, crate::writer::Guard::Any, move || {
            text.into_bytes()
        });
    save.make_folder = true;
    save
}

/// What the window says when the media bin couldn't be written.
pub fn list_not_saved(error: &std::io::Error) -> String {
    crate::whole_file::What::MediaList.not_saved(error)
}

/// Videos by their names. macOS's own player plays MP4 and MOV; FFmpeg plays the rest (and any
/// MP4 or MOV the system's player can't).
const VIDEOS: [&str; 17] = [
    "mp4", "mov", "m4v", "mkv", "webm", "avi", "wmv", "flv", "mpg", "mpeg", "m2ts", "mts", "ts", "ogv", "3gp", "vob",
    "divx",
];

fn extension(path: &Path) -> String {
    path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

/// True for a file whose name says it is a picture or a video, so it belongs in the media bin
/// and is not a song.
pub fn is_media(path: &Path) -> bool {
    let ext = extension(path);
    PICTURES.contains(&ext.as_str()) || VIDEOS.contains(&ext.as_str())
}

/// True for a video, which plays when it goes on screen.
pub fn is_video(path: &Path) -> bool {
    VIDEOS.contains(&extension(path).as_str())
}

/// How long a video is, in seconds, for its tile; `None` for a picture, or a video whose length
/// can't be read. It reads the file, so it is for the media bin's readers, not the window.
pub fn video_length(path: &Path) -> Option<f64> {
    is_video(path).then(|| sp_video::length(path)).flatten()
}

/// A video's length as its tile shows it: "0:36", "12:05", or "1:02:09" from an hour on. A
/// video of under a second counts as one.
pub fn length_words(seconds: f64) -> String {
    let all = (seconds.round() as u64).max(1);
    let (hours, minutes, seconds) = (all / 3600, all / 60 % 60, all % 60);
    if hours > 0 { format!("{hours}:{minutes:02}:{seconds:02}") } else { format!("{minutes}:{seconds:02}") }
}

/// True for a video this computer can't play because FFmpeg isn't here: any video, except on a
/// Mac, where the system's own player plays MP4 and MOV.
pub fn needs_missing_ffmpeg(path: &Path) -> bool {
    let system = cfg!(target_os = "macos") && ["mp4", "mov", "m4v", "3gp"].contains(&extension(path).as_str());
    is_video(path) && !system && !sp_video::ffmpeg_found()
}

/// How to get FFmpeg on this system, for the end of a sentence about a video that needs it;
/// while the app is fetching its own (Windows, `ffmpeg_fetch`), that it is.
pub fn ffmpeg_advice() -> String {
    if crate::ffmpeg_fetch::under_way() {
        tf(
            "media.ffmpeg.fetching",
            "SoftPresenter is getting it now ({size}).",
            &[("size", &crate::ffmpeg_fetch::planned_size())],
        )
    } else if cfg!(target_os = "macos") {
        t("media.ffmpeg.mac", "MP4 and MOV videos play without it.").to_owned()
    } else if cfg!(windows) {
        t("media.ffmpeg.windows", "Put ffmpeg.exe in the same folder as the program.").to_owned()
    } else {
        t("media.ffmpeg.linux", "Install FFmpeg (for example: sudo apt install ffmpeg).").to_owned()
    }
}

/// The end of the sentence about a video the operator added while FFmpeg was missing, kept to be
/// added once the app has fetched it.
pub fn ffmpeg_advice_kept() -> String {
    tf(
        "media.ffmpeg.fetching_add",
        "SoftPresenter is getting it now ({size}), and puts the video in the media bin once it is ready.",
        &[("size", &crate::ffmpeg_fetch::planned_size())],
    )
}

/// Reads and decodes a picture file for the screens (no larger than [`screen_side`]); for a
/// video, a still from it. The error is a plain sentence for the operator.
pub fn load_picture(path: &Path) -> Result<Picture, String> {
    load_picture_at(path, screen_side())
}

/// Reads a picture file no larger than `max_side` on its longer side, turned the way the camera
/// held it; for a video, a still from it. On a Mac the picture is decoded straight to that size
/// (a 6000 pixel photo is never held whole), and a smaller one is never enlarged.
pub fn load_picture_at(path: &Path, max_side: u32) -> Result<Picture, String> {
    let name = shown_name(path);
    let ext = extension(path);
    let from_system = |frame: Result<sp_video::Frame, String>| {
        let frame = frame?;
        Picture::from_rgba(frame.width, frame.height, frame.rgba)
            .ok_or_else(|| tf("media.picture_empty", "{name}: the picture is empty.", &[("name", &name)]))
    };
    if VIDEOS.contains(&ext.as_str()) {
        return from_system(sp_video::still(path, max_side.min(VIDEO_STILL_SIDE)));
    }
    if SYSTEM_PICTURES.contains(&ext.as_str()) {
        return from_system(sp_video::picture(path, max_side));
    }
    if !OWN_PICTURES.contains(&ext.as_str()) {
        return Err(tf(
            "media.not_media",
            "{name}: not a picture or video SoftPresenter can open (such as PNG, JPEG, HEIC, TIFF, MP4 or MOV).",
            &[("name", &name)],
        ));
    }
    // The Mac's system first; the decoders here are the way on other systems, and the way left
    // when the system can't read a file.
    #[cfg(target_os = "macos")]
    if let Ok(picture) = from_system(sp_video::picture(path, max_side)) {
        return Ok(picture);
    }
    decode_here(path, max_side).map_err(|e| {
        tf("media.not_opened", "{name}: could not be opened ({error}).", &[("name", &name), ("error", &e)])
    })
}

fn shown_name(path: &Path) -> String {
    path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned())
}

/// Decodes a picture with the `image` crate, scaled to no more than `max_side` the moment it is
/// whole (so the full-size pixels are let go before anything else is made from them), and turned
/// as its metadata says.
fn decode_here(path: &Path, max_side: u32) -> Result<Picture, String> {
    use image::ImageDecoder as _;
    let mut decoder = image::ImageReader::open(path)
        .and_then(image::ImageReader::with_guessed_format)
        .map_err(|e| e.to_string())?
        .into_decoder()
        .map_err(|e| e.to_string())?;
    let orientation = decoder.orientation().unwrap_or(image::metadata::Orientation::NoTransforms);
    let decoded = image::DynamicImage::from_decoder(decoder).map_err(|e| e.to_string())?;
    let mut small = shrink(decoded, max_side);
    small.apply_orientation(orientation);
    let rgba = small.into_rgba8();
    let (width, height) = rgba.dimensions();
    Picture::from_rgba(width, height, rgba.into_raw())
        .ok_or_else(|| t("media.empty_inside", "the picture is empty").to_owned())
}

/// A picture scaled to no more than `max_side` on its longer side; one that is not larger is
/// handed back as it is. Takes the picture, so the original is dropped as soon as the copy is
/// made.
fn shrink(picture: image::DynamicImage, max_side: u32) -> image::DynamicImage {
    if max_side > 0 && picture.width().max(picture.height()) > max_side {
        picture.resize(max_side, max_side, image::imageops::FilterType::Triangle)
    } else {
        picture
    }
}

/// How big a picture is on screen, in pixels, the way up the camera held it. Reads only the
/// file's start when the `image` crate knows the format; otherwise the system decodes a copy.
pub fn picture_size(path: &Path) -> Result<(u32, u32), String> {
    use image::ImageDecoder as _;
    use image::metadata::Orientation::{Rotate90, Rotate90FlipH, Rotate270, Rotate270FlipH};
    let known =
        image::ImageReader::open(path).and_then(image::ImageReader::with_guessed_format).ok().and_then(|reader| {
            let mut decoder = reader.into_decoder().ok()?;
            let (width, height) = decoder.dimensions();
            let turned = matches!(decoder.orientation(), Ok(Rotate90 | Rotate270 | Rotate90FlipH | Rotate270FlipH));
            Some(if turned { (height, width) } else { (width, height) })
        });
    match known {
        Some(size) => Ok(size),
        None => load_picture(path).map(|picture| (picture.width(), picture.height())),
    }
}

/// Reads a picture file, or a video's still, as the small copy the media bin keeps. True with
/// it when the copy is the whole picture (it was no larger), or a video's still (shown on the
/// screens only until the video's first picture comes), so the screens can show it as it is.
pub fn load_small(path: &Path) -> Result<(Picture, bool), String> {
    let picture = load_picture_at(path, SMALL_SIDE)?;
    // A video's still is shown on the screens only until its first picture comes, so it counts
    // as whole; a picture smaller than the copy asked for is whole too.
    let whole = is_video(path) || picture.width().max(picture.height()) < SMALL_SIDE;
    Ok((picture, whole))
}

#[cfg(test)]
mod tests {

    use super::*;

    /// A playlist goes on from a video that ended, that never gave a picture in time, or that
    /// can't play; never from one still playing or still within its wait, and never from the
    /// only item, which loops.
    #[test]
    fn a_playlist_goes_on_from_a_video_that_ends_never_starts_or_cant_play() {
        assert!(video_done(true, true, false, false), "played to its end");
        assert!(video_done(true, false, true, false), "no picture in time");
        assert!(video_done(true, false, false, true), "it can't play");
        assert!(!video_done(true, false, false, false), "playing, or still within its wait");
        assert!(!video_done(false, true, true, true), "alone, it loops");
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("softpresenter-media-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    fn write_png(path: &Path, width: u32, height: u32) {
        image::RgbaImage::from_pixel(width, height, image::Rgba([10, 200, 30, 255])).save(path).unwrap();
    }

    #[test]
    fn a_videos_tile_shows_its_length_in_minutes_and_seconds_and_hours_from_an_hour_on() {
        assert_eq!(length_words(36.4), "0:36");
        assert_eq!(length_words(35.6), "0:36");
        assert_eq!(length_words(0.3), "0:01", "never 0:00");
        assert_eq!(length_words(725.0), "12:05");
        assert_eq!(length_words(3599.4), "59:59");
        assert_eq!(length_words(3599.6), "1:00:00");
        assert_eq!(length_words(3729.0), "1:02:09");
        assert_eq!(video_length(Path::new("Welcome.png")), None, "a picture has none");
    }

    /// The system's player reads a video's length on a Mac (elsewhere ffprobe, if it is there).
    #[cfg(target_os = "macos")]
    #[test]
    fn the_sample_videos_length_is_read_for_its_tile() {
        let video = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/backgrounds/Drifting Light.mp4");
        let length = video_length(&video).expect("its length");
        assert_eq!(length_words(length), "0:16", "{length}");
    }

    #[test]
    fn a_media_playlist_is_shown_and_played_in_its_own_order_and_dragged_to_another() {
        let bin: Vec<sp_model::MediaItem> = ["/a/one.png", "/a/two.png", "/a/three.png", "/a/four.png"]
            .iter()
            .map(|path| sp_model::MediaItem::new(path.to_string(), PathBuf::from(path)))
            .collect();
        let path = |name: &str| PathBuf::from(format!("/a/{name}.png"));
        // Added three, then one, with a file the bin no longer has between them.
        let files = vec![path("three"), path("gone"), path("one"), path("four")];
        assert_eq!(playlist_places(&files, &bin), [2, 0, 3], "in the order added, not the bin's");
        let shown = vec![path("three"), path("one"), path("four")];
        // Four dragged to the front; three and one dragged after four.
        let moved = moved_files(&files, &shown, &[path("four")], 0);
        assert_eq!(moved, [path("four"), path("three"), path("one"), path("gone")]);
        let moved = moved_files(&files, &shown, &[path("three"), path("one")], 3);
        assert_eq!(moved, [path("four"), path("three"), path("one"), path("gone")]);
        // Let go where it was: nothing moves (but the missing file goes to the end).
        assert_eq!(
            moved_files(&files, &shown, &[path("one")], 1),
            [path("three"), path("one"), path("four"), path("gone")]
        );
    }

    #[test]
    fn a_playing_media_playlist_goes_on_from_the_one_on_screen_after_a_change() {
        // Playing bin item 2, the first of [2, 0, 3].
        assert_eq!(next_in_cycle(&[2, 0, 3], 0, 2), 1, "the next as it was");
        // Reordered to [0, 3, 2] while 2 is on screen: round to the first.
        assert_eq!(next_in_cycle(&[0, 3, 2], 0, 2), 0);
        // 2 taken out of [2, 0, 3]: the one that took its place, 0, is next.
        assert_eq!(next_in_cycle(&[0, 3], 0, 2), 0);
        assert_eq!(next_in_cycle(&[], 0, 2), 0);
    }

    #[test]
    fn a_media_list_that_cant_be_read_is_kept_aside_and_never_saved_over() {
        let mut list = List::default();
        list.entries.push(Entry {
            id: sp_model::Uuid::new_v4(),
            path: "/media/Sky.png".into(),
            fit: sp_model::ImageFit::Fill,
            looping: true,
            followed: false,
            sample: None,
        });
        list.playlists.push(Playlist::new("Loop".to_owned()));
        let good = serde_json::to_string_pretty(&list).unwrap();
        crate::whole_file::tests::check_loader(
            "media.json",
            crate::whole_file::What::MediaList,
            good.as_bytes(),
            |path| load_list_from(path).1,
            |path| {
                let writer = crate::writer::Writer::start(|| {});
                writer.write(list_to_save_at(path.to_owned(), &List::default()));
                writer.drain();
            },
        );
    }

    #[test]
    fn an_old_media_list_still_loads_and_a_new_one_keeps_settings_and_playlists() {
        let old = List::from_json(r#"["/a/sky.png", "/a/loop.mp4"]"#).unwrap();
        assert_eq!(old.entries.len(), 2);
        assert!(old.entries.iter().all(|entry| entry.fit == sp_model::ImageFit::Fill && entry.looping));
        let mut backgrounds = Playlist::new(new_playlist_name(&[]));
        backgrounds.toggle(Path::new("/a/sky.png"));
        assert_eq!(new_playlist_name(std::slice::from_ref(&backgrounds)), "New Media Playlist 2");
        let list = List {
            entries: vec![
                Entry {
                    id: sp_model::Uuid::new_v4(),
                    path: "/a/sky.png".into(),
                    fit: sp_model::ImageFit::Fit,
                    looping: false,
                    followed: false,
                    sample: None,
                },
                Entry {
                    id: sp_model::Uuid::new_v4(),
                    path: "/notices/week.png".into(),
                    fit: sp_model::ImageFit::Fill,
                    looping: true,
                    followed: true,
                    sample: None,
                },
            ],
            playlists: vec![backgrounds],
        };
        let text = serde_json::to_string(&list).unwrap();
        assert_eq!(text.matches("followed").count(), 1, "only an item a folder brought in says so");
        assert_eq!(List::from_json(&text), Ok(list));
        // A list saved before items said where they came from: every item was added by hand.
        let before = r#"{"entries":[{"id":"6f9619ff-8b86-d011-b42d-00c04fc964ff","path":"/a/sky.png","fit":"fill","looping":true}]}"#;
        assert!(List::from_json(before).is_ok_and(|list| !list.entries[0].followed));
        // ...and before the sample backgrounds: no item is a sample, no playlist is theirs.
        let list = List::from_json(before).unwrap();
        assert_eq!((list.entries[0].sample.as_deref(), list.playlists.len()), (None, 0));
        let old_playlist = r#"{"entries":[],"playlists":[{"id":"6f9619ff-8b86-d011-b42d-00c04fc964ff","name":"Loop","files":["/a/sky.png"]}]}"#;
        assert!(List::from_json(old_playlist).is_ok_and(|list| !list.playlists[0].sample));
    }

    #[test]
    fn which_sample_an_item_is_and_which_playlist_is_theirs_are_saved_and_read_back() {
        let mut theirs = Playlist::new("Sample Backgrounds".to_owned());
        theirs.sample = true;
        theirs.files = vec!["/media/Olive.jpg".into()];
        let sample = Entry {
            id: sp_model::Uuid::new_v4(),
            path: "/media/Chapel.jpg".into(),
            fit: sp_model::ImageFit::Fill,
            looping: true,
            followed: false,
            sample: Some("Olive.jpg".to_owned()),
        };
        let other = Entry { sample: None, path: "/media/Sky.png".into(), ..sample.clone() };
        let list = List { entries: vec![sample, other], playlists: vec![theirs, Playlist::new("Loop".to_owned())] };
        let text = serde_json::to_string(&list).unwrap();
        assert_eq!(text.matches("\"sample\"").count(), 2, "only a sample item and their playlist say so");
        let back = List::from_json(&text).unwrap();
        assert_eq!(back, list);
        assert_eq!(back.entries[0].sample.as_deref(), Some("Olive.jpg"), "still that sample under another name");
        assert!(back.playlists[0].sample && !back.playlists[1].sample);
    }

    fn bin(names: &[&str]) -> Vec<sp_model::MediaItem> {
        names.iter().map(|name| sp_model::MediaItem::new((*name).to_owned(), PathBuf::from("/m").join(name))).collect()
    }

    #[test]
    fn a_media_name_drops_its_ending_unless_another_file_has_the_same_name_in_another_format() {
        let samples = bin(&["Evening Hills.jpg", "Drifting Light.mp4", "Still.jpg"]);
        assert_eq!(label("Evening Hills.jpg", &samples), "Evening Hills");
        assert_eq!(label("Drifting Light.mp4", &samples), "Drifting Light");
        assert_eq!(label("Countdown.mp4", &[]), "Countdown", "a playlist row's name, its file not in the bin");
        assert_eq!(label("No ending", &samples), "No ending");
        assert_eq!(label("Version 1.2 final.png", &samples), "Version 1.2 final");
        // The same name in two formats, whatever the case: both keep their endings.
        let pair = bin(&["Logo.png", "LOGO.jpg", "Sky.png"]);
        assert_eq!(label("Logo.png", &pair), "Logo.png");
        assert_eq!(label("LOGO.jpg", &pair), "LOGO.jpg");
        assert_eq!(label("Sky.png", &pair), "Sky");
        let video = bin(&["Welcome.png", "Welcome.mp4"]);
        assert_eq!(label("Welcome.png", &video), "Welcome.png");
        assert_eq!(label("Welcome.mp4", &video), "Welcome.mp4");
    }

    #[test]
    fn a_name_too_long_ends_in_an_ellipsis_and_one_that_fits_is_left_alone() {
        let ten = |text: &str| text.chars().count() <= 10;
        assert_eq!(ellipsize("Olive", ten), "Olive");
        assert_eq!(ellipsize("Room Out of Focus", ten), "Room Out\u{2026}");
        assert_eq!(ellipsize("Evening Hills.jpg", ten), "Evening H\u{2026}");
        assert_eq!(ellipsize("abcdefghijkl", |text| text.chars().count() <= 1), "\u{2026}");
    }

    fn at(seconds: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(seconds)
    }

    #[test]
    fn a_followed_folder_brings_files_in_reads_changed_ones_again_and_takes_gone_ones_out() {
        let (notices, hand) = (Uuid::new_v4(), Uuid::new_v4());
        let folder = PathBuf::from("/notices");
        let file = |name: &str| folder.join(name);
        let mut followed = Playlist::new("Notices".to_owned());
        followed.id = notices;
        followed.folder = Some(folder.clone());
        followed.files = vec![file("a.png"), file("b.png"), file("c.png"), file("d.png")];
        let mut by_hand = Playlist::new("Backgrounds".to_owned());
        by_hand.id = hand;
        by_hand.files = vec![file("c.png")];
        let playlists = [followed, by_hand];
        // In the bin: a and b, c and d came with the folder; e was added by hand from it.
        let ids: Vec<Uuid> = (0..5).map(|_| Uuid::new_v4()).collect();
        let bin: Vec<(Uuid, PathBuf)> = ["a.png", "b.png", "c.png", "d.png", "e.png"]
            .iter()
            .zip(&ids)
            .map(|(name, id)| (*id, file(name)))
            .collect();
        let brought: HashSet<Uuid> = ids[..4].iter().copied().collect();
        let mut notes = FolderNotes::default();
        for (_, path) in &bin {
            notes.seen.insert(path.clone(), at(1));
        }
        notes.refused.insert(file("broken.png"), at(1));
        notes.refused.insert(file("gone.png"), at(1));
        // Now the folder has a (written over), b, a new file f, and broken.png as it was. c, d and
        // e have gone: c is still in another playlist, d nothing uses, e was added by hand.
        let listing: Vec<(PathBuf, SystemTime)> =
            vec![(file("a.png"), at(2)), (file("b.png"), at(1)), (file("broken.png"), at(1)), (file("f.png"), at(2))];
        let listings = vec![(notices, folder.clone(), Some(listing.clone()))];
        let changes = notes.look(&listings, &bin, &playlists, &brought, &HashSet::new());
        assert_eq!(changes.read_new, vec![(file("f.png"), at(2))]);
        assert_eq!(changes.read_again, vec![ids[0]]);
        assert_eq!(changes.playlists, vec![(notices, listing.iter().map(|(path, _)| path.clone()).collect())]);
        assert_eq!(changes.leave, vec![ids[3]], "only d leaves the bin");
        assert!(notes.refused.contains_key(&file("broken.png")) && !notes.refused.contains_key(&file("gone.png")));
        assert!(!notes.seen.contains_key(&file("d.png")) && notes.seen[&file("a.png")] == at(2));
        // Used on a slide (or on screen), d stays too.
        let used: HashSet<Uuid> = [ids[3]].into();
        let mut again = FolderNotes::default();
        assert!(again.look(&listings, &bin, &playlists, &brought, &used).leave.is_empty());
        // A file being read isn't asked for twice.
        notes.coming.insert(Uuid::new_v4(), (file("f.png"), at(2)));
        assert!(notes.look(&listings, &bin, &playlists, &brought, &HashSet::new()).read_new.is_empty());
    }

    #[test]
    fn a_folder_that_cant_be_read_changes_nothing() {
        let notices = Uuid::new_v4();
        let folder = PathBuf::from("/Volumes/Unplugged/notices");
        let mut followed = Playlist::new("Notices".to_owned());
        followed.id = notices;
        followed.folder = Some(folder.clone());
        followed.files = vec![folder.join("a.png")];
        let item = Uuid::new_v4();
        let bin = vec![(item, folder.join("a.png"))];
        let mut notes = FolderNotes::default();
        notes.seen.insert(folder.join("a.png"), at(1));
        let changes =
            notes.look(&[(notices, folder.clone(), None)], &bin, &[followed], &[item].into(), &HashSet::new());
        assert_eq!(changes, FolderChanges::default());
        assert!(notes.seen.contains_key(&folder.join("a.png")));
        // A real folder that can't be read is told from an empty one.
        assert!(folder_files(&folder).is_none());
    }

    #[test]
    fn a_folder_lists_its_pictures_and_videos_by_name_without_hidden_ones() {
        let dir = std::env::temp_dir().join(format!("softpresenter-folder-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["b.mp4", "a.png", ".downloading.mp4", "notes.txt"] {
            std::fs::write(dir.join(name), b"x").unwrap();
        }
        let names: Vec<String> = folder_files(&dir)
            .unwrap()
            .into_iter()
            .map(|(path, _)| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["a.png", "b.mp4"]);
        std::fs::remove_dir_all(&dir).unwrap();
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(folder_files(&dir), Some(Vec::new()), "an empty folder is not one that can't be read");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn places_after_an_item_leaves_the_bin_move_up_and_its_own_is_gone() {
        let picked: Vec<usize> = [0, 2, 3, 5].iter().filter_map(|at| place_after_removal(*at, 3)).collect();
        assert_eq!(picked, [0, 2, 4]);
        assert_eq!(place_after_removal(3, 3), None);
    }

    #[test]
    fn media_files_are_told_from_songs_by_their_names() {
        assert!(is_media(Path::new("a/Sunrise.JPG")));
        assert!(is_media(Path::new("clip.mov")));
        assert!(!is_media(Path::new("song.txt")));
        assert!(!is_media(Path::new("folder")));
    }

    #[test]
    fn loads_a_png_at_its_own_size() {
        let path = temp("small.png");
        write_png(&path, 64, 36);
        let picture = load_picture(&path).unwrap();
        assert_eq!((picture.width(), picture.height()), (64, 36));
    }

    #[test]
    fn the_small_copy_keeps_the_shape_and_says_when_it_is_the_whole_picture() {
        let (big, small) = (temp("small-copy-big.png"), temp("small-copy-small.png"));
        write_png(&big, 1920, 1080);
        write_png(&small, 640, 360);
        let (copy, whole) = load_small(&big).unwrap();
        assert_eq!((copy.width(), copy.height(), whole), (SMALL_SIDE, 540, false));
        let (copy, whole) = load_small(&small).unwrap();
        assert_eq!((copy.width(), copy.height(), whole), (640, 360, true));
        assert!(load_small(Path::new("/x/notes.txt")).unwrap_err().contains("not a picture"));
    }

    #[test]
    fn a_huge_picture_is_scaled_down_keeping_its_shape() {
        let path = temp("huge.png");
        write_png(&path, 5120, 1280);
        let picture = load_picture(&path).unwrap();
        assert_eq!((picture.width(), picture.height()), (MAX_SIDE, 640));
    }

    /// A JPEG of left half red, right half blue, with the camera's turn written in.
    fn write_jpeg(path: &Path, width: u32, height: u32, orientation: u8) {
        let mut rgb = Vec::new();
        for _ in 0..height {
            for x in 0..width {
                rgb.extend(if x < width / 2 { [230, 20, 20] } else { [20, 20, 230] });
            }
        }
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 95)
            .encode(&rgb, width, height, image::ExtendedColorType::Rgb8)
            .unwrap();
        // An EXIF segment right after the start of the picture: one tag, the orientation.
        let mut exif = b"Exif\0\0MM\0*\0\0\0\x08\0\x01\x01\x12\0\x03\0\0\0\x01".to_vec();
        exif.extend([0, orientation, 0, 0, 0, 0, 0, 0]);
        let mut with = vec![0xFF, 0xD8, 0xFF, 0xE1];
        with.extend(((exif.len() + 2) as u16).to_be_bytes());
        with.extend(exif);
        with.extend(&jpeg[2..]);
        std::fs::write(path, with).unwrap();
    }

    #[test]
    fn a_picture_is_loaded_at_the_size_asked_for_and_never_enlarged() {
        for ending in ["png", "jpg"] {
            let (big, small) = (temp(&format!("ask.{ending}")), temp(&format!("ask-small.{ending}")));
            image::RgbImage::from_pixel(3000, 1000, image::Rgb([10, 200, 30])).save(&big).unwrap();
            image::RgbImage::from_pixel(64, 36, image::Rgb([10, 200, 30])).save(&small).unwrap();
            for load in [load_picture_at, decode_here_as_load] {
                let picture = load(&big, 800).unwrap();
                assert_eq!((picture.width(), picture.height()), (800, 267), "{ending}: scaled to the side asked for");
                let picture = load(&small, 2560).unwrap();
                assert_eq!((picture.width(), picture.height()), (64, 36), "{ending}: a small picture stays as it is");
            }
        }
    }

    fn decode_here_as_load(path: &Path, side: u32) -> Result<Picture, String> {
        decode_here(path, side)
    }

    #[test]
    fn a_picture_is_turned_the_way_the_camera_held_it() {
        let path = temp("turned.jpg");
        for (orientation, size) in [(1, (40, 20)), (3, (40, 20)), (6, (20, 40)), (8, (20, 40))] {
            write_jpeg(&path, 40, 20, orientation);
            for picture in [load_picture_at(&path, 2560).unwrap(), decode_here(&path, 2560).unwrap()] {
                assert_eq!((picture.width(), picture.height()), size, "orientation {orientation}");
            }
            assert_eq!(picture_size(&path).unwrap(), size, "measured as it is shown");
        }
        // Turned a quarter clockwise, the left (red) half is on top.
        write_jpeg(&path, 40, 20, 6);
        for picture in [load_picture_at(&path, 2560).unwrap(), decode_here(&path, 2560).unwrap()] {
            let pixels = picture.pixels();
            let at = |y: usize| &pixels[(y * 20 + 10) * 4..(y * 20 + 10) * 4 + 4];
            assert!(at(5)[0] > 150 && at(5)[2] < 100, "top is red: {:?}", at(5));
            assert!(at(35)[2] > 150 && at(35)[0] < 100, "bottom is blue: {:?}", at(35));
        }
    }

    #[test]
    fn a_pictures_see_through_pixels_stay_see_through() {
        let path = temp("alpha.png");
        let mut pixels = image::RgbaImage::from_pixel(8, 8, image::Rgba([200, 100, 50, 255]));
        pixels.put_pixel(1, 1, image::Rgba([200, 100, 50, 128]));
        pixels.put_pixel(2, 2, image::Rgba([0, 0, 0, 0]));
        pixels.save(&path).unwrap();
        for picture in [load_picture_at(&path, 2560).unwrap(), decode_here(&path, 2560).unwrap()] {
            let data = picture.pixels();
            let at = |x: usize, y: usize| &data[(y * 8 + x) * 4..(y * 8 + x) * 4 + 4];
            assert_eq!(at(0, 0), [200, 100, 50, 255]);
            assert!(at(1, 1)[3].abs_diff(128) <= 1 && at(1, 1)[0].abs_diff(200) <= 3, "{:?}", at(1, 1));
            assert_eq!(at(2, 2)[3], 0);
        }
    }

    #[test]
    fn the_screens_ask_for_a_size_that_is_never_below_before_and_never_above_4k() {
        let side = AtomicU32::new(MAX_SIDE);
        raise(&side, 1920, 1080);
        assert_eq!(side.load(Ordering::Relaxed), MAX_SIDE, "a 1080p screen doesn't make pictures smaller");
        raise(&side, 3840, 2160);
        assert_eq!(side.load(Ordering::Relaxed), 3840);
        raise(&side, 1920, 1080);
        assert_eq!(side.load(Ordering::Relaxed), 3840, "nor does one opened after a 4K screen");
        raise(&side, 7680, 4320);
        assert_eq!(side.load(Ordering::Relaxed), 3840, "8K is shown from the 4K picture");
    }

    #[test]
    fn other_files_are_refused_in_plain_words() {
        assert!(is_video(Path::new("/x/clip.WEBM")) && is_video(Path::new("/x/clip.mkv")));
        assert!(load_picture(Path::new("/x/notes.txt")).unwrap_err().contains("not a picture"));
        assert!(load_picture(Path::new("/x/no-extension")).unwrap_err().contains("not a picture"));
        assert!(is_video(Path::new("a/Clip.MOV")) && !is_video(Path::new("a/photo.heic")));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn the_system_reads_tiff_and_a_missing_video_is_said_so() {
        let path = temp("small.tiff");
        image::RgbaImage::from_pixel(40, 30, image::Rgba([10, 200, 30, 255]))
            .save_with_format(&path, image::ImageFormat::Png)
            .unwrap();
        // A PNG named .tiff still opens: the system reads what the file is, not its name.
        let picture = load_picture(&path).unwrap();
        assert_eq!((picture.width(), picture.height()), (40, 30));
        assert!(load_picture(Path::new("/x/clip.MP4")).unwrap_err().contains("could not be found"));
    }

    #[test]
    fn a_broken_picture_file_is_an_error_not_a_crash() {
        let path = temp("broken.png");
        std::fs::write(&path, b"this is not a png").unwrap();
        assert!(load_picture(&path).unwrap_err().contains("could not be opened"));
        assert!(load_picture(&temp("missing.png")).unwrap_err().contains("could not be opened"));
    }
}
