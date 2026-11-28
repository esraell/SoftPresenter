//! The setup a fresh install starts with, so a church gets going on any computer without
//! knowing where things are. A welcome, then five numbered steps: the languages the church
//! shows (they choose which songbooks and Bibles are recommended), its songs (those already on
//! this computer, and free songbooks), its Bibles, the screens, and Ready, which says what is
//! set, what the system will ask permission for (a Mac and Windows ask; on Linux nothing does)
//! and how anonymous reports stand. It is shown once, on the first start, filling the window,
//! and can be opened again from Settings (and the SoftPresenter menu on a Mac), as a sheet over
//! the window. Every choice is the same one Settings, the import menu and the Bible page make,
//! so nothing here is kept apart from them, and a step left with Skip or Back changes nothing.
//! Its words fit the system it runs on (`System`).

mod bibles;
mod languages;
mod ready;
mod screens;
mod songs;
#[cfg(test)]
mod tests;

use egui::{Align2, Color32, FontId, Frame, Rect, Sense, Stroke, pos2, vec2};
use sp_engine::{Command, Show, SlideRef};

use crate::home::HomeState;
use crate::i18n::t;
use crate::studio::{CARD, Flat, HOVER, LINE, READY, SELECTED, TEXT, TEXT_DIM, TEXT_FAINT, semibold};

pub use bibles::{bible_failed_words, place_downloaded};

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

/// The steps: the welcome, which has no number, then the five along the top.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Welcome,
    /// Which languages the church shows: they choose the songbooks and Bibles recommended.
    Languages,
    /// The songs already on this computer (`find_songs`) and free songbooks (`free_songbooks`,
    /// and the song catalog's where it is offered, `songbooks`).
    Songs,
    Bibles,
    Screens,
    /// What is set, what the system will ask, and anonymous reports (`reports::ui`).
    Ready,
}

/// The steps in order, the same on every system.
pub const STEPS: [Step; 6] = [Step::Welcome, Step::Languages, Step::Songs, Step::Bibles, Step::Screens, Step::Ready];

impl Step {
    /// Its name along the top.
    fn name(self) -> &'static str {
        match self {
            Step::Welcome => t("setup.step.welcome", "Welcome"),
            Step::Languages => t("setup.step.languages", "Languages"),
            Step::Songs => t("setup.step.songs", "Songs"),
            Step::Bibles => t("setup.step.bibles", "Bibles"),
            Step::Screens => t("setup.step.screens", "Screens"),
            Step::Ready => t("setup.step.ready", "Ready"),
        }
    }

    /// Its place in [`STEPS`]: the number it has along the top (the welcome's is 0, not shown).
    fn at(self) -> usize {
        STEPS.iter().position(|step| *step == self).unwrap_or(0)
    }
}

/// The languages the Languages step offers, in the order of its cards: those SoftPresenter has
/// free songs to start with in, or speaks.
pub const LANGUAGES: [&str; 4] = ["en", "es", "ru", "uk"];

/// One of Ready's two folds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fold {
    /// What macOS, or Windows, will ask.
    Asks,
    /// What the reports send.
    Reports,
}

/// The setup while it is open.
#[derive(Clone, Debug)]
pub struct Setup {
    pub step: Step,
    /// True when it was opened again from Settings (the setup had been seen): a sheet over the
    /// window, its answers filled in as things stand. Known at its first drawing.
    again: Option<bool>,
    /// The furthest step reached: the steps along the top up to it can be clicked.
    seen: usize,
    /// The steps left with Skip, which changed nothing.
    skipped: Vec<Step>,
    /// The languages the church shows, the one on top first, as chosen on the Languages step.
    /// They are saved, and choose the recommendations outside the setup too, once that step is
    /// passed with Continue.
    pub languages: Vec<String>,
    /// What bringing chosen song files in came to, in plain words.
    pub songs_said: Option<String>,
    /// The song catalog's songbooks ticked on the Songs step.
    pub free_songs: crate::songbooks::Picked,
    /// The free songbooks shared by churches ticked on the Songs step.
    pub shared: crate::free_songbooks::Picked,
    /// The Bibles chosen on the Bibles step, given to the Bible page when it is passed.
    bibles: bibles::Choice,
    /// Which of Ready's folds is open, and whether it was opened by this drawing's click (it is
    /// then brought into view).
    fold: Option<Fold>,
    fold_opened: bool,
    /// How reports stood when last known, for the moments the reporter is busy.
    reports: Option<crate::reports::Standing>,
    /// The welcome's "Coming from SoftProjector?": unanswered on a first start; opened again,
    /// as things stand (Yes while the keys are SoftProjector's). Only an answer that differs
    /// from how things stood changes the key layout and the end of a song's mark, and only when
    /// the setup is finished on Ready.
    from_softprojector: Option<bool>,
    /// Whether the keys were SoftProjector's when the setup opened.
    softprojector_was: bool,
    /// The languages the Languages step opened with: opened again, passing it with them
    /// unchanged keeps nothing new and leaves the Bibles chosen alone.
    languages_opened: Vec<String>,
}

