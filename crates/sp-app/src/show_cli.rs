//! Command-line commands for running the show: opening a playlist item as a click does, jumping
//! to a group or a hot key, the continuous playlist, the video behind the words, stepping through
//! and setting media, the stage message and layouts, and test patterns. Each goes through the
//! window's own requests and says what it did.

use sp_engine::{Command, SlideRef};

use crate::control::{Reply, find};
use crate::home::VideoRequest;

/// Whether a name typed on the command line means this name: the same letters whatever their
/// case, in every alphabet (`припев` is `Припев`), ignoring spaces round either.
pub(crate) fn same_name(have: &str, typed: &str) -> bool {
    have.trim().to_lowercase() == typed.trim().to_lowercase()
}

fn on_off(word: Option<&String>) -> Result<bool, String> {
    match word.map(|w| w.to_lowercase()).as_deref() {
        Some("on" | "yes") => Ok(true),
        Some("off" | "no") => Ok(false),
        _ => Err("Say on or off.".to_owned()),
    }
}

impl crate::App {
    /// `playlist go ITEM [SLIDE] [in PLAYLIST]`: opens an item as a click on it does (so the
    /// continuous playlist follows on from it), and with a slide number puts that slide on screen.
    pub(crate) fn playlist_go(&mut self, words: &[String]) -> Result<Reply, String> {
        let usage = "playlist go ITEM [SLIDE] [in PLAYLIST]   (ITEM by number, title or uid)";
        let (head, list) = match words.iter().rposition(|w| w.eq_ignore_ascii_case("in")) {
            Some(at) if at + 1 < words.len() => (&words[..at], Some(words[at + 1..].join(" "))),
            _ => (words, None),
        };
        // A last word that is a number, after an item, is the slide.
        let (item, slide) = match head {
            [item @ .., last] if !item.is_empty() && last.parse::<usize>().is_ok() => {
                (item.join(" "), last.parse::<usize>().ok())
            }
            item if !item.is_empty() => (item.join(" "), None),
            _ => return Err(usage.to_owned()),
        };
        let list = self.playlist_at_for_cli(list.as_deref())?;
        let state = &self.running.as_ref().ok_or("The app isn't ready.")?.home_state;
        let at = crate::service_cli::item_in(&state.services.list[list], &item)?;
        let chosen = state.services.list[list].items[at].clone();
        let song = match &chosen {
            crate::service::Item::Song { key, .. } => Some(
                crate::studio::playlist_song(&self.show, state, key)
                    .ok_or_else(|| format!("{} can't be found.", chosen.title()))?,
            ),
            crate::service::Item::Header { name } => return Err(format!("{name} is a header; it opens nothing.")),
            crate::service::Item::Passage(_)
            | crate::service::Item::Media { .. }
            | crate::service::Item::MediaPlaylist { .. }
            | crate::service::Item::Placeholder { .. } => None,
        };
        let live = self.show.live().map(|at| at.presentation);
        self.with_state(|state| {
            state.services.show(list);
            state.services.chosen = Some(at);
            state.playlist_open = true;
            if let crate::service::Item::Passage(saved) = &chosen {
                state.bible_open = true;
                state.bible.open_saved(saved, live, crate::home::SlidesOf::Passage);
            } else {
                state.bible_open = false;
            }
        });
        let Some(song) = song else {
            let title = chosen.title().to_owned();
            // A picture, video or media playlist of the announcements plays there, as a click on
            // its tile does.
            if slide == Some(1)
                && let Some((what, from)) = self.announced_item(at, &self.playlist_stops().0)
            {
                self.announce_clicked(what, from);
                return Ok(Reply::done(format!("Playing {title} on the Announcements layer.")));
            }
            // A picture, video or media playlist has one thing to show: 1 shows it (or plays it).
            return match (&chosen, slide) {
                (crate::service::Item::Passage(_), None) => {
                    Ok(Reply::done(format!("Opened {title} on the Bible page.")))
                }
                (crate::service::Item::Passage(_), Some(number)) => self.passage_go(&title, number),
                (crate::service::Item::Media { id, .. }, Some(1)) => {
                    let at = self
                        .show
                        .media()
                        .iter()
                        .position(|media| media.id == *id)
                        .ok_or_else(|| format!("{title} is no longer in the media bin."))?;
                    self.apply(vec![Command::ShowMedia(at)]);
                    Ok(Reply::done(format!("Showing {title}.")))
                }
                (crate::service::Item::MediaPlaylist { id, .. }, Some(1)) => {
                    let state = &self.running.as_ref().ok_or("The app isn't ready.")?.home_state;
                    if !state.media_playlists.iter().any(|playlist| playlist.id == *id) {
                        return Err(format!("The media playlist {title} has been deleted."));
                    }
                    self.play_media_playlist(*id);
                    Ok(Reply::done(format!("Playing the media playlist {title}, round and round.")))
                }
                (crate::service::Item::Placeholder { .. }, Some(_)) => Err(format!(
                    "{title} is a placeholder; it has nothing to show yet (sp playlist fill {title} with SONG)."
                )),
                (_, Some(_)) => Err(format!("{title} has one thing to show: say 1 to show it.")),
                (_, None) => Ok(Reply::done(format!("Opened {title}."))),
            };
        };
        let mut commands = vec![Command::Select(song)];
        if let Some(number) = slide {
            let presentation = self.show.presentations().get(song).ok_or("That song is gone.")?;
            if !(1..=presentation.slide_count()).contains(&number) {
                return Err(format!(
                    "{} has {} slides; there is no slide {number}.",
                    presentation.title,
                    presentation.slide_count()
                ));
            }
            commands.push(Command::GoTo(SlideRef { presentation: song, slide: number - 1 }));
        }
        self.apply(commands);
        if slide.is_some() {
            return Ok(Reply::done(self.live_words()));
        }
        Ok(Reply::done(format!("Opened {}.", chosen.title())))
    }

