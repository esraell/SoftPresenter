//! The setup a fresh install starts with, so a church gets going on any computer without
//! knowing where things are: which display the congregation sees, bringing its songs in, which
//! Bibles go on top and below, and what the system will ask permission for (a Mac and Windows
//! ask; on Linux nothing does, and that step is left out). It is shown once, on the first start,
//! and can be opened again from Settings (and the SoftPresenter menu on a Mac). Every choice is
//! the same one the Screens window, the import menu and the Bible page make, so nothing here is
//! kept apart from them. Its words fit the system it runs on (`System`).

use std::path::PathBuf;

use egui::{Align, Align2, Color32, Frame, Layout, RichText, Sense, Stroke, vec2};
use sp_engine::{Command, Show, SlideRef};

use crate::displays::{DisplayInfo, kept_for_operator};
use crate::home::HomeState;
use crate::i18n::{t, tf, tn};
use crate::screens::{Kind, Output, Screen};
use crate::studio::{CARD, HOVER, READY, SELECTED, TEXT, TEXT_DIM, TEXT_FAINT, semibold};

/// The system the setup speaks of: what it asks permission for, what its menus are, what the
/// computer is called. A value rather than a build switch, so each system's words can be tried
/// on any computer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum System {
    Mac,
    Windows,
    Linux,
}

impl System {
    /// The one SoftPresenter is running on.
    pub const THIS: System = if cfg!(target_os = "macos") {
        System::Mac
    } else if cfg!(windows) {
        System::Windows
    } else {
        System::Linux
    };
}

/// The steps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Welcome,
    Screens,
    Songs,
    /// Free songbooks: those shared by churches (`free_songbooks`), and the song catalog's
    /// where it is offered (`songbooks`).
    FreeSongs,
    Bibles,
    Permissions,
    Done,
}

impl Step {
    /// Its name along the top.
    fn name(self) -> &'static str {
        match self {
            Step::Welcome => t("setup.step.welcome", "Welcome"),
            Step::Screens => t("setup.step.screens", "Screens"),
            Step::Songs => t("setup.step.songs", "Songs"),
            Step::FreeSongs => t("setup.step.free_songs", "Free Songs"),
            Step::Bibles => t("setup.step.bibles", "Bibles"),
            Step::Permissions => t("setup.step.permissions", "Permissions"),
            Step::Done => t("setup.step.done", "Done"),
        }
    }
}

/// The steps on `system`, in order. Linux has no Permissions step: nothing there asks. Free
/// Songs follows Songs: the songbooks shared by churches, and the song catalog's too where it is
/// offered (`catalog::offered`).
fn steps(system: System) -> Vec<Step> {
    let mut steps = vec![Step::Welcome, Step::Screens, Step::Songs, Step::FreeSongs];
    steps.push(Step::Bibles);
    if system != System::Linux {
        steps.push(Step::Permissions);
    }
    steps.push(Step::Done);
    steps
}

/// The setup while it is open.
#[derive(Clone, Debug)]
pub struct Setup {
    pub step: Step,
    /// A library folder of another presentation program found on this computer, how many song
    /// documents are in it, and in how many libraries.
    pub library_found: Option<(PathBuf, usize, usize)>,
    /// True once the Documents folder has been looked in for one. It is looked in only when
    /// asked on the Songs step: on a Mac the first look at Documents makes macOS ask permission,
    /// and the app waits for the answer, so it mustn't happen as the app opens with nothing said.
    pub looked: bool,
    /// What bringing songs in came to, in plain words.
    pub songs_said: Option<String>,
    /// The song catalog's songbooks ticked on the Free Songs step.
    pub free_songs: crate::songbooks::Picked,
    /// The free songbooks shared by churches ticked on the Free Songs step.
    pub shared: crate::free_songbooks::Picked,
}

impl Setup {
    pub fn new() -> Setup {
        Setup {
            step: Step::Welcome,
            library_found: None,
            looked: false,
            songs_said: None,
            free_songs: crate::songbooks::Picked::default(),
            shared: crate::free_songbooks::Picked::default(),
        }
    }

    /// The setup opened at the step a testing aid names (`free-songs`, `bibles`…), or at its
    /// start for any other value.
    pub fn opened_at(name: &str) -> Setup {
        let wanted = name.trim().to_ascii_lowercase().replace(['-', '_', ' '], "");
        let mut setup = Setup::new();
        let all =
            [Step::Welcome, Step::Screens, Step::Songs, Step::FreeSongs, Step::Bibles, Step::Permissions, Step::Done];
        if let Some(step) = all.into_iter().find(|step| format!("{step:?}").to_ascii_lowercase() == wanted) {
            setup.step = step;
        }
        setup
    }
}

impl Default for Setup {
    fn default() -> Self {
        Setup::new()
    }
}

