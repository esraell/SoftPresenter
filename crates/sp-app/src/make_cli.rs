//! Command-line commands for making things: timers, messages, props, macros, audio tracks and
//! playlists, and media playlists. Each changes what the window's own editors change, marks it
//! to be saved as they do, and says what it did.

use std::path::PathBuf;

use sp_model::{ClearWhat, ShowAction, TimerDo};

use crate::control::{Reply, find};

/// Splits `words … KEYWORD more words` at the last KEYWORD.
fn split_at_word<'a>(words: &'a [String], keyword: &str) -> (&'a [String], Option<String>) {
    match words.iter().rposition(|w| w.eq_ignore_ascii_case(keyword)) {
        Some(at) if at + 1 < words.len() => (&words[..at], Some(words[at + 1..].join(" "))),
        _ => (words, None),
    }
}

/// A length of time as typed: `M:SS`, `H:MM:SS`, or a number of minutes.
pub fn parse_length(typed: &str) -> Result<u32, String> {
    let bad = || format!("{typed} isn't a length of time: say 5:00 (minutes and seconds), 1:15:00, or 30 (minutes).");
    let parts: Vec<&str> = typed.trim().split(':').collect();
    let numbers: Vec<u32> = parts.iter().map(|p| p.parse::<u32>().map_err(|_| bad())).collect::<Result<_, _>>()?;
    let seconds = match numbers.as_slice() {
        [minutes] => minutes * 60,
        [minutes, seconds] if *seconds < 60 => minutes * 60 + seconds,
        [hours, minutes, seconds] if *minutes < 60 && *seconds < 60 => hours * 3600 + minutes * 60 + seconds,
        _ => return Err(bad()),
    };
    if seconds == 0 || seconds > 24 * 3600 {
        return Err(format!("A timer runs for more than nothing and at most a day, not {typed}."));
    }
    Ok(seconds)
}

/// A time of day as typed: `10:30`, `22:30`, `10:30pm`, `9am`.
pub fn parse_time_of_day(typed: &str) -> Result<(u8, u8, u8), String> {
    let bad = || format!("{typed} isn't a time of day: say 10:30, 22:30 or 10:30pm.");
    let lower = typed.trim().to_lowercase();
    let (clock, after_noon) = match (lower.strip_suffix("pm"), lower.strip_suffix("am")) {
        (Some(clock), _) => (clock.trim().to_owned(), Some(true)),
        (_, Some(clock)) => (clock.trim().to_owned(), Some(false)),
        _ => (lower.clone(), None),
    };
    let parts: Vec<u32> = clock.split(':').map(|p| p.parse::<u32>().map_err(|_| bad())).collect::<Result<_, _>>()?;
    let (mut hour, minute, second) = match parts.as_slice() {
        [hour] => (*hour, 0, 0),
        [hour, minute] => (*hour, *minute, 0),
        [hour, minute, second] => (*hour, *minute, *second),
        _ => return Err(bad()),
    };
    match after_noon {
        Some(_) if !(1..=12).contains(&hour) => return Err(bad()),
        Some(true) if hour != 12 => hour += 12,
        Some(false) if hour == 12 => hour = 0,
        _ => {}
    }
    if hour > 23 || minute > 59 || second > 59 {
        return Err(bad());
    }
    Ok((hour as u8, minute as u8, second as u8))
}

/// A timer's kind from `countdown 5:00`, `to 10:30`, or `up [to 20:00]`.
fn parse_kind(words: &[String]) -> Result<crate::timers::Kind, String> {
    use crate::timers::Kind;
    let usage = "countdown M:SS | to HH:MM | up [to M:SS]";
    match words.first().map(|w| w.to_lowercase()).as_deref() {
        Some("countdown" | "down") => {
            let length = words.get(1).ok_or(usage)?;
            Ok(Kind::Countdown { seconds: parse_length(length)? })
        }
        Some("to" | "until") => {
            let (hour, minute, second) = parse_time_of_day(&words[1..].join(""))?;
            Ok(Kind::CountdownTo { hour, minute, second })
        }
        Some("up" | "elapsed") => {
            let end = match words.get(1).map(|w| w.to_lowercase()).as_deref() {
                Some("to") => Some(parse_length(words.get(2).ok_or(usage)?)?),
                Some(_) => return Err(usage.to_owned()),
                None => None,
            };
            Ok(Kind::Elapsed { start: 0, end })
        }
        _ => Err(usage.to_owned()),
    }
}

pub(crate) fn kind_words(kind: &crate::timers::Kind) -> String {
    use crate::timers::Kind;
    match kind {
        Kind::Countdown { seconds } => format!("counts down from {}:{:02}", seconds / 60, seconds % 60),
        Kind::CountdownTo { hour, minute, .. } => format!("counts down to {hour:02}:{minute:02}"),
        Kind::Elapsed { end: Some(end), .. } => format!("counts up to {}:{:02}", end / 60, end % 60),
        Kind::Elapsed { end: None, .. } => "counts up".to_owned(),
    }
}

fn wants_yes(words: &[String]) -> (bool, Vec<String>) {
    (words.iter().any(|w| w == "--yes"), words.iter().filter(|w| *w != "--yes").cloned().collect())
}

/// The first words of the steps `macro add` and `macro step` take.
const STEP_WORDS: [&str; 12] =
    ["clear", "show", "hide", "start", "stop", "reset", "audience", "song", "media", "announce", "canva", "version"];

/// Where the steps begin in `macro add NAME STEP…`, or in `macro step NAME N STEP…` when
/// `numbered` (N just before them): the place of the first step's first word. The name is at
/// least one word, and may hold a step word or a number itself ("Start service", "Macro 1"): a
/// place where the words before it are a macro's name (among `names`) is taken first, then the
/// first place that fits.
fn steps_start(words: &[String], names: &[String], numbered: bool) -> Option<usize> {
    let before = usize::from(numbered);
    let first_word = |at: usize| words[at].split_whitespace().next().unwrap_or_default().to_lowercase();
    let fits = |at: &usize| {
        *at > before
            && STEP_WORDS.contains(&first_word(*at).as_str())
            && (!numbered || words[*at - 1].parse::<usize>().is_ok())
    };
    let named = |at: &usize| {
        let name = words[..*at - before].join(" ");
        names.iter().any(|have| have.eq_ignore_ascii_case(name.trim()))
    };
    (0..words.len()).filter(fits).find(named).or_else(|| (0..words.len()).find(fits))
}

