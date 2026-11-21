//! The tool tabs under Clear All in the right column: Timers, Messages, Props, Macros, Stage,
//! Audio and VoxLive.
//!
//! Each tab is a heading in small capitals and a list of cards. A card whose thing is on screen
//! is tinted orange, one whose thing runs (a timer) green; its buttons keep their grey, and keep
//! their width whatever they say, so nothing moves. What a card can do beyond its buttons is in
//! its ⋯ menu, which a right-click on the card opens too.

use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::{Align, Align2, Color32, FontId, Layout, Rect, Sense, pos2, vec2};

use crate::home::HomeState;
use crate::icons::Icon;
use crate::messages::Message;
use crate::screens::Kind;
use crate::studio::{
    ACCENT, CARD, CARD_RUNNING, CARD_SHOWN, Flat, PANEL, RADIUS_CARD, READY, TALLY, TEXT, TEXT_DIM, TEXT_FAINT, Tabs,
    caption, fixed, fixed_semibold, flat_button, flat_button_width, icon_key, paint_spaced, quiet_icon, semibold,
    spaced, tab_switch, tab_switch_height,
};
use crate::timers::{self, Timer};

/// Draws the tabs and the one that is open.
pub fn live_tabs(ui: &mut egui::Ui, show: &sp_engine::Show, state: &mut HomeState) {
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
    let scrolled = egui::ScrollArea::vertical().id_salt("live tab").auto_shrink([false, false]).show(ui, |ui| {
        // The same width whether the tab scrolls or not (its bar floats over the edge), so
        // nothing moves when a list grows long enough to scroll.
        ui.set_width(width);
        ui.spacing_mut().item_spacing.y = GAP;
        match state.live_tab {
            0 => timers_tab(ui, state),
            1 => messages_tab(ui, state),
            2 => props_tab(ui, state),
            3 => macros_tab(ui, show, state),
            4 => stage_tab(ui, state),
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

fn props_tab(ui: &mut egui::Ui, state: &mut HomeState) {
    use crate::props::Prop;
    if heading_with_add(ui, "PROPS", "Add a prop") {
        let taken = |name: &str| state.props.iter().any(|prop| prop.name == name);
        let name = (1..).map(|n| format!("Prop {n}")).find(|name| !taken(name)).unwrap_or_default();
        let prop = Prop::new(name.clone());
        state.prop_rename = Some((prop.id, name));
        state.rename_started = true;
        state.props.push(prop);
        state.props_changed = true;
    }
    if state.props.is_empty() {
        ui.label(
            egui::RichText::new(
                "No props. A prop stays over the slides until it is taken off, such as a name in a corner.",
            )
            .size(12.0)
            .color(TEXT_FAINT),
        );
        return;
    }
    enum Ask {
        Toggle,
        Edit,
        Rename,
        Duplicate,
        AutoClear(Option<u32>),
        Delete,
    }
    let mut asked: Option<(usize, Ask)> = None;
    let mut renamed: Option<Option<(sp_model::Uuid, String)>> = None;
    let mut rename_started = std::mem::take(&mut state.rename_started);
    let tile = vec2(96.0, 54.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
        for (index, prop) in state.props.iter().enumerate() {
            let live = state.props_live.iter().any(|(id, _)| *id == prop.id);
            let (rect, response) = ui.allocate_exact_size(tile, Sense::click());
            ui.painter().rect_filled(rect, 6.0, CARD);
            if live {
                ui.painter().rect_stroke(rect, 6.0, egui::Stroke::new(2.0, TALLY), egui::StrokeKind::Outside);
            } else if response.hovered() {
                ui.painter().rect_stroke(rect, 6.0, egui::Stroke::new(1.0, TEXT_FAINT), egui::StrokeKind::Outside);
            }
            match state.prop_rename.as_mut().filter(|(id, _)| *id == prop.id) {
                Some((_, typed)) => {
                    let field = crate::studio::rename_field_at(
                        ui,
                        typed,
                        FontId::proportional(12.0),
                        TEXT,
                        ACCENT,
                        rect.left_center() + vec2(8.0, 0.0),
                        rect.max.x - 4.0,
                    );
                    if std::mem::take(&mut rename_started) {
                        field.request_focus();
                    }
                    let (enter, escape) =
                        ui.input(|input| (input.key_pressed(egui::Key::Enter), input.key_pressed(egui::Key::Escape)));
                    if escape {
                        renamed = Some(None);
                    } else if enter || field.lost_focus() {
                        renamed = Some(Some((prop.id, typed.clone())));
                    }
                }
                None => {
                    ui.painter().with_clip_rect(rect.shrink(4.0)).text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        prop.name.as_str(),
                        FontId::proportional(12.0),
                        if live { TEXT } else { TEXT_DIM },
                    );
                }
            }
            if response.double_clicked() {
                asked = Some((index, Ask::Edit));
            } else if response.clicked() {
                asked = Some((index, Ask::Toggle));
            }
            response.on_hover_text("Click to show or take off; double-click to edit").context_menu(|ui| {
                if ui.button("Edit…").clicked() {
                    asked = Some((index, Ask::Edit));
                    ui.close();
                }
                if ui.button("Rename").clicked() {
                    asked = Some((index, Ask::Rename));
                    ui.close();
                }
                if ui.button("Duplicate").clicked() {
                    asked = Some((index, Ask::Duplicate));
                    ui.close();
                }
                ui.menu_button("Auto-Clear", |ui| {
                    for choice in [None, Some(5), Some(10), Some(15), Some(20), Some(30), Some(45), Some(60)] {
                        let label = choice.map_or("Never".to_owned(), |seconds| format!("After {seconds} seconds"));
                        if ui.add(egui::Button::selectable(prop.auto_clear == choice, label)).clicked() {
                            asked = Some((index, Ask::AutoClear(choice)));
                            ui.close();
                        }
                    }
                });
                ui.separator();
                if ui.button("Delete").clicked() {
                    asked = Some((index, Ask::Delete));
                    ui.close();
                }
            });
        }
    });
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
    if let Some((index, ask)) = asked {
        let id = state.props[index].id;
        match ask {
            Ask::Toggle => {
                if state.props_live.iter().any(|(live, _)| *live == id) {
                    state.props_live.retain(|(live, _)| *live != id);
                } else {
                    state.props_live.push((id, Instant::now()));
                }
            }
            Ask::Edit => {
                // The prop opens in the editor, as a slide of its own.
                state.editing = crate::studio::prop_draft(state, index);
                state.bible_open = false;
                state.reflow = None;
            }
            Ask::Rename => {
                state.prop_rename = Some((id, state.props[index].name.clone()));
                state.rename_started = true;
            }
            Ask::Duplicate => {
                let copy = state.props[index].duplicate();
                state.props.insert(index + 1, copy);
                state.props_changed = true;
            }
            Ask::AutoClear(choice) => {
                state.props[index].auto_clear = choice;
                state.props_changed = true;
            }
            Ask::Delete => {
                state.props.remove(index);
                state.props_live.retain(|(live, _)| *live != id);
                state.props_changed = true;
            }
        }
    }
    ui.add_space(8.0);
    let clear = ui.add_enabled(!state.props_live.is_empty(), egui::Button::new("Clear Props"));
    if clear.on_hover_text("Take every prop off (F4)").clicked() {
        state.props_live.clear();
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

fn macros_tab(ui: &mut egui::Ui, show: &sp_engine::Show, state: &mut HomeState) {
    use crate::macros::Macro;
    if heading_with_add(ui, "MACROS", "Add a macro") {
        let taken = |name: &str| state.macros.iter().any(|m| m.name == name);
        let name = (1..).map(|n| format!("Macro {n}")).find(|name| !taken(name)).unwrap_or_default();
        let made = Macro::new(name.clone());
        state.macro_rename = Some((made.id, name));
        state.rename_started = true;
        state.macros.push(made);
        state.macros_changed = true;
    }
    if state.macros.is_empty() {
        ui.label(
            egui::RichText::new(
                "No macros. A macro does several things at once, such as clearing the screen and starting a timer.",
            )
            .size(12.0)
            .color(TEXT_FAINT),
        );
        return;
    }
    enum Ask {
        Run,
        Rename,
        Color(sp_model::Rgba),
        Add(sp_model::ShowAction),
        Remove(usize),
        Startup,
        Duplicate,
        Delete,
    }
    let mut asked: Option<(usize, Ask)> = None;
    let mut renamed: Option<Option<(sp_model::Uuid, String)>> = None;
    let mut rename_started = std::mem::take(&mut state.rename_started);
    let tile = vec2(96.0, 60.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
        for (index, made) in state.macros.iter().enumerate() {
            let (rect, response) = ui.allocate_exact_size(tile, Sense::click());
            let color = crate::home::color32(made.color);
            ui.painter().rect_filled(rect, 8.0, if response.hovered() { color } else { color.gamma_multiply(0.85) });
            match state.macro_rename.as_mut().filter(|(id, _)| *id == made.id) {
                Some((_, typed)) => {
                    let field = crate::studio::rename_field_at(
                        ui,
                        typed,
                        FontId::proportional(12.0),
                        Color32::WHITE,
                        Color32::WHITE,
                        rect.left_center() + vec2(8.0, 0.0),
                        rect.max.x - 4.0,
                    );
                    if std::mem::take(&mut rename_started) {
                        field.request_focus();
                    }
                    let (enter, escape) =
                        ui.input(|input| (input.key_pressed(egui::Key::Enter), input.key_pressed(egui::Key::Escape)));
                    if escape {
                        renamed = Some(None);
                    } else if enter || field.lost_focus() {
                        renamed = Some(Some((made.id, typed.clone())));
                    }
                }
                None => {
                    ui.painter().with_clip_rect(rect.shrink(4.0)).text(
                        rect.center() - vec2(0.0, 6.0),
                        egui::Align2::CENTER_CENTER,
                        made.name.as_str(),
                        FontId::proportional(12.5),
                        Color32::WHITE,
                    );
                    let count = made.actions.len();
                    let note = if count == 1 { "1 action".to_owned() } else { format!("{count} actions") };
                    ui.painter().text(
                        rect.center() + vec2(0.0, 12.0),
                        egui::Align2::CENTER_CENTER,
                        note,
                        FontId::proportional(10.5),
                        Color32::from_white_alpha(200),
                    );
                }
            }
            let summary: Vec<String> = made.actions.iter().map(|action| describe_action(show, state, action)).collect();
            let hint = if summary.is_empty() {
                "No actions yet: right-click to add one".to_owned()
            } else {
                summary.join("\n")
            };
            if response.clicked() {
                asked = Some((index, Ask::Run));
            }
            response.on_hover_text(hint).context_menu(|ui| {
                ui.menu_button("Add Action", |ui| {
                    if let Some(action) = add_action_menu(ui, show, state) {
                        asked = Some((index, Ask::Add(action)));
                        ui.close();
                    }
                });
                if !made.actions.is_empty() {
                    ui.menu_button("Remove Action", |ui| {
                        for (at, words) in summary.iter().enumerate() {
                            if ui.button(words.as_str()).clicked() {
                                asked = Some((index, Ask::Remove(at)));
                                ui.close();
                            }
                        }
                    });
                }
                ui.separator();
                if ui.button("Rename").clicked() {
                    asked = Some((index, Ask::Rename));
                    ui.close();
                }
                ui.menu_button("Change Color", |ui| {
                    for (name, color) in crate::macros::COLORS {
                        if ui.add(egui::Button::selectable(made.color == color, name)).clicked() {
                            asked = Some((index, Ask::Color(color)));
                            ui.close();
                        }
                    }
                });
                if ui.add(egui::Button::selectable(made.on_startup, "Trigger on Startup")).clicked() {
                    asked = Some((index, Ask::Startup));
                    ui.close();
                }
                ui.separator();
                if ui.button("Duplicate").clicked() {
                    asked = Some((index, Ask::Duplicate));
                    ui.close();
                }
                if ui.button("Delete").clicked() {
                    asked = Some((index, Ask::Delete));
                    ui.close();
                }
            });
        }
    });
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
        match ask {
            Ask::Run => state.actions_to_run.extend(state.macros[index].actions.iter().cloned()),
            Ask::Rename => {
                state.macro_rename = Some((state.macros[index].id, state.macros[index].name.clone()));
                state.rename_started = true;
            }
            Ask::Color(color) => state.macros[index].color = color,
            Ask::Add(action) => state.macros[index].actions.push(action),
            Ask::Remove(at) => {
                state.macros[index].actions.remove(at);
            }
            Ask::Startup => state.macros[index].on_startup = !state.macros[index].on_startup,
            Ask::Duplicate => {
                let copy = state.macros[index].duplicate();
                state.macros.insert(index + 1, copy);
            }
            Ask::Delete => {
                state.macros.remove(index);
            }
        }
        state.macros_changed = true;
    }
}

/// The audio playlists: the one shown, with a menu to choose another, make one, rename it or
/// delete it.
fn audio_playlists(ui: &mut egui::Ui, state: &mut HomeState) {
    let name = state.audio.shown().name.clone();
    let picker = ui
        .add(egui::Label::new(egui::RichText::new(format!("{name} ▾")).size(12.5).color(TEXT)).sense(Sense::click()))
        .on_hover_text("Choose an audio playlist");
    egui::Popup::from_toggle_button_response(&picker)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            ui.set_min_width(200.0);
            let names: Vec<String> = state.audio.playlists.iter().map(|playlist| playlist.name.clone()).collect();
            for (at, other) in names.iter().enumerate() {
                if ui.add(egui::Button::selectable(at == state.audio.current, other.as_str())).clicked() {
                    state.audio.current = at;
                    state.audio_changed = true;
                    ui.close();
                }
            }
            ui.separator();
            if ui.button("New Playlist").clicked() {
                let made = crate::audio::Playlist::new(state.audio.new_name());
                state.audio.playlists.push(made);
                state.audio.current = state.audio.playlists.len() - 1;
                state.audio_changed = true;
            }
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Name").color(TEXT_DIM));
                let field = ui.add(egui::TextEdit::singleline(&mut state.audio.shown_mut().name).desired_width(140.0));
                state.audio_changed |= field.changed();
            });
            if ui.add_enabled(names.len() > 1, egui::Button::new("Delete Playlist")).clicked() {
                let at = state.audio.current.min(state.audio.playlists.len() - 1);
                state.audio.playlists.remove(at);
                state.audio.current = at.saturating_sub(1);
                state.audio_changed = true;
                ui.close();
            }
        });
    ui.add_space(4.0);
}