/// What the setup asks of the app: things that need more than the window's own state.
#[derive(Clone, Debug, PartialEq)]
pub enum SetupRequest {
    /// Import a folder of song documents, each folder inside it becoming a library.
    ImportLibrary(PathBuf),
    /// Ask which song files to import.
    ChooseSongFiles,
}

/// The other presentation program's libraries folder in the home folder, if it is there, with how many song
/// documents it holds and in how many libraries. (Windows names the home folder USERPROFILE.)
fn library_found() -> Option<(PathBuf, usize, usize)> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    let folder = PathBuf::from(home).join("Documents").join("ProPresenter").join("Libraries");
    let mut songs = 0;
    let mut libraries = 0;
    for library in std::fs::read_dir(&folder).ok()?.flatten().map(|entry| entry.path()).filter(|p| p.is_dir()) {
        let count = std::fs::read_dir(&library)
            .map(|read| read.flatten().filter(|e| e.path().extension().is_some_and(|x| x == "pro")).count())
            .unwrap_or(0);
        if count > 0 {
            songs += count;
            libraries += 1;
        }
    }
    (songs > 0).then_some((folder, songs, libraries))
}

/// Draws the setup over the window while it is open. Closing it, at the end or early, marks
/// the setup done, so it isn't shown at the next start.
pub fn setup_ui(ctx: &egui::Context, show: &Show, state: &mut HomeState, commands: &mut Vec<Command>) {
    let Some(mut setup) = state.setup.take() else { return };
    let mut open = true;
    let frame = Frame::new()
        .fill(Color32::from_rgb(40, 40, 42))
        .stroke(Stroke::new(1.0, Color32::from_white_alpha(22)))
        .corner_radius(16.0)
        .inner_margin(egui::Margin { left: 28, right: 28, top: 22, bottom: 20 })
        .shadow(egui::Shadow { offset: [0, 18], blur: 50, spread: 0, color: Color32::from_black_alpha(160) });
    let area = egui::Modal::default_area(egui::Id::unique("setup")).anchor(Align2::CENTER_CENTER, vec2(0.0, -10.0));
    egui::Modal::new(egui::Id::unique("setup"))
        .area(area)
        .backdrop_color(Color32::from_black_alpha(110))
        .frame(frame)
        .show(ctx, |ui| {
            let system = System::THIS;
            let steps = steps(system);
            // A step this system doesn't have (Permissions on Linux) is never shown.
            if !steps.contains(&setup.step) {
                setup.step = steps[0];
            }
            ui.set_width(540.0);
            steps_bar(ui, setup.step, &steps);
            ui.add_space(18.0);
            let mut not_now = false;
            match setup.step {
                Step::Welcome => welcome(ui, system),
                Step::Screens => screens(ui, state),
                Step::Songs => songs(ui, &mut setup, state, system),
                Step::FreeSongs => not_now = free_songs(ui, &mut setup, state),
                Step::Bibles => bibles(ui, state),
                Step::Permissions => permissions(ui, system),
                Step::Done => done(ui, show, state, commands),
            }
            ui.add_space(20.0);
            let at = steps.iter().position(|step| *step == setup.step).unwrap_or(0);
            if not_now && at + 1 < steps.len() {
                setup.step = steps[at + 1];
            }
            ui.horizontal(|ui| {
                if at == 0 {
                    if link(ui, t("setup.skip", "Skip setup")).clicked() {
                        open = false;
                    }
                } else if button(ui, t("setup.back", "Back"), false).clicked() {
                    setup.step = steps[at - 1];
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let last = at + 1 == steps.len();
                    let label = match setup.step {
                        Step::Welcome => t("setup.get_started", "Get Started"),
                        _ if last => t("setup.start_using", "Start Using SoftPresenter"),
                        _ => t("setup.continue", "Continue"),
                    };
                    if button(ui, label, true).clicked() {
                        // Continue on Free Songs: what is ticked downloads once the setup closes.
                        if setup.step == Step::FreeSongs {
                            setup.free_songs.confirmed = true;
                            setup.shared.confirmed = true;
                        }
                        if last {
                            open = false;
                        } else {
                            setup.step = steps[at + 1];
                        }
                    }
                });
            });
        });
    if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
        open = false;
    }
    if open {
        state.setup = Some(setup);
    } else {
        state.setup_done = true;
        state.identify = false;
        crate::songbooks::setup_closed(&setup.free_songs, &mut state.songbooks);
        crate::free_songbooks::setup_closed(&setup.shared, &mut state.free_songbooks);
    }
}

