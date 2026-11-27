//! The Settings window (Cmd+,), regrouped as the design has it (25 November): its pages in four
//! groups down the left (The room, On screen, Connect, Advanced), each row ending in how its page
//! stands, and the page chosen beside them. Every page shows the thing first, one main control
//! and a line of words, with the rest under More.

use egui::{Align, Align2, FontId, Frame, Layout, Rect, Sense, Stroke, StrokeKind, pos2, vec2};
use sp_engine::Show;

use super::{HomeState, Page, open_page};

mod find_card;
pub(crate) mod keys_card;
pub(crate) mod parts;
mod screens_page;
use crate::i18n::{t, tf, tk, tn};
use crate::icons::{self, Icon};
use crate::studio::{
    BG, CARD, Flat, GAP, PANEL, RADIUS, RADIUS_PANEL, READY, ROW_HOVER, TEXT, TEXT_DIM, TEXT_FAINT, fixed,
    fixed_semibold, flat_button, paint_light, paint_spaced, semibold, spaced, within,
};
use parts::{Pick, Picture, card, note, on_off, on_off_size, picks, row, row_unlined};

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
            Pane::PhoneScreens => tk("settings.pane.mirror", "AirPlay"),
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
            // Phone remote with Port and certificate folded out (`remote::pane`).
            "remote more" => Some(Pane::PhoneRemote),
            // Slides with the window of the main theme's end mark open (`settings_ui`).
            "end look" => Some(Pane::Slides),
            // Keys scrolled on to Always the same (`keys_page`).
            "keys fixed" => Some(Pane::Keys),
            // AirPlay was called Phone Screens until 26 November.
            "mirror" | "phone" | "phone screens" => Some(Pane::PhoneScreens),
            "mcp" | "ai" => Some(Pane::AiAssistants),
            "cli" => Some(Pane::CommandLine),
            // About, with the anonymous reports' See what's sent opened.
            "reports sent" | "reports" => Some(Pane::About),
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

/// What a row of the list says at its end, so a problem shows before its page is opened.
#[derive(Clone, Debug, PartialEq)]
struct Readout {
    words: String,
    /// The same, shorter, for a row whose name leaves too little room.
    short: String,
    /// Something is on: the row's light is lit.
    lit: bool,
}

impl Readout {
    fn of(words: impl Into<String>, lit: bool) -> Readout {
        let words = words.into();
        Readout { short: words.clone(), words, lit }
    }
}

/// What a page's row says at its end; Advanced's pages say nothing.
fn readout(pane: Pane, state: &HomeState, show: &Show) -> Option<Readout> {
    let off = || Readout::of(t("settings.off", "Off"), false);
    match pane {
        Pane::Screens => {
            let (on, all) = screens_on(state, show);
            let counts = [("on", crate::i18n::count(on as u64)), ("all", crate::i18n::count(all as u64))];
            let counts: Vec<(&str, &str)> = counts.iter().map(|(name, value)| (*name, value.as_str())).collect();
            Some(Readout {
                words: tf("settings.state.screens", "{on} of {all} on", &counts),
                short: tf("settings.state.screens.short", "{on}/{all} on", &counts),
                lit: on > 0,
            })
        }
        Pane::PhoneScreens if state.mirror.on => Some(Readout::of(t("settings.on", "On"), true)),
        Pane::PhoneScreens => Some(off()),
        Pane::Slides => Some(Readout::of(crate::studio::transition_name(state.transition.kind), false)),
        Pane::Language => Some(Readout::of(
            match crate::i18n::choice() {
                crate::i18n::Choice::Computer => t("settings.state.auto", "Auto"),
                crate::i18n::Choice::Is(language) => language.own_name(),
            },
            false,
        )),
        Pane::Integrations => {
            let n = connected_services(state) as u64;
            Some(Readout {
                words: tn("settings.state.connected", n, "{n} connected", "{n} connected", &[]),
                short: tn("settings.state.connected.short", n, "{n} on", "{n} on", &[]),
                lit: n > 0,
            })
        }
        Pane::AiAssistants => {
            let switches = state.mcp.switches;
            let (key, english) = match (switches.allowed, switches.screens) {
                (false, _) => return Some(off()),
                (true, false) => tk("settings.state.ai.prepare", "Can prepare"),
                (true, true) => tk("settings.state.ai.run", "Runs screens"),
            };
            Some(Readout {
                words: t(key, english).to_owned(),
                short: crate::i18n::ts(key, english).to_owned(),
                lit: true,
            })
        }
        Pane::PhoneRemote if state.remote.on => {
            let n = state.remote.pages.len() as u64;
            Some(Readout {
                words: tn("settings.state.people", n, "{n} person", "{n} people", &[]),
                short: tn("settings.state.people.short", n, "{n} on", "{n} on", &[]),
                lit: true,
            })
        }
        Pane::PhoneRemote => Some(off()),
        _ => None,
    }
}

/// How many screens are going out (shown somewhere, with their kind switched on), and how many
/// there are.
fn screens_on(state: &HomeState, show: &Show) -> (usize, usize) {
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
    (on, state.screens.len())
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
        page_ui(&mut inside, state, show);
    });
    // A theme's end mark, opened from Slides: over this window while that page is in view.
    if state.preferences_pane == Pane::Slides {
        // Testing aid: SOFTPRESENTER_CAPTURE_PREFERENCES=end-look opens Slides with the window
        // of the main theme's end mark open, to see it in a capture.
        static OPENED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if std::env::var("SOFTPRESENTER_CAPTURE_PREFERENCES").as_deref() == Ok("end-look")
            && !OPENED.swap(true, std::sync::atomic::Ordering::Relaxed)
        {
            crate::end_look::open(state, state.theme_for_songs, crate::end_look::Door::Settings);
        }
        crate::end_look::window(ui.ctx(), state, show, crate::end_look::Door::Settings);
    } else {
        crate::end_look::settings_closed(state);
    }
    // The What's new window, over the whole of Settings, from About.
    crate::whats_new::window(ui.ctx(), &mut state.whats_new);
}

/// The list: "Settings", the groups and their rows, and Set up SoftPresenter under them.
fn pane_list(ui: &mut egui::Ui, state: &mut HomeState, show: &Show) {
    let panel = ui.max_rect();
    egui::ScrollArea::vertical().id_salt("settings list").auto_shrink([false, false]).show(ui, |ui| {
        // Every room in the list is the design's own, with nothing added between.
        ui.spacing_mut().item_spacing.y = 0.0;
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
        let (key, english) = tk("settings.set_up", "Set up SoftPresenter\u{2026}");
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
    let font = if chosen { semibold(13.5) } else { FontId::proportional(13.5) };
    let name_wide = ui.painter().layout_no_wrap(pane.name().to_owned(), font.clone(), ink).size().x;
    // The readout has the room the name leaves it, in its short form when the usual is too long.
    let mut right = row.max.x - 10.0;
    if let Some(Readout { words, short, lit }) = readout(pane, state, show) {
        let caps = if chosen { fixed_semibold(11.0) } else { fixed(11.0) };
        let room = right - name_at - name_wide - 8.0 - if lit { 15.0 } else { 0.0 };
        let fits = |words: &str| spaced(ui, &words.to_uppercase(), caps.clone(), TEXT_FAINT, 0.44).size().x <= room;
        // Where not even the short form fits beside the name, the name is whole and a lit row
        // keeps only its light.
        let words = if fits(&words) { Some(words) } else { (fits(&short) || !lit).then_some(short) };
        if let Some(words) = words {
            let drawn = paint_spaced(
                ui,
                pos2(right, row.center().y),
                Align2::RIGHT_CENTER,
                &words.to_uppercase(),
                caps,
                TEXT_FAINT,
                0.44,
            );
            right = drawn.min.x - 8.0;
        }
        if lit {
            paint_light(ui.painter(), pos2(right - 3.5, row.center().y), Some(READY));
            right -= 7.0 + 8.0;
        }
    }
    let clip = Rect::from_min_max(pos2(name_at, row.min.y), pos2(right, row.max.y));
    ui.painter().with_clip_rect(clip).text(pos2(name_at, row.center().y), Align2::LEFT_CENTER, pane.name(), font, ink);
    response.clicked()
}

/// The page: its name at the top, and what it holds under it, scrolling.
fn page_ui(ui: &mut egui::Ui, state: &mut HomeState, show: &Show) {
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
                    pane_contents(ui, state, show, pane);
                    ui.add_space(20.0);
                });
            });
        },
    );
}

