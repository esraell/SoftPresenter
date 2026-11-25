//! The pictures of the media bin's items.
//!
//! Each item keeps a small copy for as long as it is in the bin, for its thumbnail and the
//! Program preview. The full-size picture the screens show is read only when the item goes on
//! screen, or is about to (the next item of a media playlist playing, the media of the slide
//! Next would show), and only a few are kept: a full-size picture is up to 26 MB, and a bin of
//! thirty backgrounds once held half a gigabyte of them all the time.
//!
//! Reading is done by threads of their own, never by the window's thread or the screens': the
//! window asks, a reader reads, and the window is woken when the picture is there. Until an
//! item's full-size picture comes (a fraction of a second), the screens show its small copy.
//! The full-size pictures have a reader of their own, so one wanted on screen never waits
//! behind the small copies being read when the app starts.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use sp_model::Uuid;
use sp_render::Picture;

/// Full-size pictures kept at once: the one on screen, the one shown before it (to go back
/// to), and one read ahead.
pub const FULL_KEPT: usize = 3;

/// Readers of small copies at work at once. At the start every item in the bin is read, and
/// a few at a time fill the bin in about a second.
const SMALL_READERS: usize = 3;

/// What a reading is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Want {
    /// The small copy, kept while the item is in the bin.
    Small,
    /// The full-size picture of the item on screen.
    FullNow,
    /// The full-size picture of an item about to go on screen.
    FullAhead,
    /// A video's length alone, for an item whose small copy was read already (one just added).
    Length,
}

struct Job {
    id: Uuid,
    path: PathBuf,
    want: Want,
    stamp: u64,
}

/// A reading a reader has done.
struct Read {
    id: Uuid,
    want: Want,
    stamp: u64,
    /// The picture, and whether a small copy is the whole picture (it was no larger).
    result: Result<(Picture, bool), String>,
    /// A video's length in seconds, read with its small copy, when it could be.
    length: Option<f64>,
}

type Wake = Arc<dyn Fn() + Send + Sync>;

/// Jobs waiting for a reader, and how many readers are at work. A reader starts when a job
/// comes and every reader is busy, and ends once nothing is left to read, so no thread waits
/// about between readings (and whatever the system keeps for a thread is let go with it).
struct Readers {
    name: &'static str,
    most: usize,
    queue: Arc<Mutex<(VecDeque<Job>, usize)>>,
}

impl Readers {
    fn new(name: &'static str, most: usize) -> Readers {
        Readers { name, most, queue: Arc::new(Mutex::new((VecDeque::new(), 0))) }
    }

    /// Puts a job in line, at its head when `first`, and starts a reader if none is free.
    fn push(&self, job: Job, first: bool, done: &Sender<Read>, wake: &Wake) {
        let Ok(mut queue) = self.queue.lock() else { return };
        if first {
            queue.0.push_front(job);
        } else {
            queue.0.push_back(job);
        }
        if queue.1 >= self.most {
            return;
        }
        queue.1 += 1;
        drop(queue);
        let (queue, done, wake) = (Arc::clone(&self.queue), done.clone(), Arc::clone(wake));
        let started =
            std::thread::Builder::new().name(self.name.to_owned()).spawn(move || read_all(&queue, &done, &wake));
        if let Err(e) = started {
            eprintln!("could not start a {} reader: {e}", self.name);
            if let Ok(mut queue) = self.queue.lock() {
                queue.1 -= 1;
            }
        }
    }

    /// Takes the waiting jobs `unwanted` picks out of the line, and says whose they were.
    fn drop_waiting(&self, unwanted: impl Fn(&Job) -> bool) -> Vec<Uuid> {
        let Ok(mut queue) = self.queue.lock() else { return Vec::new() };
        let mut dropped = Vec::new();
        queue.0.retain(|job| {
            let drop = unwanted(job);
            if drop {
                dropped.push(job.id);
            }
            !drop
        });
        dropped
    }
}