/// The Free Songs step: the songbooks shared by churches, under the song catalog's where that
/// is offered. True when Not Now was clicked (nothing of either is downloaded).
fn free_songs(ui: &mut egui::Ui, setup: &mut Setup, state: &mut HomeState) -> bool {
    let not_now = if crate::catalog::offered() {
        setup.shared.confirmed = false;
        let (shared, view) = (&mut setup.shared, &mut state.free_songbooks);
        crate::songbooks::setup_step(ui, &mut setup.free_songs, &mut state.songbooks, |ui| {
            crate::free_songbooks::setup_section(ui, shared, view, true);
        })
    } else {
        crate::free_songbooks::setup_step(ui, &mut setup.shared, &mut state.free_songbooks)
    };
    // The catalog's Not Now clears its own ticks; the shared songbooks' go with them.
    if not_now {
        setup.shared.not_now();
    }
    not_now
}

/// The dots along the top: one for each step, the one now lit.
fn steps_bar(ui: &mut egui::Ui, now: Step, steps: &[Step]) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 14.0;
        let at = steps.iter().position(|step| *step == now).unwrap_or(0);
        for (index, name) in steps.iter().map(|step| step.name()).enumerate() {
            let ink = if index == at {
                TEXT
            } else if index < at {
                TEXT_DIM
            } else {
                TEXT_FAINT
            };
            let dot = if index < at {
                "●"
            } else if index == at {
                "◉"
            } else {
                "○"
            };
            ui.label(RichText::new(format!("{dot} {name}")).size(11.5).color(ink));
        }
    });
}

pub(crate) fn title(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).font(semibold(20.0)).color(Color32::WHITE));
    ui.add_space(6.0);
}

pub(crate) fn say(ui: &mut egui::Ui, text: &str) {
    ui.add(egui::Label::new(RichText::new(text).size(13.0).color(TEXT_DIM)).wrap());
}

/// A rounded button; `main` for the one that goes on.
pub(crate) fn button(ui: &mut egui::Ui, label: &str, main: bool) -> egui::Response {
    let font = if main { semibold(13.0) } else { egui::FontId::proportional(13.0) };
    let width = ui.painter().layout_no_wrap(label.to_owned(), font.clone(), TEXT).size().x + 32.0;
    let (place, response) = ui.allocate_exact_size(vec2(width.max(90.0), 30.0), Sense::click());
    let fill = match (main, response.hovered()) {
        (true, true) => Color32::from_rgb(70, 140, 255),
        (true, false) => Color32::from_rgb(10, 110, 245),
        (false, true) => HOVER,
        (false, false) => CARD,
    };
    ui.painter().rect_filled(place, 8.0, fill);
    ui.painter().text(place.center(), Align2::CENTER_CENTER, label, font, Color32::WHITE);
    response
}

pub(crate) fn link(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add(egui::Label::new(RichText::new(label).size(12.5).color(TEXT_FAINT)).sense(Sense::click()))
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A choice in a list, lit when chosen.
fn choice(ui: &mut egui::Ui, chosen: bool, label: &str, detail: &str) -> egui::Response {
    let width = ui.available_width();
    let (place, response) = ui.allocate_exact_size(vec2(width, 40.0), Sense::click());
    let fill = if chosen {
        SELECTED
    } else if response.hovered() {
        HOVER
    } else {
        CARD
    };
    ui.painter().rect_filled(place, 9.0, fill);
    let mark = if chosen { "◉" } else { "○" };
    let ink = if chosen { READY } else { TEXT_FAINT };
    ui.painter().text(
        place.left_center() + vec2(16.0, 0.0),
        Align2::LEFT_CENTER,
        mark,
        egui::FontId::proportional(14.0),
        ink,
    );
    ui.painter().text(place.left_center() + vec2(40.0, -7.0), Align2::LEFT_CENTER, label, semibold(13.0), TEXT);
    ui.painter().text(
        place.left_center() + vec2(40.0, 9.0),
        Align2::LEFT_CENTER,
        detail,
        egui::FontId::proportional(11.0),
        TEXT_DIM,
    );
    response
}

/// What the setup is for, and where each thing can be changed later: songs come in from the
/// File menu on a Mac, and from the import button in the left column where there is no menu bar.
fn welcome_about(system: System) -> &'static str {
    match system {
        System::Mac => t(
            "setup.welcome.about",
            "A minute to get ready for Sunday: which screen the congregation sees, bringing your songs in, and your Bibles. Everything can be changed later, in the Screens window, the File menu and the Bible page.",
        ),
        System::Windows | System::Linux => t(
            "setup.welcome.about_no_menu_bar",
            "A minute to get ready for Sunday: which screen the congregation sees, bringing your songs in, and your Bibles. Everything can be changed later: in the Screens window, with the \u{2193} button in the left column, and on the Bible page.",
        ),
    }
}

fn welcome(ui: &mut egui::Ui, system: System) {
    title(ui, t("setup.welcome.title", "Welcome to SoftPresenter"));
    say(ui, welcome_about(system));
    // The words of the app itself, first of all, so the rest of the setup is read in them.
    ui.add_space(12.0);
    ui.label(t("setup.welcome.language", "Language of SoftPresenter"));
    ui.add_space(4.0);
    crate::home::language_menu(ui);
}

