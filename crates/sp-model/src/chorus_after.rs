//! Singing the chorus after each verse, as SoftProjector does: one rule for a song's parts,
//! used by the SoftProjector import and by every song made or edited as text, and the
//! arrangement it makes for a song ("Chorus after each verse"), which follows the song's parts
//! while nobody has changed it by hand.
//!
//! The rule, as SoftProjector's own song preview puts a song's parts in order:
//! - A chorus is sung where it is written, and again after every verse that follows it.
//! - A later chorus (whatever its name or number: "Chorus 2", "Chorus 1.2") takes over from the
//!   one before for the verses after it, and takes the place of the one before sung after the
//!   verse just before it. With a part of another kind (an insert, an intro, a bridge…) between
//!   the two, the one before stays where it is sung and the new one comes after.
//! - A part of another kind is shown where it is written, with no chorus after it.
//! - A part named with `&` before it ("&Куплет") is more of the verse or chorus before.
//! - A pre-chorus, which SoftProjector has no name for, is sung just before the chorus that
//!   follows the verse it comes after.

use uuid::Uuid;

use crate::types::{Arrangement, Presentation};

/// The name the arrangement is made with where the app's language isn't known (the import
/// gives it the name in the app's language: see [`Presentation::name_chorus_after`]).
pub const CHORUS_AFTER_EACH_VERSE: &str = "Chorus after each verse";

/// What the SoftProjector import named the same arrangement before arrangements were marked
/// as made by it: Chorus After Each Verse takes such an arrangement for its own, not making a
/// second beside it.
pub const AS_SUNG: &str = "As sung";

/// What kind of part of a song a group is, for the order it is sung in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Verse,
    /// More of the verse before ("&Куплет").
    MoreVerse,
    Chorus,
    /// More of the chorus before ("&Припев").
    MoreChorus,
    /// Sung just before the chorus that follows the verse before it.
    PreChorus,
    /// An insert, an intro, an ending, a bridge or any other part: shown where it is written.
    Other,
}

/// The kind of part a group's name says, in any of the app's languages and the languages
/// SoftProjector names parts in ("Куплет 2", "Strophe", "Приспів 1.2", "Refrén", "&Verse"):
/// the names the group keys and colours know as a verse or a chorus (see
/// [`crate::group_name_in_english`]) are verses and choruses.
pub fn part_of(name: &str) -> Part {
    let name = name.trim();
    let (more, name) = match name.strip_prefix('&') {
        Some(rest) => (true, rest.trim_start()),
        None => (false, name),
    };
    let english = crate::group_name_in_english(name);
    if english.starts_with("verse") {
        if more { Part::MoreVerse } else { Part::Verse }
    } else if english.starts_with("chorus") {
        if more { Part::MoreChorus } else { Part::Chorus }
    } else if ["prechorus", "pre-chorus", "pre chorus"].iter().any(|word| english.starts_with(word)) {
        Part::PreChorus
    } else {
        Part::Other
    }
}

/// The order a song's parts are sung in, as places in `parts`, by the rule in the module's
/// notes.
pub fn sung_order(parts: &[Part]) -> Vec<usize> {
    let mut order: Vec<usize> = Vec::new();
    // The chorus sung now: its part and the parts that are more of it.
    let mut chorus: Vec<usize> = Vec::new();
    // The chorus at the end of the order so far: where it starts, and whether it is the chorus
    // sung again after a verse (true) or where it is written.
    let mut last: Option<(usize, bool)> = None;
    for (at, part) in parts.iter().enumerate() {
        match part {
            Part::Verse => {
                order.push(at);
                last = None;
                if !chorus.is_empty() {
                    last = Some((order.len(), true));
                    order.extend(&chorus);
                }
            }
            // More of the verse: sung before the chorus at the end, which comes again after it.
            Part::MoreVerse => match last {
                Some((start, again)) => {
                    order.truncate(start);
                    order.push(at);
                    last = Some((order.len(), again));
                    order.extend(&chorus);
                }
                None => order.push(at),
            },
            Part::MoreChorus if !chorus.is_empty() => {
                chorus.push(at);
                match last {
                    Some((start, again)) => {
                        order.truncate(start);
                        order.extend(&chorus);
                        last = Some((start, again));
                    }
                    None => order.push(at),
                }
            }
            Part::Chorus | Part::MoreChorus => {
                // The chorus before, sung after the verse just before this one, gives way to it.
                if let Some((start, true)) = last {
                    order.truncate(start);
                }
                order.push(at);
                last = Some((order.len() - 1, false));
                chorus = vec![at];
            }
            Part::PreChorus => match last {
                Some((start, true)) => {
                    order.insert(start, at);
                    last = Some((start + 1, true));
                }
                _ => {
                    order.push(at);
                    last = None;
                }
            },
            Part::Other => {
                order.push(at);
                last = None;
            }
        }
    }
    order
}

