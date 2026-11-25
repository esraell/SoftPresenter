//! `sp library tidy [--apply] [--keep-numbers]`: the tidying every import does
//! (`sp_model::tidy_song`: titles and words put right, slides parted at blank and "* * *" lines
//! and when too long), for the songs already in the library.
//!
//! Without `--apply` nothing changes: the reply lists every change it would make (each title
//! before and after, each slide it would part, the letters and spaces it would put right, song
//! by song), the slides it would leave whole and why, and the songs with the same words (listed
//! only). With `--apply` the songs are put in place and saved as any change is, keeping their
//! ids (so playlists, libraries and arrangements keep finding them) and themes. Before a song's
//! file is written, a copy of it as it was goes into a folder beside the songs, "Backup before
//! tidying" and the date, never over a copy already there. A song whose file changed outside
//! the app is left as it is, and said. Run again, it finds nothing left to change.
//!
//! Songs kept in step with an outside source (the song catalog, TimelyChurch) are left alone:
//! a change of ours would read as the church's own, and stop their updates. So are Bible
//! passages and the VoxLive item, which aren't songs.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use sp_model::{Presentation, SplitWhy, Tidied, TidyOptions, TitleFix, WholeWhy};

use crate::control::Reply;

/// The start of the name of the folder the files are kept in before a tidy, in the data folder.
pub const BACKUP_FOLDER: &str = "Backup before tidying";

const USAGE: &str = "library tidy [--apply] [--keep-numbers]";

/// A song the tidy looked at, and what it would do to it.
struct Looked {
    at: usize,
    before: Arc<Presentation>,
    tidied: Tidied,
    libraries: Vec<String>,
    /// With `--apply`: why it was left as it was, if it was.
    not_done: Option<String>,
}

impl Looked {
    fn changes(&self) -> bool {
        self.tidied.song != *self.before
    }
}

impl crate::App {
    pub(crate) fn ctl_library_tidy(&mut self, words: &[String]) -> Result<Reply, String> {
        self.only_at_the_computer(crate::remote::policy::AtComputer::TidyLibrary)?;
        let mut apply = false;
        let mut options = TidyOptions::default();
        for word in words {
            match word.as_str() {
                "--apply" => apply = true,
                "--keep-numbers" => options.keep_numbers = true,
                _ => return Err(USAGE.to_owned()),
            }
        }
        let listed = self.listed_songs();
        let mut kept_in_step = 0;
        let mut looked: Vec<Looked> = Vec::new();
        for (at, _) in &listed {
            let Some(song) = self.show.presentations().get(*at).cloned() else { continue };
            if song.catalog.is_some() {
                kept_in_step += 1;
                continue;
            }
            let tidied = sp_model::tidy_song(&song, options);
            looked.push(Looked { at: *at, before: song, tidied, libraries: self.libraries_of(*at), not_done: None });
        }
        let duplicates = duplicates(&looked);
        let mut backup = None;
        if apply {
            backup = self.apply_tidy(&mut looked)?;
        }
        let shown = backup.as_deref().map(|folder| folder.display().to_string());
        let text = report_text(&looked, listed.len(), kept_in_step, &duplicates, apply, shown.as_deref());
        let data = report_data(&looked, listed.len(), kept_in_step, &duplicates, apply, shown.as_deref());
        Ok(Reply::with(text, data))
    }

    /// The names of the libraries a song is in.
    fn libraries_of(&self, at: usize) -> Vec<String> {
        let Some(state) = self.running.as_ref().map(|r| &r.home_state) else { return Vec::new() };
        let Some(key) = state.song_keys.get(at) else { return Vec::new() };
        state.libraries.iter().filter(|library| library.has(key)).map(|library| library.name.clone()).collect()
    }