/// The displays the audience may be given, with their sizes: each one the operator doesn't work
/// on (`displays::kept_for_operator`). None with one display.
fn audience_displays(displays: &[DisplayInfo], operator: &[usize]) -> Vec<(String, String)> {
    displays
        .iter()
        .filter(|display| kept_for_operator(displays, operator, display.index).is_none())
        .map(|display| (display.name.clone(), format!("{} × {}", display.w, display.h)))
        .collect()
}

/// What the Screens step says when only one display is connected (`name`, when it is known):
/// that is where the operator works, and the congregation sees SoftPresenter on a TV or
/// projector, or over NDI (`ndi`, when it is installed). What happens until one is plugged in
/// depends on the choice, so each choice says it.
fn one_display_words(name: Option<&str>, ndi: bool) -> String {
    let mut words = match name {
        Some(name) => tf(
            "setup.screens.one.named",
            "Only one display is connected ({name}). This is where you work, so the audience screen never covers it: the congregation sees it on a TV or projector.",
            &[("name", name)],
        ),
        None => t(
            "setup.screens.one",
            "Only one display is connected. This is where you work, so the audience screen never covers it: the congregation sees it on a TV or projector.",
        )
        .to_owned(),
    };
    if ndi {
        words.push(' ');
        words.push_str(t("setup.screens.one.ndi", "It can also be sent over NDI, to a stream or a video switcher."));
    }
    words
}

/// The Screens step's choices with one display.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OneDisplayChoice {
    /// In a window for now, and on a TV or projector as soon as one is plugged in (Automatic).
    Automatic,
    /// Only in a window, even with a TV or projector plugged in: shown only while it is the
    /// screen's output (an earlier setup gave it, and the Screens window can).
    Window,
    Ndi,
    /// Shown nowhere for now: off, or anything the Screens window gave that the step doesn't offer.
    NotNow,
}

/// Which one-display choice is lit for the audience's `output`: exactly one, whatever it is.
/// NDI counts only while it is installed (`ndi`): without it the screen shows nowhere.
fn one_display_choice(output: &Output, ndi: bool) -> OneDisplayChoice {
    match output {
        Output::Automatic => OneDisplayChoice::Automatic,
        Output::Window => OneDisplayChoice::Window,
        Output::Ndi(_) if ndi => OneDisplayChoice::Ndi,
        _ => OneDisplayChoice::NotNow,
    }
}

/// What the Screens step says with several displays: which one is kept for SoftPresenter's own
/// window. Where the window system doesn't say which display is the main one (Wayland), it is
/// the one the window is on, and those displays aren't offered.
fn several_displays_words(main: Option<&str>) -> String {
    match main {
        Some(name) => tf(
            "setup.screens.several.main",
            "The main display ({name}) is kept for SoftPresenter's own window. Choose the projector or TV; the screen goes there straight away so you can check.",
            &[("name", name)],
        ),
        None => t(
            "setup.screens.several.window",
            "The display SoftPresenter's own window is on is kept for it. Choose the projector or TV; the screen goes there straight away so you can check.",
        )
        .to_owned(),
    }
}

