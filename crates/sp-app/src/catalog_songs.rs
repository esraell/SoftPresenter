//! The song catalog's songs in the window (the engine is `catalog`, the app's side of it
//! `catalog_cli`): found with the find box, looked at in the slides area, and put to use. All of
//! it only where the catalog is offered (`catalog::offered`) and a songbook is on this computer.
//!
//! - **Found:** the find box looks in the songbooks too, on a thread of its own
//!   ([`crate::find::Asker`]), the operator's and the church's languages first
//!   (`songbooks::recommended_now`), leaving out the songs the library has already. Each song
//!   found is made into the app's song on that thread, so the window has it to hand.
//! - **Looked at:** picked, a song is shown in the slides area from one of two places the show
//!   keeps for it, never the one on screen, so the screens keep what they show (as the Bible
//!   page's two places do). It goes on screen from there like any song. Those places are the
//!   app's own presentations: never saved, edited, listed or searched as the library's.
//! - **Used:** a song **joins the library** (a library named Worship Leader App) only when it
//!   is added to a playlist (Command-Return in the find box, Add on its bar), dragged into a
//!   playlist or a library, or edited (Edit Words, Edit on its bar). The playlist's or the
//!   library's entry goes in at once, under the key the song has once filed (its id:
//!   `song_ids::library_key`), and the app files the song before the next frame ([`Join`]).
//!   Looking at a song, or putting it on screen, adds nothing.

use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use sp_engine::{Show, SlideRef};
use sp_model::{Presentation, Uuid};

use crate::catalog::{self, Catalog, Hit};
use crate::find::{Asker, Work};
use crate::home::HomeState;
use crate::i18n::tf;
use crate::service::Item;

/// How many catalog songs the find box asks for.
pub const LISTED: usize = 6;

/// What the song list knows the two places for a song looked at by: keys no playlist or library
/// has, so neither ever names them.
pub const SLOT_KEYS: [&str; 2] = ["catalog-look-1", "catalog-look-2"];

/// A song of the catalog the find box found: what the search said of it, and the song made of
/// it, with the ids it keeps if it joins the library.
#[derive(Clone, Debug)]
pub struct Row {
    pub hit: Hit,
    pub song: Arc<Presentation>,
}

impl Row {
    /// The name of its songbook, in the songbook's own language, as the setup and Settings name
    /// it.
    pub fn songbook(&self) -> String {
        catalog::songbook(&self.hit.lang).map_or_else(|| self.hit.lang.clone(), |book| book.name().to_owned())
    }

    /// Its first line as its first slide shows it (a section the pack marks starts each line
    /// with a capital), else as the search found it.
    pub fn line(&self) -> String {
        let first = self.song.slides().next().and_then(|(_, slide)| {
            let words = slide.objects.iter().find_map(|object| object.as_text())?;
            words.lines().map(str::trim).find(|line| !line.is_empty()).map(str::to_owned)
        });
        first.unwrap_or_else(|| self.hit.line.clone())
    }
}

/// What is done once a song has joined the library, besides being filed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Then {
    /// Nothing more: it was put in a playlist or a library.
    Nothing,
    /// Opened in the Edit view.
    Edit,
    /// Its words opened as text (Edit Words).
    Words,
}

/// A song to join the library, and what is done then.
#[derive(Clone, Debug)]
pub struct Join {
    pub song: Arc<Presentation>,
    pub then: Then,
}