/// A reader: reads job after job until none is left, and says each is done.
fn read_all(queue: &Mutex<(VecDeque<Job>, usize)>, done: &Sender<Read>, wake: &Wake) {
    loop {
        let job = {
            let Ok(mut queue) = queue.lock() else { return };
            match queue.0.pop_front() {
                Some(job) => job,
                None => {
                    queue.1 -= 1;
                    return;
                }
            }
        };
        // A decoder that panics on a strange file is a file that can't be read, not a reader lost.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match job.want {
            Want::Small => crate::media::load_small(&job.path),
            Want::FullNow | Want::FullAhead => crate::media::load_picture(&job.path).map(|picture| (picture, false)),
            Want::Length => Err(String::new()),
        }))
        .unwrap_or_else(|_| Err(format!("{}: the picture could not be read.", job.path.display())));
        // A video's length goes with its small copy, for its tile.
        let length = (job.want == Want::Length || job.want == Want::Small && result.is_ok())
            .then(|| std::panic::catch_unwind(|| crate::media::video_length(&job.path)).ok().flatten())
            .flatten();
        if done.send(Read { id: job.id, want: job.want, stamp: job.stamp, result, length }).is_err() {
            if let Ok(mut queue) = queue.lock() {
                queue.1 -= 1;
            }
            return;
        }
        wake();
    }
}

/// What came in from the readers, for the app to act on.
#[derive(Debug, PartialEq)]
pub enum Arrived {
    /// An item's small copy is there: its first, or a new one after its file changed.
    Small(Uuid),
    /// An item's small copy couldn't be read; the words say why.
    Refused(Uuid, String),
    /// An item's full-size picture is there.
    Full(Uuid),
    /// An item's length in seconds, read with its new small copy (`None` for a picture, or a
    /// video whose length couldn't be read).
    Length(Uuid, Option<f64>),
}

/// A full-size picture kept, and when it was last shown.
struct Kept {
    id: Uuid,
    picture: Picture,
    used: u64,
    /// The item's stamp when it was read: one read before its file changed is let go once the
    /// new small copy comes.
    stamp: u64,
}

/// The media bin's pictures, by item: the small copies, the few full-size pictures kept, and
/// the readers.
pub struct BinPictures {
    small: HashMap<Uuid, Picture>,
    /// Items whose small copy is the whole picture (or a video's still), which the screens show
    /// as it is.
    whole: HashSet<Uuid>,
    /// Each item's stamp, new each time its file is read anew, so a reading asked for before is
    /// let go when it comes. An item without one has left the bin.
    stamps: HashMap<Uuid, u64>,
    last_stamp: u64,
    small_coming: HashSet<Uuid>,
    full: Vec<Kept>,
    full_coming: HashSet<Uuid>,
    /// Items whose full-size picture couldn't be read, not tried again until their file changes.
    full_refused: HashSet<Uuid>,
    /// The item on screen, whose full-size picture is never let go, and when its full-size
    /// picture was asked for, while it is being read (for the log).
    on_screen: Option<Uuid>,
    asked: Option<(Uuid, std::time::Instant)>,
    /// Whether the screens were last handed a kept full-size picture (not a small copy, or
    /// nothing while the small copy was still being read).
    screens_full: bool,
    clock: u64,
    small_readers: Readers,
    full_reader: Readers,
    done_tx: Sender<Read>,
    done_rx: Receiver<Read>,
    wake: Wake,
}

impl BinPictures {
    /// `wake` is called on a reader's thread each time a reading is done.
    pub fn new(wake: impl Fn() + Send + Sync + 'static) -> BinPictures {
        let (done_tx, done_rx) = channel();
        BinPictures {
            small: HashMap::new(),
            whole: HashSet::new(),
            stamps: HashMap::new(),
            last_stamp: 0,
            small_coming: HashSet::new(),
            full: Vec::new(),
            full_coming: HashSet::new(),
            full_refused: HashSet::new(),
            on_screen: None,
            asked: None,
            screens_full: false,
            clock: 0,
            small_readers: Readers::new("media bin pictures", SMALL_READERS),
            full_reader: Readers::new("full-size pictures", 1),
            done_tx,
            done_rx,
            wake: Arc::new(wake),
        }
    }

    /// An item's small copy, once it is there.
    pub fn small(&self, id: Uuid) -> Option<&Picture> {
        self.small.get(&id)
    }

    /// True while any small copy is being read.
    pub fn any_coming(&self) -> bool {
        !self.small_coming.is_empty()
    }

    /// A new stamp for an item: readings asked for before it are let go when they come.
    fn new_stamp(&mut self, id: Uuid) -> u64 {
        self.last_stamp += 1;
        self.stamps.insert(id, self.last_stamp);
        self.last_stamp
    }

    /// Lets go of an item's full-size picture and whatever is being read of it.
    fn drop_full(&mut self, id: Uuid) {
        self.full.retain(|kept| kept.id != id);
        if self.on_screen == Some(id) {
            self.screens_full = false;
        }
        self.stop_full_reads(id);
    }