/// Which display the congregation sees, and a stage display for the band if there is another.
/// Choosing one puts the screen there at once, so the person can look at it. With one display
/// the audience screen is left to choose for itself (Automatic) unless told otherwise: in a
/// window until a TV or projector is plugged in, which it then fills at once (`screens::settle`).
fn screens(ui: &mut egui::Ui, state: &mut HomeState) {
    title(ui, t("setup.screens.title", "Which screen does the congregation see?"));
    let others = audience_displays(&state.displays, &state.operator_displays);
    let audience = match state.screens.iter().position(|screen| screen.kind == Kind::Audience) {
        Some(at) => at,
        None => {
            state.screens.insert(0, Screen::new("Screen 1".to_owned(), Output::Off));
            0
        }
    };
    if others.is_empty() {
        // The audience on the one display (given by a version that offered it, as on Wayland,
        // where no display is the main one) shows nowhere: it goes back to choosing for itself,
        // which shows it in a window and covers nothing.
        if let Output::Display(name) = &state.screens[audience].output
            && let Some(display) = state.displays.iter().find(|d| d.is_named(name))
            && kept_for_operator(&state.displays, &state.operator_displays, display.index).is_some()
        {
            state.screens[audience].output = Output::Automatic;
            state.screens_changed = true;
        }
        say(ui, &one_display_words(state.displays.first().map(|d| d.name.as_str()), state.ndi_available));
        ui.add_space(10.0);
        let lit = one_display_choice(&state.screens[audience].output, state.ndi_available);
        let set = |output: Output, state: &mut HomeState| {
            state.screens[audience].output = output;
            state.screens_changed = true;
        };
        if choice(
            ui,
            lit == OneDisplayChoice::Automatic,
            t("setup.screens.automatic", "In a window for now"),
            t("setup.screens.automatic.detail", "On a TV or projector as soon as one is plugged in"),
        )
        .clicked()
        {
            set(Output::Automatic, state);
        }
        ui.add_space(6.0);
        if lit == OneDisplayChoice::Window {
            choice(
                ui,
                true,
                t("setup.screens.window", "Only in a window"),
                t("setup.screens.window.detail", "Stays in a window, even with a TV or projector plugged in"),
            );
            ui.add_space(6.0);
        }
        if state.ndi_available
            && choice(
                ui,
                lit == OneDisplayChoice::Ndi,
                t("setup.screens.ndi", "Send over NDI"),
                t("setup.screens.ndi.detail", "For a stream or a video switcher"),
            )
            .clicked()
        {
            set(Output::Ndi("SoftPresenter Audience".to_owned()), state);
        }
        if state.ndi_available {
            ui.add_space(6.0);
        }
        if choice(
            ui,
            lit == OneDisplayChoice::NotNow,
            t("setup.screens.not_now", "Not now"),
            t("setup.screens.not_now.detail", "Choose in the Screens window later"),
        )
        .clicked()
        {
            set(Output::Off, state);
        }
        return;
    }
    say(ui, &several_displays_words(state.displays.iter().find(|d| d.main).map(|d| d.name.as_str())));
    ui.add_space(10.0);
    // The display the audience has, among those offered: on one kept for the operator it shows
    // nowhere, so "Not now" is lit, and it is left for the person to choose (nothing moves to
    // another display without a click).
    let current = match &state.screens[audience].output {
        Output::Display(name) => state
            .displays
            .iter()
            .find(|d| {
                d.is_named(name) && kept_for_operator(&state.displays, &state.operator_displays, d.index).is_none()
            })
            .map(|d| d.name.clone()),
        _ => None,
    };
    for (name, size) in &others {
        if choice(ui, current.as_deref() == Some(name), name, size).clicked() {
            state.screens[audience].output = Output::Display(name.clone());
            state.screens_changed = true;
        }
        ui.add_space(6.0);
    }
    if choice(
        ui,
        current.is_none(),
        t("setup.screens.not_now", "Not now"),
        t("setup.screens.not_now.display", "Choose a display later in the Screens window"),
    )
    .clicked()
    {
        state.screens[audience].output = Output::Off;
        state.screens_changed = true;
    }
    // A stage display for the band, on a display the audience isn't using.
    let free: Vec<&(String, String)> = others.iter().filter(|(name, _)| current.as_deref() != Some(name)).collect();
    if !free.is_empty() {
        ui.add_space(14.0);
        ui.label(
            RichText::new(t("setup.screens.stage", "A screen for the band (stage display)?"))
                .font(semibold(13.5))
                .color(TEXT),
        );
        ui.add_space(6.0);
        let stage = state.screens.iter().position(|screen| screen.kind == Kind::Stage);
        let stage_on = stage.and_then(|at| match &state.screens[at].output {
            Output::Display(name) => Some(name.clone()),
            _ => None,
        });
        for (name, size) in free {
            if choice(ui, stage_on.as_deref() == Some(name), name, size).clicked() {
                match stage {
                    Some(at) => state.screens[at].output = Output::Display(name.clone()),
                    None => state.screens.push(Screen::stage(
                        t("setup.screens.stage.name", "Stage").to_owned(),
                        Output::Display(name.clone()),
                    )),
                }
                state.screens_changed = true;
            }
            ui.add_space(6.0);
        }
        if choice(ui, stage_on.is_none(), t("setup.screens.stage.none", "No stage display"), "").clicked()
            && let Some(at) = stage
        {
            state.screens[at].output = Output::Off;
            state.screens_changed = true;
        }
    }
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        let identify = if state.identify {
            t("setup.screens.identify.stop", "Stop Identifying")
        } else {
            t("setup.screens.identify", "Identify Screens")
        };
        if button(ui, identify, false).clicked() {
            state.identify = !state.identify;
        }
        ui.label(
            RichText::new(t("setup.screens.identify.about", "Shows each screen's name on it"))
                .size(11.5)
                .color(TEXT_FAINT),
        );
    });
}

/// What the Songs step says before looking: a Mac asks before the Documents folder is read.
fn songs_look_words(system: System) -> &'static str {
    match system {
        System::Mac => t(
            "setup.songs.look.mac",
            "SoftPresenter can look in your Documents folder for a library of song documents (.pro files) to bring across. macOS may ask whether SoftPresenter can use the Documents folder: choose OK.",
        ),
        System::Windows | System::Linux => t(
            "setup.songs.look",
            "SoftPresenter can look in your Documents folder for a library of song documents (.pro files) to bring across.",
        ),
    }
}

