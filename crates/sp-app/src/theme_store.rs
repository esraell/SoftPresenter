//! Where themes are kept on this computer: one file for each in the Themes folder, named by
//! the theme's id, and a note of which one is in use.
//!
//! Both are written by the library's writer (`writer`), as songs and settings are: flushed to
//! the disk and renamed into place, on its thread, never the window's (`whole_file::replace`). A
//! theme's file that can't be read at the start is left out, and never saved over that run
//! (`whole_file::refuses`): the built-in theme's own file, changed by the church, would
//! otherwise get the theme the app is built with saved over it at the next change.

use std::path::{Path, PathBuf};

use std::borrow::Cow;

use crate::i18n::{t, tf, tk};
use sp_model::{Theme, Uuid};

/// The words the window shows for the built-in theme's templates, in the order of
/// [`sp_model::BUILTIN_TEMPLATES`].
const BUILTIN_TEMPLATE_WORDS: [(&str, &str); 3] = [
    tk("themes.builtin.two_languages", "Two languages"),
    tk("themes.builtin.one_language", "One language"),
    tk("themes.builtin.bible", "Bible"),
];

/// What a template is called in the window. The built-in theme's three templates (and their
/// copies in themes made from it, which keep the ids) are called in the language set while they
/// keep the name the app gave them; their files keep that English name, so the theme reads in
/// whatever language the app is in. Any other template, or one a church renamed, is called by
/// its own name.
pub fn template_name(id: Uuid, name: &str) -> Cow<'_, str> {
    sp_model::BUILTIN_TEMPLATES
        .into_iter()
        .zip(BUILTIN_TEMPLATE_WORDS)
        .find(|((builtin, english), _)| *builtin == id && *english == name)
        .map_or(Cow::Borrowed(name), |(_, (key, words))| Cow::Borrowed(t(key, words)))
}

/// The name a copy of a template is saved under (a new version's templates): as the window
/// shows it, but not remembered for turning back into English (`i18n::tg`), as the copy's name
/// is saved and the command line must show it as it is.
pub fn given_name(template: &sp_model::ThemeTemplate) -> String {
    sp_model::BUILTIN_TEMPLATES
        .into_iter()
        .zip(BUILTIN_TEMPLATE_WORDS)
        .find(|((builtin, english), _)| *builtin == template.id && *english == template.name)
        .map_or_else(|| template.name.clone(), |(_, (key, words))| crate::i18n::tg(key, words).to_owned())
}

/// What a template is called in the window (see [`template_name`]).
pub fn shown_name(template: &sp_model::ThemeTemplate) -> Cow<'_, str> {
    template_name(template.id, &template.name)
}

/// The words the window shows for the built-in theme's name ([`sp_model::BUILTIN_THEME`]).
const BUILTIN_THEME_WORDS: (&str, &str) = tk("themes.builtin.theme", "Built-in");

/// What a theme is called in the window. The built-in theme is called in the language set while
/// it keeps the name the app gave it; its file keeps that English name, as its templates' do
/// ([`template_name`]). Any other theme, or the built-in one renamed, is called by its own name.
pub fn theme_name(id: Uuid, name: &str) -> Cow<'_, str> {
    if (id, name) == sp_model::BUILTIN_THEME {
        Cow::Borrowed(t(BUILTIN_THEME_WORDS.0, BUILTIN_THEME_WORDS.1))
    } else {
        Cow::Borrowed(name)
    }
}

/// What a theme is called in the window (see [`theme_name`]).
pub fn shown_theme_name(theme: &Theme) -> Cow<'_, str> {
    theme_name(theme.id, &theme.name)
}

/// The name the theme with this id is given when `typed` is typed for it: the built-in theme
/// typed as the window calls it keeps the name the app gave it, so a translation is never saved
/// as its name; anything else is the name typed.
pub fn typed_theme_name(id: Uuid, typed: &str) -> String {
    let (builtin, english) = sp_model::BUILTIN_THEME;
    if id == builtin && typed == t(BUILTIN_THEME_WORDS.0, BUILTIN_THEME_WORDS.1) {
        english.to_owned()
    } else {
        typed.to_owned()
    }
}

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
            Err(problem) => {
                crate::applog::problem("themes", &format!("{} could not be read: {problem}", path.display()));
                crate::whole_file::leave_unread(&path, false);
            }
        }
    }
    themes
}

