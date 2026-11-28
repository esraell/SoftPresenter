//! The setup's checks: each step's flow, the languages choosing what is recommended, reports
//! starting off in the EU, the EEA and the UK, every step fitting a window of 1100 by 700 with
//! nothing to scroll (as invariants of the layout, never one system's text widths), and an
//! existing user's settings left alone.

use egui::Rect;
use sp_engine::Show;

use super::screens::{
    OneDisplayChoice, audience_displays, one_display_choice, one_display_words, several_displays_words,
};
use super::*;
use crate::displays::DisplayInfo;
use crate::i18n::{Language, with_language};
use crate::ndi_download::testing::{click, drawn};
use crate::reports::Standing;
use crate::screens::Output;

const SYSTEMS: [System; 3] = [System::Mac, System::Windows, System::Linux];
const EVERY_LANGUAGE: [Language; 4] = [Language::English, Language::Spanish, Language::Russian, Language::Ukrainian];

/// How reports stand where they start on (`on`), or off (the EU, the EEA, the UK).
fn standing(on: bool) -> Standing {
    Standing { enabled: on, starts_on: on, may_send: false, ..Standing::default() }
}

/// A computer of `system`, in a pre-release, with reports as they start outside Europe, no
/// song catalog, and Bibles to download.
fn env(system: System) -> Env {
    Env {
        system,
        reports: Some(standing(true)),
        catalog: false,
        bibles_download: true,
        version: "0.9.0-dev.30".to_owned(),
    }
}

/// A window's state with three displays and the setup open, as a first start has it.
fn home(name: &str) -> HomeState {
    let display = |index: usize, name: &str, main: bool| DisplayInfo {
        index,
        name: name.into(),
        x: 1920 * index as i32,
        w: 1920,
        h: 1080,
        main,
        ..Default::default()
    };
    let data = std::env::temp_dir().join(format!("softpresenter-setup-five-{name}-{}", std::process::id()));
    HomeState {
        ndi_download: crate::ndi_download::NdiDownload::in_folder(data),
        displays: vec![display(0, "Built-in", true), display(1, "Projector", false), display(2, "Stage TV", false)],
        ndi_available: false,
        setup: Some(Setup::new()),
        ..HomeState::default()
    }
}

/// Draws the setup twice in a window of 1100 by 700 (the second drawing has the sizes the
/// first worked out) and gives every text drawn, and where.
fn draw(ctx: &egui::Context, state: &mut HomeState, env: &Env) -> Vec<(String, Rect)> {
    let show = Show::new(Vec::new());
    let mut body = |ui: &mut egui::Ui| {
        let mut commands = Vec::new();
        setup_ui_in(ui.ctx(), &show, state, &mut commands, env);
    };
    drawn(ctx, Vec::new(), &mut body);
    drawn(ctx, Vec::new(), &mut body)
}

/// Clicks the key or the words that say exactly `words`.
fn press(ctx: &egui::Context, state: &mut HomeState, env: &Env, words: &str) -> bool {
    let show = Show::new(Vec::new());
    click(ctx, words, &mut |ui| {
        let mut commands = Vec::new();
        setup_ui_in(ui.ctx(), &show, state, &mut commands, env);
    })
}

fn said(words: &[(String, Rect)], wanted: &str) -> bool {
    words.iter().any(|(text, _)| text.contains(wanted))
}

fn place(words: &[(String, Rect)], wanted: &str) -> Rect {
    words
        .iter()
        .find(|(text, _)| text.contains(wanted))
        .map(|(_, place)| *place)
        .unwrap_or_else(|| panic!("{wanted:?} isn't drawn: {words:?}"))
}

fn step_of(state: &HomeState) -> Step {
    state.setup.as_ref().map(|setup| setup.step).expect("the setup is open")
}

/// The setup's card, as the window last placed it.
fn card(ctx: &egui::Context) -> Rect {
    ctx.memory(|memory| memory.area_rect(egui::Id::unique("setup"))).expect("the setup is drawn")
}

#[test]
fn five_steps_follow_the_welcome_on_every_system() {
    with_language(Language::English, || {
        let names: Vec<&str> = STEPS.iter().map(|step| step.name()).collect();
        assert_eq!(names, ["Welcome", "Languages", "Songs", "Bibles", "Screens", "Ready"]);
    });
    // A testing aid opens it at a step by name, or by its number along the top.
    for (name, step) in [
        ("languages", Step::Languages),
        ("Songs", Step::Songs),
        ("bibles", Step::Bibles),
        ("screens", Step::Screens),
        ("ready", Step::Ready),
        ("3", Step::Bibles),
        ("1", Step::Languages),
        ("anything else", Step::Welcome),
        // The steps there were before open where their part went.
        ("free-songs", Step::Songs),
        ("permissions", Step::Ready),
        ("reports", Step::Ready),
        ("reports-open", Step::Ready),
        ("done", Step::Ready),
    ] {
        assert_eq!(Setup::opened_at(name).step, step, "{name}");
    }
    assert_eq!(Setup::opened_at("permissions").fold, Some(Fold::Asks));
    assert_eq!(Setup::opened_at("reports-open").fold, Some(Fold::Reports));
    assert_eq!(Setup::opened_at("ready-reports").fold, Some(Fold::Reports));
    assert_eq!(Setup::opened_at("ready").fold, None);
}

