//! Command-line commands for the Bible page (search, translations shown, options, history,
//! installing and removing Bibles), themes (their templates and boxes, the main theme, copying,
//! renaming, deleting) and songs' arrangements. Each changes what the window changes, through
//! its requests where it has them, and says what it did.

use sp_model::{Arrangement, Role, Uuid};

use crate::control::{Reply, find, numbered};

fn on_off(value: &str) -> Result<bool, String> {
    match value.to_lowercase().as_str() {
        "on" | "yes" | "true" => Ok(true),
        "off" | "no" | "false" => Ok(false),
        other => Err(format!("Say on or off, not {other}.")),
    }
}

/// The lines of `bibles` for the Bibles to download (`offered`, as [`crate::bible::offered_in_order`]
/// orders them, each marked true in one of the person's languages): under "Your languages:" and
/// "Other languages:" when some are in the person's languages, as the Bibles window heads them;
/// as one list when none are. Nothing when there is none to download.
fn offered_lines(offered: &[(bool, &crate::bible::Offered)], busy: impl Fn(&str) -> bool) -> Vec<String> {
    if offered.is_empty() {
        return Vec::new();
    }
    let parted = offered.iter().any(|(yours, _)| *yours);
    let indent = if parted { "    " } else { "  " };
    let mut lines = vec!["To download (bible install NAME):".to_owned()];
    let mut part = None;
    for &(yours, bible) in offered {
        if parted && part != Some(yours) {
            part = Some(yours);
            lines.push(if yours { "  Your languages:" } else { "  Other languages:" }.to_owned());
        }
        let downloading = if busy(bible.id) { ", downloading" } else { "" };
        lines.push(format!("{indent}{} ({}, {}){downloading}", bible.name, bible.short, bible.language));
    }
    lines
}

impl crate::App {
    /// `bible text|search|rows|options|history|install|remove …` and `bibles`; `None` for a
    /// passage to look up.
    pub(crate) fn ctl_bible_more(&mut self, args: &[String]) -> Option<Result<Reply, String>> {
        let word = args.get(1)?.to_lowercase();
        let words = &args[2..];
        let result = match word.as_str() {
            "text" => self.bible_text(&words.join(" ")),
            "search" => self.bible_search(&words.join(" ")),
            "rows" => self.bible_rows(words),
            "options" => self.bible_options(words),
            "history" => self.bible_history(),
            "install" => self.bible_install(words),
            "remove" => self.bible_remove(words),
            "view" | "books" | "language" => self.bible_look(&word, words.first().map(String::as_str)),
            _ => return None,
        };
        Some(result)
    }

    /// `bible view [page|slides]`, `bible books [keypad|lists]`, `bible language
    /// [auto|en|ru|uk]`: how the Bible page shows a passage, how it offers the books, and the
    /// language the operator reads it in, as its switches set them (kept with the preferences).
    /// Without a value, says which. None of them changes the slides on screen.
    fn bible_look(&mut self, what: &str, value: Option<&str>) -> Result<Reply, String> {
        use crate::home::{BibleView, BookPicker};
        let state = &mut self.running.as_mut().ok_or("The app isn't ready.")?.home_state;
        let page = &mut state.bible;
        let value = value.map(str::to_lowercase);
        match (what, value.as_deref()) {
            ("view", Some("page")) => page.view = BibleView::Page,
            ("view", Some("slides")) => page.view = BibleView::Slides,
            ("books", Some("keypad" | "keys")) => page.books = BookPicker::Keypad,
            ("books", Some("lists" | "list")) => page.books = BookPicker::Lists,
            ("language", Some(said)) => {
                let chosen = match said {
                    "auto" => None,
                    "en" | "english" => Some("en"),
                    "ru" | "russian" | "русский" => Some("ru"),
                    "uk" | "ukrainian" | "українська" => Some("uk"),
                    other => return Err(format!("The language is auto, en, ru or uk, not {other}.")),
                };
                if let Some(language) = chosen
                    && !page.languages_in_use().iter().any(|have| have == language)
                {
                    let name = crate::bible::language_name(language);
                    return Err(format!("No Bible in use is in {name}: add one to the translations first."));
                }
                page.set_read_in(chosen.map(str::to_owned));
            }
            ("view", Some(other)) => return Err(format!("The view is page or slides, not {other}.")),
            ("books", Some(other)) => return Err(format!("The books are keypad or lists, not {other}.")),
            _ => {}
        }
        let said = match what {
            "view" => match page.view {
                BibleView::Page => "The Bible page shows the chapter as a page.".to_owned(),
                BibleView::Slides => "The Bible page shows the slides, the passage's among its chapter's.".to_owned(),
            },
            "books" => match page.books {
                BookPicker::Keypad => "The books are a keypad.".to_owned(),
                BookPicker::Lists => "The books are lists.".to_owned(),
            },
            _ => {
                let in_use = page.languages_in_use();
                match page.read_in.as_deref().filter(|language| in_use.iter().any(|have| have == language)) {
                    Some(language) => format!("The Bible page reads in {}.", crate::bible::language_name(language)),
                    None => format!(
                        "The Bible page reads in the language typed (Auto: {} now).",
                        crate::bible::language_name(&page.auto_language())
                    ),
                }
            }
        };
        Ok(Reply::done(said))
    }