    /// Puts the tidied songs in place and saves them, each song's file copied first. Returns
    /// the folder the copies are in, if any were made.
    fn apply_tidy(&mut self, looked: &mut [Looked]) -> Result<Option<PathBuf>, String> {
        let folder = crate::bible_store::data_dir().ok_or("Could not find the library folder.")?;
        let backup = backup_folder(&folder, &chrono::Local::now().format("%Y-%m-%d").to_string());
        let mut made_backup = false;
        let mut changed = Vec::new();
        for one in looked.iter_mut().filter(|one| one.changes()) {
            let at = one.at;
            if self.held_songs.contains(&at) {
                one.not_done = Some("it waits on Reload or Keep mine".to_owned());
                continue;
            }
            let file = self.sources.get(at).cloned().flatten().map(|path| crate::load::song_file_for(&path));
            if let Some(file) = file.filter(|file| file.is_file()) {
                if self.stamps.check(&file) != crate::outside::Now::Same {
                    one.not_done = Some("its file changed outside SoftPresenter".to_owned());
                    continue;
                }
                if let Err(why) = keep_copy(&file, &backup) {
                    one.not_done = Some(format!("a copy of its file couldn't be kept ({why})"));
                    continue;
                }
                made_backup = true;
            }
            let old_ids: Vec<sp_model::Uuid> = one.before.slides().map(|(_, slide)| slide.id).collect();
            self.history.forget(at);
            self.show.replace_presentation_following(at, Arc::new(one.tidied.song.clone()));
            if !self.unsaved.contains(&at) {
                self.unsaved.push(at);
            }
            if let Some(running) = self.running.as_mut() {
                for id in old_ids {
                    running.thumbs.forget(id, &mut running.egui_renderer);
                }
            }
            changed.push(at);
        }
        self.save_unsaved();
        // What was saved is read back: a song whose file doesn't hold it now wasn't saved.
        for one in looked.iter_mut().filter(|one| changed.contains(&one.at)) {
            let saved = self
                .sources
                .get(one.at)
                .cloned()
                .flatten()
                .and_then(|path| crate::load::read_song_file(&crate::load::song_file_for(&path)).ok());
            if saved.as_ref() != Some(&one.tidied.song) {
                one.not_done = Some("it couldn't be saved; it is changed in the window only".to_owned());
            }
        }
        if let Some(running) = self.running.as_mut() {
            // An editor open on a song tidied shows the slide it had, wherever it now is.
            let open = running.home_state.editing.as_ref().map(|draft| (draft.at, draft.slide.id));
            if let Some((at, id)) = open.filter(|(at, _)| changed.contains(&at.presentation)) {
                let place = self.show.presentations().get(at.presentation).and_then(|song| {
                    song.groups.iter().flat_map(|group| &group.slides).position(|slide| slide.id == id)
                });
                running.home_state.editing = place
                    .and_then(|slide| crate::editor::EditDraft::of(&self.show, sp_engine::SlideRef { slide, ..at }));
            }
            running.home_window.request_redraw();
        }
        self.refresh_output();
        let saved = looked.iter().filter(|one| one.changes() && one.not_done.is_none()).count();
        crate::applog::event(
            "library",
            &format!(
                "tidied {saved} songs{}",
                if made_backup {
                    format!("; their files as they were are in {}", backup.display())
                } else {
                    String::new()
                }
            ),
        );
        Ok(made_backup.then_some(backup))
    }
}

/// The folder a tidy keeps copies in: "Backup before tidying 2026-11-25" in the data folder, or
/// the same with a number when that is there already.
fn backup_folder(data: &Path, date: &str) -> PathBuf {
    let name = format!("{BACKUP_FOLDER} {date}");
    std::iter::once(data.join(&name))
        .chain((2..).map(|n| data.join(format!("{name} {n}"))))
        .find(|path| std::fs::symlink_metadata(path).is_err())
        .unwrap_or_else(|| data.join(name))
}

/// Copies a song's file into the backup folder, never over a file there.
fn keep_copy(file: &Path, backup: &Path) -> Result<(), String> {
    std::fs::create_dir_all(backup).map_err(|e| e.to_string())?;
    let name = file.file_name().ok_or("no file name")?.to_string_lossy().into_owned();
    let (stem, ending) = name.rsplit_once('.').map_or((name.as_str(), ""), |(s, e)| (s, e));
    let target = std::iter::once(backup.join(&name))
        .chain((2..).map(|n| backup.join(format!("{stem} {n}.{ending}"))))
        .find(|path| std::fs::symlink_metadata(path).is_err())
        .ok_or("no free name")?;
    std::fs::copy(file, &target).map(|_| ()).map_err(|e| e.to_string())
}

