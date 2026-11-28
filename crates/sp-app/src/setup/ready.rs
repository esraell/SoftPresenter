//! The setup's Ready step: what is set, line by line, each with Change; anonymous reports as
//! the last line, with their On/Off key, which acts at once; and under the list two folds side
//! by side, one open at a time: what the system will ask permission for (a Mac and Windows; none
//! on Linux) and what the reports send (`reports::ui`).

use egui::{Align2, FontId, Rect, Sense, pos2, vec2};

use super::{Env, Fold, Setup, Step, System, language_own_name};
use crate::find_songs::Phase;
use crate::home::HomeState;
use crate::home::settings::parts;
use crate::i18n::{t, tf, tn};
use crate::screens::{Kind, Output};
use crate::studio::{CARD, CARE, Flat, TEXT, TEXT_DIM, TEXT_FAINT, semibold};

/// What the system will ask permission for: the fold's title, what it says first,
/// each question with when it comes, and what to do after a wrong answer.
pub(super) struct Asks {
    pub title: &'static str,
    pub intro: &'static str,
    pub questions: Vec<(&'static str, &'static str)>,
    pub after: &'static str,
}

/// What `system` asks, from what SoftPresenter does there: on a Mac, the network for NDI, the
/// Documents folder and controlling the app that draws a presentation as pictures (by script);
/// on Windows only the firewall, the first time SoftPresenter listens on the network (NDI and
/// Remote Control), as the app that draws a presentation is asked through its own automation,
/// which Windows doesn't ask about. Nothing asks on Linux, which has no such fold.
pub(super) fn asks(system: System) -> Option<Asks> {
    match system {
        System::Mac => Some(Asks {
            title: t("setup.asks.mac.title", "What macOS will ask"),
            intro: t(
                "setup.asks.mac.intro",
                "macOS asks before an app does some things. When these come up, choose Allow or OK:",
            ),
            questions: vec![
                (
                    t("setup.asks.mac.network", "Local Network"),
                    t(
                        "setup.asks.mac.network.why",
                        "When a screen is sent over NDI (to a stream or a video switcher). Without it, NDI can't be seen on the network.",
                    ),
                ),
                (
                    t("setup.asks.mac.documents", "Files in your Documents folder"),
                    t(
                        "setup.asks.mac.documents.why",
                        "When SoftPresenter looks for a library of song documents or opens songs, pictures and videos kept there.",
                    ),
                ),
                (
                    t("setup.asks.mac.automation", "Controlling the app that opens a presentation"),
                    t(
                        "setup.asks.mac.automation.why",
                        "The first time you bring a presentation in as pictures, SoftPresenter asks an app on this Mac that can open it to draw its slides. macOS asks once, to allow that.",
                    ),
                ),
            ],
            after: t(
                "setup.asks.mac.after",
                "SoftPresenter never asks for your password. If you chose Don't Allow by mistake, change it in System Settings › Privacy & Security.",
            ),
        }),
        System::Windows => Some(Asks {
            title: t("setup.asks.windows.title", "What Windows will ask"),
            intro: t(
                "setup.asks.windows.intro",
                "Windows asks once, before other computers and phones can reach SoftPresenter. When it comes up, choose Allow:",
            ),
            questions: vec![(
                t("setup.asks.windows.firewall", "Windows Defender Firewall"),
                t(
                    "setup.asks.windows.firewall.why",
                    "The first time a screen is sent over NDI, or Remote Control is switched on, Windows asks whether to allow SoftPresenter. Choose Allow for private networks. Without it, NDI can't be seen on the network, and phones on the church network can't reach Remote Control.",
                ),
            )],
            after: t(
                "setup.asks.windows.after",
                "If you chose Cancel by mistake, allow SoftPresenter in the Windows firewall's settings.",
            ),
        }),
        System::Linux => None,
    }
}

/// "{top} on top, {below} below", for the languages and the Bibles.
fn on_top_below(top: &str, below: &str) -> String {
    tf("setup.ready.on_top_below", "{top} on top, {below} below", &[("top", top), ("below", below)])
}

/// The Languages line: the two on screen, or the one.
pub(super) fn languages_line(languages: &[String]) -> String {
    match languages {
        [] => language_own_name("en"),
        [one] => language_own_name(one),
        [top, below, ..] => on_top_below(&language_own_name(top), &language_own_name(below)),
    }
}

/// The Bibles line: the Bible page's translations as they stand (`names`, top first).
pub(super) fn bibles_line(names: &[String]) -> String {
    match names {
        [] => t("setup.ready.no_bibles", "None yet").to_owned(),
        [one] => one.clone(),
        [top, below, ..] => on_top_below(top, below),
    }
}

/// What a line of the list says, and how it is marked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Line {
    pub words: String,
    pub mark: Mark,
}

