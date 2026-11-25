//! Lists the window works out from the songs, kept from one frame to the next and worked out
//! again only when what they are made from changes.
//!
//! The window is drawn many times a second: on every mouse move, and about thirty times a
//! second while a video or a live web page is on the screens. With a library of hundreds of
//! songs, making the song list, the library search's results or what a playlist's items are on
//! every one of those frames cost more than drawing the window itself.

use std::sync::Arc;

use sp_engine::Show;

/// What every list made from the songs depends on: the songs themselves (by the show's
/// version), the name each is found by, and which are hidden or taken out of the library.
/// `epoch` goes up whenever any of them changes, so a list need only remember the epoch it was
/// made in.
#[derive(Default)]
pub struct SongsSeen {
    version: u64,
    keys: Vec<String>,
    hidden: [usize; 5],
    removed: Vec<usize>,
    epoch: u64,
}

impl SongsSeen {
    /// Looks at the songs again; called at the start of each frame. Cheap when nothing has
    /// changed: the keys and the songs taken out are compared, nothing is copied.
    pub fn check(&mut self, show: &Show, keys: &[String], hidden: [usize; 5], removed: &[usize]) {
        let same = self.epoch > 0
            && self.version == show.version()
            && self.hidden == hidden
            && self.removed == removed
            && self.keys == keys;
        if same {
            return;
        }
        self.version = show.version();
        self.keys = keys.to_vec();
        self.hidden = hidden;
        self.removed = removed.to_vec();
        self.epoch += 1;
    }

    /// The number of the songs as they are now.
    pub fn epoch(&self) -> u64 {
        self.epoch
    }
}

/// A value worked out from something (`K`), kept until that something changes.
///
/// In tests every value is worked out afresh each time and checked against the one kept, so a
/// list kept too long fails a test rather than showing the operator something out of date.
pub struct Kept<K, V> {
    from: Option<K>,
    value: Arc<V>,
    /// In tests: the last `get` would have used the value kept.
    hit: bool,
}

impl<K, V: Default> Default for Kept<K, V> {
    fn default() -> Kept<K, V> {
        Kept { from: None, value: Arc::new(V::default()), hit: false }
    }
}

impl<K, V: PartialEq + std::fmt::Debug> Kept<K, V> {
    /// The value kept, if `same` says it was worked out from what there is now. `None` means
    /// it is to be worked out and given to `keep`.
    pub fn get(&mut self, same: impl FnOnce(&K) -> bool) -> Option<Arc<V>> {
        let hit = self.from.as_ref().is_some_and(same);
        if cfg!(test) {
            self.hit = hit;
            return None;
        }
        hit.then(|| self.value.clone())
    }

    /// Keeps a value just worked out, and what it was worked out from.
    pub fn keep(&mut self, from: K, value: V) -> Arc<V> {
        if cfg!(test) && std::mem::take(&mut self.hit) {
            assert_eq!(*self.value, value, "a list kept from one frame to the next was out of date");
        }
        self.from = Some(from);
        self.value = Arc::new(value);
        self.value.clone()
    }
}

/// The song list in the left column: how many songs the library shown has, and the ones the
/// filter leaves, by their place among the songs (in order).
#[derive(Debug, Default, PartialEq)]
pub struct SongList {
    pub total: usize,
    pub shown: Vec<usize>,
}

/// The order a library's songs are listed in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SongOrder {
    /// As the library has them.
    #[default]
    Listed,
    /// By title, A to Z.
    AZ,
    /// The songs with the most slides first.
    Longest,
}

impl SongOrder {
    pub const ALL: [SongOrder; 3] = [SongOrder::Listed, SongOrder::AZ, SongOrder::Longest];

    /// The word the order is saved as.
    pub fn name(self) -> &'static str {
        match self {
            SongOrder::Listed => "listed",
            SongOrder::AZ => "az",
            SongOrder::Longest => "longest",
        }
    }

    /// The order saved as this word; as listed for any other.
    pub fn named(name: &str) -> SongOrder {
        SongOrder::ALL.into_iter().find(|order| order.name() == name).unwrap_or_default()
    }

    /// Puts songs (by their places in the show) in this order. Songs that tie keep the order
    /// they were given in.
    pub fn sort(self, show: &Show, songs: &mut [usize]) {
        let song = |at: &usize| show.presentations().get(*at);
        match self {
            SongOrder::Listed => {}
            SongOrder::AZ => songs.sort_by_cached_key(|at| song(at).map(|song| song.title.to_lowercase())),
            SongOrder::Longest => {
                songs.sort_by_cached_key(|at| std::cmp::Reverse(song(at).map_or(0, |song| song.slide_count())));
            }
        }
    }
}

/// What the song list was made from: the songs, the library shown (its songs by key, `None`
/// for All Songs), what was typed in the filter, and the order chosen.
pub type SongListFrom = (u64, Option<Vec<String>>, String, SongOrder);

