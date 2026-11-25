//! The Settings window (Cmd+,), regrouped as the design has it (25 November): its pages in four
//! groups down the left (The room, On screen, Connect, Advanced), each row ending in how its page
//! stands, and the page chosen beside them. Every page shows the thing first, one main control
//! and a line of words, with the rest under More.

use egui::{Align, Align2, FontId, Frame, Layout, Rect, Sense, pos2, vec2};
use sp_engine::Show;

use super::{HomeState, Page, open_page};
use crate::i18n::{t, tf, tk, tn};
use crate::icons::{self, Icon};
use crate::studio::{
    BG, CARD, Flat, GAP, PANEL, RADIUS, RADIUS_PANEL, READY, ROW_HOVER, TEXT, TEXT_DIM, TEXT_FAINT, fixed,
    fixed_semibold, flat_button, paint_light, paint_spaced, semibold, spaced, within,
};

/// A page of Settings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Pane {
    #[default]
    Screens,
    PhoneScreens,
    Slides,
    Groups,
    Language,
    Integrations,
    AiAssistants,
    PhoneRemote,
    Keys,
    CommandLine,
    SongFiles,
    About,
}

/// The list's four groups, in order, each with its pages: what shows and sounds in the room, how
/// slides look and behave, other services, assistants and phones, and the rest, out of the way.
const GROUPS: [(Group, &[Pane]); 4] = [
    (Group::Room, &[Pane::Screens, Pane::PhoneScreens]),
    (Group::OnScreen, &[Pane::Slides, Pane::Groups, Pane::Language]),
    (Group::Connect, &[Pane::Integrations, Pane::AiAssistants, Pane::PhoneRemote]),
    (Group::Advanced, &[Pane::Keys, Pane::CommandLine, Pane::SongFiles, Pane::About]),
];

#[derive(Clone, Copy)]
enum Group {
    Room,
    OnScreen,
    Connect,
    Advanced,
}

impl Group {
    fn name(self) -> &'static str {
        match self {
            Group::Room => t("settings.group.room", "The room"),
            Group::OnScreen => t("settings.group.on_screen", "On screen"),
            Group::Connect => t("settings.group.connect", "Connect"),
            Group::Advanced => t("settings.group.advanced", "Advanced"),
        }
    }
}

impl Pane {
    /// Every page, in the list's order.
    pub fn all() -> impl Iterator<Item = Pane> {
        GROUPS.into_iter().flat_map(|(_, panes)| panes.iter().copied())
    }

    /// Its name as the list and its page show it, in the language set.
    pub fn name(self) -> &'static str {
        let (key, english) = self.words();
        t(key, english)
    }

    fn words(self) -> (&'static str, &'static str) {
        match self {
            Pane::Screens => tk("settings.pane.screens", "Screens"),
            Pane::PhoneScreens => tk("settings.pane.mirror", "Phone Screens"),
            Pane::Slides => tk("settings.pane.slides", "Slides"),
            Pane::Groups => tk("settings.pane.groups", "Groups"),
            Pane::Language => tk("settings.pane.language", "Language"),
            Pane::Integrations => tk("settings.pane.integrations", "Integrations"),
            Pane::AiAssistants => tk("settings.pane.ai", "AI assistants"),
            Pane::PhoneRemote => tk("settings.pane.remote", "Phone remote"),
            Pane::Keys => tk("settings.pane.keys", "Keys"),
            Pane::CommandLine => tk("settings.pane.cli", "Command line"),
            Pane::SongFiles => tk("settings.pane.song_files", "Song files"),
            Pane::About => tk("settings.pane.about", "About"),
        }
    }

    /// The page a capture run asks for by name (`SOFTPRESENTER_CAPTURE_PREFERENCES`): its English
    /// name in any case, with - or _ for spaces, or a name it had before the regrouping.
    pub fn named(name: &str) -> Option<Pane> {
        let name = name.replace(['-', '_'], " ").to_lowercase();
        let before = match name.as_str() {
            "general" | "copyright" => Some(Pane::Slides),
            "key mappings" => Some(Pane::Keys),
            "remote control" | "remote" => Some(Pane::PhoneRemote),
            "mirror" | "phone" => Some(Pane::PhoneScreens),
            "mcp" | "ai" => Some(Pane::AiAssistants),
            "cli" => Some(Pane::CommandLine),
            _ => None,
        };
        before.or_else(|| Pane::all().find(|pane| pane.words().1.to_lowercase() == name))
    }

    fn icon(self) -> Icon {
        match self {
            Pane::Screens => Icon::Display,
            Pane::PhoneScreens => Icon::Phone,
            Pane::Slides => Icon::Slide,
            Pane::Groups => Icon::Dots,
            Pane::Language => Icon::Globe,
            Pane::Integrations => Icon::Link,
            Pane::AiAssistants => Icon::Sparkle,
            Pane::PhoneRemote => Icon::PhoneWave,
            Pane::Keys => Icon::Keyboard,
            Pane::CommandLine => Icon::Prompt,
            Pane::SongFiles => Icon::File,
            Pane::About => Icon::Info,
        }
    }
}

