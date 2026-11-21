//! The tool tabs under Clear All in the right column: Timers, Messages, Props, Macros, Stage,
//! Audio and VoxLive.
//!
//! Each tab is a heading in small capitals and a list of cards. A card whose thing is on screen
//! is tinted orange, one whose thing runs (a timer) green; its buttons keep their grey, and keep
//! their width whatever they say, so nothing moves. What a card can do beyond its buttons is in
//! its ⋯ menu, which a right-click on the card opens too.

use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::{Align2, Color32, FontId, Rect, Sense, pos2, vec2};

use crate::home::{HomeState, ThumbOf};
use crate::icons::Icon;
use crate::messages::Message;
use crate::screens::Kind;
use crate::studio::{
    ACCENT, CARD, CARD_RUNNING, CARD_SHOWN, Flat, PANEL, RADIUS_CARD, READY, TALLY, TEXT, TEXT_DIM, TEXT_FAINT, Tabs,
    caption, fixed, fixed_semibold, flat_button, flat_button_width, icon_key, paint_spaced, quiet_icon, semibold,
    spaced, tab_switch, tab_switch_height,
};
use crate::timers::{self, Timer};

/// Draws the tabs and the one that is open. `thumb` draws the small pictures of props and stage
/// layouts.
pub fn live_tabs(ui: &mut egui::Ui, show: &sp_engine::Show, state: &mut HomeState, thumb: &mut Thumb) {
    let width = ui.available_width();
    // VoxLive's tab, once it is connected and has screens.
    let voxlive =
        state.voxlive_slot.and_then(|slot| show.presentations().get(slot)).is_some_and(|item| item.slide_count() > 0);
    const NAMES: [&str; 7] = ["Timers", "Messages", "Props", "Macros", "Stage", "Audio", "VoxLive"];
    // Each tab's key is in the View menu, which only a Mac has.
    const HINTS: &[&str] = if cfg!(target_os = "macos") {
        &[
            "Timers (Control-C)",
            "Messages (Control-M)",
            "Props (Control-P)",
            "Macros (Control-Shift-M)",
            "Stage (Shift-Command-S)",
            "Audio (Control-A)",
            "VoxLive",
        ]
    } else {
        &[]
    };
    let count = if voxlive { 7 } else { 6 };
    if state.live_tab >= count {
        state.live_tab = 0;
    }
    // A small dot on a tab whose thing is active: orange for what is on screen (a message, a
    // prop, the audio layer playing), grey for a timer that is running, which is not on screen.
    let dots = [
        state.timers.iter().any(Timer::running).then_some(TEXT_DIM),
        state.message_live.is_some().then_some(TALLY),
        (!state.props_live.is_empty()).then_some(TALLY),
        None,
        None,
        state.audio_playing.track.is_some().then_some(TALLY),
        None,
    ];
    let tabs = Tabs { hints: &HINTS[..count.min(HINTS.len())], dots: &dots[..count], ..Tabs::plain(&NAMES[..count]) };
    let (place, _) = ui.allocate_exact_size(vec2(width, tab_switch_height(&tabs)), Sense::hover());
    if let (_, Some(tab)) = tab_switch(ui, "live tabs", place.min, width, &tabs, Some(state.live_tab)) {
        state.live_tab = tab;
    }
    ui.add_space(4.0);
    let mut area = egui::ScrollArea::vertical().id_salt("live tab").auto_shrink([false, false]);
    // Testing aid: scroll the open tab this far down, to take a picture of its foot in a window
    // too short to show all of it.
    static SCROLLED: std::sync::OnceLock<Option<f32>> = std::sync::OnceLock::new();
    if let Some(down) = *SCROLLED.get_or_init(|| std::env::var("SOFTPRESENTER_TAB_SCROLL").ok()?.parse().ok()) {
        area = area.vertical_scroll_offset(down);
    }
    let scrolled = area.show(ui, |ui| {
        // The same width whether the tab scrolls or not (its bar floats over the edge), so
        // nothing moves when a list grows long enough to scroll.
        ui.set_width(width);
        ui.spacing_mut().item_spacing.y = GAP;
        match state.live_tab {
            0 => timers_tab(ui, state),
            1 => messages_tab(ui, state),
            2 => props_tab(ui, state, thumb),
            3 => macros_tab(ui, show, state),
            4 => stage_tab(ui, show, state, thumb),
            5 => audio_tab(ui, state),
            _ => voxlive_tab(ui, show, state),
        }
        ui.add_space(4.0);
    });
    // When the tab goes on below what shows, its foot fades out, so it reads as going on (scroll
    // to see the rest) rather than cut off.
    let view = scrolled.inner_rect;
    let below = scrolled.content_size.y - scrolled.state.clamped_offset().y - view.height();
    if below > 1.0 {
        let foot = egui::Rect::from_min_max(egui::pos2(view.min.x, view.max.y - 28.0), view.max);
        let mut mesh = egui::Mesh::default();
        mesh.colored_vertex(foot.left_top(), Color32::TRANSPARENT);
        mesh.colored_vertex(foot.right_top(), Color32::TRANSPARENT);
        mesh.colored_vertex(foot.left_bottom(), PANEL);
        mesh.colored_vertex(foot.right_bottom(), PANEL);
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(1, 3, 2);
        ui.painter().add(egui::Shape::mesh(mesh));
    }
}

/// The room between a tab's heading and its cards, and between one card and the next.
const GAP: f32 = 7.0;
/// The room inside a card, round what it holds.
const CARD_MARGIN: egui::Margin = egui::Margin { left: 13, right: 10, top: 9, bottom: 9 };
/// A card's square keys (start, reset, ⋯), and the room between the parts of a card.
const CARD_KEY: f32 = 34.0;
const CARD_GAP: f32 = 10.0;
/// The face of a card's name.
fn name_font() -> FontId {
    semibold(13.0)
}

/// A tab's heading: its name in small capitals and, when it adds things, a + at its right with
/// `add` as its tooltip. Returns the row (for anything else to go in it, left of the +) and
/// true when the + was clicked.
fn heading(ui: &mut egui::Ui, title: &str, add: Option<&str>) -> (Rect, bool) {
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::hover());
    let font = fixed_semibold(11.5);
    paint_spaced(ui, row.left_center(), Align2::LEFT_CENTER, &title.to_uppercase(), font, TEXT_DIM, 0.6);
    let clicked = add.is_some_and(|hint| {
        let plus = Rect::from_min_size(pos2(row.max.x - 28.0, row.min.y), vec2(28.0, 28.0));
        let id = egui::Id::unique(("tab heading +", title));
        quiet_icon(ui, plus, id, Icon::Plus, 13.0, 0.78, Some(CARD)).on_hover_text(hint).clicked()
    });
    (row, clicked)
}

/// Words said when a tab has nothing in it yet.
fn empty_note(ui: &mut egui::Ui, words: &str) {
    ui.label(egui::RichText::new(words).size(12.0).color(TEXT_FAINT));
}

/// A card: the card grey (or a tint of it), round corners, and room round what it holds.
fn card<R>(ui: &mut egui::Ui, fill: Color32, contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .fill(fill)
        .corner_radius(RADIUS_CARD as u8)
        .inner_margin(CARD_MARGIN)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            contents(ui)
        })
        .inner
}

/// The part of a card behind its keys, which takes a right-click (for the card's menu), a
/// double-click and the pointer for a tooltip. `row` is the card's first line; this reaches
/// out over the card's margins.
fn card_back(ui: &egui::Ui, row: Rect, id: egui::Id) -> egui::Response {
    let reach = Rect::from_min_max(
        row.min - vec2(f32::from(CARD_MARGIN.left), f32::from(CARD_MARGIN.top)),
        row.max + vec2(f32::from(CARD_MARGIN.right), f32::from(CARD_MARGIN.bottom)),
    );
    ui.interact(reach, id, Sense::click())
}

/// Words set to fit `width`, in at most `rows` lines, cut short with … beyond them.
fn fitted(ui: &egui::Ui, words: &str, font: FontId, ink: Color32, width: f32, rows: usize) -> Arc<egui::Galley> {
    let format = egui::TextFormat { font_id: font, color: ink, ..Default::default() };
    let mut job = egui::text::LayoutJob::single_section(words.to_owned(), format);
    job.wrap = egui::text::TextWrapping {
        max_width: width.max(1.0),
        max_rows: rows,
        break_anywhere: rows == 1,
        overflow_character: Some('…'),
    };
    ui.fonts_mut(|fonts| fonts.layout_job(job))
}

/// A card's ⋯: always there, faded until the pointer is over it.
fn more_key(ui: &egui::Ui, rect: Rect, id: egui::Id) -> egui::Response {
    quiet_icon(ui, rect, id, Icon::More, 16.0, 0.6, None).on_hover_text("More")
}

/// A field for a length of time or a time of day as `HH:MM:SS`. What is typed is kept while the
/// field has the keyboard and read when it is left. Returns `Some` once it is left with
/// something that reads (`Some(None)` when it was emptied).
fn clock_field(ui: &mut egui::Ui, id: egui::Id, current: Option<u32>, empty: &str) -> Option<Option<u32>> {
    let shown = current.map(|seconds| timers::clock_text(i64::from(seconds))).unwrap_or_default();
    let mut text = ui.data_mut(|data| data.get_temp::<String>(id)).unwrap_or(shown);
    let response = ui.add(egui::TextEdit::singleline(&mut text).id(id).desired_width(96.0).hint_text(empty));
    if response.has_focus() {
        ui.data_mut(|data| data.insert_temp(id, text.clone()));
    }
    if response.lost_focus() {
        ui.data_mut(|data| data.remove::<String>(id));
        return if text.trim().is_empty() { Some(None) } else { timers::parse_clock(&text).map(Some) };
    }
    None
}

/// How long until a running timer's readout next changes: at its next whole second, counted
/// from when it started (a countdown to a time of day goes by the clock's seconds).
fn until_next_second(timer: &Timer, now: Instant) -> Option<Duration> {
    let started = timer.run.started?;
    let into_second = match timer.kind {
        timers::Kind::CountdownTo { .. } => {
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.subsec_nanos())
        }
        _ => (timer.run.banked + now.saturating_duration_since(started)).subsec_nanos(),
    };
    Some(Duration::from_nanos(1_000_000_000 - u64::from(into_second)))
}

/// Something asked of a timer from its card.
enum TimerAsk {
    Toggle,
    Reset,
    Edit,
    Delete,
}

/// A timer's menu, behind its ⋯ and a right-click on its card.
fn timer_menu(ui: &mut egui::Ui, open: bool) -> Option<TimerAsk> {
    let mut asked = None;
    if ui.add(egui::Button::selectable(open, "Edit…")).clicked() {
        asked = Some(TimerAsk::Edit);
    }
    ui.separator();
    if ui.button("Delete").clicked() {
        asked = Some(TimerAsk::Delete);
    }
    asked
}

