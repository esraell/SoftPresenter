//! Changing a song's slides from the command line, for a quick fix without the editor: read a
//! slide's objects and how they look, set an object's words, its look (any of the inspector's
//! text settings) or its place, and read or replace the whole song as song text. Each change
//! is saved and can be undone, as one made in the window can, and a slide on screen shows it at
//! once.

use std::sync::Arc;

use sp_engine::SlideRef;
use sp_model::{Object, ObjectKind, Presentation, Rgba, Slide, SlideRect, TextStyle, Theme};

use crate::control::Reply;

/// The slides a command reads: a song's, or a playlist item's.
struct Read {
    /// What the reply calls them: the song's title, or the passage's.
    title: String,
    presentation: Arc<Presentation>,
    /// The theme they are drawn with, for each object's look.
    theme: Arc<Theme>,
    /// Their place among the show's presentations, to mark the slide on screen: the song's;
    /// for a saved passage, the Bible page's place, when the same passage is on screen from it.
    at: Option<usize>,
}

/// A playlist item named on the command line: its number, a slide's number, and the playlist.
type ItemWords = (usize, Option<usize>, Option<String>);

/// `item N [SLIDE] [in PLAYLIST]`: a playlist item's number, a slide's number (when one is
/// wanted), and the playlist named. `None` when the words don't start with `item` and a number
/// (and, with a slide wanted, another number), so a song called "Item…" is still found by name.
fn item_words(words: &[String], with_slide: bool) -> Option<Result<ItemWords, String>> {
    let [first, number, rest @ ..] = words else { return None };
    if !first.eq_ignore_ascii_case("item") {
        return None;
    }
    let item = number.parse::<usize>().ok()?;
    let (slide, rest) = match rest {
        [number, rest @ ..] if with_slide => (Some(number.parse::<usize>().ok()?), rest),
        _ if with_slide => return None,
        rest => (None, rest),
    };
    let list = match rest {
        [] => None,
        [word, name @ ..] if word.eq_ignore_ascii_case("in") && !name.is_empty() => Some(name.join(" ")),
        _ if slide.is_some() => return Some(Err("slide item N SLIDE [in PLAYLIST]".to_owned())),
        _ => return Some(Err("slides item N [in PLAYLIST] [--full]".to_owned())),
    };
    Some(Ok((item, slide, list)))
}

impl crate::App {
    /// `slide SONG N`, `slide text …`, `slide style …`, `slide frame …`.
    pub(crate) fn ctl_slide(&mut self, args: &[String]) -> Result<Reply, String> {
        if let Some(result) = self.ctl_slide_change(args) {
            return result;
        }
        match args.get(1).map(|w| w.to_lowercase()).as_deref() {
            Some("text") => self.ctl_slide_text(&args[2..]),
            Some("style") => self.ctl_slide_style(&args[2..]),
            Some("frame") => self.ctl_slide_frame(&args[2..]),
            Some(_) => self.ctl_slide_show(&args[1..]),
            None => Err(SLIDE_USAGE.to_owned()),
        }
    }

    /// `slide SONG N [--json]`, or `slide item N SLIDE [in PLAYLIST]`: the slide's objects, and
    /// how each looks.
    fn ctl_slide_show(&mut self, args: &[String]) -> Result<Reply, String> {
        let json = args.iter().any(|w| w == "--json");
        let words: Vec<String> = args.iter().filter(|w| *w != "--json").cloned().collect();
        let (read, number) = match item_words(&words, true) {
            Some(parsed) => {
                let (item, slide, list) = parsed?;
                (self.read_item(item, list.as_deref())?, slide.unwrap_or(0).to_string())
            }
            None => {
                let [song, number] = words.as_slice() else { return Err(SLIDE_USAGE.to_owned()) };
                (self.read_song(song)?, number.clone())
            }
        };
        let index = slide_number(&number, read.presentation.slide_count())?;
        let (text, mut data) = self.slide_facts(&read, index)?;
        if json && let Some(slide) = read.presentation.slide(index).map(|(_, slide)| slide) {
            data["raw"] = serde_json::to_value(slide).unwrap_or_default();
        }
        Ok(Reply::with(text, data))
    }

    /// `slides [SONG | item N [in PLAYLIST]] [--full]`: the slides of a song (the one shown in
    /// the window if none is named) or of a playlist item, a saved Bible passage included. With
    /// `--full`, each slide as `slide SONG N` gives it, objects and looks too, in one reply.
    pub(crate) fn ctl_slides(&mut self, args: &[String]) -> Result<Reply, String> {
        let full = args.iter().any(|w| w == "--full");
        let words: Vec<String> = args.iter().skip(1).filter(|w| *w != "--full" && *w != "--json").cloned().collect();
        let (read, version) = match item_words(&words, false) {
            Some(parsed) => {
                let (item, _, list) = parsed?;
                let version = self.item_version(item, list.as_deref());
                (self.read_item(item, list.as_deref())?, version)
            }
            None => (self.read_song(&words.join(" "))?, None),
        };
        // Slides of the show's own (a song's, not a passage made only to be listed) have a
        // picture a remote page can show, drawn in the item's theme version if it has one.
        let pictured = read
            .at
            .filter(|at| self.show.presentations().get(*at).is_some_and(|song| Arc::ptr_eq(song, &read.presentation)));
        let mut lines = Vec::new();
        let mut blocks = Vec::new();
        let mut data = Vec::new();
        for (index, (group, slide)) in read.presentation.slides().enumerate() {
            let words =
                slide.texts().into_iter().map(|(_, text)| text.replace('\n', " / ")).collect::<Vec<_>>().join(" / ");
            let live = read.at.is_some_and(|at| self.show.live() == Some(SlideRef { presentation: at, slide: index }));
            // Marks for what a slide sets for itself: off, its label, its hot key.
            let mut marks = Vec::new();
            if !slide.enabled {
                marks.push("off".to_owned());
            }
            if let Some(label) = &slide.label {
                marks.push(format!("label {label}"));
            }
            if let Some(key) = slide.hot_key {
                marks.push(format!("key {key}"));
            }
            if slide.check.is_some() {
                marks.push("to check".to_owned());
            }
            let marked = if marks.is_empty() { String::new() } else { format!("   ({})", marks.join(", ")) };
            lines.push(format!(
                "{:>3}. {}{}{}{}",
                index + 1,
                if group.name.is_empty() { String::new() } else { format!("[{}] ", group.name) },
                words,
                marked,
                if live { "   ← on screen" } else { "" }
            ));
            let thumb = pictured.map(|at| self.thumb_address(at, slide, version));
            let mut entry = serde_json::json!({
                "slide": index + 1, "group": group.name, "words": words, "on_screen": live,
                "enabled": slide.enabled, "label": slide.label, "hot_key": slide.hot_key,
                "check": slide.check, "id": slide.id, "thumb": thumb,
            });
            // Words styled one by one: for each text object that has any, its number and spans.
            let styled: Vec<serde_json::Value> = slide
                .objects
                .iter()
                .enumerate()
                .filter(|(_, object)| !object.spans().is_empty())
                .map(|(at, object)| serde_json::json!({"object": at + 1, "spans": object.spans()}))
                .collect();
            if !styled.is_empty() {
                entry["styled_words"] = serde_json::json!(styled);
            }
            if full {
                let (block, facts) = self.slide_facts(&read, index)?;
                blocks.push(block);
                if let (Some(entry), serde_json::Value::Object(facts)) = (entry.as_object_mut(), facts) {
                    entry.extend(facts.into_iter().filter(|(key, _)| key != "song"));
                }
            }
            data.push(entry);
        }
        let text = if full { blocks.join("\n\n") } else { format!("{}\n{}", read.title, lines.join("\n")) };
        Ok(Reply::with(text, serde_json::json!(data)))
    }

