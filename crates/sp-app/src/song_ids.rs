//! Songs in the library are found by their id: playlists, libraries, macros, the song that
//! starts with the app and slides' "go to song" actions all name a library song by the id in
//! its file, so they keep working when the library folder moves or goes to another computer.
//! A song opened from a file elsewhere is still named by that file's path, and one with no file
//! by its title.
//!
//! Files saved before songs were found by id named them by their file's full path or by title.
//! `settle` changes those to ids, once, in whichever library folder is in use: it is set off
//! by each file's version, keeps a copy of every file it rewrites, keeps a name it can't match
//! as it was (the song then shows as missing), and on a second run changes nothing.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::Value;
use sp_model::Uuid;

/// The folder, inside the library, where `settle` keeps a copy of each file it changes.
pub const BACKUP_FOLDER: &str = "Backup before song ids";

/// The key a song is known by: its id for a song in the library's Songs folder, its file's
/// path (without the ending) for one opened from elsewhere, its title for one with no file.
pub fn key_for(source: Option<&Path>, id: Uuid, title: &str) -> String {
    let library = crate::bible_store::data_dir().map(|dir| dir.join("Songs"));
    match source {
        Some(path) if library.is_some_and(|songs| path.parent() == Some(songs.as_path())) => id.to_string(),
        Some(path) => path.with_extension("").to_string_lossy().into_owned(),
        None => format!("title:{title}"),
    }
}

/// Where a song new to the library is kept: in the Songs folder, in a file named by its id, so
/// two songs with one title never collide.
pub fn library_path(id: Uuid) -> Option<PathBuf> {
    let songs = crate::bible_store::data_dir()?.join("Songs");
    Some(songs.join(format!("{id}.{}", crate::load::SONG_EXTENSION)))
}

/// The key a song new to the library is known by once it is kept at [`library_path`]: its id
/// (its title where there is no data folder). The window knows it before the song is filed (a
/// song of the song catalog put in a playlist joins the library right after; see
/// `catalog_songs`), and the app files it under the same key.
pub fn library_key(id: Uuid, title: &str) -> String {
    key_for(library_path(id).as_deref(), id, title)
}

/// The library's songs, as `settle` matches old names against them.
struct Songs {
    /// By the name of their file, without the ending.
    by_file: HashMap<String, String>,
    /// By title: every song's id with that title.
    by_title: HashMap<String, Vec<String>>,
}

impl Songs {
    /// The id an old name stands for, or `None` to keep the name as it is: already an id, a
    /// file outside the library, or a song that can't be found (which then shows as missing).
    fn resolve(&self, key: &str) -> Option<String> {
        if Uuid::parse_str(key).is_ok() {
            return None;
        }
        if let Some(title) = key.strip_prefix("title:") {
            return match self.by_title.get(title).map(Vec::as_slice) {
                Some([one]) => Some(one.clone()),
                _ => None,
            };
        }
        // A file in a Songs folder is matched by its name inside this library, wherever the
        // path said the library was.
        let path = Path::new(key);
        let in_songs = path.parent().and_then(Path::file_name).is_some_and(|name| name == "Songs");
        if !in_songs {
            return None;
        }
        let name = path.file_name()?.to_string_lossy();
        let stem = [crate::load::SONG_EXTENSION, crate::load::OLD_SONG_EXTENSION]
            .iter()
            .find_map(|ending| name.strip_suffix(&format!(".{ending}")))
            .unwrap_or(&name);
        self.by_file.get(stem).cloned()
    }
}

/// What `settle` did, a line each, for the log.
type Done = Vec<String>;