/// Whether an arrangement is named as the one that sings the chorus after each verse is: `name`
/// (in the app's language), its English name, or "As sung".
fn named_as_it(arrangement: &Arrangement, name: &str) -> bool {
    [name, CHORUS_AFTER_EACH_VERSE, AS_SUNG].contains(&arrangement.name.as_str())
}

impl Presentation {
    /// The song's groups, by their ids, in the order they are sung with the chorus after each
    /// verse; `None` for a song without both a verse and a chorus.
    pub fn chorus_after_each_verse(&self) -> Option<Vec<Uuid>> {
        let parts: Vec<Part> = self.groups.iter().map(|group| part_of(&group.name)).collect();
        let verse = parts.iter().any(|part| matches!(part, Part::Verse | Part::MoreVerse));
        let chorus = parts.iter().any(|part| matches!(part, Part::Chorus | Part::MoreChorus));
        (verse && chorus).then(|| sung_order(&parts).into_iter().map(|at| self.groups[at].id).collect())
    }

    /// The song's arrangement that follows its parts, if it has one (see [`Arrangement::auto`]).
    pub fn automatic_arrangement(&self) -> Option<&Arrangement> {
        self.arrangements.iter().find(|arrangement| arrangement.auto)
    }

    /// After the song's groups changed: its automatic arrangement follows them, or goes when
    /// the song has no verse or no chorus any more. Arrangements made or changed by hand are
    /// never touched.
    pub fn follow_parts(&mut self) {
        let Some(at) = self.arrangements.iter().position(|arrangement| arrangement.auto) else { return };
        match self.chorus_after_each_verse() {
            Some(order) => self.arrangements[at].groups = order,
            None => {
                let gone = self.arrangements.remove(at);
                if self.arrangement == Some(gone.id) {
                    self.arrangement = None;
                }
            }
        }
    }

    /// As [`follow_parts`], after the song's groups changed from those of `before` (the song as
    /// it was): an "As sung" the SoftProjector import made before arrangements were marked, as
    /// it made it (still in the order the rule gives `before`'s groups), is taken for the
    /// automatic arrangement first, so it follows the change too. One changed by hand since
    /// gives another order and is left alone.
    ///
    /// [`follow_parts`]: Self::follow_parts
    pub fn follow_parts_from(&mut self, before: &Presentation) {
        if !self.arrangements.iter().any(|arrangement| arrangement.auto)
            && let Some(order) = before.chorus_after_each_verse()
            && let Some(old) = before
                .arrangements
                .iter()
                .find(|arrangement| arrangement.name == AS_SUNG && arrangement.groups == order)
            && let Some(taken) = self.arrangements.iter_mut().find(|arrangement| arrangement.id == old.id)
        {
            taken.auto = true;
        }
        self.follow_parts();
    }

    /// For a song made, or whose words were changed, as text: as [`follow_parts`], and a song
    /// with no automatic arrangement gets one, named `name`, when singing the chorus after each
    /// verse changes the order the parts are written in. The song plays it unless another
    /// arrangement was chosen. None is made beside one of that name (the operator's, once they
    /// changed it) or an "As sung" the SoftProjector import made before: Chorus After Each
    /// Verse takes that one.
    ///
    /// [`follow_parts`]: Self::follow_parts
    pub fn follow_parts_as_text(&mut self, name: &str) {
        self.follow_parts();
        if self.arrangements.iter().any(|arrangement| arrangement.auto || named_as_it(arrangement, name)) {
            return;
        }
        let Some(order) = self.chorus_after_each_verse() else { return };
        if order.iter().eq(self.groups.iter().map(|group| &group.id)) {
            return;
        }
        let made = Arrangement { id: Uuid::new_v4(), name: name.to_owned(), groups: order, auto: true };
        if self.active_arrangement().is_none() {
            self.arrangement = Some(made.id);
        }
        self.arrangements.push(made);
    }