/// What the Songs step says after looking: what was found, or which song files come in (Word
/// files only on a Mac, whose own converter reads them).
fn songs_found_words(system: System, found: Option<(usize, usize)>) -> String {
    let mac = system == System::Mac;
    match found {
        Some((songs, libraries)) => {
            let songs = crate::i18n::count(songs as u64);
            let libraries = libraries as u64;
            if mac {
                tn(
                    "setup.songs.found.mac",
                    libraries,
                    "A library of song documents was found on this Mac: {songs} songs in {n} library. Each library comes across as a library of the same name, with its groups, arrangements and both languages.",
                    "A library of song documents was found on this Mac: {songs} songs in {n} libraries. Each library comes across as a library of the same name, with its groups, arrangements and both languages.",
                    &[("songs", &songs)],
                )
            } else {
                tn(
                    "setup.songs.found",
                    libraries,
                    "A library of song documents was found on this computer: {songs} songs in {n} library. Each library comes across as a library of the same name, with its groups, arrangements and both languages.",
                    "A library of song documents was found on this computer: {songs} songs in {n} libraries. Each library comes across as a library of the same name, with its groups, arrangements and both languages.",
                    &[("songs", &songs)],
                )
            }
        }
        None if mac => t(
            "setup.songs.none.mac",
            "No library of song documents was found in the Documents folder. Song files from anywhere come in: text, lyric files from song services, ChordPro, Word, and .pro song documents.",
        )
        .to_owned(),
        None => t(
            "setup.songs.none",
            "No library of song documents was found in the Documents folder. Song files from anywhere come in: text, lyric files from song services, ChordPro, and .pro song documents.",
        )
        .to_owned(),
    }
}

fn songs(ui: &mut egui::Ui, setup: &mut Setup, state: &mut HomeState, system: System) {
    title(ui, t("setup.songs.title", "Bring your songs in"));
    if !setup.looked {
        say(ui, songs_look_words(system));
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if button(ui, t("setup.songs.look_for", "Look for a Song Library"), true).clicked() {
                setup.library_found = library_found();
                setup.looked = true;
            }
            if button(ui, t("setup.songs.choose", "Choose Song Files…"), false).clicked() {
                state.setup_request = Some(SetupRequest::ChooseSongFiles);
            }
        });
        ui.add_space(8.0);
        say(
            ui,
            t(
                "setup.songs.skip",
                "Or skip this and start with the sample song. Songs can be imported at any time with the \u{2193} button in the left column, or dropped on the window.",
            ),
        );
        if let Some(said) = &setup.songs_said {
            ui.add_space(10.0);
            ui.label(RichText::new(said).size(12.5).color(READY));
        }
        return;
    }
    let found = setup.library_found.as_ref().map(|(_, songs, libraries)| (*songs, *libraries));
    say(ui, &songs_found_words(system, found));
    ui.add_space(10.0);
    if let Some((folder, _, _)) = &setup.library_found {
        if button(ui, t("setup.songs.import", "Import This Library"), true).clicked() {
            state.setup_request = Some(SetupRequest::ImportLibrary(folder.clone()));
        }
        ui.add_space(10.0);
    }
    if button(ui, t("setup.songs.choose", "Choose Song Files…"), false).clicked() {
        state.setup_request = Some(SetupRequest::ChooseSongFiles);
    }
    ui.add_space(8.0);
    say(
        ui,
        t(
            "setup.songs.skip",
            "Or skip this and start with the sample song. Songs can be imported at any time with the \u{2193} button in the left column, or dropped on the window.",
        ),
    );
    if let Some(said) = &setup.songs_said {
        ui.add_space(10.0);
        ui.label(RichText::new(said).size(12.5).color(READY));
    }
}