#[test]
fn the_welcome_says_a_pre_release_is_one_and_a_release_says_nothing() {
    with_language(Language::English, || {
        let (heading, words) = pre_release_note("0.9.0-dev.16").expect("a pre-release says so");
        assert_eq!(heading, "This is a pre-release (0.9.0-dev.16)");
        assert_eq!(
            words,
            "Things may still change: the core of how SoftPresenter works and looks will stay, and bugs will be fixed in the versions that come next. Check for updates (Settings \u{203a} About) tells you when one is out."
        );
        assert!(!heading.contains('!') && !words.contains('!'), "calm, not alarming");
        // The welcome draws the card in a pre-release, and nothing of it in a release.
        let ctx = egui::Context::default();
        let mut state = home("prerelease");
        let words = draw(&ctx, &mut state, &env(System::Mac));
        assert!(said(&words, "Welcome to SoftPresenter") && said(&words, "This is a pre-release (0.9.0-dev.30)"));
        assert!(said(&words, "Language of SoftPresenter") && said(&words, "your languages, your songs, your Bibles"));
        let ctx = egui::Context::default();
        let mut state = home("release");
        let release = Env { version: "1.0.0".to_owned(), ..env(System::Mac) };
        assert!(!said(&draw(&ctx, &mut state, &release), "pre-release"));
    });
    for language in [Language::Spanish, Language::Russian, Language::Ukrainian] {
        let said = with_language(language, || pre_release_note("1.0.0-beta.1"));
        let (heading, words) = said.expect("a pre-release says so in every language");
        assert!(heading.contains("1.0.0-beta.1"), "{language:?}: {heading}");
        assert!(!words.contains("Things may"), "{language:?} has its own words: {words}");
    }
    assert_eq!(pre_release_note("0.9.0"), None);
    assert_eq!(pre_release_note("1.0.0"), None);
}

#[test]
fn a_first_start_goes_through_the_five_steps_to_the_end() {
    with_language(Language::English, || {
        let ctx = egui::Context::default();
        let env = env(System::Mac);
        let mut state = home("flow");
        let words = draw(&ctx, &mut state, &env);
        // The welcome: no steps along the top, Skip Setup and Get Started.
        assert!(said(&words, "Skip Setup") && said(&words, "Get Started") && !said(&words, "Languages"), "{words:?}");
        assert!(press(&ctx, &mut state, &env, "Get Started"));
        assert_eq!(step_of(&state), Step::Languages);
        let words = draw(&ctx, &mut state, &env);
        for wanted in [
            "Which languages does your church show?",
            "English",
            "Espa\u{f1}ol",
            "Русский",
            "Українська",
            "On top",
            "Below",
            "None (one language)",
        ] {
            assert!(said(&words, wanted), "Languages lacks {wanted:?}: {words:?}");
        }
        // Nothing is kept until the step is passed.
        assert!(state.church_languages.is_empty());
        assert!(press(&ctx, &mut state, &env, "Continue"));
        assert_eq!(step_of(&state), Step::Songs);
        assert_eq!(state.church_languages, ["en"]);
        let words = draw(&ctx, &mut state, &env);
        for wanted in [
            "Bring your songs in",
            "Already on this computer",
            "It looks in",
            "Look on This Computer",
            "Choose Song Files\u{2026}",
            "Free songbooks",
            "Recommended",
            "macOS may ask",
        ] {
            assert!(said(&words, wanted), "Songs lacks {wanted:?}: {words:?}");
        }
        // Look on This Computer asks for the search Settings' Find songs makes.
        assert!(press(&ctx, &mut state, &env, "Look on This Computer"));
        assert_eq!(state.find_songs.asks, [crate::find_songs::Ask::Find]);
        state.find_songs.asks.clear();
        // Continue installs the songbook ticked: the first English one.
        assert!(state.free_songbooks.asks.is_empty());
        assert!(press(&ctx, &mut state, &env, "Continue"));
        assert_eq!(step_of(&state), Step::Bibles);
        assert_eq!(state.free_songbooks.asks, [crate::free_songbooks::Ask::Install("christian-hymns")]);
        let words = draw(&ctx, &mut state, &env);
        // One language: one translation, no On top or Below.
        for wanted in [
            "Your Bible",
            "Choose the translation passages show.",
            "FOR YOUR LANGUAGES",
            "King James",
            "On this computer",
            "More Bibles",
        ] {
            assert!(said(&words, wanted), "Bibles lacks {wanted:?}: {words:?}");
        }
        assert!(!said(&words, "On top"), "{words:?}");
        let kjv = state.bible.shelf.position("kjv");
        assert_ne!(state.bible.rows[0].bible, kjv, "the Bible page starts with another on top");
        assert!(press(&ctx, &mut state, &env, "Continue"));
        assert_eq!(step_of(&state), Step::Screens);
        // Continue gave the Bible page the choice: the King James alone.
        assert_eq!(state.bible.rows[0].bible, kjv);
        assert_eq!(state.bible.rows.get(1).and_then(|row| row.bible), None);
        let words = draw(&ctx, &mut state, &env);
        assert!(said(&words, "Identify Screens") && said(&words, "Install NDI\u{2026}") && said(&words, "Projector"));
        assert!(press(&ctx, &mut state, &env, "Projector"));
        assert!(press(&ctx, &mut state, &env, "Continue"));
        assert_eq!(step_of(&state), Step::Ready);
        let words = draw(&ctx, &mut state, &env);
        for wanted in [
            "Ready for Sunday",
            "LANGUAGES",
            "SONGS",
            "BIBLES",
            "SCREENS",
            "REPORTS",
            "The congregation: Projector",
            "Anonymous: speeds and crashes.",
            "What macOS will ask",
            "What the reports send",
            "Click a slide to put it on screen",
            "Show a Test Slide",
            "Start Using SoftPresenter",
        ] {
            assert!(said(&words, wanted), "Ready lacks {wanted:?}: {words:?}");
        }
        assert!(!state.setup_done);
        assert!(press(&ctx, &mut state, &env, "Start Using SoftPresenter"));
        assert!(state.setup.is_none() && state.setup_done);
    });
}

