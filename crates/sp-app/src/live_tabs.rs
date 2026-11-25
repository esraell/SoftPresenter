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

use crate::home::HomeState;
use crate::i18n::{t, tf, tk, tn, ts};
use crate::icons::Icon;
use crate::messages::Message;
use crate::screens::Kind;
use crate::studio::{
    CARD, CARD_RUNNING, CARD_SHOWN, Flat, PANEL, RADIUS_CARD, READY, TALLY, TEXT, TEXT_DIM, TEXT_FAINT, Tabs, Thumb,
    fixed, fixed_semibold, flat_button, flat_button_width, icon_key, own_picture, paint_spaced, quiet_icon, semibold,
    signature, spaced, tab_switch, tab_switch_height,
};
use crate::timers::{self, Timer};

/// Draws the tabs and the one that is open. `thumb` draws the small pictures of props and stage
/// layouts.
pub fn live_tabs(ui: &mut egui::Ui, show: &sp_engine::Show, state: &mut HomeState, thumb: &mut Thumb) {
    let width = ui.available_width();
    // VoxLive's tab, once it is connected and has screens.
    let voxlive =
        state.voxlive_slot.and_then(|slot| show.presentations().get(slot)).is_some_and(|item| item.slide_count() > 0);
    let names: [&str; 7] = [
        t("right.tab.timers", "Timers"),
        t("right.tab.messages", "Messages"),
        t("right.tab.props", "Props"),
        t("right.tab.macros", "Macros"),
        t("right.tab.stage", "Stage"),
        t("right.tab.audio", "Audio"),
        t("right.tab.voxlive", "VoxLive"),
    ];
    let names = &names;
    // Each tab's key opens it and numbers what is in it (see `keys`).
    let hints: Vec<String> = names
        .iter()
        .enumerate()
        .map(|(at, name)| crate::keys::tab_hint(&tf("right.tab.numbered", "{tab}, numbered", &[("tab", name)]), at))
        .collect();
    let hints: Vec<&str> = hints.iter().map(String::as_str).collect();
    let tags: Vec<String> =
        if crate::keys::tags_shown(state) { (0..names.len()).map(crate::keys::tab_key).collect() } else { Vec::new() };
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
    let tabs = Tabs {
        hints: &hints[..count],
        dots: &dots[..count],
        tags: &tags[..tags.len().min(count)],
        ..Tabs::plain(&names[..count])
    };
    let (place, _) = ui.allocate_exact_size(vec2(width, tab_switch_height(&tabs)), Sense::hover());
    let (block, chosen) = tab_switch(ui, "live tabs", place.min, width, &tabs, Some(state.live_tab));
    if let Some(tab) = chosen {
        state.live_tab = tab;
    }
    // While the tab's things are numbered, the tabs are outlined in amber.
    if crate::keys::Numbers::showing(&state.numbers, crate::keys::NumbersOf::Tab) {
        let round = if count > 4 { 14.0 } else { block.height() / 2.0 };
        let edge = egui::Stroke::new(1.5, crate::studio::CARE);
        ui.painter().rect_stroke(block.expand(1.0), round + 1.0, edge, egui::StrokeKind::Outside);
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

/// While the numbers show on the open tab (after modifier-1 to 7), the card's number at its
/// top left corner; and whether it was just picked by its number, which does what the card's
/// main key does. `row` is the card's first line; `place` counts from 0.
fn numbered(ui: &egui::Ui, state: &HomeState, row: Rect, place: usize) -> bool {
    let card = Rect::from_min_max(
        row.min - vec2(f32::from(CARD_MARGIN.left), f32::from(CARD_MARGIN.top)),
        row.max + vec2(f32::from(CARD_MARGIN.right), f32::from(CARD_MARGIN.bottom)),
    );
    numbered_at(ui, &state.numbers, card, place)
}

/// The same for a thing whose whole rectangle is known (a stage layout's tile, a track's row).
fn numbered_at(ui: &egui::Ui, numbers: &Option<crate::keys::Numbers>, rect: Rect, place: usize) -> bool {
    use crate::keys::{Numbers, NumbersOf};
    if place < 9 && Numbers::showing(numbers, NumbersOf::Tab) {
        crate::keys::number_badge(ui.painter(), rect, place + 1);
    }
    Numbers::picked(numbers, NumbersOf::Tab, place)
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

/// Words for a place with a fixed room, filled in with `args`: the usual wording when it fits
/// `room` in `font` (with `spacing` between its letters, measured in capitals when `capitals`),
/// else the table's short form (`key.short`, as `studio::within` does), so nothing there ends in
/// "…".
fn fitting(
    ui: &egui::Ui,
    (key, english): (&'static str, &'static str),
    args: &[(&str, &str)],
    font: FontId,
    spacing: f32,
    room: f32,
    capitals: bool,
) -> String {
    let usual = crate::i18n::fill(t(key, english), args);
    let measured = if capitals { usual.to_uppercase() } else { usual.clone() };
    if spaced(ui, &measured, font, TEXT, spacing).size().x <= room {
        return usual;
    }
    crate::i18n::fill(ts(key, english), args)
}

/// A card's ⋯: always there, faded until the pointer is over it.
fn more_key(ui: &egui::Ui, rect: Rect, id: egui::Id) -> egui::Response {
    quiet_icon(ui, rect, id, Icon::More, 16.0, 0.6, None).on_hover_text(t("tools.more", "More"))
}

/// A field for a length of time or a time of day as `HH:MM:SS`. What is typed is kept while the
/// field has the keyboard and read when it is left. Returns `Some` once it is left with
/// something that reads (`Some(None)` when it was emptied).
fn clock_field(ui: &mut egui::Ui, id: egui::Id, current: Option<u32>, empty: &str) -> Option<Option<u32>> {
    let shown = current.map(|seconds| timers::clock_text(i64::from(seconds))).unwrap_or_default();
    let mut text = ui.data_mut(|data| data.get_temp::<String>(id)).unwrap_or(shown);
    let response = ui
        .add(egui::TextEdit::singleline(&mut text).id(id).background_color(PANEL).desired_width(96.0).hint_text(empty));
    if response.has_focus() {
        ui.data_mut(|data| data.insert_temp(id, text.clone()));
    }
    if response.lost_focus() {
        ui.data_mut(|data| data.remove::<String>(id));
        return if text.trim().is_empty() { Some(None) } else { timers::parse_clock(&text).map(Some) };
    }
    None
}

/// How long until the clock's next whole second, when a running timer's readout is drawn again.
/// Every timer, and the clock in the top bar, are drawn at the same moment, so the window is
/// drawn once a second however many run; a timer started part way through a second shows its
/// new second up to a second late.
fn until_next_second() -> Duration {
    let into_second =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.subsec_nanos());
    Duration::from_nanos(1_000_000_000 - u64::from(into_second))
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
    if ui.add(egui::Button::selectable(open, t("tools.menu.edit", "Edit…"))).clicked() {
        asked = Some(TimerAsk::Edit);
    }
    ui.separator();
    if ui.button(t("tools.menu.delete", "Delete")).clicked() {
        asked = Some(TimerAsk::Delete);
    }
    asked
}

fn timers_tab(ui: &mut egui::Ui, state: &mut HomeState) {
    let (_, add) = heading(ui, t("right.tab.timers", "Timers"), Some(t("tools.timer.new", "New timer")));
    if add {
        let taken = |name: &str| state.timers.iter().any(|timer| timer.name == name);
        let base = t("tools.timer.default_name", "Timer");
        let name = std::iter::once(base.to_owned())
            .chain((2..).map(|n| format!("{base} {n}")))
            .find(|name| !taken(name))
            .unwrap_or_default();
        let timer = Timer::new(name);
        state.timer_open = Some(timer.id);
        state.timers.push(timer);
        state.timers_changed = true;
    }
    if state.timers.is_empty() {
        empty_note(ui, t("tools.timer.none", "No timers. Press + to add one."));
        return;
    }
    let now = Instant::now();
    let day = timers::seconds_into_day();
    let mut asked: Option<(usize, TimerAsk)> = None;
    let next_tick = state.timers.iter().any(Timer::running).then(until_next_second);
    for index in 0..state.timers.len() {
        let id = state.timers[index].id;
        let open = state.timer_open == Some(id);
        let running = state.timers[index].running();
        card(ui, if running { CARD_RUNNING } else { CARD }, |ui| {
            let timer = &state.timers[index];
            let width = ui.available_width();
            // The time, in the fixed-width face: room is kept for its longest (with a minus sign
            // when the timer may run over), so the name beside it never moves.
            let time_font = fixed(17.0);
            let longest = if timer.allow_overrun { "-00:00:00" } else { "00:00:00" };
            // A countdown to a time of day says when it ends until it is started, rather than
            // counting while it is stopped; room is kept for that too.
            let ends = match timer.kind {
                timers::Kind::CountdownTo { hour, minute, .. } => {
                    Some(tf("tools.timer.to", "to {time}", &[("time", &format!("{hour:02}:{minute:02}"))]))
                }
                _ => None,
            };
            let time_wide = [Some(longest), ends.as_deref()]
                .into_iter()
                .flatten()
                .map(|words| spaced(ui, words, time_font.clone(), TEXT_DIM, -0.5).size().x)
                .fold(0.0, f32::max);
            let (time, ink) = match (running, ends) {
                (true, _) => (timer.text(now, day), READY),
                (false, Some(ends)) => (ends, TEXT_DIM),
                (false, None) => (timer.text(now, day), TEXT_DIM),
            };
            let name_wide = width - 3.0 * CARD_KEY - 4.0 * CARD_GAP - time_wide;
            let name = fitted(ui, &timer.name, name_font(), TEXT, name_wide, 2);
            let (row, _) = ui.allocate_exact_size(vec2(width, name.size().y.max(CARD_KEY)), Sense::hover());
            let back = card_back(ui, row, egui::Id::unique(("timer card", id)));
            if numbered(ui, state, row, index) {
                asked = Some((index, TimerAsk::Toggle));
            }
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
            if icon_key(ui, key(2.0), egui::Id::unique(("timer reset", id)), Icon::Back, 15.0, true)
                .on_hover_text(t("tools.timer.reset", "Reset"))
                .clicked()
            {
                asked = Some((index, TimerAsk::Reset));
            }
            let (icon, hint) = if running {
                (Icon::Pause, t("tools.timer.stop", "Stop"))
            } else {
                (Icon::Play, t("tools.timer.start", "Start"))
            };
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
                let (changed, renamed) = timer_settings(ui, &mut state.timers[index]);
                state.timers_changed |= changed;
                if let Some(name) = renamed
                    && let Err(problem) = state.rename_timer(index, &name)
                {
                    state.notices.refuse(problem);
                }
                ui.add_space(6.0);
                let done = Rect::from_min_size(ui.cursor().min, vec2(64.0, 24.0));
                ui.allocate_rect(done, Sense::hover());
                if flat_button(
                    ui,
                    done,
                    egui::Id::unique(("timer done", id)),
                    t("tools.done", "Done"),
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
        // At once: Undo puts it back.
        Some((index, TimerAsk::Delete)) => {
            state.delete_timer(index);
        }
        None => {}
    }
}

/// A timer's settings, under its card while they are open: its name, what it counts, and
/// whether it runs over. Returns true when one changed, and a new name once one is typed: it is
/// kept on Return or leaving the field (Escape puts the old one back), and then given to the
/// timer and the messages that show it together (see `HomeState::rename_timer`).
fn timer_settings(ui: &mut egui::Ui, timer: &mut Timer) -> (bool, Option<String>) {
    ui.add_space(8.0);
    let mut changed = false;
    let mut renamed = None;
    egui::Grid::new(("timer settings", timer.id)).num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
        ui.label(egui::RichText::new(t("tools.timer.name", "Name")).color(TEXT_DIM));
        // The name as typed so far, while the field has the keyboard.
        let field_id = egui::Id::unique(("timer name", timer.id));
        let draft_id = field_id.with("typed");
        let mut typed = ui.data(|data| data.get_temp::<String>(draft_id)).unwrap_or_else(|| timer.name.clone());
        let field =
            ui.add(egui::TextEdit::singleline(&mut typed).id(field_id).background_color(PANEL).desired_width(160.0));
        if field.lost_focus() {
            let escape = ui.input(|input| input.key_pressed(egui::Key::Escape));
            if !escape && typed.trim() != timer.name {
                renamed = Some(typed.trim().to_owned());
            }
            ui.data_mut(|data| data.remove::<String>(draft_id));
        } else if field.has_focus() {
            ui.data_mut(|data| data.insert_temp(draft_id, typed));
        } else {
            ui.data_mut(|data| data.remove::<String>(draft_id));
        }
        ui.end_row();
        ui.label(egui::RichText::new(t("tools.timer.counts", "Counts")).color(TEXT_DIM));
        let (down_from, down_to, up) = (
            t("tools.timer.kind.down_from", "Down from a length"),
            t("tools.timer.kind.down_to", "Down to a time of day"),
            t("tools.timer.kind.up", "Up"),
        );
        let kind_name = match timer.kind {
            timers::Kind::Countdown { .. } => down_from,
            timers::Kind::CountdownTo { .. } => down_to,
            timers::Kind::Elapsed { .. } => up,
        };
        // Wide enough for the longest choice in the language set, beside the arrow.
        let font = egui::TextStyle::Button.resolve(ui.style());
        let wide = (crate::studio::widest(ui, &[down_from, down_to, up], &font) + 40.0).max(160.0);
        egui::ComboBox::from_id_salt(("timer type", timer.id)).selected_text(kind_name).width(wide).show_ui(ui, |ui| {
            let choices = [
                (down_from, timers::Kind::Countdown { seconds: 300 }),
                (down_to, timers::Kind::CountdownTo { hour: 11, minute: 0, second: 0 }),
                (up, timers::Kind::Elapsed { start: 0, end: None }),
            ];
            for (name, kind) in choices {
                let same = std::mem::discriminant(&kind) == std::mem::discriminant(&timer.kind);
                if ui.add(egui::Button::selectable(same, name)).clicked() && !same {
                    timer.kind = kind;
                    timer.reset();
                    changed = true;
                }
            }
        });
        ui.end_row();
        match timer.kind {
            timers::Kind::Countdown { seconds } => {
                ui.label(egui::RichText::new(t("tools.timer.length", "Length")).color(TEXT_DIM));
                if let Some(Some(seconds)) =
                    clock_field(ui, ui.scope_id().with(("duration", timer.id)), Some(seconds), "00:05:00")
                {
                    timer.kind = timers::Kind::Countdown { seconds };
                    changed = true;
                }
                ui.end_row();
            }
            timers::Kind::CountdownTo { hour, minute, second } => {
                ui.label(egui::RichText::new(t("tools.timer.time", "Time")).color(TEXT_DIM));
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
                    ui.label(egui::RichText::new(t("tools.timer.24_hour", "24-hour")).size(11.0).color(TEXT_FAINT));
                });
                ui.end_row();
            }
            timers::Kind::Elapsed { start, end } => {
                ui.label(egui::RichText::new(t("tools.timer.from", "From")).color(TEXT_DIM));
                if let Some(Some(start)) =
                    clock_field(ui, ui.scope_id().with(("start", timer.id)), Some(start), "00:00:00")
                {
                    timer.kind = timers::Kind::Elapsed { start, end };
                    changed = true;
                }
                ui.end_row();
                ui.label(egui::RichText::new(t("tools.timer.stops_at", "Stops at")).color(TEXT_DIM));
                let never = t("tools.timer.never", "Never");
                if let Some(end) = clock_field(ui, ui.scope_id().with(("end", timer.id)), end, never) {
                    timer.kind = timers::Kind::Elapsed { start, end };
                    changed = true;
                }
                ui.end_row();
            }
        }
    });
    changed |= ui
        .checkbox(&mut timer.allow_overrun, t("tools.timer.runs_over", "Runs over (counts on past the end)"))
        .changed();
    (changed, renamed)
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
    if ui.add(egui::Button::selectable(open, t("tools.menu.edit", "Edit…"))).clicked() {
        asked = Some(MessageAsk::Edit);
    }
    if ui.button(t("tools.menu.rename", "Rename")).clicked() {
        asked = Some(MessageAsk::Rename);
    }
    if ui.button(t("tools.menu.duplicate", "Duplicate")).clicked() {
        asked = Some(MessageAsk::Duplicate);
    }
    ui.separator();
    if ui.button(t("tools.menu.cut", "Cut")).clicked() {
        asked = Some(MessageAsk::Cut);
    }
    if ui.button(t("tools.menu.copy", "Copy")).clicked() {
        asked = Some(MessageAsk::Copy);
    }
    let paste = ui.add_enabled(has_clip, egui::Button::new(t("tools.menu.paste", "Paste")));
    if paste.on_disabled_hover_text(t("tools.message.paste_first", "Copy or cut a message first")).clicked() {
        asked = Some(MessageAsk::Paste);
    }
    ui.separator();
    if ui.button(t("tools.menu.delete", "Delete")).clicked() {
        asked = Some(MessageAsk::Delete);
    }
    asked
}

/// What a message's card says under its name: its words on one line, when it leaves by itself,
/// and its look.
fn message_summary(message: &Message) -> String {
    let words = message.text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut parts = vec![if words.is_empty() { t("tools.message.no_words", "No words yet").to_owned() } else { words }];
    if let Some(seconds) = message.dismiss_after {
        parts.push(tf(
            "tools.message.summary.leaves",
            "leaves after {seconds} s",
            &[("seconds", &seconds.to_string())],
        ));
    }
    parts.push(message.look.name().to_owned());
    parts.join(" \u{b7} ")
}

/// "Words once…" in the Messages heading, left of its +: a small box to type words in and show
/// them over the slides at once. They are kept as a message ("Quick message", as `sp message
/// show --text` keeps them), to show again.
fn words_once(ui: &mut egui::Ui, row: Rect, state: &mut HomeState) {
    let font = FontId::proportional(11.5);
    let words_once = t("tools.message.words_once", "Words once…");
    let wide = flat_button_width(ui, &[words_once], &font, 9.0);
    let rect = Rect::from_min_size(pos2(row.max.x - 28.0 - 8.0 - wide, row.center().y - 12.0), vec2(wide, 24.0));
    let button = flat_button(ui, rect, egui::Id::unique("words once"), words_once, font, Flat::Plain, true)
        .on_hover_text(t("tools.message.words_once.hint", "Type some words and show them over the slides at once"));
    let draft = egui::Id::unique("words once draft");
    egui::Popup::from_toggle_button_response(&button)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            ui.set_width(250.0);
            let mut words = ui.data_mut(|data| data.get_temp::<String>(draft)).unwrap_or_default();
            let field = ui.add(
                egui::TextEdit::multiline(&mut words)
                    .background_color(CARD)
                    .desired_rows(2)
                    .desired_width(f32::INFINITY)
                    .hint_text(t("tools.message.words_once.field", "Words to show over the slides")),
            );
            if !field.has_focus() && words.is_empty() {
                field.request_focus();
            }
            ui.data_mut(|data| data.insert_temp(draft, words.clone()));
            ui.label(
                egui::RichText::new(tf(
                    "tools.message.quick.kept",
                    "Kept as the message {name}, to show again or change.",
                    &[("name", &crate::messages::quick_name(&state.messages))],
                ))
                .size(11.0)
                .color(TEXT_FAINT),
            );
            let (place, _) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::hover());
            let show = Rect::from_min_size(pos2(place.max.x - 64.0, place.min.y), vec2(64.0, 28.0));
            let can = !words.trim().is_empty();
            let id = egui::Id::unique("words once show");
            if flat_button(ui, show, id, t("tools.show", "Show"), FontId::proportional(12.5), Flat::Main, can).clicked()
            {
                let at = crate::messages::quick_message(&mut state.messages);
                state.messages[at].text = words.trim().to_owned();
                state.messages_changed = true;
                state.message_live = Some((state.messages[at].id, Instant::now()));
                ui.data_mut(|data| data.remove::<String>(draft));
                ui.close();
            }
        });
}

