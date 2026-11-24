//! Songs from SoftProjector, the open-source presentation program many Russian- and
//! Ukrainian-speaking churches use. Its songs are kept in an SQLite database (its own
//! `spData.sqlite`, or a songbook exported as `.sps`); the app reads the rows, and this turns
//! one song's text into SoftPresenter's groups and slides.
//!
//! A song's text is blocks, each starting with a line naming it: a verse ("Куплет 1",
//! "Verse 2"), the chorus ("Припев", "Приспів", "Chorus"), or a slide of another kind ("Слайд",
//! "Вставка" (an insert), "Вступление" (an intro), "Окончание" (an ending)…). A block runs to
//! the next such line and is shown as one slide. A name starting with `&` ("&Куплет") is more
//! of the verse or chorus before it. SoftProjector sings the chorus after every verse; the song
//! comes with an arrangement in that order.

use crate::{Arrangement, Copyright, Group, Object, Presentation, Role, Slide, Uuid, group_color};

/// What kind of block a title line starts.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Verse,
    Chorus,
    Other,
}

const VERSE: [&str; 4] = ["Verse", "Куплет", "Strophe", "Verš"];
const CHORUS: [&str; 6] = ["Chorus", "Sbor", "Refrain", "Припев", "Приспів", "Refrén"];
const OTHER: [&str; 22] = [
    "Slide",
    "Слайд",
    "Dia",
    "Snímek",
    "Insert",
    "Вставка",
    "Einfügung",
    "Vložka",
    "Intro",
    "Вступление",
    "Вступ",
    "Einleitung",
    "Úvod",
    "Ending",
    "Окончание",
    "Закінчення",
    "Ende",
    "Konec",
    "Bridge",
    "Мост",
    "Бридж",
    "Міст",
];

/// One block of a song's text: what its title line says (its kind, and whether it is more of
/// the block before), its name, and its lines.
type Block<'a> = (Option<(Kind, bool)>, String, Vec<&'a str>);

/// The kind of block a line starts, and whether it is more of the one before (`&`).
fn title_line(line: &str) -> Option<(Kind, bool)> {
    let line = line.trim();
    let (more, name) = match line.strip_prefix('&') {
        Some(rest) => (true, rest.trim_start()),
        None => (false, line),
    };
    // A title line is short: the name and perhaps a number.
    if name.chars().count() > 24 {
        return None;
    }
    let starts = |names: &[&str]| names.iter().any(|n| name.starts_with(n));
    let kind = if starts(&VERSE) {
        Kind::Verse
    } else if starts(&CHORUS) {
        Kind::Chorus
    } else if starts(&OTHER) {
        Kind::Other
    } else {
        return None;
    };
    Some((kind, more))
}