    /// The theme version a playlist item is set to be shown in, if any.
    fn item_version(&self, number: usize, list: Option<&str>) -> Option<sp_model::Uuid> {
        let list = self.playlist_at_for_cli(list).ok()?;
        let state = &self.running.as_ref()?.home_state;
        match state.services.list.get(list)?.items.get(number.checked_sub(1)?)? {
            crate::service::Item::Song { show_with: Some(with), .. } => Some(with.version),
            _ => None,
        }
    }

    /// The slides of the song a name, number or id stands for, or of the song shown in the
    /// window when nothing is typed.
    fn read_song(&mut self, typed: &str) -> Result<Read, String> {
        let at = if typed.trim().is_empty() { self.show.selected() } else { self.find_song(typed)? };
        let presentation = self.show.presentations().get(at).cloned().ok_or("That song is gone.")?;
        Ok(Read { title: presentation.title.clone(), theme: self.theme_of(at).clone(), presentation, at: Some(at) })
    }

    /// The slides of a playlist item (of the playlist in front if none is named): a song's, or
    /// a saved Bible passage's, made as opening it would make them, without opening it.
    fn read_item(&mut self, number: usize, list: Option<&str>) -> Result<Read, String> {
        use crate::service::Item;
        let list = self.playlist_at_for_cli(list)?;
        let state = &self.running.as_ref().ok_or("The app isn't ready.")?.home_state;
        let playlist = &state.services.list[list];
        let item = number.checked_sub(1).and_then(|at| playlist.items.get(at)).cloned().ok_or_else(|| {
            format!("{} has {} items; there is no item {number}.", playlist.name, playlist.items.len())
        })?;
        let title = item.title().to_owned();
        match &item {
            Item::Song { key, .. } => {
                let at = crate::studio::playlist_song(&self.show, state, key)
                    .ok_or_else(|| format!("{title} can't be found: it is missing from the library."))?;
                self.freshen(at, false);
                let presentation = self.show.presentations().get(at).cloned().ok_or("That song is gone.")?;
                Ok(Read {
                    title: presentation.title.clone(),
                    theme: self.theme_of(at).clone(),
                    presentation,
                    at: Some(at),
                })
            }
            Item::Passage(saved) => {
                let passage =
                    self.running.as_mut().ok_or("The app isn't ready.")?.home_state.bible.passage_of(saved)?;
                // The same passage on screen from the Bible page, so its slide is marked.
                let same = |on: &Presentation| {
                    on.title == passage.title
                        && on
                            .slides()
                            .map(|(_, slide)| slide.texts())
                            .eq(passage.slides().map(|(_, slide)| slide.texts()))
                };
                let at = self.show.live().map(|live| live.presentation).filter(|at| {
                    self.bible_slots.contains(at) && self.show.presentations().get(*at).is_some_and(|on| same(on))
                });
                Ok(Read {
                    title: passage.title.clone(),
                    presentation: Arc::new(passage),
                    theme: self.theme_for_bible.clone(),
                    at,
                })
            }
            Item::Header { .. } => Err(format!("{title} is a header; it has no slides.")),
            Item::Media { id, .. } => {
                let what = match self.show.media().iter().find(|media| media.id == *id) {
                    Some(media) if crate::media::is_video(&media.path) => "a video",
                    Some(_) => "a picture",
                    None => "a picture or video no longer in the media bin",
                };
                let name = &playlist.name;
                Err(format!("{title} is {what}, not slides (sp playlist go {number} 1 in {name} shows it)."))
            }
            Item::MediaPlaylist { .. } => {
                let name = &playlist.name;
                Err(format!("{title} is a media playlist, not slides (sp playlist go {number} 1 in {name} plays it)."))
            }
            Item::Placeholder { .. } => Err(format!(
                "{title} is a placeholder; it has no slides yet (sp playlist fill {title} with SONG in {}).",
                playlist.name
            )),
        }
    }