/// The lists the window keeps.
#[derive(Default)]
pub struct Lists {
    pub songs: SongsSeen,
    /// The library search (Command-F): the songs found for what was typed.
    pub search: Kept<(u64, String), Found>,
    pub song_list: Kept<SongListFrom, SongList>,
    /// The playlist in front, in the left column: for each item, the song it names (by place)
    /// and the library that song is in.
    pub playlist_rows: Kept<PlaylistRowsFrom, PlaylistRows>,
    /// The playlist in front, in the slide area: for each item, the song it names.
    pub playlist_songs: Kept<(u64, Vec<crate::service::Item>), Vec<Option<usize>>>,
    /// The fonts the song in front uses that this computer doesn't have.
    pub missing_fonts: Kept<MissingFontsFrom, Vec<String>>,
}

/// The songs a library search finds, best first, each by its place with the line of its words
/// that matched (none when its title did).
/// Songs found: each by its place among the show's presentations, the slide found (its first
/// for a title), and the line of its words that has what was typed (none for a title).
pub type Found = Vec<(usize, usize, Option<String>)>;

/// For each item of a playlist: the song it names, by its place, and the library that song is
/// in.
pub type PlaylistRows = Vec<(Option<usize>, Option<String>)>;

/// What a playlist's rows were made from: the songs, its items and the libraries.
pub type PlaylistRowsFrom = (u64, Vec<crate::service::Item>, Vec<crate::libraries::Library>);

/// What the missing fonts were worked out from: the songs, the song in front, its theme, and
/// the fonts installed.
pub type MissingFontsFrom = (u64, usize, sp_model::Theme, Arc<Vec<String>>);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_songs_epoch_moves_only_when_what_lists_are_made_from_changes() {
        let mut show = Show::new(vec![Arc::new(sp_model::parse_song_text("One", "A\n"))]);
        let mut seen = SongsSeen::default();
        let keys = vec!["one".to_owned()];
        seen.check(&show, &keys, [usize::MAX; 5], &[]);
        let first = seen.epoch();
        seen.check(&show, &keys, [usize::MAX; 5], &[]);
        assert_eq!(seen.epoch(), first);
        show.add_presentation(Arc::new(sp_model::parse_song_text("Two", "B\n")));
        seen.check(&show, &keys, [usize::MAX; 5], &[]);
        let added = seen.epoch();
        assert_ne!(added, first);
        let keys = vec!["one".to_owned(), "two".to_owned()];
        seen.check(&show, &keys, [usize::MAX; 5], &[]);
        assert_ne!(seen.epoch(), added);
        let renamed = vec!["one".to_owned(), "three".to_owned()];
        let before = seen.epoch();
        seen.check(&show, &renamed, [usize::MAX; 5], &[]);
        assert_ne!(seen.epoch(), before, "a key changed in place is seen");
        let before = seen.epoch();
        seen.check(&show, &renamed, [usize::MAX; 5], &[1]);
        assert_ne!(seen.epoch(), before, "a song taken out is seen");
        let before = seen.epoch();
        seen.check(&show, &renamed, [0, usize::MAX, usize::MAX, usize::MAX, usize::MAX], &[1]);
        assert_ne!(seen.epoch(), before, "a slot hidden is seen");
    }

    #[test]
    fn songs_are_listed_as_the_library_has_them_by_title_or_longest_first() {
        let song = |title: &str, slides: usize| {
            let words = (0..slides).map(|n| format!("Line {n}\n")).collect::<Vec<_>>().join("\n");
            Arc::new(sp_model::parse_song_text(title, &words))
        };
        let show = Show::new(vec![song("morning", 2), song("Evening", 5), song("Noon", 2), song("afternoon", 3)]);
        let ordered = |order: SongOrder| {
            let mut songs = vec![0, 1, 2, 3];
            order.sort(&show, &mut songs);
            songs
        };
        assert_eq!(ordered(SongOrder::Listed), [0, 1, 2, 3]);
        // By title, whatever the case of its first letter.
        assert_eq!(ordered(SongOrder::AZ), [3, 1, 0, 2]);
        // The most slides first; songs with as many keep the order they had.
        assert_eq!(ordered(SongOrder::Longest), [1, 3, 0, 2]);
        // The order is saved as a word, and an unknown word is the library's own order.
        for order in SongOrder::ALL {
            assert_eq!(SongOrder::named(order.name()), order);
        }
        assert_eq!(SongOrder::named(""), SongOrder::Listed);
    }

    #[test]
    #[should_panic(expected = "out of date")]
    fn a_kept_value_that_no_longer_matches_fails_in_tests() {
        let mut kept: Kept<u32, Vec<u32>> = Kept::default();
        assert!(kept.get(|_| true).is_none());
        kept.keep(1, vec![1]);
        // Said to be made from the same thing, but worked out differently: a stale list.
        assert!(kept.get(|from| *from == 1).is_none(), "tests always work the value out afresh");
        kept.keep(1, vec![2]);
    }
}