    /// `bible text REFERENCE`: a passage's verses, made with the Bible page's translations and
    /// options as looking it up would make them, but leaving the page as it is (what it shows,
    /// what is typed, the view the window is in). Each slide's words, the translations' parted
    /// by " / " as `slides` parts them.
    fn bible_text(&mut self, typed: &str) -> Result<Reply, String> {
        let typed = typed.trim();
        if typed.is_empty() {
            return Err("bible text REFERENCE   (as typed on the Bible page: John 3:16-18, Psalm 23)".to_owned());
        }
        if let Some(problem) = self.bible_reference_problem(typed) {
            return Err(problem);
        }
        let state = &mut self.running.as_mut().ok_or("The app isn't ready.")?.home_state;
        let (passage, saved) = state.bible.passage_typed(typed)?;
        let mut lines = vec![saved.lookup.clone()];
        let mut slides = Vec::new();
        for (index, (group, slide)) in passage.slides().enumerate() {
            let words =
                slide.texts().into_iter().map(|(_, text)| text.replace('\n', " / ")).collect::<Vec<_>>().join(" / ");
            let label = if group.name.is_empty() { String::new() } else { format!("[{}] ", group.name) };
            lines.push(format!("{:>3}. {label}{words}", index + 1));
            slides.push(serde_json::json!({"slide": index + 1, "group": group.name, "words": words}));
        }
        let bibles: Vec<String> = saved.rows.iter().map(|row| row.bible.clone()).collect();
        let data = serde_json::json!({"passage": saved.lookup, "bibles": bibles, "slides": slides});
        Ok(Reply::with(lines.join("\n"), data))
    }

    /// `bible search WORDS`: verses holding the words, found as the page's Search finds them,
    /// leaving the page's own search as it is.
    fn bible_search(&mut self, words: &str) -> Result<Reply, String> {
        if words.trim().is_empty() {
            return Err("bible search WORDS".to_owned());
        }
        let state = &mut self.running.as_mut().ok_or("The app isn't ready.")?.home_state;
        let found = state.bible.verses_with(words.trim())?;
        // Found in more than one Bible: each line says which.
        let found = &found;
        let mixed = found.iter().any(|verse| verse.bible != found[0].bible);
        let lines: Vec<String> = found
            .iter()
            .map(|verse| match mixed {
                true => format!("{} ({}): {}", verse.reference, verse.bible, verse.words),
                false => format!("{}: {}", verse.reference, verse.words),
            })
            .collect();
        let data: Vec<serde_json::Value> = found
            .iter()
            .map(|verse| {
                serde_json::json!({"verse": verse.reference, "chapter": verse.chapter, "words": verse.words, "bible": verse.bible})
            })
            .collect();
        Ok(Reply::with(numbered(&lines), serde_json::json!(data)))
    }

    /// `bible rows BIBLE…`: the translations shown, first to last (the first in the main box with
    /// its reference, the second in the translation box), as choosing them on the page does.
    fn bible_rows(&mut self, words: &[String]) -> Result<Reply, String> {
        let state = &mut self.running.as_mut().ok_or("The app isn't ready.")?.home_state;
        let shelf: Vec<String> = state.bible.shelf.bibles.iter().map(|b| b.short.clone()).collect();
        if words.is_empty() {
            let lines: Vec<String> = state
                .bible
                .rows
                .iter()
                .map(|row| {
                    let name = row.bible.and_then(|at| shelf.get(at)).cloned().unwrap_or_else(|| "(none)".to_owned());
                    let into =
                        row.verse.as_ref().map_or("not shown".to_owned(), |role| format!("words in {}", role.label()));
                    format!("{name}: {into}")
                })
                .collect();
            return Ok(Reply::with(numbered(&lines), serde_json::json!(lines)));
        }
        let names: Vec<String> = state.bible.shelf.bibles.iter().map(|b| b.name.clone()).collect();
        let mut chosen = Vec::new();
        for typed in words {
            // By the short name (KJV) or the whole name.
            let at = shelf
                .iter()
                .position(|short| short.eq_ignore_ascii_case(typed))
                .map_or_else(|| find("Bible", &names, typed), Ok)?;
            chosen.push(at);
        }
        state.bible.rows =
            chosen.iter().map(|at| crate::home::BibleRow { bible: Some(*at), verse: None, reference: None }).collect();
        state.bible.assign_boxes();
        let said: Vec<&str> = chosen.iter().map(|at| shelf[*at].as_str()).collect();
        Ok(Reply::done(format!("The Bible page shows {}.", said.join(", then "))))
    }