impl Setup {
    pub fn new() -> Setup {
        Setup {
            step: Step::Welcome,
            again: None,
            seen: 0,
            skipped: Vec::new(),
            languages: Vec::new(),
            songs_said: None,
            free_songs: crate::songbooks::Picked::default(),
            shared: crate::free_songbooks::Picked::default(),
            bibles: bibles::Choice::default(),
            fold: None,
            fold_opened: false,
            reports: None,
            from_softprojector: None,
            softprojector_was: false,
            languages_opened: Vec::new(),
        }
    }

    /// The setup opened at the step a testing aid names (`languages`, `songs`, `bibles`,
    /// `screens`, `ready`, or its number), or at its start for any other value. `ready-asks` and
    /// `ready-reports` open Ready with that fold open. The names of the steps there were before
    /// still open where their part went: `free-songs` Songs, `permissions` Ready with what the
    /// system will ask opened, `reports` and `done` Ready, `reports-open` Ready with what the
    /// reports send opened.
    pub fn opened_at(name: &str) -> Setup {
        let wanted = name.trim().to_ascii_lowercase().replace(['-', '_', ' '], "");
        let mut setup = Setup::new();
        let (step, fold) = match wanted.as_str() {
            "languages" | "1" => (Step::Languages, None),
            "songs" | "freesongs" | "2" => (Step::Songs, None),
            "bibles" | "3" => (Step::Bibles, None),
            "screens" | "4" => (Step::Screens, None),
            "ready" | "done" | "reports" | "5" => (Step::Ready, None),
            "readyasks" | "permissions" => (Step::Ready, Some(Fold::Asks)),
            "readyreports" | "reportsopen" => (Step::Ready, Some(Fold::Reports)),
            _ => (Step::Welcome, None),
        };
        setup.step = step;
        setup.seen = step.at();
        setup.fold = fold;
        setup
    }

    /// True when the setup was opened again from Settings.
    fn again(&self) -> bool {
        self.again.unwrap_or(false)
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
    /// Ask which song files to import.
    ChooseSongFiles,
}

/// What the setup is told of the computer and the build, so each can be tried on any computer:
/// the system, how anonymous reports stand (`None` in a build without them), whether the song
/// catalog is offered, and whether Bibles can be downloaded.
#[derive(Clone, Debug)]
pub struct Env {
    pub system: System,
    pub reports: Option<crate::reports::Standing>,
    pub catalog: bool,
    pub bibles_download: bool,
    /// The version, as the build shows it: a pre-release says so on the welcome.
    pub version: String,
    /// The computer's preferred languages, as the system writes them, its own first: those the
    /// Languages step offers start chosen.
    pub computer_languages: Vec<String>,
}

impl Env {
    /// As things are now. `last`: how reports stood when last known, for a moment the
    /// reporter's own thread holds them.
    fn now(last: Option<&crate::reports::Standing>) -> Env {
        Env {
            system: System::THIS,
            reports: crate::reports::offered()
                .then(|| crate::reports::standing().or_else(|| last.cloned()).unwrap_or_default()),
            catalog: crate::catalog::offered(),
            bibles_download: crate::bible_store::download_base().is_some(),
            version: crate::updates::VERSION.to_owned(),
            computer_languages: crate::i18n::computer_tags(),
        }
    }
}

/// How large the setup's card is in a window of `screen`: on a first start, most of the window
/// (940 by 720 at the most); opened again, the narrower sheet (780 by 660 at the most). The
/// same on every step, so the keys at its foot never move.
fn card_size(screen: egui::Vec2, again: bool) -> egui::Vec2 {
    if again {
        vec2((screen.x - 32.0).min(780.0), (screen.y - 32.0).min(660.0))
    } else {
        vec2((screen.x - 48.0).min(940.0), (screen.y - 48.0).min(720.0))
    }
}

/// The room inside the card's edge: at its sides, above and below.
fn card_margin(again: bool) -> egui::Margin {
    if again {
        egui::Margin { left: 28, right: 28, top: 20, bottom: 20 }
    } else {
        egui::Margin { left: 40, right: 40, top: 24, bottom: 22 }
    }
}

/// How tall the steps along the top are, and the foot's keys.
const BAR: f32 = 30.0;
const FOOT: f32 = 30.0;

/// Where the last drawing put a step's content, for the checks that every step fits: how tall
/// what the step drew is, and how tall the room it has.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Fit {
    pub content: f32,
    pub room: f32,
}

fn fit_id() -> egui::Id {
    egui::Id::unique("setup fit")
}

/// Draws the setup over the window while it is open. Closing it, at the end or early, marks
/// the setup done, so it isn't shown at the next start.
pub fn setup_ui(ctx: &egui::Context, show: &Show, state: &mut HomeState, commands: &mut Vec<Command>) {
    let env = Env::now(state.setup.as_ref().and_then(|setup| setup.reports.as_ref()));
    setup_ui_in(ctx, show, state, commands, &env);
}