    /// Stops waiting for an item's full-size picture (one being read is let go when it comes,
    /// by its stamp), and lets it be tried again if it couldn't be read.
    fn stop_full_reads(&mut self, id: Uuid) {
        self.full_coming.remove(&id);
        self.full_refused.remove(&id);
        self.full_reader.drop_waiting(|job| job.id == id);
    }

    /// Keeps an item's small copy, read already (an item just added). `whole` says it is the
    /// whole picture.
    pub fn set_small(&mut self, id: Uuid, picture: Picture, whole: bool) {
        self.new_stamp(id);
        self.drop_full(id);
        self.small_coming.remove(&id);
        self.small_readers.drop_waiting(|job| job.id == id);
        self.small.insert(id, picture);
        if whole {
            self.whole.insert(id);
        } else {
            self.whole.remove(&id);
        }
    }

    /// Has an item's small copy read by a reader (the items in the bin when the app starts, a
    /// file come into a followed folder, one written over). Whatever it had (its small copy, and
    /// its full-size picture if one is kept) is shown until the new small copy comes, so a file
    /// half written, which can't be read, leaves the item as it was.
    pub fn read(&mut self, id: Uuid, path: &Path) {
        let stamp = self.new_stamp(id);
        self.stop_full_reads(id);
        self.small_readers.drop_waiting(|job| job.id == id);
        self.small_coming.insert(id);
        let job = Job { id, path: path.to_owned(), want: Want::Small, stamp };
        self.small_readers.push(job, false, &self.done_tx, &self.wake);
    }

    /// Has a video's length read, for an item whose small copy was read already (one just
    /// added); it comes as [`Arrived::Length`]. Nothing for a picture.
    pub fn read_length(&mut self, id: Uuid, path: &Path) {
        let Some(stamp) = self.stamps.get(&id).copied().filter(|_| crate::media::is_video(path)) else { return };
        let job = Job { id, path: path.to_owned(), want: Want::Length, stamp };
        self.small_readers.push(job, false, &self.done_tx, &self.wake);
    }

    /// Forgets an item that has left the bin.
    pub fn forget(&mut self, id: Uuid) {
        self.drop_full(id);
        self.small_readers.drop_waiting(|job| job.id == id);
        self.stamps.remove(&id);
        self.small.remove(&id);
        self.whole.remove(&id);
        self.small_coming.remove(&id);
        if self.on_screen == Some(id) {
            self.on_screen = None;
        }
    }

    /// True when the screens were last handed a kept full-size picture, so a new small copy of
    /// the item on screen changes nothing there. (A video's still, or a picture no larger than
    /// its small copy, is its small copy, and changes with it.)
    pub fn screens_have_full(&self) -> bool {
        self.screens_full
    }

    /// True when an item's whole picture is there for the screens: kept, or its small copy.
    fn has_full(&self, id: Uuid, path: &Path) -> bool {
        crate::media::is_video(path) || self.whole.contains(&id) || self.full.iter().any(|kept| kept.id == id)
    }

    /// The picture for the screens of the item on screen (`None` when there is none), which is
    /// kept while it is there. Its full-size picture if it is kept; otherwise its small copy
    /// stands in, and the full one is read at once, before anything else (whatever was waiting
    /// to be read for the screens before isn't on them any more). A video shows its still only
    /// until its first picture comes, so the small copy does.
    pub fn for_screens(&mut self, live: Option<(Uuid, &Path)>) -> Option<Picture> {
        self.on_screen = live.map(|(id, _)| id);
        self.screens_full = false;
        let (id, path) = live?;
        if self.has_full(id, path) {
            self.clock += 1;
            let clock = self.clock;
            if let Some(kept) = self.full.iter_mut().find(|kept| kept.id == id) {
                kept.used = clock;
                self.screens_full = true;
                return Some(kept.picture.clone());
            }
            return self.small.get(&id).cloned();
        }
        if !self.full_refused.contains(&id) {
            let dropped = self.full_reader.drop_waiting(|job| job.want == Want::FullNow || job.id == id);
            for gone in dropped {
                self.full_coming.remove(&gone);
            }
            if self.full_coming.insert(id) {
                let stamp = *self.stamps.entry(id).or_insert(0);
                let job = Job { id, path: path.to_owned(), want: Want::FullNow, stamp };
                self.full_reader.push(job, true, &self.done_tx, &self.wake);
                self.asked = Some((id, std::time::Instant::now()));
            }
        }
        self.small.get(&id).cloned()
    }