    /// One slide as `slide SONG N` gives it: its own settings and its objects, in words and as
    /// data.
    fn slide_facts(&self, read: &Read, index: usize) -> Result<(String, serde_json::Value), String> {
        let presentation = &read.presentation;
        let (group, slide) = presentation.slide(index).ok_or("No such slide.")?;
        let theme = &read.theme;
        let mut lines = vec![format!(
            "{}, slide {} of {}{}",
            read.title,
            index + 1,
            presentation.slide_count(),
            if group.name.is_empty() { String::new() } else { format!(" ({})", group.name) }
        )];
        let mut objects = Vec::new();
        for (number, object) in slide.objects.iter().enumerate() {
            let (text, data) = describe(slide, number, object, theme);
            lines.push(text);
            objects.push(data);
        }
        // The slide's own settings, so what was set can be read back.
        let template =
            slide.template.and_then(|id| theme.templates.iter().find(|t| t.id == id)).map(|t| t.name.clone());
        let actions: Vec<String> = slide.actions.iter().map(|action| self.action_words(action)).collect();
        let mut settings = Vec::new();
        if !slide.enabled {
            settings.push("off (passed over)".to_owned());
        }
        if let Some(label) = &slide.label {
            settings.push(format!("label {label}"));
        }
        if let Some(key) = slide.hot_key {
            settings.push(format!("hot key {key}"));
        }
        if let Some(why) = &slide.check {
            settings.push(format!("to check: {why}"));
        }
        if let Some(transition) = &slide.transition {
            settings.push(format!("transition {} {:.1}s", transition.kind.name(), transition.seconds));
        }
        if let Some(name) = &template {
            settings.push(format!("template {name}"));
        }
        if let Some(seconds) = slide.advance {
            settings.push(format!("goes on after {seconds} s"));
        }
        if !settings.is_empty() {
            lines.insert(1, format!("    {}", settings.join(", ")));
        }
        if !actions.is_empty() {
            lines.push(format!("    actions: {}", actions.join("; ")));
        }
        if let Some(notes) = &slide.notes {
            lines.push(format!("    notes: {}", notes.replace('\n', " / ")));
        }
        let data = serde_json::json!({
            "song": read.title, "slide": index + 1, "group": group.name, "objects": objects,
            "enabled": slide.enabled, "label": slide.label, "notes": slide.notes, "hot_key": slide.hot_key,
            "check": slide.check, "transition": slide.transition, "template": template, "actions": actions, "advance": slide.advance,
        });
        Ok((lines.join("\n"), data))
    }

    /// `slide text SONG N [OBJECT] WORDS`: one object's words. `\n` in WORDS is a line break.
    fn ctl_slide_text(&mut self, args: &[String]) -> Result<Reply, String> {
        let usage = "slide text SONG N [OBJECT] WORDS   (\\n in WORDS starts a new line)";
        let (song, number, object, words) = match args {
            [song, number, words] => (song, number, None, words),
            [song, number, object, words] => (song, number, Some(object.as_str()), words),
            _ => return Err(usage.to_owned()),
        };
        let at = self.find_song(song)?;
        let presentation = self.show.presentations().get(at).cloned().ok_or("That song is gone.")?;
        let index = slide_number(number, presentation.slide_count())?;
        let (_, slide) = presentation.slide(index).ok_or("No such slide.")?;
        let which = find_object(slide, object)?;
        let words = words.replace("\\n", "\n");
        let mut changed = slide.clone();
        if !matches!(changed.objects[which].kind, ObjectKind::Text { .. }) {
            return Err(format!("Object {} has no words.", which + 1));
        }
        changed.objects[which].set_text(&words);
        let name = object_name(&changed.objects[which]);
        let song = presentation.with_slide(index, changed).ok_or("No such slide.")?;
        self.change_song(at, song);
        Ok(Reply::done(format!(
            "{}, slide {}: {name} now says \"{}\".",
            presentation.title,
            index + 1,
            words.replace('\n', " / ")
        )))
    }

    /// `slide style SONG N|all [OBJECT] key=value … | reset`.
    fn ctl_slide_style(&mut self, args: &[String]) -> Result<Reply, String> {
        let usage =
            "slide style SONG N|all [OBJECT] key=value … [--chars FROM-TO]   (or reset; settings: slide style keys)";
        if args.first().is_some_and(|w| w.eq_ignore_ascii_case("keys")) {
            return Ok(Reply::done(STYLE_KEYS.trim_end()));
        }
        let [song, which, rest @ ..] = args else { return Err(usage.to_owned()) };
        let (rest, chars) = take_chars(rest)?;
        let rest = &rest[..];
        if rest.is_empty() {
            return Err(usage.to_owned());
        }
        let at = self.find_song(song)?;
        let presentation = self.show.presentations().get(at).cloned().ok_or("That song is gone.")?;
        let slides: Vec<usize> = if which.eq_ignore_ascii_case("all") {
            (0..presentation.slide_count()).collect()
        } else {
            vec![slide_number(which, presentation.slide_count())?]
        };
        // An object named first, unless the first word is a setting or reset.
        let (object, settings) = match rest {
            [first, more @ ..] if !first.contains('=') && !first.eq_ignore_ascii_case("reset") => {
                (Some(first.as_str()), more)
            }
            _ => (None, rest),
        };
        if settings.is_empty() {
            return Err(usage.to_owned());
        }
        let mut song = presentation.as_ref().clone();
        let mut said = Vec::new();
        for index in &slides {
            let (_, slide) = song.slide(*index).ok_or("No such slide.")?;
            let mut changed = slide.clone();
            let which = match find_object(&changed, object) {
                Ok(which) => which,
                // With all the slides, one without that object is passed over.
                Err(_) if slides.len() > 1 => continue,
                Err(problem) => return Err(problem),
            };
            let ObjectKind::Text { text, style, spans } = &mut changed.objects[which].kind else {
                return Err(format!("Object {} has no words to style.", which + 1));
            };
            said = match chars {
                Some((from, to)) => style_words(text.chars().count(), spans, from, to, settings)?,
                None => apply_settings(style, settings)?,
            };
            song = song.with_slide(*index, changed).ok_or("No such slide.")?;
        }
        if said.is_empty() {
            return Err("No slide has that object.".to_owned());
        }
        let place = if slides.len() > 1 { "every slide".to_owned() } else { format!("slide {}", slides[0] + 1) };
        self.change_song(at, song);
        Ok(Reply::done(format!("{}, {place}: {}.", presentation.title, said.join(", "))))
    }