    /// `bible options key=value…`: verses=on|off, break=on|off, translation=on|off,
    /// reference=verse|passage|last|none.
    fn bible_options(&mut self, words: &[String]) -> Result<Reply, String> {
        let state = &mut self.running.as_mut().ok_or("The app isn't ready.")?.home_state;
        let options = &mut state.bible.options;
        let describe = |o: &crate::bible::BibleOptions| {
            let reference = match o.reference {
                crate::bible::ReferenceStyle::VerseReferences => "each slide names its verses",
                crate::bible::ReferenceStyle::PassageEach => "each slide names the passage",
                crate::bible::ReferenceStyle::PassageLast => "the last slide names the passage",
                crate::bible::ReferenceStyle::NoReference => "no reference",
            };
            format!(
                "verse numbers {}, a new slide for each verse {}, the translation's name {}, {reference}",
                if o.show_verse_numbers { "on" } else { "off" },
                if o.break_on_new_verse { "on" } else { "off" },
                if o.display_translation { "on" } else { "off" },
            )
        };
        if words.is_empty() {
            return Ok(Reply::done(describe(options)));
        }
        for setting in words {
            let (key, value) = setting.split_once('=').ok_or_else(|| format!("{setting} isn't key=value."))?;
            let switch = |value: &str| on_off(value).map_err(|_| format!("{key} is on or off, not {value}."));
            match key.to_lowercase().as_str() {
                "verses" | "numbers" | "verse_numbers" => options.show_verse_numbers = switch(value)?,
                "break" | "break_on_new_verse" => options.break_on_new_verse = switch(value)?,
                "translation" | "name" | "display_translation" => options.display_translation = switch(value)?,
                "reference" => {
                    options.reference = match value.to_lowercase().as_str() {
                        "verse" | "verses" => crate::bible::ReferenceStyle::VerseReferences,
                        "passage" | "each" => crate::bible::ReferenceStyle::PassageEach,
                        "last" => crate::bible::ReferenceStyle::PassageLast,
                        "none" | "off" => crate::bible::ReferenceStyle::NoReference,
                        other => return Err(format!("reference is verse, passage, last or none, not {other}.")),
                    }
                }
                other => return Err(format!("{other} isn't a Bible option: verses, break, translation, reference.")),
            }
        }
        state.bible.made_with.clear();
        Ok(Reply::done(format!("Bible options: {}.", describe(&state.bible.options))))
    }

    fn bible_history(&self) -> Result<Reply, String> {
        let state = &self.running.as_ref().ok_or("The app isn't ready.")?.home_state;
        let lines: Vec<String> =
            state.bible.history.iter().map(|entry| format!("{} ({})", entry.passage, entry.at)).collect();
        let data: Vec<serde_json::Value> = state
            .bible
            .history
            .iter()
            .map(|entry| serde_json::json!({"passage": entry.passage, "at": entry.at}))
            .collect();
        Ok(Reply::with(numbered(&lines), serde_json::json!(data)))
    }

    /// `bibles`: the Bibles installed, and those that can be downloaded, the person's languages
    /// first, as the Bibles window lists them.
    pub(crate) fn ctl_bibles(&self) -> Result<Reply, String> {
        let state = &self.running.as_ref().ok_or("The app isn't ready.")?.home_state;
        let shelf = &state.bible.shelf;
        let mut lines = vec!["Installed:".to_owned()];
        for bible in &shelf.bibles {
            lines.push(format!(
                "  {} ({}, {}){}",
                bible.name,
                bible.short,
                bible.language,
                if bible.shipped { ", comes with the app" } else { "" }
            ));
        }
        let yours = crate::bible::your_languages_now(shelf);
        let offered: Vec<(bool, &crate::bible::Offered)> = crate::bible::offered_in_order(&yours)
            .into_iter()
            .filter(|(_, b)| shelf.position(b.id).is_none())
            .collect();
        lines.extend(offered_lines(&offered, |id| state.bible.downloads.is_busy(id)));
        let data = serde_json::json!({
            "installed": shelf.bibles.iter().map(|b| serde_json::json!({"name": b.name, "short": b.short, "language": b.language, "shipped": b.shipped})).collect::<Vec<_>>(),
            "your_languages": yours,
            "offered": offered.iter().map(|(mine, b)| serde_json::json!({"name": b.name, "short": b.short, "language": b.language, "yours": mine, "downloading": state.bible.downloads.is_busy(b.id)})).collect::<Vec<_>>(),
        });
        Ok(Reply::with(lines.join("\n"), data))
    }