/// Changes the old song names in `folder`'s files to ids, and gives a song file whose id another
/// file already has (a copy made in the Finder) an id of its own. With `backup`, a copy of each
/// file is kept in `BACKUP_FOLDER` before it is first changed. Returns what was done.
pub fn settle(folder: &Path, backup: bool) -> Done {
    let mut done = Done::new();
    let mut keeper = Keeper { folder: folder.to_path_buf(), backup, done: Vec::new() };
    // The song files are read once, and kept up to date as they are written here.
    let mut files = song_files(folder);
    let songs = read_songs(&mut files, &mut keeper, &mut done);
    // Whether this start converts files from before songs were found by id. The preferences and
    // macros say no version, so a name there that can't be matched is said only on such a start,
    // not on every start after.
    let services = folder.join("services.json");
    let old_services = read_json(&services).is_some_and(|v| v.get("version").and_then(Value::as_u64).unwrap_or(1) < 2);
    let old_libraries = read_json(&folder.join("libraries.json")).is_some_and(|v| v.is_array());
    let old_songs = files
        .iter()
        .any(|(_, value)| value.get("version").and_then(Value::as_u64).unwrap_or(1) < 2 && has_song_action(value));
    let converting = old_services || old_libraries || old_songs;

    // Playlists.
    if let Some(mut value) = read_json(&services)
        && value.get("version").and_then(Value::as_u64).unwrap_or(1) < 2
    {
        for service in value.get_mut("list").and_then(Value::as_array_mut).into_iter().flatten() {
            for item in service.get_mut("items").and_then(Value::as_array_mut).into_iter().flatten() {
                if item.get("kind").and_then(Value::as_str) != Some("song") {
                    continue;
                }
                let old = item.get("key").and_then(Value::as_str).unwrap_or_default().to_owned();
                if let Some(id) = note(&mut done, &songs, "playlist item", &old, true) {
                    item["key"] = Value::String(id);
                }
            }
        }
        value["version"] = Value::from(2);
        keeper.write(&services, &value, &mut done);
    }

    // Libraries: a list of keys each, in a file that said no version before.
    let libraries = folder.join("libraries.json");
    if let Some(Value::Array(mut list)) = read_json(&libraries) {
        for library in &mut list {
            for key in library.get_mut("songs").and_then(Value::as_array_mut).into_iter().flatten() {
                let old = key.as_str().unwrap_or_default().to_owned();
                if let Some(id) = note(&mut done, &songs, "library entry", &old, true) {
                    *key = Value::String(id);
                }
            }
        }
        let wrapped = serde_json::json!({
            "format": crate::libraries::FORMAT,
            "version": crate::libraries::VERSION,
            "libraries": list,
        });
        keeper.write(&libraries, &wrapped, &mut done);
    }

    // The song that starts on the Announcements layer with the app. Its file has no version,
    // so it is looked at each time; a name that can't be matched is left quietly.
    let prefs = folder.join("preferences.json");
    if let Some(mut value) = read_json(&prefs)
        && let Some(start) = value.get_mut("start_with")
        && start.get("play").and_then(Value::as_str) == Some("announcements")
    {
        let old = start.get("song").and_then(Value::as_str).unwrap_or_default().to_owned();
        if let Some(id) = note(&mut done, &songs, "song started with the app", &old, converting) {
            start["song"] = Value::String(id);
            keeper.write(&prefs, &value, &mut done);
        }
    }

    // Macros' "go to song" steps; the same.
    let macros = folder.join("macros.json");
    if let Some(mut value) = read_json(&macros)
        && song_actions(&mut value, &songs, "macro step", converting, &mut done)
    {
        keeper.write(&macros, &value, &mut done);
    }

    // Slides' "go to song" actions, in song files saved before songs were found by id. A song
    // with such an action is written as version 2, which an older build refuses rather than
    // misreads.
    for (path, mut value) in files {
        if value.get("version").and_then(Value::as_u64).unwrap_or(1) >= 2 || !has_song_action(&value) {
            continue;
        }
        song_actions(&mut value, &songs, "slide action", true, &mut done);
        value["version"] = Value::from(2);
        keeper.write(&path, &value, &mut done);
    }
    done.extend(keeper.done);
    done
}