/// [`setup_ui`], told of the computer and the build (`env`).
pub fn setup_ui_in(ctx: &egui::Context, show: &Show, state: &mut HomeState, commands: &mut Vec<Command>, env: &Env) {
    let Some(mut setup) = state.setup.take() else { return };
    // The first drawing: whether it is a first start, and the answers as things stand.
    if setup.again.is_none() {
        setup.again = Some(state.setup_done);
        // Opened again by someone never asked their languages: those of the Bibles they show.
        let standing = if state.setup_done { bibles::languages_standing(state) } else { Vec::new() };
        setup.languages = languages::starting(&state.church_languages, &standing, &env.computer_languages);
        setup.languages_opened.clone_from(&setup.languages);
        if state.setup_done {
            // Their songbooks are theirs to choose: the recommended ones are marked, not ticked.
            setup.free_songs.leave_unticked();
            setup.shared.leave_unticked();
        }
        setup.bibles = bibles::Choice::starting(state, &setup.languages, setup.again(), env.bibles_download);
        setup.softprojector_was = state.keys.style == crate::key_table::KeyStyle::SoftProjector;
        setup.from_softprojector = setup.again().then_some(setup.softprojector_was);
    }
    if env.reports.is_some() {
        setup.reports.clone_from(&env.reports);
    }
    let again = setup.again();
    let mut open = true;
    // Escape closes a menu that is open (On top, Below, the app's language) and nothing more.
    let menu_open = egui::Popup::is_any_open(ctx);
    let screen = ctx.content_rect().size();
    let size = card_size(screen, again);
    let margin = card_margin(again);
    let frame = if again {
        Frame::new()
            .fill(Color32::from_rgb(40, 40, 42))
            .stroke(Stroke::new(1.0, Color32::from_white_alpha(22)))
            .corner_radius(16.0)
            .inner_margin(margin)
            .shadow(egui::Shadow { offset: [0, 18], blur: 50, spread: 0, color: Color32::from_black_alpha(160) })
    } else {
        Frame::new()
            .fill(Color32::from_rgb(18, 18, 18))
            .stroke(Stroke::new(1.0, Color32::from_rgb(42, 42, 42)))
            .corner_radius(16.0)
            .inner_margin(margin)
            .shadow(egui::Shadow { offset: [0, 30], blur: 80, spread: 0, color: Color32::from_black_alpha(180) })
    };
    // A first start fills the window in the brand's dark, so nothing behind it is read first;
    // opened again, the window stays in view behind the sheet.
    let backdrop = if again { Color32::from_black_alpha(110) } else { Color32::from_rgb(5, 5, 5) };
    let area = egui::Modal::default_area(egui::Id::unique("setup")).anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0));
    egui::Modal::new(egui::Id::unique("setup")).area(area).backdrop_color(backdrop).frame(frame).show(ctx, |ui| {
        let inner = vec2(
            size.x - f32::from(margin.left) - f32::from(margin.right),
            size.y - f32::from(margin.top) - f32::from(margin.bottom),
        );
        ui.set_width(inner.x);
        ui.set_height(inner.y);
        ui.spacing_mut().item_spacing = vec2(8.0, 0.0);
        let top = ui.cursor().min;
        // The steps along the top, numbered from Languages; the welcome has none.
        let mut go_to = None;
        if setup.step != Step::Welcome {
            let bar = Rect::from_min_size(top, vec2(inner.x, BAR));
            go_to = steps_bar(ui, bar, &setup);
        }
        let body =
            Rect::from_min_max(pos2(top.x, top.y + BAR + 10.0), pos2(top.x + inner.x, top.y + inner.y - FOOT - 14.0));
        let step = setup.step;
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(body).id_salt(("setup step", step.at())));
        child.set_clip_rect(body.expand(2.0));
        let out = egui::ScrollArea::vertical()
            .id_salt(("setup body", step.at()))
            .max_height(body.height())
            .auto_shrink([false, false])
            .show(&mut child, |ui| {
                ui.set_width(body.width());
                match step {
                    Step::Welcome => welcome(ui, &mut setup, env),
                    Step::Languages => languages::step(ui, &mut setup, again),
                    Step::Songs => songs::step(ui, &mut setup, state, env, body.height()),
                    Step::Bibles => bibles::step(ui, &mut setup, state, env),
                    Step::Screens => screens::step(ui, state),
                    Step::Ready => go_to = go_to.or(ready::step(ui, &mut setup, state, env)),
                }
            });
        ui.data_mut(|data| {
            data.insert_temp(fit_id(), Fit { content: out.content_size.y, room: out.inner_rect.height() })
        });
        let foot = Rect::from_min_size(pos2(top.x, top.y + inner.y - FOOT), vec2(inner.x, FOOT));
        match foot_keys(ui, foot, &setup, state, show) {
            Some(Pressed::Leave) => open = false,
            Some(Pressed::Back) => go(&mut setup, state, step.at().saturating_sub(1), env, ui.ctx()),
            Some(Pressed::Skip) => {
                if !setup.skipped.contains(&step) {
                    setup.skipped.push(step);
                }
                let next = step.at() + 1;
                setup.step = STEPS[next.min(STEPS.len() - 1)];
                setup.seen = setup.seen.max(setup.step.at());
            }
            Some(Pressed::TestSlide) => {
                commands.push(Command::GoTo(SlideRef { presentation: show.selected(), slide: 0 }));
                state.bible_open = false;
            }
            // The way on, where the step can be left (a language is chosen).
            Some(Pressed::On) if passed(&mut setup, state, env, ui.ctx()) => {
                if step == Step::Ready {
                    // Ready was read, with its Reports line: the person has been told.
                    if env.reports.is_some() {
                        crate::reports::ui::passed_setup_step();
                    }
                    // A changed answer to "Coming from SoftProjector?" is taken now.
                    if let Some(yes) = setup.from_softprojector.filter(|yes| *yes != setup.softprojector_was) {
                        answered_from_softprojector(state, yes);
                    }
                    open = false;
                } else {
                    setup.step = STEPS[step.at() + 1];
                    setup.seen = setup.seen.max(setup.step.at());
                }
            }
            Some(Pressed::On) | None => {}
        }
        if let Some(to) = go_to {
            go(&mut setup, state, to, env, ui.ctx());
        }
    });
    if !menu_open && ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
        open = false;
    }
    if open {
        state.setup = Some(setup);
    } else {
        // A first start left early says where the setup lives.
        if !again && setup.step != Step::Ready {
            state.notices.info(t(
                "setup.skipped.notice",
                "The setup is in Settings whenever you want it: Set up SoftPresenter\u{2026}",
            ));
        }
        state.setup_done = true;
        state.identify = false;
    }
}

