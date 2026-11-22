//! Command-line commands for screens (adding, changing, deleting), the settings kept between
//! runs, and the Reflow view. Each changes the window's own state, which is saved as the
//! window saves it, and says what it did.

use crate::control::{Reply, find};
use crate::screens::{DeckLinkOut, Keyer, Kind, Output, Screen};

/// A change to the window's state, made once every setting asked for has been read.
type Change = Box<dyn FnOnce(&mut crate::home::HomeState)>;

fn on_off(key: &str, value: &str) -> Result<bool, String> {
    match value.to_lowercase().as_str() {
        "on" | "yes" | "true" => Ok(true),
        "off" | "no" | "false" => Ok(false),
        other => Err(format!("{key} is on or off, not {other}.")),
    }
}

/// Where a screen goes, as typed: `display NAME`, `window`, `ndi NAME`, `decklink NAME|NUMBER`,
/// `automatic` or `off`. A DeckLink output is one of `found`'s, by its name or its number in
/// `sp decklink`; `current` is the output the screen has, whose format is kept when it names
/// the same device.
fn parse_output(words: &[String], found: &sp_decklink::Found, current: Option<&Output>) -> Result<Output, String> {
    let usage = "display NAME | window | ndi NAME | decklink NAME|NUMBER | automatic | off";
    let rest = words.get(1..).map(|w| w.join(" ")).unwrap_or_default();
    match words.first().map(|w| w.to_lowercase()).as_deref() {
        Some("display") if !rest.trim().is_empty() => Ok(Output::Display(rest.trim().to_owned())),
        Some("window") => Ok(Output::Window),
        Some("ndi") if !rest.trim().is_empty() => Ok(Output::Ndi(rest.trim().to_owned())),
        Some("decklink") if !rest.trim().is_empty() => decklink_output(rest.trim(), found, current),
        Some("automatic" | "auto") => Ok(Output::Automatic),
        Some("off") => Ok(Output::Off),
        _ => Err(usage.to_owned()),
    }
}

/// Where a screen goes, in words; a DeckLink output's format with where it comes from ("at
/// 1080p30, as Desktop Video Setup has it").
fn output_words(output: &Output, found: &sp_decklink::Found) -> String {
    match output {
        Output::Automatic => "the first other display (or a window)".to_owned(),
        Output::Display(name) => format!("the display {name}"),
        Output::Window => "a window".to_owned(),
        Output::Ndi(name) => format!("NDI as {name}"),
        Output::DeckLink(out) => match out.format_from(found) {
            Some((words, _)) => format!("{} at {words}", out.device),
            None => format!("{} at {}", out.device, out.mode),
        },
        Output::Off => "nowhere (off)".to_owned(),
    }
}

/// The DeckLink output typed: a device that can play now, by its number in `sp decklink` or its
/// name, in the format a new output starts in (the card's own, as Desktop Video Setup sets it,
/// where it can be used), or the screen's own format when it is already on that device.
fn decklink_output(typed: &str, found: &sp_decklink::Found, current: Option<&Output>) -> Result<Output, String> {
    if !matches!(found.driver, sp_decklink::Driver::Ready(_)) {
        return Err(found.driver.in_words());
    }
    let names: Vec<String> = found.devices.iter().map(|device| device.name.clone()).collect();
    if names.is_empty() {
        return Err("There are no DeckLink devices: Desktop Video sees no card.".to_owned());
    }
    let at = find("DeckLink output", &names, typed)?;
    let device = &found.devices[at];
    if !device.usable() {
        return Err(format!("{} can't play out in its card's present profile.", device.name));
    }
    let mode = device.default_mode().map(|mode| mode.name.clone()).unwrap_or_default();
    let new = DeckLinkOut { device: device.name.clone(), id: device.id, mode, keyer: Keyer::Off };
    Ok(Output::DeckLink(match current {
        Some(Output::DeckLink(have)) if have.same_device(&new) && device.mode(&have.mode).is_some() => have.clone(),
        _ => new,
    }))
}