/// A theme's save, for the library's writer to write (`writer`): its file in the Themes folder
/// (made if it isn't there), saved only as `guard` says (`outside`). The error is a plain
/// sentence.
pub fn to_save(theme: &Theme, guard: crate::writer::Guard) -> Result<crate::writer::Save, String> {
    let folder = folder().ok_or_else(|| {
        t("notices.save.no_place_themes", "Could not find where to keep themes on this computer.").to_owned()
    })?;
    Ok(to_save_in(&folder, theme, guard))
}

fn to_save_in(folder: &Path, theme: &Theme, guard: crate::writer::Guard) -> crate::writer::Save {
    let json = theme.to_json();
    let path = folder.join(format!("{}.sptheme", theme.id));
    let mut save = crate::writer::Save::new(path, crate::writer::Kind::Theme, guard, move || json.into_bytes());
    save.make_folder = true;
    save
}

/// The file a theme was saved in before the project was renamed (`.pctheme`), which its next
/// save replaces: taken away once that save is written (by a job handed to the writer after it).
pub fn old_file_of(theme: &Theme) -> Option<PathBuf> {
    folder().map(|folder| folder.join(format!("{}.pctheme", theme.id)))
}

/// What the window says when a theme couldn't be written.
pub fn not_saved(error: &std::io::Error) -> String {
    tf("notices.save.theme", "Could not save the theme: {error}", &[("error", &crate::notices::write_problem(error))])
}

/// Removes a theme's file. A theme that was never saved has none, which is not a failure.
pub fn delete(theme: &Theme) -> Result<(), String> {
    let Some(folder) = folder() else { return Ok(()) };
    // Its file, whichever ending it was saved with.
    for ending in ["sptheme", "pctheme"] {
        match std::fs::remove_file(folder.join(format!("{}.{ending}", theme.id))) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(tf(
                    "notices.delete.theme",
                    "Could not delete the theme: {error}",
                    &[("error", &e.to_string())],
                ));
            }
        }
    }
    Ok(())
}

/// The note of which theme is the main one, and the order of the list, for the library's writer
/// to write (`writer`), so they are as they were at the next start. `None` where there is no
/// place to keep it.
pub fn in_use_to_save(main: &Theme, themes: &[Theme]) -> Option<crate::writer::Save> {
    Some(in_use_to_save_at(in_use_file()?, main, themes))
}

fn in_use_to_save_at(file: PathBuf, main: &Theme, themes: &[Theme]) -> crate::writer::Save {
    let note = in_use_note(main, themes);
    let mut save =
        crate::writer::Save::new(file, crate::writer::Kind::ThemesInUse, crate::writer::Guard::Any, move || {
            note.into_bytes()
        });
    save.make_folder = true;
    save
}