fn timers_tab(ui: &mut egui::Ui, state: &mut HomeState) {
    let (_, add) = heading(ui, "Timers", Some("New timer"));
    if add {
        let taken = |name: &str| state.timers.iter().any(|timer| timer.name == name);
        let name = std::iter::once("Timer".to_owned())
            .chain((2..).map(|n| format!("Timer {n}")))
            .find(|name| !taken(name))
            .unwrap_or_default();
        let timer = Timer::new(name);
        state.timer_open = Some(timer.id);
        state.timers.push(timer);
        state.timers_changed = true;
    }
    if state.timers.is_empty() {
        empty_note(ui, "No timers. Press + to add one.");
        return;
    }
    let now = Instant::now();
    let day = timers::seconds_into_day();
    let mut asked: Option<(usize, TimerAsk)> = None;
    let mut next_tick: Option<Duration> = None;
    for index in 0..state.timers.len() {
        let id = state.timers[index].id;
        let open = state.timer_open == Some(id);
        let running = state.timers[index].running();
        if let Some(wait) = until_next_second(&state.timers[index], now) {
            next_tick = Some(next_tick.map_or(wait, |sooner| sooner.min(wait)));
        }
        card(ui, if running { CARD_RUNNING } else { CARD }, |ui| {
            let timer = &state.timers[index];
            let width = ui.available_width();
            // The time, in the fixed-width face: room is kept for its longest (with a minus sign
            // when the timer may run over), so the name beside it never moves.
            let time_font = fixed(17.0);
            let longest = if timer.allow_overrun { "-00:00:00" } else { "00:00:00" };
            let time_wide = spaced(ui, longest, time_font.clone(), TEXT_DIM, -0.5).size().x;
            let (time, ink) = match (running, timer.kind) {
                (true, _) => (timer.text(now, day), READY),
                // A countdown to a time of day says when it ends until it is started, rather
                // than counting while it is stopped.
                (false, timers::Kind::CountdownTo { hour, minute, .. }) => {
                    (format!("to {hour:02}:{minute:02}"), TEXT_DIM)
                }
                (false, _) => (timer.text(now, day), TEXT_DIM),
            };
            let name_wide = width - 3.0 * CARD_KEY - 4.0 * CARD_GAP - time_wide;
            let name = fitted(ui, &timer.name, name_font(), TEXT, name_wide, 2);
            let (row, _) = ui.allocate_exact_size(vec2(width, name.size().y.max(CARD_KEY)), Sense::hover());
            let back = card_back(ui, row, egui::Id::unique(("timer card", id)));
            let y = row.center().y;
            ui.painter().galley(pos2(row.min.x, y - name.size().y / 2.0), name, TEXT);
            let key = |place: f32| {
                Rect::from_min_size(
                    pos2(row.max.x - place * (CARD_KEY + CARD_GAP) - CARD_KEY, y - CARD_KEY / 2.0),
                    vec2(CARD_KEY, CARD_KEY),
                )
            };
            let time_right = key(2.0).min.x - CARD_GAP;
            paint_spaced(ui, pos2(time_right, y), Align2::RIGHT_CENTER, &time, time_font, ink, -0.5);
            if icon_key(ui, key(2.0), egui::Id::unique(("timer reset", id)), Icon::Forward, 15.0, true)
                .on_hover_text("Reset")
                .clicked()
            {
                asked = Some((index, TimerAsk::Reset));
            }
            let (icon, hint) = if running { (Icon::Pause, "Stop") } else { (Icon::Play, "Start") };
            if icon_key(ui, key(1.0), egui::Id::unique(("timer start", id)), icon, 12.0, true)
                .on_hover_text(hint)
                .clicked()
            {
                asked = Some((index, TimerAsk::Toggle));
            }
            let more = more_key(ui, key(0.0), egui::Id::unique(("timer more", id)));
            egui::Popup::menu(&more).show(|ui| {
                if let Some(ask) = timer_menu(ui, open) {
                    asked = Some((index, ask));
                }
            });
            if back.double_clicked() {
                asked = Some((index, TimerAsk::Edit));
            }
            back.on_hover_text(timer.describe()).context_menu(|ui| {
                if let Some(ask) = timer_menu(ui, open) {
                    asked = Some((index, ask));
                }
            });
            if open {
                state.timers_changed |= timer_settings(ui, &mut state.timers[index]);
                ui.add_space(6.0);
                let done = Rect::from_min_size(ui.cursor().min, vec2(64.0, 24.0));
                ui.allocate_rect(done, Sense::hover());
                if flat_button(
                    ui,
                    done,
                    egui::Id::unique(("timer done", id)),
                    "Done",
                    FontId::proportional(11.5),
                    Flat::Plain,
                    true,
                )
                .clicked()
                {
                    state.timer_open = None;
                }
            }
        });
    }
    // The readout of a running timer moves on once a second, and only while one runs.
    if let Some(wait) = next_tick {
        ui.ctx().request_repaint_after(wait + Duration::from_millis(5));
    }
    match asked {
        Some((index, TimerAsk::Toggle)) => {
            let timer = &mut state.timers[index];
            if timer.running() {
                timer.stop(now);
            } else {
                timer.start(now);
            }
        }
        Some((index, TimerAsk::Reset)) => state.timers[index].reset(),
        Some((index, TimerAsk::Edit)) => {
            let id = state.timers[index].id;
            state.timer_open = if state.timer_open == Some(id) { None } else { Some(id) };
        }
        Some((index, TimerAsk::Delete)) => {
            state.timers.remove(index);
            state.timers_changed = true;
        }
        None => {}
    }
}

/// A timer's settings, under its card while they are open: its name, what it counts, and
/// whether it runs over. Returns true when one changed.
fn timer_settings(ui: &mut egui::Ui, timer: &mut Timer) -> bool {
    ui.add_space(8.0);
    let mut changed = false;
    egui::Grid::new(("timer settings", timer.id)).num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
        ui.label(egui::RichText::new("Name").color(TEXT_DIM));
        changed |= ui.add(egui::TextEdit::singleline(&mut timer.name).desired_width(160.0)).changed();
        ui.end_row();
        ui.label(egui::RichText::new("Counts").color(TEXT_DIM));
        let kind_name = match timer.kind {
            timers::Kind::Countdown { .. } => "Down from a length",
            timers::Kind::CountdownTo { .. } => "Down to a time of day",
            timers::Kind::Elapsed { .. } => "Up",
        };
        egui::ComboBox::from_id_salt(("timer type", timer.id)).selected_text(kind_name).width(160.0).show_ui(
            ui,
            |ui| {
                let choices = [
                    ("Down from a length", timers::Kind::Countdown { seconds: 300 }),
                    ("Down to a time of day", timers::Kind::CountdownTo { hour: 11, minute: 0, second: 0 }),
                    ("Up", timers::Kind::Elapsed { start: 0, end: None }),
                ];
                for (name, kind) in choices {
                    let same = std::mem::discriminant(&kind) == std::mem::discriminant(&timer.kind);
                    if ui.add(egui::Button::selectable(same, name)).clicked() && !same {
                        timer.kind = kind;
                        timer.reset();
                        changed = true;
                    }
                }
            },
        );
        ui.end_row();
        match timer.kind {
            timers::Kind::Countdown { seconds } => {
                ui.label(egui::RichText::new("Length").color(TEXT_DIM));
                if let Some(Some(seconds)) =
                    clock_field(ui, ui.scope_id().with(("duration", timer.id)), Some(seconds), "00:05:00")
                {
                    timer.kind = timers::Kind::Countdown { seconds };
                    changed = true;
                }
                ui.end_row();
            }
            timers::Kind::CountdownTo { hour, minute, second } => {
                ui.label(egui::RichText::new("Time").color(TEXT_DIM));
                let current = u32::from(hour) * 3600 + u32::from(minute) * 60 + u32::from(second);
                ui.horizontal(|ui| {
                    if let Some(Some(at)) =
                        clock_field(ui, ui.scope_id().with(("time", timer.id)), Some(current), "11:00:00")
                    {
                        let at = at % (24 * 3600);
                        timer.kind = timers::Kind::CountdownTo {
                            hour: u8::try_from(at / 3600).unwrap_or(0),
                            minute: u8::try_from(at / 60 % 60).unwrap_or(0),
                            second: u8::try_from(at % 60).unwrap_or(0),
                        };
                        changed = true;
                    }
                    ui.label(egui::RichText::new("24-hour").size(11.0).color(TEXT_FAINT));
                });
                ui.end_row();
            }
            timers::Kind::Elapsed { start, end } => {
                ui.label(egui::RichText::new("From").color(TEXT_DIM));
                if let Some(Some(start)) =
                    clock_field(ui, ui.scope_id().with(("start", timer.id)), Some(start), "00:00:00")
                {
                    timer.kind = timers::Kind::Elapsed { start, end };
                    changed = true;
                }
                ui.end_row();
                ui.label(egui::RichText::new("Stops at").color(TEXT_DIM));
                if let Some(end) = clock_field(ui, ui.scope_id().with(("end", timer.id)), end, "Never") {
                    timer.kind = timers::Kind::Elapsed { start, end };
                    changed = true;
                }
                ui.end_row();
            }
        }
    });
    changed |= ui.checkbox(&mut timer.allow_overrun, "Runs over (counts on past the end)").changed();
    changed
}

/// Something asked of a message from its card.
enum MessageAsk {
    Toggle,
    Edit,
    Rename,
    Duplicate,
    Cut,
    Copy,
    Paste,
    Delete,
}

/// A message's menu, behind its ⋯ and a right-click on its card.
fn message_menu(ui: &mut egui::Ui, open: bool, has_clip: bool) -> Option<MessageAsk> {
    let mut asked = None;
    if ui.add(egui::Button::selectable(open, "Edit…")).clicked() {
        asked = Some(MessageAsk::Edit);
    }
    if ui.button("Rename").clicked() {
        asked = Some(MessageAsk::Rename);
    }
    if ui.button("Duplicate").clicked() {
        asked = Some(MessageAsk::Duplicate);
    }
    ui.separator();
    if ui.button("Cut").clicked() {
        asked = Some(MessageAsk::Cut);
    }
    if ui.button("Copy").clicked() {
        asked = Some(MessageAsk::Copy);
    }
    if ui.add_enabled(has_clip, egui::Button::new("Paste")).clicked() {
        asked = Some(MessageAsk::Paste);
    }
    ui.separator();
    if ui.button("Delete").clicked() {
        asked = Some(MessageAsk::Delete);
    }
    asked
}

/// What a message's card says under its name: its words on one line, when it leaves by itself,
/// and its look.
fn message_summary(message: &Message) -> String {
    let words = message.text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut parts = vec![if words.is_empty() { "No words yet".to_owned() } else { words }];
    if let Some(seconds) = message.dismiss_after {
        parts.push(format!("leaves after {seconds} s"));
    }
    parts.push(message.look.name().to_owned());
    parts.join(" \u{b7} ")
}

/// The name of the message that words shown once are kept as, as `sp message show --text` keeps
/// them.
const QUICK_MESSAGE: &str = "Quick message";

/// "Words once…" in the Messages heading, left of its +: a small box to type words in and show
/// them over the slides at once. They are kept as the message Quick message, to show again.
fn words_once(ui: &mut egui::Ui, row: Rect, state: &mut HomeState) {
    let font = FontId::proportional(11.5);
    let wide = flat_button_width(ui, &["Words once…"], &font, 9.0);
    let rect = Rect::from_min_size(pos2(row.max.x - 28.0 - 8.0 - wide, row.center().y - 12.0), vec2(wide, 24.0));
    let button = flat_button(ui, rect, egui::Id::unique("words once"), "Words once…", font, Flat::Plain, true)
        .on_hover_text("Type some words and show them over the slides at once");
    let draft = egui::Id::unique("words once draft");
    egui::Popup::from_toggle_button_response(&button)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            ui.set_width(250.0);
            let mut words = ui.data_mut(|data| data.get_temp::<String>(draft)).unwrap_or_default();
            let field = ui.add(
                egui::TextEdit::multiline(&mut words)
                    .desired_rows(2)
                    .desired_width(f32::INFINITY)
                    .hint_text("Words to show over the slides"),
            );
            if !field.has_focus() && words.is_empty() {
                field.request_focus();
            }
            ui.data_mut(|data| data.insert_temp(draft, words.clone()));
            ui.label(
                egui::RichText::new(format!("Kept as the message {QUICK_MESSAGE}, to show again or change."))
                    .size(11.0)
                    .color(TEXT_FAINT),
            );
            let (place, _) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::hover());
            let show = Rect::from_min_size(pos2(place.max.x - 64.0, place.min.y), vec2(64.0, 28.0));
            let can = !words.trim().is_empty();
            let id = egui::Id::unique("words once show");
            if flat_button(ui, show, id, "Show", FontId::proportional(12.5), Flat::Main, can).clicked() {
                let at = match state.messages.iter().position(|message| message.name == QUICK_MESSAGE) {
                    Some(at) => at,
                    None => {
                        state.messages.push(Message::new(QUICK_MESSAGE.to_owned()));
                        state.messages.len() - 1
                    }
                };
                state.messages[at].text = words.trim().to_owned();
                state.messages_changed = true;
                state.message_live = Some((state.messages[at].id, Instant::now()));
                ui.data_mut(|data| data.remove::<String>(draft));
                ui.close();
            }
        });
}