impl crate::App {
    /// `screen add|set|delete …`.
    pub(crate) fn ctl_screen(&mut self, args: &[String]) -> Result<Reply, String> {
        let usage = "screen add audience|stage NAME [on display NAME|window|ndi NAME|decklink NAME|automatic] | screen set SCREEN key=value… | screen delete SCREEN --yes";
        let words = &args[2.min(args.len())..];
        match args.get(1).map(|w| w.to_lowercase()).as_deref() {
            Some("add") => self.screen_add(words),
            Some("set") => self.screen_set(words),
            Some("delete") => {
                let yes = words.iter().any(|w| w == "--yes");
                let typed = words.iter().filter(|w| *w != "--yes").cloned().collect::<Vec<_>>().join(" ");
                let state = &mut self.running.as_mut().ok_or("The app isn't ready.")?.home_state;
                let names: Vec<String> = state.screens.iter().map(|s| s.name.clone()).collect();
                let at = find("screen", &names, &typed)?;
                if state.screens.len() < 2 {
                    return Err(format!("{} is the only screen; there is always one.", names[at]));
                }
                if !yes {
                    return Err(format!("This deletes the screen {}. Add --yes to do it.", names[at]));
                }
                state.screens.remove(at);
                state.screen_selected = state.screen_selected.min(state.screens.len() - 1);
                state.screens_changed = true;
                Ok(Reply::done(format!("Deleted the screen {}.", names[at])))
            }
            _ => Err(usage.to_owned()),
        }
    }

    fn screen_add(&mut self, words: &[String]) -> Result<Reply, String> {
        let usage = "screen add audience|stage NAME [on display NAME|window|ndi NAME|decklink NAME|automatic]";
        let [kind, rest @ ..] = words else { return Err(usage.to_owned()) };
        let stage = match kind.to_lowercase().as_str() {
            "audience" => false,
            "stage" => true,
            _ => return Err(usage.to_owned()),
        };
        let found = self.decklink_found();
        let (name, output) = match rest.iter().position(|w| w.eq_ignore_ascii_case("on")) {
            Some(at) => (rest[..at].join(" "), parse_output(&rest[at + 1..], &found, None)?),
            None => (rest.join(" "), Output::Window),
        };
        let name = name.trim().to_owned();
        if name.is_empty() {
            return Err(usage.to_owned());
        }
        let connected = self.connected_displays();
        let state = &mut self.running.as_mut().ok_or("The app isn't ready.")?.home_state;
        if state.screens.iter().any(|s| s.name.eq_ignore_ascii_case(&name)) {
            return Err(format!("There is a screen called {name} already."));
        }
        let mut said = output_words(&output, &found);
        if let Output::Display(display) = &output
            && !connected.iter().any(|d| d.is_named(display))
        {
            let names: Vec<&str> = connected.iter().map(|d| d.name.as_str()).collect();
            said.push_str(&format!(
                " (not connected now; the screen waits for it; connected: {})",
                if names.is_empty() { "none".to_owned() } else { names.join(", ") }
            ));
        }
        let screen = if stage { Screen::stage(name.clone(), output) } else { Screen::new(name.clone(), output) };
        state.screens.push(screen);
        state.screens_changed = true;
        Ok(Reply::done(format!("Added the {} screen {name}, on {said}.", if stage { "stage" } else { "audience" })))
    }