/// The widest a page's cards grow, as the design's.
const PAGE_WIDTH: f32 = 860.0;

/// What a page holds.
fn pane_contents(ui: &mut egui::Ui, state: &mut HomeState, show: &Show, pane: Pane) {
    match pane {
        Pane::Screens => screens_page::page(ui, state, show),
        Pane::PhoneScreens => crate::mirror::pane(ui, &mut state.mirror),
        Pane::Slides => slides_page(ui, state, show),
        Pane::Groups => groups_page(ui, state, show),
        Pane::Language => language_page(ui),
        Pane::Integrations => {
            let songbooks = crate::catalog::offered().then_some(&mut state.songbooks);
            crate::integrations::pane(ui, &mut state.voxlive, &mut state.canva, songbooks, &mut state.timelychurch);
        }
        Pane::AiAssistants => crate::mcp::pane::page(ui, &mut state.mcp),
        Pane::PhoneRemote => crate::remote::pane::pane(ui, &mut state.remote),
        Pane::Keys => keys_page(ui, state),
        Pane::CommandLine => command_line_page(ui, state),
        Pane::SongFiles => song_files_page(ui, state),
        Pane::About => about_page(ui, state),
    }
}

/// Settings › Slides: what the room sees with every song, shown. The copyright line drawn on a
/// small slide, with where it falls as four keys and the church's CCLI licence beside them; the
/// end of a song's mark on a small slide, with its switch; how one
/// slide gives way to the next, the first three kinds as keys with a picture and More… for every
/// kind, then Carry on to the next item; and how a song's slides are laid out in the window.
fn slides_page(ui: &mut egui::Ui, state: &mut HomeState, show: &Show) {
    card(
        ui,
        t("settings.copyright.title", "Copyright line"),
        t("settings.copyright.about", "Where the small line under a song\u{2019}s words shows"),
        None,
        |ui| copyright(ui, state, show),
    );
    card(
        ui,
        t("settings.slides.end.title", "End of a song"),
        t("settings.slides.end.caption", "What the room sees under a song\u{2019}s last slide"),
        None,
        |ui| end_of_a_song(ui, state, show),
    );
    card(
        ui,
        t("settings.transition.title", "Between slides"),
        t("settings.transition.about", "How one slide gives way to the next"),
        Some(t("settings.transition.own", "A song or a slide can have its own")),
        |ui| {
            transitions(ui, state);
            let size = on_off_size(ui);
            row(
                ui,
                t("settings.continuous", "Carry on to the next item"),
                t("settings.continuous.about", "Next at the end of a song opens the one after it"),
                size,
                |ui, place| {
                    if on_off(ui, place, ui.scope_id().with("continuous"), state.continuous).clicked() {
                        state.continuous = !state.continuous;
                    }
                },
            );
        },
    );
    if std::mem::take(&mut state.show_interface_size) {
        ui.scroll_to_cursor(Some(Align::TOP));
    }
    card(
        ui,
        t("settings.window.title", "In your window"),
        t("settings.window.about", "How your windows are laid out for you. The room never sees this"),
        None,
        |ui| {
            use crate::home::SlideView;
            let look = crate::studio::SwitchLook {
                font: FontId::proportional(12.0),
                spacing: 0.0,
                pad: 11.0,
                height: 24.0,
                inset: 3.0,
                track: PANEL,
                idle: TEXT_DIM,
            };
            let labels = [
                t("settings.window.grid", "Grid"),
                t("settings.window.words", "Words only"),
                t("settings.window.table", "Table"),
            ];
            let views = [SlideView::Grid, SlideView::Easy, SlideView::Table];
            let chosen = views.iter().position(|view| *view == state.slide_view);
            // Measured first, then drawn at the row's end.
            let size = vec2(crate::studio::pill_switch_width(ui, &labels, &look), look.height + 2.0 * look.inset);
            row(ui, t("settings.window.shown_as", "Slides are shown as"), "", size, |ui, place| {
                let (_, clicked) = crate::studio::pill_switch(ui, "slide view", place.min, &labels, &[], chosen, &look);
                if let Some(at) = clicked {
                    state.slide_view = views[at];
                }
            });
            interface_size_row(ui, state, &look);
        },
    );
}

/// The Interface size row: Automatic, 100%, 125%, 150% or 175%, and what the windows are drawn
/// at now (which a fixed size the main window has no room for is not). The switch goes under
/// the words when the page is too narrow for both.
fn interface_size_row(ui: &mut egui::Ui, state: &mut HomeState, look: &crate::studio::SwitchLook) {
    use crate::interface_size::InterfaceSize;
    let labels: Vec<String> = InterfaceSize::ALL
        .iter()
        .map(|size| match size {
            InterfaceSize::Automatic => t("settings.size.automatic", "Automatic").to_owned(),
            InterfaceSize::Percent(n) => crate::i18n::percent(u64::from(*n)),
        })
        .collect();
    let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
    let chosen = InterfaceSize::ALL.iter().position(|size| *size == state.interface_size);
    let now = crate::i18n::percent((state.zoom_now * 100.0).round() as u64);
    let short = match state.interface_size {
        InterfaceSize::Percent(n) => state.zoom_now * 100.0 + 0.5 < f32::from(n),
        InterfaceSize::Automatic => false,
    };
    let note = if short {
        tf("settings.size.no_room", "Now {size}, as large as the main window has room for", &[("size", &now)])
    } else {
        tf("settings.size.about", "How large the buttons and words are in your windows. Now {size}", &[("size", &now)])
    };
    let size = vec2(crate::studio::pill_switch_width(ui, &labels, look), look.height + 2.0 * look.inset);
    let title = t("settings.size", "Interface size");
    let mut clicked = None;
    if ui.available_width() - size.x - 16.0 >= 220.0 {
        row(ui, title, &note, size, |ui, place| {
            clicked = crate::studio::pill_switch(ui, "interface size", place.min, &labels, &[], chosen, look).1;
        });
    } else {
        row(ui, title, &note, egui::Vec2::ZERO, |_, _| {});
        let (place, _) = ui.allocate_exact_size(vec2(ui.available_width(), size.y + 12.0), Sense::hover());
        clicked = crate::studio::pill_switch(ui, "interface size", place.min, &labels, &[], chosen, look).1;
    }
    if let Some(at) = clicked {
        state.interface_size = InterfaceSize::ALL[at];
    }
}

/// The Copyright line card's insides: a slide with the line where it shows, beside the four keys
/// and the licence (under them when the page is narrow).
fn copyright(ui: &mut egui::Ui, state: &mut HomeState, show: &Show) {
    use crate::service::CopyrightShow as C;
    let width = ui.available_width();
    let choices = [C::FirstSlide, C::LastSlide, C::EverySlide, C::Never];
    let keys = [
        (t("settings.copyright.first", "First slide"), [true, false, false]),
        (t("settings.copyright.last", "Last slide"), [false, false, true]),
        (t("settings.copyright.every", "Every slide"), [true, true, true]),
        (t("settings.copyright.never", "Nowhere"), [false, false, false]),
    ]
    .map(|(name, marked)| Pick { picture: Picture::Strip(marked), name: name.to_owned(), line: String::new() });
    // The keys beside the slide while all four fit there with no word broken, else under it.
    let beside = width - 340.0 >= parts::row_width(ui, &keys).max(320.0);
    let slide = vec2(320.0f32.min(width), 320.0f32.min(width) * 9.0 / 16.0);
    let shown = |ui: &mut egui::Ui, state: &mut HomeState| {
        let chosen = choices.iter().position(|choice| *choice == state.copyright_show);
        if let (Some(at), _) = picks(ui, "copyright", &keys, chosen, 76.0) {
            state.copyright_show = choices[at];
        }
        licence(ui, state);
    };
    ui.add_space(4.0);
    if beside {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 20.0;
            sample_slide(ui, state, show, slide);
            ui.vertical(|ui| {
                ui.set_width(ui.available_width());
                shown(ui, state);
            });
        });
    } else {
        sample_slide(ui, state, show, slide);
        // 20 under the slide, as beside it; the keys bring 4 of their own.
        ui.add_space(16.0);
        shown(ui, state);
    }
    ui.add_space(10.0);
}