    /// `bible install NAME|FILE [--name NAME] [--short SHORT] [--language CODE]`: a Bible
    /// offered, downloaded in the background, or a Bible file (the app's own, or another
    /// program's: a file, a zip or a folder of books). The flags give what the file doesn't say
    /// (or say otherwise); one that says too little is refused with the flags it needs, as no
    /// sheet can ask.
    fn bible_install(&mut self, words: &[String]) -> Result<Reply, String> {
        let mut given = sp_bible::import::About::default();
        let mut rest: Vec<&str> = Vec::new();
        let mut at = 0;
        while at < words.len() {
            let field = match words[at].as_str() {
                "--name" => Some(&mut given.name),
                "--short" => Some(&mut given.short),
                "--language" => Some(&mut given.language),
                _ => None,
            };
            match field {
                Some(field) => {
                    let value = words.get(at + 1).ok_or_else(|| format!("{} needs a value after it.", words[at]))?;
                    value.clone_into(field);
                    at += 2;
                }
                None => {
                    rest.push(&words[at]);
                    at += 1;
                }
            }
        }
        let typed = rest.join(" ");
        let typed = typed.trim();
        if typed.is_empty() {
            return Err(
                "bible install NAME | FILE [--name NAME --short SHORT --language CODE]   (bibles lists those offered)"
                    .to_owned(),
            );
        }
        let path = std::path::Path::new(typed);
        // A Bible offered, named exactly, is meant even if a file of that name happens to be
        // there; any other path that is there is read by what it holds, whatever its ending.
        let offered_name = crate::bible::offered_named(typed).is_some();
        // From a remote page only a Bible offered is installed: the disk is never looked at.
        if !self.outside_call() && path.exists() && !offered_name {
            self.running.as_ref().ok_or("The app isn't ready.")?;
            // Read on a thread of its own, as a dropped file is. The command line waits for the
            // job (the reply asks it to), so it still answers with what came in.
            let job = self.start_job();
            let work = crate::bible_store::Work::Open { path: path.to_owned(), given, ask: false };
            let said = self.start_bible_work(work, Some(job));
            return Ok(Reply::with(said, serde_json::json!({"wait": true})));
        }
        let ctx = self.running.as_ref().ok_or("The app isn't ready.")?.egui_ctx.clone();
        let state = &mut self.running.as_mut().ok_or("The app isn't ready.")?.home_state;
        let offered: Vec<&crate::bible::Offered> = crate::bible::OFFERED.iter().collect();
        let names: Vec<String> = offered.iter().map(|b| format!("{} ({})", b.name, b.short)).collect();
        let at = crate::bible::offered_named(typed)
            .and_then(|named| offered.iter().position(|b| b.id == named.id))
            .map_or_else(|| find("Bible offered", &names, typed), Ok)
            .map_err(|problem| format!("{problem} Those offered: {}.", names.join(", ")))?;
        let bible = offered[at];
        if state.bible.shelf.position(bible.id).is_some() {
            return Err(format!("The {} is installed already.", bible.name));
        }
        if state.bible.downloads.is_busy(bible.id) {
            return Ok(Reply::done(format!("The {} is downloading already.", bible.name)));
        }
        state.bible.downloads.start(bible.id, bible.checksum, move || ctx.request_repaint());
        // A job number, so `--wait` and `job N` cover it as they cover song packs.
        let job = self.start_job();
        self.bible_jobs.push((job, bible.id.to_owned(), bible.name.to_owned()));
        Ok(Reply::done(format!("Downloading the {}; bibles lists it once it is installed.", bible.name)))
    }