/// The mark at the left of a line: a tick, a dash for nothing yet (or off), or the amber light
/// of something that needs doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mark {
    Tick,
    Dash,
    Care,
}

/// The Songs line: what came in from this computer, what the free songbooks bring, a download
/// that failed (in amber), or that there are none yet.
pub(super) fn songs_line(state: &HomeState) -> Line {
    if let Some(problem) = &state.free_songbooks.problem {
        return Line { words: problem.clone(), mark: Mark::Care };
    }
    let mut parts: Vec<String> = Vec::new();
    let card = &state.find_songs;
    if card.phase == Phase::Importing {
        let percent = (card.progress.clamp(0.0, 1.0) * 100.0).floor() as u64;
        let percent = tf("settings.size.percent", "{n}%", &[("n", &crate::i18n::count(percent))]);
        parts.push(tn(
            "settings.find.bringing",
            card.bringing as u64,
            "Bringing in {n} song\u{2026} {percent}",
            "Bringing in {n} songs\u{2026} {percent}",
            &[("percent", &percent)],
        ));
    } else if let Some(brought) = card.brought.as_ref().filter(|brought| brought.came > 0) {
        let libraries = brought.from.iter().filter(|(_, songs)| *songs > 0).count().max(1);
        let libraries = tn("setup.ready.libraries", libraries as u64, "{n} library", "{n} libraries", &[]);
        parts.push(tn(
            "setup.ready.from_computer",
            brought.came as u64,
            "{n} song from this computer, in {libraries}",
            "{n} songs from this computer, in {libraries}",
            &[("libraries", &libraries)],
        ));
    }
    let free = &state.free_songbooks.known;
    let catalog = &state.songbooks.known;
    let coming: usize = free
        .busy
        .keys()
        .chain(free.held.iter())
        .filter_map(|id| crate::free_songbooks::by_id(id))
        .map(|book| book.songs)
        .sum();
    let here: usize = free.installed.values().sum::<usize>() + catalog.installed.values().sum::<usize>();
    let busy = !free.busy.is_empty() || !free.held.is_empty() || !catalog.busy.is_empty();
    if here + coming > 0 || busy {
        let mut words = if here + coming > 0 {
            tn(
                "setup.ready.free_songs",
                (here + coming) as u64,
                "{n} song from free songbooks",
                "{n} songs from free songbooks",
                &[],
            )
        } else {
            t("setup.songs.free", "Free songbooks").to_owned()
        };
        if busy {
            words = tf("setup.ready.downloading", "{what} \u{b7} downloading", &[("what", &words)]);
        }
        parts.push(words);
    }
    if parts.is_empty() {
        return Line {
            words: t("setup.ready.no_songs", "None yet. Your own songs, or free songbooks, can come in at any time.")
                .to_owned(),
            mark: Mark::Dash,
        };
    }
    Line { words: parts.join(" \u{b7} "), mark: Mark::Tick }
}

/// Where a screen goes, in a few words.
fn output_words(output: &Output) -> String {
    match output {
        Output::Display(name) => name.clone(),
        Output::Automatic => t("setup.screens.automatic", "In a window for now").to_owned(),
        Output::Window => t("setup.screens.window", "Only in a window").to_owned(),
        Output::Ndi(_) => "NDI".to_owned(),
        Output::DeckLink(out) => out.device.clone(),
        Output::Off => String::new(),
    }
}

/// The Screens line: where the congregation's screen goes, and the band's; with no screen
/// chosen (or the step skipped with none), the one thing Ready warns about.
pub(super) fn screens_line(state: &HomeState) -> Line {
    let shown = |kind: Kind| {
        state.screens.iter().find(|screen| screen.kind == kind && screen.on && screen.output != Output::Off)
    };
    let Some(audience) = shown(Kind::Audience) else {
        return Line {
            words: t("setup.ready.no_screen", "No screen chosen yet: slides won\u{2019}t show anywhere until one is.")
                .to_owned(),
            mark: Mark::Care,
        };
    };
    let mut words =
        tf("setup.ready.congregation", "The congregation: {where}", &[("where", &output_words(&audience.output))]);
    if let Some(stage) = shown(Kind::Stage) {
        words = tf(
            "setup.ready.band",
            "{congregation} \u{b7} the band: {where}",
            &[("congregation", &words), ("where", &output_words(&stage.output))],
        );
    }
    Line { words, mark: Mark::Tick }
}

/// The Reports line: by how reports start where the computer is, never by the switch; its mark
/// is the tick while they are on and the dash while off.
pub(super) fn reports_line(standing: &crate::reports::Standing) -> Line {
    Line {
        words: crate::reports::ui::setup_line(standing.starts_on).to_owned(),
        mark: if standing.enabled { Mark::Tick } else { Mark::Dash },
    }
}