/// What the find box asked the catalog: the words, the languages preferred, the songbooks on
/// this computer (with their songs), and the songs as they were (by `kept::SongsSeen`'s epoch,
/// so a song that joins the library since is left out).
type Asked = (String, Vec<&'static str>, BTreeMap<String, usize>, u64);

/// The catalog's songs in the window.
pub struct CatalogSongs {
    /// The catalog on this computer, where the window offers it.
    pub engine: Option<Arc<Catalog>>,
    /// The two places among the show's presentations for a song looked at.
    pub slots: Option<[usize; 2]>,
    /// The song each place holds, as made: the app shows a copy with ids of its own there, and
    /// this is what joins the library from it.
    pub held: [Option<Arc<Presentation>>; 2],
    /// A song to look at, and the place for it: the app puts it there after this frame, and
    /// chooses it.
    pub look: Option<(usize, Arc<Presentation>)>,
    /// Songs put to use, for the app to file after this frame.
    pub joins: Vec<Join>,
    finder: Asker<Asked, Vec<Row>>,
}

impl Default for CatalogSongs {
    fn default() -> CatalogSongs {
        CatalogSongs {
            engine: None,
            slots: None,
            held: [None, None],
            look: None,
            joins: Vec::new(),
            finder: Asker::new("find songs"),
        }
    }
}

impl CatalogSongs {
    /// The catalog's songs in a window that looks in `engine` (none where the catalog isn't
    /// offered), with these two places for a song looked at.
    pub fn new(engine: Option<Arc<Catalog>>, slots: [usize; 2]) -> CatalogSongs {
        CatalogSongs { engine, slots: Some(slots), ..CatalogSongs::default() }
    }

    /// Whether a presentation is one of the places for a song looked at.
    pub fn holds(&self, presentation: usize) -> bool {
        self.slots.is_some_and(|slots| slots.contains(&presentation))
    }

    /// The song looked at in a place, as made, by the place's presentation.
    pub fn held_at(&self, presentation: usize) -> Option<&Arc<Presentation>> {
        let place = self.slots?.iter().position(|slot| *slot == presentation)?;
        self.held[place].as_ref()
    }

    /// Asks for a song to be looked at, in the place that isn't on screen (`live`, the
    /// presentation on screen).
    pub fn look_at(&mut self, live: Option<usize>, song: Arc<Presentation>) {
        let place = match self.slots {
            Some([first, _]) if live == Some(first) => 1,
            _ => 0,
        };
        self.look = Some((place, song));
    }

    /// Asks for a song to join the library (once, however often it is asked in a frame), and
    /// returns the key it is filed under, which a playlist or a library can name at once.
    pub fn join(&mut self, song: &Arc<Presentation>, then: Then) -> String {
        if !self.joins.iter().any(|join| join.song.id == song.id) {
            self.joins.push(Join { song: song.clone(), then });
        }
        key_of(song)
    }

    /// The catalog's songs for what is typed, best first, or `None` while they are still being
    /// found: asked for when the words, the languages preferred, the songbooks here or the songs
    /// (`epoch`) have changed. `skip` (the catalog's songs the library has, by key) is worked out
    /// only then. None at all where the catalog isn't offered or no songbook is here. Never waits.
    pub fn found(
        &mut self,
        typed: &str,
        installed: &BTreeMap<String, usize>,
        epoch: u64,
        skip: impl FnOnce() -> HashSet<String>,
        wake: &egui::Context,
    ) -> Option<&[Row]> {
        let Some(engine) = self.engine.clone().filter(|_| !typed.is_empty() && !installed.is_empty()) else {
            return Some(&[]);
        };
        let prefer = crate::songbooks::recommended_now();
        let current =
            |asked: &Asked| asked.0 == typed && asked.1 == prefer && asked.2 == *installed && asked.3 == epoch;
        let ask = || {
            let asked: Asked = (typed.to_owned(), prefer.clone(), installed.clone(), epoch);
            let (typed, skip) = (typed.to_owned(), skip());
            let prefer: Vec<String> = prefer.iter().map(|code| (*code).to_owned()).collect();
            (asked, Box::new(move || search(&engine, &typed, &prefer, &skip)) as Work<Vec<Row>>)
        };
        self.finder.answer(current, ask, wake).map(|rows| rows.as_slice())
    }
}

/// What the catalog's thread does for the find box: the best songs for the words (see
/// `Catalog::search`), each made into the app's song.
fn search(engine: &Catalog, typed: &str, prefer: &[String], skip: &HashSet<String>) -> Vec<Row> {
    let found = engine.search(typed, LISTED, prefer, skip);
    let now = chrono::Utc::now().timestamp();
    found
        .hits
        .into_iter()
        .filter_map(|hit| {
            let made = engine.present(&hit.key, None, now).ok()?;
            Some(Row { hit, song: Arc::new(made.song) })
        })
        .collect()
}

/// The key a song made for the window is filed under once it joins the library.
pub fn key_of(song: &Presentation) -> String {
    crate::song_ids::library_key(song.id, &song.title)
}

/// The copies of catalog songs among the show's songs, by place, leaving out those `left_out`
/// says (the app's own presentations, songs deleted while in use): what the library has of the
/// catalog. The app's look over the copies and the find box's songs to leave out both go by it.
pub fn copies(songs: &[Arc<Presentation>], left_out: impl Fn(usize) -> bool) -> Vec<(usize, &Presentation)> {
    songs
        .iter()
        .enumerate()
        .filter(|(at, song)| {
            song.catalog.as_ref().is_some_and(|origin| origin.source == catalog::SOURCE) && !left_out(*at)
        })
        .map(|(at, song)| (at, song.as_ref()))
        .collect()
}

/// The copy of a song made for the window that a place shows: the same in every way but its
/// ids, which are its own, so nothing that finds a song or a slide by its id mistakes the copy
/// for the song once that has joined the library. (A song's fingerprint leaves its ids out, so
/// the copy is the song as made.)
pub fn shown_copy(song: &Presentation) -> Presentation {
    let mut copy = song.clone();
    copy.id = Uuid::new_v4();
    for group in &mut copy.groups {
        group.id = Uuid::new_v4();
        for slide in &mut group.slides {
            slide.id = Uuid::new_v4();
            for object in &mut slide.objects {
                object.id = Uuid::new_v4();
            }
        }
    }
    copy
}

/// Puts a song to look at in its place, as the app does after the frame that asked: the place
/// asked for unless it is on screen (then the other, so the screens keep what they show), as a
/// [`shown_copy`]. Its first slide as sung is picked and lit for a moment, as the find box does
/// for a song of the library: Enter puts it on screen. Returns the place's presentation, for the
/// app to choose, and the slides it showed before, whose thumbnails can go.
pub fn put_in_place(
    show: &mut Show,
    state: &mut HomeState,
    place: usize,
    song: Arc<Presentation>,
) -> Option<(usize, Vec<Uuid>)> {
    let slots = state.catalog.slots?;
    let live = show.live().map(|at| at.presentation);
    let place = if live == Some(slots[place % 2]) { (place + 1) % 2 } else { place % 2 };
    let slot = slots[place];
    let was = show.presentations().get(slot)?.slides().map(|(_, slide)| slide.id).collect();
    let copy = shown_copy(&song);
    let first = copy.order().into_iter().find(|at| copy.slide(*at).is_some_and(|(_, slide)| slide.enabled));
    show.replace_presentation(slot, Arc::new(copy));
    state.catalog.held[place] = Some(song);
    if let Some(slide) = first {
        let at = SlideRef { presentation: slot, slide };
        state.spotlight = Some((at, None));
        state.spotlight_scroll = 12;
        state.set_pick(show, at, false);
    }
    Some((slot, was))
}

/// What the window does once a song it put to use has joined the library (`index`, its place
/// among the songs; `already` when a copy had joined since it was made, which is the one used):
/// entries put in under the key it would have had are made to name that copy; a notice says
/// where a song new to the library went; the song filed takes the place of the one looked at in
/// the slides area, and of the open item's song when the item names it (it was dropped there);
/// Edit and Edit Words open it. Returns the song to choose, if any.
pub fn joined(
    show: &Show,
    state: &mut HomeState,
    song: &Presentation,
    index: usize,
    already: bool,
    then: Then,
) -> Option<usize> {
    let origin = song.catalog.as_ref()?;
    let planned = key_of(song);
    let key = state.song_keys.get(index).cloned().unwrap_or_else(|| planned.clone());
    if key != planned {
        rename_entries(state, &planned, &key);
    }
    let looked = state
        .catalog
        .held_at(show.selected())
        .is_some_and(|held| held.catalog.as_ref().is_some_and(|mark| mark.key == origin.key));
    let opened = state.playlist_open
        && state
            .services
            .chosen
            .and_then(|at| state.services.front().items.get(at))
            .is_some_and(|item| matches!(item, Item::Song { key: named, .. } if *named == key));
    let filed = show.presentations().get(index)?;
    if !already {
        state.notices.info(tf(
            "notices.catalog.joined",
            "{song} is in your library now, in {library}.",
            &[("song", &crate::i18n::quoted(&filed.title)), ("library", catalog::LIBRARY_NAME)],
        ));
    }
    match then {
        Then::Nothing => {}
        Then::Edit => {
            state.reflow = None;
            state.editing = crate::editor::EditDraft::of(show, SlideRef { presentation: index, slide: 0 });
        }
        Then::Words => {
            state.editing = None;
            state.reflow = Some((index, sp_model::to_song_text(filed)));
        }
    }
    (looked || opened || then != Then::Nothing).then(|| {
        state.bible_open = false;
        index
    })
}

/// Makes the playlists' and libraries' entries that name `from` name `to` instead: for a song
/// put in under the key it would have had, when a copy of it had joined the library since it
/// was made (and that copy is the one used).
pub fn rename_entries(state: &mut HomeState, from: &str, to: &str) {
    for list in &mut state.services.list {
        for item in &mut list.items {
            if let Item::Song { key, .. } = item
                && key == from
            {
                *key = to.to_owned();
                state.services.changed = true;
            }
        }
    }
    for library in &mut state.libraries {
        if let Some(at) = library.songs.iter().position(|have| have == from) {
            if library.has(to) {
                library.songs.remove(at);
            } else {
                library.songs[at] = to.to_owned();
            }
            state.libraries_changed = true;
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::catalog::fixtures::{pack, xml};
    use crate::i18n::{Language, with_language};

    /// Songbooks written for the window's tests, in a folder of the test's own: Spanish (two
    /// songs) and English (two). "Zelora" is a word made up for them, in two titles and a line;
    /// "lantern" is in an English title.
    pub(crate) fn engine(name: &str) -> Arc<Catalog> {
        let root = std::env::temp_dir().join(format!("softpresenter-test-catalog-songs-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let catalog = Catalog::at(root);
        let file =
            |name: &str, title: &str, sections: &[(&str, &str)]| (format!("{name}.xml"), xml(title, sections, &[]));
        let spanish = pack(&[
            file(
                "a",
                "Zelora en la mañana",
                &[("Verse 1", "una línea escrita para la prueba"), ("Chorus 1", "otra línea de la prueba")],
            ),
            file("b", "Canción del faro", &[("Verse 1", "zelora brilla en el puerto de prueba")]),
        ]);
        catalog.install("es", &spanish, None, None, 0).expect("the Spanish songbook goes in");
        let english = pack(&[
            file(
                "a",
                "Zelora at dawn",
                &[("Verse 1", "a line written for the test"), ("Chorus 1", "another test line")],
            ),
            file("b", "Lantern of the bay", &[("Verse 1", "a lantern line made up for a test")]),
        ]);
        catalog.install("en", &english, None, None, 0).expect("the English songbook goes in");
        Arc::new(catalog)
    }

    /// The songbooks [`engine`] puts on the computer, as the windows know them.
    pub(crate) fn installed() -> BTreeMap<String, usize> {
        [("en".to_owned(), 2), ("es".to_owned(), 2)].into()
    }

    #[test]
    fn the_find_box_has_the_operators_language_first_and_none_the_library_has() {
        let (ctx, engine) = (egui::Context::default(), engine("prefer"));
        let found = |language: Language, skip: HashSet<String>| -> Vec<Row> {
            with_language(language, || {
                let mut songs = CatalogSongs::new(Some(engine.clone()), [0, 1]);
                songs.found("zelora", &installed(), 1, || skip, &ctx).expect("answered at once in a test").to_vec()
            })
        };
        let titles = |rows: &[Row]| rows.iter().map(|row| row.hit.title.clone()).collect::<Vec<_>>();
        // Titles that start with the word first, in the operator's language before the other;
        // then a song with it in a line.
        let spanish = found(Language::Spanish, HashSet::new());
        assert_eq!(titles(&spanish), ["Zelora en la mañana", "Zelora at dawn", "Canción del faro"]);
        let english = found(Language::English, HashSet::new());
        assert_eq!(titles(&english), ["Zelora at dawn", "Zelora en la mañana", "Canción del faro"]);
        // Each with its songbook named in its own language, its first line, and the song made,
        // marked with where it came from, filed under its id once it joins.
        let first = &spanish[0];
        assert_eq!(
            (first.songbook().as_str(), first.hit.line.as_str()),
            ("Español", "una línea escrita para la prueba")
        );
        let mark = first.song.catalog.as_ref().expect("marked");
        let marked = (mark.source.as_str(), mark.key.as_str(), mark.language.as_str());
        assert_eq!(marked, (catalog::SOURCE, first.hit.key.as_str(), "es"));
        assert_eq!(key_of(&first.song), first.song.id.to_string());
        // A song the library has is left out.
        let left_out = found(Language::Spanish, [first.hit.key.clone()].into());
        assert_eq!(titles(&left_out), ["Zelora at dawn", "Canción del faro"]);
    }

    #[test]
    fn nothing_is_looked_for_without_the_catalog_a_songbook_or_words() {
        let (ctx, engine) = (egui::Context::default(), engine("none"));
        let mut offered = CatalogSongs::new(Some(engine), [0, 1]);
        let mut not_offered = CatalogSongs::new(None, [0, 1]);
        let none: BTreeMap<String, usize> = BTreeMap::new();
        assert!(not_offered.found("zelora", &installed(), 1, HashSet::new, &ctx).expect("at once").is_empty());
        assert!(offered.found("zelora", &none, 1, HashSet::new, &ctx).expect("at once").is_empty());
        assert!(offered.found("", &installed(), 1, HashSet::new, &ctx).expect("at once").is_empty());
        assert_eq!(offered.found("lantern", &installed(), 1, HashSet::new, &ctx).map(<[Row]>::len), Some(1));
    }

    #[test]
    fn a_song_looked_at_goes_where_nothing_is_on_screen_and_a_join_is_asked_once() {
        let engine = engine("places");
        let key = engine.search("lantern", 1, &[], &HashSet::new()).hits[0].key.clone();
        let song = Arc::new(engine.present(&key, None, 0).expect("made").song);
        let mut songs = CatalogSongs::new(None, [7, 8]);
        songs.look_at(None, song.clone());
        assert_eq!(songs.look.as_ref().map(|(place, _)| *place), Some(0));
        songs.look_at(Some(7), song.clone());
        assert_eq!(songs.look.as_ref().map(|(place, _)| *place), Some(1), "not the place on screen");
        songs.look_at(Some(8), song.clone());
        assert_eq!(songs.look.as_ref().map(|(place, _)| *place), Some(0));
        assert!(songs.holds(7) && songs.holds(8) && !songs.holds(6));
        // Asked twice in a frame (a drop and a click), it joins once, under its id.
        assert_eq!(songs.join(&song, Then::Nothing), song.id.to_string());
        songs.join(&song, Then::Words);
        assert_eq!(songs.joins.len(), 1);
    }

    #[test]
    fn a_song_joined_from_the_window_is_the_catalogs_to_the_letter_so_updates_still_reach_it() {
        let engine = engine("updates");
        let key = engine.search("lantern", 1, &[], &HashSet::new()).hits[0].key.clone();
        let made = engine.present(&key, None, 0).expect("made").song;
        // The copy a place shows has ids of its own and is the same song.
        let shown = shown_copy(&made);
        assert_ne!(shown.id, made.id);
        let ids = |song: &Presentation| song.slides().map(|(_, slide)| slide.id).collect::<Vec<_>>();
        assert!(ids(&shown).iter().all(|id| !ids(&made).contains(id)));
        assert_eq!(catalog::fingerprint(&shown), catalog::fingerprint(&made));
        // The song filed is the one made, unchanged: a newer text updates it; once the church has
        // changed it, the newer text is only offered.
        let mark = made.catalog.as_ref().expect("marked");
        assert_eq!(catalog::fingerprint(&made), mark.made_hash);
        let newer = catalog::Source::Has("0123456789abcdef".to_owned());
        assert_eq!(catalog::judge(&made, &newer), catalog::Verdict::Update);
        let mut changed = made.clone();
        changed.title.push_str(" (ours)");
        assert_eq!(catalog::judge(&changed, &newer), catalog::Verdict::Offer("0123456789abcdef".to_owned()));
    }

    #[test]
    fn entries_put_in_for_a_song_that_had_joined_since_name_the_copy_there() {
        let mut state = HomeState::default();
        state.services.add(Item::song("planned", "Lantern of the bay"));
        state.services.add(Item::song("other", "Other"));
        let mut hymns = crate::libraries::Library::new("Hymns".to_owned());
        hymns.songs = vec!["first".to_owned(), "planned".to_owned()];
        let mut both = crate::libraries::Library::new("Both".to_owned());
        both.songs = vec!["planned".to_owned(), "filed".to_owned()];
        state.libraries = vec![hymns, both];
        state.services.changed = false;
        rename_entries(&mut state, "planned", "filed");
        let keys: Vec<&str> = state
            .services
            .front()
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Song { key, .. } => Some(key.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(keys, ["filed", "other"]);
        assert_eq!(state.libraries[0].songs, ["first", "filed"], "in its place");
        assert_eq!(state.libraries[1].songs, ["filed"], "once");
        assert!(state.services.changed && state.libraries_changed);
    }
}