fn audio_tab(ui: &mut egui::Ui, state: &mut HomeState) {
    use crate::audio::clock;
    use crate::home::AudioRequest;
    if heading_with_add(ui, "AUDIO", "Add audio files to this playlist") {
        state.audio_request = Some(AudioRequest::Add);
    }
    audio_playlists(ui, state);
    let playing = state.audio_playing;
    // What is playing: its name, how far in, and the controls.
    if let Some(id) = playing.track {
        let name = state.audio.track(id).map_or("", |track| track.name.as_str());
        egui::Frame::new().fill(CARD).corner_radius(8).inner_margin(egui::Margin::symmetric(10, 8)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(egui::RichText::new(name).size(13.0).color(TEXT));
            let fraction = playing.length.map_or(0.0, |length| {
                (playing.position.as_secs_f32() / length.as_secs_f32().max(0.001)).clamp(0.0, 1.0)
            });
            ui.add(egui::ProgressBar::new(fraction).desired_height(6.0));
            let total = playing.length.map_or_else(|| "–".to_owned(), clock);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("{} / {total}", clock(playing.position)))
                        .size(11.5)
                        .color(TEXT_DIM)
                        .monospace(),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("■").on_hover_text("Stop").clicked() {
                        state.audio_request = Some(AudioRequest::Stop);
                    }
                    if ui.button("15 ⟳").on_hover_text("Skip Forward 15 Seconds").clicked() {
                        state.audio_request = Some(AudioRequest::Skip(15.0));
                    }
                    let (label, hint) = if playing.paused { ("▶", "Play") } else { ("❚❚", "Pause") };
                    if ui.button(label).on_hover_text(hint).clicked() {
                        state.audio_request = Some(AudioRequest::TogglePause);
                    }
                    if ui.button("⟲ 15").on_hover_text("Skip Backward 15 Seconds").clicked() {
                        state.audio_request = Some(AudioRequest::Skip(-15.0));
                    }
                });
            });
        });
    }
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Volume").size(12.0).color(TEXT_DIM));
        let mut volume = state.audio_volume;
        if ui.add(egui::Slider::new(&mut volume, 0.0..=1.5).show_value(false)).changed() {
            state.audio_volume = volume;
            state.audio_request = Some(AudioRequest::Volume(volume));
        }
    });
    // How tracks follow one another.
    ui.horizontal(|ui| {
        if ui
            .selectable_label(state.audio.through, "Play Through")
            .on_hover_text("When a track ends, play the next")
            .clicked()
        {
            state.audio.through = !state.audio.through;
            state.audio_changed = true;
        }
        if ui
            .selectable_label(state.audio.shuffle, "Shuffle")
            .on_hover_text("Play the tracks in a random order")
            .clicked()
        {
            state.audio.shuffle = !state.audio.shuffle;
            state.audio_changed = true;
        }
    });
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Crossfade").size(12.0).color(TEXT_DIM));
        let mut seconds = state.audio.crossfade;
        let slider = egui::Slider::new(&mut seconds, 0.0..=10.0).suffix(" s").max_decimals(1);
        if ui.add(slider).on_hover_text("How long one track fades into the next").changed() {
            state.audio.crossfade = seconds;
            state.audio_changed = true;
        }
    });
    if state.audio.shown().tracks.is_empty() {
        ui.label(
            egui::RichText::new("No audio. Press + to add music to play, such as walk-in music.")
                .size(12.0)
                .color(TEXT_FAINT),
        );
        return;
    }
    for track in &state.audio.shown().tracks {
        let on = playing.track == Some(track.id);
        let (row, response) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click());
        if on {
            ui.painter().rect_filled(row, 6.0, ACCENT.gamma_multiply(0.35));
        } else if response.hovered() {
            ui.painter().rect_filled(row, 6.0, crate::studio::HOVER);
        }
        ui.painter().text(
            row.left_center() + vec2(10.0, 0.0),
            egui::Align2::LEFT_CENTER,
            format!("♫  {}", track.name),
            FontId::proportional(13.0),
            TEXT,
        );
        if response.clicked() {
            state.audio_request = Some(AudioRequest::Play(track.id));
        }
        response.on_hover_text("Click to play from the start").context_menu(|ui| {
            if ui.button("Remove").clicked() {
                state.audio_request = Some(AudioRequest::Remove(track.id));
                ui.close();
            }
        });
    }
    ui.add_space(6.0);
    if ui
        .add_enabled(playing.track.is_some(), egui::Button::new("Clear Audio"))
        .on_hover_text("Stop the audio (F5)")
        .clicked()
    {
        state.audio_request = Some(AudioRequest::Stop);
    }
}