/// How well the step last drawn fits its room (see [`Fit`]).
#[cfg(test)]
pub fn fit(ctx: &egui::Context) -> Fit {
    ctx.data(|data| data.get_temp::<Fit>(fit_id()).unwrap_or_default())
}

/// Goes to another step from the steps along the top, a Change key or Back. Going on to a
/// later step this way takes this step's choices, as its own key does (the languages kept,
/// the songbooks and songs ticked brought in, the Bibles given to the Bible page), so nothing
/// chosen is dropped unsaid; Languages needs one language chosen. Going back takes nothing.
fn go(setup: &mut Setup, state: &mut HomeState, to: usize, env: &Env, ctx: &egui::Context) {
    let to = to.min(STEPS.len() - 1);
    if to > setup.step.at() && !passed(setup, state, env, ctx) {
        return;
    }
    setup.step = STEPS[to];
    setup.seen = setup.seen.max(to);
    setup.fold_opened = false;
}

/// Continue on a step: what it chose is done. False when the step can't be left yet (no
/// language chosen).
fn passed(setup: &mut Setup, state: &mut HomeState, env: &Env, ctx: &egui::Context) -> bool {
    let step = setup.step;
    match step {
        Step::Welcome | Step::Screens | Step::Ready => {}
        Step::Languages => {
            if setup.languages.is_empty() {
                return false;
            }
            languages::passed(setup, state);
        }
        Step::Songs => songs::passed(setup, state),
        Step::Bibles => bibles::passed(setup, state, env, ctx),
    }
    setup.skipped.retain(|skipped| *skipped != step);
    true
}

/// What a click at the foot asked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pressed {
    /// Skip Setup, or Close: the setup closes.
    Leave,
    Back,
    Skip,
    TestSlide,
    /// The way on: Get Started, Continue, Bring In and Continue, Start Using SoftPresenter.
    On,
}

fn skip_setup_words() -> &'static str {
    t("setup.skip", "Skip Setup")
}

/// The words of the way on, on each step: the Songs step's say that songs come in when some
/// found on this computer are ticked.
fn on_words(step: Step, again: bool, bringing: bool) -> &'static str {
    match step {
        Step::Welcome if again => t("setup.continue", "Continue"),
        Step::Welcome => t("setup.get_started", "Get Started"),
        Step::Songs if bringing => t("setup.songs.bring_in", "Bring In and Continue"),
        Step::Ready => t("setup.start_using", "Start Using SoftPresenter"),
        _ => t("setup.continue", "Continue"),
    }
}

/// Every wording the way on can have, so its key is as wide as the longest of them on every
/// step (190 points at least), and the side keys' (88 at least).
fn on_wordings() -> [&'static str; 4] {
    [
        t("setup.get_started", "Get Started"),
        t("setup.continue", "Continue"),
        t("setup.songs.bring_in", "Bring In and Continue"),
        t("setup.start_using", "Start Using SoftPresenter"),
    ]
}

fn side_wordings() -> [&'static str; 4] {
    [skip_setup_words(), t("setup.close", "Close"), t("setup.back", "Back"), t("setup.skip_step", "Skip")]
}

/// The widest of `wordings` in `font`, with `pad` each side, `least` at least.
fn key_width(ui: &egui::Ui, wordings: &[&str], font: &FontId, pad: f32, least: f32) -> f32 {
    crate::studio::flat_button_width(ui, wordings, font, pad).max(least).ceil()
}