/// Which translation goes on top (the main words) and which below (the translation).
fn bibles(ui: &mut egui::Ui, state: &mut HomeState) {
    title(ui, t("setup.bibles.title", "Your Bibles"));
    say(
        ui,
        t(
            "setup.bibles.about",
            "Bible passages show two translations, one above the other. Choose which goes on top and which below.",
        ),
    );
    ui.add_space(12.0);
    let names: Vec<String> = state.bible.shelf.bibles.iter().map(|b| format!("{} ({})", b.name, b.short)).collect();
    if names.is_empty() || state.bible.rows.is_empty() {
        say(
            ui,
            t(
                "setup.bibles.none_installed",
                "No Bibles are installed yet. Open the Bible page's Options to download one.",
            ),
        );
        return;
    }
    let mut changed = false;
    egui::Grid::new("setup bibles").num_columns(2).spacing([14.0, 10.0]).show(ui, |ui| {
        ui.label(RichText::new(t("setup.bibles.top", "On top")).color(TEXT));
        let top = state.bible.rows[0].bible.unwrap_or(0).min(names.len() - 1);
        egui::ComboBox::from_id_salt("setup top bible").width(340.0).selected_text(names[top].clone()).show_ui(
            ui,
            |ui| {
                for (at, name) in names.iter().enumerate() {
                    if ui.selectable_label(at == top, name).clicked() {
                        state.bible.rows[0].bible = Some(at);
                        changed = true;
                    }
                }
            },
        );
        ui.end_row();
        ui.label(RichText::new(t("setup.bibles.below", "Below")).color(TEXT));
        let below = state.bible.rows.get(1).and_then(|row| row.bible);
        let none = t("setup.bibles.none", "None (one translation)");
        let shown = below.and_then(|at| names.get(at).cloned()).unwrap_or_else(|| none.to_owned());
        egui::ComboBox::from_id_salt("setup second bible").width(340.0).selected_text(shown).show_ui(ui, |ui| {
            if ui.selectable_label(below.is_none(), none).clicked() {
                if let Some(row) = state.bible.rows.get_mut(1) {
                    row.bible = None;
                }
                changed = true;
            }
            for (at, name) in names.iter().enumerate() {
                if ui.selectable_label(below == Some(at), name).clicked() {
                    if state.bible.rows.len() < 2 && state.bible.add_row() {
                        state.bible.assign_boxes();
                    }
                    if let Some(row) = state.bible.rows.get_mut(1) {
                        row.bible = Some(at);
                    }
                    changed = true;
                }
            }
        });
        ui.end_row();
    });
    if changed {
        state.bible.made_with.clear();
    }
    ui.add_space(10.0);
    say(ui, t("setup.bibles.more", "More translations can be downloaded from the Bible page's Options."));
}

/// What the system will ask permission for: the Permissions step's title, what it says first,
/// each question with when it comes, and what to do after a wrong answer.
struct Asks {
    title: &'static str,
    intro: &'static str,
    questions: Vec<(&'static str, &'static str)>,
    after: &'static str,
}