fn messages_tab(ui: &mut egui::Ui, state: &mut HomeState) {
    let (row, add) = heading(ui, "Messages", Some("New message"));
    words_once(ui, row, state);
    if add {
        let taken = |name: &str| state.messages.iter().any(|message| message.name == name);
        let name = (1..).map(|n| format!("New Message {n}")).find(|name| !taken(name)).unwrap_or_default();
        let message = Message::new(name);
        state.message_selected = Some(message.id);
        state.messages.push(message);
        state.messages_changed = true;
    }
    if state.messages.is_empty() {
        empty_note(ui, "No messages. A message shows over the slides, such as a notice for parents.");
        return;
    }
    let live = state.message_live.map(|(id, _)| id);
    let has_clip = state.message_clip.is_some();
    let mut asked: Option<(usize, MessageAsk)> = None;
    let mut renamed: Option<Option<(sp_model::Uuid, String)>> = None;
    let mut rename_started = std::mem::take(&mut state.rename_started);
    let timer_names: Vec<String> = state.timers.iter().map(|timer| timer.name.clone()).collect();
    let button_font = FontId::proportional(12.5);
    let button_wide = flat_button_width(ui, &["Show", "Hide"], &button_font, 13.0);
    for index in 0..state.messages.len() {
        let id = state.messages[index].id;
        let showing = live == Some(id);
        let open = state.message_selected == Some(id);
        card(ui, if showing { CARD_SHOWN } else { CARD }, |ui| {
            let message = &state.messages[index];
            let width = ui.available_width();
            let text_wide = width - CARD_KEY - button_wide - 2.0 * CARD_GAP;
            let name = fitted(ui, &message.name, name_font(), TEXT, text_wide, 2);
            let sub = fitted(ui, &message_summary(message), FontId::proportional(12.0), TEXT_FAINT, text_wide, 1);
            let block = name.size().y + sub.size().y;
            let (row, _) = ui.allocate_exact_size(vec2(width, block.max(CARD_KEY)), Sense::hover());
            let back = card_back(ui, row, egui::Id::unique(("message card", id)));
            let top = row.center().y - block / 2.0;
            let (name_tall, first_line) = (name.size().y, name.rows.first().map_or(15.0, |row| row.height()));
            match state.message_rename.as_mut().filter(|(renaming, _)| *renaming == id) {
                Some((_, typed)) => {
                    let start = pos2(row.min.x, top + first_line / 2.0);
                    let right = row.min.x + text_wide;
                    let field = crate::studio::rename_field_at(ui, typed, name_font(), TEXT, ACCENT, start, right);
                    if std::mem::take(&mut rename_started) {
                        field.request_focus();
                    }
                    let (enter, escape) =
                        ui.input(|input| (input.key_pressed(egui::Key::Enter), input.key_pressed(egui::Key::Escape)));
                    if escape {
                        renamed = Some(None);
                    } else if enter || field.lost_focus() {
                        renamed = Some(Some((id, typed.clone())));
                    }
                }
                None => {
                    ui.painter().galley(pos2(row.min.x, top), name, TEXT);
                }
            }
            ui.painter().galley(pos2(row.min.x, top + name_tall), sub, TEXT_FAINT);
            let y = row.center().y;
            let more = Rect::from_min_size(pos2(row.max.x - CARD_KEY, y - CARD_KEY / 2.0), vec2(CARD_KEY, CARD_KEY));
            let button =
                Rect::from_min_size(pos2(more.min.x - CARD_GAP - button_wide, y - 15.0), vec2(button_wide, 30.0));
            let (words, look, hint) = if showing {
                ("Hide", Flat::On, "Take this message off the screens (F6)")
            } else {
                ("Show", Flat::Plain, "Show this message over the slides on the audience screens")
            };
            let toggle = egui::Id::unique(("message show", id));
            if flat_button(ui, button, toggle, words, button_font.clone(), look, true).on_hover_text(hint).clicked() {
                asked = Some((index, MessageAsk::Toggle));
            }
            let more = more_key(ui, more, egui::Id::unique(("message more", id)));
            egui::Popup::menu(&more).show(|ui| {
                if let Some(ask) = message_menu(ui, open, has_clip) {
                    asked = Some((index, ask));
                }
            });
            if back.double_clicked() {
                asked = Some((index, MessageAsk::Edit));
            }
            back.context_menu(|ui| {
                if let Some(ask) = message_menu(ui, open, has_clip) {
                    asked = Some((index, ask));
                }
            });
            if open {
                state.messages_changed |= message_settings(ui, &mut state.messages[index], &timer_names);
                ui.add_space(6.0);
                let done = Rect::from_min_size(ui.cursor().min, vec2(64.0, 24.0));
                ui.allocate_rect(done, Sense::hover());
                let font = FontId::proportional(11.5);
                if flat_button(ui, done, egui::Id::unique(("message done", id)), "Done", font, Flat::Plain, true)
                    .clicked()
                {
                    state.message_selected = None;
                }
            }
        });
    }
    state.rename_started = rename_started;
    match renamed {
        Some(Some((id, name))) => {
            if let Some(message) = state.messages.iter_mut().find(|message| message.id == id)
                && !name.trim().is_empty()
            {
                message.name = name.trim().to_owned();
                state.messages_changed = true;
            }
            state.message_rename = None;
        }
        Some(None) => state.message_rename = None,
        None => {}
    }
    let remove = |state: &mut HomeState, index: usize| {
        let gone = state.messages.remove(index);
        if live == Some(gone.id) {
            state.message_live = None;
        }
        state.messages_changed = true;
        gone
    };
    let Some((index, ask)) = asked else { return };
    match ask {
        MessageAsk::Toggle => {
            let id = state.messages[index].id;
            state.message_live = if live == Some(id) { None } else { Some((id, Instant::now())) };
        }
        MessageAsk::Edit => {
            let id = state.messages[index].id;
            state.message_selected = if state.message_selected == Some(id) { None } else { Some(id) };
        }
        MessageAsk::Rename => {
            state.message_rename = Some((state.messages[index].id, state.messages[index].name.clone()));
            state.rename_started = true;
        }
        MessageAsk::Duplicate => {
            let copy = state.messages[index].duplicate();
            state.messages.insert(index + 1, copy);
            state.messages_changed = true;
        }
        MessageAsk::Copy => state.message_clip = Some(state.messages[index].clone()),
        MessageAsk::Cut => state.message_clip = Some(remove(state, index)),
        MessageAsk::Paste => {
            if let Some(clip) = &state.message_clip {
                // A pasted message is a new one: it gets an id of its own, so it can be pasted again.
                let pasted = Message { id: sp_model::Uuid::new_v4(), ..clip.clone() };
                state.messages.insert(index + 1, pasted);
                state.messages_changed = true;
            }
        }
        MessageAsk::Delete => {
            remove(state, index);
        }
    }
}

/// A message's settings, under its card while they are open: its name, its words, the fill-in
/// tokens, its look and when it leaves. Returns true when one changed.
fn message_settings(ui: &mut egui::Ui, message: &mut Message, timer_names: &[String]) -> bool {
    ui.add_space(8.0);
    let mut changed = false;
    changed |= ui.add(egui::TextEdit::singleline(&mut message.name).desired_width(f32::INFINITY)).changed();
    changed |= ui
        .add(
            egui::TextEdit::multiline(&mut message.text)
                .desired_rows(3)
                .desired_width(f32::INFINITY)
                .hint_text("Words to show"),
        )
        .changed();
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt(("add token", message.id)).selected_text("Add Token…").width(130.0).show_ui(
            ui,
            |ui| {
                if ui.button("System Clock").clicked() {
                    message.text.push_str(crate::messages::CLOCK_TOKEN);
                    changed = true;
                }
                for name in timer_names {
                    if ui.button(format!("Timer: {name}")).clicked() {
                        message.text.push_str(&crate::messages::timer_token(name));
                        changed = true;
                    }
                }
            },
        );
        egui::ComboBox::from_id_salt(("message look", message.id))
            .selected_text(message.look.name())
            .width(150.0)
            .show_ui(ui, |ui| {
                for look in crate::messages::Look::ALL {
                    if ui.add(egui::Button::selectable(message.look == look, look.name())).clicked() {
                        message.look = look;
                        changed = true;
                    }
                }
            });
    });
    let leaves = match message.dismiss_after {
        None => "Stays until taken off".to_owned(),
        Some(seconds) => format!("Leaves after {seconds} seconds"),
    };
    egui::ComboBox::from_id_salt(("dismiss", message.id)).selected_text(leaves).width(180.0).show_ui(ui, |ui| {
        for choice in [None, Some(5), Some(10), Some(15), Some(20), Some(30), Some(45), Some(60)] {
            let label =
                choice.map_or("Stays until taken off".to_owned(), |seconds| format!("Leaves after {seconds} seconds"));
            if ui.add(egui::Button::selectable(message.dismiss_after == choice, label)).clicked() {
                message.dismiss_after = choice;
                changed = true;
            }
        }
    });
    changed
}

/// Asks for a picture of a slide; see [`ThumbOf`].
type Thumb<'t> = dyn FnMut(ThumbOf) -> Option<(egui::TextureId, bool)> + 't;

/// A number that changes when any of `parts` does, to tell when a picture must be drawn again.
fn signature(parts: &[&str]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    parts.hash(&mut hasher);
    hasher.finish()
}

/// A small picture of a slide drawn with a theme of its own, in `rect` with round corners: a
/// prop's, or a stage layout's. `key` keeps it apart from other pictures of the same slide;
/// it is drawn again only when `said` (a [`signature`] of what it shows) is not what it was
/// when it was last drawn, and is black until it is ready.
#[allow(clippy::too_many_arguments)]
fn own_picture(
    ui: &egui::Ui,
    thumb: &mut Thumb,
    rect: Rect,
    corner: f32,
    key: sp_model::Uuid,
    slide: &sp_model::Slide,
    theme: &sp_model::Theme,
    said: u64,
) {
    let drawn = egui::Id::unique(("own picture drawn", key));
    let again = ui.data(|data| data.get_temp::<u64>(drawn)) != Some(said);
    let kept = sp_model::Slide { id: key, ..slide.clone() };
    ui.painter().rect_filled(rect, corner, Color32::BLACK);
    match thumb(ThumbOf::Own(&kept, theme, again)) {
        Some((texture, _)) => {
            ui.data_mut(|data| data.insert_temp(drawn, said));
            egui::Image::from_texture((texture, rect.size())).corner_radius(corner).paint_at(ui, rect);
        }
        None => {
            // Forgotten already: the next frame draws it from fresh.
            ui.data_mut(|data| data.insert_temp(drawn, said));
            ui.ctx().request_repaint();
        }
    }
}

/// The theme a prop's picture is drawn with: the built-in one's boxes on black, as a prop lies
/// over whatever is on screen.
fn prop_theme() -> &'static sp_model::Theme {
    static THEME: std::sync::OnceLock<sp_model::Theme> = std::sync::OnceLock::new();
    THEME.get_or_init(|| sp_model::Theme {
        id: sp_model::Uuid::from_u128(0x7072_6f70_0000_0000_0000_0000_0000_0001),
        name: "Props".to_owned(),
        background: None,
        background_picture: None,
        ..sp_model::Theme::builtin()
    })
}

/// Something asked of a prop from its card.
enum PropAsk {
    Toggle,
    Edit,
    Rename,
    Duplicate,
    AutoClear(Option<u32>),
    Delete,
}

/// A prop's menu, behind its ⋯ and a right-click on its card.
fn prop_menu(ui: &mut egui::Ui, auto_clear: Option<u32>) -> Option<PropAsk> {
    let mut asked = None;
    if ui.button("Edit in the Editor").clicked() {
        asked = Some(PropAsk::Edit);
    }
    ui.menu_button("Clears Itself", |ui| {
        for choice in [None, Some(5), Some(10), Some(15), Some(20), Some(30), Some(45), Some(60)] {
            let label = choice.map_or("Never".to_owned(), |seconds| format!("After {seconds} seconds"));
            if ui.add(egui::Button::selectable(auto_clear == choice, label)).clicked() {
                asked = Some(PropAsk::AutoClear(choice));
                ui.close();
            }
        }
    });
    if ui.button("Rename").clicked() {
        asked = Some(PropAsk::Rename);
    }
    if ui.button("Duplicate").clicked() {
        asked = Some(PropAsk::Duplicate);
    }
    ui.separator();
    if ui.button("Delete").clicked() {
        asked = Some(PropAsk::Delete);
    }
    asked
}