    /// `slide frame SONG N|all OBJECT x=… y=… w=… h=…`: where an object sits, in the slide's
    /// pixels; with `all`, on every slide that has that object.
    fn ctl_slide_frame(&mut self, args: &[String]) -> Result<Reply, String> {
        let usage = "slide frame SONG N|all OBJECT x=… y=… w=… h=…   (or reset, back to the theme's place)";
        let [song, number, object, settings @ ..] = args else { return Err(usage.to_owned()) };
        if settings.is_empty() {
            return Err(usage.to_owned());
        }
        let at = self.find_song(song)?;
        let presentation = self.show.presentations().get(at).cloned().ok_or("That song is gone.")?;
        let every = number.eq_ignore_ascii_case("all");
        let slides: Vec<usize> = if every {
            (0..presentation.slide_count()).collect()
        } else {
            vec![slide_number(number, presentation.slide_count())?]
        };
        let theme = self.theme_of(at).clone();
        let reset = settings.iter().any(|w| w.eq_ignore_ascii_case("reset"));
        let mut song = presentation.as_ref().clone();
        let mut last = None;
        for index in &slides {
            let (_, slide) = song.slide(*index).ok_or("No such slide.")?;
            let which = match find_object(slide, Some(object)) {
                Ok(which) => which,
                // With all the slides, one without that object is passed over.
                Err(_) if every => continue,
                Err(problem) => return Err(problem),
            };
            let mut changed = slide.clone();
            let placed = theme.place_on(slide, slide.objects[which].role.as_ref());
            let target = &mut changed.objects[which];
            target.frame = if reset { None } else { Some(set_frame(target.frame.unwrap_or(placed), settings)?) };
            last = Some((object_name(target), target.frame.unwrap_or(placed), *index));
            song = song.with_slide(*index, changed).ok_or("No such slide.")?;
        }
        let Some((name, frame, index)) = last else { return Err("No slide has that object.".to_owned()) };
        self.change_song(at, song);
        // Said, though not refused: a box partly or wholly off the slide.
        let (slide_w, slide_h) = (sp_model::SLIDE_WIDTH, sp_model::SLIDE_HEIGHT);
        let outside = frame.x + frame.w <= 0.0 || frame.y + frame.h <= 0.0 || frame.x >= slide_w || frame.y >= slide_h;
        let partly = frame.x < 0.0 || frame.y < 0.0 || frame.x + frame.w > slide_w || frame.y + frame.h > slide_h;
        let warning = if outside {
            format!(" It is off the slide ({slide_w:.0} x {slide_h:.0}), so it won't show.")
        } else if partly {
            format!(" Part of it is off the slide ({slide_w:.0} x {slide_h:.0}).")
        } else {
            String::new()
        };
        let place = if every { "every slide".to_owned() } else { format!("slide {}", index + 1) };
        Ok(Reply::done(format!(
            "{}, {place}: {name} at x={:.0} y={:.0} w={:.0} h={:.0}.{warning}",
            presentation.title, frame.x, frame.y, frame.w, frame.h
        )))
    }

    /// `song text SONG [FILE]`: the song as song text, or its words replaced from a file. Its
    /// slides keep their looks, labels and notes where they still match, as Reflow keeps them.
    pub(crate) fn ctl_song_text(&mut self, args: &[String]) -> Result<Reply, String> {
        let usage = "song text SONG [FILE]";
        let (song, file) = match args {
            [song] => (song, None),
            [song, file] => (song, Some(file)),
            _ => return Err(usage.to_owned()),
        };
        let at = self.find_song(song)?;
        let presentation = self.show.presentations().get(at).cloned().ok_or("That song is gone.")?;
        let Some(file) = file else {
            let text = sp_model::to_song_text(&presentation);
            return Ok(Reply::with(text.clone(), serde_json::json!({"song": presentation.title, "text": text})));
        };
        self.only_at_the_computer(crate::remote::policy::AtComputer::SongWordsFile)?;
        // The file named by its name alone in replies (the client sends the whole path).
        let shown =
            std::path::Path::new(file).file_name().map_or_else(|| file.clone(), |n| n.to_string_lossy().into_owned());
        let bytes = std::fs::read(file).map_err(|e| format!("Could not read {shown}: {e}"))?;
        let text = sp_model::decode_text(&bytes);
        let mut song = presentation.reflowed(&text);
        // Words replaced as text: the chorus is sung after each verse, as Edit Words does.
        song.follow_parts_as_text(&crate::home::chorus_after_name());
        if song.slide_count() == 0 {
            return Err(format!("{shown} has no words in it; {} is unchanged.", presentation.title));
        }
        let count = song.slide_count();
        self.change_song(at, song);
        Ok(Reply::done(format!("{} now has the words from {shown}: {count} slides.", presentation.title)))
    }

    /// Puts a song changed from the command line in its place, as a change in the window is
    /// put: it can be undone (from the Service view, or the Edit view's undo), it is saved, its
    /// pictures are drawn again, an editor open on it shows it, and the screens show it at once.
    pub(crate) fn change_song(&mut self, at: usize, song: Presentation) {
        let ids: Vec<sp_model::Uuid> = song.slides().map(|(_, slide)| slide.id).collect();
        self.remember_for_undo(at);
        self.note_song_step();
        self.show.replace_presentation(at, Arc::new(song));
        if !self.unsaved.contains(&at) {
            self.unsaved.push(at);
        }
        self.save_unsaved();
        if let Some(running) = self.running.as_mut() {
            for id in ids {
                running.thumbs.forget(id, &mut running.egui_renderer);
            }
            let open = running.home_state.editing.as_ref().map(|draft| draft.at).filter(|open| open.presentation == at);
            if let Some(open) = open {
                running.home_state.editing = crate::editor::EditDraft::of(&self.show, open);
            }
            running.home_window.request_redraw();
        }
        self.refresh_output();
    }
}

const SLIDE_USAGE: &str = "slide SONG N [--json], slide text SONG N [OBJECT] WORDS, slide style SONG N|all [OBJECT] key=value…, slide frame SONG N OBJECT x= y= w= h=";

