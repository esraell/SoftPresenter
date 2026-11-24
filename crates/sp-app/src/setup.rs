//! The setup a fresh install starts with, so a church gets going on any Mac without knowing
//! where things are: which display the congregation sees, bringing its songs in, which Bibles
//! go on top and below, and what macOS will ask permission for. It is shown once, on the first
//! start, and can be opened again from the SoftPresenter menu. Every choice is the same one
//! the Screens window, File menu and Bible page make, so nothing here is kept apart from them.

use std::path::PathBuf;

use egui::{Align, Align2, Color32, Frame, Layout, RichText, Sense, Stroke, vec2};
use sp_engine::{Command, Show, SlideRef};

use crate::home::HomeState;
use crate::screens::{Kind, Output, Screen};
use crate::studio::{CARD, HOVER, READY, SELECTED, TEXT, TEXT_DIM, TEXT_FAINT, semibold};

/// The steps, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Welcome,
    Screens,
    Songs,
    Bibles,
    Permissions,
    Done,
}

const STEPS: [Step; 6] = [Step::Welcome, Step::Screens, Step::Songs, Step::Bibles, Step::Permissions, Step::Done];

/// The setup while it is open.
#[derive(Clone, Debug)]
pub struct Setup {
    pub step: Step,
    /// A library folder of another presentation program found on this Mac, how many song
    /// documents are in it, and in how many libraries.
    pub library_found: Option<(PathBuf, usize, usize)>,
    /// True once the Documents folder has been looked in for one. It is looked in only when
    /// asked on the Songs step: the first look at Documents makes macOS ask permission, and the
    /// app waits for the answer, so it mustn't happen as the app opens with nothing said.
    pub looked: bool,
    /// What bringing songs in came to, in plain words.
    pub songs_said: Option<String>,
}