fn props_tab(ui: &mut egui::Ui, state: &mut HomeState, thumb: &mut Thumb) {
    use crate::props::Prop;
    let (_, add) = heading(ui, "Props", Some("New prop"));
    if add {
        let taken = |name: &str| state.props.iter().any(|prop| prop.name == name);
        let name = (1..).map(|n| format!("Prop {n}")).find(|name| !taken(name)).unwrap_or_default();
        let prop = Prop::new(name.clone());
        state.prop_rename = Some((prop.id, name));
        state.rename_started = true;
        state.props.push(prop);
        state.props_changed = true;
    }
    if state.props.is_empty() {
        empty_note(ui, "No props. A prop stays over the slides until it is taken off, such as a name in a corner.");
        return;
    }
    let mut asked: Option<(usize, PropAsk)> = None;
    let mut renamed: Option<Option<(sp_model::Uuid, String)>> = None;
    let mut rename_started = std::mem::take(&mut state.rename_started);
    let button_font = FontId::proportional(12.5);
    let button_wide = flat_button_width(ui, &["Show", "Hide"], &button_font, 13.0);
    // The picture, 16 to 9.
    let picture = vec2(62.0, 62.0 * 9.0 / 16.0);
    for index in 0..state.props.len() {
        let id = state.props[index].id;
        let showing = state.props_live.iter().any(|(live, _)| *live == id);
        card(ui, if showing { CARD_SHOWN } else { CARD }, |ui| {
            let prop = &state.props[index];
            let width = ui.available_width();
            let text_wide = width - picture.x - CARD_KEY - button_wide - 3.0 * CARD_GAP;
            let name = fitted(ui, &prop.name, name_font(), TEXT, text_wide, 2);
            let leaves = match prop.auto_clear {
                None => "Stays until taken off".to_owned(),
                Some(seconds) => format!("Clears itself after {seconds} s"),
            };
            let sub = fitted(ui, &leaves, FontId::proportional(12.0), TEXT_FAINT, text_wide, 1);
            let block = name.size().y + sub.size().y;
            let (row, _) = ui.allocate_exact_size(vec2(width, block.max(CARD_KEY).max(picture.y)), Sense::hover());
            let back = card_back(ui, row, egui::Id::unique(("prop card", id)));
            let y = row.center().y;
            let place = Rect::from_min_size(pos2(row.min.x, y - picture.y / 2.0), picture);
            let key = sp_model::Uuid::from_u128(prop.id.as_u128() ^ 0x7072_6f70_7069_6374_7572_6500_0000_0000);
            let said = signature(&[&serde_json::to_string(&prop.slide).unwrap_or_default()]);
            own_picture(ui, thumb, place, 4.0, key, &prop.slide, prop_theme(), said);
            let left = place.max.x + CARD_GAP;
            let top = y - block / 2.0;
            let (name_tall, first_line) = (name.size().y, name.rows.first().map_or(15.0, |row| row.height()));
            match state.prop_rename.as_mut().filter(|(renaming, _)| *renaming == id) {
                Some((_, typed)) => {
                    let start = pos2(left, top + first_line / 2.0);
                    let field =
                        crate::studio::rename_field_at(ui, typed, name_font(), TEXT, ACCENT, start, left + text_wide);
                    if std::mem::take(&mut rename_started) {
                        field.request_focus();
                    }
                    let (enter, escape) =
                        ui.input(|input| (input.key_pressed(egui::Key::Enter), input.key_pressed(egui::Key::Escape)));
                    if escape {
                        renamed = Some(None);
                    } else if enter || field.lost_focus() {
                        renamed = Some(Some((id, typed.clone())));
                    }
                }
                None => {
                    ui.painter().galley(pos2(left, top), name, TEXT);
                }
            }
            ui.painter().galley(pos2(left, top + name_tall), sub, TEXT_FAINT);
            let more = Rect::from_min_size(pos2(row.max.x - CARD_KEY, y - CARD_KEY / 2.0), vec2(CARD_KEY, CARD_KEY));
            let button =
                Rect::from_min_size(pos2(more.min.x - CARD_GAP - button_wide, y - 15.0), vec2(button_wide, 30.0));
            let (words, look, hint) = if showing {
                ("Hide", Flat::On, "Take this prop off the screens")
            } else {
                ("Show", Flat::Plain, "Show this prop over the slides on the audience screens")
            };
            let toggle = egui::Id::unique(("prop show", id));
            if flat_button(ui, button, toggle, words, button_font.clone(), look, true).on_hover_text(hint).clicked() {
                asked = Some((index, PropAsk::Toggle));
            }
            let auto_clear = prop.auto_clear;
            let more = more_key(ui, more, egui::Id::unique(("prop more", id)));
            egui::Popup::menu(&more).show(|ui| {
                if let Some(ask) = prop_menu(ui, auto_clear) {
                    asked = Some((index, ask));
                }
            });
            if back.double_clicked() {
                asked = Some((index, PropAsk::Edit));
            }
            back.on_hover_text("Double-click to change it in the editor").context_menu(|ui| {
                if let Some(ask) = prop_menu(ui, auto_clear) {
                    asked = Some((index, ask));
                }
            });
        });
    }
    state.rename_started = rename_started;
    match renamed {
        Some(Some((id, name))) => {
            if let Some(prop) = state.props.iter_mut().find(|prop| prop.id == id)
                && !name.trim().is_empty()
            {
                prop.name = name.trim().to_owned();
                state.props_changed = true;
            }
            state.prop_rename = None;
        }
        Some(None) => state.prop_rename = None,
        None => {}
    }
    let Some((index, ask)) = asked else { return };
    let id = state.props[index].id;
    match ask {
        PropAsk::Toggle => {
            if state.props_live.iter().any(|(live, _)| *live == id) {
                state.props_live.retain(|(live, _)| *live != id);
            } else {
                state.props_live.push((id, Instant::now()));
            }
        }
        PropAsk::Edit => {
            // The prop opens in the editor, as a slide of its own.
            state.editing = crate::studio::prop_draft(state, index);
            state.bible_open = false;
            state.reflow = None;
        }
        PropAsk::Rename => {
            state.prop_rename = Some((id, state.props[index].name.clone()));
            state.rename_started = true;
        }
        PropAsk::Duplicate => {
            let copy = state.props[index].duplicate();
            state.props.insert(index + 1, copy);
            state.props_changed = true;
        }
        PropAsk::AutoClear(choice) => {
            state.props[index].auto_clear = choice;
            state.props_changed = true;
        }
        PropAsk::Delete => {
            state.props.remove(index);
            state.props_live.retain(|(live, _)| *live != id);
            state.props_changed = true;
        }
    }
}

/// The title of the song an action names by its key, among the songs in the library.
fn song_title(show: &sp_engine::Show, state: &HomeState, key: &str) -> Option<String> {
    let hidden = state.bible.slots.unwrap_or([usize::MAX; 2]);
    listed_songs(show, state, hidden).find(|(have, _)| *have == key).map(|(_, song)| song.title.clone())
}

/// The songs an action can name, each with its key: those in the library, in order.
fn listed_songs<'a>(
    show: &'a sp_engine::Show,
    state: &'a HomeState,
    hidden: [usize; 2],
) -> impl Iterator<Item = (&'a str, &'a sp_model::Presentation)> {
    state
        .song_keys
        .iter()
        .enumerate()
        .filter(move |(index, _)| !hidden.contains(index) && !state.removed_songs.contains(index))
        .filter_map(|(index, key)| Some((key.as_str(), &**show.presentations().get(index)?)))
}

/// An action in words. Songs and pictures are looked up only for the actions that name one, so
/// the Macros tab doesn't copy the whole library on every frame.
pub fn describe_action(show: &sp_engine::Show, state: &HomeState, action: &sp_model::ShowAction) -> String {
    crate::macros::describe(
        action,
        |id| state.messages.iter().find(|m| m.id == id).map(|m| m.name.clone()),
        |id| state.props.iter().find(|p| p.id == id).map(|p| p.name.clone()),
        |id| state.timers.iter().find(|t| t.id == id).map(|t| t.name.clone()),
        |key| song_title(show, state, key),
        |id| show.media().iter().find(|item| item.id == id).map(|item| item.name.clone()),
    )
}

/// The menu of actions to add: clearing, messages, props, timers, the audience screens, songs.
/// Returns the one chosen.
pub fn add_action_menu(ui: &mut egui::Ui, show: &sp_engine::Show, state: &HomeState) -> Option<sp_model::ShowAction> {
    use sp_model::{ClearWhat, ShowAction, TimerDo};
    let mut chosen = None;
    ui.menu_button("Clear", |ui| {
        for what in ClearWhat::ALL {
            if ui.button(what.name()).clicked() {
                chosen = Some(ShowAction::Clear { what });
            }
        }
    });
    ui.menu_button("Message", |ui| {
        if state.messages.is_empty() {
            ui.label("No messages yet");
        }
        for message in &state.messages {
            for show in [true, false] {
                let label = format!("{} {}", if show { "Show" } else { "Hide" }, message.name);
                if ui.button(label).clicked() {
                    chosen = Some(ShowAction::Message { id: message.id, show });
                }
            }
        }
    });
    ui.menu_button("Prop", |ui| {
        if state.props.is_empty() {
            ui.label("No props yet");
        }
        for prop in &state.props {
            for show in [true, false] {
                let label = format!("{} {}", if show { "Show" } else { "Hide" }, prop.name);
                if ui.button(label).clicked() {
                    chosen = Some(ShowAction::Prop { id: prop.id, show });
                }
            }
        }
    });
    ui.menu_button("Timer", |ui| {
        if state.timers.is_empty() {
            ui.label("No timers yet");
        }
        for timer in &state.timers {
            for act in TimerDo::ALL {
                if ui.button(format!("{} {}", act.name(), timer.name)).clicked() {
                    chosen = Some(ShowAction::Timer { id: timer.id, act });
                }
            }
        }
    });
    ui.menu_button("Audience Screens", |ui| {
        for on in [true, false] {
            if ui.button(if on { "On" } else { "Off" }).clicked() {
                chosen = Some(ShowAction::Audience { on });
            }
        }
    });
    ui.menu_button("Media", |ui| {
        if show.media().is_empty() {
            ui.label("No pictures in the media bin yet");
        }
        for item in show.media() {
            if ui.button(format!("Show {}", item.name)).clicked() {
                chosen = Some(ShowAction::Media { id: item.id });
            }
        }
    });
    ui.menu_button("Go to Song", |ui| {
        egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
            let hidden = state.bible.slots.unwrap_or([usize::MAX; 2]);
            for (key, song) in listed_songs(show, state, hidden) {
                if ui.button(song.title.as_str()).clicked() {
                    chosen = Some(ShowAction::Song { key: key.to_owned() });
                }
            }
        });
    });
    chosen
}

/// Something asked of a macro from its card.
enum MacroAsk {
    Run,
    Steps,
    Rename,
    Color(sp_model::Rgba),
    Startup,
    Add(sp_model::ShowAction),
    Earlier(usize),
    Remove(usize),
    Duplicate,
    Delete,
}

/// A macro's menu, behind its ⋯ and a right-click on its card.
fn macro_menu(ui: &mut egui::Ui, made: &crate::macros::Macro, open: bool) -> Option<MacroAsk> {
    let mut asked = None;
    if ui.add(egui::Button::selectable(open, "Edit Steps…")).clicked() {
        asked = Some(MacroAsk::Steps);
    }
    ui.menu_button("Colour", |ui| {
        for (name, color) in crate::macros::COLORS {
            if ui.add(egui::Button::selectable(made.color == color, name)).clicked() {
                asked = Some(MacroAsk::Color(color));
                ui.close();
            }
        }
    });
    if ui.add(egui::Button::selectable(made.on_startup, "Run When SoftPresenter Opens")).clicked() {
        asked = Some(MacroAsk::Startup);
    }
    if ui.button("Rename").clicked() {
        asked = Some(MacroAsk::Rename);
    }
    if ui.button("Duplicate").clicked() {
        asked = Some(MacroAsk::Duplicate);
    }
    ui.separator();
    if ui.button("Delete").clicked() {
        asked = Some(MacroAsk::Delete);
    }
    asked
}

/// The room inside a macro's card, round its first line.
const MACRO_MARGIN: egui::Margin = egui::Margin { left: 12, right: 4, top: 6, bottom: 6 };

