//! Adding the sample backgrounds (see `backgrounds`) to the media bin. They are copied into the
//! library's Media folder like any file the church brings, so they can be renamed or deleted and
//! survive an update of the app; each item remembers which sample it is (`MediaItem::sample`),
//! and a media playlist marked as theirs gathers them in order. Adding again puts back only the
//! ones the bin doesn't have, into the same playlist. What one add did is kept as an [`Added`],
//! so one Undo takes it all away (and one Redo puts it back with the same ids).

use std::path::{Path, PathBuf};

use sp_model::{ImageFit, MediaItem, Uuid};

use crate::backgrounds;
use crate::media::Playlist;

/// The name of the media playlist that gathers them, until a church renames it.
pub const PLAYLIST_NAME: &str = "Sample Backgrounds";

/// The sample backgrounds the bin hasn't got, by file name, in the order they are added.
pub fn missing(bin: &[MediaItem]) -> Vec<&'static str> {
    backgrounds::FILES
        .into_iter()
        .filter(|name| !bin.iter().any(|item| item.sample.as_deref() == Some(*name)))
        .collect()
}

/// Whether the media bin's menu and its empty place offer to add them: at least one is missing,
/// and the files came with this copy of the app.
pub fn offered(bin: &[MediaItem]) -> bool {
    !missing(bin).is_empty() && backgrounds::available()
}

/// What one add did, to take away again (and put back).
#[derive(Clone, Debug, PartialEq)]
pub struct Added {
    /// The items that joined the bin, in order.
    pub items: Vec<MediaItem>,
    /// The files copied into the Media folder for them: Undo deletes these and no others (a file
    /// already there, from an earlier add, stays).
    pub copied: Vec<PathBuf>,
    /// The media playlist that gathers them.
    pub playlist: Uuid,
    /// Whether this add made it, so Undo takes the whole playlist away.
    pub made: bool,
    /// The files this add put into a playlist that was there already, for Undo to take out.
    pub joined: Vec<PathBuf>,
    /// What the media strip showed before: All Media, or a media playlist.
    pub shown_before: Option<Uuid>,
}

/// Adds the missing sample backgrounds: copies each into `media` (the library's Media folder),
/// lists the items to put in the bin, puts them in the sample playlist (made here if it isn't
/// there) and shows that playlist. `again` is an earlier add being redone: its ids are kept. On
/// an error nothing is left changed.
pub fn add(
    media: &Path,
    bin: &[MediaItem],
    playlists: &mut Vec<Playlist>,
    shown: &mut Option<Uuid>,
    again: Option<&Added>,
) -> Result<Added, String> {
    let wanted = missing(bin);
    if wanted.is_empty() {
        return Err("The sample backgrounds are all in the media bin already.".to_owned());
    }
    std::fs::create_dir_all(media).map_err(|e| format!("Could not make the Media folder: {e}"))?;
    let in_bin = |path: &Path| bin.iter().any(|item| item.path == path);
    let (mut items, mut copied) = (Vec::new(), Vec::new());
    for name in wanted {
        let placed = backgrounds::find(name)
            .ok_or_else(|| "The sample backgrounds didn't come with this copy of SoftPresenter.".to_owned())
            .and_then(|from| place(&from, media, name, &in_bin));
        let (path, fresh) = match placed {
            Ok(placed) => placed,
            Err(problem) => {
                for file in &copied {
                    let _ = std::fs::remove_file(file);
                }
                return Err(problem);
            }
        };
        if fresh {
            copied.push(path.clone());
        }
        let id = again.and_then(|before| before.items.iter().find(|item| item.sample.as_deref() == Some(name)));
        let file = path.file_name().map_or_else(|| name.to_owned(), |file| file.to_string_lossy().into_owned());
        items.push(MediaItem {
            id: id.map_or_else(Uuid::new_v4, |item| item.id),
            name: file,
            path,
            fit: ImageFit::Fill,
            // A video goes round and round behind the words; a picture has nothing to loop.
            looping: true,
            sample: Some(name.to_owned()),
        });
    }
    let found = playlists.iter().position(|playlist| playlist.sample);
    let made = found.is_none();
    let at = found.unwrap_or_else(|| {
        let mut playlist = Playlist::new(unused_name(playlists));
        playlist.sample = true;
        if let Some(before) = again.filter(|before| before.made) {
            playlist.id = before.playlist;
        }
        playlists.push(playlist);
        playlists.len() - 1
    });
    let playlist = &mut playlists[at];
    let mut joined = Vec::new();
    for item in &items {
        if !playlist.has(&item.path) {
            playlist.files.push(item.path.clone());
            if !made {
                joined.push(item.path.clone());
            }
        }
    }
    let added = Added { items, copied, playlist: playlist.id, made, joined, shown_before: *shown };
    *shown = Some(added.playlist);
    Ok(added)
}