fn messages_tab(ui: &mut egui::Ui, state: &mut HomeState) {
    let (row, add) = heading(ui, t("right.tab.messages", "Messages"), Some(t("tools.message.new", "New message")));
    words_once(ui, row, state);
    if add {
        let taken = |name: &str| state.messages.iter().any(|message| message.name == name);
        let base = t("tools.message.default_name", "New Message");
        let name = (1..).map(|n| format!("{base} {n}")).find(|name| !taken(name)).unwrap_or_default();
        let message = Message::new(name);
        state.message_selected = Some(message.id);
        state.messages.push(message);
        state.messages_changed = true;
    }
    if state.messages.is_empty() {
        empty_note(
            ui,
            t("tools.message.none", "No messages. A message shows over the slides, such as a notice for parents."),
        );
        return;
    }
    let live = state.message_live.map(|(id, _)| id);
    let has_clip = state.message_clip.is_some();
    let mut asked: Option<(usize, MessageAsk)> = None;
    let mut renamed: Option<Option<(sp_model::Uuid, String)>> = None;
    let mut rename_started = std::mem::take(&mut state.rename_started);
    let timer_names: Vec<String> = state.timers.iter().map(|timer| timer.name.clone()).collect();
    let button_font = FontId::proportional(12.5);
    let button_wide = flat_button_width(ui, &[t("tools.show", "Show"), t("tools.hide", "Hide")], &button_font, 13.0);
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
            if numbered(ui, state, row, index) {
                asked = Some((index, MessageAsk::Toggle));
            }
            let top = row.center().y - block / 2.0;
            let (name_tall, first_line) = (name.size().y, name.rows.first().map_or(15.0, |row| row.height()));
            match state.message_rename.as_mut().filter(|(renaming, _)| *renaming == id) {
                Some((_, typed)) => {
                    let start = pos2(row.min.x, top + first_line / 2.0);
                    let right = row.min.x + text_wide;
                    let look = crate::studio::Lettering::plain(name_font(), TEXT);
                    let field = crate::studio::rename_field_at(ui, typed, &look, start, right);
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
                (
                    t("tools.hide", "Hide"),
                    Flat::On,
                    crate::keys::clear_hint(t("tools.message.hide.hint", "Take this message off the screens"), "F6"),
                )
            } else {
                (
                    t("tools.show", "Show"),
                    Flat::Plain,
                    t("tools.message.show.hint", "Show this message over the slides on the audience screens")
                        .to_owned(),
                )
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
                if flat_button(
                    ui,
                    done,
                    egui::Id::unique(("message done", id)),
                    t("tools.done", "Done"),
                    font,
                    Flat::Plain,
                    true,
                )
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
        // Cut and Delete take it out at once: Undo puts it back.
        MessageAsk::Cut => state.message_clip = state.delete_message(index),
        MessageAsk::Paste => {
            if let Some(clip) = &state.message_clip {
                // A pasted message is a new one: it gets an id of its own, so it can be pasted again,
                // and is never the one words shown once are kept as.
                let pasted = Message { id: sp_model::Uuid::new_v4(), quick: false, ..clip.clone() };
                state.messages.insert(index + 1, pasted);
                state.messages_changed = true;
            }
        }
        MessageAsk::Delete => {
            state.delete_message(index);
        }
    }
}

/// A message's settings, under its card while they are open: its name, its words, the fill-in
/// tokens, its look and when it leaves. Returns true when one changed.
fn message_settings(ui: &mut egui::Ui, message: &mut Message, timer_names: &[String]) -> bool {
    ui.add_space(8.0);
    let mut changed = false;
    changed |= ui
        .add(egui::TextEdit::singleline(&mut message.name).background_color(PANEL).desired_width(f32::INFINITY))
        .changed();
    changed |= ui
        .add(
            egui::TextEdit::multiline(&mut message.text)
                .background_color(PANEL)
                .desired_rows(3)
                .desired_width(f32::INFINITY)
                .hint_text(t("tools.message.words_hint", "Words to show")),
        )
        .changed();
    ui.horizontal(|ui| {
        let add_token = t("tools.message.add_token", "Add Token…");
        egui::ComboBox::from_id_salt(("add token", message.id)).selected_text(add_token).width(130.0).show_ui(
            ui,
            |ui| {
                if ui.button(t("tools.message.token.clock", "System Clock")).clicked() {
                    message.text.push_str(crate::messages::CLOCK_TOKEN);
                    changed = true;
                }
                for name in timer_names {
                    if ui.button(tf("tools.message.token.timer", "Timer: {name}", &[("name", name)])).clicked() {
                        message.text.push_str(&crate::messages::timer_token(name));
                        changed = true;
                    }
                }
            },
        );
        // Wide enough for the longest look in the language set, beside the arrow.
        let names: Vec<&str> = crate::messages::Look::ALL.iter().map(|look| look.name()).collect();
        let font = egui::TextStyle::Button.resolve(ui.style());
        let wide = (crate::studio::widest(ui, &names, &font) + 40.0).max(150.0);
        egui::ComboBox::from_id_salt(("message look", message.id))
            .selected_text(message.look.name())
            .width(wide)
            .show_ui(ui, |ui| {
                for look in crate::messages::Look::ALL {
                    if ui.add(egui::Button::selectable(message.look == look, look.name())).clicked() {
                        message.look = look;
                        changed = true;
                    }
                }
            });
    });
    let leaves = |choice: Option<u32>| match choice {
        None => t("tools.message.stays", "Stays until taken off").to_owned(),
        Some(seconds) => {
            tf("tools.message.leaves_after", "Leaves after {seconds} seconds", &[("seconds", &seconds.to_string())])
        }
    };
    let choices = [None, Some(5), Some(10), Some(15), Some(20), Some(30), Some(45), Some(60)];
    let labels: Vec<String> = choices.iter().map(|choice| leaves(*choice)).collect();
    // Wide enough for the longest choice in the language set, beside the arrow.
    let font = egui::TextStyle::Button.resolve(ui.style());
    let wide =
        (crate::studio::widest(ui, &labels.iter().map(String::as_str).collect::<Vec<_>>(), &font) + 40.0).max(180.0);
    let shown = leaves(message.dismiss_after);
    egui::ComboBox::from_id_salt(("dismiss", message.id)).selected_text(shown).width(wide).show_ui(ui, |ui| {
        for (choice, label) in choices.into_iter().zip(labels) {
            if ui.add(egui::Button::selectable(message.dismiss_after == choice, label)).clicked() {
                message.dismiss_after = choice;
                changed = true;
            }
        }
    });
    changed
}

/// A prop's picture in `rect`, drawn with the props' theme, again only when the prop changes.
/// The Props tab and the Edit view's list of props draw it the same way, and share it.
pub fn prop_picture(ui: &egui::Ui, thumb: &mut Thumb, rect: Rect, corner: f32, prop: &crate::props::Prop) {
    let key = sp_model::Uuid::from_u128(prop.id.as_u128() ^ 0x7072_6f70_7069_6374_7572_6500_0000_0000);
    let said = signature(&[&serde_json::to_string(&prop.slide).unwrap_or_default()]);
    own_picture(ui, thumb, rect, corner, key, &prop.slide, prop_theme(), said);
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

/// Something asked of a prop: from its card in the Props tab, or from the list of props in the
/// Edit view.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PropAsk {
    Toggle,
    Edit,
    Rename,
    Duplicate,
    AutoClear(Option<u32>),
    Delete,
    /// A new prop, at the end of the list, with its name ready to be typed over.
    New,
    /// One place up the list (true) or down.
    Move(bool),
}

/// A prop's menu: behind its ⋯ and a right-click on its card in the Props tab, and a
/// right-click on its picture in the Edit view (`in_editor`), where it is open already, so
/// Edit in the Editor isn't offered, and where Delete asks first.
pub fn prop_menu(ui: &mut egui::Ui, auto_clear: Option<u32>, in_editor: bool) -> Option<PropAsk> {
    let mut asked = None;
    if !in_editor && ui.button(t("tools.prop.edit", "Edit in the Editor")).clicked() {
        asked = Some(PropAsk::Edit);
    }
    ui.menu_button(t("tools.prop.clears_itself", "Clears Itself"), |ui| {
        for choice in [None, Some(5), Some(10), Some(15), Some(20), Some(30), Some(45), Some(60)] {
            let label = choice.map_or(t("tools.prop.never", "Never").to_owned(), |seconds| {
                tf("tools.prop.after", "After {seconds} seconds", &[("seconds", &seconds.to_string())])
            });
            if ui.add(egui::Button::selectable(auto_clear == choice, label)).clicked() {
                asked = Some(PropAsk::AutoClear(choice));
                ui.close();
            }
        }
    });
    if ui.button(t("tools.menu.rename", "Rename")).clicked() {
        asked = Some(PropAsk::Rename);
    }
    if ui.button(t("tools.menu.duplicate", "Duplicate")).clicked() {
        asked = Some(PropAsk::Duplicate);
    }
    ui.separator();
    if ui.button(t("tools.menu.delete", "Delete")).clicked() {
        asked = Some(PropAsk::Delete);
    }
    asked
}

/// Carries out what was asked of the prop at `index` (for New, of none). Returns where the prop
/// it made (New, Duplicate) or moved (Move) now is.
pub fn carry_out_prop_ask(state: &mut HomeState, index: usize, ask: PropAsk) -> Option<usize> {
    if ask == PropAsk::New {
        let taken = |name: &str| state.props.iter().any(|prop| prop.name == name);
        let base = t("tools.prop.default_name", "Prop");
        let name = (1..).map(|n| format!("{base} {n}")).find(|name| !taken(name)).unwrap_or_default();
        let prop = crate::props::Prop::new(name.clone());
        state.prop_rename = Some((prop.id, name));
        state.rename_started = true;
        state.props.push(prop);
        state.props_changed = true;
        return Some(state.props.len() - 1);
    }
    let id = state.props.get(index)?.id;
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
            return Some(index + 1);
        }
        PropAsk::AutoClear(choice) => {
            state.props[index].auto_clear = choice;
            state.props_changed = true;
        }
        // At once: Undo puts it back.
        PropAsk::Delete => {
            state.delete_prop(index);
        }
        PropAsk::Move(up) => {
            let to = if up { index.checked_sub(1)? } else { index + 1 };
            if to >= state.props.len() {
                return None;
            }
            state.props.swap(index, to);
            state.props_changed = true;
            return Some(to);
        }
        PropAsk::New => {}
    }
    None
}