/// What the tips line says to a first-timer.
fn tips_words() -> &'static str {
    t(
        "setup.ready.tips",
        "Click a slide to put it on screen, Space for the next, Esc to clear. Later, in Settings: Phone remote, to run the slides from a phone.",
    )
}

/// What a line's key does.
enum Key<'a> {
    /// Change (or Choose, for the line that warns), to a step.
    Go(&'a str, Step),
    /// Try Again, for a download that failed.
    Retry,
    /// The reports' On/Off key.
    Switch(bool),
}

/// The step: the step to go to, when a line's Change was pressed.
pub fn step(ui: &mut egui::Ui, setup: &mut Setup, state: &mut HomeState, env: &Env) -> Option<usize> {
    super::head(
        ui,
        t("setup.ready.title", "Ready for Sunday"),
        t("setup.ready.about", "Here is what is set. Everything can be changed in Settings at any time."),
    );
    let again = setup.again();
    let side = if again { 190.0 } else { 270.0 };
    let gap = 18.0;
    let left = ui.available_width() - side - gap;
    let change = t("setup.ready.change", "Change");
    let choose = t("setup.ready.choose", "Choose");
    let retry = t("setup.ready.try_again", "Try Again");
    // Every line's key is as wide as the widest of them, 70 at least.
    let font = FontId::proportional(12.5);
    let switch = parts::on_off_size(ui).x;
    let key_width =
        crate::studio::flat_button_width(ui, &[change, choose, retry], &font, 12.0).max(switch).max(70.0).ceil();
    let mut go = None;
    let mut retry_now = false;
    let mut switched = None;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.allocate_ui_with_layout(vec2(left, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.set_width(left);
            let songs = songs_line(state);
            let screens = screens_line(state);
            let mut lines: Vec<(&str, Line, Key)> = vec![
                (
                    t("setup.step.languages", "Languages"),
                    Line { words: languages_line(&setup.languages), mark: Mark::Tick },
                    Key::Go(change, Step::Languages),
                ),
                (
                    t("setup.step.songs", "Songs"),
                    songs.clone(),
                    if songs.mark == Mark::Care { Key::Retry } else { Key::Go(change, Step::Songs) },
                ),
                (
                    t("setup.step.bibles", "Bibles"),
                    Line { words: bibles_line(&super::bibles::names(state)), mark: Mark::Tick },
                    Key::Go(change, Step::Bibles),
                ),
                (
                    t("setup.step.screens", "Screens"),
                    screens.clone(),
                    Key::Go(if screens.mark == Mark::Care { choose } else { change }, Step::Screens),
                ),
            ];
            // Anonymous reports: the last line, where the build has them.
            if let Some(standing) = &env.reports {
                lines.push((
                    t("setup.ready.reports", "Reports"),
                    reports_line(standing),
                    Key::Switch(standing.enabled),
                ));
            }
            for (label, line, key) in lines {
                let pressed = list_line(ui, label, &line, key_width, |ui, rect| match &key {
                    Key::Go(words, _) => crate::studio::flat_button(
                        ui,
                        rect,
                        ui.scope_id().with(("setup ready key", label)),
                        words,
                        font.clone(),
                        Flat::Plain,
                        true,
                    )
                    .clicked(),
                    Key::Retry => crate::studio::flat_button(
                        ui,
                        rect,
                        ui.scope_id().with("setup ready retry"),
                        retry,
                        font.clone(),
                        Flat::Plain,
                        true,
                    )
                    .clicked(),
                    Key::Switch(on) => {
                        // The same key as Settings' switches, as wide as the lines' other keys.
                        let size = parts::on_off_size(ui);
                        let rect = Rect::from_center_size(rect.center(), vec2(rect.width().max(size.x), size.y));
                        parts::on_off(ui, rect, crate::reports::ui::setup_switch_id(), *on).clicked()
                    }
                });
                if pressed {
                    match key {
                        Key::Go(_, step) => go = Some(step.at()),
                        Key::Retry => retry_now = true,
                        Key::Switch(on) => switched = Some(!on),
                    }
                }
                ui.add_space(6.0);
            }
        });
        ui.add_space(gap);
        ui.allocate_ui_with_layout(vec2(side, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.set_width(side);
            super::languages::preview(ui, side, &setup.languages);
            ui.add_space(8.0);
            super::small(ui, t("setup.ready.look", "How songs will look. Change the look in Edit \u{203a} Themes."), 2);
        });
    });
    if retry_now {
        // The songbooks ticked that aren't here are asked for again.
        state.free_songbooks.problem = None;
        setup.shared.confirmed = true;
        crate::free_songbooks::setup_closed(&setup.shared, &mut state.free_songbooks);
        setup.shared.confirmed = false;
    }
    if let Some(on) = switched {
        // At once, as Settings' switch does; the line shows it before the reporter answers.
        crate::reports::ui::setup_switched(ui.ctx(), on);
        if let Some(standing) = setup.reports.as_mut() {
            standing.enabled = on;
        }
    }
    ui.add_space(4.0);
    folds(ui, setup, state, env);
    // The first-timer's tips; run again from Settings, the sheet leaves them out.
    if !again {
        ui.add_space(8.0);
        super::words(ui, tips_words(), FontId::proportional(12.5), TEXT_DIM, 18.0, 1);
    }
    go
}