    /// Chorus After Each Verse: the song's automatic arrangement made, named `name`, or brought
    /// up to date, and chosen. One of that name changed by hand since, or an "As sung" the
    /// SoftProjector import made before, is taken for it (keeping its name) rather than a second
    /// made. `false`, with nothing changed, for a song without both a verse and a chorus.
    pub fn sing_chorus_after_each_verse(&mut self, name: &str) -> bool {
        let Some(order) = self.chorus_after_each_verse() else { return false };
        let at = self
            .arrangements
            .iter()
            .position(|arrangement| arrangement.auto)
            .or_else(|| self.arrangements.iter().position(|arrangement| named_as_it(arrangement, name)));
        let id = match at {
            Some(at) => {
                let arrangement = &mut self.arrangements[at];
                arrangement.groups = order;
                arrangement.auto = true;
                arrangement.id
            }
            None => {
                let made = Arrangement { id: Uuid::new_v4(), name: name.to_owned(), groups: order, auto: true };
                let id = made.id;
                self.arrangements.push(made);
                id
            }
        };
        self.arrangement = Some(id);
        true
    }

    /// Gives the automatic arrangement a song was just made with, named in English, the name
    /// in the app's language.
    pub fn name_chorus_after(&mut self, name: &str) {
        for arrangement in &mut self.arrangements {
            if arrangement.auto && arrangement.name == CHORUS_AFTER_EACH_VERSE {
                arrangement.name = name.to_owned();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_song_text;

    /// The order sung, by the groups' names, of groups with these names.
    fn sung(names: &[&str]) -> Vec<String> {
        let parts: Vec<Part> = names.iter().map(|name| part_of(name)).collect();
        sung_order(&parts).into_iter().map(|at| names[at].to_owned()).collect()
    }

    #[test]
    fn the_chorus_written_once_is_sung_after_each_verse() {
        // At the top: where it is written, then after each verse.
        assert_eq!(sung(&["Chorus", "Verse 1", "Verse 2"]), ["Chorus", "Verse 1", "Chorus", "Verse 2", "Chorus"]);
        // After the first verse: once there, then after each verse after it.
        assert_eq!(
            sung(&["Verse 1", "Chorus", "Verse 2", "Verse 3"]),
            ["Verse 1", "Chorus", "Verse 2", "Chorus", "Verse 3", "Chorus"]
        );
        // A verse before any chorus has none after it.
        assert_eq!(
            sung(&["Verse 1", "Verse 2", "Chorus", "Verse 3"]),
            ["Verse 1", "Verse 2", "Chorus", "Verse 3", "Chorus"]
        );
    }

    #[test]
    fn a_later_chorus_takes_over_from_where_it_is_written() {
        assert_eq!(
            sung(&["Chorus", "Verse 1", "Verse 2", "Chorus 1.2", "Verse 3", "Verse 4"]),
            ["Chorus", "Verse 1", "Chorus", "Verse 2", "Chorus 1.2", "Verse 3", "Chorus 1.2", "Verse 4", "Chorus 1.2"]
        );
        // A chorus written out after every verse is sung as written: the copy after each verse
        // gives way to the one written there.
        assert_eq!(sung(&["Verse 1", "Chorus", "Verse 2", "Chorus"]), ["Verse 1", "Chorus", "Verse 2", "Chorus"]);
        // Two choruses written one after the other are both sung there; the second goes on.
        assert_eq!(sung(&["Chorus", "Chorus 2", "Verse 1"]), ["Chorus", "Chorus 2", "Verse 1", "Chorus 2"]);
    }

    #[test]
    fn with_an_insert_between_two_choruses_the_first_stays() {
        assert_eq!(
            sung(&["Verse 1", "Chorus", "Verse 2", "Вставка", "Chorus 2", "Verse 3"]),
            ["Verse 1", "Chorus", "Verse 2", "Chorus", "Вставка", "Chorus 2", "Verse 3", "Chorus 2"]
        );
        // Without it, the second takes the first's place after verse 2.
        assert_eq!(
            sung(&["Verse 1", "Chorus", "Verse 2", "Chorus 2", "Verse 3"]),
            ["Verse 1", "Chorus", "Verse 2", "Chorus 2", "Verse 3", "Chorus 2"]
        );
        // Inserts, intros and endings are shown where they are, with no chorus after them.
        assert_eq!(
            sung(&["Intro", "Verse 1", "Chorus", "Verse 2", "Slide", "Ending"]),
            ["Intro", "Verse 1", "Chorus", "Verse 2", "Chorus", "Slide", "Ending"]
        );
        // A bridge too: the chorus written after it is sung after it, and after verse 2 before it.
        assert_eq!(
            sung(&["Verse 1", "Chorus", "Verse 2", "Bridge", "Chorus"]),
            ["Verse 1", "Chorus", "Verse 2", "Chorus", "Bridge", "Chorus"]
        );
    }

    #[test]
    fn a_part_named_with_an_ampersand_is_more_of_the_one_before() {
        // More of a verse comes before the chorus after it.
        assert_eq!(
            sung(&["Verse 1", "Chorus", "Verse 2", "&Verse 2", "Verse 3"]),
            ["Verse 1", "Chorus", "Verse 2", "&Verse 2", "Chorus", "Verse 3", "Chorus"]
        );
        // More of the chorus is sung with it from then on.
        assert_eq!(
            sung(&["Chorus", "&Chorus", "Verse 1", "Verse 2"]),
            ["Chorus", "&Chorus", "Verse 1", "Chorus", "&Chorus", "Verse 2", "Chorus", "&Chorus"]
        );
        assert_eq!(part_of("&Куплет 2"), Part::MoreVerse);
        assert_eq!(part_of("& Приспів"), Part::MoreChorus);
    }

    #[test]
    fn a_pre_chorus_written_out_leads_into_the_chorus() {
        assert_eq!(
            sung(&["Verse 1", "Pre-Chorus", "Chorus", "Verse 2", "Pre-Chorus", "Chorus", "Bridge", "Chorus"]),
            ["Verse 1", "Pre-Chorus", "Chorus", "Verse 2", "Pre-Chorus", "Chorus", "Bridge", "Chorus"]
        );
        // Written once, it is sung once: the chorus follows each verse.
        assert_eq!(
            sung(&["Verse 1", "Pre-Chorus", "Chorus", "Verse 2"]),
            ["Verse 1", "Pre-Chorus", "Chorus", "Verse 2", "Chorus"]
        );
    }

    #[test]
    fn parts_are_known_by_their_names_in_russian_ukrainian_spanish_and_softprojectors_languages() {
        for verse in
            ["Verse", "Verse 2", "Куплет 1", "куплет", "Strophe 3", "Verš 1", "Estrofa 2", "VERSE 1:", "Vers 2"]
        {
            assert_eq!(part_of(verse), Part::Verse, "{verse}");
        }
        for chorus in ["Chorus", "Chorus 1.2", "Припев", "ПРИПЕВ 2", "Приспів:", "Refrain", "Sbor", "Refrén", "Coro"]
        {
            assert_eq!(part_of(chorus), Part::Chorus, "{chorus}");
        }
        for pre in ["Pre-Chorus", "Prechorus", "Предприпев", "Передприспів", "Precoro", "Vorrefrain"]
        {
            assert_eq!(part_of(pre), Part::PreChorus, "{pre}");
        }
        for other in ["Вставка", "Слайд", "Intro", "Вступление", "Окончание", "Закінчення", "Bridge", "Мост", "Tag", ""]
        {
            assert_eq!(part_of(other), Part::Other, "{other}");
        }
        assert_eq!(
            sung(&["Куплет 1", "Припев", "Куплет 2", "Куплет 3", "Окончание"]),
            ["Куплет 1", "Припев", "Куплет 2", "Припев", "Куплет 3", "Припев", "Окончание"]
        );
        assert_eq!(
            sung(&["Приспів", "Куплет 1", "Куплет 2", "Приспів 1.2", "Куплет 3"]),
            ["Приспів", "Куплет 1", "Приспів", "Куплет 2", "Приспів 1.2", "Куплет 3", "Приспів 1.2"]
        );
    }

    #[test]
    fn a_song_without_a_chorus_or_without_a_verse_has_no_such_order() {
        assert_eq!(sung(&["Verse 1", "Verse 2"]), ["Verse 1", "Verse 2"]);
        assert!(parse_song_text("T", "[Verse 1]\nA\n\n[Verse 2]\nB\n").chorus_after_each_verse().is_none());
        assert!(parse_song_text("T", "[Chorus]\nA\n\n[Bridge]\nB\n").chorus_after_each_verse().is_none());
        assert!(parse_song_text("T", "").chorus_after_each_verse().is_none());
    }

    fn names(song: &Presentation, arrangement: &Arrangement) -> Vec<String> {
        arrangement
            .groups
            .iter()
            .map(|id| song.groups.iter().find(|group| group.id == *id).map_or("?", |g| g.name.as_str()).to_owned())
            .collect()
    }

    const SONG: &str = "[Verse 1]\nOne\n\n[Chorus]\nSing\n\n[Verse 2]\nTwo\n";

    #[test]
    fn a_song_made_as_text_gets_the_arrangement_chosen_and_it_follows_its_parts() {
        let mut song = parse_song_text("T", SONG);
        song.follow_parts_as_text("Chorus after each verse");
        let made = song.automatic_arrangement().expect("made").clone();
        assert_eq!(made.name, "Chorus after each verse");
        assert_eq!(names(&song, &made), ["Verse 1", "Chorus", "Verse 2", "Chorus"]);
        assert_eq!(song.arrangement, Some(made.id), "chosen");
        // Its words changed as text: the same arrangement follows the new parts.
        let mut edited = song.reflowed(&format!("{SONG}\n[Verse 3]\nThree\n"));
        edited.follow_parts_as_text("Another name");
        let now = edited.automatic_arrangement().expect("still there");
        assert_eq!((now.id, now.name.as_str()), (made.id, "Chorus after each verse"));
        assert_eq!(names(&edited, now), ["Verse 1", "Chorus", "Verse 2", "Chorus", "Verse 3", "Chorus"]);
        assert_eq!(edited.arrangements.len(), 1);
        // Without the chorus, it goes, and the song is sung as written.
        let gone = edited.reflowed("[Verse 1]\nOne\n\n[Verse 2]\nTwo\n");
        assert!(gone.arrangements.is_empty());
        assert_eq!(gone.arrangement, None);
    }

    #[test]
    fn a_song_sung_as_written_gets_none_and_another_chosen_stays_chosen() {
        let mut song = parse_song_text("T", "[Verse 1]\nOne\n\n[Chorus]\nSing\n");
        song.follow_parts_as_text("x");
        assert!(song.arrangements.is_empty(), "the chorus after the only verse: as written");
        let mut song = parse_song_text("T", SONG);
        let mine = song.new_arrangement(|n| format!("Mine {n}"));
        song.arrangement = Some(mine.id);
        song.arrangements.push(mine.clone());
        song.follow_parts_as_text("x");
        assert_eq!(song.arrangements.len(), 2);
        assert_eq!(song.arrangement, Some(mine.id), "the operator's choice stays");
        assert_eq!(song.arrangements[0], mine, "and theirs is untouched");
    }

    #[test]
    fn an_arrangement_changed_by_hand_is_left_alone() {
        let mut song = parse_song_text("T", SONG);
        song.follow_parts_as_text("x");
        let by_hand = {
            let arrangement = &mut song.arrangements[0];
            arrangement.groups.truncate(2);
            arrangement.auto = false;
            arrangement.clone()
        };
        let mut edited = song.reflowed(&format!("{SONG}\n[Verse 3]\nThree\n"));
        assert_eq!(edited.arrangements, std::slice::from_ref(&by_hand));
        // Nor is another made beside it.
        edited.follow_parts_as_text("x");
        assert_eq!(edited.arrangements, std::slice::from_ref(&by_hand));
        let gone = edited.reflowed("[Verse 1]\nOne\n");
        assert_eq!(gone.arrangements, [by_hand], "kept even without a chorus");
    }

    #[test]
    fn group_changes_in_the_editor_carry_the_arrangement_along() {
        let mut song = parse_song_text("T", "[Verse 1]\nOne\n\n[Chorus]\nSing\n\n[Verse 2]\nTwo\n\nTwo more\n");
        song.follow_parts_as_text("x");
        // A new group started at the last slide.
        let split = song.with_group_starting(3, "Verse 3").unwrap();
        let auto = split.automatic_arrangement().unwrap();
        assert_eq!(names(&split, auto), ["Verse 1", "Chorus", "Verse 2", "Chorus", "Verse 3", "Chorus"]);
        // A group renamed into the chorus.
        let renamed = split.with_group_starting(2, "Chorus 2").unwrap();
        let auto = renamed.automatic_arrangement().unwrap();
        assert_eq!(names(&renamed, auto), ["Verse 1", "Chorus", "Chorus 2", "Verse 3", "Chorus 2"]);
        // The chorus's only slide deleted: no chorus, no arrangement.
        let fewer = song.without_slide(1).unwrap();
        assert!(fewer.arrangements.is_empty());
        assert_eq!(fewer.arrangement, None);
        // A group emptied by moving its slide away.
        let (moved, _) = song.with_slides_moved(&[2, 3], 0, false).unwrap();
        assert_eq!(moved.groups.len(), 2);
        let auto = moved.automatic_arrangement().unwrap();
        assert_eq!(names(&moved, auto), ["Verse 1", "Chorus"]);
    }

    #[test]
    fn an_as_sung_the_import_made_before_follows_the_words_as_they_change() {
        // As an older build imported it: "As sung", not marked, in the order the rule gives.
        let mut old = parse_song_text("T", SONG);
        let order = old.chorus_after_each_verse().unwrap();
        let as_sung = Arrangement { id: Uuid::new_v4(), name: AS_SUNG.to_owned(), groups: order, auto: false };
        old.arrangement = Some(as_sung.id);
        old.arrangements.push(as_sung.clone());
        let mut edited = old.reflowed(&format!("{SONG}\n[Verse 3]\nThree\n"));
        edited.follow_parts_as_text("x");
        let [taken] = edited.arrangements.as_slice() else { panic!("{:?}", edited.arrangements) };
        assert_eq!((taken.id, taken.name.as_str(), taken.auto), (as_sung.id, AS_SUNG, true));
        assert_eq!(names(&edited, taken), ["Verse 1", "Chorus", "Verse 2", "Chorus", "Verse 3", "Chorus"]);
        assert_eq!(edited.arrangement, Some(as_sung.id));
        // A group started in the editor too.
        let split = old.with_group_starting(2, "Verse 3").unwrap();
        assert!(split.arrangements[0].auto);
        // One changed by hand since is left as it is.
        let mut changed = old.clone();
        changed.arrangements[0].groups.truncate(2);
        let edited = changed.reflowed(&format!("{SONG}\n[Verse 3]\nThree\n"));
        assert_eq!(edited.arrangements, changed.arrangements);
    }

    #[test]
    fn chorus_after_each_verse_makes_or_refreshes_and_chooses_it_taking_the_old_as_sung() {
        let mut song = parse_song_text("T", SONG);
        assert!(song.sing_chorus_after_each_verse("Припев после каждого куплета"));
        let made = song.automatic_arrangement().unwrap().clone();
        assert_eq!(made.name, "Припев после каждого куплета");
        assert_eq!(song.arrangement, Some(made.id));
        // Again: the same one, chosen again after As written was.
        song.arrangement = None;
        assert!(song.sing_chorus_after_each_verse("x"));
        assert_eq!(song.arrangements.len(), 1);
        assert_eq!(song.arrangement, Some(made.id));
        // A song the SoftProjector import gave an "As sung" before: that one is brought up to
        // date and kept, under its name.
        let mut old = parse_song_text("T", SONG);
        let as_sung = Arrangement { id: Uuid::new_v4(), name: AS_SUNG.to_owned(), groups: vec![], auto: false };
        old.arrangements.push(as_sung.clone());
        old.follow_parts_as_text("x");
        assert_eq!(old.arrangements.len(), 1, "editing as text makes none beside it");
        assert!(old.sing_chorus_after_each_verse("x"));
        assert_eq!(old.arrangements.len(), 1);
        let taken = &old.arrangements[0];
        assert_eq!((taken.id, taken.name.as_str(), taken.auto), (as_sung.id, AS_SUNG, true));
        assert_eq!(names(&old, taken), ["Verse 1", "Chorus", "Verse 2", "Chorus"]);
        // Without a chorus: nothing to do.
        let mut verses = parse_song_text("T", "[Verse 1]\nOne\n\n[Verse 2]\nTwo\n");
        assert!(!verses.sing_chorus_after_each_verse("x"));
        assert!(verses.arrangements.is_empty());
    }

    #[test]
    fn the_mark_is_saved_only_when_set_and_older_files_read_as_before() {
        let mut song = parse_song_text("T", SONG);
        song.follow_parts_as_text("x");
        let json = crate::to_json(&song);
        assert!(json.contains("\"auto\": true"), "{json}");
        assert!(json.contains("\"version\": 1"), "no new version of the format: {json}");
        let read = crate::from_json(&json).unwrap();
        assert!(read.arrangements[0].auto);
        song.arrangements[0].auto = false;
        let json = crate::to_json(&song);
        assert!(!json.contains("\"auto\""), "{json}");
        assert!(!crate::from_json(&json).unwrap().arrangements[0].auto);
    }
}