    /// A slide of the passage just opened on the Bible page, on screen, as a click on it there
    /// does. The passage takes its place among the show's at once, not when the window next
    /// draws.
    fn passage_go(&mut self, title: &str, number: usize) -> Result<Reply, String> {
        let running = self.running.as_mut().ok_or("The app isn't ready.")?;
        let Some((slot, passage)) = running.home_state.bible.pending.take() else {
            let message = running.home_state.bible.message.clone();
            return Err(message.unwrap_or_else(|| format!("Could not open {title}.")));
        };
        let (count, shown) = (passage.slide_count(), passage.title.clone());
        self.show.replace_presentation(slot, std::sync::Arc::new(passage));
        if !(1..=count).contains(&number) {
            return Err(format!("{shown} has {count} slides; there is no slide {number}."));
        }
        self.apply(vec![Command::Select(slot), Command::GoTo(SlideRef { presentation: slot, slide: number - 1 })]);
        Ok(Reply::done(format!("{shown}: {}", self.live_words())))
    }

    /// `group NAME|LETTER [in SONG]`: the first slide of a group on screen, in the song on
    /// screen (or the one shown, or the one named), as the group's letter key does.
    pub(crate) fn ctl_group(&mut self, args: &[String]) -> Result<Reply, String> {
        let words = args.get(1..).unwrap_or_default();
        let (name, song) = match words.iter().rposition(|w| w.eq_ignore_ascii_case("in")) {
            Some(at) if at + 1 < words.len() => (words[..at].join(" "), Some(words[at + 1..].join(" "))),
            _ => (words.join(" "), None),
        };
        if name.trim().is_empty() {
            return Err("group NAME [in SONG]   (or a group's letter: A for Verse 1, C for the chorus…)".to_owned());
        }
        let at = match song {
            Some(song) => self.find_song(&song)?,
            None => self.show.live().map_or(self.show.selected(), |at| at.presentation),
        };
        let presentation = self.show.presentations().get(at).cloned().ok_or("That song is gone.")?;
        // By the group's name, else by its letter.
        let mut index = 0;
        let mut found = None;
        for group in &presentation.groups {
            if same_name(&group.name, &name)
                && let Some(skip) = group.slides.iter().position(|slide| slide.enabled)
            {
                found = Some(index + skip);
                break;
            }
            index += group.slides.len();
        }
        let found = match found {
            Some(slide) => Some(SlideRef { presentation: at, slide }),
            None if name.trim().chars().count() == 1 => {
                self.apply(vec![Command::Select(at)]);
                let letter = name.trim().chars().next().unwrap_or(' ');
                self.show.group_start(sp_model::groups_for_key(letter))
            }
            None => None,
        };
        let mut groups: Vec<&str> = Vec::new();
        for group in &presentation.groups {
            if !groups.contains(&group.name.as_str()) {
                groups.push(group.name.as_str());
            }
        }
        let found = found.ok_or_else(|| {
            format!("{} has no group {}. Its groups: {}.", presentation.title, name.trim(), groups.join(", "))
        })?;
        self.apply(vec![Command::Select(at), Command::GoTo(found)]);
        Ok(Reply::done(self.live_words()))
    }