fn macros_tab(ui: &mut egui::Ui, show: &sp_engine::Show, state: &mut HomeState) {
    use crate::macros::Macro;
    // The macro whose steps are open under it, kept by the window.
    let open_id = egui::Id::unique("macro steps open");
    let mut open_macro = ui.data(|data| data.get_temp::<sp_model::Uuid>(open_id));
    let (_, add) = heading(ui, "Macros", Some("New macro"));
    if add {
        let taken = |name: &str| state.macros.iter().any(|m| m.name == name);
        let name = (1..).map(|n| format!("Macro {n}")).find(|name| !taken(name)).unwrap_or_default();
        let made = Macro::new(name.clone());
        state.macro_rename = Some((made.id, name));
        state.rename_started = true;
        open_macro = Some(made.id);
        state.macros.push(made);
        state.macros_changed = true;
    }
    if state.macros.is_empty() {
        empty_note(
            ui,
            "No macros. A macro does several things at once, such as clearing the screen and starting a timer.",
        );
        return;
    }
    let mut asked: Option<(usize, MacroAsk)> = None;
    let mut renamed: Option<Option<(sp_model::Uuid, String)>> = None;
    let mut rename_started = std::mem::take(&mut state.rename_started);
    let run_font = semibold(12.5);
    let run_wide =
        12.0 + 10.0 + 6.0 + ui.painter().layout_no_wrap("Run".to_owned(), run_font.clone(), TEXT).size().x + 12.0;
    for index in 0..state.macros.len() {
        let made = &state.macros[index];
        let id = made.id;
        let open = open_macro == Some(id);
        let steps: Vec<String> = made.actions.iter().map(|action| describe_action(show, state, action)).collect();
        egui::Frame::new().fill(CARD).corner_radius(RADIUS_CARD as u8).inner_margin(MACRO_MARGIN).show(ui, |ui| {
            ui.set_width(ui.available_width());
            let made = &state.macros[index];
            let width = ui.available_width();
            let (row, _) = ui.allocate_exact_size(vec2(width, 34.7), Sense::hover());
            let reach = Rect::from_min_max(
                row.min - vec2(f32::from(MACRO_MARGIN.left), f32::from(MACRO_MARGIN.top)),
                row.max + vec2(f32::from(MACRO_MARGIN.right), f32::from(MACRO_MARGIN.bottom)),
            );
            let back = ui.interact(reach, egui::Id::unique(("macro card", id)), Sense::click());
            let y = row.center().y;
            // From the right: ⋯, the chevron that opens the steps, and Run.
            let more = Rect::from_min_size(pos2(row.max.x - 28.0, y - 14.0), vec2(28.0, 28.0));
            let chevron = Rect::from_min_size(pos2(more.min.x - 4.0 - 26.0, y - 14.0), vec2(26.0, 28.0));
            let run = Rect::from_min_size(pos2(chevron.min.x - 6.0 - run_wide, y - 15.0), vec2(run_wide, 30.0));
            let text_wide = run.min.x - 8.0 - row.min.x;
            // The name with the macro's colour as a dot before it, and under it how many steps.
            let colour = crate::home::color32(made.color);
            let name_left = row.min.x + 9.0 + 7.0;
            let name = fitted(ui, &made.name, name_font(), TEXT, text_wide - 16.0, 1);
            let count = made.actions.len();
            let mut said = if count == 1 { "1 step".to_owned() } else { format!("{count} steps") };
            if made.on_startup {
                said.push_str(" \u{b7} when SoftPresenter opens");
            }
            let sub = fitted(ui, &said, FontId::proportional(12.0), TEXT_FAINT, text_wide, 1);
            let name_y = row.min.y + 17.5 / 2.0;
            ui.painter().circle_filled(pos2(row.min.x + 4.5, name_y), 4.5, colour);
            match state.macro_rename.as_mut().filter(|(renaming, _)| *renaming == id) {
                Some((_, typed)) => {
                    let start = pos2(name_left, name_y);
                    let field = crate::studio::rename_field_at(
                        ui,
                        typed,
                        name_font(),
                        TEXT,
                        ACCENT,
                        start,
                        row.min.x + text_wide,
                    );
                    if std::mem::take(&mut rename_started) {
                        field.request_focus();
                    }
                    let (enter, escape) =
                        ui.input(|input| (input.key_pressed(egui::Key::Enter), input.key_pressed(egui::Key::Escape)));
                    if escape {
                        renamed = Some(None);
                    } else if enter || field.lost_focus() {
                        renamed = Some(Some((id, typed.clone())));
                    }
                }
                None => {
                    ui.painter().galley(pos2(name_left, name_y - name.size().y / 2.0), name, TEXT);
                }
            }
            ui.painter().galley(pos2(row.min.x, row.min.y + 18.5), sub, TEXT_FAINT);
            // Run: the only thing that runs the macro.
            let run_response = ui.interact(run, egui::Id::unique(("macro run", id)), Sense::click());
            crate::studio::paint_flat(ui.painter(), run, 9.0, Flat::Plain, run_response.hovered());
            let word = ui.painter().layout_no_wrap("Run".to_owned(), run_font.clone(), TEXT);
            let icon = Rect::from_center_size(pos2(run.min.x + 12.0 + 5.0, y), vec2(10.0, 10.0));
            crate::icons::paint(ui.painter(), Icon::Play, icon, TEXT);
            ui.painter().galley(pos2(icon.max.x + 6.0, y - word.size().y / 2.0), word, TEXT);
            if run_response.on_hover_text("Run this macro now").clicked() {
                asked = Some((index, MacroAsk::Run));
            }
            let glyph = if open { Icon::ChevronUp } else { Icon::ChevronDown };
            let hint = if open { "Hide its steps" } else { "Its steps" };
            if quiet_icon(ui, chevron, egui::Id::unique(("macro steps", id)), glyph, 13.0, 0.78, None)
                .on_hover_text(hint)
                .clicked()
            {
                asked = Some((index, MacroAsk::Steps));
            }
            let more = quiet_icon(ui, more, egui::Id::unique(("macro more", id)), Icon::More, 16.0, 0.6, None)
                .on_hover_text("More");
            egui::Popup::menu(&more).show(|ui| {
                if let Some(ask) = macro_menu(ui, made, open) {
                    asked = Some((index, ask));
                }
            });
            let summary = if steps.is_empty() { "No steps yet".to_owned() } else { steps.join("\n") };
            back.on_hover_text(summary).context_menu(|ui| {
                if let Some(ask) = macro_menu(ui, made, open) {
                    asked = Some((index, ask));
                }
            });
            if !open {
                return;
            }
            // Its steps, in the order they are done, each with Earlier and Remove; then a menu
            // of steps to add.
            ui.add_space(6.0);
            ui.spacing_mut().item_spacing.y = 2.0;
            let inner = width - 4.0;
            if steps.is_empty() {
                empty_note(ui, "No steps yet");
            }
            for (at, words) in steps.iter().enumerate() {
                let (line, _) = ui.allocate_exact_size(vec2(inner, 26.0), Sense::hover());
                let cy = line.center().y;
                let number = format!("{}", at + 1);
                ui.painter().text(pos2(line.min.x, cy), Align2::LEFT_CENTER, number, fixed(12.0), TEXT_FAINT);
                let remove = Rect::from_min_size(pos2(line.max.x - 26.0, cy - 13.0), vec2(26.0, 26.0));
                let earlier = remove.translate(vec2(-28.0, 0.0));
                let text =
                    fitted(ui, words, FontId::proportional(12.5), TEXT, earlier.min.x - 6.0 - line.min.x - 22.0, 1);
                ui.painter().galley(pos2(line.min.x + 22.0, cy - text.size().y / 2.0), text, TEXT);
                if at > 0
                    && quiet_icon(
                        ui,
                        earlier,
                        egui::Id::unique(("macro step earlier", id, at)),
                        Icon::ChevronUp,
                        12.0,
                        0.6,
                        Some(crate::studio::BUTTON),
                    )
                    .on_hover_text("Do this step earlier")
                    .clicked()
                {
                    asked = Some((index, MacroAsk::Earlier(at)));
                }
                if quiet_icon(
                    ui,
                    remove,
                    egui::Id::unique(("macro step remove", id, at)),
                    Icon::Close,
                    11.0,
                    0.6,
                    Some(crate::studio::BUTTON),
                )
                .on_hover_text("Remove this step")
                .clicked()
                {
                    asked = Some((index, MacroAsk::Remove(at)));
                }
            }
            ui.add_space(4.0);
            let (place, _) = ui.allocate_exact_size(vec2(inner, 28.0), Sense::hover());
            let adding = ui.interact(place, egui::Id::unique(("macro add step", id)), Sense::click());
            ui.painter().rect_filled(
                place,
                crate::studio::RADIUS,
                if adding.hovered() { crate::studio::BUTTON } else { PANEL },
            );
            ui.painter().text(
                place.left_center() + vec2(10.0, 0.0),
                Align2::LEFT_CENTER,
                "Add a step…",
                FontId::proportional(12.5),
                TEXT_DIM,
            );
            let mark = Rect::from_center_size(place.right_center() - vec2(14.0, 0.0), vec2(11.0, 11.0));
            crate::icons::paint(ui.painter(), Icon::ChevronDown, mark, TEXT_DIM);
            egui::Popup::menu(&adding).show(|ui| {
                if let Some(action) = add_action_menu(ui, show, state) {
                    asked = Some((index, MacroAsk::Add(action)));
                    ui.close();
                }
            });
            ui.add_space(3.0);
        });
    }
    state.rename_started = rename_started;
    match renamed {
        Some(Some((id, name))) => {
            if let Some(made) = state.macros.iter_mut().find(|m| m.id == id)
                && !name.trim().is_empty()
            {
                made.name = name.trim().to_owned();
                state.macros_changed = true;
            }
            state.macro_rename = None;
        }
        Some(None) => state.macro_rename = None,
        None => {}
    }
    if let Some((index, ask)) = asked {
        let id = state.macros[index].id;
        // Running a macro or opening its steps changes nothing to save.
        state.macros_changed |= !matches!(ask, MacroAsk::Run | MacroAsk::Steps);
        match ask {
            MacroAsk::Run => state.actions_to_run.extend(state.macros[index].actions.iter().cloned()),
            MacroAsk::Steps => open_macro = if open_macro == Some(id) { None } else { Some(id) },
            MacroAsk::Rename => {
                state.macro_rename = Some((id, state.macros[index].name.clone()));
                state.rename_started = true;
            }
            MacroAsk::Color(color) => state.macros[index].color = color,
            MacroAsk::Startup => state.macros[index].on_startup = !state.macros[index].on_startup,
            MacroAsk::Add(action) => state.macros[index].actions.push(action),
            MacroAsk::Earlier(at) => state.macros[index].actions.swap(at - 1, at),
            MacroAsk::Remove(at) => {
                state.macros[index].actions.remove(at);
            }
            MacroAsk::Duplicate => {
                let copy = state.macros[index].duplicate();
                state.macros.insert(index + 1, copy);
            }
            MacroAsk::Delete => {
                state.macros.remove(index);
            }
        }
    }
    ui.data_mut(|data| match open_macro {
        Some(id) => {
            data.insert_temp(open_id, id);
        }
        None => data.remove::<sp_model::Uuid>(open_id),
    });
}

/// A line to drag along, as the Audio tab's place in the track and its level are: a slim track,
/// filled up to `fraction` in `fill`, with a round handle there. Returns where along it the
/// pointer is while it is pressed or dragged (`None` otherwise), and the response, whose
/// `drag_stopped` or `clicked` says when to act.
fn drag_line(
    ui: &egui::Ui,
    rect: Rect,
    id: egui::Id,
    fraction: f32,
    fill: Color32,
    enabled: bool,
) -> (Option<f32>, egui::Response) {
    let sense = if enabled { Sense::click_and_drag() } else { Sense::hover() };
    let response = ui.interact(rect, id, sense);
    let held = response.is_pointer_button_down_on() || response.dragged();
    let at = (held || response.clicked() || response.drag_stopped())
        .then(|| response.interact_pointer_pos())
        .flatten()
        .map(|pointer| ((pointer.x - rect.min.x) / rect.width()).clamp(0.0, 1.0));
    let shown = at.unwrap_or(fraction).clamp(0.0, 1.0);
    let painter = ui.painter();
    let fade = if enabled { 1.0 } else { 0.4 };
    let track = Rect::from_center_size(rect.center(), vec2(rect.width(), 5.0));
    painter.rect_filled(track, 2.5, crate::studio::BUTTON_HOVER.gamma_multiply(fade));
    let filled = Rect::from_min_max(track.min, pos2(track.min.x + track.width() * shown, track.max.y));
    painter.rect_filled(filled, 2.5, fill.gamma_multiply(fade));
    let knob = pos2(track.min.x + track.width() * shown, track.center().y);
    painter.circle_filled(knob + vec2(0.0, 1.0), 7.5, Color32::from_black_alpha(90));
    painter.circle_filled(knob, 7.0, TEXT.gamma_multiply(fade));
    (at, response)
}

/// A round key of the audio player, `rect` wide (round, or a capsule when wider than tall):
/// the button grey, lighter under the pointer, or the ink for the big play key.
fn player_key(ui: &egui::Ui, rect: Rect, id: egui::Id, main: bool, enabled: bool) -> egui::Response {
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let response = ui.interact(rect, id, sense);
    let hovered = enabled && response.hovered();
    let fill = match (main, hovered) {
        (true, true) => Color32::WHITE,
        (true, false) => TEXT,
        (false, true) => crate::studio::BUTTON_HOVER,
        (false, false) => crate::studio::BUTTON,
    };
    ui.painter().rect_filled(rect, rect.height() / 2.0, if enabled { fill } else { fill.gamma_multiply(0.5) });
    response
}

/// Something asked of a track from its row.
enum TrackAsk {
    Play,
    Up,
    Down,
    Rename,
    Remove,
}

/// A track's menu, behind its ⋯ and a right-click on its row.
fn track_menu(ui: &mut egui::Ui, first: bool, last: bool) -> Option<TrackAsk> {
    let mut asked = None;
    if ui.button("Play").clicked() {
        asked = Some(TrackAsk::Play);
    }
    ui.separator();
    if ui.add_enabled(!first, egui::Button::new("Move Up")).clicked() {
        asked = Some(TrackAsk::Up);
    }
    if ui.add_enabled(!last, egui::Button::new("Move Down")).clicked() {
        asked = Some(TrackAsk::Down);
    }
    if ui.button("Rename").clicked() {
        asked = Some(TrackAsk::Rename);
    }
    ui.separator();
    if ui.button("Remove").clicked() {
        asked = Some(TrackAsk::Remove);
    }
    asked
}