/// What a row of the list says at its end, so a problem shows before its page is opened: the
/// words, and whether its light is lit (something is on).
fn readout(pane: Pane, state: &HomeState, show: &Show, ui: &egui::Ui, room: f32) -> Option<(String, bool)> {
    let fits = |words: &str| spaced(ui, &words.to_uppercase(), fixed(11.0), TEXT, 0.44).size().x <= room;
    let off = || t("settings.state.off", "Off").to_owned();
    match pane {
        Pane::Screens => {
            let output_on = show.output_enabled();
            let on = state
                .screens
                .iter()
                .zip(&state.screen_status)
                .filter(|(screen, (_, shown))| {
                    *shown
                        && match screen.kind {
                            crate::screens::Kind::Audience => output_on,
                            crate::screens::Kind::Stage => state.stage_on,
                        }
                })
                .count();
            let words = tf(
                "settings.state.screens",
                "{on} of {all} on",
                &[("on", &crate::i18n::count(on as u64)), ("all", &crate::i18n::count(state.screens.len() as u64))],
            );
            Some((words, on > 0))
        }
        Pane::PhoneScreens => {
            Some((if state.mirror.on { t("settings.state.on", "On").to_owned() } else { off() }, state.mirror.on))
        }
        Pane::Slides => Some((crate::studio::transition_name(state.transition.kind).to_owned(), false)),
        Pane::Language => Some((
            match crate::i18n::choice() {
                crate::i18n::Choice::Computer => t("settings.state.auto", "Auto").to_owned(),
                crate::i18n::Choice::Is(language) => language.own_name().to_owned(),
            },
            false,
        )),
        Pane::Integrations => {
            let n = connected_services(state);
            Some((tn("settings.state.connected", n as u64, "{n} connected", "{n} connected", &[]), n > 0))
        }
        Pane::AiAssistants => {
            let switches = state.mcp.switches;
            let (key, english) = match (switches.allowed, switches.screens) {
                (false, _) => return Some((off(), false)),
                (true, false) => tk("settings.state.ai.prepare", "Can prepare"),
                (true, true) => tk("settings.state.ai.run", "Runs screens"),
            };
            let words = t(key, english);
            let short = crate::i18n::ts(key, english);
            Some((if fits(words) { words } else { short }.to_owned(), true))
        }
        Pane::PhoneRemote => Some(if state.remote.on {
            let n = state.remote.pages.len() as u64;
            (tn("settings.state.people", n, "{n} person", "{n} people", &[]), true)
        } else {
            (off(), false)
        }),
        _ => None,
    }
}

/// How many of the services in Integrations are connected (or, for the songbooks, have any).
fn connected_services(state: &HomeState) -> usize {
    let voxlive = matches!(state.voxlive.status, crate::voxlive::Status::Connected);
    let canva = matches!(state.canva.status, crate::canva::Status::Connected);
    let timelychurch =
        state.timelychurch.offered && matches!(state.timelychurch.status, crate::timelychurch::Status::Connected);
    let songbooks = crate::catalog::offered() && crate::songbooks::state_words(&state.songbooks).0;
    [voxlive, canva, timelychurch, songbooks].into_iter().filter(|on| *on).count()
}