    /// `hotkey KEY`: the slide with that hot key on screen, as pressing the key does.
    pub(crate) fn ctl_hotkey(&mut self, args: &[String]) -> Result<Reply, String> {
        let key = args.get(1).filter(|k| k.chars().count() == 1).ok_or("hotkey KEY   (one letter or digit)")?;
        let letter = key.chars().next().unwrap_or(' ').to_ascii_uppercase();
        let found = self
            .show
            .hot_key_slide(letter)
            .ok_or_else(|| format!("No slide of the song shown has the hot key {letter}."))?;
        self.apply(vec![Command::GoTo(found)]);
        Ok(Reply::done(self.live_words()))
    }

    /// `continuous on|off`: whether the playlist goes on to its next item after a song's last slide.
    pub(crate) fn ctl_continuous(&mut self, args: &[String]) -> Result<Reply, String> {
        let on = on_off(args.get(1))?;
        self.with_state(|state| state.continuous = on);
        Ok(Reply::done(format!(
            "Continuous {}: {}.",
            if on { "on" } else { "off" },
            if on {
                "after a song's last slide, Next goes on to the playlist's next item"
            } else {
                "Next stops at a song's last slide"
            }
        )))
    }

    /// `video play|pause|restart|seek SECONDS|loop on|off|mute on|off`: the video behind the words.
    pub(crate) fn ctl_video(&mut self, args: &[String]) -> Result<Reply, String> {
        let usage = "video play | pause | restart | seek SECONDS | loop on|off | mute on|off";
        let playing = self.video.as_ref().map(|(_, player)| player.playing());
        let Some(playing) = playing else { return Err("No video is playing behind the words.".to_owned()) };
        let (request, said) = match args.get(1).map(|w| w.to_lowercase()).as_deref() {
            Some("play") if playing => return Ok(Reply::done("The video is playing already.")),
            Some("pause") if !playing => return Ok(Reply::done("The video is paused already.")),
            Some("play") => (VideoRequest::TogglePause, "Playing the video.".to_owned()),
            Some("pause") => (VideoRequest::TogglePause, "Paused the video.".to_owned()),
            Some("restart") => (VideoRequest::Restart, "The video starts again.".to_owned()),
            Some("seek") => {
                let to: f64 = args.get(2).and_then(|w| w.parse().ok()).filter(|s: &f64| *s >= 0.0).ok_or(usage)?;
                let length = self.video.as_ref().and_then(|(_, player)| player.duration());
                match length {
                    None => {
                        return Err(
                            "The video's length isn't known yet (it has only just started); try again in a moment."
                                .to_owned(),
                        );
                    }
                    Some(length) if to > length => return Err(format!("The video is {length:.0} seconds long.")),
                    Some(_) => {}
                }
                let shown = format!("{to:.1}");
                let shown = shown.trim_end_matches(".0");
                (VideoRequest::Seek(to), format!("The video is at {shown} seconds."))
            }
            Some("loop") => {
                let on = on_off(args.get(2))?;
                (VideoRequest::Looping(on), format!("The video {}.", if on { "loops" } else { "stops at its end" }))
            }
            Some("mute") => {
                let on = on_off(args.get(2))?;
                (VideoRequest::Muted(on), format!("The video's sound is {}.", if on { "off" } else { "on" }))
            }
            _ => return Err(usage.to_owned()),
        };
        self.with_state(|state| state.video_request = Some(request));
        self.handle_video();
        Ok(Reply::done(said))
    }