    fn bible_remove(&mut self, words: &[String]) -> Result<Reply, String> {
        let yes = words.iter().any(|w| w == "--yes");
        let typed = words.iter().filter(|w| *w != "--yes").cloned().collect::<Vec<_>>().join(" ");
        let state = &mut self.running.as_mut().ok_or("The app isn't ready.")?.home_state;
        let shelf = &state.bible.shelf;
        let names: Vec<String> = shelf.bibles.iter().map(|b| b.name.clone()).collect();
        let at = shelf
            .bibles
            .iter()
            .position(|b| b.short.eq_ignore_ascii_case(&typed))
            .map_or_else(|| find("Bible", &names, &typed), Ok)?;
        if shelf.bibles[at].shipped {
            return Err(format!("The {} comes with the app and stays.", names[at]));
        }
        if !yes {
            return Err(format!("This removes the {} (its file goes to the Trash). Add --yes to do it.", names[at]));
        }
        state.bible.options.problem = None;
        state.bible.remove_bible(at);
        if let Some(problem) = state.bible.options.problem.take() {
            return Err(problem);
        }
        Ok(Reply::done(format!("Removed the {}; its file is in the Trash.", names[at])))
    }

    /// `theme templates|main|duplicate|rename|delete|box|template …`.
    pub(crate) fn ctl_theme(&mut self, args: &[String]) -> Result<Reply, String> {
        let usage = "theme templates THEME | theme main THEME | theme duplicate THEME [as NAME] | theme rename THEME to NAME | \
                     theme delete THEME --yes | theme box THEME TEMPLATE BOX key=value… | theme template THEME TEMPLATE for=songs|bible|both|none";
        let word = args.get(1).map(|w| w.to_lowercase()).unwrap_or_default();
        let words = &args[2..];
        let names: Vec<String> = self.themes.iter().map(|t| t.name.clone()).collect();
        let theme_at = |typed: &str| find("theme", &names, typed);
        match word.as_str() {
            "templates" => {
                let at = theme_at(&words.join(" "))?;
                let theme = &self.themes[at];
                let mut lines = vec![theme.name.clone()];
                let mut data = Vec::new();
                for (number, template) in theme.templates.iter().enumerate() {
                    let used = match (template.songs, template.bible) {
                        (true, true) => "songs and Bible",
                        (true, false) => "songs",
                        (false, true) => "Bible",
                        (false, false) => "neither",
                    };
                    lines.push(format!("  {}. {} (for {used})", number + 1, template.name));
                    let mut boxes = Vec::new();
                    for (n, look) in template.boxes.iter().enumerate() {
                        let name = look.name.clone().unwrap_or_else(|| look.role.label());
                        lines.push(format!(
                            "       box {}: {name} — {} {}pt, at x={:.0} y={:.0} w={:.0} h={:.0}",
                            n + 1,
                            look.font_family,
                            look.size,
                            look.rect.x,
                            look.rect.y,
                            look.rect.w,
                            look.rect.h
                        ));
                        boxes.push(serde_json::json!({"name": name, "role": look.role.label(), "font": look.font_family, "size": look.size, "frame": look.rect}));
                    }
                    data.push(serde_json::json!({"name": template.name, "songs": template.songs, "bible": template.bible, "boxes": boxes}));
                }
                Ok(Reply::with(
                    lines.join("\n"),
                    serde_json::json!({"theme": theme.name, "main": at == self.songs_theme, "templates": data}),
                ))
            }
            "main" => {
                let at = theme_at(&words.join(" "))?;
                if at == self.songs_theme {
                    return Ok(Reply::done(format!("{} is the main theme already.", names[at])));
                }
                self.with_state(|state| state.theme_request = Some(crate::home::ThemeRequest::UseForSongs(at)));
                self.handle_themes();
                Ok(Reply::done(format!("{} is the main theme: songs that don't set their own use it.", names[at])))
            }
            "duplicate" => {
                let (from, as_name) = match words.iter().rposition(|w| w.eq_ignore_ascii_case("as")) {
                    Some(i) if i + 1 < words.len() => (words[..i].join(" "), Some(words[i + 1..].join(" "))),
                    _ => (words.join(" "), None),
                };
                let at = theme_at(&from)?;
                let mut copy = self.themes[at].clone();
                copy.id = Uuid::new_v4();
                let base = as_name.unwrap_or_else(|| format!("{} copy", copy.name));
                copy.name = base.clone();
                let mut n = 2;
                while self.themes.iter().any(|t| t.name == copy.name) {
                    copy.name = format!("{base} {n}");
                    n += 1;
                }
                let name = copy.name.clone();
                self.themes.push(copy);
                // Written by the library's writer; one that can't be is said in the notices.
                self.write_theme(self.themes.len() - 1, false);
                self.themes_changed();
                Ok(Reply::done(format!("Copied {} as {name}.", names[at])))
            }
            "rename" => {
                let (from, to) = match words.iter().rposition(|w| w.eq_ignore_ascii_case("to")) {
                    Some(i) if i + 1 < words.len() => (words[..i].join(" "), words[i + 1..].join(" ")),
                    _ => return Err("theme rename THEME to NAME".to_owned()),
                };
                let at = theme_at(&from)?;
                if self.themes.iter().enumerate().any(|(i, t)| i != at && t.name.eq_ignore_ascii_case(to.trim())) {
                    return Err(format!("There is a theme called {} already.", to.trim()));
                }
                self.themes[at].name = to.trim().to_owned();
                self.theme_saved_and_shown(at);
                Ok(Reply::done(format!("Renamed the theme {} to {}.", names[at], to.trim())))
            }
            "delete" => {
                let yes = words.iter().any(|w| w == "--yes");
                let at = theme_at(&words.iter().filter(|w| *w != "--yes").cloned().collect::<Vec<_>>().join(" "))?;
                if self.themes[at].id.is_nil() {
                    return Err("The Built-in theme stays; it can be changed but not deleted.".to_owned());
                }
                if at == self.songs_theme {
                    return Err(format!("{} is the main theme; make another theme the main one first.", names[at]));
                }
                if !yes {
                    return Err(format!(
                        "This deletes the theme {} for good; songs that use it go back to the main theme. Add --yes to do it.",
                        names[at]
                    ));
                }
                self.with_state(|state| state.theme_request = Some(crate::home::ThemeRequest::Delete(at)));
                self.handle_themes();
                Ok(Reply::done(format!("Deleted the theme {}.", names[at])))
            }
            "box" => self.theme_box(words),
            "template" => self.theme_template(words),
            _ => Err(usage.to_owned()),
        }
    }