/// The list's width, as the design's left column.
const LIST_WIDTH: f32 = crate::studio::LEFT_WIDTH;
/// A row of the list.
const ROW: f32 = 34.0;
/// The small capitals of the list's headings: their size and the room between their letters.
const CAPS: f32 = 11.5;
const CAPS_SPACING: f32 = 0.575;

/// The Settings window's contents: the list of pages down the left, in a panel of its own, and the
/// chosen page beside it, as the home window's columns are.
pub fn settings_ui(ui: &mut egui::Ui, state: &mut HomeState, show: &Show) {
    egui::CentralPanel::default().frame(Frame::new().fill(BG).inner_margin(GAP)).show(ui, |ui| {
        let whole = ui.max_rect();
        let list = Rect::from_min_size(whole.min, vec2(LIST_WIDTH, whole.height()));
        let page = Rect::from_min_max(pos2(list.max.x + GAP, whole.min.y), whole.max);
        ui.painter().rect_filled(list, RADIUS_PANEL, PANEL);
        ui.painter().rect_filled(page, RADIUS_PANEL, PANEL);
        let mut inside = ui.new_child(egui::UiBuilder::new().max_rect(list).layout(Layout::top_down(Align::Min)));
        inside.set_clip_rect(list);
        pane_list(&mut inside, state, show);
        let mut inside = ui.new_child(egui::UiBuilder::new().max_rect(page).layout(Layout::top_down(Align::Min)));
        inside.set_clip_rect(page);
        page_ui(&mut inside, state);
    });
}

/// The list: "Settings", the groups and their rows, and Set Up SoftPresenter under them.
fn pane_list(ui: &mut egui::Ui, state: &mut HomeState, show: &Show) {
    let panel = ui.max_rect();
    egui::ScrollArea::vertical().id_salt("settings list").auto_shrink([false, false]).show(ui, |ui| {
        let top = ui.cursor().min.y;
        paint_spaced(
            ui,
            pos2(panel.min.x + 18.0, top + 16.0),
            Align2::LEFT_TOP,
            &t("settings.title", "Settings").to_uppercase(),
            fixed_semibold(CAPS),
            TEXT_DIM,
            CAPS_SPACING,
        );
        ui.add_space(50.0);
        let left = panel.min.x + 10.0;
        let width = panel.width() - 20.0;
        for (at, (group, panes)) in GROUPS.into_iter().enumerate() {
            ui.add_space(if at == 0 { 4.0 } else { 14.0 });
            let (heading, _) = ui.allocate_exact_size(vec2(panel.width(), 21.0), Sense::hover());
            paint_spaced(
                ui,
                pos2(left + 10.0, heading.min.y),
                Align2::LEFT_TOP,
                &group.name().to_uppercase(),
                fixed_semibold(CAPS),
                TEXT_DIM,
                CAPS_SPACING,
            );
            for pane in panes.iter().copied() {
                let (place, _) = ui.allocate_exact_size(vec2(panel.width(), ROW), Sense::hover());
                let row = Rect::from_min_size(pos2(left, place.min.y), vec2(width, ROW));
                if list_row(ui, row, pane, state, show) {
                    state.preferences_pane = pane;
                }
                ui.add_space(1.0);
            }
        }
        // The setup a fresh install starts with, run again. It opens over the home window, so
        // Settings steps aside.
        ui.add_space(13.0);
        let font = FontId::proportional(12.5);
        let room = width - 20.0 - 26.0;
        let (key, english) = tk("settings.set_up", "Set Up SoftPresenter\u{2026}");
        let words = within(ui, key, english, font.clone(), 0.0, room, false);
        let wide = ui.painter().layout_no_wrap(words.clone(), font.clone(), TEXT).size().x + 26.0;
        let (place, _) = ui.allocate_exact_size(vec2(panel.width(), 30.0), Sense::hover());
        let button = Rect::from_min_size(pos2(left + 10.0, place.min.y), vec2(wide, 30.0));
        let hint = if crate::setup::System::THIS == crate::setup::System::Linux {
            t("settings.set_up.hint", "Run the first-start setup again: screens, songs, Bibles")
        } else {
            t(
                "settings.set_up.hint_permissions",
                "Run the first-start setup again: screens, songs, Bibles, permissions",
            )
        };
        if flat_button(ui, button, ui.scope_id().with("set up"), &words, font, Flat::Plain, true)
            .on_hover_text(hint)
            .clicked()
        {
            state.preferences_open = false;
            state.menu_asked = Some(crate::menubar::Action::SetUp);
        }
        ui.add_space(16.0);
    });
}