fn props_tab(ui: &mut egui::Ui, state: &mut HomeState, thumb: &mut Thumb) {
    let (_, add) = heading(ui, t("right.tab.props", "Props"), Some(t("tools.prop.new", "New prop")));
    if add {
        carry_out_prop_ask(state, 0, PropAsk::New);
    }
    if state.props.is_empty() {
        empty_note(
            ui,
            t(
                "tools.prop.none",
                "No props. A prop stays over the slides until it is taken off, such as a name in a corner.",
            ),
        );
        return;
    }
    let mut asked: Option<(usize, PropAsk)> = None;
    let mut renamed: Option<Option<(sp_model::Uuid, String)>> = None;
    let mut rename_started = std::mem::take(&mut state.rename_started);
    let button_font = FontId::proportional(12.5);
    let button_wide = flat_button_width(ui, &[t("tools.show", "Show"), t("tools.hide", "Hide")], &button_font, 13.0);
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
            // In one line beside the picture: a short form where the usual words don't fit.
            let small = FontId::proportional(12.0);
            let leaves = match prop.auto_clear {
                None => fitting(
                    ui,
                    tk("tools.prop.stays", "Stays until taken off"),
                    &[],
                    small.clone(),
                    0.0,
                    text_wide,
                    false,
                ),
                Some(seconds) => fitting(
                    ui,
                    tk("tools.prop.clears_after", "Clears itself after {seconds} s"),
                    &[("seconds", &seconds.to_string())],
                    small.clone(),
                    0.0,
                    text_wide,
                    false,
                ),
            };
            let sub = fitted(ui, &leaves, small, TEXT_FAINT, text_wide, 1);
            let block = name.size().y + sub.size().y;
            let (row, _) = ui.allocate_exact_size(vec2(width, block.max(CARD_KEY).max(picture.y)), Sense::hover());
            let back = card_back(ui, row, egui::Id::unique(("prop card", id)));
            if numbered(ui, state, row, index) {
                asked = Some((index, PropAsk::Toggle));
            }
            let y = row.center().y;
            let place = Rect::from_min_size(pos2(row.min.x, y - picture.y / 2.0), picture);
            prop_picture(ui, thumb, place, 4.0, prop);
            let left = place.max.x + CARD_GAP;
            let top = y - block / 2.0;
            let (name_tall, first_line) = (name.size().y, name.rows.first().map_or(15.0, |row| row.height()));
            match state.prop_rename.as_mut().filter(|(renaming, _)| *renaming == id) {
                Some((_, typed)) => {
                    let start = pos2(left, top + first_line / 2.0);
                    let look = crate::studio::Lettering::plain(name_font(), TEXT);
                    let field = crate::studio::rename_field_at(ui, typed, &look, start, left + text_wide);
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
                (t("tools.hide", "Hide"), Flat::On, t("tools.prop.hide.hint", "Take this prop off the screens"))
            } else {
                (
                    t("tools.show", "Show"),
                    Flat::Plain,
                    t("tools.prop.show.hint", "Show this prop over the slides on the audience screens"),
                )
            };
            let toggle = egui::Id::unique(("prop show", id));
            if flat_button(ui, button, toggle, words, button_font.clone(), look, true).on_hover_text(hint).clicked() {
                asked = Some((index, PropAsk::Toggle));
            }
            let auto_clear = prop.auto_clear;
            let more = more_key(ui, more, egui::Id::unique(("prop more", id)));
            egui::Popup::menu(&more).show(|ui| {
                if let Some(ask) = prop_menu(ui, auto_clear, false) {
                    asked = Some((index, ask));
                }
            });
            if back.double_clicked() {
                asked = Some((index, PropAsk::Edit));
            }
            back.on_hover_text(t("tools.prop.double_click", "Double-click to change it in the editor")).context_menu(
                |ui| {
                    if let Some(ask) = prop_menu(ui, auto_clear, false) {
                        asked = Some((index, ask));
                    }
                },
            );
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
    if let Some((index, ask)) = asked {
        carry_out_prop_ask(state, index, ask);
    }
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
    step_words(show, state, action).0
}

/// An action in words, and whether what it names (a song, a message, a prop, a timer, a picture,
/// a media playlist or a theme version) has gone.
fn step_words(show: &sp_engine::Show, state: &HomeState, action: &sp_model::ShowAction) -> (String, bool) {
    let gone = std::cell::Cell::new(false);
    let found = |name: Option<String>| {
        gone.set(gone.get() || name.is_none());
        name
    };
    let words = crate::macros::said(
        action,
        |id| found(state.messages.iter().find(|m| m.id == id).map(|m| m.name.clone())),
        |id| found(state.props.iter().find(|p| p.id == id).map(|p| p.name.clone())),
        |id| found(state.timers.iter().find(|t| t.id == id).map(|t| t.name.clone())),
        |_| found(crate::macros::step_song_title(show, state, action)),
        |id| found(show.media().iter().find(|item| item.id == id).map(|item| item.name.clone())),
        |id| found(state.media_playlists.iter().find(|p| p.id == id).map(|p| p.name.clone())),
        |id| found(state.versions.iter().find(|v| v.id == id).map(|v| v.name.clone())),
    );
    (words, gone.get())
}

/// The groups of steps a macro or a slide can do, in the order their menus list them.
#[derive(Clone, Copy, PartialEq, Eq)]
enum StepGroup {
    Clear,
    Messages,
    Props,
    Timers,
    Audience,
    Song,
    Media,
    Announcements,
    Version,
    CanvaSync,
}

impl StepGroup {
    const ALL: [StepGroup; 10] = [
        StepGroup::Clear,
        StepGroup::Messages,
        StepGroup::Props,
        StepGroup::Timers,
        StepGroup::Audience,
        StepGroup::Song,
        StepGroup::Media,
        StepGroup::Announcements,
        StepGroup::Version,
        StepGroup::CanvaSync,
    ];

    /// The group a step is in.
    fn of(step: &sp_model::ShowAction) -> StepGroup {
        use sp_model::ShowAction;
        match step {
            ShowAction::Clear { .. } => StepGroup::Clear,
            ShowAction::Message { .. } => StepGroup::Messages,
            ShowAction::Prop { .. } => StepGroup::Props,
            ShowAction::Timer { .. } => StepGroup::Timers,
            ShowAction::Audience { .. } => StepGroup::Audience,
            ShowAction::Song { .. } => StepGroup::Song,
            ShowAction::Media { .. } => StepGroup::Media,
            ShowAction::Announce { .. } => StepGroup::Announcements,
            ShowAction::Version { .. } => StepGroup::Version,
            ShowAction::CanvaSync => StepGroup::CanvaSync,
        }
    }

    fn title(self) -> &'static str {
        match self {
            StepGroup::Clear => t("tools.steps.clear", "Clear"),
            StepGroup::Messages => t("right.tab.messages", "Messages"),
            StepGroup::Props => t("right.tab.props", "Props"),
            StepGroup::Timers => t("right.tab.timers", "Timers"),
            StepGroup::Audience => t("tools.steps.audience", "Audience Screens"),
            StepGroup::Song => t("tools.steps.song", "Go to a Song"),
            StepGroup::Media => t("tools.steps.media", "Show a Picture or Video"),
            StepGroup::Announcements => t("tools.steps.announcements", "Announcements"),
            StepGroup::Version => t("tools.steps.version", "Theme Version"),
            StepGroup::CanvaSync => t("tools.step.canva", "Sync from Canva"),
        }
    }
}

/// A button in a menu that opens `contents` beside it, marked as chosen when `marked`. Returns
/// what `contents` returned, while it is open.
fn submenu<R>(ui: &mut egui::Ui, marked: bool, title: &str, contents: impl FnOnce(&mut egui::Ui) -> R) -> Option<R> {
    use egui::containers::menu::SubMenuButton;
    let button = egui::Button::selectable(marked, title).right_text(SubMenuButton::arrow_atom(None));
    SubMenuButton::from_button(button).ui(ui, contents).1.map(|shown| shown.inner)
}

/// A small heading at the top of a menu, in faint capitals.
pub(crate) fn menu_heading(ui: &mut egui::Ui, words: &str) {
    let indent = ui.spacing().button_padding.x;
    ui.horizontal(|ui| {
        ui.add_space(indent);
        let words = egui::RichText::new(words.to_uppercase()).font(fixed_semibold(9.5)).extra_letter_spacing(0.7);
        ui.label(words.color(TEXT_FAINT));
    });
}

/// The steps of one group, each a button, the one `current` is marked; Announcements has a
/// menu for each kind of thing it plays. Returns the one chosen.
fn group_steps(
    ui: &mut egui::Ui,
    show: &sp_engine::Show,
    state: &HomeState,
    group: StepGroup,
    current: Option<&sp_model::ShowAction>,
) -> Option<sp_model::ShowAction> {
    use sp_model::{Announced, ClearWhat, ShowAction, TimerDo};
    let mut chosen = None;
    let mut offer = |ui: &mut egui::Ui, words: &str, step: ShowAction| {
        if ui.add(egui::Button::selectable(current == Some(&step), words)).clicked() {
            chosen = Some(step);
        }
    };
    let hidden = state.bible.slots.unwrap_or([usize::MAX; 2]);
    match group {
        StepGroup::Clear => {
            for what in ClearWhat::ALL {
                offer(ui, crate::macros::clear_words(what), ShowAction::Clear { what });
            }
        }
        StepGroup::Messages => {
            if state.messages.is_empty() {
                ui.label(t("tools.steps.no_messages", "No messages yet"));
            }
            for message in &state.messages {
                for show in [true, false] {
                    let words = if show {
                        tf("tools.steps.show_named", "Show {name}", &[("name", &message.name)])
                    } else {
                        tf("tools.steps.hide_named", "Hide {name}", &[("name", &message.name)])
                    };
                    offer(ui, &words, ShowAction::Message { id: message.id, show });
                }
            }
        }
        StepGroup::Props => {
            if state.props.is_empty() {
                ui.label(t("tools.steps.no_props", "No props yet"));
            }
            for prop in &state.props {
                for show in [true, false] {
                    let words = if show {
                        tf("tools.steps.show_named", "Show {name}", &[("name", &prop.name)])
                    } else {
                        tf("tools.steps.hide_named", "Hide {name}", &[("name", &prop.name)])
                    };
                    offer(ui, &words, ShowAction::Prop { id: prop.id, show });
                }
            }
        }
        StepGroup::Timers => {
            if state.timers.is_empty() {
                ui.label(t("tools.steps.no_timers", "No timers yet"));
            }
            for timer in &state.timers {
                for act in TimerDo::ALL {
                    let template = match act {
                        TimerDo::Start => t("tools.steps.timer_start", "Start {name}"),
                        TimerDo::Stop => t("tools.steps.timer_stop", "Stop {name}"),
                        TimerDo::Reset => t("tools.steps.timer_reset", "Reset {name}"),
                    };
                    let words = crate::i18n::fill(template, &[("name", &timer.name)]);
                    offer(ui, &words, ShowAction::Timer { id: timer.id, act });
                }
            }
        }
        StepGroup::Audience => {
            for on in [true, false] {
                let words = if on { t("tools.steps.audience_on", "On") } else { t("tools.steps.audience_off", "Off") };
                offer(ui, words, ShowAction::Audience { on });
            }
        }
        StepGroup::Song => {
            egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                for (key, song) in listed_songs(show, state, hidden) {
                    offer(ui, song.title.as_str(), ShowAction::Song { key: key.to_owned() });
                }
            });
        }
        StepGroup::Media => {
            if show.media().is_empty() {
                ui.label(t("tools.steps.no_media", "No pictures or videos in the media bin yet"));
            }
            egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                for item in show.media() {
                    offer(ui, item.name.as_str(), ShowAction::Media { id: item.id });
                }
            });
        }
        // Something that goes round on the Announcements layer: a media playlist, a picture or
        // a video, or a song.
        StepGroup::Announcements => {
            let playing = match current {
                Some(ShowAction::Announce { what }) => Some(what),
                _ => None,
            };
            let announce = |what| ShowAction::Announce { what };
            let marked = matches!(playing, Some(Announced::MediaPlaylist { .. }));
            submenu(ui, marked, t("tools.steps.media_playlist", "Media Playlist"), |ui| {
                if state.media_playlists.is_empty() {
                    ui.label(t("tools.steps.no_media_playlists", "No media playlists yet"));
                }
                for playlist in &state.media_playlists {
                    offer(ui, playlist.name.as_str(), announce(Announced::MediaPlaylist { id: playlist.id }));
                }
            });
            let marked = matches!(playing, Some(Announced::Media { .. }));
            submenu(ui, marked, t("tools.steps.picture_or_video", "Picture or Video"), |ui| {
                if show.media().is_empty() {
                    ui.label(t("tools.steps.no_media", "No pictures or videos in the media bin yet"));
                }
                egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                    for item in show.media() {
                        offer(ui, item.name.as_str(), announce(Announced::Media { id: item.id }));
                    }
                });
            });
            let marked = matches!(playing, Some(Announced::Song { .. }));
            submenu(ui, marked, t("tools.steps.announce_song", "Song"), |ui| {
                egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                    for (key, song) in listed_songs(show, state, hidden) {
                        offer(ui, song.title.as_str(), announce(Announced::Song { key: key.to_owned() }));
                    }
                });
            });
        }
        // A theme version on every screen whose theme has it, kept or not, or the usual look.
        StepGroup::Version => {
            if state.versions.is_empty() {
                ui.label(t("tools.steps.no_versions", "No versions yet: add one in Edit \u{203a} Themes"));
            }
            for version in &state.versions {
                for keep in [false, true] {
                    let step = ShowAction::Version { id: Some(version.id), keep };
                    offer(ui, &describe_action(show, state, &step), step);
                }
            }
            let usual = ShowAction::Version { id: None, keep: false };
            offer(ui, &describe_action(show, state, &usual), usual);
        }
        StepGroup::CanvaSync => {
            let hint = t(
                "tools.steps.canva.hint",
                "Fetch a fresh announcements video (Settings \u{203a} Integrations \u{203a} Canva Sync)",
            );
            let marked = current == Some(&ShowAction::CanvaSync);
            if ui.add(egui::Button::selectable(marked, group.title())).on_hover_text(hint).clicked() {
                chosen = Some(ShowAction::CanvaSync);
            }
        }
    }
    chosen
}