/// "Your church's CCLI licence" and its field, in the fixed-width face.
fn licence(ui: &mut egui::Ui, state: &mut HomeState) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 12.0;
        ui.label(
            egui::RichText::new(t("settings.copyright.licence", "Your church\u{2019}s CCLI licence")).color(TEXT_DIM),
        );
        let (place, _) = ui.allocate_exact_size(vec2(130.0, 32.0), Sense::hover());
        ui.painter().rect_filled(place, RADIUS, PANEL);
        let field = ui.put(
            place.shrink2(vec2(10.0, 7.0)),
            egui::TextEdit::singleline(&mut state.ccli_licence)
                .frame(egui::Frame::NONE)
                .font(fixed(13.0))
                .text_color(TEXT)
                .margin(egui::Margin::ZERO),
        );
        if field.has_focus() {
            ui.painter().rect_stroke(place, RADIUS, Stroke::new(1.5, TEXT_FAINT), StrokeKind::Inside);
        }
    });
}

/// The small slide the copyright line is drawn on: the first song with its details filled in, its
/// first two lines, and its line at the foot as the screens show it (unless it shows nowhere).
fn sample_slide(ui: &mut egui::Ui, state: &HomeState, show: &Show, size: egui::Vec2) {
    let (place, response) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter().with_clip_rect(place);
    painter.rect_filled(place, RADIUS, egui::Color32::BLACK);
    let song = show.presentations().iter().find(|song| !song.copyright.is_empty()).or(show.presentations().first());
    let words: Vec<String> = song
        .and_then(|song| song.slides().next())
        .and_then(|(_, slide)| slide.texts().into_iter().next())
        .map(|(_, words)| words.lines().filter(|line| !line.trim().is_empty()).take(2).map(str::to_owned).collect())
        .unwrap_or_default();
    let lines: Vec<_> = words
        .iter()
        .map(|line| parts::one_line(ui, line, crate::studio::reading(15.0), TEXT, place.width() - 28.0))
        .collect();
    let height: f32 = lines.iter().map(|line| line.size().y).sum::<f32>() + 2.0 * lines.len().saturating_sub(1) as f32;
    let mut y = place.center().y - 6.0 - height / 2.0;
    for line in lines {
        let line_height = line.size().y;
        painter.galley(pos2(place.center().x - line.size().x / 2.0, y), line, TEXT);
        y += line_height + 2.0;
    }
    if state.copyright_show != crate::service::CopyrightShow::Never {
        let line = song
            .filter(|song| !song.copyright.is_empty())
            .map(|song| song.copyright.line(&song.title, state.ccli_licence.trim()))
            .unwrap_or_else(|| {
                t("settings.copyright.empty", "Fill in a song\u{2019}s details to see its line").to_owned()
            });
        let galley = parts::one_line(ui, &line, FontId::proportional(8.0), TEXT, place.width() - 20.0);
        let band = Rect::from_min_max(
            pos2(place.min.x + 10.0, place.max.y - 8.0 - galley.size().y - 4.0),
            pos2(place.max.x - 10.0, place.max.y - 8.0),
        );
        painter.rect_stroke(band, 4.0, Stroke::new(1.0, TEXT_FAINT), StrokeKind::Outside);
        painter.galley(pos2(band.center().x - galley.size().x / 2.0, band.min.y + 2.0), galley, TEXT);
    }
    response.on_hover_text(t(
        "settings.copyright.details",
        "A song's writer, publisher and CCLI song number are set with the Info icon in its header.",
    ));
}

/// The End of a song card's insides: a small slide with a song's last words and the main
/// theme's mark, beside the switch and what it does (under the slide when the page is narrow);
/// and under the switch the key that opens the mark's look, which is the theme's (`end_look`).
fn end_of_a_song(ui: &mut egui::Ui, state: &mut HomeState, show: &Show) {
    let width = ui.available_width();
    // A smaller slide than the copyright line's: two lines and the mark need no more, and the
    // page stays short.
    let slide = vec2(192.0, 108.0);
    let sample = |ui: &mut egui::Ui, state: &HomeState| {
        let (place, _) = ui.allocate_exact_size(slide, Sense::hover());
        let look = crate::end_look::look_of(state, state.theme_for_songs);
        crate::end_look::sample(ui, place, show, &look, state.end_mark);
    };
    let switch = |ui: &mut egui::Ui, state: &mut HomeState| {
        let size = on_off_size(ui);
        row_unlined(
            ui,
            t("settings.slides.end.line", "Mark the end of a song"),
            t(
                "settings.slides.end.note",
                "A mark under the words of a song\u{2019}s last slide as sung, on the audience screens",
            ),
            size,
            |ui, place| {
                if on_off(ui, place, ui.scope_id().with("end mark"), state.end_mark).clicked() {
                    state.end_mark = !state.end_mark;
                }
            },
        );
        // Change how it looks…, and whose the look is: beside the key where there is room for
        // the sentence on one line there, else under it.
        let words = t("settings.slides.end.look", "Change how it looks\u{2026}");
        let main = state.theme_for_songs;
        let name = match (state.theme_ids.get(main), state.theme_names.get(main)) {
            (Some(id), Some(name)) => crate::theme_store::theme_name(*id, name).into_owned(),
            _ => String::new(),
        };
        let whose = tf(
            "settings.slides.end.look.note",
            "Its mark, size, colour and place belong to the theme ({theme}).",
            &[("theme", &name)],
        );
        let key = parts::key_size(ui, words);
        let room = ui.available_width();
        let font = FontId::proportional(12.5);
        let beside = crate::studio::widest(ui, &[whose.as_str()], &font) <= room - key.x - 10.0;
        let note_room = if beside { room - key.x - 10.0 } else { room };
        let note = parts::lines(ui, &whose, font, TEXT_FAINT, note_room, 18.0);
        let tall = if beside { key.y.max(note.size().y) } else { key.y + 6.0 + note.size().y };
        let (place, _) = ui.allocate_exact_size(vec2(room, tall), Sense::hover());
        let at = Rect::from_min_size(place.min, key);
        if parts::key(ui, at, ui.scope_id().with("end look"), words, false).clicked() {
            crate::end_look::open(state, main, crate::end_look::Door::Settings);
        }
        let note_at = if beside {
            pos2(at.max.x + 10.0, at.center().y - note.size().y / 2.0)
        } else {
            pos2(place.min.x, at.max.y + 6.0)
        };
        ui.painter().galley(note_at, note, TEXT_FAINT);
    };
    ui.add_space(4.0);
    if width - 212.0 >= 320.0 {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 20.0;
            sample(ui, state);
            ui.vertical(|ui| {
                ui.set_width(ui.available_width());
                switch(ui, state);
            });
        });
    } else {
        sample(ui, state);
        ui.add_space(8.0);
        switch(ui, state);
    }
    ui.add_space(10.0);
}

/// The Between slides keys: Cut, Dissolve and Fade, each with a picture of how it looks, and More…
/// for every other kind, its time and direction (the dock's sheet).
fn transitions(ui: &mut egui::Ui, state: &mut HomeState) {
    use sp_model::TransitionKind as K;
    let now = state.transition;
    let three = [K::Cut, K::Dissolve, K::Fade];
    let other = !three.contains(&now.kind);
    let time = |look: sp_model::Transition| crate::i18n::seconds(f64::from(look.seconds));
    let mut keys: Vec<Pick> = three
        .iter()
        .map(|kind| Pick {
            picture: Picture::Transition(Some(*kind)),
            name: crate::studio::transition_name(*kind).to_owned(),
            line: if *kind == K::Cut {
                t("settings.transition.cut", "At once").to_owned()
            } else if *kind == now.kind {
                time(now)
            } else {
                String::new()
            },
        })
        .collect();
    keys.push(if other {
        Pick {
            picture: Picture::Transition(None),
            name: crate::studio::transition_name(now.kind).to_owned(),
            line: time(now),
        }
    } else {
        Pick {
            picture: Picture::Transition(None),
            name: t("settings.transition.more", "More\u{2026}").to_owned(),
            line: t("settings.transition.more.about", "Every kind, its time and direction").to_owned(),
        }
    });
    let chosen = three.iter().position(|kind| *kind == now.kind).or(Some(3));
    let (clicked, responses) = picks(ui, "transition", &keys, chosen, 76.0);
    if let Some(at) = clicked.filter(|at| *at < 3)
        && three[at] != now.kind
    {
        // A newly chosen kind keeps the direction and takes its usual time, as the sheet does.
        state.transition = sp_model::Transition { direction: now.direction, ..sp_model::Transition::of(three[at]) };
    }
    if let Some(more) = responses.get(3) {
        egui::Popup::from_toggle_button_response(more)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| crate::studio::transition_picker(ui, &mut state.transition, "settings"));
    }
}