    /// `media next|previous`: the next or previous picture or video in the media bin behind the
    /// words.
    pub(crate) fn media_step(&mut self, forward: bool) -> Result<Reply, String> {
        let count = self.show.media().len();
        if count == 0 {
            return Err("The media bin is empty.".to_owned());
        }
        let next = match self.show.live_media() {
            Some(at) if forward => (at + 1) % count,
            Some(at) => (at + count - 1) % count,
            None => 0,
        };
        self.apply(vec![Command::ShowMedia(next)]);
        Ok(Reply::done(format!("Showing {} behind the words.", self.show.media()[next].name)))
    }

    /// `media set ITEM scaling=fit|fill|stretch loop=on|off`: how a picture or video fills the screen.
    pub(crate) fn media_set(&mut self, words: &[String]) -> Result<Reply, String> {
        let usage = "media set ITEM scaling=fit|fill|stretch loop=on|off";
        let settings: Vec<&String> = words.iter().filter(|w| w.contains('=')).collect();
        let name: Vec<&String> = words.iter().filter(|w| !w.contains('=')).collect();
        if settings.is_empty() || name.is_empty() {
            return Err(usage.to_owned());
        }
        let at = crate::control::find_media(
            self.show.media(),
            &name.iter().map(|w| w.as_str()).collect::<Vec<_>>().join(" "),
        )?;
        let item = self.show.media()[at].clone();
        let (mut fit, mut looping) = (item.fit, item.looping);
        let video = crate::media::is_video(&item.path);
        for setting in settings {
            let (key, value) = setting.split_once('=').unwrap_or_default();
            match key.to_lowercase().as_str() {
                "scaling" | "fit" => {
                    fit = match value.to_lowercase().as_str() {
                        "fit" => sp_model::ImageFit::Fit,
                        "fill" => sp_model::ImageFit::Fill,
                        "stretch" => sp_model::ImageFit::Stretch,
                        other => return Err(format!("scaling is fit, fill or stretch, not {other}.")),
                    }
                }
                "loop" if !video => return Err(format!("{} is a picture: loop is for videos.", item.name)),
                "loop" => looping = on_off(Some(&value.to_owned()))?,
                other => return Err(format!("{other} isn't a media setting: scaling, loop.")),
            }
        }
        let changes = self.show.set_media_look(at, fit, looping);
        if changes.media {
            self.refresh_output();
        }
        self.save_media_list();
        let scaling = match fit {
            sp_model::ImageFit::Fit => "fitted inside the screen",
            sp_model::ImageFit::Fill => "filling the screen",
            sp_model::ImageFit::Stretch => "stretched to the screen",
        };
        if !video {
            return Ok(Reply::done(format!("{}: {scaling}.", item.name)));
        }
        Ok(Reply::done(format!("{}: {scaling}, {}.", item.name, if looping { "looping" } else { "not looping" })))
    }

    /// `stage message TEXT | stage message clear`, and `stage layout SCREEN LAYOUT`.
    pub(crate) fn ctl_stage_more(&mut self, args: &[String]) -> Option<Result<Reply, String>> {
        let word = args.get(1)?.to_lowercase();
        let result = match word.as_str() {
            "message" => {
                let text = args.get(2..).unwrap_or_default().join(" ");
                if text.trim().is_empty() {
                    Err("stage message TEXT | stage message clear".to_owned())
                } else if text.trim().eq_ignore_ascii_case("clear") {
                    self.with_state(|state| state.stage_message_live = false);
                    self.sync_overlay();
                    Ok(Reply::done("The stage message is off."))
                } else {
                    let text = text.replace("\\n", "\n");
                    self.with_state(|state| {
                        state.stage_message = text.clone();
                        state.stage_message_live = true;
                    });
                    self.sync_overlay();
                    Ok(Reply::done(format!("On the stage screens: \u{201c}{}\u{201d}", text.trim())))
                }
            }
            "layout" => self.stage_layout(args.get(2..).unwrap_or_default()),
            "layouts" => self.stage_layouts(),
            "objects" => self.stage_objects(args.get(2..).unwrap_or_default()),
            "picture" => self.stage_picture(args.get(2..).unwrap_or_default()),
            "frame" => self.stage_frame(args.get(2..).unwrap_or_default()),
            _ => return None,
        };
        Some(result)
    }