/// Every step a macro or a slide can do, in groups (Clear, Messages, Props, Timers, Audience
/// Screens, Go to a Song, Show a Picture or Video, Announcements, Theme Version), each opening
/// its steps, and Sync from Canva on its own. The one menu that adds a step, changes one, and
/// adds a slide's action, so they never drift apart. `current` is the step being changed: its
/// group and its step are marked. Returns the one chosen.
fn step_groups(
    ui: &mut egui::Ui,
    show: &sp_engine::Show,
    state: &HomeState,
    current: Option<&sp_model::ShowAction>,
) -> Option<sp_model::ShowAction> {
    let own = current.map(StepGroup::of);
    let mut chosen = None;
    for group in StepGroup::ALL {
        let step = if group == StepGroup::CanvaSync {
            group_steps(ui, show, state, group, current)
        } else {
            submenu(ui, own == Some(group), group.title(), |ui| group_steps(ui, show, state, group, current)).flatten()
        };
        chosen = chosen.or(step);
    }
    chosen
}

/// A macro's menu of steps under a heading: "Add a step" from + Add a step, "Change this step
/// to" from a step, with `current` that step. Returns the step chosen.
fn step_menu(
    ui: &mut egui::Ui,
    show: &sp_engine::Show,
    state: &HomeState,
    heading: &str,
    current: Option<&sp_model::ShowAction>,
) -> Option<sp_model::ShowAction> {
    menu_heading(ui, heading);
    step_groups(ui, show, state, current)
}

/// A slide's Add Action menu: every step it can do, in their groups. Returns the one chosen.
pub fn add_action_menu(ui: &mut egui::Ui, show: &sp_engine::Show, state: &HomeState) -> Option<sp_model::ShowAction> {
    step_groups(ui, show, state, None)
}

/// Something asked of a macro from its card.
enum MacroAsk {
    Run,
    Steps,
    Rename,
    Color(sp_model::Rgba),
    Startup,
    Add(sp_model::ShowAction),
    /// The step at a place does this instead.
    Change(usize, sp_model::ShowAction),
    /// The step at a place goes to another (both counting from 0).
    Move(usize, usize),
    Remove(usize),
    Duplicate,
    Delete,
}

/// A macro's menu, behind its ⋯ and a right-click on its card.
fn macro_menu(ui: &mut egui::Ui, made: &crate::macros::Macro, open: bool) -> Option<MacroAsk> {
    let mut asked = None;
    if ui.add(egui::Button::selectable(open, t("tools.macro.edit_steps", "Edit Steps…"))).clicked() {
        asked = Some(MacroAsk::Steps);
    }
    ui.menu_button(t("tools.macro.colour", "Colour"), |ui| {
        for ((key, english), color) in crate::macros::COLORS {
            if ui.add(egui::Button::selectable(made.color == color, t(key, english))).clicked() {
                asked = Some(MacroAsk::Color(color));
                ui.close();
            }
        }
    });
    let startup = t("tools.macro.run_at_start.menu", "Run When SoftPresenter Opens");
    if ui.add(egui::Button::selectable(made.on_startup, startup)).clicked() {
        asked = Some(MacroAsk::Startup);
    }
    if ui.button(t("tools.menu.rename", "Rename")).clicked() {
        asked = Some(MacroAsk::Rename);
    }
    if ui.button(t("tools.menu.duplicate", "Duplicate")).clicked() {
        asked = Some(MacroAsk::Duplicate);
    }
    ui.separator();
    if ui.button(t("tools.menu.delete", "Delete")).clicked() {
        asked = Some(MacroAsk::Delete);
    }
    asked
}

/// Testing aid: SOFTPRESENTER_MACRO_STEPS opens the first macro's steps (`open`), and with them
/// the menu of its step N (`menu=N`), or presses on its step N and holds it over the lower half
/// of step M (`drag=N>M`, see `steps_aid_events`), for pictures of each. Steps count from 1.
#[derive(Clone, Copy)]
enum StepsAid {
    Open,
    Menu(usize),
    Drag(usize, usize),
}

fn steps_aid() -> Option<StepsAid> {
    static ASKED: std::sync::OnceLock<Option<StepsAid>> = std::sync::OnceLock::new();
    *ASKED.get_or_init(|| {
        let asked = std::env::var("SOFTPRESENTER_MACRO_STEPS").ok()?;
        match asked.split_once('=') {
            None if asked == "open" => Some(StepsAid::Open),
            Some(("menu", step)) => Some(StepsAid::Menu(step.parse().ok()?)),
            Some(("drag", steps)) => {
                let (from, to) = steps.split_once('>')?;
                Some(StepsAid::Drag(from.parse().ok()?, to.parse().ok()?))
            }
            _ => {
                eprintln!("SOFTPRESENTER_MACRO_STEPS: say open, menu=N or drag=N>M");
                None
            }
        }
    })
}

/// Where the first macro's steps were drawn last, for `steps_aid_events`.
fn steps_aid_rows() -> egui::Id {
    egui::Id::unique("macro steps aid rows")
}

/// Testing aid: the pointer's events for this frame of SOFTPRESENTER_MACRO_STEPS=drag=N>M, once
/// the steps have been drawn for a moment: a press on step N carried to the lower half of step
/// M and held there. The window's own pointer is moved, not the mouse. It goes a step a frame,
/// so it is for a capture run (`--capture-home`).
pub fn steps_aid_events(ctx: &egui::Context) -> Vec<egui::Event> {
    /// Frames the steps are left to settle after they are first drawn.
    const SETTLE: u32 = 10;
    static STEP: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let Some(StepsAid::Drag(from, to)) = steps_aid() else { return Vec::new() };
    let rows: Option<Vec<Rect>> = ctx.data(|data| data.get_temp(steps_aid_rows()));
    let place = |rows: &[Rect], step: usize| rows.get(step.checked_sub(1)?).copied();
    let Some((from, to)) = rows.and_then(|rows| Some((place(&rows, from)?, place(&rows, to)?))) else {
        ctx.request_repaint();
        return Vec::new();
    };
    let step = STEP.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    ctx.request_repaint();
    if step < SETTLE {
        return Vec::new();
    }
    let to = to.center() + vec2(0.0, to.height() / 4.0);
    crate::home::pointer_drag(step - SETTLE, from.center(), to, false).unwrap_or_default()
}

/// The room inside a macro's card, round its first line.
const MACRO_MARGIN: egui::Margin = egui::Margin { left: 12, right: 4, top: 6, bottom: 6 };