/// What the window says when the note of the theme in use couldn't be written.
pub fn in_use_not_saved(error: &std::io::Error) -> String {
    tf(
        "notices.save.themes_in_use",
        "Could not save which theme is in use: {error}",
        &[("error", &crate::notices::write_problem(error))],
    )
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
    fn the_built_in_templates_are_shown_in_the_language_set_and_kept_in_english() {
        use crate::i18n::{Language, with_language};
        let builtin = Theme::builtin();
        let shown = |theme: &Theme| -> Vec<String> {
            theme.templates.iter().map(|template| shown_name(template).into_owned()).collect()
        };
        // In English as the theme names them: the window's words are sp-model's.
        assert_eq!(
            with_language(Language::English, || shown(&builtin)),
            builtin.templates.iter().map(|template| template.name.clone()).collect::<Vec<_>>()
        );
        assert_eq!(with_language(Language::Spanish, || shown(&builtin)), ["Dos idiomas", "Un idioma", "Biblia"]);
        // A theme made from the built-in one keeps the ids, and so the words.
        let copy = Theme { id: Uuid::new_v4(), name: "Evening".to_owned(), ..builtin.clone() };
        assert_eq!(with_language(Language::Spanish, || shown(&copy))[0], "Dos idiomas");
        // A template a church renamed is called as it was named, and another template with the
        // English name is not the built-in one.
        let mut renamed = builtin.clone();
        renamed.templates[0].name = "Sunday".to_owned();
        renamed.templates[1].id = Uuid::new_v4();
        assert_eq!(with_language(Language::Spanish, || shown(&renamed))[..2], ["Sunday", "One language"]);
        // Edited and saved in Spanish, the built-in theme's file keeps the English names, and
        // its boxes, made into a slide and back, are still called after their roles.
        let saved = with_language(Language::Spanish, || {
            let mut theme = builtin.clone();
            for template in &mut theme.templates {
                *template = template.with_slide(&template.as_slide(crate::editor::sample_words()));
            }
            Theme::from_json(&theme.to_json()).unwrap()
        });
        assert_eq!(
            saved.templates.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
            ["Two languages", "One language", "Bible"]
        );
        assert!(saved.all_templates().flat_map(|t| &t.boxes).all(|style| style.name.is_none()), "{saved:?}");
    }

    #[test]
    fn the_built_in_theme_is_called_in_the_language_set_and_keeps_its_english_name() {
        use crate::i18n::{Language, with_language};
        let builtin = Theme::builtin();
        // In English as the theme names it; in Spanish as the window says it.
        assert_eq!(with_language(Language::English, || shown_theme_name(&builtin).into_owned()), "Built-in");
        assert_eq!(with_language(Language::Spanish, || shown_theme_name(&builtin).into_owned()), "Incluido");
        // Renamed, it is called by its name; another theme of the same name is not the built-in.
        let renamed = Theme { name: "Domingo".to_owned(), ..builtin.clone() };
        assert_eq!(with_language(Language::Spanish, || shown_theme_name(&renamed).into_owned()), "Domingo");
        let other = Theme { id: Uuid::new_v4(), ..builtin.clone() };
        assert_eq!(with_language(Language::Spanish, || shown_theme_name(&other).into_owned()), "Built-in");
        // Typed back as the window says it, the built-in theme keeps the app's name for it (so
        // its file keeps "Built-in"), and a renamed one gets it back; any other theme, or other
        // words, take what was typed.
        with_language(Language::Spanish, || {
            assert_eq!(typed_theme_name(builtin.id, "Incluido"), "Built-in");
            assert_eq!(typed_theme_name(builtin.id, "Domingo"), "Domingo");
            assert_eq!(typed_theme_name(other.id, "Incluido"), "Incluido");
        });
        assert_eq!(with_language(Language::English, || typed_theme_name(builtin.id, "Built-in")), "Built-in");
        let saved = with_language(Language::Spanish, || {
            let mut theme = builtin.clone();
            theme.name = typed_theme_name(theme.id, &shown_theme_name(&theme));
            Theme::from_json(&theme.to_json()).unwrap()
        });
        assert_eq!(saved.name, "Built-in");
    }

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
        let any = crate::writer::Guard::Any;
        crate::whole_file::tests::written(to_save_in(&folder, &sunday, any));
        crate::whole_file::tests::written(to_save_in(&folder, &built_in, any));
        std::fs::write(folder.join("broken.pctheme"), "not a theme").unwrap();

        let loaded = load_from(&folder);
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0], built_in);
        assert_eq!(loaded[1], sunday);
        // The old built-in Bible theme, if it was saved, is a theme like any other.
        crate::whole_file::tests::written(to_save_in(&folder, &Theme::builtin_bible(), any));
        assert_eq!(load_from(&folder).len(), 3);
        let _ = std::fs::remove_dir_all(&folder);
    }

    /// A theme is written by the library's writer, whole: under a name of its own beside its
    /// file (`<id>.sptheme.part`, which the library tidy knows as half-written), then renamed
    /// over it, its folder made if need be; a save that fails leaves nothing half-written. Only
    /// over the file as the app last saw it, unless Keep mine.
    #[test]
    fn a_theme_is_saved_whole_by_the_library_writer_and_never_over_a_change_made_outside() {
        let folder = std::env::temp_dir().join(format!("softpresenter-test-theme-writer-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let mut theme = Theme { id: Uuid::new_v4(), name: "Evening".to_owned(), ..Theme::builtin() };
        let save = to_save_in(&folder, &theme, crate::writer::Guard::Unchanged(None));
        let file = save.path.clone();
        assert_eq!(file, folder.join(format!("{}.sptheme", theme.id)));
        crate::i18n::with_language(crate::i18n::Language::English, || {
            let said = crate::writer::Kind::Theme.problem(&file, &std::io::Error::other("x"));
            assert_eq!(said, "Could not save the theme: x");
        });
        crate::whole_file::tests::written(save);
        assert_eq!(load_from(&folder)[1], theme, "the folder made, the theme whole in it");
        assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 1, "nothing half-written beside it");
        // Changed outside the app since: the save is left undone.
        let seen = crate::outside::stamp(&file);
        std::fs::write(&file, theme.to_json().replace("Evening", "Changed outside")).unwrap();
        theme.name = "Night".to_owned();
        let writer = crate::writer::Writer::start(|| {});
        writer.write(to_save_in(&folder, &theme, crate::writer::Guard::Unchanged(seen)));
        writer.drain();
        assert!(
            matches!(writer.finished().as_slice(), [crate::writer::Done::ChangedOutside { path, .. }] if *path == file)
        );
        assert_eq!(load_from(&folder)[1].name, "Changed outside", "never saved over");
        // Keep mine goes over it.
        crate::whole_file::tests::written(to_save_in(&folder, &theme, crate::writer::Guard::Any));
        assert_eq!(load_from(&folder)[1].name, "Night");
        let _ = std::fs::remove_dir_all(&folder);
    }

    /// The built-in theme's own file, changed by the church, that can't be read at the start is
    /// left out (the theme the app is built with takes its place for the run), and no save goes
    /// over it until the app starts again.
    #[test]
    fn a_built_in_themes_file_that_cant_be_read_is_never_saved_over_that_run() {
        let folder = std::env::temp_dir().join(format!("softpresenter-test-theme-unread-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        let file = folder.join(format!("{}.sptheme", Uuid::nil()));
        let cut_short = &Theme::builtin().to_json()[..200];
        std::fs::write(&file, cut_short).unwrap();
        assert_eq!(load_from(&folder), vec![Theme::builtin()], "the theme the app is built with, for the run");
        let writer = crate::writer::Writer::start(|| {});
        writer.write(to_save_in(&folder, &Theme::builtin(), crate::writer::Guard::Any));
        writer.drain();
        assert!(matches!(writer.finished().as_slice(), [crate::writer::Done::NotRead { .. }]));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), cut_short, "left as it was");
        let _ = std::fs::remove_dir_all(&folder);
    }

    /// The note of the main theme and the order of the list is written by the library's writer,
    /// whole, its folder made if need be, and read back as it was left.
    #[test]
    fn the_note_of_the_theme_in_use_is_written_whole_by_the_library_writer() {
        let folder = std::env::temp_dir().join(format!("softpresenter-test-theme-note-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let (alpha, beta) = (
            Theme { id: Uuid::new_v4(), name: "Alpha".to_owned(), ..Theme::builtin() },
            Theme { id: Uuid::new_v4(), name: "Beta".to_owned(), ..Theme::builtin() },
        );
        let file = folder.join("themes-in-use.txt");
        crate::whole_file::tests::written(in_use_to_save_at(
            file.clone(),
            &alpha,
            &[Theme::builtin(), beta.clone(), alpha.clone()],
        ));
        let note = std::fs::read_to_string(&file).unwrap();
        assert_eq!(note, format!("main={}\norder={},{},{}\n", alpha.id, Uuid::nil(), beta.id, alpha.id));
        let mut loaded = vec![Theme::builtin(), alpha, beta];
        assert_eq!(settle(&mut loaded, &note).0, 2, "the main theme found again, in its place");
        assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 1, "nothing half-written beside it");
        crate::i18n::with_language(crate::i18n::Language::English, || {
            assert_eq!(
                crate::writer::Kind::ThemesInUse.problem(&file, &std::io::Error::other("x")),
                "Could not save which theme is in use: x"
            );
        });
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