/// One SoftProjector song as a presentation. `number` is its number in its songbook, put
/// before the title so a song can still be found by it.
pub fn from_softprojector(number: Option<i64>, title: &str, words: &str, music: &str, text: &str) -> Presentation {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = text.lines().collect();
    // The blocks: each title line's kind, its name, and its words.
    let mut blocks: Vec<Block> = Vec::new();
    for line in &lines {
        match title_line(line) {
            Some(kind) => blocks.push((Some(kind), line.trim().trim_start_matches('&').trim().to_owned(), Vec::new())),
            None => match blocks.last_mut() {
                Some(block) => block.2.push(line),
                // Words before any title line are a slide of their own.
                None => blocks.push((None, String::new(), vec![line])),
            },
        }
    }
    let mut groups: Vec<Group> = Vec::new();
    let mut order: Vec<Uuid> = Vec::new();
    let mut chorus: Option<usize> = None;
    let mut last_verse: Option<usize> = None;
    let mut choruses = 0;
    for (kind, name, words) in blocks {
        let words = words.join("\n").trim().to_owned();
        if words.is_empty() {
            continue;
        }
        let slide = Slide::new(vec![Object::text(Role::primary(), words)]);
        match kind {
            // More of the verse or chorus before: another slide in its group.
            Some((Kind::Verse, true)) if last_verse.is_some() => {
                groups[last_verse.unwrap_or(0)].slides.push(slide);
            }
            Some((Kind::Chorus, true)) if chorus.is_some() => {
                groups[chorus.unwrap_or(0)].slides.push(slide);
            }
            Some((Kind::Chorus, _)) => {
                choruses += 1;
                let name = if name.is_empty() || (choruses > 1 && groups.iter().any(|g| g.name == name)) {
                    format!("{name} {choruses}").trim().to_owned()
                } else {
                    name
                };
                groups.push(new_group(&name, slide));
                let at = groups.len() - 1;
                // A chorus is sung where it is written, and then after each verse; a new chorus
                // takes the place of the one before.
                if order.last() != Some(&groups[at].id) {
                    order.push(groups[at].id);
                }
                chorus = Some(at);
            }
            Some((Kind::Verse, _)) => {
                groups.push(new_group(&name, slide));
                let at = groups.len() - 1;
                order.push(groups[at].id);
                if let Some(chorus) = chorus {
                    order.push(groups[chorus].id);
                }
                last_verse = Some(at);
            }
            _ => {
                groups.push(new_group(&name, slide));
                order.push(groups[groups.len() - 1].id);
            }
        }
    }
    // A chorus written after the first verse comes after it once, not twice.
    order.dedup();
    let plain_order = groups.iter().map(|g| g.id).collect::<Vec<_>>();
    let (arrangements, arrangement) = if order != plain_order && !order.is_empty() {
        let arrangement = Arrangement { id: Uuid::new_v4(), name: "As sung".to_owned(), groups: order };
        let id = arrangement.id;
        (vec![arrangement], Some(id))
    } else {
        (Vec::new(), None)
    };
    let title = title.trim();
    let title = match number.filter(|n| *n > 0) {
        Some(n) if !title.is_empty() => format!("{n}. {title}"),
        Some(n) => n.to_string(),
        None if title.is_empty() => "Untitled".to_owned(),
        None => title.to_owned(),
    };
    let (words, music) = (words.trim(), music.trim());
    let author = match (words.is_empty(), music.is_empty()) {
        (false, true) => words.to_owned(),
        (true, false) => music.to_owned(),
        (false, false) if words == music => words.to_owned(),
        (false, false) => format!("{words} / {music}"),
        (true, true) => String::new(),
    };
    Presentation {
        id: Uuid::new_v4(),
        title,
        groups,
        copyright: Copyright { author, ..Copyright::default() },
        theme: None,
        arrangements,
        arrangement,
        loops: false,
        transition: None,
        catalog: None,
    }
}

fn new_group(name: &str, slide: Slide) -> Group {
    Group { id: Uuid::new_v4(), name: name.to_owned(), color: group_color(name), slides: vec![slide] }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(song: &Presentation) -> Vec<(String, String)> {
        song.groups
            .iter()
            .flat_map(|g| {
                g.slides.iter().map(move |s| (g.name.clone(), s.text_for(&Role::primary()).unwrap_or("").to_owned()))
            })
            .collect()
    }

    /// Words written for this test, in the shape SoftProjector keeps them.
    const SONG: &str = "Куплет 1\nСвет над рекой\nПесня над тобой\n\nПрипев\nДомой, домой\n\nКуплет 2\nВечер настал\n\n&Куплет 2\nИ день прошёл\n\nОкончание\nАминь\n";

    #[test]
    fn blocks_become_groups_and_the_chorus_follows_every_verse() {
        let song = from_softprojector(Some(123), "Река", "Анна", "Борис", SONG);
        assert_eq!(song.title, "123. Река");
        assert_eq!(song.copyright.author, "Анна / Борис");
        let got = words(&song);
        assert_eq!(got[0], ("Куплет 1".to_owned(), "Свет над рекой\nПесня над тобой".to_owned()));
        assert_eq!(got[1], ("Припев".to_owned(), "Домой, домой".to_owned()));
        assert_eq!(song.groups[2].name, "Куплет 2");
        assert_eq!(song.groups[2].slides.len(), 2, "the & block is more of verse 2");
        let names: Vec<&str> = song.arrangements[0]
            .groups
            .iter()
            .map(|id| song.groups.iter().find(|g| g.id == *id).unwrap().name.as_str())
            .collect();
        assert_eq!(names, ["Куплет 1", "Припев", "Куплет 2", "Припев", "Окончание"]);
        assert_eq!(song.arrangement, Some(song.arrangements[0].id));
    }

    #[test]
    fn a_song_without_titles_or_chorus_is_one_slide_and_no_arrangement() {
        let song = from_softprojector(None, "Words", "", "", "One line\nTwo line\n");
        assert_eq!(words(&song).len(), 1);
        assert!(song.arrangements.is_empty());
        assert_eq!(song.title, "Words");
    }
}