/// The steps in the words after a macro's name, each as its words: they are parted by ";" or by
/// the word "then" (which needs no quoting in a terminal).
fn step_texts(words: &[String]) -> Vec<String> {
    let joined = words
        .iter()
        .map(|w| if w.eq_ignore_ascii_case("then") { ";" } else { w.as_str() })
        .collect::<Vec<_>>()
        .join(" ");
    joined.split(';').map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned).collect()
}

/// `macro move NAME N to M`: the macro's name, the step's number now and the number it is to
/// have.
fn parse_move(words: &[String]) -> Option<(String, usize, usize)> {
    match words {
        [name @ .., from, to, place] if !name.is_empty() && to.eq_ignore_ascii_case("to") => {
            Some((name.join(" "), from.parse().ok()?, place.parse().ok()?))
        }
        _ => None,
    }
}

/// The place (counting from 0) of step `number` (counting from 1) in a macro of `count` steps,
/// or what to say when it has no such step.
fn step_place(name: &str, count: usize, number: usize) -> Result<usize, String> {
    if (1..=count).contains(&number) {
        return Ok(number - 1);
    }
    let plural = if count == 1 { "" } else { "s" };
    Err(format!("{name} has {count} step{plural}; there is no step {number}."))
}

impl crate::App {
    /// `timer new|set|delete …`; `None` for another word.
    pub(crate) fn ctl_timer_make(&mut self, args: &[String]) -> Option<Result<Reply, String>> {
        let word = args.get(1)?.to_lowercase();
        let words = &args[2..];
        let result = match word.as_str() {
            // `timer new NAME countdown 5:00 | to 10:30 | up [to M:SS]`
            "new" => {
                // The name is the words before the first place the rest reads as a kind, so a
                // timer may have "to" or "up" in its name ("Walk to Church countdown 5:00").
                let usage = "timer new NAME countdown M:SS | to HH:MM | up [to M:SS]";
                match (1..words.len()).find(|at| parse_kind(&words[*at..]).is_ok()) {
                    Some(at) => self.timer_new(&words[..at].join(" "), &words[at..]),
                    None => {
                        // Say what was wrong with the last kind typed, if there was one.
                        let keywords = ["countdown", "down", "to", "until", "up", "elapsed"];
                        let last = words
                            .iter()
                            .rposition(|w| keywords.contains(&w.to_lowercase().as_str()))
                            .filter(|at| *at > 0);
                        Err(last.and_then(|at| parse_kind(&words[at..]).err()).unwrap_or_else(|| usage.to_owned()))
                    }
                }
            }
            // `timer set NAME countdown 5:00 | to 10:30 | up | overrun on|off | name NEW`
            "set" => self.timer_set(words),
            // `timer rename OLD to NEW`: the messages that show it are changed with it.
            "rename" => match split_at_word(words, "to") {
                (old, Some(new)) if !old.is_empty() => self.timer_rename(&old.join(" "), &new),
                _ => Err("timer rename OLD to NEW".to_owned()),
            },
            "delete" => {
                let (yes, name) = wants_yes(words);
                self.with_named("timer", &name.join(" "), |state| state.timers.iter().map(|t| t.name.clone()).collect())
                    .and_then(|at| {
                        let state = &mut self.running.as_mut().ok_or("The app isn't ready.")?.home_state;
                        let name = state.timers[at].name.clone();
                        if !yes {
                            return Err(format!("This deletes the timer {name}. Add --yes to do it."));
                        }
                        state.timers.remove(at);
                        state.timers_changed = true;
                        Ok(Reply::done(format!("Deleted the timer {name}.")))
                    })
            }
            _ => return None,
        };
        Some(result)
    }

    /// Which of the window's list (named by `kind`) a name stands for.
    fn with_named(
        &self,
        kind: &str,
        typed: &str,
        names: impl FnOnce(&crate::home::HomeState) -> Vec<String>,
    ) -> Result<usize, String> {
        let state = &self.running.as_ref().ok_or("The app isn't ready.")?.home_state;
        find(kind, &names(state), typed)
    }

    fn timer_new(&mut self, name: &str, kind: &[String]) -> Result<Reply, String> {
        let kind = parse_kind(kind)?;
        let state = &mut self.running.as_mut().ok_or("The app isn't ready.")?.home_state;
        if state.timers.iter().any(|t| t.name.eq_ignore_ascii_case(name)) {
            return Err(format!("There is a timer called {name} already."));
        }
        let mut timer = crate::timers::Timer::new(name.to_owned());
        timer.kind = kind;
        state.timers.push(timer);
        state.timers_changed = true;
        Ok(Reply::done(format!("Made the timer {name}: it {}.", kind_words(&kind))))
    }

    fn timer_rename(&mut self, old: &str, new: &str) -> Result<Reply, String> {
        let index = self.with_named("timer", old, |state| state.timers.iter().map(|t| t.name.clone()).collect())?;
        let state = &mut self.running.as_mut().ok_or("The app isn't ready.")?.home_state;
        let was = state.rename_timer(index, new)?;
        Ok(Reply::done(format!("Renamed the timer {was} to {}.", new.trim())))
    }