    /// `screen set SCREEN key=value…`: name, output, kind, layers, color, edges, theme, layout,
    /// background, alpha, key.
    fn screen_set(&mut self, words: &[String]) -> Result<Reply, String> {
        let usage = "screen set SCREEN name=… output=display:NAME|window|ndi:NAME|decklink:NAME|automatic|off mode=FORMAT keyer=off|external|internal kind=audience|stage \
                     layers=slides,media,props,messages,announcements color=#RRGGBB edges=LEFT,TOP,RIGHT,BOTTOM theme=THEME|main layout=STAGE-LAYOUT \
                     background=solid|see-through alpha=straight|premultiplied key=SCREEN|none";
        let at = words.iter().position(|w| w.contains('=')).filter(|at| *at > 0).ok_or(usage)?;
        let theme_names: Vec<(sp_model::Uuid, String)> = self.themes.iter().map(|t| (t.id, t.name.clone())).collect();
        let connected = self.connected_displays();
        let found = self.decklink_found();
        let state = &mut self.running.as_mut().ok_or("The app isn't ready.")?.home_state;
        let names: Vec<String> = state.screens.iter().map(|s| s.name.clone()).collect();
        let index = find("screen", &names, &words[..at].join(" "))?;
        let layouts: Vec<(sp_model::Uuid, String)> =
            state.stage_layouts.iter().map(|l| (l.id, l.name.clone())).collect();
        let mut screen = state.screens[index].clone();
        let mut said = Vec::new();
        // Solid or see-through, and the alpha, are weighed together once all are read.
        let (mut background, mut alpha): (Option<bool>, Option<crate::screens::Alpha>) = (None, None);
        for setting in &words[at..] {
            let (key, value) = setting.split_once('=').ok_or(usage)?;
            match key.to_lowercase().as_str() {
                "background" => {
                    background = Some(match value.to_lowercase().replace([' ', '_'], "-").as_str() {
                        "solid" => false,
                        "see-through" | "seethrough" | "transparent" | "clear" => true,
                        other => return Err(format!("background is solid or see-through, not {other}.")),
                    });
                }
                "alpha" => {
                    alpha = Some(
                        crate::screens::Alpha::parse(value)
                            .ok_or_else(|| format!("alpha is straight or premultiplied, not {value}."))?,
                    );
                }
                "key" | "key_of" | "key-of" => {
                    if ["none", "off", "no"].contains(&value.to_lowercase().as_str()) {
                        screen.look.key_of = None;
                        said.push("not a key".to_owned());
                    } else {
                        let fill = find("screen", &names, value)?;
                        let of = &state.screens[fill];
                        if fill == index {
                            return Err(format!("{} can't be its own key.", names[index]));
                        }
                        if of.kind != Kind::Audience {
                            return Err(format!("{} is a stage screen, which has no key.", of.name));
                        }
                        if of.look.key_of.is_some() {
                            return Err(format!(
                                "{} is a key itself; a key is of a screen that shows the show.",
                                of.name
                            ));
                        }
                        screen.look.key_of = Some(of.id);
                        said.push(format!("the key of {}", of.name));
                    }
                }
                "name" => {
                    screen.name = value.trim().to_owned();
                    said.push(format!("called {}", screen.name));
                }
                "output" => {
                    let words: Vec<String> = match value.split_once(':') {
                        Some((kind, name)) => vec![kind.to_owned(), name.to_owned()],
                        None => vec![value.to_owned()],
                    };
                    screen.output = parse_output(&words, &found, Some(&screen.output))?;
                    said.push(format!("on {}", output_words(&screen.output, &found)));
                    // A display not connected now is kept (the screen waits for it), and said.
                    if let Output::Display(name) = &screen.output
                        && !connected.iter().any(|display| display.is_named(name))
                    {
                        let connected: Vec<&str> = connected.iter().map(|d| d.name.as_str()).collect();
                        said.push(format!(
                            "but no display called {name} is connected now, so the screen waits for it (connected: {})",
                            if connected.is_empty() { "none".to_owned() } else { connected.join(", ") }
                        ));
                    }
                }
                "mode" | "format" => {
                    let Output::DeckLink(out) = &mut screen.output else {
                        return Err("mode is for a screen on a DeckLink output (output=decklink:NAME).".to_owned());
                    };
                    let device = found
                        .find(out.id, &out.device)
                        .ok_or_else(|| format!("{} isn't connected now, so its formats aren't known.", out.device))?;
                    let mode = device.mode(value).ok_or_else(|| {
                        let modes: Vec<&str> = device.offered().map(|mode| mode.name.as_str()).collect();
                        format!("{} has no format {value}. Its formats: {}.", device.name, modes.join(", "))
                    })?;
                    out.mode = mode.name.clone();
                    said.push(format!("in {}", out.mode));
                }
                "keyer" => {
                    let Output::DeckLink(out) = &mut screen.output else {
                        return Err("keyer is for a screen on a DeckLink output (output=decklink:NAME).".to_owned());
                    };
                    let keyer = Keyer::parse(value)
                        .ok_or_else(|| format!("keyer is off, external or internal, not {value}."))?;
                    if keyer != Keyer::Off {
                        let device = found
                            .find(out.id, &out.device)
                            .ok_or_else(|| format!("{} isn't connected now, so its keyer isn't known.", out.device))?;
                        let can = match keyer {
                            Keyer::External => device.external_keying,
                            Keyer::Internal => device.internal_keying,
                            Keyer::Off => true,
                        };
                        if !can {
                            return Err(format!(
                                "{} can't do {} keying in its card's present profile (it can do {}).",
                                device.name,
                                keyer.name(),
                                device.keying_in_words()
                            ));
                        }
                        if !device.mode(&out.mode).is_some_and(|mode| mode.keying) {
                            let keyed: Vec<&str> =
                                device.offered().filter(|mode| mode.keying).map(|mode| mode.name.as_str()).collect();
                            return Err(format!(
                                "{} can't key in {}. It can key in: {}.",
                                device.name,
                                out.mode,
                                keyed.join(", ")
                            ));
                        }
                    }
                    out.keyer = keyer;
                    said.push(
                        match keyer {
                            Keyer::Off => "keyer off",
                            Keyer::External => "fill and key from the card's keyer",
                            Keyer::Internal => "keyed by the card over its input",
                        }
                        .to_owned(),
                    );
                }
                "kind" => {
                    screen.kind = match value.to_lowercase().as_str() {
                        "audience" => Kind::Audience,
                        "stage" => Kind::Stage,
                        other => return Err(format!("kind is audience or stage, not {other}.")),
                    };
                    said.push(format!(
                        "a{} {value} screen",
                        if value.eq_ignore_ascii_case("audience") { "n" } else { "" }
                    ));
                }
                "layers" => {
                    let wanted: Vec<String> =
                        value.split(',').map(|w| w.trim().to_lowercase()).filter(|w| !w.is_empty()).collect();
                    let known = ["slides", "media", "props", "messages", "announcements"];
                    if let Some(bad) = wanted.iter().find(|w| !known.contains(&w.as_str()) && w.as_str() != "all") {
                        return Err(format!(
                            "{bad} isn't a layer: slides, media, props, messages, announcements (or all)."
                        ));
                    }
                    let has = |layer: &str| wanted.iter().any(|w| w == layer || w == "all");
                    screen.look.slides = has("slides");
                    screen.look.media = has("media");
                    screen.look.props = has("props");
                    screen.look.messages = has("messages");
                    screen.look.announcements = has("announcements");
                    said.push(format!("shows {}", wanted.join(", ")));
                }
                "color" | "colour" => {
                    screen.look.color = crate::slide_cli::parse_color(value)
                        .ok_or_else(|| format!("{value} isn't a colour: #RRGGBB, #RRGGBBAA or a colour's name."))?;
                    said.push(format!("colour {value}"));
                }
                "edges" | "mask" => {
                    let numbers: Vec<f32> = value
                        .split(',')
                        .map(|n| n.trim().parse::<f32>())
                        .collect::<Result<_, _>>()
                        .map_err(|_| "edges are four numbers, in slide pixels: left,top,right,bottom".to_owned())?;
                    let [left, top, right, bottom] = numbers.as_slice() else {
                        return Err("edges are four numbers, in slide pixels: left,top,right,bottom".to_owned());
                    };
                    if numbers.iter().any(|n| !(0.0..=540.0).contains(n)) {
                        return Err("Each edge covers 0 to 540 slide pixels.".to_owned());
                    }
                    screen.look.mask = [*left, *top, *right, *bottom];
                    said.push(format!("edges covered {left},{top},{right},{bottom}"));
                }
                "theme" => {
                    screen.theme = if value.eq_ignore_ascii_case("main") || value.eq_ignore_ascii_case("none") {
                        None
                    } else {
                        let names: Vec<String> = theme_names.iter().map(|(_, n)| n.clone()).collect();
                        Some(theme_names[find("theme", &names, value)?].0)
                    };
                    said.push(format!("theme {value}"));
                }
                "layout" => {
                    let names: Vec<String> = layouts.iter().map(|(_, n)| n.clone()).collect();
                    let which = find("stage layout", &names, value)
                        .map_err(|problem| format!("{problem} The stage layouts: {}.", names.join(", ")))?;
                    screen.stage_layout = Some(layouts[which].0);
                    said.push(format!("stage layout {}", names[which]));
                }
                other => return Err(format!("{other} isn't a screen setting. {usage}")),
            }
        }
        match (background, alpha) {
            (Some(false), Some(_)) => {
                return Err("A solid screen has no alpha; alpha is for background=see-through.".to_owned());
            }
            (Some(false), None) => {
                screen.look.see_through = None;
                said.push("solid".to_owned());
            }
            (Some(true), _) => {
                let alpha = alpha.or(screen.look.see_through).unwrap_or_default();
                screen.look.see_through = Some(alpha);
                said.push(format!("see-through, {} alpha", alpha.name()));
            }
            (None, Some(alpha)) if screen.look.see_through.is_some() => {
                screen.look.see_through = Some(alpha);
                said.push(format!("{} alpha", alpha.name()));
            }
            (None, Some(_)) => {
                return Err(format!(
                    "{} is solid; add background=see-through to send it see-through with that alpha.",
                    screen.name
                ));
            }
            (None, None) => {}
        }
        if (background == Some(true) || screen.look.key_of.is_some()) && screen.kind != Kind::Audience {
            return Err(format!("{} is a stage screen; only an audience screen is see-through or a key.", screen.name));
        }
        if background == Some(true) && screen.look.key_of.is_some() {
            said.push("(a key shows its screen's key, not a background of its own)".to_owned());
        }
        let was = std::mem::replace(&mut state.screens[index], screen);
        state.screens_changed = true;
        Ok(Reply::done(format!("The screen {}: {}.", was.name, said.join(", "))))
    }