/// The settings `slide style` takes.
pub const STYLE_KEYS: &str = "\
Settings for slide style (key=value; key=reset goes back to the theme's; reset alone, all of them):
  font=NAME            size=1-1000          weight=100-1000|light|regular|medium|semibold|bold|heavy
  italic=on|off        underline=on|off     strike=on|off        shadow=on|off   (yes|no too)
  color=#RRGGBB[AA]    outline=0-100        outline_color=#RRGGBB[AA]
  align=left|center|right|justify           valign=top|middle|bottom
  caps=none|all_caps|small_caps|title_case|start_case
  scaling=none|text_down|text_up|text_up_or_down|container_up_or_down
  line_transform=none|remove_line_returns|line_returns_as_slashes
  line_height=0.1-10   letter_spacing=-50-200  paragraph_spacing=0-500   baseline=-500-500
";

/// A slide by its number (from 1).
fn slide_number(typed: &str, count: usize) -> Result<usize, String> {
    if count == 0 {
        return Err("That song has no slides.".to_owned());
    }
    match typed.trim().parse::<usize>() {
        Ok(n) if n >= 1 && n <= count => Ok(n - 1),
        _ => Err(format!("Slide numbers go from 1 to {count}.")),
    }
}

/// What an object is called: its name, or its role, or its kind.
fn object_name(object: &Object) -> String {
    if !object.name.trim().is_empty() {
        return object.name.trim().to_owned();
    }
    match (&object.role, &object.kind) {
        (Some(role), _) => role.label(),
        (None, ObjectKind::Text { .. }) => "Text".to_owned(),
        (None, ObjectKind::Web { .. }) => "Web page".to_owned(),
        (None, ObjectKind::Ndi { .. }) => "NDI source".to_owned(),
        (None, ObjectKind::Mirror { .. }) => "Phone or iPad Screen".to_owned(),
        (None, _) => "Object".to_owned(),
    }
}

/// An object of a slide by its number (from 1), its name or its role; with none named, the
/// main text (the box for the main language, or the first with words).
pub fn find_object(slide: &Slide, typed: Option<&str>) -> Result<usize, String> {
    let is_text = |object: &Object| matches!(object.kind, ObjectKind::Text { .. });
    let Some(typed) = typed.map(str::trim).filter(|t| !t.is_empty()) else {
        return slide
            .objects
            .iter()
            .position(|o| o.role == Some(sp_model::Role::primary()))
            .or_else(|| slide.objects.iter().position(is_text))
            .ok_or_else(|| "The slide has no words.".to_owned());
    };
    if let Ok(n) = typed.parse::<usize>() {
        return if n >= 1 && n <= slide.objects.len() {
            Ok(n - 1)
        } else {
            Err(format!("Object numbers on this slide go from 1 to {}.", slide.objects.len()))
        };
    }
    let wanted = typed.to_lowercase();
    let main_words = ["main", "primary"];
    slide
        .objects
        .iter()
        .position(|o| {
            o.name.trim().to_lowercase() == wanted
                || o.role.as_ref().is_some_and(|role| {
                    role.0.to_lowercase() == wanted
                        || role.label().to_lowercase() == wanted
                        || (main_words.contains(&wanted.as_str()) && *role == sp_model::Role::primary())
                })
        })
        .ok_or_else(|| {
            let names: Vec<String> =
                slide.objects.iter().enumerate().map(|(n, o)| format!("{} {}", n + 1, object_name(o))).collect();
            format!("This slide has no object called {typed}. It has: {}.", names.join(", "))
        })
}

/// A colour written #RGB, #RRGGBB or #RRGGBBAA.
pub fn parse_color(typed: &str) -> Option<Rgba> {
    let hex = typed.trim().trim_start_matches('#');
    let byte = |at: usize| u8::from_str_radix(hex.get(at..at + 2)?, 16).ok();
    match hex.len() {
        3 => {
            let digit = |at: usize| u8::from_str_radix(hex.get(at..=at)?, 16).ok().map(|d| d * 17);
            Some(Rgba { r: digit(0)?, g: digit(1)?, b: digit(2)?, a: 255 })
        }
        6 => Some(Rgba { r: byte(0)?, g: byte(2)?, b: byte(4)?, a: 255 }),
        8 => Some(Rgba { r: byte(0)?, g: byte(2)?, b: byte(4)?, a: byte(6)? }),
        _ => None,
    }
}

/// Takes `--chars FROM-TO` (or `--chars=FROM-TO`) out of a command's words: the characters of
/// the words to style, counted from 0 with TO not included, as the saved file counts them.
type Chars = Option<(usize, usize)>;

fn take_chars(words: &[String]) -> Result<(Vec<String>, Chars), String> {
    let mut kept = Vec::new();
    let mut range = None;
    let mut words = words.iter();
    while let Some(word) = words.next() {
        let value = if word == "--chars" {
            Some(words.next().ok_or("--chars needs a range, such as 4-9.")?.as_str())
        } else {
            word.strip_prefix("--chars=")
        };
        let Some(value) = value else {
            kept.push(word.clone());
            continue;
        };
        let bad = || format!("--chars is FROM-TO, two numbers counted from 0 (TO not included), not {value}.");
        let (from, to) = value.split_once('-').ok_or_else(bad)?;
        let (from, to) =
            (from.trim().parse::<usize>().map_err(|_| bad())?, to.trim().parse::<usize>().map_err(|_| bad())?);
        if from >= to {
            return Err(bad());
        }
        range = Some((from, to));
    }
    Ok((kept, range))
}