    fn timer_set(&mut self, words: &[String]) -> Result<Reply, String> {
        let usage = "timer set NAME countdown M:SS | to HH:MM | up [to M:SS] | overrun on|off | name NEW";
        // As for timer new: the first place the rest reads as a setting.
        let reads = |rest: &[String]| match rest.first().map(|w| w.to_lowercase()).as_deref() {
            Some("overrun") => matches!(rest.get(1).map(|w| w.to_lowercase()).as_deref(), Some("on" | "off")),
            Some("name") => rest.len() > 1,
            _ => parse_kind(rest).is_ok(),
        };
        let at = (1..words.len()).find(|at| reads(&words[*at..])).ok_or(usage)?;
        let index = self.with_named("timer", &words[..at].join(" "), |state| {
            state.timers.iter().map(|t| t.name.clone()).collect()
        })?;
        let rest = &words[at..];
        let state = &mut self.running.as_mut().ok_or("The app isn't ready.")?.home_state;
        let was = state.timers[index].name.clone();
        let said = match rest[0].to_lowercase().as_str() {
            "overrun" => {
                let on = match rest.get(1).map(|w| w.to_lowercase()).as_deref() {
                    Some("on") => true,
                    Some("off") => false,
                    _ => return Err(usage.to_owned()),
                };
                state.timers[index].allow_overrun = on;
                format!("{} past its end", if on { "keeps counting" } else { "stops" })
            }
            // Renamed with the messages that show it, as in the window.
            "name" => {
                let new = rest[1..].join(" ");
                if new.trim().is_empty() {
                    return Err(usage.to_owned());
                }
                state.rename_timer(index, &new)?;
                format!("is now called {}", new.trim())
            }
            _ => {
                let kind = parse_kind(rest)?;
                state.timers[index].kind = kind;
                state.timers[index].run = crate::timers::Run::default();
                kind_words(&kind)
            }
        };
        state.timers_changed = true;
        Ok(Reply::done(format!("The timer {was} {said}.")))
    }

    /// `message new|set|delete …` and `message show --text WORDS`; `None` for another word.
    pub(crate) fn ctl_message_make(&mut self, args: &[String]) -> Option<Result<Reply, String>> {
        let word = args.get(1)?.to_lowercase();
        let words = &args[2..];
        let result = match word.as_str() {
            "new" => match words {
                [name, text @ ..] => self.message_new(name, &text.join(" ")),
                [] => Err("message new NAME [WORDS]   (put a name with spaces in quotes)".to_owned()),
            },
            "set" => self.message_set(words),
            "delete" => {
                let (yes, name) = wants_yes(words);
                self.with_named("message", &name.join(" "), |state| {
                    state.messages.iter().map(|m| m.name.clone()).collect()
                })
                .and_then(|at| {
                    let state = &mut self.running.as_mut().ok_or("The app isn't ready.")?.home_state;
                    let name = state.messages[at].name.clone();
                    if !yes {
                        return Err(format!("This deletes the message {name}. Add --yes to do it."));
                    }
                    state.messages.remove(at);
                    state.messages_changed = true;
                    Ok(Reply::done(format!("Deleted the message {name}.")))
                })
            }
            // `message show --text WORDS`: words shown once, kept as the message "Quick message".
            "show" if words.first().is_some_and(|w| w == "--text") => self.message_quick(&words[1..].join(" ")),
            _ => return None,
        };
        Some(result)
    }

    fn message_new(&mut self, name: &str, text: &str) -> Result<Reply, String> {
        let state = &mut self.running.as_mut().ok_or("The app isn't ready.")?.home_state;
        if state.messages.iter().any(|m| m.name.eq_ignore_ascii_case(name)) {
            return Err(format!("There is a message called {name} already."));
        }
        let mut message = crate::messages::Message::new(name.to_owned());
        message.text = text.replace("\\n", "\n");
        state.messages.push(message);
        state.messages_changed = true;
        let words =
            if text.trim().is_empty() { "no words yet".to_owned() } else { format!("\u{201c}{}\u{201d}", text.trim()) };
        Ok(Reply::done(format!("Made the message {name}: {words}.")))
    }

    fn message_set(&mut self, words: &[String]) -> Result<Reply, String> {
        let usage = "message set NAME text=WORDS | leaves=SECONDS|never | name=NEW   (quote a setting with spaces)";
        let at = words.iter().position(|w| w.contains('=')).filter(|at| *at > 0).ok_or(usage)?;
        let index = self.with_named("message", &words[..at].join(" "), |state| {
            state.messages.iter().map(|m| m.name.clone()).collect()
        })?;
        let state = &mut self.running.as_mut().ok_or("The app isn't ready.")?.home_state;
        let was = state.messages[index].name.clone();
        let mut said = Vec::new();
        for setting in &words[at..] {
            let (key, value) = setting.split_once('=').ok_or(usage)?;
            let message = &mut state.messages[index];
            match key.to_lowercase().as_str() {
                "text" | "words" => {
                    message.text = value.replace("\\n", "\n");
                    said.push(format!("words \u{201c}{value}\u{201d}"));
                }
                "leaves" | "dismiss" => {
                    message.dismiss_after = match value.to_lowercase().as_str() {
                        "never" | "no" | "off" => None,
                        seconds => Some(
                            seconds
                                .parse::<u32>()
                                .ok()
                                .filter(|s| *s > 0)
                                .ok_or_else(|| format!("leaves is a number of seconds, or never, not {value}."))?,
                        ),
                    };
                    said.push(
                        message
                            .dismiss_after
                            .map_or("stays until cleared".to_owned(), |s| format!("leaves after {s} s")),
                    );
                }
                "name" => {
                    message.name = value.trim().to_owned();
                    said.push(format!("now called {}", value.trim()));
                }
                other => return Err(format!("{other} isn't a message setting: text, leaves, name.")),
            }
        }
        state.messages_changed = true;
        Ok(Reply::done(format!("The message {was}: {}.", said.join(", "))))
    }

    fn message_quick(&mut self, text: &str) -> Result<Reply, String> {
        if text.trim().is_empty() {
            return Err("message show --text WORDS".to_owned());
        }
        const QUICK: &str = "Quick message";
        let state = &mut self.running.as_mut().ok_or("The app isn't ready.")?.home_state;
        let at = match state.messages.iter().position(|m| m.name == QUICK) {
            Some(at) => at,
            None => {
                state.messages.push(crate::messages::Message::new(QUICK.to_owned()));
                state.messages.len() - 1
            }
        };
        state.messages[at].text = text.replace("\\n", "\n");
        state.messages_changed = true;
        let id = state.messages[at].id;
        self.run_action(ShowAction::Message { id, show: true });
        Ok(Reply::done(format!("Showing \u{201c}{}\u{201d} (kept as the message {QUICK}).", text.trim())))
    }