/// The songs with the same words (after tidying), by their places in `looked`, two or more to
/// a list; songs without words are never the same.
fn duplicates(looked: &[Looked]) -> Vec<Vec<usize>> {
    let mut by_words: HashMap<String, Vec<usize>> = HashMap::new();
    for (place, one) in looked.iter().enumerate() {
        let key = sp_model::words_key(&one.tidied.song);
        if !key.is_empty() {
            by_words.entry(key).or_default().push(place);
        }
    }
    let mut lists: Vec<Vec<usize>> = by_words.into_values().filter(|list| list.len() > 1).collect();
    lists.sort_by_key(|list| one_title(&looked[list[0]]).to_lowercase());
    lists
}

fn one_title(one: &Looked) -> &str {
    &one.tidied.song.title
}

/// What kind of title change a song is listed under, in the text: the first that applies.
fn title_kind(fixes: &[TitleFix]) -> Option<TitleFix> {
    [
        TitleFix::Number,
        TitleFix::Garbled,
        TitleFix::Untitled,
        TitleFix::Ending,
        TitleFix::Punctuation,
        TitleFix::Letters,
    ]
    .into_iter()
    .find(|kind| fixes.contains(kind))
}

fn split_words(why: SplitWhy) -> String {
    match why {
        SplitWhy::Separator => "at a \u{201c}* * *\u{201d} line".to_owned(),
        SplitWhy::Blank => "at a blank line".to_owned(),
        SplitWhy::Long(lines) => format!("{lines} lines"),
    }
}

fn split_word(why: SplitWhy) -> &'static str {
    match why {
        SplitWhy::Separator => "separator",
        SplitWhy::Blank => "blank_line",
        SplitWhy::Long(_) => "long",
    }
}

fn whole_words(why: WholeWhy) -> &'static str {
    match why {
        WholeWhy::Translation => "its translation doesn't part the same way",
        WholeWhy::Styled => "some of its words are styled on their own",
        WholeWhy::Boxes => "it has more than one box of words",
    }
}

fn whole_word(why: WholeWhy) -> &'static str {
    match why {
        WholeWhy::Translation => "translation",
        WholeWhy::Styled => "styled",
        WholeWhy::Boxes => "boxes",
    }
}

/// A song's character fixes in words: "3 joined letters, 1 line separator".
fn chars_words(chars: &sp_model::CharFixes) -> String {
    let names = [
        ("joined letter", "joined letters"),
        ("line separator made a line break", "line separators made line breaks"),
        ("odd space", "odd spaces"),
        ("tab", "tabs"),
        ("zero-width character", "zero-width characters"),
        ("run of spaces", "runs of spaces"),
        ("line trimmed", "lines trimmed"),
        ("\u{201c}* * *\u{201d} line", "\u{201c}* * *\u{201d} lines"),
        ("blank line", "blank lines"),
    ];
    chars
        .counts()
        .iter()
        .zip(names)
        .filter(|((_, n), _)| *n > 0)
        .map(|((_, n), (one, many))| format!("{n} {}", if *n == 1 { one } else { many }))
        .collect::<Vec<_>>()
        .join(", ")
}