    fn stage_layout(&mut self, words: &[String]) -> Result<Reply, String> {
        let usage = "stage layout SCREEN LAYOUT   (screens and stage layouts by name or number)";
        let [screen, layout @ ..] = words else { return Err(usage.to_owned()) };
        if layout.is_empty() {
            return Err(usage.to_owned());
        }
        let state = &mut self.running.as_mut().ok_or("The app isn't ready.")?.home_state;
        let screens: Vec<String> = state.screens.iter().map(|s| s.name.clone()).collect();
        let at = find("screen", &screens, screen)?;
        let layouts: Vec<String> = state.stage_layouts.iter().map(|l| l.name.clone()).collect();
        let which = find("stage layout", &layouts, &layout.join(" "))
            .map_err(|problem| format!("{problem} The stage layouts: {}.", layouts.join(", ")))?;
        state.screens[at].stage_layout = Some(state.stage_layouts[which].id);
        state.screens_changed = true;
        Ok(Reply::done(format!("{} shows the stage layout {}.", screens[at], layouts[which])))
    }

    /// A stage layout by its name or number, as `stage layouts` lists them.
    fn stage_layout_at(&self, typed: &str) -> Result<usize, String> {
        let state = &self.running.as_ref().ok_or("The app isn't ready.")?.home_state;
        let layouts: Vec<String> = state.stage_layouts.iter().map(|l| l.name.clone()).collect();
        find("stage layout", &layouts, typed)
            .map_err(|problem| format!("{problem} The stage layouts: {}.", layouts.join(", ")))
    }

    /// Puts a changed stage layout in place, to be saved; the Edit view, if it has the layout
    /// open, opens it again as it now is.
    fn put_stage_layout(&mut self, at: usize, layout: sp_model::ThemeTemplate) {
        self.with_state(|state| {
            state.stage_layouts[at] = layout;
            state.stage_layouts_changed = true;
            let open = state.editing.as_ref().is_some_and(|d| crate::stage_editor::is_stage(d) && d.at.slide == at);
            if open {
                state.editing = crate::stage_editor::draft(state, at);
            }
        });
    }

    /// `stage objects LAYOUT`: a stage layout's objects, back first as they are drawn: what each
    /// shows, and where.
    fn stage_objects(&self, words: &[String]) -> Result<Reply, String> {
        let at = self.stage_layout_at(&words.join(" ")).map_err(|p| format!("stage objects LAYOUT: {p}"))?;
        let state = &self.running.as_ref().ok_or("The app isn't ready.")?.home_state;
        let layout = &state.stage_layouts[at];
        let objects = crate::stage::objects(layout);
        let mut lines = Vec::new();
        let mut data = Vec::new();
        for (number, object) in objects.iter().enumerate() {
            let shows = crate::stage::describe(object, &state.screens);
            let place = object.frame.unwrap_or(sp_model::SlideRect { x: 0.0, y: 0.0, w: 1920.0, h: 1080.0 });
            let marks = match (object.hidden, object.locked) {
                (true, true) => " (hidden, locked)",
                (true, false) => " (hidden)",
                (false, true) => " (locked)",
                (false, false) => "",
            };
            lines.push(format!(
                "{}. {}: {shows}, at x={:.0} y={:.0} w={:.0} h={:.0}{marks}",
                number + 1,
                object.name,
                place.x,
                place.y,
                place.w,
                place.h
            ));
            let screen = match &object.kind {
                sp_model::ObjectKind::Screen { screen, .. } => {
                    let (shown, fell_back) = crate::stage::source_screen(*screen, &state.screens);
                    let name = |id: Option<sp_model::Uuid>| {
                        state.screens.iter().find(|s| Some(s.id) == id).map(|s| s.name.clone())
                    };
                    serde_json::json!({"chosen": screen.map_or("main".to_owned(), |_| name(*screen).unwrap_or_default()),
                        "shows": name(shown), "fell_back": fell_back})
                }
                _ => serde_json::Value::Null,
            };
            data.push(serde_json::json!({
                "object": number + 1,
                "name": object.name,
                "shows": shows,
                "item": crate::stage::item_of(object),
                "screen": screen,
                "frame": {"x": place.x, "y": place.y, "w": place.w, "h": place.h},
                "hidden": object.hidden,
                "locked": object.locked,
            }));
        }
        let text = format!("{}, back first:\n{}", layout.name, lines.join("\n"));
        Ok(Reply::with(text, serde_json::json!(data)))
    }