    /// `prop new NAME [PICTURE]`, `prop delete NAME --yes`; `None` for another word.
    pub(crate) fn ctl_prop_make(&mut self, args: &[String]) -> Option<Result<Reply, String>> {
        let word = args.get(1)?.to_lowercase();
        let words = &args[2..];
        let result = match word.as_str() {
            "new" => self.prop_new(words),
            "delete" => {
                let (yes, name) = wants_yes(words);
                self.with_named("prop", &name.join(" "), |state| state.props.iter().map(|p| p.name.clone()).collect())
                    .and_then(|at| {
                        let state = &mut self.running.as_mut().ok_or("The app isn't ready.")?.home_state;
                        let name = state.props[at].name.clone();
                        if !yes {
                            return Err(format!("This deletes the prop {name}. Add --yes to do it."));
                        }
                        let gone = state.props.remove(at);
                        state.props_live.retain(|(id, _)| *id != gone.id);
                        state.props_changed = true;
                        Ok(Reply::done(format!("Deleted the prop {name}.")))
                    })
            }
            _ => return None,
        };
        Some(result)
    }

    fn prop_new(&mut self, words: &[String]) -> Result<Reply, String> {
        let usage = "prop new NAME [PICTURE]   (words of its name, or a picture filling it)";
        let (name_words, picture) = match words.last() {
            Some(last) if words.len() > 1 && std::path::Path::new(last).is_file() => {
                (&words[..words.len() - 1], Some(PathBuf::from(last)))
            }
            // A last word that names a file that isn't there is a mistake, not part of the name.
            Some(last)
                if words.len() > 1 && (last.contains('/') || crate::media::is_media(std::path::Path::new(last))) =>
            {
                return Err(format!("There is no file {last}."));
            }
            _ => (words, None),
        };
        let name = name_words.join(" ");
        if name.trim().is_empty() {
            return Err(usage.to_owned());
        }
        let mut prop = crate::props::Prop::new(name.trim().to_owned());
        let said = match &picture {
            Some(path) => {
                if !crate::media::is_media(path) || crate::media::is_video(path) {
                    return Err(format!("{} isn't a picture.", path.display()));
                }
                let imported = crate::images::import(path)?;
                let object = sp_model::Object::image(imported.file, imported.name, imported.width, imported.height);
                prop.slide = sp_model::Slide::new(vec![object]);
                "a picture".to_owned()
            }
            None => "its name in words".to_owned(),
        };
        let state = &mut self.running.as_mut().ok_or("The app isn't ready.")?.home_state;
        if state.props.iter().any(|p| p.name.eq_ignore_ascii_case(name.trim())) {
            return Err(format!("There is a prop called {} already.", name.trim()));
        }
        state.props.push(prop);
        state.props_changed = true;
        Ok(Reply::done(format!("Made the prop {}, showing {said}.", name.trim())))
    }

    /// `macro new|add|step|move|startup|remove|delete …`; `None` for another word.
    pub(crate) fn ctl_macro_make(&mut self, args: &[String]) -> Option<Result<Reply, String>> {
        let word = args.get(1)?.to_lowercase();
        let words = &args[2..];
        let result = match word.as_str() {
            "new" => {
                let name = words.join(" ");
                let name = name.trim();
                match self.running.as_mut().map(|r| &mut r.home_state) {
                    _ if name.is_empty() => Err("macro new NAME".to_owned()),
                    Some(state) if state.macros.iter().any(|m| m.name.eq_ignore_ascii_case(name)) => {
                        Err(format!("There is a macro called {name} already."))
                    }
                    Some(state) => {
                        state.macros.push(crate::macros::Macro::new(name.to_owned()));
                        state.macros_changed = true;
                        Ok(Reply::done(format!("Made the macro {name}; give it steps with macro add {name} STEP.")))
                    }
                    None => Err("The app isn't ready.".to_owned()),
                }
            }
            // `macro add NAME STEP [; STEP…]`
            "add" => self.macro_add(words),
            // `macro step NAME N STEP`: step N does this instead, at the same point.
            "step" => self.macro_step(words),
            // `macro move NAME N to M`: step N becomes step M.
            "move" => match parse_move(words) {
                Some((name, from, to)) => self.macro_move(&name, from, to),
                None => Err("macro move NAME N to M".to_owned()),
            },
            // `macro startup NAME on|off`: run it when SoftPresenter opens, or not.
            "startup" => match words {
                [name @ .., on]
                    if !name.is_empty() && (on.eq_ignore_ascii_case("on") || on.eq_ignore_ascii_case("off")) =>
                {
                    let on = on.eq_ignore_ascii_case("on");
                    self.with_named("macro", &name.join(" "), |state| {
                        state.macros.iter().map(|m| m.name.clone()).collect()
                    })
                    .and_then(|at| {
                        let state = &mut self.running.as_mut().ok_or("The app isn't ready.")?.home_state;
                        state.macros[at].on_startup = on;
                        state.macros_changed = true;
                        let name = &state.macros[at].name;
                        Ok(Reply::done(if on {
                            format!("{name} runs when SoftPresenter opens.")
                        } else {
                            format!("{name} no longer runs when SoftPresenter opens.")
                        }))
                    })
                }
                _ => Err("macro startup NAME on|off".to_owned()),
            },
            // `macro remove NAME N`
            "remove" => match words {
                [name @ .., number] if !name.is_empty() => {
                    let number = number.parse::<usize>().map_err(|_| "macro remove NAME N".to_owned());
                    number.and_then(|number| {
                        let at = self.with_named("macro", &name.join(" "), |state| {
                            state.macros.iter().map(|m| m.name.clone()).collect()
                        })?;
                        let state = &mut self.running.as_mut().ok_or("The app isn't ready.")?.home_state;
                        let place = step_place(&state.macros[at].name, state.macros[at].actions.len(), number)?;
                        state.macros[at].actions.remove(place);
                        state.macros_changed = true;
                        Ok(Reply::done(format!("Took step {number} out of {}.", state.macros[at].name)))
                    })
                }
                _ => Err("macro remove NAME N".to_owned()),
            },
            "delete" => {
                let (yes, name) = wants_yes(words);
                self.with_named("macro", &name.join(" "), |state| state.macros.iter().map(|m| m.name.clone()).collect())
                    .and_then(|at| {
                        let state = &mut self.running.as_mut().ok_or("The app isn't ready.")?.home_state;
                        let name = state.macros[at].name.clone();
                        if !yes {
                            return Err(format!("This deletes the macro {name}. Add --yes to do it."));
                        }
                        state.macros.remove(at);
                        state.macros_changed = true;
                        Ok(Reply::done(format!("Deleted the macro {name}.")))
                    })
            }
            _ => return None,
        };
        Some(result)
    }