#[test]
fn skip_changes_nothing_and_ready_says_what_is_missing() {
    with_language(Language::English, || {
        let ctx = egui::Context::default();
        let env = env(System::Windows);
        let mut state = home("skip");
        for screen in &mut state.screens {
            screen.output = Output::Off;
        }
        let rows = state.bible.rows.clone();
        assert!(press(&ctx, &mut state, &env, "Get Started"));
        for step in [Step::Languages, Step::Songs, Step::Bibles, Step::Screens] {
            assert_eq!(step_of(&state), step);
            assert!(press(&ctx, &mut state, &env, "Skip"), "{step:?} has Skip");
        }
        assert_eq!(step_of(&state), Step::Ready);
        assert!(state.church_languages.is_empty() && state.free_songbooks.asks.is_empty());
        assert!(state.find_songs.asks.is_empty() && state.bible.wanted.is_empty());
        assert_eq!(state.bible.rows, rows);
        let words = draw(&ctx, &mut state, &env);
        // No screen chosen is the one thing Ready warns about, with Choose; no songs says so.
        assert!(said(&words, "No screen chosen yet: slides won\u{2019}t show anywhere until one is."), "{words:?}");
        assert!(said(&words, "Choose") && said(&words, "None yet. Your own songs, or free songbooks"), "{words:?}");
        assert!(press(&ctx, &mut state, &env, "Choose"));
        assert_eq!(step_of(&state), Step::Screens);
        // Back goes back a step, and a step along the top that was reached can be clicked.
        assert!(press(&ctx, &mut state, &env, "Back"));
        assert_eq!(step_of(&state), Step::Bibles);
        assert!(press(&ctx, &mut state, &env, "Ready"));
        assert_eq!(step_of(&state), Step::Ready);
        // Skip Setup, from the welcome, closes it and says where it lives.
        let mut state = home("skip-setup");
        assert!(press(&ctx, &mut state, &env, "Skip Setup"));
        assert!(state.setup.is_none() && state.setup_done);
        assert!(state.notices.said_since(0).is_some_and(|said| said.contains("Set up SoftPresenter\u{2026}")));
    });
}

#[test]
fn the_languages_choose_what_is_recommended() {
    // The computer's languages start chosen, those the step offers; kept answers come back.
    let tags = |tags: &[&str]| tags.iter().map(|tag| (*tag).to_owned()).collect::<Vec<String>>();
    assert_eq!(languages::starting(&[], &tags(&["ru-RU", "en-US", "de-DE"])), ["ru", "en"]);
    assert_eq!(languages::starting(&[], &tags(&["cs-CZ"])), ["en"]);
    assert_eq!(languages::starting(&[], &[]), ["en"]);
    assert_eq!(languages::starting(&tags(&["uk", "ru"]), &tags(&["en-US"])), ["uk", "ru"]);
    // The song catalog: the church's languages, not the app's own.
    let church = tags(&["ru", "uk"]);
    assert_eq!(crate::songbooks::recommended_for(&church), ["ru", "uk"]);
    assert!(crate::songbooks::recommended_for(&[]).is_empty());
    // The free songbooks: the first of each of the church's languages.
    assert_eq!(crate::free_songbooks::church_languages_offered(&church), ["ru", "uk"]);
    // The Bibles: the first in each language, one on this computer before one to download.
    let shelf = crate::bible::Shelf::shipped();
    let all = bibles::every(&shelf, true);
    let name = |language: &str| bibles::recommended(&all, language).map(|bible| (bible.id.as_str(), bible.here));
    assert_eq!(name("en"), Some(("kjv", true)));
    assert_eq!(name("ru"), Some(("syn", true)));
    let (ukrainian, here) = name("uk").expect("a Ukrainian Bible is offered");
    assert!(!here || shelf.position(ukrainian).is_some());
    assert_eq!(name("xx"), None);
    // Without downloads, only what is here is listed.
    assert!(bibles::every(&shelf, false).iter().all(|bible| bible.here));
    let recommended: Vec<&str> = bibles::recommended_for(&all, &church).iter().map(|bible| bible.id.as_str()).collect();
    assert_eq!(recommended, ["syn", ukrainian]);
    // A first start puts the first language's on top and the second's below.
    let mut choice = bibles::Choice::default();
    choice.follow(&all, &church);
    assert_eq!((choice.top.as_deref(), choice.below.as_deref()), (Some("syn"), Some(ukrainian)));
    // A Bible chosen in the right language stays when the step is passed again.
    choice.put_on_top("syn");
    choice.follow(&all, &tags(&["ru", "en"]));
    assert_eq!((choice.top.as_deref(), choice.below.as_deref()), (Some("syn"), Some("kjv")));
    choice.put_below("web");
    choice.follow(&all, &tags(&["ru", "en"]));
    assert_eq!(choice.below.as_deref(), Some("web"), "another English one chosen stays");
    // One language leaves one translation.
    choice.follow(&all, &tags(&["en"]));
    assert_eq!((choice.top.as_deref(), choice.below.as_deref()), (Some("kjv"), None));
}

#[test]
fn on_top_and_below_swap_and_below_again_leaves_one() {
    let mut choice = bibles::Choice::default();
    choice.top = Some("syn".to_owned());
    choice.below = Some("kjv".to_owned());
    choice.put_on_top("kjv");
    assert_eq!((choice.top.as_deref(), choice.below.as_deref()), (Some("kjv"), Some("syn")));
    choice.put_below("kjv");
    assert_eq!((choice.top.as_deref(), choice.below.as_deref()), (Some("syn"), Some("kjv")));
    choice.put_on_top("web");
    assert_eq!((choice.top.as_deref(), choice.below.as_deref()), (Some("web"), Some("kjv")));
    choice.put_below("kjv");
    assert_eq!((choice.top.as_deref(), choice.below.as_deref()), (Some("web"), None));
    // The one on top can't go below with nothing to take its place.
    choice.put_below("web");
    assert_eq!((choice.top.as_deref(), choice.below.as_deref()), (Some("web"), None));
    choice.only("bsb");
    assert_eq!((choice.top.as_deref(), choice.below.as_deref()), (Some("bsb"), None));
}