/// Styles the characters from `from` to `to` of words `length` characters long: the settings
/// a word can have (font, size, weight, italic, underline, strike, colour) go to a span there,
/// `key=reset` clears one, and `reset` alone clears them all. Anything else is the box's.
fn style_words(
    length: usize,
    spans: &mut Vec<sp_model::TextSpan>,
    from: usize,
    to: usize,
    settings: &[String],
) -> Result<Vec<String>, String> {
    let to = to.min(length);
    if from >= to {
        return Err(format!("The words have {length} characters; --chars {from}-{to} is past them."));
    }
    let field = |key: &str| match key.trim().to_lowercase().as_str() {
        "font" | "font_family" => Some("font_family"),
        "color" | "colour" | "fill" => Some("fill"),
        "size" => Some("size"),
        "weight" => Some("weight"),
        "italic" => Some("italic"),
        "underline" => Some("underline"),
        "strike" => Some("strike"),
        _ => None,
    };
    let mut clear: Vec<&'static str> = Vec::new();
    let mut set: Vec<String> = Vec::new();
    let mut everything = false;
    for setting in settings {
        if setting.eq_ignore_ascii_case("reset") {
            everything = true;
            continue;
        }
        let (key, value) = setting.split_once('=').ok_or_else(|| format!("{setting}: write settings as key=value."))?;
        let name = field(key).ok_or_else(|| {
            format!("{key} is a setting of the whole box; with --chars only font, size, weight, italic, underline, strike and colour apply to words.")
        })?;
        if value.trim().eq_ignore_ascii_case("reset") {
            clear.push(name);
        } else {
            set.push(setting.clone());
        }
    }
    let mut scratch = TextStyle::default();
    let mut said = apply_settings(&mut scratch, &set)?;
    if everything {
        said.push("back to the box's look".to_owned());
    }
    said.extend(clear.iter().map(|name| format!("{name} as the box has it")));
    said.push(format!("characters {from}-{to}"));
    sp_model::restyle(spans, from, to, length, |own| {
        if everything {
            *own = sp_model::SpanStyle::default();
        }
        for name in &clear {
            match *name {
                "font_family" => own.font_family = None,
                "fill" => own.fill = None,
                "size" => own.size = None,
                "weight" => own.weight = None,
                "italic" => own.italic = None,
                "underline" => own.underline = None,
                _ => own.strike = None,
            }
        }
        *own = own.over(&sp_model::SpanStyle {
            font_family: scratch.font_family.clone(),
            size: scratch.size,
            weight: scratch.weight,
            italic: scratch.italic,
            underline: scratch.underline,
            strike: scratch.strike,
            fill: scratch.fill,
        });
    });
    Ok(said)
}

/// Changes a text object's own look by settings written key=value (see [`STYLE_KEYS`]), or
/// `reset` for the theme's look. Returns each change in words, or what is wrong.
pub fn apply_settings(style: &mut TextStyle, settings: &[String]) -> Result<Vec<String>, String> {
    let mut said = Vec::new();
    let mut fields = serde_json::to_value(&*style).map_err(|e| e.to_string())?;
    for setting in settings {
        if setting.eq_ignore_ascii_case("reset") {
            fields = serde_json::to_value(TextStyle::default()).map_err(|e| e.to_string())?;
            said.push("back to the theme's look".to_owned());
            continue;
        }
        let Some((key, value)) = setting.split_once('=') else {
            return Err(format!("{setting}: write settings as key=value (see slide style keys)."));
        };
        let (key, value) = (key.trim().to_lowercase(), value.trim());
        let field = match key.as_str() {
            "font" => "font_family",
            "color" | "colour" => "fill",
            "outline" => "outline_width",
            "outline_colour" => "outline_color",
            other => other,
        };
        let known = [
            "font_family",
            "size",
            "weight",
            "italic",
            "fill",
            "align",
            "outline_width",
            "outline_color",
            "shadow",
            "valign",
            "caps",
            "underline",
            "strike",
            "scaling",
            "line_transform",
            "line_height",
            "letter_spacing",
            "paragraph_spacing",
            "baseline",
        ];
        if !known.contains(&field) {
            return Err(format!("{key} isn't a setting slide style knows (see slide style keys)."));
        }
        let parsed = if value.eq_ignore_ascii_case("reset") {
            serde_json::Value::Null
        } else {
            match field {
                "italic" | "shadow" | "underline" | "strike" => match value.to_lowercase().as_str() {
                    "on" | "yes" | "true" => serde_json::json!(true),
                    "off" | "no" | "false" => serde_json::json!(false),
                    _ => return Err(format!("{key} is on or off, not {value}.")),
                },
                "fill" | "outline_color" => {
                    let color =
                        parse_color(value).ok_or_else(|| format!("{value} isn't a colour; write it #RRGGBB."))?;
                    serde_json::to_value(color).map_err(|e| e.to_string())?
                }
                "weight" => {
                    let weight = match value.to_lowercase().as_str() {
                        "light" => 300,
                        "regular" | "normal" => 400,
                        "medium" => 500,
                        "semibold" => 600,
                        "bold" => 700,
                        "heavy" | "black" => 900,
                        number => {
                            number.parse::<u16>().map_err(|_| format!("weight is a number or bold, not {value}."))?
                        }
                    };
                    if !(100..=1000).contains(&weight) {
                        return Err(format!("weight goes from 100 to 1000, not {value}."));
                    }
                    serde_json::json!(weight)
                }
                "size" | "outline_width" | "line_height" | "letter_spacing" | "paragraph_spacing" | "baseline" => {
                    let number = value.parse::<f32>().map_err(|_| format!("{key} is a number, not {value}."))?;
                    let (low, high) = match field {
                        "size" => (1.0, 1000.0),
                        "outline_width" => (0.0, 100.0),
                        "line_height" => (0.1, 10.0),
                        "letter_spacing" => (-50.0, 200.0),
                        "paragraph_spacing" => (0.0, 500.0),
                        _ => (-500.0, 500.0),
                    };
                    if !(low..=high).contains(&number) {
                        return Err(format!("{key} goes from {low} to {high}, not {value}."));
                    }
                    serde_json::json!(number)
                }
                "font_family" if value.is_empty() => return Err("font needs a name (or font=reset).".to_owned()),
                "font_family" => serde_json::json!(value),
                // A choice among names: written as the theme files write them.
                _ => serde_json::json!(value.to_lowercase().replace([' ', '-'], "_")),
            }
        };
        fields[field] = parsed;
        said.push(if value.eq_ignore_ascii_case("reset") {
            format!("{key} as the theme has it")
        } else {
            format!("{key} {value}")
        });
    }
    let changed: TextStyle = serde_json::from_value(fields).map_err(|e| {
        let text = e.to_string();
        format!("That isn't a value slide style takes: {}", text.split(" at line").next().unwrap_or(&text))
    })?;
    *style = changed;
    Ok(said)
}