    /// The displays connected now, as the operator's window sees them (the event loop's own list
    /// can be empty in a quiet run), else as the screens were last placed.
    pub(crate) fn connected_displays(&self) -> Vec<crate::displays::DisplayInfo> {
        let Some(running) = self.running.as_ref() else { return Vec::new() };
        let monitors: Vec<_> = running.home_window.available_monitors().collect();
        if !monitors.is_empty() {
            return crate::display_list(monitors.into_iter(), running.home_window.primary_monitor());
        }
        if !running.home_state.displays.is_empty() {
            return running.home_state.displays.clone();
        }
        // Where even that is empty (a run started where the system lists no monitors), at least
        // the display the window is on.
        running
            .home_window
            .current_monitor()
            .map(|monitor| crate::display_list(std::iter::once(monitor), running.home_window.primary_monitor()))
            .unwrap_or_default()
    }

    /// The DeckLink devices as last found, and how the driver stands.
    fn decklink_found(&self) -> sp_decklink::Found {
        self.running
            .as_ref()
            .map_or(sp_decklink::Found { driver: sp_decklink::Driver::Looking, devices: Vec::new() }, |running| {
                running.home_state.decklink.clone()
            })
    }

    /// `decklink`: the DeckLink devices, the formats each plays out in and whether each can key,
    /// numbered for `screen set SCREEN output=decklink:NUMBER`. Asks for them to be listed again
    /// too (a card's profile may have changed, which the driver doesn't tell of).
    pub(crate) fn ctl_decklink(&self) -> Result<Reply, String> {
        if let Some(discovery) = self.running.as_ref().and_then(|running| running.decklink.as_ref()) {
            discovery.refresh();
        }
        let found = self.decklink_found();
        if !matches!(found.driver, sp_decklink::Driver::Ready(_)) {
            let words = found.driver.in_words();
            return Ok(Reply::with(words.clone(), serde_json::json!({"driver": words, "devices": []})));
        }
        if found.devices.is_empty() {
            let words = "No DeckLink devices: Desktop Video sees no card.".to_owned();
            return Ok(Reply::with(words, serde_json::json!({"driver": found.driver.in_words(), "devices": []})));
        }
        let lines: Vec<String> = found
            .devices
            .iter()
            .map(|device| {
                if !device.usable() {
                    return format!("{} (can't play out in its card's present profile)", device.name);
                }
                let modes: Vec<&str> = device.offered().map(|mode| mode.name.as_str()).collect();
                let start = device
                    .start(false)
                    .map_or(String::new(), |start| format!("; a new output starts in {}", start.in_words()));
                format!(
                    "{}: {}, {}; formats {}{start}",
                    device.name,
                    device.connections_in_words(),
                    device.keying_in_words(),
                    modes.join(", ")
                )
            })
            .collect();
        let data: Vec<serde_json::Value> = found
            .devices
            .iter()
            .map(|device| {
                serde_json::json!({
                    "name": device.name,
                    "model": device.model,
                    "id": device.id,
                    "usable": device.usable(),
                    "connections": device.connections_in_words(),
                    "external_keying": device.external_keying,
                    "internal_keying": device.internal_keying,
                    "formats": device.offered().map(|mode| mode.name.clone()).collect::<Vec<_>>(),
                    "keyed_formats": device.offered().filter(|mode| mode.keying).map(|mode| mode.name.clone()).collect::<Vec<_>>(),
                    "default_format": device.default_mode().map(|mode| mode.name.clone()),
                    "default_format_from": device.start(false).map(|start| if start.from_card() { "card" } else { "usual" }),
                    "default_format_words": device.start(false).map(|start| start.in_words()),
                    "card_format": device.card_format(false).name(),
                })
            })
            .collect();
        Ok(Reply::with(
            crate::control::numbered(&lines),
            serde_json::json!({"driver": found.driver.in_words(), "devices": data}),
        ))
    }