#[test]
fn a_russian_and_ukrainian_church_is_recommended_its_songbooks_and_bibles() {
    with_language(Language::English, || {
        let ctx = egui::Context::default();
        let env = env(System::Mac);
        let mut state = home("ru-uk");
        // The app is in English; the church shows Russian on top and Ukrainian below.
        state.setup = Some(Setup::opened_at("languages"));
        draw(&ctx, &mut state, &env);
        state.setup.as_mut().unwrap().languages = vec!["ru".to_owned(), "uk".to_owned()];
        let words = draw(&ctx, &mut state, &env);
        assert!(said(&words, "How a song will look.") && said(&words, "Слова песни") && said(&words, "Слова пісні"));
        assert!(place(&words, "Слова песни").center().y < place(&words, "Слова пісні").center().y, "Russian is on top");
        assert!(press(&ctx, &mut state, &env, "Continue"));
        assert_eq!(state.church_languages, ["ru", "uk"]);
        assert_eq!(crate::songbooks::church_languages(), ["ru", "uk"]);
        crate::songbooks::set_church_languages(&[]);
        // Songs: the first Russian and the first Ukrainian songbook are ticked and say
        // Recommended, and no English one is.
        let words = draw(&ctx, &mut state, &env);
        let setup = state.setup.as_ref().unwrap();
        assert_eq!(setup.shared.ticked, ["pv3300", "evangelski-pisni"]);
        let recommended: Vec<&String> =
            words.iter().map(|(text, _)| text).filter(|text| text.contains("Recommended")).collect();
        assert_eq!(recommended.len(), 2, "{recommended:?}");
        assert!(
            recommended.iter().all(|line| line.contains("Русский") || line.contains("Українська")),
            "{recommended:?}"
        );
        // The list starts with the church's languages, Russian first.
        let first = |name: &str| place(&words, name).min.y;
        assert!(first("Песнь Возрождения 3300") < first("Євангельські Пісні"));
        assert!(first("Євангельські Пісні") < first("Christian Hymns"));
        assert!(press(&ctx, &mut state, &env, "Continue"));
        assert_eq!(
            state.free_songbooks.asks,
            [crate::free_songbooks::Ask::Install("pv3300"), crate::free_songbooks::Ask::Install("evangelski-pisni")]
        );
        // Bibles: the Synodal on top, a Ukrainian one below, which downloads with Continue.
        let words = draw(&ctx, &mut state, &env);
        for wanted in [
            "Your Bibles",
            "On top",
            "Below",
            "Русский \u{b7} On this computer",
            "Українська \u{b7} Downloads with Continue",
            "Recommended",
        ] {
            assert!(said(&words, wanted), "Bibles lacks {wanted:?}: {words:?}");
        }
        let choice = &state.setup.as_ref().unwrap().bibles;
        assert_eq!(choice.top.as_deref(), Some("syn"));
        let ukrainian = choice.below.clone().expect("a Ukrainian Bible is chosen below");
        assert!(state.bible.shelf.position(&ukrainian).is_none(), "it is still to download");
    });
}

#[test]
fn bibles_reach_the_bible_page_only_with_continue_and_one_to_download_when_it_is_here() {
    let mut state = home("bibles");
    let mut setup = Setup::new();
    setup.bibles.top = Some("kjv".to_owned());
    setup.bibles.below = Some("no-such-bible".to_owned());
    let rows = state.bible.rows.clone();
    // Where Bibles can't be downloaded, one that isn't here is left out; the top goes on.
    let nowhere = Env { bibles_download: false, ..env(System::Linux) };
    bibles::passed(&mut setup, &mut state, &nowhere, &egui::Context::default());
    assert_eq!(state.bible.rows[0].bible, state.bible.shelf.position("kjv"));
    assert_eq!(state.bible.rows[1], rows[1], "the row below keeps what it had");
    assert!(state.bible.wanted.is_empty());
    // One the setup waits for takes its row once it is on the shelf.
    let web = state.bible.shelf.position("web").unwrap();
    state.bible.wanted = vec![("web".to_owned(), 1)];
    place_downloaded(&mut state.bible);
    assert_eq!(state.bible.rows[1].bible, Some(web));
    assert!(state.bible.wanted.is_empty());
    // One whose download ended without it is waited for no longer, and the row stays.
    state.bible.wanted = vec![("never-came".to_owned(), 0)];
    place_downloaded(&mut state.bible);
    assert!(state.bible.wanted.is_empty());
    assert_eq!(state.bible.rows[0].bible, state.bible.shelf.position("kjv"));
    // Ready names the Bibles as they stand, top first.
    assert_eq!(bibles::names(&state).len(), 2);
}

#[test]
fn reports_start_off_in_the_eu_the_eea_and_the_uk() {
    use crate::reports::region::starts_on;
    for country in ["DE", "FR", "NO", "IS", "LI", "GB", "PL"] {
        assert!(!starts_on(Some(country)), "{country}");
    }
    for country in ["US", "CA", "UA", "BR"] {
        assert!(starts_on(Some(country)), "{country}");
    }
    with_language(Language::English, || {
        // The line follows the region, never the switch; the mark follows the switch.
        let off = ready::reports_line(&standing(false));
        assert_eq!(off.mark, ready::Mark::Dash);
        assert_eq!(
            off.words,
            "Off until turned on; they help us fix problems. Never your songs, Bible text, file names or anything about you."
        );
        let on = ready::reports_line(&standing(true));
        assert_eq!(on.mark, ready::Mark::Tick);
        assert_eq!(
            on.words,
            "Anonymous: speeds and crashes. Never your songs, Bible text, file names or anything about you."
        );
        let turned_on = Standing { enabled: true, ..standing(false) };
        assert_eq!(ready::reports_line(&turned_on).words, off.words);
        assert_eq!(ready::reports_line(&turned_on).mark, ready::Mark::Tick);
        // Drawn: a computer in Germany sees them off, with the line for there; the switch acts
        // at once and moves nothing.
        let ctx = egui::Context::default();
        let german = Env { reports: Some(standing(starts_on(Some("DE")))), ..env(System::Windows) };
        let mut state = home("reports-eu");
        state.setup = Some(Setup::opened_at("ready"));
        let words = draw(&ctx, &mut state, &german);
        assert!(said(&words, "Off until turned on; they help us fix problems.") && !said(&words, "Anonymous: speeds"));
        let line = place(&words, "Off until turned on");
        let key = words.iter().find(|(text, _)| text == "Off").map(|(_, place)| *place).expect("the switch says Off");
        assert!(!words.iter().any(|(text, _)| text == "On"));
        assert!(press(&ctx, &mut state, &german, "Off"));
        assert!(state.setup.as_ref().unwrap().reports.as_ref().unwrap().enabled, "the switch acts at once");
        // The reporter says how it stands at the next drawing.
        let now_on = Env { reports: Some(Standing { enabled: true, ..standing(false) }), ..german.clone() };
        let words = draw(&ctx, &mut state, &now_on);
        assert!(words.iter().any(|(text, _)| text == "On"), "{words:?}");
        assert_eq!(place(&words, "Off until turned on"), line, "the line keeps its words and its place");
        let on_key = words.iter().find(|(text, _)| text == "On").map(|(_, place)| *place).unwrap();
        // The key is one size whichever word it says: the two words share a middle.
        assert!((on_key.center().x - key.center().x).abs() < 2.0 && (on_key.center().y - key.center().y).abs() < 2.0);
        // Elsewhere they are on, and say so.
        let mut state = home("reports-us");
        state.setup = Some(Setup::opened_at("ready"));
        let words = draw(&ctx, &mut state, &env(System::Mac));
        assert!(said(&words, "Anonymous: speeds and crashes.") && words.iter().any(|(text, _)| text == "On"));
        // A build without the reports has no line and no fold for them.
        let none = Env { reports: None, ..env(System::Mac) };
        let mut state = home("reports-none");
        state.setup = Some(Setup::opened_at("ready"));
        let words = draw(&ctx, &mut state, &none);
        assert!(
            !said(&words, "REPORTS") && !said(&words, "What the reports send") && said(&words, "What macOS will ask")
        );
    });
}