/// Says what became of an old name, and passes on the id it now is, if any. Not finding a song
/// (or finding several with the title) is said only with `say_missing`.
fn note(done: &mut Done, songs: &Songs, name: &str, old: &str, say_missing: bool) -> Option<String> {
    let new = songs.resolve(old);
    match &new {
        Some(id) => done.push(format!("{name}: {} is now the song {id}", shown(old))),
        None if say_missing && looks_like_library_song(old) => {
            let several = old.strip_prefix("title:").and_then(|title| songs.by_title.get(title)).map_or(0, Vec::len);
            let why = if several > 1 {
                format!("{several} songs have {}", shown(old))
            } else {
                format!("no song found for {}", shown(old))
            };
            done.push(format!("{name}: {why}; kept, and shown as missing"));
        }
        None => {}
    }
    new
}

/// Whether an old name pointed at a song in a library (by title, or a file in a Songs folder),
/// so that not finding it is worth saying.
fn looks_like_library_song(key: &str) -> bool {
    key.starts_with("title:") || Path::new(key).parent().and_then(Path::file_name).is_some_and(|name| name == "Songs")
}

/// A name as the log shows it: a path by its last two parts, so no home folder is written.
fn shown(key: &str) -> String {
    match key.strip_prefix("title:") {
        Some(title) => format!("the title \u{201c}{title}\u{201d}"),
        None => {
            let parts: Vec<&str> = key.rsplit(['/', '\\']).take(2).collect();
            format!("the file \u{2026}/{}", parts.into_iter().rev().collect::<Vec<_>>().join("/"))
        }
    }
}

/// Whether there is a `{"do": "song", …}` anywhere inside `value`.
fn has_song_action(value: &Value) -> bool {
    match value {
        Value::Object(map) => {
            map.get("do").and_then(Value::as_str) == Some("song") || map.values().any(has_song_action)
        }
        Value::Array(list) => list.iter().any(has_song_action),
        _ => false,
    }
}

/// Changes the old names in every `{"do": "song", "key": …}` inside `value` to ids. Returns
/// whether any changed.
fn song_actions(value: &mut Value, songs: &Songs, name: &str, say_missing: bool, done: &mut Done) -> bool {
    match value {
        Value::Object(map) => {
            let mut any = false;
            if map.get("do").and_then(Value::as_str) == Some("song")
                && let Some(old) = map.get("key").and_then(Value::as_str).map(str::to_owned)
                && let Some(id) = note(done, songs, name, &old, say_missing)
            {
                map.insert("key".to_owned(), Value::String(id));
                any = true;
            }
            for inner in map.values_mut() {
                any |= song_actions(inner, songs, name, say_missing, done);
            }
            any
        }
        Value::Array(list) => {
            let mut any = false;
            for inner in list {
                any |= song_actions(inner, songs, name, say_missing, done);
            }
            any
        }
        _ => false,
    }
}

/// The song files in the folder's Songs folder, in name order, read as JSON.
fn song_files(folder: &Path) -> Vec<(PathBuf, Value)> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(folder.join("Songs"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| crate::load::is_song_file(path))
        .collect();
    paths.sort();
    paths.into_iter().filter_map(|path| read_json(&path).map(|value| (path, value))).collect()
}

