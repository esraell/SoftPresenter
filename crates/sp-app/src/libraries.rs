//! Libraries: named collections of songs, such as Christmas or Youth, beside All Songs. A song
//! can be in any number of them; a library only lists songs, by their keys (a library song's
//! id). Kept in `libraries.json`, which before version 2 was a bare list naming songs by path.

use crate::i18n::{t, tf};
use serde::{Deserialize, Serialize};
use sp_model::Uuid;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Library {
    pub id: Uuid,
    pub name: String,
    /// The songs in it, by the keys the app knows them by.
    pub songs: Vec<String>,
}

impl Library {
    pub fn new(name: String) -> Library {
        Library { id: Uuid::new_v4(), name, songs: Vec::new() }
    }

    pub fn has(&self, key: &str) -> bool {
        self.songs.iter().any(|have| have == key)
    }

    /// Puts a song in, or takes it out if it is in.
    pub fn toggle(&mut self, key: &str) {
        if self.has(key) {
            self.songs.retain(|have| have != key);
        } else {
            self.songs.push(key.to_owned());
        }
    }
}

/// A name no library has: "New Library", "New Library 2", and so on.
pub fn new_name(libraries: &[Library]) -> String {
    let taken = |name: &str| libraries.iter().any(|library| library.name == name);
    let base = t("notices.new_library.name", "New Library");
    std::iter::once(base.to_owned())
        .chain((2..).map(|n| format!("{base} {n}")))
        .find(|name| !taken(name))
        .unwrap_or_default()
}

/// What the libraries file says it is.
pub const FORMAT: &str = "softpresenter-libraries";
/// The newest version of the file this build writes and reads: 2, songs named by id.
pub const VERSION: u32 = 2;

#[derive(Serialize, Deserialize)]
struct File {
    format: String,
    version: u32,
    libraries: Vec<Library>,
}

fn file() -> Option<std::path::PathBuf> {
    crate::bible_store::data_dir().map(|dir| dir.join("libraries.json"))
}

/// Reads the libraries file: version 2, or the bare list of before. The error is a sentence.
pub fn from_json(text: &str) -> Result<Vec<Library>, String> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("The libraries file could not be read ({e})."))?;
    if value.is_array() {
        return serde_json::from_value(value).map_err(|e| format!("The libraries file could not be read ({e})."));
    }
    if value.get("format").and_then(|f| f.as_str()) != Some(FORMAT) {
        return Err("That is not a SoftPresenter libraries file.".to_owned());
    }
    if value.get("version").and_then(serde_json::Value::as_u64).unwrap_or(0) > u64::from(VERSION) {
        return Err("The libraries file was made by a newer SoftPresenter.".to_owned());
    }
    serde_json::from_value::<File>(value)
        .map(|file| file.libraries)
        .map_err(|e| format!("The libraries file could not be read ({e})."))
}

pub fn to_json(libraries: &[Library]) -> String {
    let file = File { format: FORMAT.to_owned(), version: VERSION, libraries: libraries.to_vec() };
    let mut text = serde_json::to_string_pretty(&file).unwrap_or_default();
    text.push('\n');
    text
}

/// The libraries as last saved. A file that can't be read (or comes from a newer build) is kept
/// aside (`whole_file`), so the empty list used instead is never saved over it, and comes back to
/// tell the operator.
pub fn load() -> (Vec<Library>, Option<crate::whole_file::Unread>) {
    file().map_or((Vec::new(), None), |path| load_from(&path))
}

fn load_from(path: &std::path::Path) -> (Vec<Library>, Option<crate::whole_file::Unread>) {
    crate::whole_file::read_or_default(path, crate::whole_file::What::Libraries, from_json)
}

/// The libraries' file and what goes in it, for the library's writer to write (`writer`).
pub fn to_save(libraries: &[Library]) -> Result<crate::writer::Save, String> {
    let path = file()
        .ok_or_else(|| t("media.no_place", "Could not find where to keep settings on this computer.").to_owned())?;
    Ok(to_save_at(path, libraries))
}

fn to_save_at(path: std::path::PathBuf, libraries: &[Library]) -> crate::writer::Save {
    let text = to_json(libraries);
    let mut save =
        crate::writer::Save::new(path, crate::writer::Kind::Libraries, crate::writer::Guard::Any, move || {
            text.into_bytes()
        });
    save.make_folder = true;
    save
}

/// What the window says when the libraries couldn't be written.
pub fn not_saved(error: &std::io::Error) -> String {
    tf(
        "notices.save.libraries",
        "Could not save the libraries: {error}",
        &[("error", &crate::notices::write_problem(error))],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_libraries_file_that_cant_be_read_is_kept_aside_and_never_saved_over() {
        let mut christmas = Library::new("Christmas".to_owned());
        christmas.toggle("songs/joy");
        let good = to_json(&[christmas]);
        crate::whole_file::tests::check_loader(
            "libraries.json",
            crate::whole_file::What::Libraries,
            good.as_bytes(),
            |path| load_from(path).1,
            |path| {
                let writer = crate::writer::Writer::start(|| {});
                writer.write(to_save_at(path.to_owned(), &[]));
                writer.drain();
            },
        );
    }

    #[test]
    fn songs_go_in_and_out_of_libraries_and_names_do_not_repeat() {
        let mut christmas = Library::new(new_name(&[]));
        assert_eq!(christmas.name, "New Library");
        christmas.toggle("songs/silent-night");
        christmas.toggle("songs/joy");
        assert!(christmas.has("songs/joy"));
        christmas.toggle("songs/joy");
        assert_eq!(christmas.songs, ["songs/silent-night"]);
        assert_eq!(new_name(std::slice::from_ref(&christmas)), "New Library 2");
        let saved = vec![christmas];
        assert_eq!(from_json(&to_json(&saved)).unwrap(), saved);
        // The bare list of before still reads; a newer file or another kind is refused.
        assert_eq!(from_json(&serde_json::to_string(&saved).unwrap()).unwrap(), saved);
        assert!(
            from_json(r#"{"format":"softpresenter-libraries","version":3,"libraries":[]}"#)
                .unwrap_err()
                .contains("newer")
        );
        assert!(from_json(r#"{"format":"other","version":1}"#).is_err());
    }
}
