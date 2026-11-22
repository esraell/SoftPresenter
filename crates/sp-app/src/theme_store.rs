//! Where themes are kept on this computer: one file for each in the Themes folder, named by
//! the theme's id, and a note of which one is in use.

use std::path::{Path, PathBuf};

use sp_model::{Theme, Uuid};

fn folder() -> Option<PathBuf> {
    crate::bible_store::data_dir().map(|dir| dir.join("Themes"))
}

/// The file a theme is saved in.
pub fn file_of(theme: &Theme) -> Option<PathBuf> {
    folder().map(|folder| folder.join(format!("{}.sptheme", theme.id)))
}

fn in_use_file() -> Option<PathBuf> {
    crate::bible_store::data_dir().map(|dir| dir.join("themes-in-use.txt"))
}

/// Every theme, the place in the list of the main theme (the one songs and Bible slides are
/// drawn with), and the places of themes changed while loading, to be saved. The built-in
/// theme is always first, as it was last saved if it has been changed; the others follow in the
/// order they were dragged into, and any the note doesn't list after them in name order.
pub fn load_all() -> (Vec<Theme>, usize, Vec<usize>) {
    let mut themes = folder().map_or_else(|| vec![Theme::builtin()], |folder| load_from(&folder));
    let note = in_use_file().and_then(|file| std::fs::read_to_string(file).ok()).unwrap_or_default();
    let (main, changed) = settle(&mut themes, &note);
    (themes, main, changed)
}

/// Puts the themes in order and finds the main one from the note of what is in use. A note
/// from before templates said what they were for names, besides the songs theme, the theme
/// and template Bible slides used: that template is brought into the main theme as its Bible
/// template, looking as it did, and the main theme is returned among those to save.
fn settle(themes: &mut [Theme], note: &str) -> (usize, Vec<usize>) {
    themes[1..].sort_by_key(|theme| theme.name.to_lowercase());
    // The order kept: `order=<id>,<id>…`.
    let order: Vec<Uuid> = note
        .lines()
        .find_map(|line| line.strip_prefix("order="))
        .map(|ids| ids.split(',').filter_map(|id| Uuid::parse_str(id.trim()).ok()).collect())
        .unwrap_or_default();
    themes[1..].sort_by_key(|theme| order.iter().position(|id| *id == theme.id).unwrap_or(usize::MAX));
    // The note has a line for each thing: `main=<id>` (earlier, `songs=<id>`), and in old
    // notes `bible=<id>` and `bible_template=<id>`.
    let noted = |kind: &str| {
        note.lines()
            .find_map(|line| line.strip_prefix(kind)?.strip_prefix('='))
            .and_then(|id| Uuid::parse_str(id.trim()).ok())
    };
    let place = |id: Uuid| themes.iter().position(|theme| theme.id == id);
    let main = noted("main").or_else(|| noted("songs")).and_then(place).unwrap_or(0);
    if !note.lines().any(|line| line.starts_with("bible=")) {
        return (main, Vec::new());
    }
    // The theme Bible slides used: the one noted, else the built-in Bible theme (as saved, if
    // it was).
    let bible_at = noted("bible").and_then(place).or_else(|| place(sp_model::BIBLE_THEME_ID));
    let source = bible_at.map_or_else(Theme::builtin_bible, |at| themes[at].clone());
    let by_id = noted("bible_template").and_then(|id| source.templates.iter().position(|t| t.id == id));
    let alone = note.lines().any(|line| line.trim() == "bible_layout=alone");
    let template = by_id.unwrap_or(usize::from(alone)).min(source.templates.len().saturating_sub(1));
    if bible_at == Some(main) {
        themes[main].set_bible_template(template);
    } else if let Some(original) = source.templates.get(template) {
        let brought = sp_model::ThemeTemplate {
            id: Uuid::new_v4(),
            name: "Bible".to_owned(),
            songs: false,
            bible: true,
            ..original.clone()
        };
        let theme = &mut themes[main];
        // It takes the place of the main theme's own Bible template, if it has one.
        match theme.templates.iter().position(|t| t.bible) {
            Some(at) => theme.templates[at] = brought,
            None => theme.templates.push(brought),
        }
        let at = theme.templates.len() - 1;
        let at = theme.templates.iter().position(|t| t.bible).unwrap_or(at);
        theme.set_bible_template(at);
    }
    (main, vec![main])
}