/// One row of the list: its icon, its name, and how its page stands (a light when something is
/// on, and a word or two in the fixed-width face). True when clicked.
fn list_row(ui: &mut egui::Ui, row: Rect, pane: Pane, state: &HomeState, show: &Show) -> bool {
    let chosen = state.preferences_pane == pane;
    let response = ui.interact(row, ui.scope_id().with(("settings pane", pane as u8)), Sense::click());
    if chosen {
        ui.painter().rect_filled(row, RADIUS, CARD);
    } else if response.hovered() {
        ui.painter().rect_filled(row, RADIUS, ROW_HOVER);
    }
    let ink = if chosen { TEXT } else { TEXT_DIM };
    let icon = Rect::from_center_size(pos2(row.min.x + 18.0, row.center().y), vec2(13.0, 13.0));
    icons::paint(ui.painter(), pane.icon(), icon, if chosen { TEXT } else { TEXT_FAINT });
    let name_at = row.min.x + 34.0;
    // The readout has the room the name leaves it, at most half the row.
    let mut right = row.max.x - 10.0;
    if let Some((words, lit)) = readout(pane, state, show, ui, row.width() / 2.0) {
        let font = if chosen { fixed_semibold(11.0) } else { fixed(11.0) };
        let drawn = paint_spaced(
            ui,
            pos2(right, row.center().y),
            Align2::RIGHT_CENTER,
            &words.to_uppercase(),
            font,
            TEXT_FAINT,
            0.44,
        );
        right = drawn.min.x - 8.0;
        if lit {
            paint_light(ui.painter(), pos2(right - 3.5, row.center().y), Some(READY));
            right -= 7.0 + 8.0;
        }
    }
    let font = if chosen { semibold(13.5) } else { FontId::proportional(13.5) };
    let clip = Rect::from_min_max(pos2(name_at, row.min.y), pos2(right, row.max.y));
    ui.painter().with_clip_rect(clip).text(pos2(name_at, row.center().y), Align2::LEFT_CENTER, pane.name(), font, ink);
    response.clicked()
}

/// The page: its name at the top, and what it holds under it, scrolling.
fn page_ui(ui: &mut egui::Ui, state: &mut HomeState) {
    let panel = ui.max_rect();
    let pane = state.preferences_pane;
    let title = spaced(ui, pane.name(), semibold(26.0), TEXT, -0.78);
    let title_height = title.size().y.max(35.0);
    ui.painter().galley(pos2(panel.min.x + 28.0, panel.min.y + 22.0), title, TEXT);
    let body = Rect::from_min_max(pos2(panel.min.x, panel.min.y + 22.0 + title_height + 14.0), panel.max);
    let mut inside = ui.new_child(egui::UiBuilder::new().max_rect(body).layout(Layout::top_down(Align::Min)));
    egui::ScrollArea::vertical().id_salt(("settings page", pane as u8)).auto_shrink([false, false]).show(
        &mut inside,
        |ui| {
            let width = (body.width() - 56.0).clamp(0.0, PAGE_WIDTH);
            ui.horizontal_top(|ui| {
                ui.add_space(28.0);
                ui.vertical(|ui| {
                    ui.set_width(width);
                    ui.set_max_width(width);
                    pane_contents(ui, state, pane);
                    ui.add_space(20.0);
                });
            });
        },
    );
}