/// Settings › Language: the words of SoftPresenter itself. The computer's language first (named in
/// that language), then each language written in itself. A choice shows at once, in this window
/// and every other.
fn language_page(ui: &mut egui::Ui) {
    use crate::i18n::Choice;
    card(
        ui,
        t("settings.language", "Language"),
        t("settings.language.about", "The words of SoftPresenter itself: buttons, menus and messages. Changes at once"),
        None,
        |ui| {
            let keys: Vec<Pick> = Choice::ALL
                .iter()
                .map(|choice| match choice {
                    Choice::Computer => Pick {
                        picture: Picture::Computer,
                        name: t("settings.language.this_computer", "This computer\u{2019}s").to_owned(),
                        line: crate::i18n::computer().unwrap_or(crate::i18n::Language::English).own_name().to_owned(),
                    },
                    Choice::Is(language) => Pick {
                        picture: Picture::Flag(*language),
                        name: language.own_name().to_owned(),
                        line: String::new(),
                    },
                })
                .collect();
            let now = crate::i18n::choice();
            let chosen = Choice::ALL.iter().position(|choice| *choice == now);
            if let (Some(at), _) = picks(ui, "language", &keys, chosen, 76.0) {
                crate::i18n::set_choice(Choice::ALL[at]);
            }
            note(
                ui,
                t(
                    "settings.language.not_this",
                    "Songs and Bibles keep their own languages: Set up SoftPresenter \u{203a} Languages. The Bible page has its own reading language (\u{410}\u{430}).",
                ),
            );
        },
    );
}

/// Settings › Keys, top to bottom: the switch for the keys on the buttons; Running the show,
/// what each key that runs the show does (`keys_card`); and Always the same, a line for each
/// key that can't be changed, from the list the list of keys (`?`) is made from, and for each
/// of the menus' shortcuts after them.
fn keys_page(ui: &mut egui::Ui, state: &mut HomeState) {
    card(
        ui,
        t("settings.keys.buttons.title", "On the buttons"),
        t("settings.keys.buttons.about", "Key tags"),
        None,
        |ui| {
            let size = on_off_size(ui);
            let label = t("keys.show_on_buttons", "Show keys on buttons");
            let about = t("settings.keys.buttons.note", "A small tag beside each button with its key");
            row(ui, label, about, size, |ui, place| {
                if on_off(ui, place, ui.scope_id().with("keys on buttons"), state.keys_on_buttons).clicked() {
                    state.keys_on_buttons = !state.keys_on_buttons;
                }
            });
        },
    );
    keys_card::keys_card(ui, state);
    // What each line says, and its keys: the fixed keys as the list of keys has them, then the
    // menus' shortcuts.
    let mut lines: Vec<(String, Vec<String>)> = Vec::new();
    for (_, rows) in crate::keys::fixed_keys() {
        lines.extend(rows.into_iter().map(|(keys, what)| (what, keys)));
    }
    for (_, command, key) in crate::menubar::shortcuts() {
        lines.push((command.trim_end_matches('…').to_owned(), vec![key]));
    }
    // Testing aid: SOFTPRESENTER_CAPTURE_PREFERENCES=keys-fixed opens Keys scrolled on to Always
    // the same, which is under the table of keys, to see it in a capture.
    if std::env::var("SOFTPRESENTER_CAPTURE_PREFERENCES").as_deref() == Ok("keys-fixed") {
        ui.scroll_to_cursor(Some(egui::Align::TOP));
    }
    // Every key opens the list of keys (`?`), in the operator's window.
    let every = t("settings.keys.fixed.every", "Every key");
    if parts::card_with_key(
        ui,
        t("settings.keys.fixed.title", "Always the same"),
        t("settings.keys.fixed.about", "Fixed keys \u{b7} press ? anywhere for the whole list"),
        Some(every),
        |ui| {
            for (what, keys) in &lines {
                fixed_line(ui, what, keys);
            }
            ui.add_space(6.0);
        },
    ) {
        state.key_sheet = true;
    }
}

/// A line of Always the same, as the design draws it: a line across its top, what the keys do
/// at the left, and each key as a cap of its own at the right, the last ending at the card's
/// edge. Keys that would take more than their share of the width go on to another line of
/// caps, and what they do wraps in the room they leave; a cap's own words never break.
fn fixed_line(ui: &mut egui::Ui, what: &str, keys: &[String]) {
    const CAP: f32 = 24.0;
    const BETWEEN: f32 = 6.0;
    let width = ui.available_width();
    let caps: Vec<std::sync::Arc<egui::Galley>> =
        keys.iter().map(|key| ui.painter().layout_no_wrap(key.clone(), fixed_semibold(11.0), TEXT)).collect();
    let wide = |cap: &egui::Galley| (cap.size().x + 14.0).max(26.0);
    // The caps in lines of at most their share of the width, a line taking as many as fit.
    let share = (width * 0.45).max(150.0);
    let mut cap_lines: Vec<(Vec<usize>, f32)> = Vec::new();
    for (at, cap) in caps.iter().enumerate() {
        let cap_wide = wide(cap);
        match cap_lines.last_mut() {
            Some((line, taken)) if *taken + BETWEEN + cap_wide <= share => {
                line.push(at);
                *taken += BETWEEN + cap_wide;
            }
            _ => cap_lines.push((vec![at], cap_wide)),
        }
    }
    let caps_wide = cap_lines.iter().map(|(_, taken)| *taken).fold(0.0, f32::max);
    let caps_tall = cap_lines.len() as f32 * CAP + cap_lines.len().saturating_sub(1) as f32 * 4.0;
    let label = parts::lines(ui, what, semibold(13.0), TEXT, (width - caps_wide - 16.0).max(80.0), 18.0);
    let inner = label.size().y.max(caps_tall);
    let (place, _) = ui.allocate_exact_size(vec2(width, inner + 24.0), Sense::hover());
    let painter = ui.painter();
    painter.hline(place.x_range(), place.min.y + 0.5, Stroke::new(1.0, crate::studio::LINE));
    painter.galley(pos2(place.min.x, place.center().y - label.size().y / 2.0), label, TEXT);
    let mut top = place.center().y - caps_tall / 2.0;
    for (line, taken) in &cap_lines {
        let mut left = place.max.x - taken;
        for at in line {
            let cap = &caps[*at];
            let block = Rect::from_min_size(pos2(left, top), vec2(wide(cap), CAP));
            painter.rect_filled(block, 6.0, crate::studio::SELECTED);
            painter.galley(block.center() - cap.size() / 2.0, cap.clone(), TEXT);
            left = block.max.x + BETWEEN;
        }
        top += CAP + 4.0;
    }
}

/// A group as Settings writes it: the fuller of its names ("Verse 1" for the group that
/// "verse" names too), with a capital.
pub(super) fn group_shown(names: &[&str]) -> String {
    let name = names.iter().find(|name| name.contains(' ')).or(names.first()).copied().unwrap_or("");
    let mut shown = name.to_owned();
    if let Some(first) = shown.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    shown
}