    /// After a theme was changed here: saved (unless its file changed outside the app), and
    /// everything drawn with it brought up to date.
    fn theme_saved_and_shown(&mut self, at: usize) {
        if !self.themes_unsaved.contains(&at) {
            self.themes_unsaved.push(at);
        }
        self.themes_changed();
        self.refresh_output();
        self.save_unsaved();
    }

    /// The template of a theme a name (or number) stands for.
    fn template_of(&self, theme: usize, typed: &str) -> Result<usize, String> {
        let names: Vec<String> = self.themes[theme].templates.iter().map(|t| t.name.clone()).collect();
        find("template", &names, typed)
            .map_err(|problem| format!("{problem} {} has: {}.", self.themes[theme].name, names.join(", ")))
    }

    /// `theme box THEME TEMPLATE BOX key=value…`: a template box's look (the `slide style`
    /// keys) and place (x= y= w= h=). Theme, template and box are each one word, or quoted.
    fn theme_box(&mut self, words: &[String]) -> Result<Reply, String> {
        let usage =
            "theme box THEME TEMPLATE BOX key=value…   (quote names with spaces; keys as slide style, and x= y= w= h=)";
        let [theme, template, which, settings @ ..] = words else { return Err(usage.to_owned()) };
        if settings.is_empty() {
            return Err(usage.to_owned());
        }
        let names: Vec<String> = self.themes.iter().map(|t| t.name.clone()).collect();
        let at = find("theme", &names, theme)?;
        let t = self.template_of(at, template)?;
        let boxes = &self.themes[at].templates[t].boxes;
        let labels: Vec<String> = boxes.iter().map(|b| b.name.clone().unwrap_or_else(|| b.role.label())).collect();
        let b = labels
            .iter()
            .position(|label| label.eq_ignore_ascii_case(which))
            .or_else(|| boxes.iter().position(|look| look.role == Role(which.to_lowercase())))
            .map_or_else(|| find("box", &labels, which), Ok)
            .map_err(|problem| format!("{problem} The template's boxes: {}.", labels.join(", ")))?;
        let (place, look): (Vec<String>, Vec<String>) = settings.iter().cloned().partition(|s| {
            matches!(
                s.split('=').next().unwrap_or("").to_lowercase().as_str(),
                "x" | "y" | "w" | "h" | "width" | "height"
            )
        });
        let mut changed = self.themes[at].templates[t].boxes[b].clone();
        let mut said = Vec::new();
        if !look.is_empty() {
            let mut style = sp_model::TextStyle::default();
            said = crate::slide_cli::apply_settings(&mut style, &look)?;
            let rect = changed.rect;
            changed = style.over(&changed);
            changed.rect = rect;
        }
        if !place.is_empty() {
            changed.rect = crate::slide_cli::set_frame(changed.rect, &place)?;
            said.push(format!(
                "at x={:.0} y={:.0} w={:.0} h={:.0}",
                changed.rect.x, changed.rect.y, changed.rect.w, changed.rect.h
            ));
        }
        self.themes[at].templates[t].boxes[b] = changed;
        self.theme_saved_and_shown(at);
        let template_name = self.themes[at].templates[t].name.clone();
        Ok(Reply::done(format!("{}, {template_name}, {}: {}.", names[at], labels[b], said.join(", "))))
    }