fn load_from(folder: &Path) -> Vec<Theme> {
    let mut themes = vec![Theme::builtin()];
    let files = std::fs::read_dir(folder).ok();
    for path in files.into_iter().flatten().flatten().map(|entry| entry.path()) {
        // Themes saved before the project was renamed end in .pctheme; they still open.
        if path.extension().is_none_or(|ext| ext != "sptheme" && ext != "pctheme") {
            continue;
        }
        let read = std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|text| Theme::from_json(&text));
        match read {
            // The built-in theme's own file takes its place.
            Ok(theme) if theme.id.is_nil() => themes[0] = theme,
            Ok(theme) if themes.iter().all(|have| have.id != theme.id) => themes.push(theme),
            Ok(_) => {}
            Err(problem) => eprintln!("{}: {problem}", path.display()),
        }
    }
    themes
}

/// Writes a theme to its file. The error is a plain sentence.
pub fn save(theme: &Theme) -> Result<(), String> {
    let folder = folder().ok_or_else(|| "Could not find where to keep themes on this computer.".to_owned())?;
    save_in(&folder, theme)
}

fn save_in(folder: &Path, theme: &Theme) -> Result<(), String> {
    std::fs::create_dir_all(folder).map_err(|e| format!("Could not make the Themes folder: {e}"))?;
    let path = folder.join(format!("{}.sptheme", theme.id));
    // Write beside it and rename, so a half-written file is never left under the real name.
    let partial = path.with_extension("sptheme.part");
    std::fs::write(&partial, theme.to_json())
        .and_then(|()| std::fs::rename(&partial, &path))
        .map_err(|e| format!("Could not save the theme: {e}"))?;
    // The same theme saved before the rename is replaced by the new file, not left beside it.
    let _ = std::fs::remove_file(folder.join(format!("{}.pctheme", theme.id)));
    Ok(())
}

/// Removes a theme's file. A theme that was never saved has none, which is not a failure.
pub fn delete(theme: &Theme) -> Result<(), String> {
    let Some(folder) = folder() else { return Ok(()) };
    // Its file, whichever ending it was saved with.
    for ending in ["sptheme", "pctheme"] {
        match std::fs::remove_file(folder.join(format!("{}.{ending}", theme.id))) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("Could not delete the theme: {e}")),
        }
    }
    Ok(())
}

/// Remembers which theme is the main one, and the order of the list, for the next start.
pub fn remember_in_use(main: &Theme, themes: &[Theme]) {
    if let Some(file) = in_use_file() {
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(file, in_use_note(main, themes));
    }
}