/// The Audio tab's heading: the playlist shown, chosen from a drop-down, its ⋯ (Rename, New
/// Playlist, Delete) and + for a new playlist.
fn audio_heading(ui: &mut egui::Ui, state: &mut HomeState) {
    let (row, add) = heading(ui, "Audio", Some("New audio playlist"));
    if add {
        let made = crate::audio::Playlist::new(state.audio.new_name());
        state.audio.playlists.push(made);
        state.audio.current = state.audio.playlists.len() - 1;
        state.audio_changed = true;
    }
    let more = Rect::from_min_size(pos2(row.max.x - 28.0 - 8.0 - 28.0, row.min.y), vec2(28.0, 28.0));
    let select = Rect::from_min_size(pos2(more.min.x - 8.0 - 140.0, row.center().y - 12.0), vec2(140.0, 24.0));
    let renaming_id = egui::Id::unique("audio playlist renaming");
    let renaming = ui.data(|data| data.get_temp::<String>(renaming_id));
    let name = state.audio.shown().name.clone();
    match renaming {
        Some(mut typed) => {
            let start = pos2(select.min.x + 8.0, select.center().y);
            let field = crate::studio::rename_field_at(
                ui,
                &mut typed,
                FontId::proportional(11.5),
                TEXT,
                ACCENT,
                start,
                select.max.x,
            );
            let (enter, escape) =
                ui.input(|input| (input.key_pressed(egui::Key::Enter), input.key_pressed(egui::Key::Escape)));
            if !field.has_focus() && !field.lost_focus() {
                field.request_focus();
            }
            if escape {
                ui.data_mut(|data| data.remove::<String>(renaming_id));
            } else if enter || field.lost_focus() {
                if !typed.trim().is_empty() {
                    state.audio.shown_mut().name = typed.trim().to_owned();
                    state.audio_changed = true;
                }
                ui.data_mut(|data| data.remove::<String>(renaming_id));
            } else {
                ui.data_mut(|data| data.insert_temp(renaming_id, typed));
            }
        }
        None => {
            let picker = small_select(ui, select, egui::Id::unique("audio playlist picker"), &name)
                .on_hover_text("Choose an audio playlist");
            egui::Popup::menu(&picker).show(|ui| {
                let names: Vec<String> = state.audio.playlists.iter().map(|playlist| playlist.name.clone()).collect();
                for (at, other) in names.iter().enumerate() {
                    if ui.add(egui::Button::selectable(at == state.audio.current, other.as_str())).clicked() {
                        state.audio.current = at;
                        state.audio_changed = true;
                    }
                }
            });
        }
    }
    let more = quiet_icon(ui, more, egui::Id::unique("audio playlist more"), Icon::More, 16.0, 0.6, Some(CARD))
        .on_hover_text("More");
    let deleting_id = egui::Id::unique("audio playlist deleting");
    let many = state.audio.playlists.len() > 1;
    egui::Popup::menu(&more).show(|ui| {
        if ui.button("Rename").clicked() {
            ui.data_mut(|data| data.insert_temp(renaming_id, name.clone()));
        }
        if ui.button("New Playlist").clicked() {
            let made = crate::audio::Playlist::new(state.audio.new_name());
            state.audio.playlists.push(made);
            state.audio.current = state.audio.playlists.len() - 1;
            state.audio_changed = true;
        }
        ui.separator();
        if ui.add_enabled(many, egui::Button::new("Delete…")).clicked() {
            ui.data_mut(|data| data.insert_temp(deleting_id, true));
        }
    });
    if ui.data(|data| data.get_temp::<bool>(deleting_id)).unwrap_or(false) {
        let title = format!("Delete the audio playlist \u{201c}{name}\u{201d}?");
        match crate::studio::alert(
            ui.ctx(),
            "delete audio playlist",
            Icon::Trash,
            &title,
            "Its tracks leave the list; the files stay on this computer.",
            "Delete",
        ) {
            Some(true) => {
                if state.audio.playlists.len() > 1 {
                    let at = state.audio.current.min(state.audio.playlists.len() - 1);
                    state.audio.playlists.remove(at);
                    state.audio.current = at.saturating_sub(1);
                    state.audio_changed = true;
                }
                ui.data_mut(|data| data.remove::<bool>(deleting_id));
            }
            Some(false) => ui.data_mut(|data| data.remove::<bool>(deleting_id)),
            None => {}
        }
    }
}