#[test]
fn ready_folds_what_the_system_asks_one_open_at_a_time() {
    with_language(Language::English, || {
        let titles = |system, reports| {
            ready::fold_titles(system, reports).into_iter().map(|(_, title)| title).collect::<Vec<_>>()
        };
        assert_eq!(titles(System::Mac, true), ["What macOS will ask", "What the reports send"]);
        assert_eq!(titles(System::Windows, true), ["What Windows will ask", "What the reports send"]);
        // Nothing asks on Linux.
        assert_eq!(titles(System::Linux, true), ["What the reports send"]);
        assert!(titles(System::Linux, false).is_empty());
        let ctx = egui::Context::default();
        let env = env(System::Mac);
        let mut state = home("folds");
        state.setup = Some(Setup::opened_at("ready"));
        let closed = draw(&ctx, &mut state, &env);
        assert!(!said(&closed, "Local Network") && !said(&closed, "The next report, as it stands now"));
        assert!(press(&ctx, &mut state, &env, "What macOS will ask"));
        let open = draw(&ctx, &mut state, &env);
        for wanted in [
            "macOS asks before an app does some things",
            "Local Network",
            "Files in your Documents folder",
            "Controlling the app that opens a presentation",
            "SoftPresenter never asks for your password",
        ] {
            assert!(said(&open, wanted), "{wanted:?}: {open:?}");
        }
        // The fold's key keeps its place and size, open or closed.
        assert_eq!(place(&open, "What macOS will ask").size(), place(&closed, "What macOS will ask").size());
        // Opening the other closes this one.
        assert!(press(&ctx, &mut state, &env, "What the reports send"));
        let other = draw(&ctx, &mut state, &env);
        assert!(!said(&other, "Local Network") && said(&other, "The next report, as it stands now"), "{other:?}");
        assert!(said(&other, "Read the privacy page") && said(&other, "This is a test copy"));
        // Windows hears only of its firewall.
        let windows = Env { system: System::Windows, ..env.clone() };
        let mut state = home("folds-windows");
        state.setup = Some(Setup::opened_at("permissions"));
        let words = draw(&ctx, &mut state, &windows);
        for wanted in ["What Windows will ask", "Windows Defender Firewall", "private networks", "Remote Control"] {
            assert!(said(&words, wanted), "{wanted:?}: {words:?}");
        }
        assert!(!said(&words, "macOS"));
    });
}

/// Every word the setup draws on `system`, step by step, in English.
fn all_words(system: System) -> String {
    with_language(Language::English, || {
        let ctx = egui::Context::default();
        let env = env(system);
        let mut out = Vec::new();
        for step in ["welcome", "languages", "songs", "bibles", "screens", "ready", "permissions", "reports-open"] {
            let mut state = home("words");
            state.setup = Some(Setup::opened_at(step));
            out.extend(draw(&ctx, &mut state, &env).into_iter().map(|(text, _)| text));
        }
        out.join("\n")
    })
}

#[test]
fn off_a_mac_the_setup_never_speaks_of_one() {
    for system in [System::Windows, System::Linux] {
        let words = all_words(system);
        for mac in ["Mac", "macOS", "Finder", "System Settings", "\u{2318}", "Command-", "menu bar", "File menu"] {
            // The displays' sample names aside, nothing names a Mac.
            assert!(!words.replace("Built-in", "").contains(mac), "{system:?} says {mac:?}:\n{words}");
        }
    }
    let mac = all_words(System::Mac);
    for said in ["macOS may ask whether SoftPresenter can look in each folder", "What macOS will ask", "Local Network"]
    {
        assert!(mac.contains(said), "a Mac doesn't hear {said:?}:\n{mac}");
    }
    let windows = all_words(System::Windows);
    assert!(windows.contains("What Windows will ask") && windows.contains("Windows Defender Firewall"));
    let linux = all_words(System::Linux);
    assert!(!linux.contains("will ask") && linux.contains("What the reports send"));
}

/// A source the search found, with `songs` new songs.
fn source(name: &str, songs: usize) -> crate::find_songs::Source {
    use crate::find_songs::{Kind, PlaceName, Read, Usual, Where};
    crate::find_songs::Source {
        name: name.to_owned(),
        kind: Kind::Songbook,
        place: Where { place: PlaceName::Usual(Usual::SoftProjector), parts: Vec::new() },
        path: std::path::PathBuf::from(name),
        read: Read::Nothing,
        songs,
        here: 0,
        twice: 0,
        why: None,
        caution: None,
        cloud: false,
        new: (0..songs as u64).map(|n| n + songs as u64 * 1000).collect(),
    }
}