    /// How long the small copy stood in on the screens for an item whose full-size picture has
    /// just come, if it was asked for there.
    pub fn stood_in(&mut self, id: Uuid) -> Option<std::time::Duration> {
        let (asked, since) = self.asked?;
        (asked == id).then(|| {
            self.asked = None;
            since.elapsed()
        })
    }

    /// Has an item's full-size picture read ahead, for an item about to go on screen, unless it
    /// is kept or being read already. Only the newest item asked for waits to be read.
    pub fn read_ahead(&mut self, id: Uuid, path: &Path) {
        if self.has_full(id, path) || self.full_coming.contains(&id) || self.full_refused.contains(&id) {
            return;
        }
        for gone in self.full_reader.drop_waiting(|job| job.want == Want::FullAhead) {
            self.full_coming.remove(&gone);
        }
        self.full_coming.insert(id);
        let stamp = *self.stamps.entry(id).or_insert(0);
        let job = Job { id, path: path.to_owned(), want: Want::FullAhead, stamp };
        self.full_reader.push(job, false, &self.done_tx, &self.wake);
    }

    /// Keeps a full-size picture, letting go of the one shown longest ago past [`FULL_KEPT`]
    /// (never the one on screen, nor this one).
    fn keep_full(&mut self, id: Uuid, picture: Picture, stamp: u64) {
        self.full.retain(|kept| kept.id != id);
        self.clock += 1;
        self.full.push(Kept { id, picture, used: self.clock, stamp });
        while self.full.len() > FULL_KEPT {
            let on_screen = self.on_screen;
            let oldest = self
                .full
                .iter()
                .enumerate()
                .filter(|(_, kept)| kept.id != id && Some(kept.id) != on_screen)
                .min_by_key(|(_, kept)| kept.used)
                .map(|(at, _)| at);
            let Some(at) = oldest else { break };
            self.full.remove(at);
        }
    }

    /// Takes in what the readers have read since the last look.
    pub fn take_arrived(&mut self) -> Vec<Arrived> {
        let mut arrived = Vec::new();
        while let Ok(read) = self.done_rx.try_recv() {
            let (id, length) = (read.id, read.length);
            let came = self.arrived(read);
            let small = matches!(came, Some(Arrived::Small(_)));
            arrived.extend(came);
            if small {
                arrived.push(Arrived::Length(id, length));
            }
        }
        arrived
    }

    /// Takes in one reading; one asked for before the item's file was read anew, or for an item
    /// that has left the bin, is let go.
    fn arrived(&mut self, read: Read) -> Option<Arrived> {
        let id = read.id;
        if self.stamps.get(&id) != Some(&read.stamp) {
            return None;
        }
        match read.want {
            Want::Small => {
                self.small_coming.remove(&id);
                match read.result {
                    Ok((picture, whole)) => {
                        self.small.insert(id, picture);
                        if whole {
                            self.whole.insert(id);
                        } else {
                            self.whole.remove(&id);
                        }
                        // A full-size picture of the file as it was before is let go (the screens
                        // then take the new small copy, and the new full-size picture is read).
                        if self.full.iter().any(|kept| kept.id == id && kept.stamp != read.stamp) {
                            self.drop_full(id);
                        }
                        Some(Arrived::Small(id))
                    }
                    Err(problem) => Some(Arrived::Refused(id, problem)),
                }
            }
            Want::Length => Some(Arrived::Length(id, read.length)),
            Want::FullNow | Want::FullAhead => {
                self.full_coming.remove(&id);
                match read.result {
                    Ok((picture, _)) => {
                        self.keep_full(id, picture, read.stamp);
                        Some(Arrived::Full(id))
                    }
                    Err(problem) => {
                        crate::applog::event("media", &format!("could not read the full-size picture: {problem}"));
                        self.full_refused.insert(id);
                        None
                    }
                }
            }
        }
    }