/// The widest a page's cards grow, as the design's.
const PAGE_WIDTH: f32 = 860.0;

/// What a page holds.
fn pane_contents(ui: &mut egui::Ui, state: &mut HomeState, pane: Pane) {
    match pane {
        Pane::Screens => screens_page(ui, state),
        Pane::PhoneScreens => crate::mirror::pane(ui, &mut state.mirror),
        Pane::Slides => slides_page(ui, state),
        Pane::Groups => groups_page(ui),
        Pane::Language => language_page(ui),
        Pane::Integrations => {
            let songbooks = crate::catalog::offered().then_some(&mut state.songbooks);
            let timelychurch = state.timelychurch.offered.then_some(&mut state.timelychurch);
            crate::integrations::pane(ui, &mut state.voxlive, &mut state.canva, songbooks, timelychurch);
        }
        Pane::AiAssistants => crate::mcp::pane::card(ui, &mut state.mcp),
        Pane::PhoneRemote => crate::remote::pane::pane(ui, &mut state.remote),
        Pane::Keys => keys_page(ui, state),
        Pane::CommandLine => command_line_page(ui, state),
        Pane::SongFiles => song_files_page(ui, state),
        Pane::About => about_page(ui, state),
    }
}

/// Settings › Screens: Configure Screens, the stage screens' switch, and the check strip (mark
/// each screen with its name and colour, and a test pattern on every screen, to line up
/// projectors and cables).
fn screens_page(ui: &mut egui::Ui, state: &mut HomeState) {
    // Its key is the Mac menu bar's.
    let hint = match crate::keys::screens_key() {
        Some(key) => tf("settings.screens.configure.key", "Configure Screens ({key})", &[("key", &key)]),
        None => t("settings.screens.configure.hint", "Configure Screens").to_owned(),
    };
    if ui.button(t("settings.screens.configure", "Configure Screens…")).on_hover_text(hint).clicked() {
        state.settings_open = true;
    }
    ui.add_space(14.0);
    ui.checkbox(&mut state.stage_on, t("settings.screens.stage_on", "Stage screens on"));
    ui.add_space(14.0);
    crate::studio::caption(ui, &t("settings.screens.check", "Check the Screens").to_uppercase());
    ui.horizontal_wrapped(|ui| {
        let identify = ui.add(egui::Button::selectable(
            state.identify,
            if state.identify {
                t("settings.screens.identify.stop", "Stop identifying")
            } else {
                t("settings.screens.identify", "Identify")
            },
        ));
        if identify
            .on_hover_text(t(
                "settings.screens.identify.hint",
                "Each screen shows its name and colour. The room and the stream see it.",
            ))
            .clicked()
        {
            state.identify = !state.identify;
        }
    });
    ui.add_space(6.0);
    ui.horizontal_wrapped(|ui| {
        ui.label(t("settings.screens.pattern", "Test pattern"));
        for (pattern, label) in [
            (None, t("settings.screens.pattern.off", "Off")),
            (Some(crate::props::TestPattern::ColorBars), t("right.playing.bars", "Colour bars")),
            (Some(crate::props::TestPattern::Grid), t("right.playing.grid", "Grid")),
            (Some(crate::props::TestPattern::White), t("right.playing.white", "White")),
        ] {
            if ui.add(egui::Button::selectable(state.test_pattern == pattern, label)).clicked() {
                state.test_pattern = pattern;
            }
        }
    });
    ui.label(
        egui::RichText::new(t("settings.screens.seen", "Everyone watching sees these.")).size(11.5).color(TEXT_DIM),
    );
}