#[test]
fn songs_found_are_ticked_and_come_in_with_bring_in_and_continue() {
    with_language(Language::English, || {
        let ctx = egui::Context::default();
        let env = env(System::Windows);
        let mut state = home("found");
        state.setup = Some(Setup::opened_at("songs"));
        let before = draw(&ctx, &mut state, &env);
        assert!(said(&before, "Continue") && !said(&before, "Bring In and Continue"));
        assert!(!said(&before, "macOS"), "only a Mac is told what macOS asks");
        // The search found two songbooks: both are ticked, and the key says what it does.
        state.find_songs.phase = crate::find_songs::Phase::Found;
        state.find_songs.add(source("Песнь Возрождения", 3301));
        state.find_songs.add(source("Hymns", 12));
        let found = draw(&ctx, &mut state, &env);
        for wanted in [
            "Песнь Возрождения",
            "Hymns",
            "from SoftProjector",
            "3,313 songs come in with Continue, each library under its own name.",
            "Bring In and Continue",
        ] {
            assert!(said(&found, wanted), "{wanted:?}: {found:?}");
        }
        // The card's own line goes once there are results.
        assert!(said(&before, "Song libraries, SoftProjector songbooks and song files"));
        assert!(!said(&found, "Song libraries, SoftProjector songbooks and song files"));
        // The key keeps its place and size whatever it says.
        let (was, now) = (place(&before, "Continue"), place(&found, "Bring In and Continue"));
        assert!((was.center().x - now.center().x).abs() < 2.0 && (was.center().y - now.center().y).abs() < 2.0);
        assert!(state.find_songs.asks.is_empty(), "nothing comes in until the key is pressed");
        assert!(press(&ctx, &mut state, &env, "Bring In and Continue"));
        assert_eq!(step_of(&state), Step::Bibles);
        assert_eq!(state.find_songs.asks, [crate::find_songs::Ask::Import]);
        // Unticked, nothing comes in and the key says Continue.
        let mut state = home("found-unticked");
        state.setup = Some(Setup::opened_at("songs"));
        state.find_songs.phase = crate::find_songs::Phase::Found;
        state.find_songs.add(source("Hymns", 12));
        state.find_songs.ticked[0] = false;
        let unticked = draw(&ctx, &mut state, &env);
        assert!(said(&unticked, "Tick what to bring in.") && !said(&unticked, "Bring In and Continue"));
        assert!(press(&ctx, &mut state, &env, "Skip"));
        assert!(state.find_songs.asks.is_empty() && state.free_songbooks.asks.is_empty());
        // What came in is said on Ready.
        state.find_songs.phase = crate::find_songs::Phase::Done;
        state.find_songs.brought = Some(crate::find_songs::Brought {
            came: 3313,
            from: vec![("Песнь Возрождения".to_owned(), 3301), ("Hymns".to_owned(), 12)],
            ..Default::default()
        });
        assert_eq!(ready::songs_line(&state).words, "3,313 songs from this computer, in 2 libraries");
        // A download that failed turns the line amber, with Try Again.
        state.free_songbooks.problem = Some("Песнь Возрождения 3300 didn\u{2019}t download.".to_owned());
        assert_eq!(ready::songs_line(&state).mark, ready::Mark::Care);
        state.setup = Some(Setup::opened_at("ready"));
        assert!(said(&draw(&ctx, &mut state, &env), "Try Again"));
    });
}

/// The states Ready is checked in: how reports stand, a download that failed, songs brought
/// in from this computer, no screen chosen.
fn ready_cases() -> Vec<(&'static str, Option<Standing>, fn(&mut HomeState))> {
    vec![
        ("reports on", Some(standing(true)), |_| {}),
        ("reports off", Some(standing(false)), |_| {}),
        ("no reports", None, |_| {}),
        ("a download failed", Some(standing(true)), |state| {
            state.free_songbooks.problem = Some(
                "Песнь Возрождения 3300 couldn\u{2019}t be downloaded: the connection dropped part way. Try again later."
                    .to_owned(),
            );
        }),
        ("songs brought in", Some(standing(false)), |state| {
            state.find_songs.phase = crate::find_songs::Phase::Done;
            state.find_songs.brought = Some(crate::find_songs::Brought {
                came: 4372,
                from: (0..6).map(|n| (format!("Library {n}"), 700)).collect(),
                ..Default::default()
            });
            state.free_songbooks.known.busy.insert("pv3300", Some(38));
        }),
        ("no screen chosen", Some(standing(true)), |state| {
            for screen in &mut state.screens {
                screen.output = Output::Off;
            }
        }),
    ]
}

/// Checks one drawing of a step: the card is inside the window, the step's content is no
/// taller than its room (nothing to scroll), and no words run past the card's sides.
fn check_fit(ctx: &egui::Context, words: &[(String, Rect)], what: &str) {
    let whole = card(ctx);
    assert!(whole.min.y >= 0.0 && whole.max.y <= 700.0, "{what}: the card is {whole:?}");
    assert!(whole.min.x >= 0.0 && whole.max.x <= 1100.0, "{what}: the card is {whole:?}");
    let fit = fit(ctx);
    assert!(fit.room > 300.0, "{what}: {fit:?}");
    assert!(fit.content <= fit.room + 0.5, "{what}: the step is {} tall in a room of {}", fit.content, fit.room);
    for (text, at) in words {
        if whole.contains(at.center()) {
            assert!(
                at.min.x >= whole.min.x - 0.5 && at.max.x <= whole.max.x + 0.5,
                "{what}: {text:?} runs past the card ({at:?} in {whole:?})"
            );
        }
    }
}