    /// The macros' names, in order.
    fn macro_names(&self) -> Vec<String> {
        self.running.as_ref().map(|r| r.home_state.macros.iter().map(|m| m.name.clone()).collect()).unwrap_or_default()
    }

    fn macro_add(&mut self, words: &[String]) -> Result<Reply, String> {
        let usage = "macro add NAME STEP [then STEP…]   steps: clear all|slide|media|props|messages, show|hide message NAME, \
                     show|hide prop NAME, start|stop|reset timer NAME, audience on|off, song SONG, media ITEM, \
                     announce SONG|media ITEM|media-playlist NAME, canva sync, version NAME [keep]|usual";
        let names = self.macro_names();
        let at = steps_start(words, &names, false).ok_or(usage)?;
        let index = find("macro", &names, &words[..at].join(" "))?;
        let actions =
            step_texts(&words[at..]).iter().map(|step| self.parse_step(step)).collect::<Result<Vec<_>, _>>()?;
        if actions.is_empty() {
            return Err(usage.to_owned());
        }
        let said: Vec<String> = actions.iter().map(|action| self.action_words(action)).collect();
        let state = &mut self.running.as_mut().ok_or("The app isn't ready.")?.home_state;
        state.macros[index].actions.extend(actions);
        state.macros_changed = true;
        Ok(Reply::done(format!("{}: added {}.", state.macros[index].name, said.join("; "))))
    }

    /// `macro step NAME N STEP`: step N of the macro does STEP instead (in the words `macro add`
    /// takes), at the same point among its steps.
    fn macro_step(&mut self, words: &[String]) -> Result<Reply, String> {
        let usage = "macro step NAME N STEP   (STEP in the words macro add takes)";
        let names = self.macro_names();
        let at = steps_start(words, &names, true).ok_or(usage)?;
        let number = words[at - 1].parse::<usize>().map_err(|_| usage.to_owned())?;
        let index = find("macro", &names, &words[..at - 1].join(" "))?;
        let place = {
            let made = &self.running.as_ref().ok_or("The app isn't ready.")?.home_state.macros[index];
            step_place(&made.name, made.actions.len(), number)?
        };
        let step = match step_texts(&words[at..]).as_slice() {
            [step] => self.parse_step(step)?,
            [] => return Err(usage.to_owned()),
            _ => return Err("macro step changes one step; add more with macro add.".to_owned()),
        };
        let now = self.action_words(&step);
        let state = &mut self.running.as_mut().ok_or("The app isn't ready.")?.home_state;
        let was = state.macros[index].replace_step(place, step).ok_or(usage)?;
        state.macros_changed = true;
        let name = state.macros[index].name.clone();
        let was = self.action_words(&was);
        Ok(Reply::done(format!("{name}: step {number} is now {now} (it was {was}).")))
    }

    /// `macro move NAME N to M`: step N of the macro becomes step M, the steps between closing up.
    fn macro_move(&mut self, name: &str, from: usize, to: usize) -> Result<Reply, String> {
        let index = find("macro", &self.macro_names(), name)?;
        let state = &mut self.running.as_mut().ok_or("The app isn't ready.")?.home_state;
        let made = &mut state.macros[index];
        let (count, name) = (made.actions.len(), made.name.clone());
        let (place, target) = (step_place(&name, count, from)?, step_place(&name, count, to)?);
        if !made.move_step(place, target) {
            return Ok(Reply::done(format!("{name}: step {from} is there already.")));
        }
        state.macros_changed = true;
        let step = state.macros[index].actions[target].clone();
        Ok(Reply::done(format!("{name}: {} is now step {to}.", self.action_words(&step))))
    }