fn audio_tab(ui: &mut egui::Ui, state: &mut HomeState) {
    use crate::audio::clock;
    use crate::home::AudioRequest;
    audio_heading(ui, state);
    let playing = state.audio_playing;
    // The track the player shows: the one playing, or the one ready to play next (the first of
    // the playlist to begin with), kept by the window.
    let ready_id = egui::Id::unique("audio ready");
    let shown_tracks: Vec<sp_model::Uuid> = state.audio.shown().tracks.iter().map(|track| track.id).collect();
    let ready = ui
        .data(|data| data.get_temp::<sp_model::Uuid>(ready_id))
        .filter(|id| state.audio.track(*id).is_some())
        .or(shown_tracks.first().copied());
    let current = playing.track.or(ready);
    let mut ready_now = current;
    // The playlist the current track is in, for Previous and Next.
    let list: Vec<sp_model::Uuid> = current
        .and_then(|id| state.audio.playlists.iter().find(|playlist| playlist.tracks.iter().any(|t| t.id == id)))
        .map_or(shown_tracks.clone(), |playlist| playlist.tracks.iter().map(|track| track.id).collect());
    let place = current.and_then(|id| list.iter().position(|have| *have == id));
    let muted_id = egui::Id::unique("audio muted from");
    let muted_from = ui.data(|data| data.get_temp::<f32>(muted_id)).filter(|_| state.audio_volume == 0.0);

    // The player: what plays and how far in, its keys, its level, and how tracks follow on.
    let mut request: Option<AudioRequest> = None;
    egui::Frame::new()
        .fill(CARD)
        .corner_radius(RADIUS_CARD as u8)
        .inner_margin(egui::Margin { left: 14, right: 14, top: 14, bottom: 12 })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 12.0;
            let width = ui.available_width();
            // What plays.
            let (now, _) = ui.allocate_exact_size(vec2(width, 39.0), Sense::hover());
            let (said, said_ink) = match (playing.track, playing.paused, current) {
                (Some(_), false, _) => ("PLAYING", READY),
                (Some(_), true, _) => ("PAUSED", TEXT_FAINT),
                (None, _, Some(_)) => ("READY", TEXT_FAINT),
                (None, _, None) => ("NOTHING TO PLAY", TEXT_FAINT),
            };
            paint_spaced(ui, now.min, Align2::LEFT_TOP, said, fixed_semibold(11.5), said_ink, 0.6);
            let (title, ink) = match current.and_then(|id| state.audio.track(id)) {
                Some(track) => (track.name.as_str(), TEXT),
                None => ("Add a track to this playlist", TEXT_DIM),
            };
            let title = fitted(ui, title, semibold(16.0), ink, width, 1);
            ui.painter().galley(pos2(now.min.x, now.max.y - title.size().y), title, ink);

            // How far in: the time gone, a line to drag, the time left.
            let (seek, _) = ui.allocate_exact_size(vec2(width, 22.0), Sense::hover());
            let length = playing.length.filter(|_| playing.track.is_some());
            let fraction = length.map_or(0.0, |length| {
                (playing.position.as_secs_f32() / length.as_secs_f32().max(0.001)).clamp(0.0, 1.0)
            });
            let line = Rect::from_min_max(pos2(seek.min.x + 52.0, seek.min.y), pos2(seek.max.x - 52.0, seek.max.y));
            let fill = if playing.track.is_some() && !playing.paused { READY } else { TEXT };
            let (dragged, response) =
                drag_line(ui, line, egui::Id::unique("audio seek"), fraction, fill, length.is_some());
            let shown = match (dragged, length) {
                (Some(at), Some(length)) => length.mul_f32(at),
                _ => playing.position,
            };
            if (response.drag_stopped() || response.clicked())
                && let (Some(at), Some(length)) = (dragged, length)
            {
                let to = length.as_secs_f32() * at;
                request = Some(AudioRequest::Skip(to - playing.position.as_secs_f32()));
            }
            response.on_hover_text("Where in the track: drag to move through it");
            let gone = if length.is_some() { clock(shown) } else { "0:00".to_owned() };
            let left =
                length.map_or("0:00".to_owned(), |length| format!("\u{2212}{}", clock(length.saturating_sub(shown))));
            let time = fixed(12.0);
            ui.painter().text(seek.left_center(), Align2::LEFT_CENTER, gone, time.clone(), TEXT_DIM);
            ui.painter().text(seek.right_center(), Align2::RIGHT_CENTER, left, time, TEXT_DIM);

            // The keys: previous, back 15 seconds, play or pause, forward 15, next.
            let (keys, _) = ui.allocate_exact_size(vec2(width, 48.0), Sense::hover());
            let y = keys.center().y;
            let mut x = keys.center().x - 120.0;
            let mut next_rect = |wide: f32, tall: f32| {
                let rect = Rect::from_min_size(pos2(x, y - tall / 2.0), vec2(wide, tall));
                x += wide + 10.0;
                rect
            };
            let (previous, back, play, forward, next) = (
                next_rect(36.0, 36.0),
                next_rect(40.0, 36.0),
                next_rect(48.0, 48.0),
                next_rect(40.0, 36.0),
                next_rect(36.0, 36.0),
            );
            let loaded = playing.track.is_some();
            let steps = list.len() > 1 || loaded;
            let glyph = |rect: Rect, icon: Icon, side: f32, ink: Color32| {
                crate::icons::paint(ui.painter(), icon, Rect::from_center_size(rect.center(), vec2(side, side)), ink);
            };
            let ink = |enabled: bool| if enabled { TEXT } else { TEXT_FAINT };
            if player_key(ui, previous, egui::Id::unique("audio previous"), false, steps)
                .on_hover_text("Previous track (the start of this one first)")
                .clicked()
            {
                if loaded && playing.position.as_secs_f32() > 3.0 {
                    request = Some(AudioRequest::Skip(-(playing.position.as_secs_f32() + 1.0)));
                } else if let Some(at) = place.filter(|_| !list.is_empty()) {
                    let before = list[(at + list.len() - 1) % list.len()];
                    if loaded {
                        request = Some(AudioRequest::Play(before));
                    }
                    ready_now = Some(before);
                }
            }
            glyph(previous, Icon::ChevronLeft, 14.0, ink(steps));
            let font = fixed_semibold(11.5);
            if player_key(ui, back, egui::Id::unique("audio back"), false, loaded)
                .on_hover_text("Back 15 seconds")
                .clicked()
            {
                request = Some(AudioRequest::Skip(-15.0));
            }
            ui.painter().text(back.center(), Align2::CENTER_CENTER, "\u{2212}15", font.clone(), ink(loaded));
            let can_play = loaded || current.is_some();
            let pausing = loaded && !playing.paused;
            let hint = if pausing { "Pause" } else { "Play" };
            if player_key(ui, play, egui::Id::unique("audio play"), true, can_play).on_hover_text(hint).clicked() {
                request = Some(match (loaded, current) {
                    (true, _) => AudioRequest::TogglePause,
                    (false, Some(id)) => AudioRequest::Play(id),
                    (false, None) => AudioRequest::TogglePause,
                });
            }
            let (icon, nudge) = if pausing { (Icon::Pause, 0.0) } else { (Icon::Play, 1.5) };
            glyph(play.translate(vec2(nudge, 0.0)), icon, 16.0, Color32::from_gray(17));
            if player_key(ui, forward, egui::Id::unique("audio forward"), false, loaded)
                .on_hover_text("Forward 15 seconds")
                .clicked()
            {
                request = Some(AudioRequest::Skip(15.0));
            }
            ui.painter().text(forward.center(), Align2::CENTER_CENTER, "+15", font, ink(loaded));
            let can_next = list.len() > 1;
            if player_key(ui, next, egui::Id::unique("audio next"), false, can_next)
                .on_hover_text("Next track")
                .clicked()
                && let Some(at) = place
            {
                let pick = |count: usize| (sp_model::Uuid::new_v4().as_u128() % count as u128) as usize;
                let after = match current {
                    Some(id) if loaded => state.audio.after(id, pick).map(|track| track.id),
                    _ => Some(list[(at + 1) % list.len()]),
                };
                if let Some(after) = after {
                    if loaded {
                        request = Some(AudioRequest::Play(after));
                    }
                    ready_now = Some(after);
                }
            }
            glyph(next, Icon::ChevronRight, 14.0, ink(can_next));

            // The level: mute, a line to drag, and the level in numbers.
            let (level_row, _) = ui.allocate_exact_size(vec2(width, 30.0), Sense::hover());
            let mute = Rect::from_min_size(pos2(level_row.min.x, level_row.center().y - 14.0), vec2(28.0, 28.0));
            let mute_response = ui.interact(mute, egui::Id::unique("audio mute"), Sense::click());
            if mute_response.hovered() {
                ui.painter().rect_filled(mute, crate::studio::RADIUS, crate::studio::BUTTON);
            }
            let (icon, ink, hint) = match muted_from {
                Some(_) => (Icon::Muted, crate::studio::CARE, "Muted. Click to bring the sound back"),
                None => (Icon::Sound, TEXT_DIM, "Mute"),
            };
            crate::icons::paint(ui.painter(), icon, Rect::from_center_size(mute.center(), vec2(15.0, 15.0)), ink);
            if mute_response.on_hover_text(hint).clicked() {
                match muted_from {
                    Some(level) => {
                        state.audio_volume = level;
                        request = Some(AudioRequest::Volume(level));
                        ui.data_mut(|data| data.remove::<f32>(muted_id));
                    }
                    None => {
                        ui.data_mut(|data| data.insert_temp(muted_id, state.audio_volume));
                        state.audio_volume = 0.0;
                        request = Some(AudioRequest::Volume(0.0));
                    }
                }
            }
            // The level goes up to half as loud again as the track itself.
            const LOUDEST: f32 = 1.5;
            let level = muted_from.unwrap_or(state.audio_volume);
            let line = Rect::from_min_max(
                pos2(level_row.min.x + 36.0, level_row.center().y - 11.0),
                pos2(level_row.max.x - 52.0, level_row.center().y + 11.0),
            );
            let (moved, response) = drag_line(ui, line, egui::Id::unique("audio level"), level / LOUDEST, TEXT, true);
            if let Some(at) = moved {
                let level = (at * LOUDEST * 100.0).round() / 100.0;
                if level != state.audio_volume || muted_from.is_some() {
                    // Moving the level brings the sound back.
                    ui.data_mut(|data| data.remove::<f32>(muted_id));
                    state.audio_volume = level;
                    request = Some(AudioRequest::Volume(level));
                }
            }
            response.on_hover_text("Volume");
            let words = match muted_from {
                Some(_) => "Muted".to_owned(),
                None => format!("{:.0}%", state.audio_volume * 100.0),
            };
            ui.painter().text(level_row.right_center(), Align2::RIGHT_CENTER, words, fixed(12.0), TEXT_DIM);

            // How tracks follow one another, under a line.
            let (options, _) = ui.allocate_exact_size(vec2(width, 35.0), Sense::hover());
            ui.painter().hline(options.x_range(), options.min.y, egui::Stroke::new(1.0, crate::studio::LINE));
            let y = options.min.y + 10.0 + 12.0;
            let small = FontId::proportional(11.5);
            let through_wide = flat_button_width(ui, &["Play through"], &small, 9.0);
            let through = Rect::from_min_size(pos2(options.min.x, y - 12.0), vec2(through_wide, 24.0));
            let look = if state.audio.through { Flat::On } else { Flat::Plain };
            if flat_button(ui, through, egui::Id::unique("audio through"), "Play through", small.clone(), look, true)
                .on_hover_text("When a track ends, go on to the next")
                .clicked()
            {
                state.audio.through = !state.audio.through;
                state.audio_changed = true;
            }
            let shuffle_wide = flat_button_width(ui, &["Shuffle"], &small, 9.0);
            let shuffle = Rect::from_min_size(pos2(through.max.x + 6.0, y - 12.0), vec2(shuffle_wide, 24.0));
            let look = if state.audio.shuffle { Flat::On } else { Flat::Plain };
            if flat_button(ui, shuffle, egui::Id::unique("audio shuffle"), "Shuffle", small, look, true)
                .on_hover_text("Play the tracks in a random order")
                .clicked()
            {
                state.audio.shuffle = !state.audio.shuffle;
                state.audio_changed = true;
            }
            let fade = state.audio.crossfade;
            let fade_words = |seconds: f32| if seconds <= 0.0 { "Off".to_owned() } else { format!("{seconds} s") };
            let select = Rect::from_min_size(pos2(options.max.x - 70.0, y - 12.0), vec2(70.0, 24.0));
            paint_spaced(
                ui,
                pos2(select.min.x - 8.0, y),
                Align2::RIGHT_CENTER,
                "CROSSFADE",
                fixed_semibold(11.5),
                TEXT_FAINT,
                0.6,
            );
            let picker = small_select(ui, select, egui::Id::unique("audio crossfade"), &fade_words(fade))
                .on_hover_text("How long one track fades into the next");
            egui::Popup::menu(&picker).show(|ui| {
                let mut choices = vec![0.0, 1.0, 2.0, 3.0, 5.0, 8.0, 10.0];
                if !choices.contains(&fade) {
                    choices.push(fade);
                    choices.sort_by(f32::total_cmp);
                }
                for seconds in choices {
                    if ui.add(egui::Button::selectable(fade == seconds, fade_words(seconds))).clicked() {
                        state.audio.crossfade = seconds;
                        state.audio_changed = true;
                    }
                }
            });
        });
    if playing.track.is_some() {
        ready_now = playing.track;
    }
    ui.data_mut(|data| match ready_now {
        Some(id) => {
            data.insert_temp(ready_id, id);
        }
        None => data.remove::<sp_model::Uuid>(ready_id),
    });

    // The tracks: how many, Add tracks, and a row each to play it, drag it to another place or
    // open its menu.
    let count = shown_tracks.len();
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::hover());
    let says = if count == 1 { "1 track".to_owned() } else { format!("{count} tracks") };
    paint_spaced(ui, row.left_center(), Align2::LEFT_CENTER, &says.to_uppercase(), fixed_semibold(11.5), TEXT_DIM, 0.6);
    let small = FontId::proportional(11.5);
    let add_wide = flat_button_width(ui, &["+ Add tracks…"], &small, 9.0);
    let add = Rect::from_min_size(pos2(row.max.x - add_wide, row.min.y), vec2(add_wide, 24.0));
    if flat_button(ui, add, egui::Id::unique("audio add tracks"), "+ Add tracks…", small, Flat::Plain, true)
        .on_hover_text("Add music files from this computer to this playlist")
        .clicked()
    {
        request = Some(AudioRequest::Add);
    }
    if count == 0 {
        empty_note(ui, "No tracks yet. Add music files from this computer.");
    }
    let renaming_id = egui::Id::unique("audio track renaming");
    let mut renaming = ui.data(|data| data.get_temp::<(sp_model::Uuid, String)>(renaming_id));
    let mut renamed: Option<(sp_model::Uuid, String)> = None;
    let mut asked: Option<(usize, TrackAsk)> = None;
    let mut rows: Vec<Rect> = Vec::with_capacity(count);
    let mut dragging: Option<usize> = None;
    let width = ui.available_width();
    for (at, track) in state.audio.shown().tracks.iter().enumerate() {
        let (line, _) = ui.allocate_exact_size(vec2(width, 34.0), Sense::hover());
        rows.push(line);
        let id = track.id;
        let response = ui.interact(line, egui::Id::unique(("audio track", id)), Sense::click_and_drag());
        if response.dragged() || response.drag_stopped() {
            dragging = Some(at);
        }
        let on = playing.track == Some(id);
        let chosen = current == Some(id);
        let fill = if on {
            Some(CARD_RUNNING)
        } else if chosen || response.hovered() {
            Some(CARD)
        } else {
            None
        };
        if let Some(fill) = fill {
            ui.painter().rect_filled(line, crate::studio::RADIUS, fill);
        }
        let y = line.center().y;
        if on && !playing.paused {
            let mark = Rect::from_center_size(pos2(line.min.x + 10.0 + 8.0, y), vec2(10.0, 10.0));
            crate::icons::paint(ui.painter(), Icon::Play, mark, READY);
        } else {
            let number = format!("{:02}", at + 1);
            ui.painter().text(pos2(line.min.x + 10.0, y), Align2::LEFT_CENTER, number, fixed(12.0), TEXT_FAINT);
        }
        let more = Rect::from_min_size(pos2(line.max.x - 28.0 - 3.0, y - 14.0), vec2(28.0, 28.0));
        let name_left = line.min.x + 10.0 + 24.0 + 8.0;
        let name_right = more.min.x - 8.0;
        match renaming.as_mut().filter(|(renamed, _)| *renamed == id) {
            Some((_, typed)) => {
                let font = name_font();
                let field =
                    crate::studio::rename_field_at(ui, typed, font, TEXT, ACCENT, pos2(name_left, y), name_right);
                if !field.has_focus() && !field.lost_focus() {
                    field.request_focus();
                }
                let (enter, escape) =
                    ui.input(|input| (input.key_pressed(egui::Key::Enter), input.key_pressed(egui::Key::Escape)));
                if escape {
                    renaming = None;
                } else if enter || field.lost_focus() {
                    if !typed.trim().is_empty() {
                        renamed = Some((id, typed.trim().to_owned()));
                    }
                    renaming = None;
                }
            }
            None => {
                let font = if chosen { semibold(13.0) } else { FontId::proportional(13.0) };
                let name = fitted(ui, &track.name, font, TEXT, name_right - name_left, 1);
                ui.painter().galley(pos2(name_left, y - name.size().y / 2.0), name, TEXT);
            }
        }
        let first = at == 0;
        let last = at + 1 == count;
        let more = quiet_icon(ui, more, egui::Id::unique(("audio track more", id)), Icon::More, 16.0, 0.55, None)
            .on_hover_text("More");
        egui::Popup::menu(&more).show(|ui| {
            if let Some(ask) = track_menu(ui, first, last) {
                asked = Some((at, ask));
            }
        });
        if response.clicked() {
            asked = Some((at, TrackAsk::Play));
        }
        response.on_hover_text("Click to play this track; drag it to another place").context_menu(|ui| {
            if let Some(ask) = track_menu(ui, first, last) {
                asked = Some((at, ask));
            }
        });
    }
    // A track being dragged: an ink line shows where it would go, by which half of a row the
    // pointer is over; letting go moves it there.
    if let (Some(from), Some(pointer)) = (dragging, ui.ctx().pointer_latest_pos()) {
        let gap = rows.iter().position(|row| pointer.y < row.center().y).unwrap_or(rows.len());
        let y = match rows.get(gap) {
            Some(row) => row.min.y - GAP / 2.0,
            None => rows.last().map_or(0.0, |row| row.max.y + GAP / 2.0),
        };
        if gap != from && gap != from + 1 {
            ui.painter().hline(rows[0].x_range(), y, egui::Stroke::new(2.0, TEXT));
        }
        if ui.input(|input| input.pointer.any_released()) && gap != from && gap != from + 1 {
            let tracks = &mut state.audio.shown_mut().tracks;
            let moved = tracks.remove(from);
            tracks.insert(if gap > from { gap - 1 } else { gap }, moved);
            state.audio_changed = true;
        }
    }
    if let Some((id, name)) = renamed
        && let Some(track) = state.audio.playlists.iter_mut().flat_map(|p| p.tracks.iter_mut()).find(|t| t.id == id)
    {
        track.name = name;
        state.audio_changed = true;
    }
    ui.data_mut(|data| match renaming {
        Some(renaming) => {
            data.insert_temp(renaming_id, renaming);
        }
        None => data.remove::<(sp_model::Uuid, String)>(renaming_id),
    });
    if let Some((at, ask)) = asked {
        let id = state.audio.shown().tracks[at].id;
        match ask {
            TrackAsk::Play => {
                request = Some(AudioRequest::Play(id));
                ui.data_mut(|data| data.insert_temp(ready_id, id));
            }
            TrackAsk::Up | TrackAsk::Down => {
                let to = if matches!(ask, TrackAsk::Up) { at.saturating_sub(1) } else { (at + 1).min(count - 1) };
                state.audio.shown_mut().tracks.swap(at, to);
                state.audio_changed = true;
            }
            TrackAsk::Rename => {
                let name = state.audio.shown().tracks[at].name.clone();
                ui.data_mut(|data| data.insert_temp(renaming_id, (id, name)));
            }
            TrackAsk::Remove => request = Some(AudioRequest::Remove(id)),
        }
    }
    if let Some(request) = request {
        state.audio_request = Some(request);
    }
}

/// A small drop-down, as the design draws one: its words and a chevron on the card grey, lighter
/// under the pointer. Open its choices with `egui::Popup::menu` on what it returns.
fn small_select(ui: &egui::Ui, rect: Rect, id: egui::Id, words: &str) -> egui::Response {
    let reach = ((crate::studio::LEAST_CLICK - rect.height()) / 2.0).max(0.0);
    let response = ui.interact(rect.expand2(vec2(0.0, reach)), id, Sense::click());
    let fill = if response.hovered() { crate::studio::BUTTON } else { CARD };
    ui.painter().rect_filled(rect, crate::studio::RADIUS, fill);
    let text = fitted(ui, words, FontId::proportional(11.5), TEXT, rect.width() - 30.0, 1);
    ui.painter().galley(pos2(rect.min.x + 8.0, rect.center().y - text.size().y / 2.0), text, TEXT);
    let mark = Rect::from_center_size(pos2(rect.max.x - 12.0, rect.center().y), vec2(10.0, 10.0));
    crate::icons::paint(ui.painter(), Icon::ChevronDown, mark, TEXT_DIM);
    response
}