/// The keys at the foot, the same sizes and places on every step: Back (Skip Setup or Close on
/// the welcome), Skip (Show a Test Slide on Ready), and the way on at the right; what is
/// downloading at the left.
fn foot_keys(ui: &mut egui::Ui, foot: Rect, setup: &Setup, state: &HomeState, show: &Show) -> Option<Pressed> {
    let step = setup.step;
    let plain = FontId::proportional(13.0);
    let on_width = key_width(ui, &on_wordings(), &semibold(13.0), 16.0, 190.0);
    let side_width = key_width(ui, &side_wordings(), &plain, 13.0, 88.0);
    let mut pressed = None;
    let on = Rect::from_min_max(pos2(foot.max.x - on_width, foot.min.y), foot.max);
    let can_go_on = step != Step::Languages || !setup.languages.is_empty();
    let words = on_words(step, setup.again(), songs::bringing(state));
    if crate::studio::flat_button(ui, on, ui.scope_id().with("setup on"), words, semibold(13.0), Flat::Main, can_go_on)
        .clicked()
    {
        pressed = Some(Pressed::On);
    }
    // The key beside it: Skip on the middle steps, Show a Test Slide on Ready, none on the welcome.
    let mut left = on.min.x - 8.0;
    match step {
        Step::Welcome => {}
        Step::Ready => {
            let words = t("setup.done.test_slide", "Show a Test Slide");
            let width = key_width(ui, &[words], &plain, 13.0, 88.0);
            let rect = Rect::from_min_max(pos2(left - width, foot.min.y), pos2(left, foot.max.y));
            let can = !show.presentations().is_empty();
            if crate::studio::flat_button(
                ui,
                rect,
                ui.scope_id().with("setup test"),
                words,
                plain.clone(),
                Flat::Plain,
                can,
            )
            .clicked()
            {
                pressed = Some(Pressed::TestSlide);
            }
            left = rect.min.x - 8.0;
        }
        _ => {
            let rect = Rect::from_min_max(pos2(left - side_width, foot.min.y), pos2(left, foot.max.y));
            let words = t("setup.skip_step", "Skip");
            if crate::studio::flat_button(
                ui,
                rect,
                ui.scope_id().with("setup skip"),
                words,
                plain.clone(),
                Flat::Plain,
                true,
            )
            .clicked()
            {
                pressed = Some(Pressed::Skip);
            }
            left = rect.min.x - 8.0;
        }
    }
    // Back, in one place on every step: beside Skip's place, whether or not Skip is there.
    let back_right = on.min.x - 8.0 - side_width - 8.0;
    let back_right = if step == Step::Ready { left } else { back_right };
    let rect = Rect::from_min_max(pos2(back_right - side_width, foot.min.y), pos2(back_right, foot.max.y));
    let (words, what) = match step {
        Step::Welcome if setup.again() => (t("setup.close", "Close"), Pressed::Leave),
        Step::Welcome => (skip_setup_words(), Pressed::Leave),
        _ => (t("setup.back", "Back"), Pressed::Back),
    };
    if crate::studio::flat_button(ui, rect, ui.scope_id().with("setup back"), words, plain, Flat::Plain, true).clicked()
    {
        pressed = Some(what);
    }
    // What is downloading, at the left, on every step.
    if let Some(words) = songs::downloading_words(state) {
        let room = rect.min.x - foot.min.x - 12.0;
        let galley = crate::home::settings::parts::one_line(ui, &words, FontId::proportional(12.0), TEXT_DIM, room);
        ui.painter().galley(pos2(foot.min.x, foot.center().y - galley.size().y / 2.0), galley, TEXT_DIM);
    }
    pressed
}

/// The steps along the top, numbered from Languages: the one now in ink on the well, those done
/// with a tick, those skipped with a dash. A step already reached can be clicked; the step
/// clicked, if any.
fn steps_bar(ui: &mut egui::Ui, bar: Rect, setup: &Setup) -> Option<usize> {
    let mut clicked = None;
    let mut x = bar.min.x;
    let now = setup.step.at();
    for step in STEPS.iter().skip(1) {
        let at = step.at();
        let font = FontId::proportional(12.0);
        let name = ui.painter().layout_no_wrap(step.name().to_owned(), font, TEXT);
        let width = 5.0 + 20.0 + 7.0 + name.size().x + 10.0;
        let rect = Rect::from_min_size(pos2(x, bar.min.y), vec2(width, BAR));
        let reached = at <= setup.seen;
        let sense = if reached { Sense::click() } else { Sense::hover() };
        let response = ui.interact(rect, ui.scope_id().with(("setup step key", at)), sense);
        if response.clicked() && at != now {
            clicked = Some(at);
        }
        let skipped = setup.skipped.contains(step) && at != now;
        let done = at < now && !skipped;
        let ink = if at == now {
            TEXT
        } else if at < now {
            TEXT_DIM
        } else {
            TEXT_FAINT
        };
        if at == now {
            ui.painter().rect_filled(rect, BAR / 2.0, CARD);
        } else if reached && response.hovered() {
            ui.painter().rect_filled(rect, BAR / 2.0, HOVER);
        }
        let centre = pos2(rect.min.x + 15.0, rect.center().y);
        if at == now {
            ui.painter().circle_filled(centre, 10.0, TEXT);
        } else if done || skipped {
            ui.painter().circle_filled(centre, 10.0, SELECTED);
        } else {
            ui.painter().circle_stroke(centre, 9.5, Stroke::new(1.0, LINE));
        }
        let mark_ink = if at == now { crate::studio::PANEL } else { ink };
        if done {
            tick(ui.painter(), centre, 4.0, Stroke::new(1.6, TEXT));
        } else {
            let mark = if skipped { "\u{2013}".to_owned() } else { at.to_string() };
            ui.painter().text(centre, Align2::CENTER_CENTER, mark, semibold(10.5), mark_ink);
        }
        ui.painter().galley_with_override_text_color(
            pos2(rect.min.x + 32.0, rect.center().y - name.size().y / 2.0),
            name,
            ink,
        );
        x += width + 6.0;
    }
    clicked
}