/// Settings › Slides: where a song's copyright line shows, with the church's CCLI licence.
fn slides_page(ui: &mut egui::Ui, state: &mut HomeState) {
    crate::studio::caption(ui, &t("settings.copyright", "Copyright").to_uppercase());
    ui.label(t("settings.copyright.about", "Show a song's copyright line on the audience screens:"));
    for choice in crate::service::CopyrightShow::ALL {
        use crate::service::CopyrightShow;
        let name = match choice {
            CopyrightShow::Never => t("settings.copyright.never", "Never"),
            CopyrightShow::FirstSlide => t("settings.copyright.first", "On a song's first slide"),
            CopyrightShow::LastSlide => t("settings.copyright.last", "On a song's last slide"),
            CopyrightShow::EverySlide => t("settings.copyright.every", "On every slide of a song"),
        };
        ui.radio_value(&mut state.copyright_show, choice, name);
    }
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.label(t("settings.copyright.licence", "CCLI Licence Number"));
        ui.add(egui::TextEdit::singleline(&mut state.ccli_licence).desired_width(160.0));
    });
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(t(
            "settings.copyright.details",
            "A song's writer, publisher and CCLI song number are set with the Info icon in its header.",
        ))
        .size(11.5)
        .color(TEXT_DIM),
    );
}