    /// `stage picture LAYOUT main|SCREEN|off`: the layout's screen picture shows that audience
    /// screen (one is added, at the top right, if it has none); `off` takes its screen pictures
    /// away.
    fn stage_picture(&mut self, words: &[String]) -> Result<Reply, String> {
        let usage = "stage picture LAYOUT main|SCREEN|off   (a picture of what an audience screen shows, live)";
        let [layout, source @ ..] = words else { return Err(usage.to_owned()) };
        if source.is_empty() {
            return Err(usage.to_owned());
        }
        let at = self.stage_layout_at(layout)?;
        let source = source.join(" ");
        let state = &self.running.as_ref().ok_or("The app isn't ready.")?.home_state;
        let mut changed = crate::stage::normalized(&state.stage_layouts[at]);
        let name = changed.name.clone();
        let is_picture = |o: &sp_model::Object| matches!(o.kind, sp_model::ObjectKind::Screen { .. });
        if source.eq_ignore_ascii_case("off") {
            let before = changed.objects.len();
            changed.objects.retain(|o| !is_picture(o));
            let gone = before - changed.objects.len();
            self.put_stage_layout(at, changed);
            return Ok(Reply::done(match gone {
                0 => format!("{name} has no screen picture."),
                1 => format!("Took the screen picture off {name}."),
                n => format!("Took {n} screen pictures off {name}."),
            }));
        }
        let audience: Vec<&crate::screens::Screen> =
            state.screens.iter().filter(|s| s.kind == crate::screens::Kind::Audience).collect();
        let wanted = if source.eq_ignore_ascii_case("main") {
            None
        } else {
            let names: Vec<String> = audience.iter().map(|s| s.name.clone()).collect();
            if let Some(stage) =
                state.screens.iter().find(|s| s.kind == crate::screens::Kind::Stage && same_name(&s.name, &source))
            {
                return Err(format!(
                    "{} is a stage screen; a screen picture shows an audience screen: main, {}.",
                    stage.name,
                    names.join(", ")
                ));
            }
            let found = find("audience screen", &names, &source)
                .map_err(|problem| format!("{problem} The audience screens: main, {}.", names.join(", ")))?;
            Some(audience[found].id)
        };
        let listed: Vec<(sp_model::Uuid, String)> = audience.iter().map(|s| (s.id, s.name.clone())).collect();
        let shown_name = crate::stage_editor::screen_object_name(wanted, &listed);
        let place = match changed.objects.iter_mut().find(|o| is_picture(o)) {
            Some(object) => {
                if let sp_model::ObjectKind::Screen { screen, .. } = &mut object.kind {
                    *screen = wanted;
                }
                object.name = shown_name;
                object.frame
            }
            None => {
                let mut object = sp_model::Object::screen(wanted);
                object.name = shown_name;
                let place = object.frame;
                changed.objects.push(object);
                place
            }
        };
        let describe = |id: Option<sp_model::Uuid>| match id.and_then(|id| listed.iter().find(|(have, _)| *have == id))
        {
            Some((_, name)) => name.clone(),
            None => format!(
                "the main audience screen ({})",
                listed.first().map_or("there is none".to_owned(), |(_, name)| name.clone())
            ),
        };
        let place = place.unwrap_or(sp_model::SlideRect { x: 0.0, y: 0.0, w: 1920.0, h: 1080.0 });
        let said = format!(
            "{name} shows what {} shows, at x={:.0} y={:.0} w={:.0} h={:.0}.",
            describe(wanted),
            place.x,
            place.y,
            place.w,
            place.h
        );
        self.put_stage_layout(at, changed);
        Ok(Reply::done(said))
    }