/// A tick, as the layout in use has before its name.
fn paint_tick(painter: &egui::Painter, rect: Rect, ink: Color32) {
    let at = |x: f32, y: f32| pos2(rect.min.x + x * rect.width(), rect.min.y + y * rect.height());
    let points = vec![at(0.12, 0.55), at(0.4, 0.8), at(0.9, 0.22)];
    painter.add(egui::Shape::line(points, egui::Stroke::new(1.8, ink)));
}

/// The role of the timer's box in a stage layout.
const STAGE_TIMER: &str = "timer";

fn stage_tab(ui: &mut egui::Ui, show: &sp_engine::Show, state: &mut HomeState, thumb: &mut Thumb) {
    let stages: Vec<usize> =
        state.screens.iter().enumerate().filter(|(_, screen)| screen.kind == Kind::Stage).map(|(at, _)| at).collect();
    if stages.is_empty() {
        heading(ui, "Stage", None);
        empty_note(ui, "No stage screens. Add one in Settings, under Screens.");
        let (place, _) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::hover());
        let font = FontId::proportional(11.5);
        let wide = flat_button_width(ui, &["Set Up Screens…"], &font, 9.0);
        let rect = Rect::from_min_size(place.min, vec2(wide, 24.0));
        if flat_button(ui, rect, egui::Id::unique("stage set up screens"), "Set Up Screens…", font, Flat::Plain, true)
            .clicked()
        {
            state.settings_open = true;
        }
        return;
    }
    // Which stage screens the layouts are for: all of them, or the one chosen (kept by the
    // window). With one stage screen there is nothing to choose.
    let chosen_id = egui::Id::unique("stage screen chosen");
    let chosen = ui
        .data(|data| data.get_temp::<sp_model::Uuid>(chosen_id))
        .filter(|id| stages.iter().any(|&at| state.screens[at].id == *id));
    if stages.len() > 1 {
        let (row, _) = heading(ui, "Which stage screen", None);
        let words = chosen
            .and_then(|id| state.screens.iter().find(|screen| screen.id == id))
            .map_or("All stage screens", |screen| screen.name.as_str());
        let rect = Rect::from_min_size(pos2(row.max.x - 150.0, row.center().y - 12.0), vec2(150.0, 24.0));
        let picker = small_select(ui, rect, egui::Id::unique("stage screen picker"), words);
        egui::Popup::menu(&picker).show(|ui| {
            if ui.add(egui::Button::selectable(chosen.is_none(), "All stage screens")).clicked() {
                ui.data_mut(|data| data.remove::<sp_model::Uuid>(chosen_id));
            }
            for &at in &stages {
                let screen = &state.screens[at];
                if ui.add(egui::Button::selectable(chosen == Some(screen.id), screen.name.as_str())).clicked() {
                    let id = screen.id;
                    ui.data_mut(|data| data.insert_temp(chosen_id, id));
                }
            }
        });
    }
    let targets: Vec<usize> =
        stages.iter().copied().filter(|&at| chosen.is_none_or(|id| state.screens[at].id == id)).collect();
    let layout_of = |at: usize| crate::stage::layout_for(&state.stage_layouts, state.screens[at].stage_layout).id;
    let shown = layout_of(targets[0]);
    let same = targets.iter().all(|&at| layout_of(at) == shown);
    // Said when what is chosen does not show now, so a change is not expected on the stage.
    let off = !state.stage_on || targets.iter().all(|&at| state.screens[at].output == crate::screens::Output::Off);
    if off {
        let says = if state.stage_on { "This stage screen is switched off" } else { "The stage screens are off" };
        let words = format!("{says}. The layout still changes, ready for when it is on.");
        egui::Frame::new()
            .corner_radius(RADIUS_CARD as u8)
            .stroke(egui::Stroke::new(1.0, crate::studio::LINE))
            .inner_margin(egui::Margin::symmetric(11, 8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(egui::RichText::new(words).size(12.5).color(crate::studio::CARE));
            });
    }
    let title =
        if stages.len() > 1 { "Layout".to_owned() } else { format!("{} \u{b7} Layout", state.screens[stages[0]].name) };
    let (row, _) = heading(ui, &title, None);
    let font = FontId::proportional(11.5);
    let wide = flat_button_width(ui, &["Edit layouts…"], &font, 9.0);
    let rect = Rect::from_min_size(pos2(row.max.x - wide, row.center().y - 12.0), vec2(wide, 24.0));
    if flat_button(ui, rect, egui::Id::unique("stage edit layouts"), "Edit layouts…", font, Flat::Plain, true)
        .on_hover_text("Lay out what the stage screens show")
        .clicked()
    {
        state.edit_kind_request = Some(crate::home::EditKind::Stage);
    }

    // Every layout as a tile with a picture of what the stage would show in it now; the one in
    // use is outlined, with a tick. One click switches.
    if state.stage_layouts.is_empty() {
        state.stage_layouts = crate::stage::builtin_layouts();
    }
    let now = show.live_slide().map(|(_, slide)| slide);
    let next = show.live().and(show.upcoming()).and_then(|at| {
        let song = show.presentations().get(at.presentation)?;
        song.slide(at.slide).map(|(_, slide)| slide)
    });
    let about = match show.live_slide() {
        Some((song, _)) => crate::stage::About {
            title: Some(song.title.clone()),
            group: show.live().and_then(|at| song.slide(at.slide)).map(|(group, _)| group.name.clone()),
        },
        None => crate::stage::About::default(),
    };
    let overlay = crate::output::Overlay {
        stage_message: state.stage_message_live.then(|| state.stage_message.clone()),
        timers: state.timers.iter().filter(|timer| timer.running()).take(1).cloned().collect(),
        ..Default::default()
    };
    let width = ui.available_width();
    let tile_wide = (width - 8.0) / 2.0;
    let picture = vec2(tile_wide - 12.0, (tile_wide - 12.0) * 9.0 / 16.0);
    let tile_tall = 6.0 + picture.y + 6.0 + 17.0 + 8.0;
    let mut switch_to = None;
    let mut row = Rect::NOTHING;
    for (place, layout) in state.stage_layouts.iter().enumerate() {
        if place % 2 == 0 {
            if place > 0 {
                ui.add_space(1.0);
            }
            row = ui.allocate_exact_size(vec2(width, tile_tall), Sense::hover()).0;
        }
        let tile = Rect::from_min_size(
            pos2(row.min.x + (tile_wide + 8.0) * (place % 2) as f32, row.min.y),
            vec2(tile_wide, tile_tall),
        );
        let response = ui.interact(tile, egui::Id::unique(("stage layout tile", layout.id)), Sense::click());
        let on = same && layout.id == shown;
        let fill = if on || response.hovered() { crate::studio::BUTTON } else { CARD };
        ui.painter().rect_filled(tile, 10.0, fill);
        if on {
            let edge = egui::Stroke::new(2.0, crate::studio::SELECTED);
            ui.painter().rect_stroke(tile, 10.0, edge, egui::StrokeKind::Inside);
        }
        // The stage's picture in this layout, drawn again when what it shows changes: a new
        // slide, the stage message, the clock's minute. A running timer is named but its time
        // is left out, so the pictures are not drawn again every second.
        let mut stage = crate::stage::slide(now, next, &overlay, &about, layout);
        let timer_name = overlay.timers.first().map(|timer| timer.name.clone());
        for object in &mut stage.objects {
            if object.role.as_ref().is_some_and(|role| role.0 == STAGE_TIMER)
                && let Some(name) = &timer_name
            {
                *object = sp_model::Object::text(sp_model::Role(STAGE_TIMER.to_owned()), name.clone());
            }
        }
        let texts = format!("{:?}", stage.texts());
        let said = signature(&[&texts, &serde_json::to_string(layout).unwrap_or_default()]);
        let key = sp_model::Uuid::from_u128(layout.id.as_u128() ^ 0x7374_6167_6570_6963_7475_7265_0000_0000);
        let shot = Rect::from_min_size(tile.min + vec2(6.0, 6.0), picture);
        own_picture(ui, thumb, shot, 5.0, key, &stage, &crate::stage::theme_for(layout), said);
        let name_y = shot.max.y + 6.0 + 8.5;
        let mut left = tile.min.x + 9.0;
        if on {
            paint_tick(ui.painter(), Rect::from_center_size(pos2(left + 6.0, name_y), vec2(12.0, 12.0)), READY);
            left += 17.0;
        }
        let (font, ink) = if on { (semibold(12.5), TEXT) } else { (FontId::proportional(12.5), TEXT_DIM) };
        let name = fitted(ui, &layout.name, font, ink, tile.max.x - 9.0 - left, 1);
        ui.painter().galley(pos2(left, name_y - name.size().y / 2.0), name, ink);
        if response.on_hover_text(format!("Switch the stage to {}", layout.name)).clicked() {
            switch_to = Some(layout.id);
        }
    }
    if let Some(layout) = switch_to {
        for &at in &targets {
            state.screens[at].stage_layout = Some(layout);
        }
        state.screens_changed = true;
    }

    // The note for the people on stage: typed here, sent with Show on stage (or Change it while
    // one shows), taken off with Clear. What is typed waits until it is sent.
    ui.add_space(6.0);
    let live = state.stage_message_live;
    heading(ui, if live { "Stage message \u{b7} showing" } else { "Stage message" }, None);
    let draft_id = egui::Id::unique("stage message draft");
    let mut words = ui.data(|data| data.get_temp::<String>(draft_id)).unwrap_or_else(|| state.stage_message.clone());
    let field = ui.add(
        egui::TextEdit::singleline(&mut words)
            .desired_width(f32::INFINITY)
            .min_size(vec2(0.0, 32.0))
            .margin(egui::Margin::symmetric(10, 8))
            .background_color(CARD)
            .hint_text("A note for the people on stage"),
    );
    if field.changed() {
        ui.data_mut(|data| data.insert_temp(draft_id, words.clone()));
    }
    let changed = words != state.stage_message;
    let can_send = !words.trim().is_empty() && (!live || changed);
    let entered = field.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
    let (place, _) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::hover());
    let font = FontId::proportional(12.5);
    let send_wide = flat_button_width(ui, &["Show on stage", "Change it"], &font, 13.0);
    let send = Rect::from_min_size(place.min, vec2(send_wide, 30.0));
    let (send_words, send_look) = if live { ("Change it", Flat::Plain) } else { ("Show on stage", Flat::Main) };
    let send_response =
        flat_button(ui, send, egui::Id::unique("stage message show"), send_words, font.clone(), send_look, can_send)
            .on_hover_text("Show these words at the top of the stage screens (Return)");
    if (send_response.clicked() || entered) && can_send {
        state.stage_message = words.trim().to_owned();
        state.stage_message_live = true;
        ui.data_mut(|data| data.remove::<String>(draft_id));
    }
    let clear_wide = flat_button_width(ui, &["Clear"], &font, 13.0);
    let clear = Rect::from_min_size(pos2(send.max.x + 6.0, place.min.y), vec2(clear_wide, 30.0));
    if flat_button(ui, clear, egui::Id::unique("stage message clear"), "Clear", font, Flat::Plain, live)
        .on_hover_text("Take the message off the stage screens")
        .clicked()
    {
        state.stage_message_live = false;
    }
}

/// The VoxLive tab: the church's VoxLive screens, a button each. A click puts that screen's
/// captions on the screens in place of what is there (a verse, a song); a click on a verse
/// brings the verse back. On the Bible page too, where this column stays.
fn voxlive_tab(ui: &mut egui::Ui, show: &sp_engine::Show, state: &mut HomeState) {
    let Some(slot) = state.voxlive_slot else { return };
    let Some(item) = show.presentations().get(slot) else { return };
    caption(ui, "VOXLIVE");
    ui.add_space(2.0);
    let live = show.live().filter(|at| at.presentation == slot).map(|at| at.slide);
    let width = ui.available_width();
    for (index, (group, _)) in item.slides().enumerate() {
        let on = live == Some(index);
        let (row, response) = ui.allocate_exact_size(vec2(width, 38.0), Sense::click());
        let fill = if on {
            crate::integrations::VOXLIVE
        } else if response.hovered() {
            Color32::from_white_alpha(18)
        } else {
            CARD
        };
        ui.painter().rect_filled(row, 8.0, fill);
        ui.painter().text(
            row.left_center() + vec2(12.0, 0.0),
            egui::Align2::LEFT_CENTER,
            group.name.as_str(),
            crate::studio::semibold(13.0),
            TEXT,
        );
        if on {
            ui.painter().circle_filled(row.right_center() - vec2(14.0, 0.0), 4.0, TALLY);
        }
        let response = response.on_hover_text(if on {
            "On the screens now"
        } else {
            "Put these captions on the screens, in place of what is there"
        });
        if response.clicked() {
            state.voxlive_go = Some(index);
        }
        ui.add_space(4.0);
    }
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new("A verse clicked after brings the verse back; this brings the captions back.")
            .size(11.5)
            .color(TEXT_FAINT),
    );
}