impl Setup {
    pub fn new() -> Setup {
        Setup { step: Step::Welcome, library_found: None, looked: false, songs_said: None }
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

/// The ProPresenter libraries folder in the home folder, if it is there, with how many song
/// documents it holds and in how many libraries.
fn library_found() -> Option<(PathBuf, usize, usize)> {
    let folder = PathBuf::from(std::env::var_os("HOME")?).join("Documents").join("ProPresenter").join("Libraries");
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
            ui.set_width(540.0);
            steps_bar(ui, setup.step);
            ui.add_space(18.0);
            match setup.step {
                Step::Welcome => welcome(ui),
                Step::Screens => screens(ui, state),
                Step::Songs => songs(ui, &mut setup, state),
                Step::Bibles => bibles(ui, state),
                Step::Permissions => permissions(ui),
                Step::Done => done(ui, show, state, commands),
            }
            ui.add_space(20.0);
            let at = STEPS.iter().position(|step| *step == setup.step).unwrap_or(0);
            ui.horizontal(|ui| {
                if at == 0 {
                    if link(ui, "Skip setup").clicked() {
                        open = false;
                    }
                } else if button(ui, "Back", false).clicked() {
                    setup.step = STEPS[at - 1];
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let last = at + 1 == STEPS.len();
                    let label = match setup.step {
                        Step::Welcome => "Get Started",
                        _ if last => "Start Using SoftPresenter",
                        _ => "Continue",
                    };
                    if button(ui, label, true).clicked() {
                        if last {
                            open = false;
                        } else {
                            setup.step = STEPS[at + 1];
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
    }
}

/// The dots along the top: one for each step, the one now lit.
fn steps_bar(ui: &mut egui::Ui, now: Step) {
    let names = ["Welcome", "Screens", "Songs", "Bibles", "Permissions", "Done"];
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 14.0;
        let at = STEPS.iter().position(|step| *step == now).unwrap_or(0);
        for (index, name) in names.iter().enumerate() {
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

fn title(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).font(semibold(20.0)).color(Color32::WHITE));
    ui.add_space(6.0);
}

fn say(ui: &mut egui::Ui, text: &str) {
    ui.add(egui::Label::new(RichText::new(text).size(13.0).color(TEXT_DIM)).wrap());
}

/// A rounded button; `main` for the one that goes on.
fn button(ui: &mut egui::Ui, label: &str, main: bool) -> egui::Response {
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

fn link(ui: &mut egui::Ui, label: &str) -> egui::Response {
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

fn welcome(ui: &mut egui::Ui) {
    title(ui, "Welcome to SoftPresenter");
    say(
        ui,
        "A minute to get ready for Sunday: which screen the congregation sees, bringing your songs in, and your Bibles. \
         Everything can be changed later, in the Screens window, the File menu and the Bible page.",
    );
}

/// Which display the congregation sees, and a stage display for the band if there is another.
/// Choosing one puts the screen there at once, so the person can look at it.
fn screens(ui: &mut egui::Ui, state: &mut HomeState) {
    title(ui, "Which screen does the congregation see?");
    let others: Vec<(String, String)> = state
        .displays
        .iter()
        .filter(|display| !display.main)
        .map(|display| (display.name.clone(), format!("{} × {}", display.w, display.h)))
        .collect();
    let audience = match state.screens.iter().position(|screen| screen.kind == Kind::Audience) {
        Some(at) => at,
        None => {
            state.screens.insert(0, Screen::new("Screen 1".to_owned(), Output::Off));
            0
        }
    };
    let main_name = state.displays.iter().find(|d| d.main).map_or_else(String::new, |d| d.name.clone());
    if others.is_empty() {
        say(
            ui,
            &format!(
                "Only one display is connected{}. The main display is kept for SoftPresenter's own window, so the audience \
                 screen opens in a window for now. Connect the projector or TV and choose it here or in the Screens window.",
                if main_name.is_empty() { String::new() } else { format!(" ({main_name})") }
            ),
        );
        ui.add_space(10.0);
        let output = state.screens[audience].output.clone();
        let set = |output: Output, state: &mut HomeState| {
            state.screens[audience].output = output;
            state.screens_changed = true;
        };
        if choice(
            ui,
            matches!(output, Output::Window | Output::Automatic),
            "In a window",
            "Move it to a projector later",
        )
        .clicked()
        {
            set(Output::Window, state);
        }
        ui.add_space(6.0);
        if state.ndi_available
            && choice(ui, matches!(output, Output::Ndi(_)), "Send over NDI", "For a stream or a video switcher")
                .clicked()
        {
            set(Output::Ndi("SoftPresenter Audience".to_owned()), state);
        }
        return;
    }
    say(
        ui,
        &format!(
            "The main display{} is kept for SoftPresenter's own window. Choose the projector or TV; the screen goes \
             there straight away so you can check.",
            if main_name.is_empty() { String::new() } else { format!(" ({main_name})") }
        ),
    );
    ui.add_space(10.0);
    let current = match &state.screens[audience].output {
        Output::Display(name) => state.displays.iter().find(|d| d.is_named(name)).map(|d| d.name.clone()),
        _ => None,
    };
    for (name, size) in &others {
        if choice(ui, current.as_deref() == Some(name), name, size).clicked() {
            state.screens[audience].output = Output::Display(name.clone());
            state.screens_changed = true;
        }
        ui.add_space(6.0);
    }
    if choice(ui, current.is_none(), "Not now", "Choose a display later in the Screens window").clicked() {
        state.screens[audience].output = Output::Off;
        state.screens_changed = true;
    }
    // A stage display for the band, on a display the audience isn't using.
    let free: Vec<&(String, String)> = others.iter().filter(|(name, _)| current.as_deref() != Some(name)).collect();
    if !free.is_empty() {
        ui.add_space(14.0);
        ui.label(RichText::new("A screen for the band (stage display)?").font(semibold(13.5)).color(TEXT));
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
                    None => state.screens.push(Screen::stage("Stage".to_owned(), Output::Display(name.clone()))),
                }
                state.screens_changed = true;
            }
            ui.add_space(6.0);
        }
        if choice(ui, stage_on.is_none(), "No stage display", "").clicked()
            && let Some(at) = stage
        {
            state.screens[at].output = Output::Off;
            state.screens_changed = true;
        }
    }
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if button(ui, if state.identify { "Stop Identifying" } else { "Identify Screens" }, false).clicked() {
            state.identify = !state.identify;
        }
        ui.label(RichText::new("Shows each screen's name on it").size(11.5).color(TEXT_FAINT));
    });
}

fn songs(ui: &mut egui::Ui, setup: &mut Setup, state: &mut HomeState) {
    title(ui, "Bring your songs in");
    if !setup.looked {
        say(
            ui,
            "SoftPresenter can look in your Documents folder for a library of song documents (.pro files) to bring \
             across. macOS may ask whether SoftPresenter can use the Documents folder: choose OK.",
        );
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if button(ui, "Look for a Song Library", true).clicked() {
                setup.library_found = library_found();
                setup.looked = true;
            }
            if button(ui, "Choose Song Files…", false).clicked() {
                state.setup_request = Some(SetupRequest::ChooseSongFiles);
            }
        });
        ui.add_space(8.0);
        say(
            ui,
            "Or skip this and start with the sample song. Songs can be imported at any time with the \u{2193} button in the left column, or dropped on the window.",
        );
        if let Some(said) = &setup.songs_said {
            ui.add_space(10.0);
            ui.label(RichText::new(said).size(12.5).color(READY));
        }
        return;
    }
    if let Some((folder, songs, libraries)) = &setup.library_found {
        say(
            ui,
            &format!(
                "A library of song documents was found on this Mac: {songs} songs in {libraries} {}. Each library comes across \
                 as a library of the same name, with its groups, arrangements and both languages.",
                if *libraries == 1 { "library" } else { "libraries" }
            ),
        );
        ui.add_space(10.0);
        if button(ui, "Import This Library", true).clicked() {
            state.setup_request = Some(SetupRequest::ImportLibrary(folder.clone()));
        }
        ui.add_space(10.0);
    } else {
        say(
            ui,
            "No library of song documents was found in the Documents folder. Song files from anywhere come in: text, lyric \
             files from song services, ChordPro, Word, and .pro song documents.",
        );
        ui.add_space(10.0);
    }
    if button(ui, "Choose Song Files…", false).clicked() {
        state.setup_request = Some(SetupRequest::ChooseSongFiles);
    }
    ui.add_space(8.0);
    say(
        ui,
        "Or skip this and start with the sample song. Songs can be imported at any time with the \u{2193} button in the left column, or dropped on the window.",
    );
    if let Some(said) = &setup.songs_said {
        ui.add_space(10.0);
        ui.label(RichText::new(said).size(12.5).color(READY));
    }
}

/// Which translation goes on top (the main words) and which below (the translation).
fn bibles(ui: &mut egui::Ui, state: &mut HomeState) {
    title(ui, "Your Bibles");
    say(ui, "Bible passages show two translations, one above the other. Choose which goes on top and which below.");
    ui.add_space(12.0);
    let names: Vec<String> = state.bible.shelf.bibles.iter().map(|b| format!("{} ({})", b.name, b.short)).collect();
    if names.is_empty() || state.bible.rows.is_empty() {
        say(ui, "No Bibles are installed yet. Open the Bible page's Options to download one.");
        return;
    }
    let mut changed = false;
    egui::Grid::new("setup bibles").num_columns(2).spacing([14.0, 10.0]).show(ui, |ui| {
        ui.label(RichText::new("On top").color(TEXT));
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
        ui.label(RichText::new("Below").color(TEXT));
        let below = state.bible.rows.get(1).and_then(|row| row.bible);
        let shown = below.and_then(|at| names.get(at).cloned()).unwrap_or_else(|| "None (one translation)".to_owned());
        egui::ComboBox::from_id_salt("setup second bible").width(340.0).selected_text(shown).show_ui(ui, |ui| {
            if ui.selectable_label(below.is_none(), "None (one translation)").clicked() {
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
    say(ui, "More translations can be downloaded from the Bible page's Options.");
}

fn permissions(ui: &mut egui::Ui) {
    title(ui, "What macOS will ask");
    say(ui, "macOS asks before an app does some things. When these come up, choose Allow or OK:");
    ui.add_space(10.0);
    for (what, why) in [
        (
            "Local Network",
            "When a screen is sent over NDI (to a stream or a video switcher). Without it, NDI can't be seen on the network.",
        ),
        (
            "Files in your Documents folder",
            "When SoftPresenter looks for a library of song documents or opens songs, pictures and videos kept there.",
        ),
        (
            "Controlling the app that opens a presentation",
            "The first time you bring a presentation in as pictures, SoftPresenter asks an app on this Mac that can open it to draw its slides. macOS asks once, to allow that.",
        ),
    ] {
        ui.label(RichText::new(what).font(semibold(13.0)).color(TEXT));
        say(ui, why);
        ui.add_space(8.0);
    }
    say(
        ui,
        "SoftPresenter never asks for your password. If you chose Don't Allow by mistake, change it in System Settings › Privacy & Security.",
    );
}

fn done(ui: &mut egui::Ui, show: &Show, state: &mut HomeState, commands: &mut Vec<Command>) {
    title(ui, "You're ready");
    say(
        ui,
        "Click a slide to put it on screen, press the arrow keys or the space bar to go on, and Escape to clear. \
         The Bible page looks passages up as you type, such as \"John 3 16\" or \"Ин 3:16\".",
    );
    ui.add_space(12.0);
    if !show.presentations().is_empty() && button(ui, "Show a Test Slide", false).clicked() {
        commands.push(Command::GoTo(SlideRef { presentation: show.selected(), slide: 0 }));
        state.bible_open = false;
    }
}