/// Settings › Command line: the `sp` command, which controls the running app from a terminal or a
/// script. Its row says where sp stands (read again each time the page comes into view, and after
/// each click) and offers what fits: Install, Use This Copy when sp opens another copy, Remove
/// once it is there (quiet while sp is where it should be), and nothing when the Linux package
/// put it in (the package removes it); under the row, what the last click did.
fn command_line_page(ui: &mut egui::Ui, state: &mut HomeState) {
    use crate::cli_install::{Status, Tone};
    let card_state = &mut state.command_line;
    // Not drawn in the pass before: the page has just come into view (chosen, or the window
    // opened again), so it reads afresh and forgets what it said.
    let pass = ui.ctx().cumulative_pass_nr();
    if card_state.drawn.is_none_or(|last| pass != last && pass != last + 1) {
        card_state.status = None;
        card_state.said = None;
    }
    card_state.drawn = Some(pass);
    let status = card_state.status.get_or_insert_with(crate::cli_install::status).clone();
    let (install, removable) = match &status {
        Status::Missing => (Some(t("settings.cli.install", "Install")), false),
        Status::Here { findable: true, .. } => (None, true),
        Status::Here { findable: false, .. } => (Some(t("settings.cli.install", "Install")), true),
        Status::Elsewhere { .. } | Status::Gone { .. } => (Some(t("settings.cli.use_this", "Use This Copy")), true),
        Status::Packaged { .. } => (None, false),
    };
    // Remove is quiet while sp is where it should be: there is nothing to put right.
    let quiet_remove = matches!(status, Status::Here { findable: true, .. });
    let (_, words) = crate::cli_install::status_words(&status, cfg!(windows));
    let remove_words = t("settings.cli.remove", "Remove");
    let install_size = install.map(|words| parts::key_size(ui, words));
    let remove_size = removable.then(|| parts::key_size(ui, remove_words));
    let size = vec2(
        install_size.map_or(0.0, |size| size.x)
            + remove_size.map_or(0.0, |size| size.x)
            + if install_size.is_some() && remove_size.is_some() { 8.0 } else { 0.0 },
        30.0,
    );
    let mut clicked_install = false;
    let mut clicked_remove = false;
    card(
        ui,
        t("settings.cli", "Command line"),
        t("settings.cli.about", "The sp command controls SoftPresenter from a terminal or a script."),
        None,
        |ui| {
            row(ui, t("settings.cli.sp", "The sp command"), &words, size, |ui, place| {
                let mut x = place.min.x;
                if let (Some(words), Some(wide)) = (install, install_size) {
                    let rect = Rect::from_min_size(pos2(x, place.min.y), wide);
                    let hint = t("settings.cli.install.hint", "Puts sp (and softpresenter) where a terminal finds it");
                    clicked_install = parts::key(ui, rect, ui.scope_id().with("install sp"), words, false)
                        .on_hover_text(hint)
                        .clicked();
                    x += wide.x + 8.0;
                }
                if let Some(wide) = remove_size {
                    let rect = Rect::from_min_size(pos2(x, place.min.y), wide);
                    let hint = t(
                        "settings.cli.remove.hint",
                        "Takes sp and softpresenter away again, with anything SoftPresenter added for them",
                    );
                    clicked_remove = parts::key(ui, rect, ui.scope_id().with("remove sp"), remove_words, quiet_remove)
                        .on_hover_text(hint)
                        .clicked();
                }
            });
            // What the last click did, under the row; a light for what needs a hand.
            if let Some((tone, said)) = state.command_line.said.clone() {
                match tone {
                    Tone::Care | Tone::Trouble => parts::said_line(ui, Some((tone, &said)), ""),
                    _ => parts::note(ui, &said),
                }
            }
        },
    );
    let card_state = &mut state.command_line;
    if clicked_install {
        card_state.said = Some(match crate::cli_install::install() {
            Ok(done) => (
                if done.add_folder { Tone::Care } else { Tone::Good },
                crate::cli_install::installed_words(&done, cfg!(windows)),
            ),
            Err(problem) => (Tone::Trouble, problem),
        });
        card_state.status = Some(crate::cli_install::status());
    }
    if clicked_remove {
        card_state.said = Some(match crate::cli_install::remove() {
            Ok(done) => (Tone::Plain, crate::cli_install::removed_words(&done)),
            Err(problem) => (Tone::Trouble, problem),
        });
        card_state.status = Some(crate::cli_install::status());
    }
}

/// Settings › Song files: first, finding the songs already on this computer (`find_card`); then
/// that songs are plain files that can be changed outside the app, reading them again, and what
/// happens when one was.
fn song_files_page(ui: &mut egui::Ui, state: &mut HomeState) {
    find_card::find_card(ui, &mut state.find_songs);
    card(
        ui,
        t("settings.song_files.title", "Song files"),
        t("settings.song_files.about", "Songs are plain files that can be changed outside the app"),
        None,
        |ui| {
            let reload = t("settings.song_files.reload", "Reload songs");
            let size = parts::key_size(ui, reload);
            let note = t("settings.song_files.reload.hint", "Brings in songs and themes changed outside the app");
            row(ui, t("settings.song_files.again", "Read them again"), note, size, |ui, place| {
                if parts::key(ui, place, ui.scope_id().with("reload songs"), reload, false).clicked() {
                    state.menu_asked = Some(crate::menubar::Action::ReloadSongs);
                }
            });
            row(
                ui,
                t("settings.song_files.changed", "When a song was changed elsewhere"),
                t("settings.song_files.changed.about", "A bar offers Reload or Keep mine; nothing is swapped unasked"),
                vec2(0.0, 30.0),
                |_, _| {},
            );
        },
    );
}

/// Settings › About: the version, updates (and under them, in a pre-release, a row saying so),
/// what's new (the What's new window, over this one: `whats_new.rs`), the guide, reporting a
/// problem and the licences, each a row with its key; then the anonymous reports' card.
fn about_page(ui: &mut egui::Ui, state: &mut HomeState) {
    let version = tf(
        "settings.about.version",
        "Version {version} \u{b7} free and open source, GPL-3.0-or-later",
        &[("version", crate::updates::VERSION)],
    );
    let mut opened = None;
    card(ui, "SoftPresenter", &version, None, |ui| {
        updates_row(ui, state);
        // A pre-release says so under Updates, as the setup's first step does.
        if let Some((heading, about)) = crate::updates::pre_release_words(crate::updates::VERSION) {
            let updates =
                t("settings.about.prerelease.updates", "Check for updates, above, tells you when one is out.");
            row(ui, &heading, &format!("{about} {updates}"), vec2(0.0, 30.0), |_, _| {});
        }
        for (label, key, note, page) in [
            (
                t("settings.about.whats_new.label", "What\u{2019}s new"),
                t("settings.about.whats_new", "What\u{2019}s new"),
                t("settings.about.whats_new.about", "The changes in this version"),
                None,
            ),
            (
                t("settings.about.help", "Help"),
                t("settings.about.guide", "Open the guide"),
                t("settings.about.guide.about", "How to set up the screens, run a service and more, on the web"),
                Some(Page::Guide),
            ),
            (
                t("settings.about.wrong", "Something wrong?"),
                t("settings.about.report", "Report an issue\u{2026}"),
                t("settings.about.report.about", "Opens a new issue on GitHub, with the version filled in"),
                Some(Page::ReportIssue),
            ),
            (
                t("settings.about.licences.label", "Licences"),
                t("settings.about.licences", "Licences\u{2026}"),
                t("settings.about.licences.about", "SoftPresenter\u{2019}s and those of the parts it carries"),
                Some(Page::Licences),
            ),
        ] {
            let size = parts::key_size(ui, key);
            row(ui, label, note, size, |ui, place| {
                if parts::key(ui, place, ui.scope_id().with(("about", key)), key, false).clicked() {
                    opened = Some(page);
                }
            });
        }
    });
    // A web page, or (with none) the What's new window on the version running.
    match opened {
        Some(Some(page)) => open_page(state, page),
        Some(None) => crate::whats_new::open_over_about(state),
        None => {}
    }
    crate::reports::ui::settings_card(ui, state);
}

/// Settings › About's Updates row: what the last Check for Updates found and when, and the key
/// that checks (greyed, saying so, while a check runs); once a newer version was found on GitHub,
/// a row under it with the key that opens its download page. The same as the Mac's app menu
/// item, which opens this page; on Windows and Linux, which have no menu bar, this is its place.
fn updates_row(ui: &mut egui::Ui, state: &mut HomeState) {
    let now = crate::updates::status();
    let font = FontId::proportional(12.5);
    let check = t("settings.updates.check", "Check for updates");
    let checking = t("settings.updates.checking", "Checking\u{2026}");
    let line = crate::updates::line(&now, state.clock.known());
    // The key keeps one width whichever it says.
    let size = vec2(crate::studio::widest(ui, &[check, checking], &font) + 26.0, 30.0);
    row(ui, t("settings.updates.label", "Updates"), &line, size, |ui, place| {
        let words = if now.checking { checking } else { check };
        let id = ui.scope_id().with("updates check");
        if flat_button(ui, place, id, words, font, Flat::Plain, !now.checking).clicked() {
            state.menu_asked = Some(crate::menubar::Action::CheckForUpdates);
        }
    });
    let Some((crate::updates::Found::Newer { version, page }, _)) = &now.last else { return };
    let open = t("settings.updates.open", "Open the download page");
    let label = tf("settings.updates.download", "SoftPresenter {version}", &[("version", version)]);
    let note = t("settings.updates.download.about", "Its page on GitHub, where it is downloaded from");
    row(ui, &label, note, parts::key_size(ui, open), |ui, place| {
        if parts::key(ui, place, ui.scope_id().with("updates open"), open, false).clicked()
            && let Err(e) = crate::canva::open_page(page)
        {
            state.notices.trouble(tf(
                "settings.about.not_opened",
                "Could not open the web page: {error}",
                &[("error", &e.to_string())],
            ));
        }
    });
}