    /// `stage frame LAYOUT OBJECT x=… y=… w=… h=…`: where one of a stage layout's objects sits,
    /// in the stage's pixels (1920 × 1080). OBJECT is its number in `stage objects` or its name.
    fn stage_frame(&mut self, words: &[String]) -> Result<Reply, String> {
        let usage = "stage frame LAYOUT OBJECT x=… y=… w=… h=…";
        let [layout, object, settings @ ..] = words else { return Err(usage.to_owned()) };
        if settings.is_empty() {
            return Err(usage.to_owned());
        }
        let at = self.stage_layout_at(layout)?;
        let state = &self.running.as_ref().ok_or("The app isn't ready.")?.home_state;
        let current = &state.stage_layouts[at];
        let objects = crate::stage::objects(current);
        let names: Vec<String> = objects.iter().map(|o| o.name.clone()).collect();
        let index = find("object", &names, object)
            .map_err(|problem| format!("{problem} {}'s objects: {}.", current.name, names.join(", ")))?;
        let was = objects[index].frame.unwrap_or(sp_model::SlideRect { x: 0.0, y: 0.0, w: 1920.0, h: 1080.0 });
        let frame = crate::slide_cli::set_frame(was, settings)?;
        let name = names[index].clone();
        let changed = crate::stage::with_frame(current, index, frame);
        let layout_name = current.name.clone();
        self.put_stage_layout(at, changed);
        Ok(Reply::done(format!(
            "{layout_name}: {name} at x={:.0} y={:.0} w={:.0} h={:.0}.",
            frame.x, frame.y, frame.w, frame.h
        )))
    }

    /// `stage layouts`: each stage layout, and the screens that show it.
    fn stage_layouts(&self) -> Result<Reply, String> {
        let state = &self.running.as_ref().ok_or("The app isn't ready.")?.home_state;
        let lines: Vec<String> = state
            .stage_layouts
            .iter()
            .map(|layout| {
                let on: Vec<&str> = state
                    .screens
                    .iter()
                    .filter(|s| s.stage_layout == Some(layout.id))
                    .map(|s| s.name.as_str())
                    .collect();
                if on.is_empty() { layout.name.clone() } else { format!("{} (on {})", layout.name, on.join(", ")) }
            })
            .collect();
        let data: Vec<serde_json::Value> = state
            .stage_layouts
            .iter()
            .map(|layout| {
                let on: Vec<&str> = state
                    .screens
                    .iter()
                    .filter(|s| s.stage_layout == Some(layout.id))
                    .map(|s| s.name.as_str())
                    .collect();
                serde_json::json!({"name": layout.name, "screens": on})
            })
            .collect();
        Ok(Reply::with(crate::control::numbered(&lines), serde_json::json!(data)))
    }

    /// `test bars|grid|white|off`: a test pattern on the audience screens, for lining up a projector.
    pub(crate) fn ctl_test(&mut self, args: &[String]) -> Result<Reply, String> {
        let pattern = match args.get(1).map(|w| w.to_lowercase()).as_deref() {
            Some("bars" | "colorbars" | "colourbars") => Some(crate::props::TestPattern::ColorBars),
            Some("grid") => Some(crate::props::TestPattern::Grid),
            Some("white") => Some(crate::props::TestPattern::White),
            Some("off") => None,
            _ => return Err("test bars | grid | white | off".to_owned()),
        };
        self.with_state(|state| state.test_pattern = pattern);
        self.sync_overlay();
        Ok(Reply::done(match pattern {
            Some(crate::props::TestPattern::ColorBars) => "Colour bars on the audience screens.",
            Some(crate::props::TestPattern::Grid) => "A grid on the audience screens.",
            Some(crate::props::TestPattern::White) => "White on the audience screens.",
            None => "The test pattern is off.",
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::same_name;

    #[test]
    fn names_match_in_any_case_in_any_alphabet() {
        assert!(same_name("Припев", "припев"));
        assert!(same_name("припев", " ПРИПЕВ "));
        assert!(same_name("Verse 1", "verse 1"));
        assert!(same_name("Strophe Ä", "strophe ä"));
        assert!(!same_name("Припев", "Куплет"));
        assert!(!same_name("Припев 1", "Припев"));
    }
}