    /// One step of a macro, as words: the slide-action vocabulary.
    fn parse_step(&self, step: &str) -> Result<ShowAction, String> {
        let words: Vec<&str> = step.split_whitespace().collect();
        let rest = |from: usize| words.get(from..).map(|w| w.join(" ")).unwrap_or_default();
        let state = &self.running.as_ref().ok_or("The app isn't ready.")?.home_state;
        let named = |kind: &str, names: Vec<String>, typed: String| find(kind, &names, &typed);
        let first = words.first().map(|w| w.to_lowercase()).unwrap_or_default();
        let second = words.get(1).map(|w| w.to_lowercase()).unwrap_or_default();
        match (first.as_str(), second.as_str()) {
            ("clear", what) => {
                let what = match what {
                    "all" | "" => ClearWhat::All,
                    "slide" | "slides" => ClearWhat::Slide,
                    "media" => ClearWhat::Media,
                    "props" | "prop" => ClearWhat::Props,
                    "messages" | "message" => ClearWhat::Messages,
                    other => return Err(format!("clear {other}? Say clear all, slide, media, props or messages.")),
                };
                Ok(ShowAction::Clear { what })
            }
            ("show" | "hide", "message") => {
                let at = named("message", state.messages.iter().map(|m| m.name.clone()).collect(), rest(2))?;
                Ok(ShowAction::Message { id: state.messages[at].id, show: first == "show" })
            }
            ("show" | "hide", "prop") => {
                let at = named("prop", state.props.iter().map(|p| p.name.clone()).collect(), rest(2))?;
                Ok(ShowAction::Prop { id: state.props[at].id, show: first == "show" })
            }
            ("start" | "stop" | "reset", "timer") => {
                let at = named("timer", state.timers.iter().map(|t| t.name.clone()).collect(), rest(2))?;
                let act = match first.as_str() {
                    "start" => TimerDo::Start,
                    "stop" => TimerDo::Stop,
                    _ => TimerDo::Reset,
                };
                Ok(ShowAction::Timer { id: state.timers[at].id, act })
            }
            ("audience", "on" | "off") => Ok(ShowAction::Audience { on: second == "on" }),
            ("song", _) => {
                let at = self.find_song_as_is(&rest(1))?;
                let key = state.song_keys.get(at).cloned().ok_or("That song can't be gone to.")?;
                Ok(ShowAction::Song { key })
            }
            ("media", _) => {
                let names: Vec<String> = self.show.media().iter().map(|m| m.name.clone()).collect();
                let at = named("media item", names, rest(1))?;
                Ok(ShowAction::Media { id: self.show.media()[at].id })
            }
            // `announce media ITEM`, `announce media-playlist NAME`, `announce SONG`.
            ("announce", "media") if words.len() > 2 => {
                let names: Vec<String> = self.show.media().iter().map(|m| m.name.clone()).collect();
                let at = named("media item", names, rest(2))?;
                Ok(ShowAction::Announce { what: sp_model::Announced::Media { id: self.show.media()[at].id } })
            }
            ("announce", "media-playlist") => {
                let at =
                    named("media playlist", state.media_playlists.iter().map(|p| p.name.clone()).collect(), rest(2))?;
                Ok(ShowAction::Announce {
                    what: sp_model::Announced::MediaPlaylist { id: state.media_playlists[at].id },
                })
            }
            ("canva", "sync") => Ok(ShowAction::CanvaSync),
            // `version usual`, `version NAME`, `version NAME keep` (kept until switched back).
            ("version", "usual" | "off") => Ok(ShowAction::Version { id: None, keep: false }),
            ("version", _) => {
                let keep = words.last().is_some_and(|w| w.eq_ignore_ascii_case("keep"));
                let name = words.get(1..words.len() - usize::from(keep)).map(|w| w.join(" ")).unwrap_or_default();
                let names: Vec<String> = state.versions.iter().map(|v| v.name.clone()).collect();
                let at = named("version", names, name)?;
                Ok(ShowAction::Version { id: Some(state.versions[at].id), keep })
            }
            ("announce", _) => {
                let at = self.find_song_as_is(&rest(1))?;
                let key = state.song_keys.get(at).cloned().ok_or("That song can't be played as announcements.")?;
                Ok(ShowAction::Announce { what: sp_model::Announced::Song { key } })
            }
            _ => Err(format!("{step}: not a step SoftPresenter knows.")),
        }
    }

    /// `audio add|next|previous|shuffle|through|crossfade|playlist …`; `None` for another word.
    pub(crate) fn ctl_audio_make(&mut self, args: &[String]) -> Option<Result<Reply, String>> {
        let word = args.get(1)?.to_lowercase();
        let words = &args[2..];
        let result = match word.as_str() {
            "add" => self.audio_add(words),
            "next" => self.audio_step(true),
            "previous" | "prev" | "back" => self.audio_step(false),
            "shuffle" | "through" => {
                let on = match words.first().map(|w| w.to_lowercase()).as_deref() {
                    Some("on") => Ok(true),
                    Some("off") => Ok(false),
                    _ => Err(format!("audio {word} on|off")),
                };
                on.and_then(|on| {
                    let state = &mut self.running.as_mut().ok_or("The app isn't ready.")?.home_state;
                    let said = if word == "shuffle" {
                        state.audio.shuffle = on;
                        if on {
                            "After a track, a random other one of its playlist plays."
                        } else {
                            "Tracks play in order."
                        }
                    } else {
                        state.audio.through = on;
                        if on { "After a track, the next one plays." } else { "Each track stops at its end." }
                    };
                    state.audio_changed = true;
                    Ok(Reply::done(said))
                })
            }
            "crossfade" => {
                let seconds = words
                    .first()
                    .and_then(|w| w.trim_end_matches('s').parse::<f32>().ok())
                    .filter(|s| (0.0..=12.0).contains(s));
                match seconds {
                    Some(seconds) => {
                        self.with_state(|state| {
                            state.audio.crossfade = seconds;
                            state.audio_changed = true;
                        });
                        Ok(Reply::done(if seconds == 0.0 {
                            "No crossfade: each track starts as the one before ends.".to_owned()
                        } else {
                            format!("The next track fades in over the last {seconds} s of the one before.")
                        }))
                    }
                    None => Err("audio crossfade SECONDS   (0 to 12)".to_owned()),
                }
            }
            "playlist" => self.audio_playlist(words),
            "playlists" => {
                let state = self.running.as_ref().map(|r| &r.home_state);
                let names: Vec<String> = state
                    .map(|s| {
                        s.audio.playlists.iter().map(|p| format!("{} ({} tracks)", p.name, p.tracks.len())).collect()
                    })
                    .unwrap_or_default();
                Ok(Reply::with(crate::control::numbered(&names), serde_json::json!(names)))
            }
            _ => return None,
        };
        Some(result)
    }