/// Settings › Groups: each group's colour, name, the key that jumps to it and how many slides of
/// the library use it, in a card with a heading over each column, laid out as the design's grid.
fn groups_page(ui: &mut egui::Ui, state: &mut HomeState, show: &Show) {
    let uses = group_uses(state, show);
    card(
        ui,
        t("settings.pane.groups", "Groups"),
        t("settings.groups.about", "A song\u{2019}s parts: their names, colours and jump keys"),
        None,
        |ui| {
            // The design's columns: the colour (26), the name (1.4 shares), the key (54), Used on
            // (1 share) and its last (30, for deleting a group, which the app's fixed groups
            // don't have), 12 apart.
            const BETWEEN: f32 = 12.0;
            let width = ui.available_width();
            let shares = (width - 26.0 - 54.0 - 30.0 - 4.0 * BETWEEN).max(0.0) / 2.4;
            let name_left = 26.0 + BETWEEN;
            let key_left = name_left + 1.4 * shares + BETWEEN;
            let used_left = key_left + 54.0 + BETWEEN;
            let caps = |ui: &egui::Ui, words: &str, font: FontId, ink| {
                parts::words(ui, &words.to_uppercase(), font, ink, 0.575, f32::INFINITY)
            };
            let top = ui.cursor().min;
            ui.painter().hline(top.x..=top.x + width, top.y + 0.5, egui::Stroke::new(1.0, crate::studio::LINE));
            let (head, _) = ui.allocate_exact_size(vec2(width, 32.0), Sense::hover());
            for (left, words) in [
                (name_left, t("settings.groups.name", "Name")),
                (key_left, t("settings.groups.key", "Key")),
                (used_left, t("settings.groups.used", "Used on")),
            ] {
                let heading = caps(ui, words, fixed_semibold(11.5), TEXT_DIM);
                ui.painter().galley(pos2(head.min.x + left, head.min.y + 12.0), heading, TEXT_DIM);
            }
            for ((key, names), used) in sp_model::GROUP_KEYS.iter().zip(uses.iter()) {
                let name = names.iter().find(|name| name.contains(' ')).or(names.first()).copied().unwrap_or("");
                let shown = group_shown(names);
                let (place, _) = ui.allocate_exact_size(vec2(width, 38.0), Sense::hover());
                let painter = ui.painter();
                let middle = place.center().y;
                let swatch = Rect::from_min_size(pos2(place.min.x, middle - 13.0), vec2(26.0, 26.0));
                painter.rect_filled(swatch, 6.0, super::color32(sp_model::group_color(name)));
                painter.rect_stroke(swatch, 6.0, Stroke::new(1.0, crate::studio::LINE), StrokeKind::Outside);
                painter.text(
                    pos2(place.min.x + name_left, middle),
                    Align2::LEFT_CENTER,
                    shown,
                    FontId::proportional(13.0),
                    TEXT,
                );
                // The key, as a key cap in the lighter grey of what is on.
                let cap = Rect::from_min_size(pos2(place.min.x + key_left, middle - 12.0), vec2(54.0, 24.0));
                painter.rect_filled(cap, 6.0, crate::studio::SELECTED);
                let letter = key.to_ascii_uppercase().to_string();
                painter.text(cap.center(), Align2::CENTER_CENTER, letter, fixed_semibold(11.0), TEXT);
                let words = match *used {
                    0 => t("settings.groups.not_used", "Not used").to_owned(),
                    n => tn("settings.groups.slides", n as u64, "{n} slide", "{n} slides", &[]),
                };
                let readout = caps(ui, &words, fixed(11.5), TEXT_FAINT);
                ui.painter().galley(
                    pos2(place.min.x + used_left, middle - readout.size().y / 2.0),
                    readout,
                    TEXT_FAINT,
                );
            }
            ui.add_space(12.0);
        },
    );
}