/// What `system` asks, from what SoftPresenter does there: on a Mac, the network for NDI, the
/// Documents folder and controlling the app that draws a presentation as pictures (by script);
/// on Windows only the firewall, the first time SoftPresenter listens on the network (NDI and
/// Remote Control), as the app that draws a presentation is asked through its own automation,
/// which Windows doesn't ask about. Nothing asks on Linux, which has no such step.
fn asks(system: System) -> Option<Asks> {
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

fn permissions(ui: &mut egui::Ui, system: System) {
    let Some(asks) = asks(system) else { return };
    title(ui, asks.title);
    say(ui, asks.intro);
    ui.add_space(10.0);
    for (what, why) in &asks.questions {
        ui.label(RichText::new(*what).font(semibold(13.0)).color(TEXT));
        say(ui, why);
        ui.add_space(8.0);
    }
    say(ui, asks.after);
}

fn done(ui: &mut egui::Ui, show: &Show, state: &mut HomeState, commands: &mut Vec<Command>) {
    title(ui, t("setup.done.title", "You're ready"));
    say(
        ui,
        t(
            "setup.done.about",
            "Click a slide to put it on screen, press the arrow keys or the space bar to go on, and Escape to clear. The Bible page looks passages up as you type, such as \"John 3 16\" or \"Ин 3:16\".",
        ),
    );
    ui.add_space(12.0);
    if !show.presentations().is_empty() && button(ui, t("setup.done.test_slide", "Show a Test Slide"), false).clicked()
    {
        commands.push(Command::GoTo(SlideRef { presentation: show.selected(), slide: 0 }));
        state.bible_open = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{Language, with_language};

    /// Every word the steps say on `system` that doesn't need a window, in English.
    fn all_words(system: System) -> String {
        with_language(Language::English, || {
            let mut words = vec![
                welcome_about(system).to_owned(),
                songs_look_words(system).to_owned(),
                songs_found_words(system, Some((12, 1))),
                songs_found_words(system, None),
                one_display_words(Some("DP-1"), true),
                several_displays_words(Some("Built-in")),
                several_displays_words(None),
            ];
            if let Some(asks) = asks(system) {
                words.extend([asks.title, asks.intro, asks.after].map(str::to_owned));
                words.extend(asks.questions.iter().flat_map(|(what, why)| [what.to_string(), why.to_string()]));
            }
            words.extend(steps(system).iter().map(|step| step.name().to_owned()));
            words.push(crate::free_songbooks::later_words().to_owned());
            words.join("\n")
        })
    }

    #[test]
    fn off_a_mac_the_setup_never_speaks_of_one() {
        for system in [System::Windows, System::Linux] {
            let words = all_words(system);
            for mac in
                ["Mac", "macOS", "Finder", "System Settings", "\u{2318}", "Command-", "menu bar", "File menu", "Word"]
            {
                assert!(!words.contains(mac), "{system:?} says {mac:?}:\n{words}");
            }
        }
    }

    #[test]
    fn a_mac_hears_what_macos_asks_as_before() {
        let words = all_words(System::Mac);
        for said in ["macOS may ask", "on this Mac", "the File menu", "Word", "What macOS will ask", "Local Network"] {
            assert!(words.contains(said), "a Mac doesn't hear {said:?}:\n{words}");
        }
        assert_eq!(steps(System::Mac).len(), 7);
    }

    #[test]
    fn free_songs_follows_songs_in_every_build() {
        // The songbooks shared by churches are offered in every build, the song catalog's too
        // where it is offered: the step follows Songs on every system.
        for system in [System::Mac, System::Windows, System::Linux] {
            let steps = steps(system);
            let songs = steps.iter().position(|step| *step == Step::Songs);
            assert_eq!(steps.get(songs.map_or(0, |at| at + 1)), Some(&Step::FreeSongs), "{system:?}");
        }
        // A testing aid opens it at a step by name.
        assert_eq!(Setup::opened_at("free-songs").step, Step::FreeSongs);
        assert_eq!(Setup::opened_at("Bibles").step, Step::Bibles);
        assert_eq!(Setup::opened_at("1").step, Step::Welcome);
    }

    #[test]
    fn windows_hears_only_of_its_firewall_and_linux_of_nothing() {
        let windows = all_words(System::Windows);
        for said in ["What Windows will ask", "Windows Defender Firewall", "private networks", "NDI", "Remote Control"]
        {
            assert!(windows.contains(said), "Windows doesn't hear {said:?}:\n{windows}");
        }
        assert!(windows.contains("this computer") && windows.contains("\u{2193} button in the left column"));
        assert!(steps(System::Windows).contains(&Step::Permissions));
        // Nothing asks on Linux, so it has no Permissions step.
        assert!(asks(System::Linux).is_none());
        assert_eq!(
            steps(System::Linux),
            [Step::Welcome, Step::Screens, Step::Songs, Step::FreeSongs, Step::Bibles, Step::Done]
        );
        assert!(!all_words(System::Linux).contains("Permissions"));
    }

    fn display(index: usize, name: &str, main: bool) -> DisplayInfo {
        DisplayInfo { index, name: name.to_owned(), main, w: 1920, h: 1080, ..Default::default() }
    }

    #[test]
    fn one_display_is_never_offered_to_the_audience() {
        // The owner's Wayland computer: one display, which nothing says is the main one.
        let one = [display(0, "DP-1", false)];
        assert!(audience_displays(&one, &[]).is_empty());
        assert!(audience_displays(&one, &[0]).is_empty());
        let words = with_language(Language::English, || one_display_words(Some("DP-1"), false));
        assert!(words.contains("(DP-1)") && words.contains("where you work") && words.contains("TV or projector"));
        assert!(!words.contains("NDI"), "{words}");
        assert!(one_display_words(None, true).contains("NDI"));
        // What happens until a TV is plugged in depends on the choice, so the words don't say it.
        assert!(!words.contains("window") && !words.contains("straight away"), "{words}");
    }

    #[test]
    fn with_one_display_exactly_the_right_choice_is_lit() {
        use OneDisplayChoice::*;
        let ndi = || Output::Ndi("SoftPresenter Audience".to_owned());
        for (output, installed, lit) in [
            (Output::Automatic, true, Automatic),
            // An earlier setup's "In a window": shown as what it is, never as Automatic.
            (Output::Window, true, Window),
            (ndi(), true, Ndi),
            // NDI without NDI installed shows nowhere.
            (ndi(), false, NotNow),
            (Output::Off, true, NotNow),
            // A projector that isn't plugged in.
            (Output::Display("Projector".to_owned()), true, NotNow),
        ] {
            assert_eq!(one_display_choice(&output, installed), lit, "{output:?}");
        }
    }

    #[test]
    fn several_displays_offer_all_but_the_operators() {
        let mac = [display(0, "Built-in", true), display(1, "Projector", false)];
        assert_eq!(audience_displays(&mac, &[0]), [("Projector".to_owned(), "1920 × 1080".to_owned())]);
        // Wayland: the display the home window is on stands for the main one, once it is known.
        let wayland = [display(0, "DP-1", false), display(1, "HDMI-1", false)];
        let names = |list: Vec<(String, String)>| list.into_iter().map(|(name, _)| name).collect::<Vec<_>>();
        assert_eq!(names(audience_displays(&wayland, &[0])), ["HDMI-1"]);
        assert_eq!(names(audience_displays(&wayland, &[])), ["DP-1", "HDMI-1"]);
        let (main, window) = with_language(Language::English, || {
            (several_displays_words(Some("Built-in")), several_displays_words(None))
        });
        assert!(main.starts_with("The main display (Built-in) is kept"));
        assert!(window.starts_with("The display SoftPresenter's own window is on is kept"));
    }
}