    /// `theme template THEME TEMPLATE for=songs|bible|both|none`: what a template is for.
    fn theme_template(&mut self, words: &[String]) -> Result<Reply, String> {
        let usage = "theme template THEME TEMPLATE for=songs|bible|both|none";
        let [theme, template, setting] = words else { return Err(usage.to_owned()) };
        let names: Vec<String> = self.themes.iter().map(|t| t.name.clone()).collect();
        let at = find("theme", &names, theme)?;
        let t = self.template_of(at, template)?;
        let wanted = setting.strip_prefix("for=").ok_or(usage)?.to_lowercase();
        let (songs, bible) = match wanted.as_str() {
            "songs" => (true, false),
            "bible" => (false, true),
            "both" => (true, true),
            "none" | "neither" => (false, false),
            _ => return Err(usage.to_owned()),
        };
        let theme_ref = &mut self.themes[at];
        theme_ref.templates[t].songs = songs;
        if bible {
            // One template of a theme is the one Bible slides use.
            theme_ref.set_bible_template(t);
        } else {
            theme_ref.templates[t].bible = false;
        }
        let name = theme_ref.templates[t].name.clone();
        self.theme_saved_and_shown(at);
        Ok(Reply::done(format!("{}, {name}: for {wanted}.", names[at])))
    }

    /// `song arrangements SONG`, `song arrangement SONG use NAME|none`, `song arrangement SONG
    /// set NAME GROUP…`, `song arrangement SONG delete NAME`, `song arrangement SONG
    /// chorus-after`.
    pub(crate) fn ctl_song_arrangement(&mut self, args: &[String]) -> Option<Result<Reply, String>> {
        let word = args.get(1)?.to_lowercase();
        let result = match word.as_str() {
            "arrangements" => self.song_arrangements(&args[2..].join(" ")),
            "arrangement" => self.song_arrangement(&args[2..]),
            _ => return None,
        };
        Some(result)
    }

    fn song_arrangements(&mut self, song: &str) -> Result<Reply, String> {
        let at = self.find_song(song)?;
        let presentation = self.show.presentations()[at].clone();
        let group_name =
            |id: &Uuid| presentation.groups.iter().find(|g| g.id == *id).map_or("?", |g| g.name.as_str()).to_owned();
        let mut lines = vec![format!(
            "{}: groups {}",
            presentation.title,
            presentation.groups.iter().map(|g| g.name.as_str()).collect::<Vec<_>>().join(", ")
        )];
        let mut data = Vec::new();
        for arrangement in &presentation.arrangements {
            let used = presentation.arrangement == Some(arrangement.id);
            let order: Vec<String> = arrangement.groups.iter().map(group_name).collect();
            // Made to sing the chorus after each verse, and changed by nobody since.
            let marks: Vec<&str> = [used.then_some("in use"), arrangement.auto.then_some("follows the song's parts")]
                .into_iter()
                .flatten()
                .collect();
            let marks = if marks.is_empty() { String::new() } else { format!(" ({})", marks.join(", ")) };
            lines.push(format!("  {}{marks}: {}", arrangement.name, order.join(", ")));
            data.push(serde_json::json!({
                "name": arrangement.name,
                "in_use": used,
                "auto": arrangement.auto,
                "groups": order,
            }));
        }
        if presentation.arrangements.is_empty() {
            lines.push("  no arrangements: the groups play in order".to_owned());
        }
        Ok(Reply::with(lines.join("\n"), serde_json::json!(data)))
    }