/// A tick drawn around `centre`, `size` from its middle to its ends.
pub(crate) fn tick(painter: &egui::Painter, centre: egui::Pos2, size: f32, stroke: Stroke) {
    let points =
        vec![centre + vec2(-size, 0.0), centre + vec2(-size * 0.3, size * 0.7), centre + vec2(size, -size * 0.7)];
    painter.add(egui::Shape::line(points, stroke));
}

/// A step's title, and the line under it.
pub(crate) fn head(ui: &mut egui::Ui, title: &str, line: &str) {
    words(ui, title, semibold(24.0), TEXT, 30.0, 1);
    ui.add_space(4.0);
    if !line.is_empty() {
        words(ui, line, FontId::proportional(13.5), TEXT_DIM, 19.0, 1);
    }
    ui.add_space(14.0);
}

/// Words wrapped at the room there is, every line `line` tall, keeping the room of `least`
/// lines whatever they come to, so what follows never moves as they change. Where they went.
pub(crate) fn words(ui: &mut egui::Ui, text: &str, font: FontId, ink: Color32, line: f32, least: usize) -> Rect {
    let width = ui.available_width();
    let galley = crate::home::settings::parts::lines(ui, text, font, ink, width, line);
    let height = galley.size().y.max(least as f32 * line);
    let (place, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    ui.painter().galley(place.min, galley, ink);
    place
}

/// Small faint words, as the design's captions and notes are, keeping `least` lines' room.
pub(crate) fn small(ui: &mut egui::Ui, text: &str, least: usize) -> Rect {
    words(ui, text, FontId::proportional(12.0), TEXT_FAINT, 17.0, least)
}

/// A caption in small capitals, as the cards' are.
pub(crate) fn cap(ui: &mut egui::Ui, text: &str) {
    words(ui, &text.to_uppercase(), semibold(10.5), TEXT_FAINT, 16.0, 1);
}

/// A plain key as wide as its words (`least` at least), 30 high, at the cursor.
pub(crate) fn key(ui: &mut egui::Ui, salt: &str, label: &str, least: f32, enabled: bool) -> egui::Response {
    let font = FontId::proportional(12.5);
    let width = key_width(ui, &[label], &font, 13.0, least);
    let (place, _) = ui.allocate_exact_size(vec2(width, 30.0), Sense::hover());
    crate::studio::flat_button(ui, place, ui.scope_id().with(("setup key", salt)), label, font, Flat::Plain, enabled)
}

/// Faint words that take a click, for a way out to a page.
pub(crate) fn link(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add(egui::Label::new(egui::RichText::new(label).size(12.5).color(TEXT_FAINT)).sense(Sense::click()))
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// "Recommended", as the Songs and Bibles steps mark what the church's languages choose.
pub(crate) fn recommended_words() -> &'static str {
    t("setup.recommended", "Recommended")
}

/// What a songbook's row says under its name, with Recommended after it.
pub(crate) fn recommended_detail(detail: &str) -> String {
    // A songbook whose size isn't known yet says nothing before it.
    if detail.is_empty() {
        return recommended_words().to_owned();
    }
    format!("{detail} \u{b7} {}", recommended_words())
}

/// A language written in itself (Русский), for the cards, the menus and Ready's lines.
pub(crate) fn language_own_name(code: &str) -> String {
    match crate::catalog::songbook(code) {
        Some(book) => book.name().to_owned(),
        None => crate::bible::language_name(code),
    }
}

/// A slide-shaped picture, 16 by 9, as wide as `width`: black, with lines of words in its
/// middle, each (its words, its size at a slide 1920 wide, its ink). The window's own painter
/// draws it, so it shows the order and the languages, not the theme's look.
pub(crate) fn slide_picture(ui: &mut egui::Ui, width: f32, lines: &[(String, f32, Color32)]) -> Rect {
    let (place, _) = ui.allocate_exact_size(vec2(width, (width * 9.0 / 16.0).round()), Sense::hover());
    let painter = ui.painter().with_clip_rect(place);
    painter.rect_filled(place, 6.0, Color32::BLACK);
    painter.rect_stroke(place, 6.0, Stroke::new(1.0, LINE), egui::StrokeKind::Inside);
    let scale = width / 1920.0;
    let galleys: Vec<_> = lines
        .iter()
        .map(|(text, size, ink)| {
            let mut job = egui::text::LayoutJob::simple(
                text.clone(),
                FontId::proportional((size * scale).max(7.0)),
                *ink,
                width - 28.0,
            );
            job.halign = egui::Align::Center;
            ui.ctx().fonts_mut(|fonts| fonts.layout_job(job))
        })
        .collect();
    let gap = 6.0;
    let total: f32 =
        galleys.iter().map(|galley| galley.size().y).sum::<f32>() + gap * galleys.len().saturating_sub(1) as f32;
    let mut y = place.center().y - total / 2.0;
    for galley in galleys {
        let height = galley.size().y;
        painter.galley(pos2(place.center().x, y), galley, TEXT);
        y += height + gap;
    }
    place
}

/// What the welcome says under its title: on a first start, what the minute is for; opened
/// again, that the answers stand.
fn welcome_words(again: bool) -> (&'static str, &'static str) {
    if again {
        (
            t("setup.again.title", "Set up SoftPresenter"),
            t("setup.again.about", "Your answers are filled in. Change what you like; nothing else changes."),
        )
    } else {
        (
            t("setup.welcome.title", "Welcome to SoftPresenter"),
            t(
                "setup.welcome.lead",
                "A minute to get ready for Sunday: your languages, your songs, your Bibles and the screens. Everything can be changed later, in Settings.",
            ),
        )
    }
}

/// What the welcome says in a pre-release (`version`, as the build shows it), under the
/// language: that it is one, what may change and what won't, and where to hear of the next.
/// `None` for a release.
fn pre_release_note(version: &str) -> Option<(String, String)> {
    let (heading, about) = crate::updates::pre_release_words(version)?;
    let updates =
        t("setup.prerelease.updates", "Check for updates (Settings \u{203a} About) tells you when one is out.");
    Some((heading, format!("{about} {updates}")))
}

/// What the keys and the end of a song's mark will be once the setup is finished: as the answer
/// to "Coming from SoftProjector?" has them if it differs from how things stood, else as they are.
pub(crate) fn keys_will_be(setup: &Setup, state: &HomeState) -> (crate::key_table::KeyStyle, bool) {
    use crate::key_table::KeyStyle;
    match setup.from_softprojector {
        Some(yes) if yes != setup.softprojector_was => {
            (if yes { KeyStyle::SoftProjector } else { KeyStyle::SoftPresenter }, yes)
        }
        _ => (state.keys.style, state.end_mark),
    }
}

/// The setup was finished with a changed answer to "Coming from SoftProjector?": Yes gives
/// SoftProjector's key layout and the end of a song's mark, No the standard layout and no mark.
/// The one place that answer is acted on: what else follows from it (how a click on a slide
/// shows it) is set here too.
pub(crate) fn answered_from_softprojector(state: &mut HomeState, yes: bool) {
    use crate::key_table::KeyStyle;
    state.keys.set_style(if yes { KeyStyle::SoftProjector } else { KeyStyle::SoftPresenter });
    state.end_mark = yes;
    // CLICK MODE: the slide click mode (Double-click for Yes) is set here once that setting is in.
}

/// What Yes to "Coming from SoftProjector?" keeps, said under it: a Mac's F keys are its own, so
/// there it speaks of Esc alone.
fn from_softprojector_words(system: System) -> &'static str {
    match system {
        System::Mac => t(
            "setup.from.line.mac",
            "Yes keeps SoftProjector\u{2019}s Esc, which hides the words and leaves the background up, and its *\u{a0}*\u{a0}* under a song\u{2019}s last slide. Both can be changed in Settings \u{203a} Keys and Settings \u{203a} Slides.",
        ),
        System::Windows | System::Linux => t(
            "setup.from.line",
            "Yes keeps SoftProjector\u{2019}s keys (Esc hides the words, F5 shows, F2 adds to the playlist) and its *\u{a0}*\u{a0}* under a song\u{2019}s last slide. Both can be changed in Settings \u{203a} Keys and Settings \u{203a} Slides.",
        ),
    }
}