impl Added {
    /// Takes it all away: deletes the files it copied, takes its items out of the sample
    /// playlist (the playlist itself, if it made it) and shows what the strip showed before.
    /// Returns the ids of the bin items to remove.
    pub fn take_back(&self, playlists: &mut Vec<Playlist>, shown: &mut Option<Uuid>) -> Vec<Uuid> {
        for file in &self.copied {
            let _ = std::fs::remove_file(file);
        }
        if self.made {
            playlists.retain(|playlist| playlist.id != self.playlist);
        } else if let Some(playlist) = playlists.iter_mut().find(|playlist| playlist.id == self.playlist) {
            playlist.files.retain(|file| !self.joined.contains(file));
        }
        if *shown == Some(self.playlist) {
            *shown = self.shown_before.filter(|id| playlists.iter().any(|playlist| playlist.id == *id));
        }
        self.items.iter().map(|item| item.id).collect()
    }
}

/// "Sample Backgrounds", or with a number when a playlist has that name already.
fn unused_name(playlists: &[Playlist]) -> String {
    let taken = |name: &str| playlists.iter().any(|playlist| playlist.name == name);
    std::iter::once(PLAYLIST_NAME.to_owned())
        .chain((2..).map(|n| format!("{PLAYLIST_NAME} {n}")))
        .find(|name| !taken(name))
        .unwrap_or_default()
}

/// Puts a sample in `media` under its own name; "Olive 2.jpg" when a different file has that
/// name, or the bin has the file. A copy left by an earlier add is used as it is, never written
/// over. Returns where it is, and whether it was copied now.
fn place(from: &Path, media: &Path, name: &str, in_bin: &dyn Fn(&Path) -> bool) -> Result<(PathBuf, bool), String> {
    let (stem, ending) = match name.rsplit_once('.') {
        Some((stem, ending)) => (stem, format!(".{ending}")),
        None => (name, String::new()),
    };
    for n in 1..1000 {
        let file = if n == 1 { name.to_owned() } else { format!("{stem} {n}{ending}") };
        let path = media.join(&file);
        if in_bin(&path) {
            continue;
        }
        if !path.exists() {
            crate::images::copy_into(from, media, &file)?;
            return Ok((path, true));
        }
        if crate::pack::same_contents(&path, from) {
            return Ok((path, false));
        }
    }
    Err(format!("Could not find a name for {name} in the Media folder."))
}

impl crate::App {
    /// Adds the sample backgrounds the bin hasn't got (see [`add`]), shows the playlist that
    /// gathers them, and keeps the add as one step for Undo.
    pub(crate) fn add_sample_backgrounds(&mut self) -> Result<Added, String> {
        let media = crate::bible_store::data_dir()
            .map(|dir| dir.join("Media"))
            .ok_or_else(|| "Could not find where to keep pictures on this computer.".to_owned())?;
        let bin = self.show.media().to_vec();
        let running = self.running.as_mut().ok_or_else(|| "The window isn't open.".to_owned())?;
        let state = &mut running.home_state;
        let added = add(&media, &bin, &mut state.media_playlists, &mut state.media_playlist_shown, None)?;
        state.media_playlists_changed = true;
        crate::applog::event(
            "media",
            &format!("added {} sample backgrounds in the playlist {}", added.items.len(), PLAYLIST_NAME),
        );
        self.bring_in(&added);
        // Anything deleted just before is in the history first, so Undo takes the steps in order.
        self.keep_removed();
        self.history.remember_removed(crate::removed::Removed::Samples(Box::new(added.clone())));
        Ok(added)
    }

    /// Puts the items of an add in the bin; their pictures are read on the readers' threads, as
    /// at the start, so the bin fills in while the window stays ready.
    fn bring_in(&mut self, added: &Added) {
        for item in &added.items {
            self.bin_pictures.read(item.id, &item.path);
            self.show.add_media(item.clone());
        }
        self.save_media_list();
        if let Some(running) = self.running.as_ref() {
            running.home_window.request_redraw();
        }
    }