fn stage_tab(ui: &mut egui::Ui, state: &mut HomeState) {
    caption(ui, "STAGE");
    let mut any = false;
    let mut chosen: Option<(usize, sp_model::Uuid)> = None;
    for (index, screen) in state.screens.iter().enumerate().filter(|(_, screen)| screen.kind == Kind::Stage) {
        any = true;
        let status = state.screen_status.get(index).map_or("", |(status, _)| status.as_str());
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(screen.name.as_str()).size(13.0).color(TEXT));
            ui.label(egui::RichText::new(status).size(11.5).color(TEXT_DIM));
        });
        // The layout the screen is drawn with.
        let layout = crate::stage::layout_for(&state.stage_layouts, screen.stage_layout);
        egui::ComboBox::from_id_salt(("stage layout", screen.id))
            .selected_text(layout.name.as_str())
            .width(ui.available_width() - 8.0)
            .show_ui(ui, |ui| {
                for option in &state.stage_layouts {
                    if ui.add(egui::Button::selectable(option.id == layout.id, option.name.as_str())).clicked() {
                        chosen = Some((index, option.id));
                    }
                }
            });
    }
    if let Some((index, layout)) = chosen
        && let Some(screen) = state.screens.get_mut(index)
    {
        screen.stage_layout = Some(layout);
        state.screens_changed = true;
    }
    if !any {
        ui.label(egui::RichText::new("No stage screens yet.").size(12.0).color(TEXT_FAINT));
    }
    ui.horizontal(|ui| {
        if ui.button("Configure Screens…").clicked() {
            state.settings_open = true;
        }
        if ui.button("Edit Layouts…").on_hover_text("Lay out what the stage screens show").clicked() {
            state.edit_kind_request = Some(crate::home::EditKind::Stage);
        }
    });
    ui.add_space(10.0);
    caption(ui, "STAGE MESSAGE");
    let field = ui.add(
        egui::TextEdit::multiline(&mut state.stage_message)
            .desired_rows(2)
            .desired_width(f32::INFINITY)
            .hint_text("A note for the people on stage"),
    );
    if field.changed() && state.stage_message.trim().is_empty() {
        state.stage_message_live = false;
    }
    ui.label(
        egui::RichText::new("Appears in a red band at the top of the stage screens.").size(11.0).color(TEXT_FAINT),
    );
    let showing = state.stage_message_live;
    let label = if showing { "Hide" } else { "Show" };
    let has_words = !state.stage_message.trim().is_empty();
    let button = egui::Button::new(egui::RichText::new(label).color(Color32::WHITE))
        .fill(if showing { TALLY } else { crate::studio::SELECTED })
        .min_size(vec2(ui.available_width(), 30.0));
    if ui.add_enabled(has_words || showing, button).clicked() {
        state.stage_message_live = !showing;
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

/// A heading with a + at its right, for the tabs not yet in the new design. Returns true when
/// the + was clicked.
fn heading_with_add(ui: &mut egui::Ui, title: &str, hint: &str) -> bool {
    heading(ui, title, Some(hint)).1
}