#[test]
fn every_step_fits_a_window_of_1100_by_700_with_nothing_to_scroll() {
    for language in EVERY_LANGUAGE {
        with_language(language, || {
            for system in SYSTEMS {
                for again in [false, true] {
                    for church in [vec!["en"], vec!["ru", "uk"], vec!["en", "es", "ru", "uk"]] {
                        for step in ["welcome", "languages", "songs", "bibles", "screens"] {
                            let ctx = egui::Context::default();
                            let mut state = home("fit");
                            state.setup_done = again;
                            state.church_languages = church.iter().map(|code| (*code).to_owned()).collect();
                            if again {
                                // Someone whose Bibles are the recommended ones: one chosen from
                                // More Bibles opens it, and an open fold may scroll.
                                let all = bibles::every(&state.bible.shelf, false);
                                let on_shelf = |at: usize| {
                                    church
                                        .get(at)
                                        .and_then(|language| bibles::recommended(&all, language))
                                        .and_then(|bible| state.bible.shelf.position(&bible.id))
                                };
                                let (top, below) = (on_shelf(0), on_shelf(1));
                                state.bible.rows[0].bible = top;
                                state.bible.rows[1].bible = below;
                            }
                            state.setup = Some(Setup::opened_at(step));
                            let what = format!("{language:?} {system:?} again={again} {church:?} {step}");
                            let words = draw(&ctx, &mut state, &env(system));
                            check_fit(&ctx, &words, &what);
                            if step == "songs" {
                                let card = songs::card_fit(&ctx);
                                assert!(card.content <= card.room + 0.5, "{what}: the songs card holds {card:?}");
                            }
                        }
                    }
                    // Ready, with both folds closed, in each of its states.
                    for (case, reports, prepare) in ready_cases() {
                        let ctx = egui::Context::default();
                        let mut state = home("fit-ready");
                        state.setup_done = again;
                        state.church_languages = vec!["ru".to_owned(), "uk".to_owned()];
                        prepare(&mut state);
                        state.setup = Some(Setup::opened_at("ready"));
                        let env = Env { reports, ..env(system) };
                        let words = draw(&ctx, &mut state, &env);
                        check_fit(&ctx, &words, &format!("{language:?} {system:?} again={again} Ready, {case}"));
                    }
                }
            }
            // The Songs step with a search's finds, and with the song catalog offered too.
            for catalog in [false, true] {
                let ctx = egui::Context::default();
                let mut state = home("fit-songs");
                state.find_songs.phase = crate::find_songs::Phase::Found;
                for n in 0..7 {
                    state.find_songs.add(source(&format!("Песнь Возрождения {n}"), 3301));
                }
                state.setup = Some(Setup::opened_at("songs"));
                let env = Env { catalog, ..env(System::Mac) };
                let words = draw(&ctx, &mut state, &env);
                check_fit(&ctx, &words, &format!("{language:?} Songs with finds, catalog={catalog}"));
                let card = songs::card_fit(&ctx);
                assert!(
                    card.content <= card.room + 0.5,
                    "{language:?} catalog={catalog}: the songs card holds {card:?}"
                );
            }
        });
    }
}

#[test]
fn the_foot_keys_keep_their_places_and_sizes_on_every_step() {
    for language in EVERY_LANGUAGE {
        with_language(language, || {
            let ctx = egui::Context::default();
            let env = env(System::Mac);
            let mut cards = Vec::new();
            let mut ons = Vec::new();
            let mut backs = Vec::new();
            let mut skips = Vec::new();
            for step in STEPS {
                let mut state = home("foot");
                state.setup = Some(Setup::opened_at(&format!("{step:?}")));
                let words = draw(&ctx, &mut state, &env);
                cards.push(card(&ctx));
                let centre = |wanted: &str| words.iter().find(|(text, _)| text == wanted).map(|(_, at)| at.center());
                ons.push(
                    centre(on_words(step, false, false))
                        .unwrap_or_else(|| panic!("{language:?} {step:?} has no way on")),
                );
                let back = if step == Step::Welcome { skip_setup_words() } else { t("setup.back", "Back") };
                backs.push((step, centre(back).unwrap_or_else(|| panic!("{language:?} {step:?} has no {back}"))));
                if !matches!(step, Step::Welcome | Step::Ready) {
                    skips.push(
                        centre(t("setup.skip_step", "Skip"))
                            .unwrap_or_else(|| panic!("{language:?} {step:?} has no Skip")),
                    );
                }
            }
            // Words in the middle of one key share a middle, to within what their letters' shapes differ by.
            let same = |a: egui::Pos2, b: egui::Pos2| (a.x - b.x).abs() < 2.0 && (a.y - b.y).abs() < 2.0;
            assert!(cards.windows(2).all(|pair| pair[0] == pair[1]), "{language:?}: the card changes size: {cards:?}");
            assert!(ons.windows(2).all(|pair| same(pair[0], pair[1])), "{language:?}: the way on moves: {ons:?}");
            assert!(skips.windows(2).all(|pair| same(pair[0], pair[1])), "{language:?}: Skip moves: {skips:?}");
            // Back keeps its place on every step that has Skip beside it (Ready has Show a Test
            // Slide there, which is wider).
            let middle: Vec<egui::Pos2> =
                backs.iter().filter(|(step, _)| *step != Step::Ready).map(|(_, at)| *at).collect();
            assert!(middle.windows(2).all(|pair| same(pair[0], pair[1])), "{language:?}: Back moves: {backs:?}");
            // Every wording of the way on fits its key, and every side key's fits its own.
            let whole = cards[0];
            assert!(ons[0].x < whole.max.x && ons[0].y < whole.max.y);
        });
    }
}

#[test]
fn nothing_moves_when_a_language_is_chosen() {
    with_language(Language::English, || {
        let ctx = egui::Context::default();
        let env = env(System::Mac);
        let mut state = home("still");
        state.setup = Some(Setup::opened_at("languages"));
        let one = draw(&ctx, &mut state, &env);
        assert!(said(&one, "One language on screen."));
        assert!(press(&ctx, &mut state, &env, "Русский"));
        let two = draw(&ctx, &mut state, &env);
        assert_eq!(state.setup.as_ref().unwrap().languages, ["en", "ru"]);
        assert!(said(&two, "How a song will look."));
        for fixed in ["English", "Українська", "On top", "Below", "Continue", "Back", "Skip"] {
            let at = |words: &[(String, Rect)]| words.iter().find(|(text, _)| text == fixed).map(|(_, at)| *at);
            assert_eq!(at(&one), at(&two), "{fixed:?} moved");
        }
        // The fourth language says the third's songs and Bibles are there too.
        assert!(press(&ctx, &mut state, &env, "Українська"));
        assert!(said(&draw(&ctx, &mut state, &env), "Українська songs and Bibles are there too"));
        // With none chosen the way on waits.
        for name in ["English", "Русский", "Українська"] {
            assert!(press(&ctx, &mut state, &env, name));
        }
        assert!(said(&draw(&ctx, &mut state, &env), "Choose at least one."));
        press(&ctx, &mut state, &env, "Continue");
        assert_eq!(step_of(&state), Step::Languages, "no language, no going on");
    });
}