/// Settings › Groups: each group's colour, and the key that jumps to it.
fn groups_page(ui: &mut egui::Ui) {
    let about = t("settings.groups.about", "Each group's colour, and the key that jumps to it.");
    ui.label(egui::RichText::new(about).color(TEXT_DIM));
    ui.add_space(8.0);
    egui::Grid::new("groups").num_columns(3).spacing([16.0, 8.0]).show(ui, |ui| {
        for (key, names) in sp_model::GROUP_KEYS {
            let name = names.iter().find(|name| name.contains(' ')).or(names.first()).copied().unwrap_or("");
            let mut shown = name.to_owned();
            if let Some(first) = shown.get_mut(0..1) {
                first.make_ascii_uppercase();
            }
            let (swatch, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
            ui.painter().rect_filled(swatch, 4.0, super::color32(sp_model::group_color(name)));
            ui.label(shown);
            ui.label(egui::RichText::new(key.to_ascii_uppercase().to_string()).monospace().color(TEXT_DIM));
            ui.end_row();
        }
    });
}

/// Settings › Language: the words of SoftPresenter itself. The computer's language first (named in
/// that language), then each language written in itself. A choice shows at once, in this window
/// and every other.
fn language_page(ui: &mut egui::Ui) {
    crate::studio::caption(ui, &t("settings.language", "Language").to_uppercase());
    ui.label(t("settings.language.about", "The words of SoftPresenter itself"));
    super::language_menu(ui);
    ui.add_space(2.0);
    ui.label(
        egui::RichText::new(t("settings.language.detail", "Buttons, menus and messages. Changes at once."))
            .size(11.5)
            .color(TEXT_DIM),
    );
    ui.add_space(6.0);
    ui.label(
        egui::RichText::new(t(
            "settings.language.not_this",
            "The languages your songs and Bibles show in are set in Setup \u{203a} Languages; the Bible page has its own reading language.",
        ))
        .size(11.5)
        .color(TEXT_DIM),
    );
}

/// Settings › Keys: the switch for the keys on the buttons, and every key the list of keys (`?`)
/// has, from the same list so the two always agree; then the menus' shortcuts.
fn keys_page(ui: &mut egui::Ui, state: &mut HomeState) {
    let about = t("settings.keys.about", "The keys that run the show, and the menu shortcuts.");
    ui.label(egui::RichText::new(about).color(TEXT_DIM));
    ui.add_space(8.0);
    ui.checkbox(&mut state.keys_on_buttons, t("keys.show_on_buttons", "Show keys on buttons"));
    ui.add_space(8.0);
    egui::Grid::new("keys").num_columns(3).spacing([16.0, 6.0]).striped(true).show(ui, |ui| {
        for (group, rows) in crate::keys::every_key() {
            for (keys, what) in rows {
                ui.label(egui::RichText::new(crate::keys::group_title(group)).color(TEXT_DIM));
                ui.label(what);
                ui.label(egui::RichText::new(keys.join("  ")).monospace());
                ui.end_row();
            }
        }
        for (menu, command, key) in crate::menubar::shortcuts() {
            ui.label(egui::RichText::new(menu).color(TEXT_DIM));
            ui.label(command.trim_end_matches('…'));
            ui.label(egui::RichText::new(key).monospace());
            ui.end_row();
        }
    });
}

/// Settings › Command line: the `sp` command, which controls the running app from a terminal or a
/// script. It says where sp stands (read again each time the page comes into view, and after each
/// click) and offers what fits: Install, Use This Copy when sp opens another copy, Remove once it
/// is there; under the buttons, what the last click did, in place of where it stood.
fn command_line_page(ui: &mut egui::Ui, state: &mut HomeState) {
    use crate::cli_install::{Status, Tone};
    use crate::studio::{CARE, TALLY};
    ui.label(t("settings.cli.about", "The sp command controls SoftPresenter from a terminal or a script."));
    ui.add_space(4.0);
    let card = &mut state.command_line;
    // Not drawn in the pass before: the page has just come into view (chosen, or the window
    // opened again), so it reads afresh and forgets what it said.
    let pass = ui.ctx().cumulative_pass_nr();
    if card.drawn.is_none_or(|last| pass != last && pass != last + 1) {
        card.status = None;
        card.said = None;
    }
    card.drawn = Some(pass);
    let status = card.status.get_or_insert_with(crate::cli_install::status).clone();
    let (install, removable) = match &status {
        Status::Missing => (Some(t("settings.cli.install", "Install")), false),
        Status::Here { findable: true, .. } => (None, true),
        Status::Here { findable: false, .. } => (Some(t("settings.cli.install", "Install")), true),
        Status::Elsewhere { .. } | Status::Gone { .. } => (Some(t("settings.cli.use_this", "Use This Copy")), true),
    };
    let mut clicked_install = false;
    let mut clicked_remove = false;
    ui.horizontal(|ui| {
        if let Some(words) = install {
            let hint = t("settings.cli.install.hint", "Puts sp (and softpresenter) where a terminal finds it");
            clicked_install = ui.button(words).on_hover_text(hint).clicked();
        }
        if removable {
            let hint = t(
                "settings.cli.remove.hint",
                "Takes sp and softpresenter away again, with anything SoftPresenter added for them",
            );
            clicked_remove = ui
                .add(
                    egui::Button::new(egui::RichText::new(t("settings.cli.remove", "Remove")).color(TEXT_DIM))
                        .frame(false),
                )
                .on_hover_text(hint)
                .clicked();
        }
    });
    if clicked_install {
        card.said = Some(match crate::cli_install::install() {
            Ok(done) => (
                if done.add_folder { Tone::Care } else { Tone::Good },
                crate::cli_install::installed_words(&done, cfg!(windows)),
            ),
            Err(problem) => (Tone::Trouble, problem),
        });
        card.status = Some(crate::cli_install::status());
    }
    if clicked_remove {
        card.said = Some(match crate::cli_install::remove() {
            Ok(done) => (Tone::Plain, crate::cli_install::removed_words(&done)),
            Err(problem) => (Tone::Trouble, problem),
        });
        card.status = Some(crate::cli_install::status());
    }
    let (tone, words) = card.said.clone().unwrap_or_else(|| crate::cli_install::status_words(&status, cfg!(windows)));
    let colour = match tone {
        Tone::Good => READY,
        Tone::Plain => TEXT_FAINT,
        Tone::Care => CARE,
        Tone::Trouble => TALLY,
    };
    ui.add_space(2.0);
    // A line short enough to read at a glance, wrapped beside its dot.
    let room = (ui.available_width() - 14.0).min(640.0);
    ui.horizontal_top(|ui| {
        let (place, _) = ui.allocate_exact_size(vec2(8.0, 16.0), Sense::hover());
        ui.painter().circle_filled(place.center(), 3.5, colour);
        ui.vertical(|ui| {
            ui.set_max_width(room);
            ui.label(egui::RichText::new(words).size(12.0).color(TEXT_DIM));
        });
    });
}

/// Settings › Song files: songs are plain files that can be changed outside the app; read them
/// again.
fn song_files_page(ui: &mut egui::Ui, state: &mut HomeState) {
    ui.label(t("settings.song_files.about", "Songs are plain files that can be changed outside the app."));
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        if ui
            .button(t("settings.song_files.reload", "Reload songs"))
            .on_hover_text(t("settings.song_files.reload.hint", "Brings in songs and themes changed outside the app"))
            .clicked()
        {
            state.menu_asked = Some(crate::menubar::Action::ReloadSongs);
        }
        ui.label(egui::RichText::new(t("settings.song_files.again", "Read them again")).size(11.5).color(TEXT_DIM));
    });
}