    fn song_arrangement(&mut self, words: &[String]) -> Result<Reply, String> {
        let usage = "song arrangement SONG use NAME|none | song arrangement SONG set NAME GROUP… | song arrangement SONG delete NAME | song arrangement SONG chorus-after";
        let [song, verb, rest @ ..] = words else { return Err(usage.to_owned()) };
        let at = self.find_song(song)?;
        let presentation = self.show.presentations()[at].clone();
        let mut changed = presentation.as_ref().clone();
        let names: Vec<String> = changed.arrangements.iter().map(|a| a.name.clone()).collect();
        let said = match verb.to_lowercase().as_str() {
            "use" => {
                let typed = rest.join(" ");
                if typed.eq_ignore_ascii_case("none") {
                    changed.arrangement = None;
                    "plays its groups in order".to_owned()
                } else {
                    let which = find("arrangement", &names, &typed)?;
                    changed.arrangement = Some(changed.arrangements[which].id);
                    format!("uses the arrangement {}", names[which])
                }
            }
            "set" => {
                let [name, groups @ ..] = rest else { return Err(usage.to_owned()) };
                if groups.is_empty() {
                    return Err(usage.to_owned());
                }
                let group_names: Vec<String> = changed.groups.iter().map(|g| g.name.clone()).collect();
                let mut order = Vec::new();
                for typed in groups {
                    // A name several groups have (a song with two Chorus groups) is the first of them.
                    let g = group_names
                        .iter()
                        .position(|name| name.eq_ignore_ascii_case(typed))
                        .map_or_else(|| find("group", &group_names, typed), Ok)
                        .map_err(|problem| {
                            format!("{problem} {}'s groups: {}.", changed.title, group_names.join(", "))
                        })?;
                    order.push(changed.groups[g].id);
                }
                match changed.arrangements.iter_mut().find(|a| a.name.eq_ignore_ascii_case(name)) {
                    // Set by hand, it no longer follows the song's parts.
                    Some(have) => {
                        have.groups = order;
                        have.auto = false;
                    }
                    None => {
                        let made = Arrangement { id: Uuid::new_v4(), name: name.clone(), groups: order, auto: false };
                        changed.arrangement = Some(made.id);
                        changed.arrangements.push(made);
                    }
                }
                format!("arrangement {name}: {}", groups.join(", "))
            }
            "delete" => {
                let which = find("arrangement", &names, &rest.join(" "))?;
                let gone = changed.arrangements.remove(which);
                if changed.arrangement == Some(gone.id) {
                    changed.arrangement = None;
                }
                format!("deleted the arrangement {}", gone.name)
            }
            // Chorus After Each Verse in the window's arrangement menu.
            "chorus-after" if rest.is_empty() => {
                if !changed.sing_chorus_after_each_verse(&crate::home::chorus_after_name()) {
                    return Err(format!(
                        "{} has no verse and chorus to sing that way (its groups: {}).",
                        presentation.title,
                        presentation.groups.iter().map(|g| g.name.as_str()).collect::<Vec<_>>().join(", ")
                    ));
                }
                let made = changed.active_arrangement().ok_or("The arrangement could not be made.")?;
                let group_name =
                    |id: &Uuid| changed.groups.iter().find(|g| g.id == *id).map_or("?", |g| g.name.as_str());
                let order: Vec<&str> = made.groups.iter().map(group_name).collect();
                format!("sings the chorus after each verse, in the arrangement {}: {}", made.name, order.join(", "))
            }
            _ => return Err(usage.to_owned()),
        };
        self.change_song(at, changed);
        Ok(Reply::done(format!("{}: {said}.", presentation.title)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bibles_lists_the_persons_languages_first_under_their_headings() {
        let offered = crate::bible::offered_in_order(&["ru", "uk"]);
        let lines = offered_lines(&offered, |id| id == "ukronpu");
        assert_eq!(lines[0], "To download (bible install NAME):");
        assert_eq!(lines[1], "  Your languages:");
        assert!(lines[2].starts_with("    ") && lines[2].ends_with(", Russian)"), "{}", lines[2]);
        let others = lines.iter().position(|line| line == "  Other languages:").expect("Other languages:");
        assert!(lines[2..others].iter().all(|line| line.contains(", Russian)") || line.contains(", Ukrainian)")));
        assert!(lines[2..others].iter().any(|line| line.ends_with(", Ukrainian), downloading")));
        assert!(lines[others + 1..].iter().all(|line| line.starts_with("    ") && !line.contains(", Russian)")));
        assert_eq!(lines.len(), crate::bible::OFFERED.len() + 3);
        // None in the person's languages: one list, as before, with no headings.
        let lines = offered_lines(&crate::bible::offered_in_order(&[]), |_| false);
        assert_eq!(lines.len(), crate::bible::OFFERED.len() + 1);
        assert!(lines[1..].iter().all(|line| line.starts_with("  ") && !line.starts_with("   ")));
        // Nothing to download: nothing said.
        assert!(offered_lines(&[], |_| false).is_empty());
    }
}