/// Reads the library's songs. Where several files have one id (a copy made in the Finder), the
/// file named after that id keeps it, else the oldest, else the first by name; each of the others
/// is given a new id of its own, written back to its file.
fn read_songs(read: &mut [(PathBuf, Value)], keeper: &mut Keeper, done: &mut Done) -> Songs {
    let mut songs = Songs { by_file: HashMap::new(), by_title: HashMap::new() };
    let files: Vec<(PathBuf, Value, String)> = read
        .iter()
        .filter_map(|(path, value)| {
            let id = value.get("id").and_then(Value::as_str)?.to_owned();
            Some((path.clone(), value.clone(), id))
        })
        .collect();
    let born = |path: &Path| std::fs::metadata(path).and_then(|meta| meta.created().or_else(|_| meta.modified())).ok();
    // Which file keeps each id.
    let mut keeps: HashMap<&str, &Path> = HashMap::new();
    for (path, _, id) in &files {
        let named_for_it = |path: &Path| path.file_stem().is_some_and(|stem| stem.to_string_lossy() == id.as_str());
        match keeps.get(id.as_str()) {
            None => {
                keeps.insert(id, path);
            }
            Some(kept) => {
                let better = match (named_for_it(path), named_for_it(kept)) {
                    (true, false) => true,
                    (false, true) => false,
                    // Files come in name order, so the first by name wins a tie.
                    _ => born(path).zip(born(kept)).is_some_and(|(new, old)| new < old),
                };
                if better {
                    keeps.insert(id, path);
                }
            }
        }
    }
    for (path, value, id) in &files {
        let mut id = id.clone();
        if let Some(kept) = keeps.get(id.as_str()).copied()
            && kept != path.as_path()
        {
            let fresh = Uuid::new_v4().to_string();
            done.push(format!(
                "{} had the same id as {}; it now has an id of its own",
                file_name(path),
                file_name(kept)
            ));
            let mut value = value.clone();
            value["id"] = Value::String(fresh.clone());
            keeper.write(path, &value, done);
            // The files read are kept as written, for the passes after this one.
            if let Some((_, kept_value)) = read.iter_mut().find(|(read_path, _)| read_path == path) {
                *kept_value = value;
            }
            id = fresh;
        }
        if let Some(stem) = path.file_stem() {
            songs.by_file.insert(stem.to_string_lossy().into_owned(), id.clone());
        }
        let title = value.get("title").and_then(Value::as_str).unwrap_or_default().to_owned();
        songs.by_title.entry(title).or_default().push(id);
    }
    songs
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(String::new, |name| name.to_string_lossy().into_owned())
}