/// How many slides use each group a key jumps to (`GROUP_KEYS`, in order, under any of its
/// names), across the songs the library lists (not the ones it hides or has taken out). Worked out
/// again only when the songs change, never on every frame.
fn group_uses(state: &mut HomeState, show: &Show) -> std::sync::Arc<Vec<usize>> {
    let epoch = state.lists.songs.epoch();
    if let Some(kept) = state.lists.group_uses.get(|made_in| *made_in == epoch) {
        return kept;
    }
    let hidden = state.hidden_slots();
    let mut uses = vec![0; sp_model::GROUP_KEYS.len()];
    for (at, song) in show.presentations().iter().enumerate() {
        if hidden.contains(&at) || state.removed_songs.contains(&at) {
            continue;
        }
        for group in &song.groups {
            let name = group.name.trim().to_lowercase();
            if let Some(key) = sp_model::GROUP_KEYS.iter().position(|(_, names)| names.contains(&name.as_str())) {
                uses[key] += group.slides.len();
            }
        }
    }
    state.lists.group_uses.keep(epoch, uses)
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

    /// Clicks the words `name` where a frame of Settings drew them. False when they weren't drawn.
    pub(super) fn click(ctx: &egui::Context, state: &mut HomeState, show: &Show, name: &str) -> bool {
        click_where(ctx, state, show, name, false)
    }

    /// As [`click`], on the last place the words were drawn (the page's, not the list's).
    pub(super) fn click_last(ctx: &egui::Context, state: &mut HomeState, show: &Show, name: &str) -> bool {
        click_where(ctx, state, show, name, true)
    }

    fn click_where(ctx: &egui::Context, state: &mut HomeState, show: &Show, name: &str, last: bool) -> bool {
        use egui::{Event, Modifiers};
        let mut at = None;
        for round in 0..2 {
            let output = crate::studio::tests::drawn(ctx, vec![], |ui| settings_ui(ui, state, show));
            if round == 1 {
                at = if last {
                    crate::studio::tests::place_on_top(&output, name)
                } else {
                    crate::studio::tests::place_of(&output, name)
                };
            }
            output.drop_without_applying_deltas();
        }
        let Some(at) = at else { return false };
        let press = |pressed| Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        for events in [vec![Event::PointerMoved(at), press(true)], vec![press(false)]] {
            crate::studio::tests::drawn(ctx, events, |ui| settings_ui(ui, state, show)).drop_without_applying_deltas();
        }
        true
    }

    #[test]
    fn slides_sets_the_copyright_line_the_transition_and_the_view() {
        use crate::service::CopyrightShow;
        use sp_model::TransitionKind as K;
        let ctx = crate::studio::tests::styled();
        let show = Show::new(Vec::new());
        let mut state = HomeState { preferences_pane: Pane::Slides, ..HomeState::default() };
        for (name, want) in [
            ("Every slide", CopyrightShow::EverySlide),
            ("Last slide", CopyrightShow::LastSlide),
            ("First slide", CopyrightShow::FirstSlide),
            ("Nowhere", CopyrightShow::Never),
        ] {
            assert!(click(&ctx, &mut state, &show, name), "{name}");
            assert_eq!(state.copyright_show, want, "{name}");
        }
        // With no song's details filled in, the slide says how to see a line.
        state.copyright_show = CopyrightShow::FirstSlide;
        let drawn = crate::studio::tests::drawn(&ctx, vec![], |ui| settings_ui(ui, &mut state, &show));
        assert!(crate::studio::tests::place_of(&drawn, "Fill in a song\u{2019}s details to see its line").is_some());
        drawn.drop_without_applying_deltas();
        assert!(click(&ctx, &mut state, &show, "Cut"));
        assert_eq!(state.transition.kind, K::Cut);
        assert!(click(&ctx, &mut state, &show, "Fade"));
        assert_eq!(state.transition, sp_model::Transition::of(K::Fade), "a new kind takes its usual time");
        // The page's last On/Off: the End of a song card's is above it.
        let (continuous, end_mark) = (state.continuous, state.end_mark);
        let words = if continuous { "On" } else { "Off" };
        assert!(click_last(&ctx, &mut state, &show, words));
        assert_eq!(state.continuous, !continuous, "Carry on to the next item switches Continuous");
        assert_eq!(state.end_mark, end_mark, "and nothing else");
        // In your window is under the fold of this 800-point frame since the End of a song
        // card came in above it: scrolled to, as the Interface size row's link does.
        state.show_interface_size = true;
        for _ in 0..60 {
            // The page glides there over some frames.
            crate::studio::tests::drawn(&ctx, vec![], |ui| settings_ui(ui, &mut state, &show))
                .drop_without_applying_deltas();
        }
        assert!(click(&ctx, &mut state, &show, "Table"));
        assert_eq!(state.slide_view, crate::home::SlideView::Table);
        assert!(click(&ctx, &mut state, &show, "Words only"));
        assert_eq!(state.slide_view, crate::home::SlideView::Easy);
    }

    #[test]
    fn slides_opens_the_main_themes_end_mark_and_draws_its_mark_on_the_card() {
        use sp_model::{EndLook, EndMark, Theme};
        let ctx = crate::studio::tests::styled();
        let song = sp_model::parse_song_text("Song", "One\nTwo\n");
        let show = Show::new(vec![std::sync::Arc::new(song)]);
        let builtin = Theme::builtin();
        let evening = sp_model::Uuid::from_u128(7);
        let mut state = HomeState {
            preferences_open: true,
            preferences_pane: Pane::Slides,
            theme_ids: vec![builtin.id, evening],
            theme_names: vec![builtin.name.clone(), "Evening".to_owned()],
            theme_end_looks: vec![EndLook::default(), EndLook { mark: EndMark::Dash, ..EndLook::default() }],
            theme_for_songs: 1,
            end_mark: true,
            ..HomeState::default()
        };
        // The card's small slide draws the main theme's mark, and its note names that theme.
        let drawn = crate::studio::tests::drawn(&ctx, vec![], |ui| settings_ui(ui, &mut state, &show));
        let has = |words: &str| crate::studio::tests::place_of(&drawn, words).is_some();
        assert!(has(&EndMark::Dash.text()) && !has(&EndMark::Star.text()));
        assert!(has("Its mark, size, colour and place belong to the theme (Evening)."));
        drawn.drop_without_applying_deltas();
        // Its key opens the window for the main theme.
        assert!(click(&ctx, &mut state, &show, "Change how it looks\u{2026}"));
        let open = state.end_look.expect("the window is open");
        assert_eq!((open.theme, open.door), (evening, crate::end_look::Door::Settings));
        let drawn = crate::studio::tests::drawn(&ctx, vec![], |ui| settings_ui(ui, &mut state, &show));
        assert!(crate::studio::tests::place_of(&drawn, "The end of a song \u{b7} Evening").is_some());
        drawn.drop_without_applying_deltas();
    }

    #[test]
    fn slides_turns_the_end_mark_on_and_off() {
        let ctx = crate::studio::tests::styled();
        let song = sp_model::parse_song_text("Song", "One\nTwo\n\nThree\nFour\nThe last line\n");
        let show = Show::new(vec![std::sync::Arc::new(song)]);
        let mut state = HomeState { preferences_pane: Pane::Slides, ..HomeState::default() };
        assert!(!state.end_mark, "off until asked for");
        let mark = "*\u{a0}\u{a0}\u{a0}\u{a0}*\u{a0}\u{a0}\u{a0}\u{a0}*";
        let page = |state: &mut HomeState| {
            let drawn = crate::studio::tests::drawn(&ctx, vec![], |ui| settings_ui(ui, state, &show));
            let place = |words: &str| crate::studio::tests::place_of(&drawn, words);
            let places = (
                ["Copyright line", "End of a song", "Between slides", "Mark the end of a song", "The last line"]
                    .map(|words| place(words).unwrap_or_else(|| panic!("{words}"))),
                place(mark),
                place("Four"),
                place("Three"),
            );
            drawn.drop_without_applying_deltas();
            places
        };
        let ([copyright, card, between, line, last], no_mark, above, third_last) = page(&mut state);
        // The card is between Copyright line and Between slides, and says what it is.
        assert!(copyright.y < card.y && card.y < between.y, "{copyright:?} {card:?} {between:?}");
        assert!(card.y < line.y && line.y < between.y);
        // Its slide shows the song's last slide's last two lines, and no mark while it is off.
        assert!(above.is_some() && third_last.is_none() && no_mark.is_none());
        // The card's key is the page's first On/Off.
        assert!(click(&ctx, &mut state, &show, "Off"));
        assert!(state.end_mark, "on");
        assert!(!state.continuous, "Carry on to the next item is another switch");
        let ([.., line_on, last_on], mark_on, ..) = page(&mut state);
        let mark_on = mark_on.expect("the mark is drawn on the small slide");
        // Under the words, centred as they are; nothing moved or changed size for it.
        assert!(mark_on.y > last_on.y && (mark_on.x - last_on.x).abs() < 1.0, "{mark_on:?} under {last_on:?}");
        assert_eq!((last_on, line_on), (last, line), "the words and the line stay where they were");
        assert!(click(&ctx, &mut state, &show, "On"));
        assert!(!state.end_mark, "off again");
        let ([.., line_off, last_off], mark_off, ..) = page(&mut state);
        assert!(mark_off.is_none());
        assert_eq!((last_off, line_off), (last, line));
        // The sample is the first song's last slide as sung, with its translation's last
        // line, the mark under that: a deck in front of the song, and the song's blank and
        // disabled slides after its last words, are passed over.
        let mut deck = sp_model::parse_song_text("Sermon", "Point one\n");
        deck.groups[0].slides[0].objects[0].role = None;
        let mut song = sp_model::parse_song_text("Song", "One\n\nMorning\nlight\n--\nRanok\nsvitlo\n\nLeft out\n");
        song.groups[0].slides[2].enabled = false;
        song.groups[0].slides.push(sp_model::Slide::new(Vec::new()));
        let show = Show::new(vec![std::sync::Arc::new(deck), std::sync::Arc::new(song)]);
        state.end_mark = true;
        let drawn = crate::studio::tests::drawn(&ctx, vec![], |ui| settings_ui(ui, &mut state, &show));
        let place = |words: &str| crate::studio::tests::place_of(&drawn, words);
        let [light, translated, stars] = ["light", "svitlo", mark].map(|words| place(words).expect(words));
        assert!(light.y < translated.y && translated.y < stars.y, "{light:?} {translated:?} {stars:?}");
        for absent in ["Left out", "One", "Ranok"] {
            assert!(place(absent).is_none(), "{absent}");
        }
        drawn.drop_without_applying_deltas();
        // Saved with the other choices, and off in a file from before it.
        state.end_mark = true;
        assert!(state.prefs().end_mark);
        let before: crate::service::Prefs = serde_json::from_str("{}").expect("an older file");
        assert!(!before.end_mark);
    }

    #[test]
    fn language_shows_each_language_in_its_own_name_after_the_computers() {
        let ctx = crate::studio::tests::styled();
        let show = Show::new(Vec::new());
        let mut state = HomeState { preferences_pane: Pane::Language, ..HomeState::default() };
        let drawn = crate::studio::tests::drawn(&ctx, vec![], |ui| settings_ui(ui, &mut state, &show));
        let place =
            |words: &str| crate::studio::tests::place_on_top(&drawn, words).unwrap_or_else(|| panic!("{words}"));
        let computer = place("This computer\u{2019}s");
        for name in ["English", "Español", "Русский", "Українська"] {
            assert!(place(name).x > computer.x, "{name} comes after the computer's");
        }
        drawn.drop_without_applying_deltas();
    }

    #[test]
    fn every_service_has_the_same_card_and_one_opens_its_page() {
        let ctx = crate::studio::tests::styled();
        let show = Show::new(Vec::new());
        let mut state = HomeState { preferences_pane: Pane::Integrations, ..HomeState::default() };
        state.timelychurch.offered = false;
        let drawn = crate::studio::tests::drawn(&ctx, vec![], |ui| settings_ui(ui, &mut state, &show));
        for name in ["VoxLive", "Canva Sync", "TimelyChurch", "More to come", "COMING SOON"] {
            assert!(crate::studio::tests::place_of(&drawn, name).is_some(), "{name}");
        }
        drawn.drop_without_applying_deltas();
        // TimelyChurch, not offered yet, is greyed and takes no clicks.
        assert!(click(&ctx, &mut state, &show, "TimelyChurch"));
        assert!(!state.timelychurch.open);
        assert!(click(&ctx, &mut state, &show, "Canva Sync"));
        assert!(state.canva.open, "a card opens its service's page");
        // The page has the way back.
        assert!(click_last(&ctx, &mut state, &show, "Integrations"));
        assert!(!state.canva.open);
    }

    /// Every piece of words a frame drew, with the rectangle it took.
    pub(super) fn words_drawn(output: &egui::FullOutput) -> Vec<(String, Rect)> {
        fn walk(shape: &egui::Shape, into: &mut Vec<(String, Rect)>) {
            match shape {
                egui::Shape::Text(text) => into.push((text.galley.text().to_owned(), text.visual_bounding_rect())),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| walk(shape, into)),
                _ => {}
            }
        }
        let mut into = Vec::new();
        output.shapes.iter().for_each(|clipped| walk(&clipped.shape, &mut into));
        into
    }

    /// Draws Settings at a page in a window `width` wide and tall enough for the whole page, two
    /// frames (the second with every size known), and gives what the second drew.
    fn settings_at(width: f32, pane: Pane) -> Vec<(String, Rect)> {
        settings_with(width, pane, &Show::new(Vec::new()))
    }

    /// As [`settings_at`], with these songs.
    fn settings_with(width: f32, pane: Pane, show: &Show) -> Vec<(String, Rect)> {
        let ctx = crate::studio::tests::styled();
        let mut state = HomeState { preferences_pane: pane, ..HomeState::default() };
        let mut drawn = Vec::new();
        for _ in 0..2 {
            let raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(width, 4000.0))),
                ..egui::RawInput::default()
            };
            let output = ctx.run_ui(raw, |ui| settings_ui(ui, &mut state, show));
            drawn = words_drawn(&output);
            output.drop_without_applying_deltas();
        }
        drawn
    }

    #[test]
    fn always_the_same_is_plain_lines_each_with_its_keys_as_caps_at_the_right() {
        use crate::i18n::Language;
        for language in Language::ALL {
            crate::i18n::with_language(language, || {
                for width in [820.0, 1100.0, 1440.0] {
                    let said = format!("{language:?} at {width}");
                    // The page's words end 28 inside its panel, which ends a gap inside the window.
                    let right = width - GAP - 28.0;
                    let drawn = settings_at(width, Pane::Keys);
                    for (words, rect) in &drawn {
                        assert!(rect.max.x <= right + 0.5, "{said}: \u{201c}{words}\u{201d} ends at {}", rect.max.x);
                    }
                    // Every fixed key and every menu shortcut has a line: what it does at the
                    // left, and each of its keys as a cap of its own at the right.
                    let mut lines: Vec<(String, Vec<String>)> = Vec::new();
                    for (_, rows) in crate::keys::fixed_keys() {
                        lines.extend(rows.into_iter().map(|(keys, what)| (what, keys)));
                    }
                    for (_, command, key) in crate::menubar::shortcuts() {
                        lines.push((command.trim_end_matches('\u{2026}').to_owned(), vec![key]));
                    }
                    // The lines are under the card's title, in the list's order.
                    let title = t("settings.keys.fixed.title", "Always the same");
                    let under = drawn.iter().find(|(words, _)| words == title).expect("the card").1.max.y;
                    let mut above = under;
                    let mut ends: Vec<f32> = Vec::new();
                    for (what, keys) in &lines {
                        let label = drawn
                            .iter()
                            .find(|(words, rect)| words == what && rect.min.y > above - 1.0)
                            .unwrap_or_else(|| panic!("{said}: the line of {what}"))
                            .1;
                        // Its caps: the pieces of words that are its keys, beside it.
                        let mut last_end = f32::MIN;
                        for key in keys {
                            let cap = drawn
                                .iter()
                                .filter(|(words, rect)| words == key && rect.min.y > above - 1.0)
                                .map(|(_, rect)| *rect)
                                .find(|rect| {
                                    rect.center().y > label.min.y - 30.0 && rect.center().y < label.max.y + 30.0
                                })
                                .unwrap_or_else(|| panic!("{said}: the cap {key} of {what}"));
                            assert!(cap.min.x > label.max.x, "{said}: {key} is at the right of \u{201c}{what}\u{201d}");
                            last_end = last_end.max(cap.max.x);
                        }
                        ends.push(last_end);
                        above = label.min.y;
                    }
                    // Every line's caps end at one place, the right edge of the card's inside
                    // (which is 18 inside the card, and the card never passes the page): the last
                    // cap's words end inside its block, which is 26 wide at least with the words
                    // in its middle.
                    let most = ends.iter().copied().fold(f32::MIN, f32::max);
                    assert!(most <= right - 18.0 + 0.5, "{said}: the caps end at {most}");
                    for ((what, _), end) in lines.iter().zip(&ends) {
                        assert!(
                            most - *end <= 13.5,
                            "{said}: the caps of \u{201c}{what}\u{201d} end at {end}, not {most}"
                        );
                    }
                }
            });
        }
    }

    #[test]
    fn no_page_passes_the_window_edge_at_its_least_width() {
        use crate::i18n::Language;
        let right = 820.0 - GAP - 28.0;
        for language in [Language::English, Language::Spanish] {
            crate::i18n::with_language(language, || {
                for pane in [
                    Pane::Screens,
                    Pane::Slides,
                    Pane::Groups,
                    Pane::Language,
                    Pane::Integrations,
                    Pane::CommandLine,
                    Pane::SongFiles,
                    Pane::About,
                ] {
                    // Words with nothing to draw (an empty line) take no room.
                    for (words, rect) in settings_at(820.0, pane).into_iter().filter(|(words, _)| !words.is_empty()) {
                        assert!(
                            rect.max.x <= right + 0.5,
                            "{language:?}, {pane:?}: \u{201c}{words}\u{201d} ends at {}",
                            rect.max.x
                        );
                    }
                }
            });
        }
    }

    #[test]
    fn groups_say_how_many_slides_use_each() {
        let song = sp_model::parse_song_text("Test", "[Verse 1]\nOne\n\nTwo\n\n[Chorus]\nThree\n");
        let show = Show::new(vec![std::sync::Arc::new(song)]);
        let drawn = settings_with(1100.0, Pane::Groups, &show);
        let count = |wanted: &str| drawn.iter().filter(|(words, _)| words == wanted).count();
        assert_eq!(count("USED ON"), 1);
        assert_eq!(count("2 SLIDES"), 1, "Verse 1");
        assert_eq!(count("1 SLIDE"), 1, "Chorus");
        assert_eq!(count("NOT USED"), sp_model::GROUP_KEYS.len() - 2);
        let spanish =
            crate::i18n::with_language(crate::i18n::Language::Spanish, || settings_with(1100.0, Pane::Groups, &show));
        assert!(spanish.iter().any(|(words, _)| words == "2 DIAPOSITIVAS"));
        assert!(spanish.iter().any(|(words, _)| words == "SIN USAR"));
    }

    #[test]
    fn a_row_says_how_its_page_stands() {
        let show = Show::new(Vec::new());
        let mut state = HomeState::default();
        state.mcp.switches = crate::mcp::Switches::default();
        state.remote.on = false;
        let said = |state: &HomeState, pane: Pane| readout(pane, state, &show).map(|said| (said.words, said.lit));
        assert_eq!(said(&state, Pane::AiAssistants), Some(("Off".to_owned(), false)));
        state.mcp.switches.allowed = true;
        assert_eq!(said(&state, Pane::AiAssistants), Some(("Can prepare".to_owned(), true)));
        state.mcp.switches.screens = true;
        assert_eq!(said(&state, Pane::AiAssistants), Some(("Runs screens".to_owned(), true)));
        assert_eq!(said(&state, Pane::PhoneRemote), Some(("Off".to_owned(), false)));
        state.remote.on = true;
        assert_eq!(said(&state, Pane::PhoneRemote), Some(("0 people".to_owned(), true)));
        assert_eq!(said(&state, Pane::Integrations), Some(("0 connected".to_owned(), false)));
        let all = state.screens.len();
        assert_eq!(said(&state, Pane::Screens), Some((format!("0 of {all} on"), false)), "none is shown yet");
        assert_eq!(said(&state, Pane::Keys), None, "Advanced's pages say nothing at the end of their rows");
        // In Spanish, a row whose name is long has a shorter readout to hand.
        let spanish =
            crate::i18n::with_language(crate::i18n::Language::Spanish, || readout(Pane::Integrations, &state, &show));
        assert_eq!(spanish.map(|said| said.short), Some("0 activas".to_owned()));
    }
}