fn macros_tab(ui: &mut egui::Ui, show: &sp_engine::Show, state: &mut HomeState) {
    use crate::macros::Macro;
    // The macro whose steps are open under it, kept by the window.
    let open_id = egui::Id::unique("macro steps open");
    let mut open_macro = ui.data(|data| data.get_temp::<sp_model::Uuid>(open_id));
    // Testing aid: SOFTPRESENTER_MACRO_STEPS opens the first macro's steps, once.
    static AID_OPENED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if steps_aid().is_some() && !state.macros.is_empty() && !AID_OPENED.swap(true, std::sync::atomic::Ordering::Relaxed)
    {
        open_macro = state.macros.first().map(|made| made.id);
    }
    let (_, add) = heading(ui, t("right.tab.macros", "Macros"), Some(t("tools.macro.new", "New macro")));
    if add {
        let taken = |name: &str| state.macros.iter().any(|m| m.name == name);
        let base = t("tools.macro.default_name", "Macro");
        let name = (1..).map(|n| format!("{base} {n}")).find(|name| !taken(name)).unwrap_or_default();
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
            t(
                "tools.macro.none",
                "No macros. A macro does several things at once, such as clearing the screen and starting a timer.",
            ),
        );
        return;
    }
    let mut asked: Option<(usize, MacroAsk)> = None;
    let mut renamed: Option<Option<(sp_model::Uuid, String)>> = None;
    let mut rename_started = std::mem::take(&mut state.rename_started);
    let run_font = semibold(12.5);
    let run_word = t("tools.macro.run", "Run");
    let run_wide =
        12.0 + 10.0 + 6.0 + ui.painter().layout_no_wrap(run_word.to_owned(), run_font.clone(), TEXT).size().x + 12.0;
    for index in 0..state.macros.len() {
        let made = &state.macros[index];
        let id = made.id;
        let open = open_macro == Some(id);
        let steps: Vec<(String, bool)> = made.actions.iter().map(|action| step_words(show, state, action)).collect();
        egui::Frame::new().fill(CARD).corner_radius(RADIUS_CARD as u8).inner_margin(MACRO_MARGIN).show(ui, |ui| {
            ui.set_width(ui.available_width());
            let made = &state.macros[index];
            let width = ui.available_width();
            let (row, _) = ui.allocate_exact_size(vec2(width, 34.7), Sense::hover());
            if numbered(ui, state, row, index) {
                asked = Some((index, MacroAsk::Run));
            }
            let reach = Rect::from_min_max(
                row.min - vec2(f32::from(MACRO_MARGIN.left), f32::from(MACRO_MARGIN.top)),
                row.max + vec2(f32::from(MACRO_MARGIN.right), f32::from(MACRO_MARGIN.bottom)),
            );
            let back = ui.interact(reach, egui::Id::unique(("macro card", id)), Sense::click());
            let y = row.center().y;
            // From the right: ⋯, the chevron that opens the steps, and Run.
            let more = Rect::from_min_size(pos2(row.max.x - 28.0, y - 14.0), vec2(28.0, 28.0));
            let chevron = Rect::from_min_size(pos2(more.min.x - 2.0 - 28.0, y - 14.0), vec2(28.0, 28.0));
            let run = Rect::from_min_size(pos2(chevron.min.x - 6.0 - run_wide, y - 15.0), vec2(run_wide, 30.0));
            let text_wide = run.min.x - 8.0 - row.min.x;
            // The name with the macro's colour as a dot before it, and under it how many steps.
            let colour = crate::home::color32(made.color);
            let name_left = row.min.x + 9.0 + 7.0;
            let name = fitted(ui, &made.name, name_font(), TEXT, text_wide - 16.0, 1);
            let count = made.actions.len();
            let steps_said = tn("tools.macro.steps", count as u64, "{n} step", "{n} steps", &[]);
            let said = if made.on_startup {
                tf("tools.macro.steps.at_start", "{steps} \u{b7} when SoftPresenter opens", &[("steps", &steps_said)])
            } else {
                steps_said
            };
            let sub = fitted(ui, &said, FontId::proportional(12.0), TEXT_FAINT, text_wide, 1);
            let name_y = row.min.y + 17.5 / 2.0;
            ui.painter().circle_filled(pos2(row.min.x + 4.5, name_y), 4.5, colour);
            match state.macro_rename.as_mut().filter(|(renaming, _)| *renaming == id) {
                Some((_, typed)) => {
                    let start = pos2(name_left, name_y);
                    let look = crate::studio::Lettering::plain(name_font(), TEXT);
                    let field = crate::studio::rename_field_at(ui, typed, &look, start, row.min.x + text_wide);
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
            let word = ui.painter().layout_no_wrap(run_word.to_owned(), run_font.clone(), TEXT);
            let icon = Rect::from_center_size(pos2(run.min.x + 12.0 + 5.0, y), vec2(10.0, 10.0));
            crate::icons::paint(ui.painter(), Icon::Play, icon, TEXT);
            ui.painter().galley(pos2(icon.max.x + 6.0, y - word.size().y / 2.0), word, TEXT);
            if run_response.on_hover_text(t("tools.macro.run.hint", "Run this macro now")).clicked() {
                asked = Some((index, MacroAsk::Run));
            }
            let glyph = if open { Icon::ChevronUp } else { Icon::ChevronDown };
            let hint = if open { t("tools.macro.hide_steps", "Hide its steps") } else { t("tools.macro.show_steps", "Its steps") };
            if quiet_icon(ui, chevron, egui::Id::unique(("macro steps", id)), glyph, 13.0, 0.78, None)
                .on_hover_text(hint)
                .clicked()
            {
                asked = Some((index, MacroAsk::Steps));
            }
            let more = quiet_icon(ui, more, egui::Id::unique(("macro more", id)), Icon::More, 16.0, 0.6, None)
                .on_hover_text(t("tools.more", "More"));
            egui::Popup::menu(&more).show(|ui| {
                if let Some(ask) = macro_menu(ui, made, open) {
                    asked = Some((index, ask));
                }
            });
            let summary = if steps.is_empty() {
                t("tools.macro.no_steps", "No steps yet").to_owned()
            } else {
                steps.iter().map(|(words, _)| words.as_str()).collect::<Vec<_>>().join("\n")
            };
            back.on_hover_text(summary).context_menu(|ui| {
                if let Some(ask) = macro_menu(ui, made, open) {
                    asked = Some((index, ask));
                }
            });
            if !open {
                return;
            }
            // Whether it runs as SoftPresenter opens, in plain view (also in its menu).
            ui.add_space(6.0);
            let width_here = ui.available_width();
            ui.allocate_ui_with_layout(
                vec2(width_here, 22.0),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    let hint = t("tools.macro.run_at_start.hint", "Run this macro's steps as SoftPresenter opens");
                    if crate::home::switch(ui, made.on_startup, hint) {
                        asked = Some((index, MacroAsk::Startup));
                    }
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(t("tools.macro.run_at_start", "Run when SoftPresenter opens"))
                                .font(FontId::proportional(12.5))
                                .color(TEXT),
                        );
                    });
                },
            );
            ui.label(
                egui::RichText::new(t(
                    "tools.macro.audience_off_note",
                    "Audience Screens Off here takes effect before any screen draws, so the audience screens never flash on.",
                ))
                .font(FontId::proportional(11.5))
                .color(TEXT_FAINT),
            );
            // Its steps, in the order they are done. Each is a button (its words and a faint chevron)
            // that changes what it does, in place; its row is dragged to another place; × removes
            // it. Words naming something gone are amber, and wrap to a second line before they
            // are cut short.
            ui.add_space(6.0);
            ui.spacing_mut().item_spacing.y = 2.0;
            let inner = width - 4.0;
            if steps.is_empty() {
                empty_note(ui, t("tools.macro.no_steps", "No steps yet"));
            }
            let mut rows: Vec<Rect> = Vec::with_capacity(steps.len());
            let (mut dragging, mut dropped) = (None, None);
            let font = FontId::proportional(12.5);
            // From the left: the number, the button (6 points of room round its words and the
            // chevron), and ×.
            let (number_wide, remove_wide, pad, mark) = (16.0, 26.0, 6.0, 9.0);
            let button_wide = inner - number_wide - remove_wide - 6.0;
            for (at, (words, gone)) in steps.iter().enumerate() {
                let ink = if *gone { crate::studio::CARE } else { TEXT };
                let text = fitted(ui, words, font.clone(), ink, button_wide - 3.0 * pad - mark, 2);
                let (line, _) = ui.allocate_exact_size(vec2(inner, (text.size().y + 6.0).max(26.0)), Sense::hover());
                rows.push(line);
                let cy = line.center().y;
                let button =
                    Rect::from_min_size(pos2(line.min.x + number_wide, line.min.y), vec2(button_wide, line.height()));
                // The whole row takes the press, ahead of ×, which is laid over it.
                let response = ui.interact(line, egui::Id::unique(("macro step", id, at)), Sense::click_and_drag());
                // Testing aid: SOFTPRESENTER_MACRO_STEPS=menu=N opens step N's menu, once.
                static AID_MENU: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
                if index == 0
                    && matches!(steps_aid(), Some(StepsAid::Menu(step)) if step == at + 1)
                    && !AID_MENU.swap(true, std::sync::atomic::Ordering::Relaxed)
                {
                    egui::Popup::open_id(ui.ctx(), egui::Popup::default_response_id(&response));
                }
                let menu_open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&response));
                if response.hovered() || menu_open {
                    ui.painter().rect_filled(button, 6.0, crate::studio::BUTTON);
                }
                let number = format!("{}", at + 1);
                ui.painter().text(pos2(line.min.x, cy), Align2::LEFT_CENTER, number, fixed(12.0), TEXT_FAINT);
                ui.painter().galley(pos2(button.min.x + pad, cy - text.size().y / 2.0), text, ink);
                let chevron = Rect::from_center_size(pos2(button.max.x - pad - mark / 2.0, cy), vec2(mark, mark));
                crate::icons::paint(ui.painter(), Icon::ChevronDown, chevron, TEXT_FAINT);
                if response.dragged() {
                    dragging = Some(at);
                }
                if crate::studio::drag_let_go(&response) {
                    dropped = Some(at);
                }
                let hint = if *gone {
                    t("tools.macro.step_gone.hint", "What this step names has gone: click to choose another")
                } else {
                    t("tools.macro.step.hint", "Click to change this step; drag it to another place")
                };
                let response = response.on_hover_text(hint);
                // A click (or a right-click) opens every step in groups, its own marked; the one
                // chosen takes its place.
                let change = |ui: &mut egui::Ui, asked: &mut Option<(usize, MacroAsk)>| {
                    let current = &made.actions[at];
                    let heading = t("tools.macro.change_step", "Change this step to");
                    if let Some(step) = step_menu(ui, show, state, heading, Some(current)) {
                        if step != *current {
                            *asked = Some((index, MacroAsk::Change(at, step)));
                        }
                        ui.close();
                    }
                };
                let opened = response.clicked() || response.secondary_clicked();
                egui::Popup::menu(&response)
                    .open_memory(opened.then_some(egui::SetOpenCommand::Toggle))
                    .show(|ui| change(ui, &mut asked));
                let remove = Rect::from_min_size(pos2(line.max.x - remove_wide, cy - 13.0), vec2(remove_wide, 26.0));
                if quiet_icon(
                    ui,
                    remove,
                    egui::Id::unique(("macro step remove", id, at)),
                    Icon::Close,
                    11.0,
                    0.6,
                    Some(crate::studio::BUTTON),
                )
                .on_hover_text(t("tools.macro.remove_step", "Remove this step"))
                .clicked()
                {
                    asked = Some((index, MacroAsk::Remove(at)));
                }
            }
            if index == 0 && steps_aid().is_some() {
                ui.data_mut(|data| data.insert_temp(steps_aid_rows(), rows.clone()));
            }
            // A step being dragged goes faint, its words follow the pointer, and an ink line shows
            // where it will go, by which half of a step the pointer is over; letting go moves it
            // there, and Escape leaves it where it was (as the playlist's items do).
            if let Some(y) = ui.input(|input| input.pointer.interact_pos()).map(|at| at.y) {
                let slot = crate::studio::drop_slot(&rows, y);
                if let Some(from) = dragging {
                    crate::studio::paint_rows_moving(ui.painter(), &rows, |at| at == from, slot, CARD);
                    crate::studio::paint_carried(ui.ctx(), &steps[from].0);
                }
                if let Some(from) = dropped {
                    let to = if slot > from { slot - 1 } else { slot };
                    if to != from {
                        asked = Some((index, MacroAsk::Move(from, to)));
                    }
                }
            }
            // + Add a step: a small button opening every step, in their groups.
            ui.add_space(6.0);
            let small = FontId::proportional(11.5);
            let words = t("tools.macro.add_step", "+ Add a step");
            let (place, _) = ui.allocate_exact_size(vec2(inner, 24.0), Sense::hover());
            let button = Rect::from_min_size(place.min, vec2(flat_button_width(ui, &[words], &small, 9.0), 24.0));
            let adding =
                flat_button(ui, button, egui::Id::unique(("macro add step", id)), words, small, Flat::Plain, true)
                    .on_hover_text(t("tools.macro.add_step.hint", "Add a step at the end"));
            egui::Popup::menu(&adding).show(|ui| {
                if let Some(action) = step_menu(ui, show, state, t("tools.macro.add_step.heading", "Add a step"), None) {
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
            MacroAsk::Change(at, step) => {
                state.macros[index].replace_step(at, step);
            }
            MacroAsk::Move(from, to) => {
                state.macros[index].move_step(from, to);
            }
            MacroAsk::Remove(at) => {
                state.macros[index].actions.remove(at);
            }
            MacroAsk::Duplicate => {
                let copy = state.macros[index].duplicate();
                state.macros.insert(index + 1, copy);
            }
            // At once: Undo puts it back.
            MacroAsk::Delete => {
                state.delete_macro(index);
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
    if ui.button(t("tools.audio.play", "Play")).clicked() {
        asked = Some(TrackAsk::Play);
    }
    ui.separator();
    let up = ui.add_enabled(!first, egui::Button::new(t("tools.audio.move_up", "Move Up")));
    if up.on_disabled_hover_text(crate::studio::at_top()).clicked() {
        asked = Some(TrackAsk::Up);
    }
    let down = ui.add_enabled(!last, egui::Button::new(t("tools.audio.move_down", "Move Down")));
    if down.on_disabled_hover_text(crate::studio::at_bottom()).clicked() {
        asked = Some(TrackAsk::Down);
    }
    if ui.button(t("tools.menu.rename", "Rename")).clicked() {
        asked = Some(TrackAsk::Rename);
    }
    ui.separator();
    if ui.button(t("tools.audio.remove", "Remove")).clicked() {
        asked = Some(TrackAsk::Remove);
    }
    asked
}

/// The Audio tab's heading: the playlist shown, chosen from a drop-down, its ⋯ (Rename, New
/// Audio Playlist…, Delete) and + for a new audio playlist. A new one is shown with its name
/// ready to type where the drop-down was.
fn audio_heading(ui: &mut egui::Ui, state: &mut HomeState) {
    let (row, add) =
        heading(ui, t("right.tab.audio", "Audio"), Some(t("tools.audio.new_playlist", "New Audio Playlist\u{2026}")));
    let more = Rect::from_min_size(pos2(row.max.x - 28.0 - 8.0 - 28.0, row.min.y), vec2(28.0, 28.0));
    let select = Rect::from_min_size(pos2(more.min.x - 8.0 - 140.0, row.center().y - 12.0), vec2(140.0, 24.0));
    let renaming_id = egui::Id::unique("audio playlist renaming");
    let new_playlist = |ui: &egui::Ui, state: &mut HomeState| {
        let made = crate::audio::Playlist::new(state.audio.new_name());
        ui.data_mut(|data| data.insert_temp(renaming_id, made.name.clone()));
        state.audio.playlists.push(made);
        state.audio.current = state.audio.playlists.len() - 1;
        state.audio_changed = true;
    };
    if add {
        new_playlist(ui, state);
    }
    let renaming = ui.data(|data| data.get_temp::<String>(renaming_id));
    let name = state.audio.shown().name.clone();
    match renaming {
        Some(mut typed) => {
            // The menu stays drawn round the name, which is typed where it was shown.
            small_select(ui, select, egui::Id::unique("audio playlist picker"), "");
            let start = pos2(select.min.x + 8.0, select.center().y);
            let look = crate::studio::Lettering::plain(FontId::proportional(11.5), TEXT);
            let field = crate::studio::rename_field_at(ui, &mut typed, &look, start, select.max.x - 22.0);
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
                .on_hover_text(t("tools.audio.choose_playlist", "Choose an audio playlist"));
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
        .on_hover_text(t("tools.more", "More"));
    let many = state.audio.playlists.len() > 1;
    egui::Popup::menu(&more).show(|ui| {
        if ui.button(t("tools.menu.rename", "Rename")).clicked() {
            ui.data_mut(|data| data.insert_temp(renaming_id, name.clone()));
        }
        if ui.button(t("tools.audio.new_playlist", "New Audio Playlist\u{2026}")).clicked() {
            new_playlist(ui, state);
        }
        ui.separator();
        // At once, asking nothing: Undo puts it back (the files stay on this computer).
        let delete = ui.add_enabled(many, egui::Button::new(t("tools.menu.delete", "Delete")));
        if delete.on_disabled_hover_text(t("tools.audio.one_playlist", "There is always one audio playlist")).clicked()
        {
            state.delete_audio_playlist(state.audio.current.min(state.audio.playlists.len() - 1));
        }
    });
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
        .filter(|id| shown_tracks.contains(id))
        .or(shown_tracks.first().copied());
    let current = playing.track.or(ready);
    let mut ready_now = current;
    // The playlist the current track is in, and its place there, for Previous and Next (for
    // one taken out while it plays, the place it had).
    let found = current.and_then(|id| state.audio.place_of(id));
    let list: Vec<sp_model::Uuid> =
        found.map_or(shown_tracks.clone(), |(playlist, _)| playlist.tracks.iter().map(|track| track.id).collect());
    let place = found.map(|(_, at)| at);
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
                (Some(_), false, _) => (t("tools.audio.playing", "PLAYING"), READY),
                (Some(_), true, _) => (t("tools.audio.paused", "PAUSED"), TEXT_FAINT),
                (None, _, Some(_)) => (t("tools.audio.ready", "READY"), TEXT_FAINT),
                (None, _, None) => (t("tools.audio.nothing", "NOTHING TO PLAY"), TEXT_FAINT),
            };
            paint_spaced(ui, now.min, Align2::LEFT_TOP, said, fixed_semibold(11.5), said_ink, 0.6);
            let (title, ink) = match current.and_then(|id| state.audio.track(id)) {
                Some(track) => (track.name.as_str(), TEXT),
                None => (t("tools.audio.add_a_track", "Add a track to this playlist"), TEXT_DIM),
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
            response.on_hover_text(t("tools.audio.seek.hint", "Where in the track: drag to move through it"));
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
                .on_hover_text(t("tools.audio.previous", "Previous track (the start of this one first)"))
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
                .on_hover_text(t("tools.audio.back", "Back 15 seconds"))
                .clicked()
            {
                request = Some(AudioRequest::Skip(-15.0));
            }
            ui.painter().text(back.center(), Align2::CENTER_CENTER, "\u{2212}15", font.clone(), ink(loaded));
            let can_play = loaded || current.is_some();
            let pausing = loaded && !playing.paused;
            let hint = if pausing { t("tools.audio.pause", "Pause") } else { t("tools.audio.play", "Play") };
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
                .on_hover_text(t("tools.audio.forward", "Forward 15 seconds"))
                .clicked()
            {
                request = Some(AudioRequest::Skip(15.0));
            }
            ui.painter().text(forward.center(), Align2::CENTER_CENTER, "+15", font, ink(loaded));
            let can_next = list.len() > 1;
            if player_key(ui, next, egui::Id::unique("audio next"), false, can_next)
                .on_hover_text(t("tools.audio.next", "Next track"))
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
                Some(_) => (
                    Icon::Muted,
                    crate::studio::CARE,
                    t("tools.audio.muted.hint", "Muted. Click to bring the sound back"),
                ),
                None => (Icon::Sound, TEXT_DIM, t("tools.audio.mute", "Mute")),
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
            response.on_hover_text(t("tools.audio.volume", "Volume"));
            // Right of the level's line, clear of its handle at the loudest: a short form where the
            // usual word is longer.
            let words = match muted_from {
                Some(_) => fitting(ui, tk("tools.audio.muted", "Muted"), &[], fixed(12.0), 0.0, 44.0, false),
                None => format!("{:.0}%", state.audio_volume * 100.0),
            };
            ui.painter().text(level_row.right_center(), Align2::RIGHT_CENTER, words, fixed(12.0), TEXT_DIM);

            // How tracks follow one another, under a line.
            let (options, _) = ui.allocate_exact_size(vec2(width, 35.0), Sense::hover());
            ui.painter().hline(options.x_range(), options.min.y, egui::Stroke::new(1.0, crate::studio::LINE));
            let y = options.min.y + 10.0 + 12.0;
            let small = FontId::proportional(11.5);
            let through_words = t("tools.audio.through", "Play through");
            let through_wide = flat_button_width(ui, &[through_words], &small, 9.0);
            let through = Rect::from_min_size(pos2(options.min.x, y - 12.0), vec2(through_wide, 24.0));
            let look = if state.audio.through { Flat::On } else { Flat::Plain };
            if flat_button(ui, through, egui::Id::unique("audio through"), through_words, small.clone(), look, true)
                .on_hover_text(t("tools.audio.through.hint", "When a track ends, go on to the next"))
                .clicked()
            {
                state.audio.through = !state.audio.through;
                state.audio_changed = true;
            }
            let shuffle_words = t("tools.audio.shuffle", "Shuffle");
            let shuffle_wide = flat_button_width(ui, &[shuffle_words], &small, 9.0);
            let shuffle = Rect::from_min_size(pos2(through.max.x + 6.0, y - 12.0), vec2(shuffle_wide, 24.0));
            let look = if state.audio.shuffle { Flat::On } else { Flat::Plain };
            if flat_button(ui, shuffle, egui::Id::unique("audio shuffle"), shuffle_words, small, look, true)
                .on_hover_text(t("tools.audio.shuffle.hint", "Play the tracks in a random order"))
                .clicked()
            {
                state.audio.shuffle = !state.audio.shuffle;
                state.audio_changed = true;
            }
            let fade = state.audio.crossfade;
            let fade_words = |seconds: f32| {
                if seconds <= 0.0 {
                    t("tools.audio.crossfade.off", "Off").to_owned()
                } else {
                    crate::i18n::seconds(f64::from(seconds))
                }
            };
            let select = Rect::from_min_size(pos2(options.max.x - 70.0, y - 12.0), vec2(70.0, 24.0));
            paint_spaced(
                ui,
                pos2(select.min.x - 8.0, y),
                Align2::RIGHT_CENTER,
                t("tools.audio.crossfade", "CROSSFADE"),
                fixed_semibold(11.5),
                TEXT_FAINT,
                0.6,
            );
            let picker = small_select(ui, select, egui::Id::unique("audio crossfade"), &fade_words(fade))
                .on_hover_text(t("tools.audio.crossfade.hint", "How long one track fades into the next"));
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
    let says = tn("tools.audio.tracks", count as u64, "{n} track", "{n} tracks", &[]);
    paint_spaced(ui, row.left_center(), Align2::LEFT_CENTER, &says.to_uppercase(), fixed_semibold(11.5), TEXT_DIM, 0.6);
    let small = FontId::proportional(11.5);
    let add_words = t("tools.audio.add_tracks", "+ Add tracks…");
    let add_wide = flat_button_width(ui, &[add_words], &small, 9.0);
    let add = Rect::from_min_size(pos2(row.max.x - add_wide, row.min.y), vec2(add_wide, 24.0));
    if flat_button(ui, add, egui::Id::unique("audio add tracks"), add_words, small, Flat::Plain, true)
        .on_hover_text(t("tools.audio.add_tracks.hint", "Add music files from this computer to this playlist"))
        .clicked()
    {
        request = Some(AudioRequest::Add);
    }
    if count == 0 {
        empty_note(ui, t("tools.audio.no_tracks", "No tracks yet. Add music files from this computer."));
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
                let font = if chosen { semibold(13.0) } else { FontId::proportional(13.0) };
                let look = crate::studio::Lettering::plain(font, TEXT);
                let field = crate::studio::rename_field_at(ui, typed, &look, pos2(name_left, y), name_right);
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
            .on_hover_text(t("tools.more", "More"));
        egui::Popup::menu(&more).show(|ui| {
            if let Some(ask) = track_menu(ui, first, last) {
                asked = Some((at, ask));
            }
        });
        if response.clicked() || numbered_at(ui, &state.numbers, line, at) {
            asked = Some((at, TrackAsk::Play));
        }
        response
            .on_hover_text(t("tools.audio.track.hint", "Click to play this track; drag it to another place"))
            .context_menu(|ui| {
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

/// What the stage would show now, to draw a picture of each stage layout with: the slide on
/// screen and the next, the song's title and the group, the stage message and a timer that
/// runs. The Stage tab and the Edit view's list of layouts draw them the same way, and share
/// them.
#[derive(Default)]
pub struct StagePictures<'a> {
    now: Option<&'a sp_model::Slide>,
    next: Option<&'a sp_model::Slide>,
    about: crate::stage::About,
    overlay: crate::output::Overlay,
}

impl<'a> StagePictures<'a> {
    pub fn of(show: &'a sp_engine::Show, state: &HomeState) -> StagePictures<'a> {
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
        StagePictures { now, next, about, overlay }
    }

    /// The stage's picture in `layout`, in `rect` with corners of `corner`, drawn again when
    /// what it shows changes: a new slide, the stage message, the clock's minute, the layout
    /// itself. A running timer is named but its time is left out, so the pictures are not drawn
    /// again every second.
    pub fn paint(&self, ui: &egui::Ui, thumb: &mut Thumb, rect: Rect, corner: f32, layout: &sp_model::ThemeTemplate) {
        let mut stage = crate::stage::slide(self.now, self.next, &self.overlay, &self.about, layout);
        let timer_name = self.overlay.timers.first().map(|timer| timer.name.clone());
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
        own_picture(ui, thumb, rect, corner, key, &stage, &crate::stage::theme_for(layout), said);
    }
}

fn stage_tab(ui: &mut egui::Ui, show: &sp_engine::Show, state: &mut HomeState, thumb: &mut Thumb) {
    let stages: Vec<usize> =
        state.screens.iter().enumerate().filter(|(_, screen)| screen.kind == Kind::Stage).map(|(at, _)| at).collect();
    if stages.is_empty() {
        heading(ui, t("right.tab.stage", "Stage"), None);
        empty_note(ui, t("tools.stage.none", "No stage screens. Add one in Settings, under Screens."));
        let (place, _) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::hover());
        let font = FontId::proportional(11.5);
        let set_up = t("tools.stage.set_up_screens", "Set Up Screens…");
        let wide = flat_button_width(ui, &[set_up], &font, 9.0);
        let rect = Rect::from_min_size(place.min, vec2(wide, 24.0));
        if flat_button(ui, rect, egui::Id::unique("stage set up screens"), set_up, font, Flat::Plain, true).clicked() {
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
        // The heading and the picker share a row: short forms where the usual words don't fit.
        let room = ui.available_width() - 150.0 - 8.0;
        let which = tk("tools.stage.which_screen", "Which stage screen");
        let (row, _) = heading(ui, &fitting(ui, which, &[], fixed_semibold(11.5), 0.6, room, true), None);
        let all = tk("tools.stage.all_screens", "All stage screens");
        let all_words = fitting(ui, all, &[], FontId::proportional(11.5), 0.0, 150.0 - 30.0, false);
        let words = chosen
            .and_then(|id| state.screens.iter().find(|screen| screen.id == id))
            .map_or(all_words.as_str(), |screen| screen.name.as_str());
        let rect = Rect::from_min_size(pos2(row.max.x - 150.0, row.center().y - 12.0), vec2(150.0, 24.0));
        let picker = small_select(ui, rect, egui::Id::unique("stage screen picker"), words);
        egui::Popup::menu(&picker).show(|ui| {
            if ui.add(egui::Button::selectable(chosen.is_none(), t(all.0, all.1))).clicked() {
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
        let words = if state.stage_on {
            t(
                "tools.stage.screen_off",
                "This stage screen is switched off. The layout still changes, ready for when it is on.",
            )
        } else {
            t(
                "tools.stage.screens_off",
                "The stage screens are off. The layout still changes, ready for when it is on.",
            )
        };
        egui::Frame::new()
            .corner_radius(RADIUS_CARD as u8)
            .stroke(egui::Stroke::new(1.0, crate::studio::LINE))
            .inner_margin(egui::Margin::symmetric(11, 8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(egui::RichText::new(words).size(12.5).color(crate::studio::CARE));
            });
    }
    let title = if stages.len() > 1 {
        t("tools.stage.layout", "Layout").to_owned()
    } else {
        tf("tools.stage.layout_of", "{screen} \u{b7} Layout", &[("screen", &state.screens[stages[0]].name)])
    };
    let (row, _) = heading(ui, &title, None);
    let font = FontId::proportional(11.5);
    let edit_layouts = t("tools.stage.edit_layouts", "Edit layouts…");
    let wide = flat_button_width(ui, &[edit_layouts], &font, 9.0);
    let rect = Rect::from_min_size(pos2(row.max.x - wide, row.center().y - 12.0), vec2(wide, 24.0));
    if flat_button(ui, rect, egui::Id::unique("stage edit layouts"), edit_layouts, font, Flat::Plain, true)
        .on_hover_text(t("tools.stage.edit_layouts.hint", "Lay out what the stage screens show"))
        .clicked()
    {
        state.edit_kind_request = Some(crate::home::EditKind::Stage);
    }

    // Every layout as a tile with a picture of what the stage would show in it now; the one in
    // use is outlined, with a tick. One click switches.
    if state.stage_layouts.is_empty() {
        state.stage_layouts = crate::stage::builtin_layouts();
    }
    let pictures = StagePictures::of(show, state);
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
        let shot = Rect::from_min_size(tile.min + vec2(6.0, 6.0), picture);
        pictures.paint(ui, thumb, shot, 5.0, layout);
        let name_y = shot.max.y + 6.0 + 8.5;
        let mut left = tile.min.x + 9.0;
        if on {
            paint_tick(ui.painter(), Rect::from_center_size(pos2(left + 6.0, name_y), vec2(12.0, 12.0)), READY);
            left += 17.0;
        }
        let (font, ink) = if on { (semibold(12.5), TEXT) } else { (FontId::proportional(12.5), TEXT_DIM) };
        let name = fitted(ui, &layout.name, font, ink, tile.max.x - 9.0 - left, 1);
        ui.painter().galley(pos2(left, name_y - name.size().y / 2.0), name, ink);
        let picked = numbered_at(ui, &state.numbers, tile, place);
        let hint = tf("tools.stage.switch_to", "Switch the stage to {name}", &[("name", &layout.name)]);
        if response.on_hover_text(hint).clicked() || picked {
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
    let title = if live {
        t("tools.stage.message.showing", "Stage message \u{b7} showing")
    } else {
        t("tools.stage.message", "Stage message")
    };
    heading(ui, title, None);
    let draft_id = egui::Id::unique("stage message draft");
    let mut words = ui.data(|data| data.get_temp::<String>(draft_id)).unwrap_or_else(|| state.stage_message.clone());
    let field = ui.add(
        egui::TextEdit::singleline(&mut words)
            .desired_width(f32::INFINITY)
            .min_size(vec2(0.0, 32.0))
            .margin(egui::Margin::symmetric(10, 8))
            .background_color(CARD)
            .hint_text(t("tools.stage.message.field", "A note for the people on stage")),
    );
    if field.changed() {
        ui.data_mut(|data| data.insert_temp(draft_id, words.clone()));
    }
    let changed = words != state.stage_message;
    let can_send = !words.trim().is_empty() && (!live || changed);
    let entered = field.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
    let (place, _) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::hover());
    let font = FontId::proportional(12.5);
    let (show_on_stage, change_it) =
        (t("tools.stage.message.show", "Show on stage"), t("tools.stage.message.change", "Change it"));
    let send_wide = flat_button_width(ui, &[show_on_stage, change_it], &font, 13.0);
    let send = Rect::from_min_size(place.min, vec2(send_wide, 30.0));
    let (send_words, send_look) = if live { (change_it, Flat::Plain) } else { (show_on_stage, Flat::Main) };
    let send_response =
        flat_button(ui, send, egui::Id::unique("stage message show"), send_words, font.clone(), send_look, can_send)
            .on_hover_text(t(
                "tools.stage.message.show.hint",
                "Show these words at the top of the stage screens (Return)",
            ));
    if (send_response.clicked() || entered) && can_send {
        state.stage_message = words.trim().to_owned();
        state.stage_message_live = true;
        ui.data_mut(|data| data.remove::<String>(draft_id));
    }
    let clear_words = t("tools.stage.message.clear", "Clear");
    let clear_wide = flat_button_width(ui, &[clear_words], &font, 13.0);
    let clear = Rect::from_min_size(pos2(send.max.x + 6.0, place.min.y), vec2(clear_wide, 30.0));
    if flat_button(ui, clear, egui::Id::unique("stage message clear"), clear_words, font, Flat::Plain, live)
        .on_hover_text(t("tools.stage.message.clear.hint", "Take the message off the stage screens"))
        .clicked()
    {
        state.stage_message_live = false;
    }
}

/// The VoxLive tab: the church's VoxLive screens, a card each. Show puts that screen's captions
/// on the screens in place of what is there (a verse, a song); a click on a verse brings the
/// verse back. On the Bible page too, where this column stays.
fn voxlive_tab(ui: &mut egui::Ui, show: &sp_engine::Show, state: &mut HomeState) {
    let Some(slot) = state.voxlive_slot else { return };
    let Some(item) = show.presentations().get(slot) else { return };
    heading(ui, t("tools.voxlive.screens", "VoxLive screens"), None);
    let live = show.live().filter(|at| at.presentation == slot).map(|at| at.slide);
    let button_font = FontId::proportional(12.5);
    let (show_words, on_screen) = (t("tools.show", "Show"), t("tools.voxlive.on_screen", "On screen"));
    let button_wide = flat_button_width(ui, &[show_words, on_screen], &button_font, 13.0);
    for (index, (group, _)) in item.slides().enumerate() {
        let on = live == Some(index);
        card(ui, if on { CARD_SHOWN } else { CARD }, |ui| {
            let width = ui.available_width();
            let text_wide = width - button_wide - CARD_GAP;
            let name = fitted(ui, &group.name, name_font(), TEXT, text_wide, 2);
            // Under the name, the screen's language and what it shows, as VoxLive says them.
            let about = state.voxlive.screens.iter().find(|screen| screen.name == group.name).map(|screen| {
                let shows = match screen.mode.as_str() {
                    "captions" => t("tools.voxlive.captions", "Captions"),
                    "reference" => t("tools.voxlive.reference", "Bible passage"),
                    other => other,
                };
                [screen.language.to_uppercase(), shows.to_owned()]
                    .into_iter()
                    .filter(|part| !part.is_empty())
                    .collect::<Vec<_>>()
                    .join(" \u{b7} ")
            });
            let sub = about
                .filter(|about| !about.is_empty())
                .map(|about| fitted(ui, &about, FontId::proportional(12.0), TEXT_FAINT, text_wide, 1));
            let block = name.size().y + sub.as_ref().map_or(0.0, |sub| sub.size().y);
            let (row, _) = ui.allocate_exact_size(vec2(width, block.max(30.0)), Sense::hover());
            let y = row.center().y;
            let top = y - block / 2.0;
            let name_tall = name.size().y;
            ui.painter().galley(pos2(row.min.x, top), name, TEXT);
            if let Some(sub) = sub {
                ui.painter().galley(pos2(row.min.x, top + name_tall), sub, TEXT_FAINT);
            }
            let button = Rect::from_min_size(pos2(row.max.x - button_wide, y - 15.0), vec2(button_wide, 30.0));
            let (words, look, hint) = if on {
                (
                    on_screen,
                    Flat::On,
                    t("tools.voxlive.on_screen.hint", "On the screens now. A verse or a slide clicked brings it back"),
                )
            } else {
                (
                    show_words,
                    Flat::Plain,
                    t("tools.voxlive.show.hint", "Put these captions on the screens, in place of what is there"),
                )
            };
            let id = egui::Id::unique(("voxlive show", index));
            let picked = numbered(ui, state, row, index);
            if flat_button(ui, button, id, words, button_font.clone(), look, true).on_hover_text(hint).clicked()
                || picked
            {
                state.voxlive_go = Some(index);
            }
        });
    }
    ui.label(
        egui::RichText::new(t(
            "tools.voxlive.note",
            "LIVE CAPTIONS TAKE THE PLACE OF THE SLIDE. CLICK A VERSE OR A SLIDE TO BRING IT BACK.",
        ))
        .font(fixed(11.5))
        .extra_letter_spacing(0.6)
        .color(TEXT_DIM),
    );
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use egui::{Event, Modifiers, RawInput, pos2, vec2};
    use sp_model::{ShowAction, parse_song_text};

    use super::*;
    use crate::home::AudioRequest;

    fn show() -> sp_engine::Show {
        sp_engine::Show::new(vec![Arc::new(parse_song_text("One", "Words\n"))])
    }

    /// Runs one frame of the tabs in a column as wide as the window's, with these events.
    /// Returns each piece of text it drew, with where, in the order drawn (menus last).
    fn frame(
        ctx: &egui::Context,
        show: &sp_engine::Show,
        state: &mut HomeState,
        events: Vec<Event>,
        time: f64,
    ) -> Vec<(String, Rect)> {
        fn walk(shape: &egui::Shape, into: &mut Vec<(String, Rect)>) {
            match shape {
                egui::Shape::Text(text) => into.push((text.galley.text().to_owned(), text.visual_bounding_rect())),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| walk(shape, into)),
                _ => {}
            }
        }
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(340.0, 1400.0))),
            time: Some(time),
            events,
            ..RawInput::default()
        };
        let output = ctx.run_ui(raw, |ui| live_tabs(ui, show, state, &mut |_| None));
        let mut texts = Vec::new();
        output.shapes.iter().for_each(|clipped| walk(&clipped.shape, &mut texts));
        output.drop_without_applying_deltas();
        texts
    }

    /// A moment after the last frame.
    fn next(ctx: &egui::Context) -> f64 {
        ctx.input(|input| input.time) + 0.1
    }

    #[test]
    fn a_timer_renamed_takes_the_messages_that_show_it_along() {
        let key = |key: egui::Key| Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        let ctx = egui::Context::default();
        let show = show();
        let mut state = HomeState::default();
        state.timers.push(Timer::new("Sermon".to_owned()));
        state.timers.push(Timer::new("Walk-in".to_owned()));
        let mut message = Message::new("Ends".to_owned());
        message.text = "Ends in {Timer: Sermon}".to_owned();
        state.messages.push(message);
        state.live_tab = 0;
        state.timer_open = Some(state.timers[0].id);
        let field = egui::Id::unique(("timer name", state.timers[0].id));
        let typing = |state: &mut HomeState, words: &str, end: egui::Key| {
            frame(&ctx, &show, state, vec![], next(&ctx));
            ctx.memory_mut(|memory| memory.request_focus(field));
            frame(&ctx, &show, state, vec![], next(&ctx));
            // Typed over the whole name.
            let all = vec![Event::Key {
                key: egui::Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::COMMAND,
            }];
            frame(&ctx, &show, state, all, next(&ctx));
            frame(&ctx, &show, state, vec![Event::Text(words.to_owned())], next(&ctx));
            frame(&ctx, &show, state, vec![key(end)], next(&ctx));
            frame(&ctx, &show, state, vec![], next(&ctx));
        };
        // While it is typed, nothing changes; Return gives the timer and the message the name.
        typing(&mut state, "Preaching", egui::Key::Enter);
        assert_eq!(state.timers[0].name, "Preaching");
        assert_eq!(state.messages[0].text, "Ends in {Timer: Preaching}");
        assert!(state.timers_changed && state.messages_changed);
        // Escape puts the name back, and the message keeps it.
        typing(&mut state, "Talk", egui::Key::Escape);
        assert_eq!(state.timers[0].name, "Preaching");
        assert_eq!(state.messages[0].text, "Ends in {Timer: Preaching}");
        // Another timer's name is refused, and says so.
        typing(&mut state, "walk-in", egui::Key::Enter);
        assert_eq!(state.timers[0].name, "Preaching");
        assert_eq!(state.notices.refused(), Some("There is a timer called walk-in already."));
    }

    /// The middle of where `words` were drawn last (in a menu, when it is open).
    fn place_of(texts: &[(String, Rect)], words: &str) -> egui::Pos2 {
        texts
            .iter()
            .rfind(|(text, _)| text == words)
            .map(|(_, rect)| rect.center())
            .unwrap_or_else(|| panic!("{words} is drawn: {:?}", texts.iter().map(|(text, _)| text).collect::<Vec<_>>()))
    }

    /// Moves the pointer to `at`, and with `press` presses and lets go there, as the pointer
    /// would. Returns what the last frame drew.
    fn point(
        ctx: &egui::Context,
        show: &sp_engine::Show,
        state: &mut HomeState,
        at: egui::Pos2,
        press: bool,
    ) -> Vec<(String, Rect)> {
        let button = |pressed| Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        let mut frames = vec![vec![Event::PointerMoved(at)], vec![]];
        if press {
            frames.extend([vec![button(true)], vec![button(false)], vec![]]);
        }
        let mut texts = Vec::new();
        for events in frames {
            texts = frame(ctx, show, state, events, next(ctx));
        }
        texts
    }

    /// The Macros tab with one macro, Opening, doing these steps, its steps open; and a prop
    /// (Logo) and a message (Phones) for steps to name.
    fn opened(steps: Vec<ShowAction>) -> (egui::Context, HomeState, sp_model::Uuid) {
        let mut state = HomeState { live_tab: 3, ..HomeState::default() };
        state.props.push(crate::props::Prop::new("Logo".to_owned()));
        state.messages.push(Message::new("Phones".to_owned()));
        let mut made = crate::macros::Macro::new("Opening".to_owned());
        made.actions = steps;
        let id = made.id;
        state.macros.push(made);
        let ctx = egui::Context::default();
        ctx.data_mut(|data| data.insert_temp(egui::Id::unique("macro steps open"), id));
        (ctx, state, id)
    }

    /// Clicks the middle of what was drawn under `id` (or a point `nudge` from it), as the
    /// pointer would.
    fn click(ctx: &egui::Context, show: &sp_engine::Show, state: &mut HomeState, id: egui::Id, nudge: egui::Vec2) {
        let start = ctx.input(|input| input.time) + 0.1;
        frame(ctx, show, state, vec![], start);
        let at = ctx.read_response(id).expect("drawn").rect.center() + nudge;
        let press = |pressed| Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        let frames = [vec![Event::PointerMoved(at)], vec![press(true)], vec![press(false)], vec![]];
        for (n, events) in frames.into_iter().enumerate() {
            frame(ctx, show, state, events, start + 0.1 * (n + 1) as f64);
        }
    }

    /// Where along what was drawn under `id` a share `along` of its width is, at its middle.
    fn along(ctx: &egui::Context, id: egui::Id, along: f32) -> egui::Pos2 {
        let rect = ctx.read_response(id).expect("drawn").rect;
        pos2(rect.min.x + rect.width() * along, rect.center().y)
    }

    /// Presses at `from`, drags to `to` in a few steps, and lets go there, as the pointer would.
    fn drag(ctx: &egui::Context, show: &sp_engine::Show, state: &mut HomeState, from: egui::Pos2, to: egui::Pos2) {
        let start = ctx.input(|input| input.time) + 0.1;
        let button = |at, pressed| Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        let mut frames = vec![vec![Event::PointerMoved(from)], vec![button(from, true)]];
        frames.extend((1..=4).map(|step| vec![Event::PointerMoved(from + (to - from) * (step as f32 / 4.0))]));
        frames.extend([vec![button(to, false)], vec![]]);
        for (n, events) in frames.into_iter().enumerate() {
            frame(ctx, show, state, events, start + 0.1 * (n + 1) as f64);
        }
    }

    /// Tracks of these names (their files never read) in the playlist shown, by their ids.
    fn tracks(state: &mut HomeState, names: &[&str]) -> Vec<sp_model::Uuid> {
        let tracks: Vec<crate::audio::Track> =
            names.iter().map(|name| crate::audio::Track::of(format!("{name}.wav").into())).collect();
        let ids = tracks.iter().map(|track| track.id).collect();
        state.audio.shown_mut().tracks = tracks;
        ids
    }

    #[test]
    fn dragging_the_level_sets_the_volume_where_it_is_let_go() {
        let show = show();
        let mut state = HomeState { live_tab: 5, audio_volume: 0.5, ..HomeState::default() };
        let ctx = egui::Context::default();
        frame(&ctx, &show, &mut state, vec![], 0.0);
        let level = egui::Id::unique("audio level");
        // From the handle (a third of the way, as the line goes to 150%) to nine tenths along.
        drag(&ctx, &show, &mut state, along(&ctx, level, 0.5 / 1.5), along(&ctx, level, 0.9));
        let Some(AudioRequest::Volume(volume)) = state.audio_request else { panic!("{:?}", state.audio_request) };
        assert!((volume - 1.35).abs() < 0.02, "{volume}");
        assert_eq!(state.audio_volume, volume);
    }

    #[test]
    fn dragging_the_place_in_a_track_jumps_by_the_difference() {
        use std::time::Duration;
        let show = show();
        let mut state = HomeState { live_tab: 5, ..HomeState::default() };
        let ids = tracks(&mut state, &["One", "Two"]);
        state.audio_playing = crate::audio::Playing {
            track: Some(ids[0]),
            position: Duration::from_secs(30),
            length: Some(Duration::from_secs(100)),
            paused: false,
        };
        let ctx = egui::Context::default();
        frame(&ctx, &show, &mut state, vec![], 0.0);
        let seek = egui::Id::unique("audio seek");
        drag(&ctx, &show, &mut state, along(&ctx, seek, 0.3), along(&ctx, seek, 0.6));
        let Some(AudioRequest::Skip(by)) = state.audio_request else { panic!("{:?}", state.audio_request) };
        assert!((by - 30.0).abs() < 1.0, "{by}");
    }

    #[test]
    fn a_track_dragged_below_another_moves_there() {
        let show = show();
        let mut state = HomeState { live_tab: 5, ..HomeState::default() };
        let ids = tracks(&mut state, &["One", "Two", "Three"]);
        let ctx = egui::Context::default();
        frame(&ctx, &show, &mut state, vec![], 0.0);
        let row = |id| ctx.read_response(egui::Id::unique(("audio track", id))).expect("drawn").rect;
        // From the first row's name to the lower half of the last.
        let from = row(ids[0]).left_center() + vec2(60.0, 0.0);
        let to = row(ids[2]).center() + vec2(-40.0, 10.0);
        drag(&ctx, &show, &mut state, from, to);
        let order: Vec<sp_model::Uuid> = state.audio.shown().tracks.iter().map(|track| track.id).collect();
        assert_eq!(order, vec![ids[1], ids[2], ids[0]]);
        assert_eq!(state.audio_request, None, "a drag plays nothing");
    }

    #[test]
    fn a_new_audio_playlist_is_shown_with_its_name_ready_to_type() {
        let show = show();
        let mut state = HomeState { live_tab: 5, ..HomeState::default() };
        let ctx = egui::Context::default();
        let before = state.audio.playlists.len();
        click(&ctx, &show, &mut state, egui::Id::unique(("tab heading +", "Audio")), vec2(0.0, 0.0));
        assert_eq!(state.audio.playlists.len(), before + 1);
        assert_eq!(state.audio.current, before, "the new one is shown");
        assert_eq!(state.audio.shown().name, "New Audio Playlist");
        let typing = ctx.data(|data| data.get_temp::<String>(egui::Id::unique("audio playlist renaming")));
        assert_eq!(typing.as_deref(), Some("New Audio Playlist"), "its name is ready to type");
        assert!(state.audio_changed);
    }

    #[test]
    fn a_macro_runs_from_its_run_key_and_not_from_its_name() {
        let show = show();
        let mut state = HomeState { live_tab: 3, ..HomeState::default() };
        let mut made = crate::macros::Macro::new("Opening".to_owned());
        made.actions = vec![ShowAction::Clear { what: sp_model::ClearWhat::All }];
        let id = made.id;
        state.macros.push(made);
        let ctx = egui::Context::default();
        // The card's name, at its left.
        click(&ctx, &show, &mut state, egui::Id::unique(("macro card", id)), vec2(-100.0, 0.0));
        assert!(state.actions_to_run.is_empty(), "the name alone does nothing");
        click(&ctx, &show, &mut state, egui::Id::unique(("macro run", id)), vec2(0.0, 0.0));
        assert_eq!(state.actions_to_run.len(), 1);
    }

    #[test]
    fn add_a_step_offers_every_step_in_groups_and_adds_the_one_chosen() {
        use sp_model::ClearWhat;
        let show = show();
        let (ctx, mut state, id) = opened(vec![ShowAction::Clear { what: ClearWhat::All }]);
        let logo = state.props[0].id;
        click(&ctx, &show, &mut state, egui::Id::unique(("macro add step", id)), vec2(0.0, 0.0));
        let texts = frame(&ctx, &show, &mut state, vec![], next(&ctx));
        place_of(&texts, "ADD A STEP");
        let groups = [
            "Clear",
            "Messages",
            "Props",
            "Timers",
            "Audience Screens",
            "Go to a Song",
            "Show a Picture or Video",
            "Announcements",
            "Theme Version",
            "Sync from Canva",
        ];
        let heights: Vec<f32> = groups.iter().map(|group| place_of(&texts, group).y).collect();
        assert!(heights.windows(2).all(|pair| pair[0] < pair[1]), "in this order: {groups:?}");
        // Props opens its steps; Show Logo adds one, at the end.
        let texts = point(&ctx, &show, &mut state, place_of(&texts, "Props"), false);
        point(&ctx, &show, &mut state, place_of(&texts, "Show Logo"), true);
        let added = vec![ShowAction::Clear { what: ClearWhat::All }, ShowAction::Prop { id: logo, show: true }];
        assert_eq!(state.macros[0].actions, added);
        assert!(state.macros_changed);
    }

    #[test]
    fn a_macro_step_can_clear_the_audio_or_the_announcements() {
        use sp_model::ClearWhat;
        let show = show();
        let (ctx, mut state, id) = opened(vec![ShowAction::Clear { what: ClearWhat::All }]);
        click(&ctx, &show, &mut state, egui::Id::unique(("macro add step", id)), vec2(0.0, 0.0));
        let texts = frame(&ctx, &show, &mut state, vec![], next(&ctx));
        let texts = point(&ctx, &show, &mut state, place_of(&texts, "Clear"), false);
        // Every layer the right column's keys clear, under Clear All.
        let heights: Vec<f32> = ClearWhat::ALL.iter().map(|what| place_of(&texts, what.name()).y).collect();
        assert!(heights.windows(2).all(|pair| pair[0] < pair[1]), "in this order: {:?}", ClearWhat::ALL);
        point(&ctx, &show, &mut state, place_of(&texts, "Clear Announcements"), true);
        assert_eq!(cleared(&state), [ClearWhat::All, ClearWhat::Announcements]);
    }

    /// The rows of an open macro's first `count` steps, top to bottom, as drawn last.
    fn step_rows(ctx: &egui::Context, id: sp_model::Uuid, count: usize) -> Vec<Rect> {
        (0..count).map(|at| ctx.read_response(egui::Id::unique(("macro step", id, at))).expect("drawn").rect).collect()
    }

    /// Steps that clear these, and what the first macro's steps clear now.
    fn clearing(what: &[sp_model::ClearWhat]) -> Vec<ShowAction> {
        what.iter().map(|what| ShowAction::Clear { what: *what }).collect()
    }

    fn cleared(state: &HomeState) -> Vec<sp_model::ClearWhat> {
        let what = |step: &ShowAction| if let ShowAction::Clear { what } = step { Some(*what) } else { None };
        state.macros[0].actions.iter().filter_map(what).collect()
    }

    /// Presses at `from` and carries it to `to` in a few steps, holding it there; returns what the
    /// last frame drew. `let_go` lets go (with Escape first when `escape`).
    fn hold(
        ctx: &egui::Context,
        show: &sp_engine::Show,
        state: &mut HomeState,
        from: egui::Pos2,
        to: egui::Pos2,
    ) -> Vec<(String, Rect)> {
        let press = Event::PointerButton {
            pos: from,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        };
        let mut frames = vec![vec![Event::PointerMoved(from)], vec![press]];
        frames.extend((1..=4).map(|step| vec![Event::PointerMoved(from + (to - from) * (step as f32 / 4.0))]));
        let mut texts = Vec::new();
        for events in frames {
            texts = frame(ctx, show, state, events, next(ctx));
        }
        texts
    }

    fn let_go(
        ctx: &egui::Context,
        show: &sp_engine::Show,
        state: &mut HomeState,
        at: egui::Pos2,
        escape: bool,
    ) -> Vec<(String, Rect)> {
        let release = Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        };
        let key = Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        let mut frames = if escape { vec![vec![key]] } else { Vec::new() };
        frames.extend([vec![release], vec![]]);
        let mut texts = Vec::new();
        for events in frames {
            texts = frame(ctx, show, state, events, next(ctx));
        }
        texts
    }

    #[test]
    fn a_step_dragged_goes_where_the_ink_line_was() {
        use sp_model::ClearWhat::{All, Media, Props, Slide};
        let show = show();
        let (ctx, mut state, id) = opened(clearing(&[All, Slide, Media, Props]));
        frame(&ctx, &show, &mut state, vec![], 0.0);
        let rows = step_rows(&ctx, id, 4);
        // The first step, over the lower half of the third: its words follow the pointer, and it
        // goes after the third.
        let to = rows[2].center() + vec2(0.0, rows[2].height() / 4.0);
        let held = hold(&ctx, &show, &mut state, rows[0].center(), to);
        assert_eq!(held.iter().filter(|(words, _)| words == "Clear All").count(), 2, "its words follow the pointer");
        assert_eq!(cleared(&state), [All, Slide, Media, Props], "nothing moves until it is let go");
        let texts = let_go(&ctx, &show, &mut state, to, false);
        assert_eq!(cleared(&state), [Slide, Media, All, Props]);
        assert!(state.macros_changed);
        assert!(!texts.iter().any(|(words, _)| words == "CHANGE THIS STEP TO"), "a drag opens no menu");
        // The last, over the upper half of the first: it goes first.
        let to = rows[0].center() - vec2(0.0, rows[0].height() / 4.0);
        hold(&ctx, &show, &mut state, rows[3].center(), to);
        let_go(&ctx, &show, &mut state, to, false);
        assert_eq!(cleared(&state), [Props, Slide, Media, All]);
    }

    #[test]
    fn escape_stops_a_step_being_dragged_and_it_stays_where_it_was() {
        use sp_model::ClearWhat::{All, Media, Slide};
        let show = show();
        let (ctx, mut state, id) = opened(clearing(&[All, Slide, Media]));
        frame(&ctx, &show, &mut state, vec![], 0.0);
        let rows = step_rows(&ctx, id, 3);
        let to = rows[2].center() + vec2(0.0, rows[2].height() / 4.0);
        hold(&ctx, &show, &mut state, rows[0].center(), to);
        let texts = let_go(&ctx, &show, &mut state, to, true);
        assert_eq!(cleared(&state), [All, Slide, Media]);
        assert!(!state.macros_changed);
        assert!(!texts.iter().any(|(words, _)| words == "CHANGE THIS STEP TO"), "nor is it a click");
    }

    #[test]
    fn a_press_on_a_step_that_barely_moves_is_a_click_and_opens_its_menu() {
        use sp_model::ClearWhat::{All, Slide};
        let show = show();
        let (ctx, mut state, id) = opened(clearing(&[All, Slide]));
        frame(&ctx, &show, &mut state, vec![], 0.0);
        let rows = step_rows(&ctx, id, 2);
        // Pressed on the first, moved 3 points towards the second, let go.
        let at = rows[0].center();
        hold(&ctx, &show, &mut state, at, at + vec2(0.0, 3.0));
        let texts = let_go(&ctx, &show, &mut state, at + vec2(0.0, 3.0), false);
        place_of(&texts, "CHANGE THIS STEP TO");
        assert_eq!(cleared(&state), [All, Slide]);
        assert!(!state.macros_changed);
    }

    #[test]
    fn a_step_changed_from_its_menu_keeps_its_place() {
        use sp_model::ClearWhat::All;
        let show = show();
        let (ctx, mut state, id) = opened(Vec::new());
        let (logo, phones) = (state.props[0].id, state.messages[0].id);
        let off = ShowAction::Audience { on: false };
        state.macros[0].actions =
            vec![ShowAction::Clear { what: All }, ShowAction::Prop { id: logo, show: true }, off.clone()];
        click(&ctx, &show, &mut state, egui::Id::unique(("macro step", id, 1)), vec2(0.0, 0.0));
        let texts = frame(&ctx, &show, &mut state, vec![], next(&ctx));
        place_of(&texts, "CHANGE THIS STEP TO");
        let texts = point(&ctx, &show, &mut state, place_of(&texts, "Messages"), false);
        point(&ctx, &show, &mut state, place_of(&texts, "Show Phones"), true);
        let changed = vec![ShowAction::Clear { what: All }, ShowAction::Message { id: phones, show: true }, off];
        assert_eq!(state.macros[0].actions, changed);
        assert!(state.macros_changed);
    }

    #[test]
    fn a_step_whose_song_has_gone_says_so() {
        let show = show();
        let gone = ShowAction::Announce { what: sp_model::Announced::Song { key: "nowhere".to_owned() } };
        let (ctx, mut state, _) = opened(vec![gone]);
        let texts = frame(&ctx, &show, &mut state, vec![], 0.0);
        place_of(&texts, "Play on the Announcements Layer: (a song no longer in the library)");
    }

    #[test]
    fn a_version_step_reads_by_its_name_and_a_removed_one_says_so() {
        let show = show();
        let offering = sp_model::Version { id: sp_model::Uuid::from_u128(7), name: "Offering".to_owned() };
        let steps = vec![
            ShowAction::Version { id: Some(offering.id), keep: true },
            ShowAction::Version { id: Some(sp_model::Uuid::from_u128(8)), keep: false },
        ];
        let (ctx, mut state, id) = opened(steps);
        state.versions = vec![offering.clone()];
        let texts = frame(&ctx, &show, &mut state, vec![], 0.0);
        place_of(&texts, "Use the Offering version until switched back");
        place_of(&texts, "Use (a version that has been removed)");
        // Its menu offers the version's steps under Theme Version; one chosen takes its place.
        click(&ctx, &show, &mut state, egui::Id::unique(("macro step", id, 1)), vec2(0.0, 0.0));
        let texts = frame(&ctx, &show, &mut state, vec![], next(&ctx));
        let texts = point(&ctx, &show, &mut state, place_of(&texts, "Theme Version"), false);
        point(&ctx, &show, &mut state, place_of(&texts, "Back to the usual look"), true);
        assert_eq!(state.macros[0].actions[1], ShowAction::Version { id: None, keep: false });
    }

    #[test]
    fn a_right_click_on_a_step_opens_its_menu_too() {
        use sp_model::ClearWhat::{All, Slide};
        let show = show();
        let (ctx, mut state, id) = opened(clearing(&[All, Slide]));
        frame(&ctx, &show, &mut state, vec![], 0.0);
        let at = step_rows(&ctx, id, 2)[1].center();
        let button = |pressed| Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        let mut texts = Vec::new();
        for events in [vec![Event::PointerMoved(at)], vec![button(true)], vec![button(false)], vec![]] {
            texts = frame(&ctx, &show, &mut state, events, next(&ctx));
        }
        place_of(&texts, "CHANGE THIS STEP TO");
    }

    #[test]
    fn a_steps_song_is_found_as_running_the_step_finds_it() {
        // The song One is kept under another key than its id: Go to Song finds it by its id, as a
        // playlist does, but the Announcements layer plays only the song kept under the key itself.
        let show = show();
        let id = show.presentations()[0].id.to_string();
        let steps = vec![
            ShowAction::Song { key: id.clone() },
            ShowAction::Announce { what: sp_model::Announced::Song { key: id } },
            ShowAction::Announce { what: sp_model::Announced::Song { key: "one-file".to_owned() } },
        ];
        let (ctx, mut state, _) = opened(steps);
        state.song_keys = vec!["one-file".to_owned()];
        let texts = frame(&ctx, &show, &mut state, vec![], 0.0);
        place_of(&texts, "Go to Song: One");
        place_of(&texts, "Play on the Announcements Layer: (a song no longer in the library)");
        place_of(&texts, "Play on the Announcements Layer: One");
    }

    #[test]
    fn show_and_hide_put_a_message_on_and_take_it_off() {
        let show = show();
        let mut state = HomeState { live_tab: 1, ..HomeState::default() };
        let message = Message::new("Phones".to_owned());
        let id = message.id;
        state.messages.push(message);
        let ctx = egui::Context::default();
        let key = egui::Id::unique(("message show", id));
        click(&ctx, &show, &mut state, key, vec2(0.0, 0.0));
        assert_eq!(state.message_live.map(|(live, _)| live), Some(id));
        let wide = ctx.read_response(key).expect("drawn").rect.width();
        click(&ctx, &show, &mut state, key, vec2(0.0, 0.0));
        assert_eq!(state.message_live, None);
        assert_eq!(ctx.read_response(key).expect("drawn").rect.width(), wide, "Show and Hide are as wide");
    }

    #[test]
    fn a_timer_starts_from_its_key_and_resets() {
        let show = show();
        let mut state = HomeState::default();
        let timer = Timer::new("Sermon".to_owned());
        let id = timer.id;
        state.timers.push(timer);
        let ctx = egui::Context::default();
        click(&ctx, &show, &mut state, egui::Id::unique(("timer start", id)), vec2(0.0, 0.0));
        assert!(state.timers[0].running());
        click(&ctx, &show, &mut state, egui::Id::unique(("timer reset", id)), vec2(0.0, 0.0));
        assert!(!state.timers[0].running());
    }

    #[test]
    fn a_layout_tile_switches_every_stage_screen() {
        use crate::screens::{Output, Screen};
        let show = show();
        let mut state = HomeState { live_tab: 4, ..HomeState::default() };
        state.screens = vec![
            Screen::new("Room".to_owned(), Output::Window),
            Screen::stage("Band".to_owned(), Output::Window),
            Screen::stage("Pulpit".to_owned(), Output::Window),
        ];
        let layouts = crate::stage::builtin_layouts();
        let ctx = egui::Context::default();
        click(&ctx, &show, &mut state, egui::Id::unique(("stage layout tile", layouts[1].id)), vec2(0.0, 0.0));
        assert_eq!(state.screens[1].stage_layout, Some(layouts[1].id));
        assert_eq!(state.screens[2].stage_layout, Some(layouts[1].id));
        assert_eq!(state.screens[0].stage_layout, None, "an audience screen has no layout");
        assert!(state.screens_changed);
    }

    #[test]
    fn the_audio_keys_ask_the_player_for_what_it_can_do() {
        let show = show();
        let mut state = HomeState { live_tab: 5, audio_volume: 0.8, ..HomeState::default() };
        let tracks: Vec<crate::audio::Track> =
            ["One", "Two"].iter().map(|name| crate::audio::Track::of(format!("{name}.wav").into())).collect();
        let (first, second) = (tracks[0].id, tracks[1].id);
        state.audio.shown_mut().tracks = tracks;
        let ctx = egui::Context::default();
        // With nothing playing, Next chooses the track Play starts, and plays nothing.
        click(&ctx, &show, &mut state, egui::Id::unique("audio next"), vec2(0.0, 0.0));
        assert_eq!(state.audio_request, None);
        click(&ctx, &show, &mut state, egui::Id::unique("audio play"), vec2(0.0, 0.0));
        assert_eq!(state.audio_request.take(), Some(AudioRequest::Play(second)));
        // A track's row plays it.
        click(&ctx, &show, &mut state, egui::Id::unique(("audio track", first)), vec2(0.0, 0.0));
        assert_eq!(state.audio_request.take(), Some(AudioRequest::Play(first)));
        // Mute turns the level down to nothing, and back to where it was.
        click(&ctx, &show, &mut state, egui::Id::unique("audio mute"), vec2(0.0, 0.0));
        assert_eq!(state.audio_request.take(), Some(AudioRequest::Volume(0.0)));
        click(&ctx, &show, &mut state, egui::Id::unique("audio mute"), vec2(0.0, 0.0));
        assert_eq!(state.audio_request.take(), Some(AudioRequest::Volume(0.8)));
    }

    #[test]
    fn a_card_or_track_renamed_in_place_keeps_its_name_where_and_as_it_was_and_its_height() {
        use crate::studio::tests::{drawn, key, letters, same_letters, styled};
        let ctx = styled();
        let show = show();
        let mut state = HomeState::default();
        state.messages.push(Message::new("Notice for parents".to_owned()));
        state.props.push(crate::props::Prop::new("Logo".to_owned()));
        state.macros.push(crate::macros::Macro::new("Start the service".to_owned()));
        let track = tracks(&mut state, &["Prelude"])[0];
        let ids = [state.messages[0].id, state.props[0].id, state.macros[0].id, track];
        let places = [("message card", 1), ("prop card", 2), ("macro card", 3), ("audio track", 5)];
        let names = ["Notice for parents", "Logo", "Start the service", "Prelude"];
        for (((place, tab), id), words) in places.into_iter().zip(ids).zip(names) {
            state.live_tab = tab;
            let drawn_tab = |state: &mut HomeState, events: Vec<Event>| {
                drawn(&ctx, events, |ui| live_tabs(ui, &show, state, &mut |_| None))
            };
            let card = || ctx.read_response(egui::Id::unique((place, id))).expect("drawn").rect;
            let before = letters(drawn_tab(&mut state, vec![]), words);
            let was = card();
            let renaming = (id, words.to_owned());
            match tab {
                1 => state.message_rename = Some(renaming),
                2 => state.prop_rename = Some(renaming),
                3 => state.macro_rename = Some(renaming),
                _ => ctx.data_mut(|data| {
                    data.insert_temp(egui::Id::unique("audio track renaming"), renaming);
                }),
            }
            state.rename_started = true;
            drawn_tab(&mut state, vec![]).drop_without_applying_deltas();
            same_letters(&before, &letters(drawn_tab(&mut state, vec![]), words), place);
            assert_eq!(card(), was, "{place}: as tall, in the same place");
            drawn_tab(&mut state, vec![key(egui::Key::Escape)]).drop_without_applying_deltas();
            let left = state.message_rename.is_none() && state.prop_rename.is_none() && state.macro_rename.is_none();
            assert!(left, "{place}: Escape ends it");
        }
    }

    #[test]
    fn an_audio_playlist_renamed_keeps_its_menu_round_it_and_its_name_where_it_was() {
        use crate::studio::tests::{drawn, key, letters, same_letters, styled};
        let ctx = styled();
        let show = show();
        let mut state = HomeState { live_tab: 5, ..HomeState::default() };
        state.audio.shown_mut().name = "Walk-in".to_owned();
        let drawn_tab = |state: &mut HomeState, events: Vec<Event>| {
            drawn(&ctx, events, |ui| live_tabs(ui, &show, state, &mut |_| None))
        };
        let menu = || ctx.read_response(egui::Id::unique("audio playlist picker")).expect("drawn").rect;
        let before = letters(drawn_tab(&mut state, vec![]), "Walk-in");
        let was = menu();
        let renaming = egui::Id::unique("audio playlist renaming");
        ctx.data_mut(|data| data.insert_temp(renaming, "Walk-in".to_owned()));
        drawn_tab(&mut state, vec![]).drop_without_applying_deltas();
        same_letters(&before, &letters(drawn_tab(&mut state, vec![]), "Walk-in"), "audio playlist");
        assert_eq!(menu(), was, "the menu is still drawn round the name");
        drawn_tab(&mut state, vec![Event::Text(" music".to_owned())]).drop_without_applying_deltas();
        drawn_tab(&mut state, vec![key(egui::Key::Enter)]).drop_without_applying_deltas();
        assert_eq!(state.audio.shown().name, "Walk-in music", "Return keeps what was typed");
    }
}