#[test]
fn opened_again_it_changes_nothing_of_an_existing_users_settings_unless_asked() {
    with_language(Language::English, || {
        let ctx = egui::Context::default();
        let env = env(System::Mac);
        // Someone who set SoftPresenter up before there were church languages.
        let mut state = home("again");
        state.setup = None;
        state.setup_done = true;
        let before = state.prefs();
        assert!(before.setup_done && before.church_languages.is_empty());
        let rows = state.bible.rows.clone();
        let screens = state.screens.clone();
        // Their preferences file has no such line, and gains none.
        let written = serde_json::to_string(&before).unwrap();
        assert!(!written.contains("church_languages"), "{written}");
        let read: crate::service::Prefs = serde_json::from_str(&written).unwrap();
        assert!(read.church_languages.is_empty());
        // Opened from Settings: a sheet with their answers, Close and Continue.
        state.setup = Some(Setup::new());
        let words = draw(&ctx, &mut state, &env);
        assert!(said(&words, "Set up SoftPresenter") && said(&words, "Your answers are filled in."));
        assert!(said(&words, "Close") && !said(&words, "Skip Setup") && !said(&words, "Welcome to"));
        // Walked through with Skip and Back, then closed with Escape: nothing changed.
        assert!(press(&ctx, &mut state, &env, "Continue"));
        for _ in 0..4 {
            draw(&ctx, &mut state, &env);
            assert!(press(&ctx, &mut state, &env, "Skip"));
        }
        assert_eq!(step_of(&state), Step::Ready);
        let ready = draw(&ctx, &mut state, &env);
        // The sheet leaves the first-timer's tips out.
        assert!(!said(&ready, "Click a slide to put it on screen"));
        for _ in 0..3 {
            assert!(press(&ctx, &mut state, &env, "Back"));
            draw(&ctx, &mut state, &env);
        }
        assert_eq!(step_of(&state), Step::Songs);
        let show = Show::new(Vec::new());
        let escape = egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        drawn(&ctx, vec![escape], &mut |ui| {
            let mut commands = Vec::new();
            setup_ui_in(ui.ctx(), &show, &mut state, &mut commands, &env);
        });
        assert!(state.setup.is_none());
        assert_eq!(state.prefs(), before, "the preferences are as they were");
        assert_eq!(state.bible.rows, rows);
        assert_eq!(state.screens, screens);
        assert!(state.free_songbooks.asks.is_empty() && state.songbooks.asks.is_empty());
        assert!(state.find_songs.asks.is_empty() && state.bible.wanted.is_empty());
        assert!(state.notices.said_since(0).is_none(), "nothing is said to someone who knows where it is");
        // The Bibles step shows their Bibles as they stand, and Continue with nothing changed
        // leaves them.
        state.setup = Some(Setup::opened_at("bibles"));
        draw(&ctx, &mut state, &env);
        let choice = state.setup.as_ref().unwrap().bibles.clone();
        let id = |row: usize| rows[row].bible.map(|at| state.bible.shelf.bibles[at].id.clone());
        assert_eq!((choice.top, choice.below), (id(0), id(1)));
        assert!(press(&ctx, &mut state, &env, "Continue"));
        assert_eq!(state.bible.rows, rows);
        // Close, from the welcome, changes nothing either.
        state.setup = Some(Setup::new());
        assert!(press(&ctx, &mut state, &env, "Close"));
        assert_eq!(state.prefs(), before);
    });
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
    let (main, window) =
        with_language(Language::English, || (several_displays_words(Some("Built-in")), several_displays_words(None)));
    assert!(main.starts_with("The main display (Built-in) is kept"));
    assert!(window.starts_with("The display SoftPresenter's own window is on is kept"));
}

#[test]
fn the_screens_step_has_ndi_beside_identify() {
    with_language(Language::English, || {
        for (ndi, key, line) in [
            (false, "Install NDI\u{2026}", "NDI sends the slides over your network, for a live stream"),
            (true, "NDI is installed", "choose NDI for it in Settings \u{203a} Screens"),
        ] {
            let ctx = egui::Context::default();
            let mut state = home("ndi");
            state.ndi_available = ndi;
            state.setup = Some(Setup::opened_at("screens"));
            let words = draw(&ctx, &mut state, &env(System::Mac));
            assert!(said(&words, "Identify Screens") && said(&words, key) && said(&words, line), "{ndi}: {words:?}");
            assert!(said(&words, "Identify shows each screen\u{2019}s name on it."));
            // Identify and NDI share one row: the key (or the line that NDI is here) starts
            // level with Identify Screens, to its right.
            let (identify, beside) = (place(&words, "Identify Screens"), place(&words, key));
            assert!(beside.min.x > identify.max.x, "{identify:?} {beside:?}");
            assert!((beside.center().y - identify.center().y).abs() < 4.0, "{identify:?} {beside:?}");
            check_fit(&ctx, &words, &format!("Screens, NDI {ndi}"));
        }
    });
}

#[test]
fn install_ndi_in_the_setup_opens_the_window_over_the_home_window() {
    with_language(Language::English, || {
        let ctx = egui::Context::default();
        let mut state = home("open");
        state.setup = Some(Setup::opened_at("screens"));
        let words = draw(&ctx, &mut state, &env(System::Mac));
        assert!(press(&ctx, &mut state, &env(System::Mac), "Install NDI\u{2026}"), "the key is drawn: {words:?}");
        assert_eq!(state.ndi_download.shown_in(), Some(crate::ndi_download::Place::Home));
        // The setup keeps its step, to come back to when the window is left.
        assert!(state.setup.as_ref().is_some_and(|setup| setup.step == Step::Screens));
    });
}