fn read_json(path: &Path) -> Option<Value> {
    let meta = std::fs::metadata(path).ok()?;
    // Settings and songs are small; anything far bigger is not one of them.
    if !meta.is_file() || meta.len() > 64 * 1024 * 1024 {
        return None;
    }
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/// Writes changed files, each beside itself and then moved into place, after keeping a copy
/// of the file as it was (once) when asked to.
struct Keeper {
    folder: PathBuf,
    backup: bool,
    done: Done,
}

impl Keeper {
    fn write(&mut self, path: &Path, value: &Value, done: &mut Done) {
        if self.backup
            && let Ok(inside) = path.strip_prefix(&self.folder)
        {
            let copy = self.folder.join(BACKUP_FOLDER).join(inside);
            if !copy.exists() {
                let kept = copy.parent().is_some_and(|dir| std::fs::create_dir_all(dir).is_ok())
                    && std::fs::copy(path, &copy).is_ok();
                if !kept {
                    done.push(format!("could not keep a copy of {}; left it as it was", file_name(path)));
                    return;
                }
            }
        }
        let mut text = serde_json::to_string_pretty(value).unwrap_or_default();
        text.push('\n');
        let partial = path.with_extension("ids-part");
        match std::fs::write(&partial, text).and_then(|()| std::fs::rename(&partial, path)) {
            Ok(()) => self.done.push(format!("rewrote {}", file_name(path))),
            Err(e) => {
                let _ = std::fs::remove_file(&partial);
                done.push(format!("could not rewrite {}: {e}", file_name(path)));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("sp-song-ids-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Songs")).unwrap();
        dir
    }

    fn song(folder: &Path, file: &str, id: &str, title: &str, extra: &str) {
        let text = format!(
            r#"{{"format":"softpresenter-presentation","version":1,"id":"{id}","title":"{title}","groups":[]{extra}}}"#
        );
        std::fs::write(folder.join("Songs").join(file), text).unwrap();
    }

    const RIVER: &str = "11111111-1111-4111-8111-111111111111";
    const HILLS: &str = "22222222-2222-4222-8222-222222222222";

    /// A library as an older build left it, its paths naming another computer's folder.
    fn old_library(name: &str) -> PathBuf {
        let folder = temp(name);
        let action =
            format!(r#","slides_extra":[{{"do":"song","key":"/elsewhere/Library/SoftPresenter/Songs/{HILLS}"}}]"#);
        song(&folder, &format!("{RIVER}.sppres"), RIVER, "River Song", &action);
        song(&folder, &format!("{HILLS}.sppres"), HILLS, "Quiet Hills", "");
        let old = |id: &str| format!("/elsewhere/Library/SoftPresenter/Songs/{id}");
        let services = serde_json::json!({
            "format": "softpresenter-services", "version": 1, "current": 0,
            "list": [{"name": "Sunday", "items": [
                {"kind": "song", "key": old(RIVER), "title": "River Song"},
                {"kind": "song", "key": "title:Quiet Hills", "title": "Quiet Hills"},
                {"kind": "song", "key": "title:New Song", "title": "New Song"},
                {"kind": "song", "key": "/Users/someone/Desktop/notes", "title": "notes"},
                {"kind": "header", "name": "Sermon"},
            ]}],
        });
        std::fs::write(folder.join("services.json"), services.to_string()).unwrap();
        let libraries =
            serde_json::json!([{"id": Uuid::new_v4(), "name": "Youth", "songs": [old(HILLS), old("gone")]}]);
        std::fs::write(folder.join("libraries.json"), libraries.to_string()).unwrap();
        let prefs = serde_json::json!({"dissolve": true, "start_with": {"play": "announcements", "song": old(RIVER)}});
        std::fs::write(folder.join("preferences.json"), prefs.to_string()).unwrap();
        let macros = serde_json::json!([{"id": Uuid::new_v4(), "name": "Open", "actions": [{"do": "song", "key": "title:River Song"}]}]);
        std::fs::write(folder.join("macros.json"), macros.to_string()).unwrap();
        folder
    }

    fn read(path: PathBuf) -> Value {
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    #[test]
    fn old_names_become_ids_and_what_cant_be_found_is_kept() {
        let folder = old_library("settle");
        let done = settle(&folder, true);
        let services = read(folder.join("services.json"));
        assert_eq!(services["version"], 2);
        let keys: Vec<&str> = services["list"][0]["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|item| item.get("key").and_then(Value::as_str))
            .collect();
        assert_eq!(keys, [RIVER, HILLS, "title:New Song", "/Users/someone/Desktop/notes"]);
        let libraries = read(folder.join("libraries.json"));
        assert_eq!(libraries["version"], 2);
        assert_eq!(
            libraries["libraries"][0]["songs"],
            serde_json::json!([HILLS, "/elsewhere/Library/SoftPresenter/Songs/gone"])
        );
        assert_eq!(read(folder.join("preferences.json"))["start_with"]["song"], RIVER);
        assert_eq!(read(folder.join("preferences.json"))["dissolve"], true, "the other choices stay");
        assert_eq!(read(folder.join("macros.json"))[0]["actions"][0]["key"], RIVER);
        let river = read(folder.join("Songs").join(format!("{RIVER}.sppres")));
        assert_eq!(river["version"], 2);
        assert_eq!(river["slides_extra"][0]["key"], HILLS);
        let hills = read(folder.join("Songs").join(format!("{HILLS}.sppres")));
        assert_eq!(hills["version"], 1, "a song without a song action is left alone");
        // The log names what changed and what was kept, without anyone's home folder.
        assert!(
            done.iter().any(|line| line.contains("no song found for the title \u{201c}New Song\u{201d}")),
            "{done:?}"
        );
        assert!(done.iter().all(|line| !line.contains("/elsewhere/Library")), "{done:?}");
        // A copy of each file as it was is kept.
        let kept = folder.join(BACKUP_FOLDER);
        assert_eq!(read(kept.join("services.json"))["version"], 1);
        assert!(read(kept.join("libraries.json")).is_array());
        assert!(kept.join("Songs").join(format!("{RIVER}.sppres")).is_file());
        assert!(!kept.join("Songs").join(format!("{HILLS}.sppres")).exists());
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_second_run_changes_nothing() {
        let folder = old_library("twice");
        settle(&folder, true);
        let snapshot = |folder: &Path| -> Vec<(PathBuf, String)> {
            let mut all = Vec::new();
            let mut stack = vec![folder.to_path_buf()];
            while let Some(dir) = stack.pop() {
                for entry in std::fs::read_dir(dir).unwrap().flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        stack.push(path);
                    } else {
                        all.push((path.clone(), std::fs::read_to_string(&path).unwrap()));
                    }
                }
            }
            all.sort();
            all
        };
        let before = snapshot(&folder);
        let done = settle(&folder, true);
        assert!(done.is_empty(), "{done:?}");
        assert_eq!(snapshot(&folder), before);
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_song_file_copied_in_the_finder_gets_an_id_of_its_own() {
        let folder = temp("copies");
        // "<id> copy" sorts before "<id>" (a space before a dot): the file named after the id
        // keeps it all the same.
        song(&folder, &format!("{RIVER} copy.sppres"), RIVER, "River Song", "");
        song(&folder, &format!("{RIVER}.sppres"), RIVER, "River Song", "");
        let done = settle(&folder, false);
        let original = read(folder.join(format!("Songs/{RIVER}.sppres")));
        let copy = read(folder.join(format!("Songs/{RIVER} copy.sppres")));
        assert_eq!(original["id"], RIVER, "the file named after the id keeps it");
        assert_ne!(copy["id"], RIVER);
        assert!(Uuid::parse_str(copy["id"].as_str().unwrap()).is_ok());
        assert!(
            done.iter().any(|line| line.contains(&format!("{RIVER} copy.sppres had the same id as {RIVER}.sppres"))),
            "{done:?}"
        );
        assert!(!folder.join(BACKUP_FOLDER).exists(), "no copies kept when not asked");
        assert!(settle(&folder, false).is_empty());
        // Neither named after the id: the oldest keeps it.
        let other = temp("copies-oldest");
        song(&other, "b.sppres", HILLS, "Quiet Hills", "");
        std::thread::sleep(std::time::Duration::from_millis(20));
        song(&other, "a.sppres", HILLS, "Quiet Hills", "");
        settle(&other, false);
        assert_eq!(read(other.join("Songs/b.sppres"))["id"], HILLS, "the older file keeps the id");
        assert_ne!(read(other.join("Songs/a.sppres"))["id"], HILLS);
        let _ = std::fs::remove_dir_all(&folder);
        let _ = std::fs::remove_dir_all(&other);
    }

    #[test]
    fn a_copied_song_with_an_old_slide_action_keeps_both_changes() {
        // The copy needs an id of its own and its old "go to song" action changed: the second
        // change is written over the first, so it must start from the file as the first left it.
        let folder = temp("copy-and-action");
        let action = r#","slides_extra":[{"do":"song","key":"title:Quiet Hills"}]"#;
        song(&folder, &format!("{RIVER} copy.sppres"), RIVER, "River Song", action);
        song(&folder, &format!("{RIVER}.sppres"), RIVER, "River Song", "");
        song(&folder, &format!("{HILLS}.sppres"), HILLS, "Quiet Hills", "");
        settle(&folder, false);
        let copy = read(folder.join(format!("Songs/{RIVER} copy.sppres")));
        assert_ne!(copy["id"], RIVER, "the copy kept its id of its own");
        assert_eq!(copy["version"], 2);
        assert_eq!(copy["slides_extra"][0]["key"], HILLS, "and its action points at the song by id");
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn names_are_matched_inside_this_library_only() {
        let songs = Songs {
            by_file: HashMap::from([(RIVER.to_owned(), RIVER.to_owned())]),
            by_title: HashMap::from([
                ("River Song".to_owned(), vec![RIVER.to_owned()]),
                ("Twice".to_owned(), vec![RIVER.to_owned(), HILLS.to_owned()]),
            ]),
        };
        assert_eq!(songs.resolve(&format!("/a/b/Songs/{RIVER}")).as_deref(), Some(RIVER));
        assert_eq!(songs.resolve(&format!("/a/b/Songs/{RIVER}.sppres")).as_deref(), Some(RIVER));
        assert_eq!(songs.resolve(&format!("/a/b/Other/{RIVER}")), None, "not a Songs folder");
        assert_eq!(songs.resolve("title:River Song").as_deref(), Some(RIVER));
        assert_eq!(songs.resolve("title:Twice"), None, "two songs with that title: kept as missing");
        assert_eq!(songs.resolve(RIVER), None, "already an id");
    }
}