    /// `displays`: the displays connected now, by name, with their size and which is the main one.
    pub(crate) fn ctl_displays(&self) -> Result<Reply, String> {
        let displays = self.connected_displays();
        let lines: Vec<String> = displays
            .iter()
            .map(|d| format!("{} ({}x{}){}", d.name, d.w, d.h, if d.main { ", the main display" } else { "" }))
            .collect();
        let data: Vec<serde_json::Value> = displays
            .iter()
            .map(|d| serde_json::json!({"name": d.name, "width": d.w, "height": d.h, "main": d.main}))
            .collect();
        Ok(Reply::with(crate::control::numbered(&lines), serde_json::json!(data)))
    }

    /// `settings`, and `settings set key=value…`: the choices kept between runs.
    pub(crate) fn ctl_settings(&mut self, args: &[String]) -> Result<Reply, String> {
        let usage = "settings | settings set transition=KIND[:SECONDS] media_transition=… copyright=never|first|last|every \
                     ccli_licence=NUMBER start_audience=on|off start_with=nothing|media-playlist:NAME|announcements:SONG \
                     continuous=on|off view=grid|easy|table columns=1-12";
        let state = &self.running.as_ref().ok_or("The app isn't ready.")?.home_state;
        if args.get(1).is_none() {
            let prefs = state.prefs();
            let start_with = match &prefs.start_with {
                crate::service::StartWith::Nothing => "nothing".to_owned(),
                crate::service::StartWith::MediaPlaylist { id } => state
                    .media_playlists
                    .iter()
                    .find(|p| p.id == *id)
                    .map_or("a media playlist (gone)".to_owned(), |p| format!("the media playlist {}", p.name)),
                crate::service::StartWith::Announcements { song } => {
                    crate::studio::playlist_song(&self.show, state, song).map_or("a song (gone)".to_owned(), |at| {
                        format!("the announcements song {}", self.show.presentations()[at].title)
                    })
                }
            };
            let transition = |t: Option<sp_model::Transition>| {
                t.map_or("Dissolve".to_owned(), |t| format!("{} {:.1}s", t.kind.name(), t.seconds))
            };
            let lines = [
                format!("transition: {}", transition(prefs.transition)),
                format!("media transition: {}", transition(prefs.media_transition)),
                format!("copyright line: {}", prefs.copyright_show.name()),
                format!("CCLI licence: {}", if prefs.ccli_licence.is_empty() { "(none)" } else { &prefs.ccli_licence }),
                format!("audience screens at start: {}", if prefs.start_audience_off { "off" } else { "on" }),
                format!("starts with: {start_with}"),
                format!("continuous playlist: {}", if prefs.continuous { "on" } else { "off" }),
                format!("slides: {} view, {} across", prefs.slide_view, prefs.columns),
            ];
            let data = serde_json::to_value(&prefs).unwrap_or_default();
            return Ok(Reply::with(lines.join("\n"), data));
        }
        if !args[1].eq_ignore_ascii_case("set") || args.len() < 3 {
            return Err(usage.to_owned());
        }
        let media_playlists: Vec<(sp_model::Uuid, String)> =
            state.media_playlists.iter().map(|p| (p.id, p.name.clone())).collect();
        let mut said = Vec::new();
        let mut changes: Vec<Change> = Vec::new();
        for setting in &args[2..] {
            let (key, value) = setting.split_once('=').ok_or_else(|| format!("{setting} isn't key=value. {usage}"))?;
            match key.to_lowercase().as_str() {
                "transition" | "media_transition" => {
                    let words: Vec<String> =
                        value.split([':', ' ']).filter(|w| !w.is_empty()).map(str::to_owned).collect();
                    let transition = crate::service_cli::parse_transition(&words)?.unwrap_or_default();
                    let media = key.eq_ignore_ascii_case("media_transition");
                    said.push(format!(
                        "{} {} {:.1}s",
                        if media { "media transition" } else { "transition" },
                        transition.kind.name(),
                        transition.seconds
                    ));
                    changes.push(Box::new(move |state| {
                        if media {
                            state.media_transition = transition;
                        } else {
                            state.transition = transition;
                        }
                    }));
                }
                "copyright" => {
                    let show = match value.to_lowercase().as_str() {
                        "never" | "off" => crate::service::CopyrightShow::Never,
                        "first" => crate::service::CopyrightShow::FirstSlide,
                        "last" => crate::service::CopyrightShow::LastSlide,
                        "every" | "all" => crate::service::CopyrightShow::EverySlide,
                        other => return Err(format!("copyright is never, first, last or every, not {other}.")),
                    };
                    said.push(format!("copyright line {}", show.name().to_lowercase()));
                    changes.push(Box::new(move |state| state.copyright_show = show));
                }
                "ccli_licence" | "ccli" | "licence" | "license" => {
                    if !value.chars().all(|c| c.is_ascii_digit()) {
                        return Err(format!("A CCLI licence number is digits only, not {value}."));
                    }
                    let value = value.to_owned();
                    said.push(format!("CCLI licence {value}"));
                    changes.push(Box::new(move |state| state.ccli_licence = value));
                }
                "start_audience" => {
                    let on = on_off(key, value)?;
                    said.push(format!("audience screens {} at start", if on { "on" } else { "off" }));
                    changes.push(Box::new(move |state| state.start_audience_off = !on));
                }
                "continuous" => {
                    let on = on_off(key, value)?;
                    said.push(format!("continuous playlist {}", if on { "on" } else { "off" }));
                    changes.push(Box::new(move |state| state.continuous = on));
                }
                "start_with" => {
                    let start = match value.split_once(':') {
                        None if value.eq_ignore_ascii_case("nothing") => crate::service::StartWith::Nothing,
                        Some((kind, name)) if kind.eq_ignore_ascii_case("media-playlist") => {
                            let names: Vec<String> = media_playlists.iter().map(|(_, n)| n.clone()).collect();
                            crate::service::StartWith::MediaPlaylist {
                                id: media_playlists[find("media playlist", &names, name)?].0,
                            }
                        }
                        Some((kind, name)) if kind.eq_ignore_ascii_case("announcements") => {
                            let at = self.find_song_as_is(name)?;
                            let key = state.song_keys.get(at).cloned().ok_or("That song can't start with the app.")?;
                            crate::service::StartWith::Announcements { song: key }
                        }
                        _ => return Err("start_with is nothing, media-playlist:NAME or announcements:SONG.".to_owned()),
                    };
                    said.push(format!("starts with {value}"));
                    changes.push(Box::new(move |state| state.start_with = start));
                }
                "view" => {
                    let view = match value.to_lowercase().as_str() {
                        "grid" => crate::home::SlideView::Grid,
                        "easy" => crate::home::SlideView::Easy,
                        "table" => crate::home::SlideView::Table,
                        other => return Err(format!("view is grid, easy or table, not {other}.")),
                    };
                    said.push(format!("{value} view"));
                    changes.push(Box::new(move |state| state.slide_view = view));
                }
                "columns" => {
                    let columns =
                        value.parse::<usize>().ok().filter(|c| (1..=12).contains(c)).ok_or("columns is 1 to 12.")?;
                    said.push(format!("{columns} slides across"));
                    changes.push(Box::new(move |state| state.columns = columns));
                }
                other => return Err(format!("{other} isn't a setting. {usage}")),
            }
        }
        if let Some(running) = self.running.as_mut() {
            for change in changes {
                change(&mut running.home_state);
            }
            running.home_window.request_redraw();
        }
        Ok(Reply::done(format!("Settings: {}.", said.join(", "))))
    }

    /// `view reflow [SONG]`: the Reflow view, the song's words as text to edit.
    pub(crate) fn view_reflow(&mut self, typed: &str) -> Result<Reply, String> {
        let at = if typed.trim().is_empty() { self.show.selected() } else { self.find_song(typed)? };
        let song = self.show.presentations().get(at).cloned().ok_or("That song is gone.")?;
        let text = sp_model::to_song_text(&song);
        self.with_state(|state| state.reflow = Some((at, text)));
        Ok(Reply::done(format!("Showing {} in the Reflow view.", song.title)))
    }
}