    /// Undo and Redo of an add: takes it away again (`put_back` false) or puts it back with the
    /// same ids. False, changing nothing, when what it needs has changed so much that it can't be.
    pub(crate) fn redo_samples(&mut self, added: &Added, put_back: bool) -> bool {
        let Some(running) = self.running.as_mut() else { return false };
        let state = &mut running.home_state;
        if put_back {
            let bin = self.show.media().to_vec();
            let Ok(again) = add(
                &crate::bible_store::data_dir().unwrap_or_default().join("Media"),
                &bin,
                &mut state.media_playlists,
                &mut state.media_playlist_shown,
                Some(added),
            ) else {
                return false;
            };
            state.media_playlists_changed = true;
            self.bring_in(&again);
            return true;
        }
        let ids = added.take_back(&mut state.media_playlists, &mut state.media_playlist_shown);
        state.media_playlists_changed = true;
        if self.media_cycle.as_ref().is_some_and(|cycle| cycle.playlist == added.playlist) {
            self.stop_media_playlist();
        }
        let mut media_changed = false;
        for id in ids {
            if let Some(index) = self.show.media().iter().position(|item| item.id == id) {
                media_changed |= self.remove_bin_item(index);
            }
        }
        if media_changed {
            self.refresh_output();
        }
        self.save_media_list();
        if let Some(running) = self.running.as_ref() {
            running.home_window.request_redraw();
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder of its own, taken away with it.
    struct Folder(PathBuf);

    impl Folder {
        fn new() -> Folder {
            Folder(std::env::temp_dir().join(format!("sp-samples-{}", Uuid::new_v4())).join("Media"))
        }

        fn files(&self) -> Vec<String> {
            let mut names: Vec<String> = std::fs::read_dir(&self.0)
                .map(|dir| dir.flatten().map(|entry| entry.file_name().to_string_lossy().into_owned()).collect())
                .unwrap_or_default();
            names.sort();
            names
        }
    }

    impl Drop for Folder {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(self.0.parent().unwrap_or(&self.0));
        }
    }

    fn sorted(names: &[&str]) -> Vec<String> {
        let mut names: Vec<String> = names.iter().map(|name| (*name).to_owned()).collect();
        names.sort();
        names
    }

    #[test]
    fn the_menu_item_is_offered_only_while_one_of_the_seven_is_missing() {
        assert!(backgrounds::available(), "the samples are found in a development build");
        let mut bin = Vec::new();
        assert!(offered(&bin));
        assert_eq!(missing(&bin), backgrounds::FILES);
        for name in backgrounds::FILES {
            let mut item = MediaItem::new(name.to_owned(), PathBuf::from("/media").join(name));
            item.sample = Some(name.to_owned());
            bin.push(item);
            assert_eq!(offered(&bin), bin.len() < 7, "{} of 7 in the bin", bin.len());
        }
        assert!(missing(&bin).is_empty());
        // Another file with a sample's name isn't that sample; the item's own word is what counts.
        bin.pop();
        bin.push(MediaItem::new("Drifting Light.mp4".to_owned(), "/other/Drifting Light.mp4".into()));
        assert_eq!(missing(&bin), ["Drifting Light.mp4"]);
        // A sample renamed on disk is still that sample.
        bin[0].name = "Chapel.jpg".to_owned();
        assert_eq!(missing(&bin), ["Drifting Light.mp4"]);
    }

    #[test]
    fn adding_all_seven_makes_one_playlist_in_order_with_the_video_looping() {
        let folder = Folder::new();
        let (mut playlists, mut shown) = (Vec::new(), None);
        let added = add(&folder.0, &[], &mut playlists, &mut shown, None).unwrap();
        let names: Vec<&str> = added.items.iter().filter_map(|item| item.sample.as_deref()).collect();
        assert_eq!(names, backgrounds::FILES);
        assert_eq!(
            added.items.iter().map(|item| item.name.as_str()).collect::<Vec<_>>(),
            backgrounds::FILES,
            "named as their files are"
        );
        let video = added.items.last().unwrap();
        assert!(crate::media::is_video(&video.path) && video.looping, "Drifting Light loops");
        assert_eq!(folder.files(), sorted(&backgrounds::FILES), "copied into the library's media");
        assert!(added.items.iter().all(|item| item.path.starts_with(&folder.0) && item.path.is_file()));
        assert_eq!(playlists.len(), 1, "one playlist");
        let playlist = &playlists[0];
        assert_eq!((playlist.name.as_str(), playlist.sample, playlist.id), (PLAYLIST_NAME, true, added.playlist));
        assert_eq!(playlist.files, added.items.iter().map(|item| item.path.clone()).collect::<Vec<_>>());
        assert_eq!(shown, Some(playlist.id), "it opens in the bin");
        assert!(added.made && added.joined.is_empty() && added.copied.len() == 7);
        // With all seven in the bin, there is nothing to add.
        let bin = added.items.clone();
        assert!(add(&folder.0, &bin, &mut playlists, &mut shown, None).is_err());
        assert_eq!(playlists.len(), 1);
    }

    #[test]
    fn adding_again_after_deleting_two_puts_back_only_those_in_the_same_playlist() {
        let folder = Folder::new();
        let (mut playlists, mut shown) = (Vec::new(), None);
        let first = add(&folder.0, &[], &mut playlists, &mut shown, None).unwrap();
        let playlist = playlists[0].id;
        // The church renames the playlist and leaves the strip on All Media; Night Blue and Olive
        // go from the bin, and Olive's file from the disk too.
        playlists[0].name = "Backgrounds".to_owned();
        shown = None;
        let bin: Vec<MediaItem> = first
            .items
            .iter()
            .filter(|item| !matches!(item.sample.as_deref(), Some("Night Blue.jpg" | "Olive.jpg")))
            .cloned()
            .collect();
        let olive = first.items.iter().find(|item| item.sample.as_deref() == Some("Olive.jpg")).unwrap();
        std::fs::remove_file(&olive.path).unwrap();
        let second = add(&folder.0, &bin, &mut playlists, &mut shown, None).unwrap();
        let names: Vec<&str> = second.items.iter().filter_map(|item| item.sample.as_deref()).collect();
        assert_eq!(names, ["Night Blue.jpg", "Olive.jpg"], "only the missing ones, in order");
        assert_eq!(playlists.len(), 1, "never a second playlist");
        assert_eq!((playlists[0].id, playlists[0].name.as_str()), (playlist, "Backgrounds"));
        assert_eq!(
            playlists[0].files,
            first.items.iter().map(|item| item.path.clone()).collect::<Vec<_>>(),
            "back in their places, none twice"
        );
        assert!(!second.made && second.joined.is_empty(), "their files were listed still");
        assert_eq!(second.copied, std::slice::from_ref(&olive.path), "the file gone from the disk is copied again");
        assert_eq!(shown, Some(playlist));
        assert_eq!(folder.files(), sorted(&backgrounds::FILES), "no copy twice");
    }

    #[test]
    fn a_file_with_a_sample_name_that_is_something_else_is_never_written_over() {
        let folder = Folder::new();
        std::fs::create_dir_all(&folder.0).unwrap();
        std::fs::write(folder.0.join("Olive.jpg"), b"the church's own picture").unwrap();
        let (mut playlists, mut shown) = (Vec::new(), None);
        let added = add(&folder.0, &[], &mut playlists, &mut shown, None).unwrap();
        let olive = added.items.iter().find(|item| item.sample.as_deref() == Some("Olive.jpg")).unwrap();
        assert_eq!(olive.name, "Olive 2.jpg");
        assert_eq!(std::fs::read(folder.0.join("Olive.jpg")).unwrap(), b"the church's own picture");
        // Undo takes away what was copied and leaves theirs.
        added.take_back(&mut playlists, &mut shown);
        assert_eq!(folder.files(), ["Olive.jpg"]);
    }

    #[test]
    fn one_undo_takes_the_whole_add_away_and_redo_puts_it_back_as_it_was() {
        let folder = Folder::new();
        let (mut playlists, mut shown) = (Vec::new(), None);
        let added = add(&folder.0, &[], &mut playlists, &mut shown, None).unwrap();
        let ids = added.take_back(&mut playlists, &mut shown);
        assert_eq!(ids, added.items.iter().map(|item| item.id).collect::<Vec<_>>(), "every bin item");
        assert!(playlists.is_empty(), "the playlist is gone");
        assert!(folder.files().is_empty(), "the files are gone");
        assert_eq!(shown, None, "All Media is shown again");
        let again = add(&folder.0, &[], &mut playlists, &mut shown, Some(&added)).unwrap();
        assert_eq!(again, added, "the same ids and the same playlist");
        assert_eq!(folder.files(), sorted(&backgrounds::FILES));
    }

    #[test]
    fn undoing_a_second_add_leaves_the_first_one_and_its_playlist() {
        let folder = Folder::new();
        let (mut playlists, mut shown) = (Vec::new(), None);
        let first = add(&folder.0, &[], &mut playlists, &mut shown, None).unwrap();
        let bin: Vec<MediaItem> =
            first.items.iter().filter(|item| item.sample.as_deref() != Some("Still.jpg")).cloned().collect();
        // Still's file stays on the disk, but is taken off the playlist.
        playlists[0].files.retain(|file| *file != first.items[0].path);
        shown = None;
        let second = add(&folder.0, &bin, &mut playlists, &mut shown, None).unwrap();
        assert!(!second.made && second.joined == [first.items[0].path.clone()] && second.copied.is_empty());
        assert_eq!(playlists[0].files.len(), 7);
        let ids = second.take_back(&mut playlists, &mut shown);
        assert_eq!(ids, [second.items[0].id]);
        assert_eq!(playlists.len(), 1);
        assert_eq!(playlists[0].files.len(), 6, "only what the second add put in is taken out");
        assert_eq!(folder.files().len(), 7, "a file an earlier add copied stays");
        assert_eq!(shown, None);
    }

    #[test]
    fn a_sample_playlist_is_found_by_its_mark_not_its_name() {
        let folder = Folder::new();
        let mut other = Playlist::new(PLAYLIST_NAME.to_owned());
        other.files.push("/a/b.jpg".into());
        let (mut playlists, mut shown) = (vec![other], Some(Uuid::new_v4()));
        let added = add(&folder.0, &[], &mut playlists, &mut shown, None).unwrap();
        assert_eq!(playlists.len(), 2);
        assert_eq!(playlists[1].name, "Sample Backgrounds 2", "a playlist of the church's own keeps its name");
        assert!(added.made && playlists[1].sample && !playlists[0].sample);
    }

    /// Runs one frame of the window as the app draws it, with these events.
    fn frame(
        ctx: &egui::Context,
        show: &sp_engine::Show,
        state: &mut crate::home::HomeState,
        events: Vec<egui::Event>,
    ) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1280.0, 800.0))),
            events,
            ..egui::RawInput::default()
        };
        let output = ctx.run_ui(input, |ui| {
            crate::home::home_ui(
                ui,
                show,
                state,
                &sp_model::Theme::builtin(),
                &mut |_| None,
                crate::home::Preview::default(),
                None,
            );
        });
        output.drop_without_applying_deltas();
    }

    fn click(ctx: &egui::Context, show: &sp_engine::Show, state: &mut crate::home::HomeState, at: egui::Pos2) {
        let button = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(ctx, show, state, vec![egui::Event::PointerMoved(at)]);
        frame(ctx, show, state, vec![button(true)]);
        frame(ctx, show, state, vec![button(false)]);
    }

    #[test]
    fn an_empty_bin_offers_the_samples_beside_its_add_button_and_a_click_asks_for_them() {
        use crate::home::MediaRequest;
        let song = std::sync::Arc::new(sp_model::parse_song_text(
            "Lantern", "A light
",
        ));
        let mut show = sp_engine::Show::new(vec![song]);
        let (ctx, mut state) = (egui::Context::default(), crate::home::HomeState::default());
        frame(&ctx, &show, &mut state, Vec::new());
        frame(&ctx, &show, &mut state, Vec::new());
        let button = ctx.read_response(crate::studio::samples_button_id()).expect("the button, in an empty bin").rect;
        assert!(button.width() > 100.0 && button.height() > 20.0, "{button:?}");
        // A click on it asks for the samples; a click on the add place beside it still asks for
        // the file picker.
        click(&ctx, &show, &mut state, button.center());
        assert_eq!(state.media_request, Some(MediaRequest::AddSamples));
        state.media_request = None;
        click(&ctx, &show, &mut state, egui::pos2(button.min.x - 200.0, button.center().y));
        assert_eq!(state.media_request, Some(MediaRequest::Choose), "the add place still goes to the file picker");
        // With all seven in the bin, the bin isn't empty and nothing offers them.
        let ctx = egui::Context::default();
        for name in backgrounds::FILES {
            let mut item = MediaItem::new(name.to_owned(), PathBuf::from("/media").join(name));
            item.sample = Some(name.to_owned());
            show.add_media(item);
        }
        frame(&ctx, &show, &mut state, Vec::new());
        frame(&ctx, &show, &mut state, Vec::new());
        assert!(ctx.read_response(crate::studio::samples_button_id()).is_none());
        assert!(!offered(show.media()));
    }
}