/// The welcome: the mark, the name and one line, in the middle; the app's own language first of
/// all, so the rest of the setup is read in it; "Coming from SoftProjector?"; and, in a
/// pre-release, the card that says so.
fn welcome(ui: &mut egui::Ui, setup: &mut Setup, env: &Env) {
    let again = setup.again();
    let (title, lead) = welcome_words(again);
    let note = pre_release_note(&env.version);
    let width = ui.available_width().min(560.0);
    let room = ui.available_height();
    // Laid out once to know its height, so it sits in the middle of the step.
    let lead_galley = {
        let mut job = egui::text::LayoutJob::default();
        let format = egui::TextFormat {
            font_id: FontId::proportional(15.5),
            color: TEXT_DIM,
            line_height: Some(23.0),
            ..Default::default()
        };
        job.append(lead, 0.0, format);
        job.wrap.max_width = width.min(440.0);
        job.halign = egui::Align::Center;
        ui.ctx().fonts_mut(|fonts| fonts.layout_job(job))
    };
    let note_height = note.as_ref().map_or(0.0, |(_, words)| {
        let galley =
            crate::home::settings::parts::lines(ui, words, FontId::proportional(12.5), TEXT_DIM, width - 28.0, 18.0);
        12.0 + 22.0 + 4.0 + galley.size().y + 18.0
    });
    let height =
        54.0 + 12.0 + 42.0 + 10.0 + lead_galley.size().y + 14.0 + 30.0 + 10.0 + 30.0 + 6.0 + 34.0 + note_height;
    ui.add_space(((room - height) / 2.0).max(0.0));
    ui.vertical_centered(|ui| {
        ui.set_width(ui.available_width());
        // The mark: the brand's light key with its one orange light.
        let (mark, _) = ui.allocate_exact_size(vec2(84.0, 54.0), Sense::hover());
        ui.painter().rect_filled(mark, 14.0, TEXT);
        ui.painter().circle_filled(pos2(mark.max.x - 21.5, mark.min.y + 19.5), 8.5, crate::studio::TALLY);
        ui.add_space(12.0);
        ui.label(egui::RichText::new(title).font(semibold(34.0)).color(Color32::WHITE));
        ui.add_space(10.0);
        let (place, _) = ui.allocate_exact_size(lead_galley.size(), Sense::hover());
        // Its lines are centred on where it is drawn from.
        ui.painter().galley(pos2(place.center().x, place.min.y), lead_galley, TEXT_DIM);
        ui.add_space(14.0);
        ui.allocate_ui(vec2(ui.available_width(), 30.0), |ui| {
            ui.horizontal(|ui| {
                let label = t("setup.welcome.language", "Language of SoftPresenter");
                let label_width =
                    ui.painter().layout_no_wrap(label.to_owned(), FontId::proportional(13.0), TEXT_DIM).size().x;
                // In the middle: the label, a gap, and the menu, about as wide as it gets.
                ui.add_space(((ui.available_width() - label_width - 12.0 - 240.0) / 2.0).max(0.0));
                ui.label(egui::RichText::new(label).size(13.0).color(TEXT_DIM));
                ui.add_space(4.0);
                crate::home::language_menu(ui);
            });
        });
        // Coming from SoftProjector? Yes or No, and under them what Yes keeps (two lines' room).
        ui.add_space(10.0);
        ui.allocate_ui(vec2(ui.available_width(), 30.0), |ui| {
            ui.horizontal(|ui| {
                let label = t("setup.from.question", "Coming from SoftProjector?");
                let font = FontId::proportional(13.0);
                let label_width = ui.painter().layout_no_wrap(label.to_owned(), font.clone(), TEXT_DIM).size().x;
                let answers = [(true, t("setup.from.yes", "Yes")), (false, t("setup.from.no", "No"))];
                let key = key_width(ui, &[answers[0].1, answers[1].1], &font, 14.0, 78.0);
                ui.add_space(((ui.available_width() - label_width - 12.0 - key * 2.0 - 2.0) / 2.0).max(0.0));
                ui.label(egui::RichText::new(label).size(13.0).color(TEXT_DIM));
                ui.add_space(4.0);
                ui.spacing_mut().item_spacing.x = 2.0;
                for (yes, words) in answers {
                    let (place, _) = ui.allocate_exact_size(vec2(key, 28.0), Sense::hover());
                    let look = if setup.from_softprojector == Some(yes) { Flat::On } else { Flat::Plain };
                    let id = ui.scope_id().with(("setup from softprojector", yes));
                    if crate::studio::flat_button(ui, place, id, words, font.clone(), look, true).clicked() {
                        setup.from_softprojector = Some(yes);
                    }
                }
            });
        });
        ui.add_space(6.0);
        ui.allocate_ui_with_layout(vec2(width, 34.0), egui::Layout::top_down(egui::Align::Center), |ui| {
            ui.set_width(width);
            let galley = {
                let mut job = egui::text::LayoutJob::default();
                let format = egui::TextFormat {
                    font_id: FontId::proportional(12.0),
                    color: TEXT_FAINT,
                    line_height: Some(17.0),
                    ..Default::default()
                };
                job.append(from_softprojector_words(env.system), 0.0, format);
                job.wrap.max_width = width;
                job.halign = egui::Align::Center;
                ui.ctx().fonts_mut(|fonts| fonts.layout_job(job))
            };
            let (place, _) = ui.allocate_exact_size(vec2(width, galley.size().y.max(34.0)), Sense::hover());
            ui.painter().galley(pos2(place.center().x, place.min.y), galley, TEXT_FAINT);
        });
        // A pre-release says so, calmly, in a small card at the foot (the owner, 2026-11-25).
        if let Some((heading, words)) = note {
            ui.add_space(12.0);
            ui.allocate_ui_with_layout(vec2(width, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                Frame::new().fill(CARD).corner_radius(9.0).inner_margin(egui::Margin::symmetric(14, 11)).show(
                    ui,
                    |ui| {
                        ui.set_width(width - 28.0);
                        ui.label(egui::RichText::new(heading).font(semibold(13.0)).color(TEXT));
                        ui.add_space(4.0);
                        self::words(ui, &words, FontId::proportional(12.5), TEXT_DIM, 18.0, 1);
                    },
                );
            });
        }
    });
}

/// A light in `colour` at the left of a row, in a box 18 square.
pub(crate) fn light(ui: &egui::Ui, centre: egui::Pos2, colour: Color32) {
    crate::studio::paint_light(ui.painter(), centre, Some(colour));
}

/// The green of what is in order, for the marks the steps share.
pub(crate) const OK: Color32 = READY;