    fn audio_add(&mut self, words: &[String]) -> Result<Reply, String> {
        let (files, playlist) = split_at_word(words, "to");
        if files.is_empty() {
            return Err("audio add FILE… [to PLAYLIST]".to_owned());
        }
        let mut tracks = Vec::new();
        let mut refused = Vec::new();
        for file in files {
            let path = PathBuf::from(file);
            let name = path.file_name().map_or_else(|| file.clone(), |n| n.to_string_lossy().into_owned());
            if !path.is_file() {
                refused.push(format!("{name} isn't there"));
            } else if !crate::audio::is_audio(&path) {
                refused.push(format!("{name} isn't a sound"));
            } else {
                tracks.push(crate::audio::Track::of(path));
            }
        }
        let state = &mut self.running.as_mut().ok_or("The app isn't ready.")?.home_state;
        if tracks.is_empty() {
            return Err(refused.join("; ") + ".");
        }
        if state.audio.playlists.is_empty() {
            state.audio.playlists.push(crate::audio::Playlist::new("Audio".to_owned()));
        }
        let at = match playlist {
            Some(typed) => {
                let names: Vec<String> = state.audio.playlists.iter().map(|p| p.name.clone()).collect();
                find("audio playlist", &names, &typed)?
            }
            None => state.audio.current.min(state.audio.playlists.len() - 1),
        };
        let names: Vec<String> = tracks.iter().map(|t| t.name.clone()).collect();
        state.audio.playlists[at].tracks.extend(tracks);
        state.audio_changed = true;
        let mut text = format!("Added to {}: {}.", state.audio.playlists[at].name, names.join(", "));
        if !refused.is_empty() {
            text.push_str(&format!(" Not added: {}.", refused.join("; ")));
        }
        Ok(Reply::done(text))
    }

    fn audio_step(&mut self, forward: bool) -> Result<Reply, String> {
        let state = &self.running.as_ref().ok_or("The app isn't ready.")?.home_state;
        let playing = state.audio_playing.track.ok_or("No track is playing.")?;
        let (list, at) = state.audio.place_of(playing).ok_or("The track playing isn't in a playlist.")?;
        let tracks = &list.tracks;
        if tracks.is_empty() {
            return Err("The track playing isn't in a playlist.".to_owned());
        }
        // From one taken out while it plays, the track now at its place is the next.
        let listed = tracks.iter().any(|track| track.id == playing);
        let next = match (forward, listed) {
            (true, true) => (at + 1) % tracks.len(),
            (true, false) => at % tracks.len(),
            (false, _) => (at + tracks.len() - 1) % tracks.len(),
        };
        let (id, name) = (tracks[next].id, tracks[next].name.clone());
        self.with_state(|state| state.audio_request = Some(crate::home::AudioRequest::Play(id)));
        self.handle_audio();
        Ok(Reply::done(format!("Playing {name}.")))
    }

    fn audio_playlist(&mut self, words: &[String]) -> Result<Reply, String> {
        let usage = "audio playlist new NAME | audio playlist delete NAME --yes";
        let state = &mut self.running.as_mut().ok_or("The app isn't ready.")?.home_state;
        match words.first().map(|w| w.to_lowercase()).as_deref() {
            Some("new") => {
                let name = words[1..].join(" ");
                if name.trim().is_empty() {
                    return Err(usage.to_owned());
                }
                if state.audio.playlists.iter().any(|p| p.name.eq_ignore_ascii_case(name.trim())) {
                    return Err(format!("There is an audio playlist called {} already.", name.trim()));
                }
                state.audio.playlists.push(crate::audio::Playlist::new(name.trim().to_owned()));
                state.audio_changed = true;
                Ok(Reply::done(format!("Made the audio playlist {}.", name.trim())))
            }
            Some("delete") => {
                let (yes, name) = wants_yes(&words[1..]);
                let names: Vec<String> = state.audio.playlists.iter().map(|p| p.name.clone()).collect();
                let at = find("audio playlist", &names, &name.join(" "))?;
                if !yes {
                    return Err(format!(
                        "This deletes the audio playlist {} (the sound files stay). Add --yes to do it.",
                        names[at]
                    ));
                }
                if !state.audio.remove_playlist(at, state.audio_playing.track) {
                    return Err(format!("{} is the only audio playlist; there is always one.", names[at]));
                }
                state.audio.current = state.audio.current.min(state.audio.playlists.len().saturating_sub(1));
                state.audio_changed = true;
                Ok(Reply::done(format!("Deleted the audio playlist {}.", names[at])))
            }
            _ => Err(usage.to_owned()),
        }
    }

    /// `media-playlist new|add|remove|follow|delete …`; `None` for another word.
    pub(crate) fn ctl_media_playlist_make(&mut self, args: &[String]) -> Option<Result<Reply, String>> {
        let word = args.get(1)?.to_lowercase();
        let words = &args[2..];
        let result = match word.as_str() {
            "new" => {
                let name = words.join(" ");
                match self.running.as_mut().map(|r| &mut r.home_state) {
                    _ if name.trim().is_empty() => Err("media-playlist new NAME".to_owned()),
                    Some(state) if state.media_playlists.iter().any(|p| p.name.eq_ignore_ascii_case(name.trim())) => {
                        Err(format!("There is a media playlist called {} already.", name.trim()))
                    }
                    Some(state) => {
                        state.media_playlists.push(crate::media::Playlist::new(name.trim().to_owned()));
                        state.media_playlists_changed = true;
                        Ok(Reply::done(format!("Made the media playlist {}.", name.trim())))
                    }
                    None => Err("The app isn't ready.".to_owned()),
                }
            }
            "add" | "remove" => {
                let keyword = if word == "add" { "to" } else { "from" };
                match split_at_word(words, keyword) {
                    (items, Some(list)) if !items.is_empty() => self.media_playlist_items(items, &list, word == "add"),
                    _ => Err(format!(
                        "media-playlist {word} ITEM… {keyword} PLAYLIST   (media bin items by name; quote names with spaces)"
                    )),
                }
            }
            "follow" => match words {
                [name @ .., folder] if !name.is_empty() => self.media_playlist_follow(&name.join(" "), folder),
                _ => Err("media-playlist follow PLAYLIST FOLDER|off".to_owned()),
            },
            "delete" => {
                let (yes, name) = wants_yes(words);
                self.with_named("media playlist", &name.join(" "), |state| state.media_playlists.iter().map(|p| p.name.clone()).collect())
                    .and_then(|at| {
                        let state = &mut self.running.as_mut().ok_or("The app isn't ready.")?.home_state;
                        let name = state.media_playlists[at].name.clone();
                        if !yes {
                            return Err(format!("This deletes the media playlist {name} (its pictures and videos stay in the bin). Add --yes to do it."));
                        }
                        state.media_playlists.remove(at);
                        state.media_playlists_changed = true;
                        Ok(Reply::done(format!("Deleted the media playlist {name}.")))
                    })
            }
            _ => return None,
        };
        Some(result)
    }