/// A frame changed by settings written x=, y=, w= (or width=), h= (or height=).
pub fn set_frame(mut frame: SlideRect, settings: &[String]) -> Result<SlideRect, String> {
    for setting in settings {
        let Some((key, value)) = setting.split_once('=') else {
            return Err(format!("{setting}: write the place as x=… y=… w=… h=…."));
        };
        let number = value.trim().parse::<f32>().map_err(|_| format!("{key} is a number, not {value}."))?;
        match key.trim().to_lowercase().as_str() {
            "x" => frame.x = number,
            "y" => frame.y = number,
            "w" | "width" | "h" | "height" if number <= 0.0 => {
                return Err(format!("{key} must be more than 0, not {value}."));
            }
            "w" | "width" => frame.w = number,
            "h" | "height" => frame.h = number,
            other => return Err(format!("{other} isn't x, y, w or h.")),
        }
    }
    Ok(frame)
}

/// One object of a slide, in words and as data: its number, name, role, kind, words, place,
/// and how its text looks, each setting the slide's own or the theme's.
fn describe(slide: &Slide, number: usize, object: &Object, theme: &Theme) -> (String, serde_json::Value) {
    let place = object.frame.unwrap_or_else(|| theme.place_on(slide, object.role.as_ref()));
    let whose_place = if object.frame.is_some() { "its own" } else { "the theme's" };
    let role = object.role.as_ref().map(|role| role.0.clone()).unwrap_or_default();
    let mut text = format!("{:>2}. {}", number + 1, object_name(object));
    if !role.is_empty() && object_name(object) != object.role.as_ref().map(|r| r.label()).unwrap_or_default() {
        text.push_str(&format!(" (role {role})"));
    }
    let mut data = serde_json::json!({
        "object": number + 1,
        "name": object.name,
        "role": role,
        "frame": {"x": place.x, "y": place.y, "w": place.w, "h": place.h, "own": object.frame.is_some()},
    });
    match &object.kind {
        ObjectKind::Text { text: words, style, spans } => {
            text.push_str(&format!(": \"{}\"", words.replace('\n', " / ")));
            text.push_str(&format!(
                "\n    at x={:.0} y={:.0} w={:.0} h={:.0} ({whose_place})",
                place.x, place.y, place.w, place.h
            ));
            data["kind"] = serde_json::json!("text");
            data["text"] = serde_json::json!(words);
            if !spans.is_empty() {
                data["spans"] = serde_json::to_value(spans).unwrap_or_default();
            }
            if let Some(look) = theme.text_style_on(slide, object.role.as_ref(), style) {
                let own = serde_json::to_value(style).unwrap_or_default();
                let mark = |field: &str| if own.get(field).is_some_and(|v| !v.is_null()) { "*" } else { "" };
                text.push_str(&format!(
                    "\n    font {}{}, size {:.0}{}, weight {}{}, italic {}{}, colour #{:02x}{:02x}{:02x}{}, align {:?}{}, scaling {:?}{}",
                    look.font_family,
                    mark("font_family"),
                    look.size,
                    mark("size"),
                    look.weight,
                    mark("weight"),
                    if look.italic { "on" } else { "off" },
                    mark("italic"),
                    look.fill.r,
                    look.fill.g,
                    look.fill.b,
                    mark("fill"),
                    look.align,
                    mark("align"),
                    look.scaling,
                    mark("scaling"),
                ));
                data["style"] = own;
                data["look"] = serde_json::to_value(&look).unwrap_or_default();
            }
        }
        ObjectKind::Web { url } => {
            text.push_str(&format!(
                ": {url}\n    at x={:.0} y={:.0} w={:.0} h={:.0} ({whose_place})",
                place.x, place.y, place.w, place.h
            ));
            data["kind"] = serde_json::json!("web");
            data["url"] = serde_json::json!(url);
        }
        ObjectKind::Ndi { source, scaling, quality, sound } => {
            let shown = if source.is_empty() { "no source chosen" } else { source.as_str() };
            text.push_str(&format!(
                ": {shown}\n    at x={:.0} y={:.0} w={:.0} h={:.0} ({whose_place})",
                place.x, place.y, place.w, place.h
            ));
            data["kind"] = serde_json::json!("ndi");
            data["source"] = serde_json::json!(source);
            data["scaling"] = serde_json::to_value(scaling).unwrap_or_default();
            data["quality"] = serde_json::to_value(quality).unwrap_or_default();
            data["sound"] = serde_json::json!(sound);
        }
        ObjectKind::Mirror { scaling } => {
            text.push_str(&format!(
                ": whoever shows their screen\n    at x={:.0} y={:.0} w={:.0} h={:.0} ({whose_place})",
                place.x, place.y, place.w, place.h
            ));
            data["kind"] = serde_json::json!("mirror");
            data["scaling"] = serde_json::to_value(scaling).unwrap_or_default();
        }
        _ => {
            text.push_str(&format!(
                " at x={:.0} y={:.0} w={:.0} h={:.0} ({whose_place})",
                place.x, place.y, place.w, place.h
            ));
            data["kind"] = serde_json::json!("other");
        }
    }
    (text, data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sp_model::Role;

    fn slide() -> Slide {
        let mut named = Object::text(Role::translation(), "Перевод");
        named.name = "Russian".to_owned();
        Slide::new(vec![Object::text(Role::primary(), "Main words"), named])
    }

    #[test]
    fn chars_style_words_and_reset_takes_it_off_again() {
        let words = |list: &[&str]| list.iter().map(|w| (*w).to_owned()).collect::<Vec<String>>();
        let (rest, range) = take_chars(&words(&["size=30", "--chars", "4-9", "weight=bold"])).unwrap();
        assert_eq!((rest, range), (words(&["size=30", "weight=bold"]), Some((4, 9))));
        assert_eq!(take_chars(&words(&["--chars=0-1"])).unwrap().1, Some((0, 1)));
        for bad in ["9-4", "x", "3-", "--chars"] {
            let given = if bad == "--chars" { words(&[bad]) } else { words(&["--chars", bad]) };
            assert!(take_chars(&given).is_err(), "{bad}");
        }
        let mut spans = Vec::new();
        let said = style_words(10, &mut spans, 4, 9, &words(&["weight=bold", "colour=#ff0000"])).unwrap();
        assert!(said.contains(&"characters 4-9".to_owned()), "{said:?}");
        assert_eq!(spans.len(), 1);
        assert_eq!((spans[0].start, spans[0].end, spans[0].style.weight), (4, 9, Some(700)));
        assert_eq!(spans[0].style.fill, Some(Rgba { r: 255, g: 0, b: 0, a: 255 }));
        // A setting of the whole box is not for words.
        assert!(style_words(10, &mut spans, 4, 9, &words(&["align=left"])).is_err());
        // One setting taken off, then the rest.
        style_words(10, &mut spans, 4, 9, &words(&["colour=reset"])).unwrap();
        assert_eq!(spans[0].style.fill, None);
        style_words(10, &mut spans, 0, 10, &words(&["reset"])).unwrap();
        assert!(spans.is_empty());
    }

    #[test]
    fn an_object_is_found_by_number_name_or_role_and_the_main_text_by_default() {
        let slide = slide();
        assert_eq!(find_object(&slide, None), Ok(0));
        assert_eq!(find_object(&slide, Some("2")), Ok(1));
        assert_eq!(find_object(&slide, Some("russian")), Ok(1));
        assert_eq!(find_object(&slide, Some("Translation")), Ok(1));
        assert_eq!(find_object(&slide, Some("main")), Ok(0));
        assert!(find_object(&slide, Some("3")).is_err());
        let missing = find_object(&slide, Some("logo")).unwrap_err();
        assert!(missing.contains("1 Main") && missing.contains("2 Russian"), "{missing}");
    }

    #[test]
    fn settings_change_the_look_and_reset_goes_back_to_the_theme() {
        let mut style = TextStyle::default();
        let said = apply_settings(
            &mut style,
            &[
                "size=90".into(),
                "weight=bold".into(),
                "italic=on".into(),
                "color=#ff8800".into(),
                "align=left".into(),
                "scaling=none".into(),
            ],
        )
        .unwrap();
        assert_eq!(said.len(), 6);
        assert_eq!(style.size, Some(90.0));
        assert_eq!(style.weight, Some(700));
        assert_eq!(style.italic, Some(true));
        assert_eq!(style.fill, Some(Rgba { r: 255, g: 136, b: 0, a: 255 }));
        assert_eq!(style.align, Some(sp_model::Align::Left));
        assert_eq!(style.scaling, Some(sp_model::Scaling::None));
        apply_settings(&mut style, &["size=reset".into()]).unwrap();
        assert_eq!(style.size, None);
        assert_eq!(style.weight, Some(700), "the rest kept");
        apply_settings(&mut style, &["reset".into()]).unwrap();
        assert_eq!(style, TextStyle::default());
        // What is wrong is said plainly, and nothing is changed.
        assert!(apply_settings(&mut style, &["sise=90".into()]).unwrap_err().contains("sise"));
        assert!(apply_settings(&mut style, &["align=sideways".into()]).is_err());
        assert!(apply_settings(&mut style, &["color=orange".into()]).unwrap_err().contains("colour"));
        assert!(apply_settings(&mut style, &["size=-5".into()]).unwrap_err().contains("from 1 to 1000"));
        assert!(apply_settings(&mut style, &["size=99999".into()]).is_err());
        assert!(apply_settings(&mut style, &["outline=-1".into()]).is_err());
        assert!(apply_settings(&mut style, &["font=".into()]).is_err());
        assert!(apply_settings(&mut style, &["weight=5".into()]).is_err());
        assert_eq!(style, TextStyle::default());
    }

    #[test]
    fn a_playlist_item_is_named_by_item_and_its_number() {
        let words = |text: &str| text.split(' ').map(str::to_owned).collect::<Vec<_>>();
        assert_eq!(item_words(&words("item 3"), false), Some(Ok((3, None, None))));
        assert_eq!(
            item_words(&words("Item 3 in Friday Evening"), false),
            Some(Ok((3, None, Some("Friday Evening".into()))))
        );
        assert_eq!(item_words(&words("item 3 2 in Friday"), true), Some(Ok((3, Some(2), Some("Friday".into())))));
        assert_eq!(item_words(&words("item 3 2"), true), Some(Ok((3, Some(2), None))));
        // A song called "Item…": slides of "Item Song", slide 3 of the song "item".
        assert_eq!(item_words(&words("item song"), false), None);
        assert_eq!(item_words(&words("item 3"), true), None);
        assert_eq!(item_words(&words("River Song"), false), None);
        assert!(matches!(item_words(&words("item 3 of Friday"), false), Some(Err(_))));
        assert!(matches!(item_words(&words("item 3 2 in"), true), Some(Err(_))));
    }

    #[test]
    fn colours_and_places_are_read_as_written() {
        assert_eq!(parse_color("#fff"), Some(Rgba { r: 255, g: 255, b: 255, a: 255 }));
        assert_eq!(parse_color("00000080"), Some(Rgba { r: 0, g: 0, b: 0, a: 128 }));
        assert_eq!(parse_color("#12345"), None);
        let frame = SlideRect { x: 0.0, y: 0.0, w: 100.0, h: 100.0 };
        let moved = set_frame(frame, &["x=10".into(), "h=50".into()]).unwrap();
        assert_eq!((moved.x, moved.y, moved.w, moved.h), (10.0, 0.0, 100.0, 50.0));
        assert!(set_frame(frame, &["z=1".into()]).is_err());
        assert!(set_frame(frame, &["w=0".into()]).is_err());
        assert!(set_frame(frame, &["h=-3".into()]).is_err());
    }
}