    /// How many full-size pictures are kept.
    #[cfg(test)]
    fn full_kept(&self) -> Vec<Uuid> {
        self.full.iter().map(|kept| kept.id).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn picture(side: u32) -> Picture {
        Picture::from_rgba(side, side, vec![0; (side * side * 4) as usize]).unwrap()
    }

    fn quiet() -> BinPictures {
        BinPictures::new(|| {})
    }

    /// A full-size picture read for an item, as a reader hands it back.
    fn full_read(pictures: &BinPictures, id: Uuid, side: u32) -> Read {
        Read { id, want: Want::FullNow, stamp: pictures.stamps[&id], result: Ok((picture(side), false)), length: None }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("softpresenter-bin-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Waits for the readers until `done` says enough has come.
    fn wait_for(pictures: &mut BinPictures, mut done: impl FnMut(&[Arrived]) -> bool) -> Vec<Arrived> {
        let mut all = Vec::new();
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(10) {
            all.extend(pictures.take_arrived());
            if done(&all) {
                return all;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("the readers didn't finish: {all:?}");
    }

    #[test]
    fn only_a_few_full_size_pictures_are_kept_and_never_the_one_on_screen() {
        let mut pictures = quiet();
        let ids: Vec<Uuid> = (0..5).map(|_| Uuid::new_v4()).collect();
        for id in &ids {
            pictures.set_small(*id, picture(8), false);
        }
        let path = Path::new("/x/a.png");
        // The first goes on screen and stays there while the others are read.
        let read = full_read(&pictures, ids[0], 16);
        assert_eq!(pictures.arrived(read), Some(Arrived::Full(ids[0])));
        assert_eq!(pictures.for_screens(Some((ids[0], path))).map(|p| p.width()), Some(16));
        for id in &ids[1..] {
            let read = full_read(&pictures, *id, 16);
            pictures.arrived(read);
        }
        let kept = pictures.full_kept();
        assert_eq!(kept.len(), FULL_KEPT);
        assert!(kept.contains(&ids[0]), "the one on screen stays");
        assert!(kept.contains(&ids[4]) && kept.contains(&ids[3]), "then the newest");
        // Off the screen, it is let go like any other once it is the oldest.
        pictures.for_screens(None);
        for id in &ids[1..3] {
            let read = full_read(&pictures, *id, 16);
            pictures.arrived(read);
        }
        assert!(!pictures.full_kept().contains(&ids[0]));
    }

    #[test]
    fn a_reading_asked_for_before_a_file_changed_or_left_the_bin_is_let_go() {
        let mut pictures = quiet();
        let id = Uuid::new_v4();
        pictures.set_small(id, picture(8), false);
        let stale = full_read(&pictures, id, 16);
        // The file is written over: what was being read of the old one doesn't count.
        pictures.set_small(id, picture(9), false);
        assert_eq!(pictures.arrived(stale), None);
        assert!(pictures.full_kept().is_empty());
        let fresh = full_read(&pictures, id, 16);
        pictures.forget(id);
        assert_eq!(pictures.arrived(fresh), None, "the item left the bin");
        assert!(pictures.small(id).is_none() && pictures.full_kept().is_empty());
    }

    #[test]
    fn a_picture_written_over_on_screen_stays_until_the_new_one_has_been_read() {
        let mut pictures = quiet();
        let id = Uuid::new_v4();
        let path = Path::new("/x/notice.png");
        pictures.set_small(id, picture(8), false);
        let read = full_read(&pictures, id, 16);
        pictures.arrived(read);
        assert_eq!(pictures.for_screens(Some((id, path))).map(|p| p.width()), Some(16));
        // Written over: until the new small copy comes, the screens keep the old picture.
        pictures.read(id, path);
        assert_eq!(pictures.for_screens(Some((id, path))).map(|p| p.width()), Some(16));
        assert!(pictures.screens_have_full());
        // Half written, it can't be read yet: still the old picture.
        let refused = Read {
            id,
            want: Want::Small,
            stamp: pictures.stamps[&id],
            result: Err("half written".into()),
            length: None,
        };
        assert!(matches!(pictures.arrived(refused), Some(Arrived::Refused(..))));
        assert_eq!(pictures.for_screens(Some((id, path))).map(|p| p.width()), Some(16));
        // Read at last: the old full-size picture goes, so the screens take the new small copy
        // (the app is told the screens don't have the full one), and the new full one is read.
        pictures.read(id, path);
        let new =
            Read { id, want: Want::Small, stamp: pictures.stamps[&id], result: Ok((picture(9), false)), length: None };
        assert_eq!(pictures.arrived(new), Some(Arrived::Small(id)));
        assert!(!pictures.screens_have_full() && pictures.full_kept().is_empty());
        assert_eq!(pictures.for_screens(Some((id, path))).map(|p| p.width()), Some(9));
        assert!(pictures.full_coming.contains(&id));
        // At the start the full-size picture can come before the small copy: the same file, kept.
        let other = Uuid::new_v4();
        pictures.read(other, Path::new("/x/none.png"));
        let stamp = pictures.stamps[&other];
        pictures.arrived(Read {
            id: other,
            want: Want::FullNow,
            stamp,
            result: Ok((picture(16), false)),
            length: None,
        });
        pictures.arrived(Read { id: other, want: Want::Small, stamp, result: Ok((picture(8), false)), length: None });
        assert!(pictures.full_kept().contains(&other));
    }

    #[test]
    fn the_screens_get_the_small_copy_until_the_full_size_picture_is_read() {
        let dir = temp_dir("full");
        let (big, tiny) = (dir.join("big.png"), dir.join("tiny.png"));
        image::RgbaImage::from_pixel(2000, 1000, image::Rgba([200, 10, 10, 255])).save(&big).unwrap();
        image::RgbaImage::from_pixel(100, 50, image::Rgba([10, 10, 200, 255])).save(&tiny).unwrap();
        let mut pictures = quiet();
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        pictures.read(a, &big);
        pictures.read(b, &tiny);
        assert!(pictures.small_coming.contains(&a) && pictures.any_coming());
        wait_for(&mut pictures, |all| all.contains(&Arrived::Small(a)) && all.contains(&Arrived::Small(b)));
        assert!(!pictures.any_coming());
        assert_eq!(pictures.small(a).map(|p| (p.width(), p.height())), Some((crate::media::SMALL_SIDE, 480)));
        // On screen: the small copy stands in, and the full-size picture is read.
        let first = pictures.for_screens(Some((a, &big))).unwrap();
        assert_eq!(first.width(), crate::media::SMALL_SIDE);
        assert!(!pictures.has_full(a, &big) && !pictures.screens_have_full());
        wait_for(&mut pictures, |all| all.contains(&Arrived::Full(a)));
        assert_eq!(pictures.for_screens(Some((a, &big))).map(|p| p.width()), Some(2000));
        assert!(pictures.screens_have_full());
        // A picture no bigger than its small copy is its own full-size picture: nothing more is read.
        assert!(pictures.has_full(b, &tiny));
        assert_eq!(pictures.for_screens(Some((b, &tiny))).map(|p| p.width()), Some(100));
        assert!(pictures.full_coming.is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_file_that_cant_be_read_says_so_and_its_full_size_picture_isnt_asked_for_again() {
        let dir = temp_dir("broken");
        let broken = dir.join("broken.png");
        std::fs::write(&broken, b"not a picture").unwrap();
        let mut pictures = quiet();
        let id = Uuid::new_v4();
        pictures.read(id, &broken);
        let all = wait_for(&mut pictures, |all| !all.is_empty());
        assert!(matches!(&all[..], [Arrived::Refused(got, why)] if *got == id && why.contains("could not be opened")));
        // Asked for on screen, it is tried once; a second ask doesn't read it again.
        assert!(pictures.for_screens(Some((id, &broken))).is_none());
        let started = Instant::now();
        while pictures.full_coming.contains(&id) && started.elapsed() < Duration::from_secs(10) {
            pictures.take_arrived();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(pictures.full_refused.contains(&id));
        pictures.for_screens(Some((id, &broken)));
        assert!(!pictures.full_coming.contains(&id));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_new_item_on_screen_goes_before_one_still_waiting_to_be_read() {
        let mut pictures = quiet();
        // No readers are started for jobs put straight in line, so the line can be looked at.
        let (a, b, c) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        for id in [a, b, c] {
            pictures.stamps.insert(id, 0);
            pictures.full_coming.insert(id);
        }
        let job = |id, want| Job { id, path: PathBuf::from("/x/none.png"), want, stamp: 0 };
        if let Ok(mut queue) = pictures.full_reader.queue.lock() {
            queue.1 = 1; // as if the reader were busy
            queue.0.push_back(job(a, Want::FullNow));
            queue.0.push_back(job(b, Want::FullAhead));
        }
        // c goes on screen: a, waiting for the screens, isn't wanted now; b, read ahead, still is.
        pictures.full_coming.remove(&c);
        pictures.for_screens(Some((c, Path::new("/x/c.png"))));
        let waiting: Vec<(Uuid, Want)> =
            pictures.full_reader.queue.lock().unwrap().0.iter().map(|job| (job.id, job.want)).collect();
        assert_eq!(waiting, vec![(c, Want::FullNow), (b, Want::FullAhead)]);
        assert!(!pictures.full_coming.contains(&a));
    }
}