/// The note of the main theme and the order of the list.
fn in_use_note(main: &Theme, themes: &[Theme]) -> String {
    let order: Vec<String> = themes.iter().map(|theme| theme.id.to_string()).collect();
    format!("main={}\norder={}\n", main.id, order.join(","))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_themes_come_back_and_a_saved_built_in_takes_its_place() {
        let folder = std::env::temp_dir().join(format!("softpresenter-test-themes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        assert_eq!(load_from(&folder), vec![Theme::builtin()]);

        let mut sunday = Theme::builtin();
        sunday.id = Uuid::new_v4();
        sunday.name = "Sunday".to_owned();
        let mut built_in = Theme::builtin();
        built_in.templates[1].boxes[0].rect.y = 10.0;
        save_in(&folder, &sunday).unwrap();
        save_in(&folder, &built_in).unwrap();
        std::fs::write(folder.join("broken.pctheme"), "not a theme").unwrap();

        let loaded = load_from(&folder);
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0], built_in);
        assert_eq!(loaded[1], sunday);
        // The old built-in Bible theme, if it was saved, is a theme like any other.
        save_in(&folder, &Theme::builtin_bible()).unwrap();
        assert_eq!(load_from(&folder).len(), 3);
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn the_themes_come_back_in_the_order_they_were_dragged_into() {
        let theme = |name: &str| Theme { id: Uuid::new_v4(), name: name.to_owned(), ..Theme::builtin() };
        let (alpha, beta, gamma) = (theme("Alpha"), theme("Beta"), theme("Gamma"));
        // Dragged into Gamma, Alpha, Beta, with the built-in theme first; Gamma is the main one.
        let dragged = vec![Theme::builtin(), gamma.clone(), alpha.clone(), beta.clone()];
        let note = in_use_note(&gamma, &dragged);
        let mut loaded = vec![Theme::builtin(), beta.clone(), alpha.clone(), gamma.clone()];
        let (main, _) = settle(&mut loaded, &note);
        let names: Vec<&str> = loaded.iter().map(|theme| theme.name.as_str()).collect();
        assert_eq!(names[1..], ["Gamma", "Alpha", "Beta"]);
        assert_eq!(main, 1, "the main theme found again");
        // One the note doesn't know comes after them, by name; a note without an order is by name.
        let mut more = vec![Theme::builtin(), theme("Delta"), beta.clone(), alpha.clone(), gamma.clone()];
        settle(&mut more, &note);
        let names: Vec<&str> = more.iter().map(|theme| theme.name.as_str()).collect();
        assert_eq!(names[1..], ["Gamma", "Alpha", "Beta", "Delta"]);
        let mut plain = vec![Theme::builtin(), gamma, beta, alpha];
        settle(&mut plain, &format!("main={}\n", Uuid::nil()));
        let names: Vec<&str> = plain.iter().map(|theme| theme.name.as_str()).collect();
        assert_eq!(names[1..], ["Alpha", "Beta", "Gamma"]);
    }

    #[test]
    fn an_old_note_brings_the_bible_layout_into_the_main_theme_once() {
        // Songs used the built-in theme, saved with one template; Bible slides used the
        // built-in Bible theme, saved with the user's own layout.
        let mut main = Theme::builtin();
        main.templates.truncate(1);
        let mut bible = Theme::builtin_bible();
        bible.templates[0].boxes[0].size = 77.0;
        let mut themes = vec![main, bible.clone()];
        let note = format!(
            "songs={}\nbible={}\nbible_template={}\n",
            Uuid::nil(),
            sp_model::BIBLE_THEME_ID,
            bible.templates[0].id
        );
        let (at, changed) = settle(&mut themes, &note);
        assert_eq!((at, changed), (0, vec![0]));
        let brought = themes[0].bible_template().expect("the main theme has a Bible template");
        assert_eq!(brought.boxes, bible.templates[0].boxes, "looking as it did");
        assert!(!brought.songs, "songs don't use it");
        assert_eq!(themes[0].templates.len(), 2);
        // The new note names only the main theme, and nothing more is brought.
        let (at, changed) = settle(&mut themes, &format!("main={}\n", Uuid::nil()));
        assert_eq!((at, changed), (0, Vec::<usize>::new()));
        assert_eq!(themes[0].templates.len(), 2);
    }

    #[test]
    fn an_unsaved_built_in_theme_has_its_bible_template_replaced_not_doubled() {
        let mut bible = Theme::builtin_bible();
        bible.templates[0].boxes[0].size = 66.0;
        let mut themes = vec![Theme::builtin(), bible.clone()];
        let note = format!("songs={}\nbible={}\n", Uuid::nil(), sp_model::BIBLE_THEME_ID);
        settle(&mut themes, &note);
        assert_eq!(themes[0].templates.iter().filter(|t| t.bible).count(), 1);
        assert_eq!(themes[0].templates.len(), Theme::builtin().templates.len());
        assert_eq!(themes[0].bible_template().unwrap().boxes[0].size, 66.0);
    }
}