/// A line of the list: its mark, its name in small capitals, its words (three lines at the
/// most) and its key at the right, drawn by `key` in the room it is given; what `key` says.
fn list_line(
    ui: &mut egui::Ui,
    label: &str,
    line: &Line,
    key_width: f32,
    key: impl FnOnce(&mut egui::Ui, Rect) -> bool,
) -> bool {
    let width = ui.available_width();
    let label_width = 84.0;
    let room = width - 10.0 - 18.0 - 8.0 - label_width - 8.0 - 8.0 - key_width - 10.0;
    let ink = if line.mark == Mark::Care { TEXT } else { TEXT_DIM };
    let galley = parts::lines_at_most(ui, &line.words, FontId::proportional(12.5), ink, (room, 17.0), 3);
    let height = (galley.size().y + 12.0).max(44.0);
    let (place, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    ui.painter().rect_filled(place, 6.0, CARD);
    let centre = pos2(place.min.x + 10.0 + 9.0, place.center().y);
    match line.mark {
        Mark::Tick => super::tick(ui.painter(), centre, 4.5, egui::Stroke::new(1.6, TEXT)),
        Mark::Dash => {
            ui.painter().text(centre, Align2::CENTER_CENTER, "\u{2013}", FontId::proportional(13.0), TEXT_FAINT);
        }
        Mark::Care => super::light(ui, centre, CARE),
    }
    let label_at = pos2(place.min.x + 10.0 + 18.0 + 8.0, place.center().y);
    let label_galley = parts::one_line(ui, &label.to_uppercase(), semibold(10.5), TEXT_FAINT, label_width);
    ui.painter().galley(pos2(label_at.x, label_at.y - label_galley.size().y / 2.0), label_galley, TEXT_FAINT);
    let words_at = pos2(label_at.x + label_width + 8.0, place.center().y - galley.size().y / 2.0);
    ui.painter().galley(words_at, galley, ink);
    let key_rect =
        Rect::from_min_size(pos2(place.max.x - 10.0 - key_width, place.center().y - 13.0), vec2(key_width, 26.0));
    key(ui, key_rect)
}

/// The titles of Ready's folds on `system`, in order: what it will ask (none on Linux), and
/// what the reports send (where the build has them).
pub(super) fn fold_titles(system: System, reports: bool) -> Vec<(Fold, &'static str)> {
    let mut out = Vec::new();
    if let Some(asks) = asks(system) {
        out.push((Fold::Asks, asks.title));
    }
    if reports {
        out.push((Fold::Reports, crate::reports::ui::setup_fold_title()));
    }
    out
}

/// The two folds, side by side on one line, one open at a time; the open one's words go across
/// the whole width under the pair, and opening one brings it into view.
fn folds(ui: &mut egui::Ui, setup: &mut Setup, state: &mut HomeState, env: &Env) {
    let titles = fold_titles(env.system, env.reports.is_some());
    if titles.is_empty() {
        return;
    }
    if setup.fold.is_some_and(|open| !titles.iter().any(|(fold, _)| *fold == open)) {
        setup.fold = None;
    }
    let line = ui
        .horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 22.0;
            for (fold, title) in &titles {
                let open = setup.fold == Some(*fold);
                if super::bibles::fold_key(ui, &format!("setup fold {fold:?}"), title, open).clicked() {
                    setup.fold = if open { None } else { Some(*fold) };
                    setup.fold_opened = !open;
                }
            }
        })
        .response
        .rect;
    match setup.fold {
        Some(Fold::Asks) => {
            if let Some(asks) = asks(env.system) {
                ui.add_space(6.0);
                super::small(ui, asks.intro, 1);
                ui.add_space(8.0);
                for (what, why) in &asks.questions {
                    super::words(ui, what, semibold(13.0), TEXT, 18.0, 1);
                    super::words(ui, why, FontId::proportional(12.0), TEXT_DIM, 17.0, 1);
                    ui.add_space(8.0);
                }
                super::small(ui, asks.after, 1);
            }
        }
        Some(Fold::Reports) => {
            ui.add_space(6.0);
            let may_send = env.reports.as_ref().is_some_and(|standing| standing.may_send);
            crate::reports::ui::setup_fold(ui, state, may_send);
        }
        None => {}
    }
    if std::mem::take(&mut setup.fold_opened) {
        ui.scroll_to_rect(line, Some(egui::Align::TOP));
    }
}