/// "1 song", "2 songs".
fn counted(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// The reply's words: what it would do (or did), by kind, every song named.
fn report_text(
    looked: &[Looked],
    listed: usize,
    kept_in_step: usize,
    duplicates: &[Vec<usize>],
    applied: bool,
    backup: Option<&str>,
) -> String {
    let changing: Vec<&Looked> = looked.iter().filter(|one| one.changes()).collect();
    let mut out = Vec::new();
    let done = changing.iter().filter(|one| one.not_done.is_none()).count();
    out.push(if applied {
        format!("Tidied {done} of the library's {listed} songs.")
    } else if changing.is_empty() {
        format!("Nothing to tidy: the library's {listed} songs are as an import leaves them.")
    } else {
        format!(
            "Tidying would change {} of the library's {listed} songs. Nothing has changed yet: sp library tidy --apply does it.",
            changing.len()
        )
    });
    if let Some(folder) = backup {
        out.push(format!("Their files as they were are kept in {folder}."));
    }
    if kept_in_step > 0 {
        out.push(format!("Left alone: {kept_in_step} songs kept in step with the song catalog or TimelyChurch."));
    }
    let not_done: Vec<String> = changing
        .iter()
        .filter_map(|one| one.not_done.as_ref().map(|why| format!("    {}: {why}", one_title(one))))
        .collect();
    if !not_done.is_empty() {
        out.push(format!("\nLeft as they were ({}):", not_done.len()));
        out.extend(not_done);
    }
    let sections = [
        (TitleFix::Garbled, "Garbled titles put back"),
        (TitleFix::Untitled, "\u{201c}Untitled\u{201d} titles given the song's first line"),
        (TitleFix::Ending, "File endings taken off titles"),
        (TitleFix::Punctuation, "Commas, semicolons and colons taken off the end of titles"),
        (TitleFix::Letters, "Spaces and letters put right in titles"),
        (TitleFix::Number, "Number-only titles given the song's first line (--keep-numbers leaves them)"),
    ];
    for (kind, heading) in sections {
        let lines: Vec<String> = changing
            .iter()
            .filter(|one| title_kind(&one.tidied.title_fixes) == Some(kind))
            .map(|one| format!("    {}  \u{2192}  {}", one.before.title, one.tidied.song.title))
            .collect();
        if !lines.is_empty() {
            out.push(format!("\n{heading} ({}):", lines.len()));
            out.extend(lines);
        }
    }
    let parted: Vec<String> = changing
        .iter()
        .filter(|one| !one.tidied.splits.is_empty())
        .map(|one| {
            let each: Vec<String> = one
                .tidied
                .splits
                .iter()
                .map(|split| format!("slide {} into {} ({})", split.slide, split.into, split_words(split.why)))
                .collect();
            format!("    {}: {}", one_title(one), each.join("; "))
        })
        .collect();
    if !parted.is_empty() {
        let slides: usize = changing.iter().map(|one| one.tidied.splits.len()).sum();
        out.push(format!(
            "\nSlides parted ({} in {}):",
            counted(slides, "slide", "slides"),
            counted(parted.len(), "song", "songs")
        ));
        out.extend(parted);
    }
    let lettered: Vec<String> = changing
        .iter()
        .filter(|one| !one.tidied.chars.is_empty())
        .map(|one| format!("    {}: {}", one_title(one), chars_words(&one.tidied.chars)))
        .collect();
    if !lettered.is_empty() {
        let mut total = sp_model::CharFixes::default();
        changing.iter().for_each(|one| total.add(one.tidied.chars));
        out.push(format!(
            "\nWords put right in {}: {}.",
            counted(lettered.len(), "song", "songs"),
            chars_words(&total)
        ));
        out.extend(lettered);
    }
    let whole: Vec<String> = looked
        .iter()
        .filter(|one| !one.tidied.left_whole.is_empty())
        .map(|one| {
            let each: Vec<String> = one
                .tidied
                .left_whole
                .iter()
                .map(|left| format!("slide {} ({})", left.slide, whole_words(left.why)))
                .collect();
            format!("    {}: {}", one_title(one), each.join("; "))
        })
        .collect();
    if !whole.is_empty() {
        out.push(format!("\nSlides left whole, to look at by hand ({}):", counted(whole.len(), "song", "songs")));
        out.extend(whole);
    }
    if !duplicates.is_empty() {
        out.push(format!("\nSongs with the same words, listed only ({}):", counted(duplicates.len(), "set", "sets")));
        for list in duplicates {
            let named: Vec<String> = list
                .iter()
                .map(|&place| {
                    let one = &looked[place];
                    match one.libraries.as_slice() {
                        [] => one_title(one).to_owned(),
                        names => format!("{} ({})", one_title(one), names.join(", ")),
                    }
                })
                .collect();
            out.push(format!("    {}", named.join("  =  ")));
        }
    }
    out.join("\n")
}

/// The reply's data: every song with a change or a slide left whole, the songs with the same
/// words, and the counts.
fn report_data(
    looked: &[Looked],
    listed: usize,
    kept_in_step: usize,
    duplicates: &[Vec<usize>],
    applied: bool,
    backup: Option<&str>,
) -> serde_json::Value {
    let mut titles: HashMap<&'static str, usize> = HashMap::new();
    let mut total = sp_model::CharFixes::default();
    let mut songs = Vec::new();
    for one in looked.iter().filter(|one| one.changes() || !one.tidied.left_whole.is_empty()) {
        for fix in &one.tidied.title_fixes {
            *titles.entry(fix.word()).or_default() += 1;
        }
        total.add(one.tidied.chars);
        let chars: serde_json::Map<String, serde_json::Value> = one
            .tidied
            .chars
            .counts()
            .iter()
            .filter(|(_, n)| *n > 0)
            .map(|(k, n)| ((*k).to_owned(), (*n).into()))
            .collect();
        songs.push(serde_json::json!({
            "id": one.before.id,
            "title_before": one.before.title,
            "title": one.tidied.song.title,
            "title_fixes": one.tidied.title_fixes.iter().map(|fix| fix.word()).collect::<Vec<_>>(),
            "libraries": one.libraries,
            "changes": one.changes(),
            "splits": one.tidied.splits.iter().map(|split| serde_json::json!({
                "slide": split.slide, "into": split.into, "why": split_word(split.why),
                "lines": match split.why { SplitWhy::Long(n) => Some(n), _ => None },
            })).collect::<Vec<_>>(),
            "left_whole": one.tidied.left_whole.iter().map(|left| serde_json::json!({
                "slide": left.slide, "why": whole_word(left.why),
            })).collect::<Vec<_>>(),
            "chars": chars,
            "not_done": one.not_done,
        }));
    }
    let same: Vec<Vec<serde_json::Value>> = duplicates
        .iter()
        .map(|list| {
            list.iter()
                .map(|&place| {
                    let one = &looked[place];
                    serde_json::json!({"id": one.before.id, "title": one_title(one), "libraries": one.libraries})
                })
                .collect()
        })
        .collect();
    let changing = looked.iter().filter(|one| one.changes()).count();
    serde_json::json!({
        "applied": applied,
        "songs_looked_at": listed,
        "songs_changing": changing,
        "songs_done": if applied { Some(looked.iter().filter(|one| one.changes() && one.not_done.is_none()).count()) } else { None },
        "kept_in_step": kept_in_step,
        "backup": backup,
        "counts": {
            "titles": titles,
            "slides_parted": looked.iter().map(|one| one.tidied.splits.len()).sum::<usize>(),
            "songs_parted": looked.iter().filter(|one| !one.tidied.splits.is_empty()).count(),
            "slides_left_whole": looked.iter().map(|one| one.tidied.left_whole.len()).sum::<usize>(),
            "chars": total.counts().iter().map(|(k, n)| ((*k).to_owned(), serde_json::Value::from(*n))).collect::<serde_json::Map<_, _>>(),
            "duplicate_sets": duplicates.len(),
        },
        "songs": songs,
        "duplicates": same,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backups_never_go_over_a_folder_or_file_already_there() {
        let dir = std::env::temp_dir().join(format!("sp-tidy-backup-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let first = backup_folder(&dir, "2026-11-25");
        assert_eq!(first, dir.join("Backup before tidying 2026-11-25"));
        std::fs::create_dir_all(&first).unwrap();
        assert_eq!(backup_folder(&dir, "2026-11-25"), dir.join("Backup before tidying 2026-11-25 2"));
        let song = dir.join("song.sppres");
        std::fs::write(&song, "one").unwrap();
        keep_copy(&song, &first).unwrap();
        std::fs::write(&song, "two").unwrap();
        keep_copy(&song, &first).unwrap();
        assert_eq!(std::fs::read_to_string(first.join("song.sppres")).unwrap(), "one");
        assert_eq!(std::fs::read_to_string(first.join("song 2.sppres")).unwrap(), "two");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_number_title_is_listed_as_one_whatever_else_was_done() {
        assert_eq!(title_kind(&[TitleFix::Garbled, TitleFix::Number]), Some(TitleFix::Number));
        assert_eq!(title_kind(&[TitleFix::Letters, TitleFix::Punctuation]), Some(TitleFix::Punctuation));
        assert_eq!(title_kind(&[]), None);
    }
}