/// Settings › About: the version, what's new, the guide, reporting a problem and the licences.
fn about_page(ui: &mut egui::Ui, state: &mut HomeState) {
    ui.label(
        egui::RichText::new(tf(
            "settings.about.version",
            "Version {version} \u{b7} free and open source, GPL-3.0-or-later",
            &[("version", env!("CARGO_PKG_VERSION"))],
        ))
        .color(TEXT_DIM),
    );
    ui.add_space(12.0);
    for (label, page) in [
        (t("settings.about.whats_new", "What\u{2019}s new"), Page::WhatsNew),
        (t("settings.about.guide", "Open the guide"), Page::Guide),
        (t("settings.about.report", "Report an issue\u{2026}"), Page::ReportIssue),
        (t("settings.about.licences", "Licences\u{2026}"), Page::Licences),
    ] {
        if ui.button(label).clicked() {
            open_page(state, page);
        }
        ui.add_space(4.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_page_is_in_one_group_and_can_be_asked_for_by_name() {
        let all: Vec<Pane> = Pane::all().collect();
        assert_eq!(all.len(), 12);
        for (at, pane) in all.iter().enumerate() {
            assert!(!all[at + 1..].contains(pane), "{pane:?} is listed twice");
            assert_eq!(Pane::named(pane.words().1), Some(*pane));
        }
        // The names a capture run (or a script) used before the regrouping still find a page.
        assert_eq!(Pane::named("general"), Some(Pane::Slides));
        assert_eq!(Pane::named("key-mappings"), Some(Pane::Keys));
        assert_eq!(Pane::named("Remote Control"), Some(Pane::PhoneRemote));
        assert_eq!(Pane::named("phone_screens"), Some(Pane::PhoneScreens));
        assert_eq!(Pane::named("mcp"), Some(Pane::AiAssistants));
        assert_eq!(Pane::named("nothing"), None);
        assert_eq!(Pane::default(), Pane::Screens, "Settings opens on Screens, as the design's does");
    }

    #[test]
    fn a_row_says_how_its_page_stands() {
        let ctx = crate::studio::tests::styled();
        let show = Show::new(Vec::new());
        let mut state = HomeState::default();
        state.mcp.switches = crate::mcp::Switches::default();
        state.remote.on = false;
        let said = |state: &HomeState, pane: Pane| {
            let mut out = None;
            let drawn = crate::studio::tests::drawn(&ctx, vec![], |ui| out = readout(pane, state, &show, ui, 120.0));
            drawn.drop_without_applying_deltas();
            out
        };
        assert_eq!(said(&state, Pane::AiAssistants), Some(("Off".to_owned(), false)));
        state.mcp.switches.allowed = true;
        assert_eq!(said(&state, Pane::AiAssistants), Some(("Can prepare".to_owned(), true)));
        state.mcp.switches.screens = true;
        assert_eq!(said(&state, Pane::AiAssistants), Some(("Runs screens".to_owned(), true)));
        assert_eq!(said(&state, Pane::PhoneRemote), Some(("Off".to_owned(), false)));
        state.remote.on = true;
        assert_eq!(said(&state, Pane::PhoneRemote), Some(("0 people".to_owned(), true)));
        assert_eq!(said(&state, Pane::Integrations), Some(("0 connected".to_owned(), false)));
        assert_eq!(said(&state, Pane::Keys), None, "Advanced's pages say nothing at the end of their rows");
    }
}