    fn media_playlist_items(&mut self, items: &[String], list: &str, add: bool) -> Result<Reply, String> {
        let at = self.with_named("media playlist", list, |state| {
            state.media_playlists.iter().map(|p| p.name.clone()).collect()
        })?;
        let names: Vec<String> = self.show.media().iter().map(|m| m.name.clone()).collect();
        let mut picked = Vec::new();
        for typed in items {
            let index = find("media item", &names, typed)?;
            picked.push((self.show.media()[index].path.clone(), names[index].clone()));
        }
        let state = &mut self.running.as_mut().ok_or("The app isn't ready.")?.home_state;
        let playlist = &mut state.media_playlists[at];
        let mut done = Vec::new();
        for (path, name) in picked {
            if playlist.has(&path) != add {
                playlist.toggle(&path);
                done.push(name);
            }
        }
        let list = playlist.name.clone();
        if done.is_empty() {
            return Err(format!(
                "Nothing changed: {}.",
                if add { format!("in {list} already") } else { format!("not in {list}") }
            ));
        }
        state.media_playlists_changed = true;
        Ok(Reply::done(if add {
            format!("Added to {list}: {}.", done.join(", "))
        } else {
            format!("Took out of {list}: {}.", done.join(", "))
        }))
    }

    fn media_playlist_follow(&mut self, name: &str, folder: &str) -> Result<Reply, String> {
        let at = self.with_named("media playlist", name, |state| {
            state.media_playlists.iter().map(|p| p.name.clone()).collect()
        })?;
        let following = if folder.eq_ignore_ascii_case("off") {
            None
        } else {
            let path = PathBuf::from(folder);
            if !path.is_dir() {
                return Err(format!("There is no folder {folder}."));
            }
            Some(path)
        };
        let state = &mut self.running.as_mut().ok_or("The app isn't ready.")?.home_state;
        let list = state.media_playlists[at].name.clone();
        state.media_playlists[at].folder = following.clone();
        state.media_playlists_changed = true;
        if following.is_some() {
            self.check_folders(true);
        }
        Ok(Reply::done(match following {
            Some(path) => format!(
                "{list} follows {}: its pictures and videos come and go with the folder's.",
                path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned())
            ),
            None => format!("{list} no longer follows a folder."),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(text: &str) -> Vec<String> {
        text.split(' ').map(str::to_owned).collect()
    }

    #[test]
    fn a_macros_name_is_told_from_its_steps() {
        let names: Vec<String> = ["Macro 1", "Start service", "Before service"].map(str::to_owned).to_vec();
        let at = |text: &str, numbered: bool| steps_start(&words(text), &names, numbered);
        // macro add NAME STEP…
        assert_eq!(at("Before service announce media-playlist Loop", false), Some(2));
        assert_eq!(at("Opening canva sync", false), Some(1));
        assert_eq!(at("Start service clear all then start timer Sermon", false), Some(2), "a step word in the name");
        assert_eq!(at("Macro 1 show prop Logo", false), Some(2));
        assert_eq!(at("clear all", false), None, "a name is needed");
        // macro step NAME N STEP: the number just before the step.
        assert_eq!(at("Macro 1 2 show prop Logo", true), Some(3), "a number in the name");
        assert_eq!(at("Start service 1 hide message Phones", true), Some(3));
        assert_eq!(at("Opening show prop Logo", true), None, "no step number");
        assert_eq!(at("3 show prop Logo", true), None, "no name");
        assert_eq!(step_texts(&words("clear all then start timer Sermon")), ["clear all", "start timer Sermon"]);
        assert_eq!(
            step_texts(&["audience off; song Amazing Grace".to_owned()]),
            ["audience off", "song Amazing Grace"]
        );
    }

    #[test]
    fn macro_move_reads_which_step_and_where_it_goes() {
        assert_eq!(parse_move(&words("Macro 1 3 to 1")), Some(("Macro 1".to_owned(), 3, 1)));
        assert_eq!(parse_move(&words("Opening 1 TO 4")), Some(("Opening".to_owned(), 1, 4)));
        assert_eq!(parse_move(&words("Opening 3 1")), None, "to is needed");
        assert_eq!(parse_move(&words("3 to 1")), None, "a name is needed");
        assert_eq!(parse_move(&words("Opening three to 1")), None);
        assert_eq!(step_place("Opening", 3, 3), Ok(2));
        assert_eq!(step_place("Opening", 3, 4), Err("Opening has 3 steps; there is no step 4.".to_owned()));
        assert_eq!(step_place("Opening", 1, 0), Err("Opening has 1 step; there is no step 0.".to_owned()));
    }

    #[test]
    fn lengths_and_times_of_day_are_read_as_people_write_them() {
        assert_eq!(parse_length("5:00").unwrap(), 300);
        assert_eq!(parse_length("1:15:00").unwrap(), 4500);
        assert_eq!(parse_length("30").unwrap(), 1800);
        assert!(parse_length("5:75").is_err());
        assert!(parse_length("0").is_err());
        assert!(parse_length("soon").is_err());
        assert_eq!(parse_time_of_day("10:30").unwrap(), (10, 30, 0));
        assert_eq!(parse_time_of_day("10:30pm").unwrap(), (22, 30, 0));
        assert_eq!(parse_time_of_day("12am").unwrap(), (0, 0, 0));
        assert_eq!(parse_time_of_day("12:15pm").unwrap(), (12, 15, 0));
        assert!(parse_time_of_day("25:00").is_err());
        assert!(parse_time_of_day("13pm").is_err());
    }
}
