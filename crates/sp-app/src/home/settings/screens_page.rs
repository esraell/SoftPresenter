//! Settings › Screens, as the design's cards (22 November), on the screens as the app keeps
//! them: how many are going out and how many need attention; the check strip (mark each screen
//! with its name and colour, a test pattern on every screen); the audience and stage screens'
//! switches and Details; then a card for each screen, audience screens first, and one that adds
//! a screen. A card opens that screen in the Screens window, where everything about it is set,
//! until the design's setup in place (its questions need what each screen is for, which the app
//! doesn't keep yet) is agreed.

use egui::{Color32, FontId, Rect, Sense, Stroke, StrokeKind, pos2, vec2};
use sp_engine::Show;

use super::parts;
use crate::home::HomeState;
use crate::i18n::{t, tf, tn};
use crate::screens::{Kind, Output, Screen};
use crate::studio::{
    CARD, CARE, Flat, LINE, PANEL, READY, TEXT, TEXT_DIM, TEXT_FAINT, fixed, fixed_semibold, flat_button, semibold,
};

/// The small capitals beside the page's switches: their size and the room between their letters.
const SUB: f32 = 12.0;
const SUB_SPACING: f32 = 0.6;
/// The room between the cards, and the least a card is wide before the grid takes fewer across.
const CARD_GAP: f32 = 14.0;
const LEAST_CARD: f32 = 200.0;
/// A card's padding, its corners, and its picture's.
const CARD_PAD: f32 = 10.0;
const CARD_CORNER: f32 = 11.0;
const PICTURE_CORNER: f32 = 7.0;
/// The Add a screen card is at least this tall, as the design's.
const ADD_LEAST: f32 = 220.0;
/// A card's row with its colour, name and On key; and the height of its fix key.
const NAME_ROW: f32 = 30.0;
const FIX_HEIGHT: f32 = 24.0;

/// What a click on a card asked for.
enum CardAsked {
    /// Open the screen where it is set up.
    Open,
    /// Its On key: off, as Output's Off in the Screens window.
    SwitchOff,
    /// Its fix: a window, for now.
    UseWindow,
}

/// How a screen stands, for its light and the colour of its words: going out (green), switched
/// off (grey: off by choice, or its kind's switch is off), or not showing for some other reason
/// (amber, never the grey of Off).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Tone {
    Going,
    Off,
    Trouble,
}

/// How a screen stands: going out when it is shown somewhere and its kind is switched on; off
/// when its kind is off or it is set to go nowhere; otherwise in trouble.
pub(super) fn tone(output: &Output, shown: bool, kind_on: bool) -> Tone {
    match (kind_on, shown, output) {
        (false, _, _) => Tone::Off,
        (true, true, _) => Tone::Going,
        (true, false, Output::Off) => Tone::Off,
        (true, false, _) => Tone::Trouble,
    }
}

/// Whether a screen's kind is switched on: the audience screens' switch, or the stage screens'.
fn kind_on(kind: Kind, state: &HomeState, show: &Show) -> bool {
    match kind {
        Kind::Audience => show.output_enabled(),
        Kind::Stage => state.stage_on,
    }
}

/// Each screen's place in the list and how it stands, audience screens first, then stage
/// screens, each in the list's order: the order the cards are in.
fn in_order(state: &HomeState, show: &Show) -> Vec<(usize, Tone)> {
    let mut order: Vec<(usize, Tone)> = Vec::new();
    for kind in [Kind::Audience, Kind::Stage] {
        for (index, screen) in state.screens.iter().enumerate().filter(|(_, screen)| screen.kind == kind) {
            let shown = state.screen_status.get(index).is_some_and(|(_, shown)| *shown);
            order.push((index, tone(&screen.output, shown, kind_on(kind, state, show))));
        }
    }
    order
}

pub(super) fn page(ui: &mut egui::Ui, state: &mut HomeState, show: &Show) {
    let order = in_order(state, show);
    summary(ui, state, show, &order);
    check_strip(ui, state);
    ui.add_space(16.0);
    switches(ui, state, show);
    ui.add_space(14.0);
    grid(ui, state, show, &order);
}

/// "3 of 4 going out", and how many need attention, in amber, after it.
fn summary(ui: &mut egui::Ui, state: &HomeState, show: &Show, order: &[(usize, Tone)]) {
    let (on, all) = super::screens_on(state, show);
    let counts = [("on", crate::i18n::count(on as u64)), ("all", crate::i18n::count(all as u64))];
    let counts: Vec<(&str, &str)> = counts.iter().map(|(name, value)| (*name, value.as_str())).collect();
    let mut job = egui::text::LayoutJob::default();
    let plain = egui::TextFormat { font_id: FontId::proportional(13.0), color: TEXT_FAINT, ..Default::default() };
    job.append(&tf("settings.screens.summary", "{on} of {all} going out", &counts), 0.0, plain.clone());
    let trouble = order.iter().filter(|(_, tone)| *tone == Tone::Trouble).count() as u64;
    if trouble > 0 {
        job.append(" \u{b7} ", 0.0, plain);
        let attention = tn("settings.screens.attention", trouble, "{n} needs attention", "{n} need attention", &[]);
        job.append(&attention, 0.0, egui::TextFormat { font_id: semibold(13.0), color: CARE, ..Default::default() });
    }
    let width = ui.available_width();
    job.wrap.max_width = width;
    let line = ui.ctx().fonts_mut(|fonts| fonts.layout_job(job));
    let (place, _) = ui.allocate_exact_size(vec2(width, line.size().y), Sense::hover());
    ui.painter().galley(place.min, line, TEXT_FAINT);
    ui.add_space(14.0);
}

/// The check strip: Identify, then the test patterns, and that everyone watching sees them.
fn check_strip(ui: &mut egui::Ui, state: &mut HomeState) {
    use crate::props::TestPattern;
    // The check strip: Identify, then the test patterns.
    let font = FontId::proportional(12.5);
    let (identify, stop) =
        (t("settings.screens.identify", "Identify"), t("settings.screens.identify.stop", "Stop identifying"));
    let identify_width = crate::studio::widest(ui, &[identify, stop], &font) + 26.0;
    let look = crate::studio::SwitchLook {
        font: FontId::proportional(12.0),
        spacing: 0.0,
        pad: 11.0,
        height: 24.0,
        inset: 3.0,
        track: CARD,
        idle: TEXT_DIM,
    };
    let patterns = [
        (None, t("settings.screens.pattern.off", "Off")),
        (Some(TestPattern::ColorBars), t("right.playing.bars", "Colour bars")),
        (Some(TestPattern::Grid), t("right.playing.grid", "Grid")),
        (Some(TestPattern::White), t("right.playing.white", "White")),
    ];
    let labels: Vec<&str> = patterns.iter().map(|(_, label)| *label).collect();
    let chosen = patterns.iter().position(|(pattern, _)| *pattern == state.test_pattern);
    let caption = parts::words(
        ui,
        &t("settings.screens.pattern", "Test pattern").to_uppercase(),
        fixed_semibold(11.5),
        TEXT_DIM,
        0.575,
        f32::INFINITY,
    );
    let seen_words = t("settings.screens.seen", "Everyone watching sees these").to_uppercase();
    let seen = |ui: &egui::Ui, wrap: f32| parts::words(ui, &seen_words, fixed(SUB), TEXT_FAINT, SUB_SPACING, wrap);
    // Identify, then the caption and the switch on the same line where they fit, 20 after it (the
    // design's rule there draws nothing); where they don't, the caption and the switch on a line
    // of their own under Identify (and the switch on a line of its own too, narrower still).
    let width = ui.available_width();
    let switch_width = crate::studio::pill_switch_width(ui, &labels, &look);
    let caption_width = caption.size().x;
    let one_line = identify_width + 20.0 + caption_width + 10.0 + switch_width <= width;
    let (strip, _) = ui.allocate_exact_size(vec2(width, 30.0), Sense::hover());
    let key = Rect::from_min_size(strip.min, vec2(identify_width, 30.0));
    let words = if state.identify { stop } else { identify };
    let look_now = if state.identify { Flat::On } else { Flat::Plain };
    let identify_response = flat_button(ui, key, ui.scope_id().with("identify"), words, font, look_now, true);
    if identify_response
        .on_hover_text(t(
            "settings.screens.identify.hint",
            "Each screen shows its name and colour. The room and the stream see it.",
        ))
        .clicked()
    {
        state.identify = !state.identify;
    }
    let line = if one_line {
        Rect::from_min_max(pos2(key.max.x + 20.0, strip.min.y), strip.max)
    } else {
        ui.add_space(8.0 - ui.spacing().item_spacing.y);
        ui.allocate_exact_size(vec2(width, 30.0), Sense::hover()).0
    };
    ui.painter().galley(pos2(line.min.x, line.center().y - caption.size().y / 2.0), caption, TEXT_DIM);
    let switch_at = if caption_width + 10.0 + switch_width <= line.width() {
        pos2(line.min.x + caption_width + 10.0, line.min.y)
    } else {
        ui.add_space(8.0 - ui.spacing().item_spacing.y);
        ui.allocate_exact_size(vec2(width, 30.0), Sense::hover()).0.min
    };
    let (track, clicked) = crate::studio::pill_switch(ui, "test pattern", switch_at, &labels, &[], chosen, &look);
    if let Some(at) = clicked {
        state.test_pattern = patterns[at].0;
    }
    // The note goes beside the switch where there is room, on two lines if it must, as the
    // design's; else under it.
    let beside = strip.max.x - track.max.x - 12.0;
    let wrapped = seen(ui, beside.max(1.0));
    if beside >= 120.0 && wrapped.rows.len() <= 2 && wrapped.size().x <= beside + 0.5 {
        let at = pos2(track.max.x + 12.0, track.center().y - wrapped.size().y / 2.0);
        ui.painter().galley(at, wrapped, TEXT_FAINT);
    } else {
        let whole = seen(ui, f32::INFINITY);
        ui.add_space(6.0);
        let (place, _) = ui.allocate_exact_size(vec2(ui.available_width(), whole.size().y), Sense::hover());
        ui.painter().galley(place.min, whole, TEXT_FAINT);
    }
}

/// "All audience screens" and "All stage screens", each with its On/Off key (the top bar's two
/// keys), and Details at the right end; each pair on a line of its own where they don't fit.
fn switches(ui: &mut egui::Ui, state: &mut HomeState, show: &Show) {
    let caps = |ui: &egui::Ui, words: &str| {
        parts::words(ui, &words.to_uppercase(), fixed(SUB), TEXT_FAINT, SUB_SPACING, f32::INFINITY)
    };
    let key = parts::on_off_size(ui);
    let pairs = [
        (caps(ui, t("settings.screens.all_audience", "All audience screens")), show.output_enabled()),
        (caps(ui, t("settings.screens.all_stage", "All stage screens")), state.stage_on),
        (caps(ui, t("settings.screens.details", "Details")), state.screens_details),
    ];
    let wide = |at: usize| pairs[at].0.size().x + 10.0 + key.x;
    let width = ui.available_width();
    // Where each pair goes, by line: the first two side by side, 24 apart, if they fit, and
    // Details at the right of the last line if it fits there, else on a line of its own.
    let mut lines: Vec<Vec<usize>> =
        if wide(0) + 24.0 + wide(1) <= width { vec![vec![0, 1]] } else { vec![vec![0], vec![1]] };
    let last: f32 = lines.last().map_or(0.0, |line| line.iter().map(|at| wide(*at) + 24.0).sum());
    if last + 16.0 + wide(2) <= width {
        lines.last_mut().expect("a line").push(2);
    } else {
        lines.push(vec![2]);
    }
    let mut clicked = None;
    for (number, line) in lines.iter().enumerate() {
        if number > 0 {
            ui.add_space(8.0);
        }
        let (place, _) = ui.allocate_exact_size(vec2(width, key.y), Sense::hover());
        let mut x = place.min.x;
        for at in line {
            let (caption, on) = (&pairs[*at].0, pairs[*at].1);
            // Details keeps to the right end, as the design's.
            if *at == 2 {
                x = place.max.x - wide(2);
            }
            let caption_height = caption.size().y;
            ui.painter().galley(pos2(x, place.center().y - caption_height / 2.0), caption.clone(), TEXT_FAINT);
            let rect = Rect::from_min_size(pos2(x + caption.size().x + 10.0, place.min.y), key);
            let mut response = parts::on_off(ui, rect, ui.scope_id().with(("screens switch", *at)), on);
            if *at == 2 {
                response = response.on_hover_text(t(
                    "settings.screens.details.hint",
                    "A technical line on every card: the output in full, its format and keyer, see-through or key, theme, layers, edges",
                ));
            }
            if response.clicked() {
                clicked = Some(*at);
            }
            x = rect.max.x + 24.0;
        }
    }
    match clicked {
        // The audience screens are switched as the menu's Audience Screens does.
        Some(0) => state.menu_asked = Some(crate::menubar::Action::Audience),
        Some(1) => state.stage_on = !state.stage_on,
        Some(_) => state.screens_details = !state.screens_details,
        None => {}
    }
}

/// What a card says and how it is laid out, worked out before it is drawn so that a row of the
/// grid can be as tall as its tallest card.
struct CardView {
    index: usize,
    tone: Tone,
    main: bool,
    unplugged: bool,
    /// Set to go somewhere (its On key): anything but Off.
    on: bool,
    /// The one key that helps a screen in trouble, under its lines: a window, for a display
    /// that isn't plugged in.
    fix: Option<&'static str>,
    name: std::sync::Arc<egui::Galley>,
    status: std::sync::Arc<egui::Galley>,
    what: std::sync::Arc<egui::Galley>,
    details: Vec<std::sync::Arc<egui::Galley>>,
    /// Where it goes, in a few words, in the middle of its picture.
    destination: String,
}

impl CardView {
    fn picture_height(width: f32) -> f32 {
        (width - 2.0 * CARD_PAD) * 9.0 / 16.0
    }

    fn height(&self, width: f32) -> f32 {
        let mut height = CARD_PAD + Self::picture_height(width) + 10.0 + NAME_ROW + 4.0;
        height += self.status.size().y + 4.0 + self.what.size().y;
        if !self.details.is_empty() {
            height += 4.0 + 6.0 + self.details.iter().map(|line| line.size().y + 2.0).sum::<f32>() - 2.0;
        }
        if self.fix.is_some() {
            height += 4.0 + FIX_HEIGHT;
        }
        height + CARD_PAD
    }
}

/// A screen's card, measured for a card `width` wide.
fn card_view(ui: &egui::Ui, state: &HomeState, show: &Show, index: usize, tone: Tone, width: f32) -> CardView {
    let screen = &state.screens[index];
    let inner = width - 2.0 * CARD_PAD;
    let status_words = if kind_on(screen.kind, state, show) {
        state.screen_status.get(index).map(|(doing, _)| doing.clone()).unwrap_or_default()
    } else {
        match screen.kind {
            Kind::Audience => t("settings.screens.audience_off", "All audience screens are off").to_owned(),
            Kind::Stage => t("settings.screens.stage_off", "All stage screens are off").to_owned(),
        }
    };
    let status_ink = match tone {
        Tone::Going => TEXT_DIM,
        Tone::Off => TEXT_FAINT,
        Tone::Trouble => CARE,
    };
    let kind = match screen.kind {
        Kind::Audience => t("settings.screens.kind.audience", "Audience"),
        Kind::Stage => t("settings.screens.kind.stage", "Stage"),
    };
    let destination = destination(screen);
    let what = format!("{kind} \u{b7} {destination}");
    let details = if state.screens_details {
        technical(screen, state)
            .iter()
            .map(|line| parts::words(ui, line, fixed(11.0), TEXT_FAINT, 0.0, inner))
            .collect()
    } else {
        Vec::new()
    };
    let main = screen.kind == Kind::Audience
        && state.screens.iter().position(|first| first.kind == Kind::Audience) == Some(index);
    let unplugged = matches!(&screen.output, Output::Display(name)
        if !state.displays.iter().any(|display| display.is_named(name)));
    let fix = (unplugged && tone == Tone::Trouble).then(|| t("settings.screens.use_window", "Use a window for now"));
    let key = parts::on_off_size(ui).x;
    CardView {
        index,
        tone,
        main,
        unplugged,
        on: screen.output != Output::Off,
        fix,
        name: parts::one_line(ui, &screen.name, semibold(14.0), TEXT, inner - 18.0 - 8.0 - key),
        status: parts::lines(ui, &status_words, FontId::proportional(12.0), status_ink, inner - 14.0, 16.0),
        what: parts::lines(ui, &what, FontId::proportional(12.0), TEXT_FAINT, inner, 17.0),
        details,
        destination,
    }
}

/// Where a screen goes, in a few words: the display's name, a window, NDI and its name, the
/// DeckLink output, automatic, or off.
fn destination(screen: &Screen) -> String {
    match &screen.output {
        Output::Automatic => t("screens.target.automatic", "Automatic").to_owned(),
        Output::Display(name) => name.clone(),
        Output::Window => t("screens.output.window", "A window").to_owned(),
        Output::Ndi(name) => tf("settings.screens.ndi", "NDI {name}", &[("name", &crate::i18n::quoted(name))]),
        Output::DeckLink(out) => out.device.clone(),
        Output::Off => t("screens.output.off", "Off").to_owned(),
    }
}

/// The technical lines Details adds to a card: where it goes in full (the display's size, NDI's
/// size and rate, the DeckLink format and keyer), then how it draws (solid, see-through and its
/// soft edges, or another screen's key; its theme; its layers; covered edges; its colour), or a
/// stage screen's layout.
fn technical(screen: &Screen, state: &HomeState) -> Vec<String> {
    use crate::screens::{Alpha, Keyer};
    let output = match &screen.output {
        Output::Display(name) => match state.displays.iter().find(|display| display.is_named(name)) {
            Some(display) => format!("{} \u{b7} {} \u{d7} {}", display.name, display.w, display.h),
            None => tf("settings.screens.tech.unplugged", "{name} \u{b7} not plugged in", &[("name", name)]),
        },
        Output::Ndi(name) => tf(
            "settings.screens.tech.ndi",
            "NDI {name} \u{b7} 1920 \u{d7} 1080 \u{b7} {rate}",
            &[("name", &crate::i18n::quoted(name)), ("rate", &screen.rate.name())],
        ),
        Output::DeckLink(out) => {
            let keyer = match out.keyer {
                Keyer::Off => String::new(),
                Keyer::External => format!(" \u{b7} {}", t("settings.screens.tech.fill_key", "fill and key")),
                Keyer::Internal => {
                    format!(" \u{b7} {}", t("settings.screens.tech.over", "over the card\u{2019}s input"))
                }
            };
            format!("{} \u{b7} {}{keyer}", out.device, out.mode)
        }
        _ => destination(screen),
    };
    if screen.kind == Kind::Stage {
        let layout = screen
            .stage_layout
            .and_then(|id| state.stage_layouts.iter().find(|layout| layout.id == id))
            .or(state.stage_layouts.first())
            .map(|layout| layout.name.clone())
            .unwrap_or_default();
        return vec![output, tf("settings.screens.tech.layout", "Layout: {name}", &[("name", &layout)])];
    }
    let look = &screen.look;
    let mut parts: Vec<String> = Vec::new();
    let background = if let Some(fill) = look.key_of {
        let name =
            state.screens.iter().find(|other| other.id == fill).map(|other| other.name.clone()).unwrap_or_default();
        tf("settings.screens.tech.key_of", "Key of {name}", &[("name", &name)])
    } else {
        match look.see_through {
            None => t("settings.screens.tech.solid", "Solid").to_owned(),
            Some(Alpha::Straight) => t("settings.screens.tech.straight", "See-through, straight").to_owned(),
            Some(Alpha::Premultiplied) => {
                t("settings.screens.tech.premultiplied", "See-through, premultiplied").to_owned()
            }
        }
    };
    parts.push(background);
    let names = state.themes_shown();
    let own = screen
        .theme
        .and_then(|id| state.theme_ids.iter().position(|have| *have == id))
        .and_then(|at| names.get(at).cloned());
    let main_name = names.get(state.theme_for_songs).cloned().unwrap_or_default();
    parts.push(own.unwrap_or_else(|| tf("screens.look.main_theme", "Main theme ({name})", &[("name", &main_name)])));
    let layers = [
        (look.slides, t("screens.look.slides", "Slides")),
        (look.media, t("screens.look.media", "Media")),
        (look.props, t("screens.look.props", "Props")),
        (look.messages, t("screens.look.messages", "Messages")),
        (look.announcements, t("screens.look.announcements", "Announcements")),
    ];
    let shown: Vec<String> = layers.iter().filter(|(on, _)| *on).map(|(_, name)| name.to_lowercase()).collect();
    parts.push(match shown.len() {
        5 => t("settings.screens.tech.every_layer", "every layer").to_owned(),
        0 => t("settings.screens.tech.no_layers", "no layers").to_owned(),
        _ => shown.join(", "),
    });
    if look.mask.iter().any(|edge| *edge > 0.0) {
        let edges: Vec<String> = look.mask.iter().map(|edge| format!("{}", edge.round() as i32)).collect();
        parts.push(tf("settings.screens.tech.edges", "edges {edges}", &[("edges", &edges.join("\u{b7}"))]));
    }
    if look.see_through.is_none() && look.key_of.is_none() && (look.color.r, look.color.g, look.color.b) != (0, 0, 0) {
        let hex = format!("#{:02x}{:02x}{:02x}", look.color.r, look.color.g, look.color.b);
        parts.push(tf("settings.screens.tech.colour", "colour {hex}", &[("hex", &hex)]));
    }
    vec![output, parts.join(" \u{b7} ")]
}

/// The cards, three across where there is room (fewer in a narrower window), then Add a screen.
fn grid(ui: &mut egui::Ui, state: &mut HomeState, show: &Show, order: &[(usize, Tone)]) {
    let width = ui.available_width();
    let across = (((width + CARD_GAP) / (LEAST_CARD + CARD_GAP)).floor() as usize).clamp(1, 3);
    let card_width = (width - CARD_GAP * (across - 1) as f32) / across as f32;
    let cards: Vec<CardView> =
        order.iter().map(|(index, tone)| card_view(ui, state, show, *index, *tone, card_width)).collect();
    // The cards and, last, the Add card (None), a row of the grid at a time.
    let mut items: Vec<Option<&CardView>> = cards.iter().map(Some).collect();
    items.push(None);
    let mut opened = None;
    let mut add = false;
    let mut asked = None;
    for (number, row) in items.chunks(across).enumerate() {
        if number > 0 {
            ui.add_space(CARD_GAP);
        }
        let height = row.iter().map(|item| item.map_or(ADD_LEAST, |card| card.height(card_width))).fold(0.0, f32::max);
        let (place, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
        for (column, item) in row.iter().enumerate() {
            let x = place.min.x + column as f32 * (card_width + CARD_GAP);
            match item {
                // Every card of a row as tall as the tallest, as the design's grid has them.
                Some(card) => {
                    let rect = Rect::from_min_size(pos2(x, place.min.y), vec2(card_width, height));
                    if let Some(what) = draw_card(ui, rect, card, &state.screens[card.index], state.identify) {
                        asked = Some((card.index, what));
                    }
                }
                None => {
                    let rect = Rect::from_min_size(pos2(x, place.min.y), vec2(card_width, height));
                    add = add_card(ui, rect);
                }
            }
        }
    }
    ui.add_space(22.0);
    match asked {
        Some((index, CardAsked::Open)) => opened = Some(index),
        // On to Off is the Screens window's Off: the screen stays in the list, going nowhere.
        Some((index, CardAsked::SwitchOff)) => {
            state.screens[index].output = Output::Off;
            state.ndi_draft = None;
            state.screens_changed = true;
        }
        Some((index, CardAsked::UseWindow)) => {
            state.screens[index].output = Output::Window;
            state.ndi_draft = None;
            state.screens_changed = true;
        }
        None => {}
    }
    if add {
        // As the Screens window's +: a new audience screen goes nowhere until it is told where,
        // so nothing changes on any display by surprise; it opens there, to be set up.
        let name = crate::screens::next_name(&state.screens, Kind::Audience);
        state.screens.push(Screen::new(name, Output::Off));
        state.screens_changed = true;
        opened = Some(state.screens.len() - 1);
    }
    if let Some(index) = opened {
        state.screen_selected = index;
        state.ndi_draft = None;
        state.settings_open = true;
        state.screens_to_front = true;
    }
}

/// One screen's card: its picture, its colour, name and On key, how it stands and where it
/// goes, Details' lines, and the key that fixes its trouble. What a click on it asked for.
fn draw_card(ui: &egui::Ui, rect: Rect, card: &CardView, screen: &Screen, identify: bool) -> Option<CardAsked> {
    let response = ui.interact(rect, ui.scope_id().with(("screen card", screen.id)), Sense::click());
    let hint = match crate::keys::screens_key() {
        Some(key) => tf(
            "settings.screens.open.key",
            "Set up {name} in the Screens window ({key})",
            &[("name", &screen.name), ("key", &key)],
        ),
        None => tf("settings.screens.open", "Set up {name} in the Screens window", &[("name", &screen.name)]),
    };
    let response = response.on_hover_text(hint);
    let mut asked = response.clicked().then_some(CardAsked::Open);
    ui.painter().rect_filled(rect, CARD_CORNER, CARD);
    let inner = rect.width() - 2.0 * CARD_PAD;
    let picture =
        Rect::from_min_size(rect.min + vec2(CARD_PAD, CARD_PAD), vec2(inner, CardView::picture_height(rect.width())));
    paint_picture(ui, picture, card, screen, response.hovered(), identify);
    let mut y = picture.max.y + 10.0;
    // Its colour (as Identify shows it), its name, and its On key at the right.
    let painter = ui.painter();
    let chip = Rect::from_min_size(pos2(picture.min.x, y + NAME_ROW / 2.0 - 5.0), vec2(10.0, 10.0));
    painter.rect_filled(chip, 3.0, crate::home::color32(crate::screens::identify_color(card.index)));
    painter.galley(pos2(picture.min.x + 18.0, y + NAME_ROW / 2.0 - card.name.size().y / 2.0), card.name.clone(), TEXT);
    let key = parts::on_off_size(ui);
    let key_rect = Rect::from_min_size(pos2(picture.max.x - key.x, y + (NAME_ROW - key.y) / 2.0), key);
    let switch = parts::on_off(ui, key_rect, ui.scope_id().with(("screen on", screen.id)), card.on);
    let switch = if card.on {
        switch.on_hover_text(t(
            "settings.screens.on.hint",
            "Off sends it nowhere, as Off under Output in the Screens window",
        ))
    } else {
        switch.on_hover_text(t("settings.screens.off.hint", "Choose where it goes, in the Screens window"))
    };
    if switch.clicked() {
        asked = Some(if card.on { CardAsked::SwitchOff } else { CardAsked::Open });
    }
    y += NAME_ROW + 4.0;
    // How it stands: its light, and the words.
    let light = match card.tone {
        Tone::Going => READY,
        Tone::Off => Color32::from_rgb(0x5a, 0x5a, 0x56),
        Tone::Trouble => CARE,
    };
    let painter = ui.painter();
    painter.circle_filled(pos2(picture.min.x + 3.5, y + 8.0), 3.5, light);
    painter.galley(pos2(picture.min.x + 14.0, y), card.status.clone(), TEXT_DIM);
    y += card.status.size().y + 4.0;
    painter.galley(pos2(picture.min.x, y), card.what.clone(), TEXT_FAINT);
    y += card.what.size().y;
    if !card.details.is_empty() {
        y += 4.0;
        painter.hline(picture.min.x..=picture.max.x, y + 0.5, Stroke::new(1.0, LINE));
        y += 6.0;
        for line in &card.details {
            painter.galley(pos2(picture.min.x, y), line.clone(), TEXT_FAINT);
            y += line.size().y + 2.0;
        }
        y -= 2.0;
    }
    if let Some(fix) = card.fix {
        y += 4.0;
        let place = Rect::from_min_size(pos2(picture.min.x, y), vec2(inner, FIX_HEIGHT));
        if fix_key(ui, place, ui.scope_id().with(("screen fix", screen.id)), fix).clicked() {
            asked = Some(CardAsked::UseWindow);
        }
    }
    asked
}

/// A card's fix: the small key, as wide as the card, its words at the left.
fn fix_key(ui: &egui::Ui, rect: Rect, id: egui::Id, words: &str) -> egui::Response {
    let response = ui.interact(rect, id, Sense::click());
    let fill = if response.hovered() { crate::studio::BUTTON_HOVER } else { crate::studio::BUTTON };
    ui.painter().rect_filled(rect, 7.0, fill);
    let galley = parts::one_line(ui, words, FontId::proportional(11.5), TEXT, rect.width() - 18.0);
    ui.painter().galley(pos2(rect.min.x + 9.0, rect.center().y - galley.size().y / 2.0), galley, TEXT);
    response
}

/// A card's picture: the screen's own background (its colour, the checks of a see-through one,
/// the black of a key or a stage screen), where it goes in the middle, a green ring while it is
/// going out, and, while it isn't, dimmed with "Off" or "Not plugged in" over it. The app draws
/// no small live picture of each screen, so this stands in for one. While the screens are
/// identified, it is washed in the screen's colour with its name, as the screen itself is.
fn paint_picture(ui: &egui::Ui, picture: Rect, card: &CardView, screen: &Screen, hovered: bool, identify: bool) {
    let painter = ui.painter();
    let solid = screen.kind == Kind::Audience && screen.look.see_through.is_none() && screen.look.key_of.is_none();
    let fill = if solid {
        let colour = screen.look.color;
        Color32::from_rgb(colour.r, colour.g, colour.b)
    } else {
        Color32::BLACK
    };
    painter.rect_filled(picture, PICTURE_CORNER, fill);
    if screen.kind == Kind::Audience {
        crate::screens::paint_background(painter, picture, &screen.look);
    }
    let going = card.tone == Tone::Going;
    let words = |text: &str, ink: Color32| {
        let galley = parts::words(ui, &text.to_uppercase(), fixed(10.5), ink, 1.5, f32::INFINITY);
        let galley = if galley.size().x > picture.width() - 16.0 {
            parts::one_line(ui, &text.to_uppercase(), fixed(10.5), ink, picture.width() - 16.0)
        } else {
            galley
        };
        painter.galley(picture.center() - galley.size() / 2.0, galley, ink);
    };
    let colour = crate::home::color32(crate::screens::identify_color(card.index));
    if identify && going {
        painter.rect_filled(picture, PICTURE_CORNER, colour.gamma_multiply(0.35));
        let name = parts::one_line(ui, &screen.name, semibold(15.0), TEXT, picture.width() - 16.0);
        painter.galley(picture.center() - name.size() / 2.0, name, TEXT);
    } else if going || hovered {
        words(&card.destination, Color32::from_gray(0x8a));
    }
    if !going && !hovered {
        // The design's 40% for a screen that isn't going out, and its words over it.
        painter.rect_filled(picture, PICTURE_CORNER, PANEL.gamma_multiply(0.6));
        let over = if card.unplugged {
            t("settings.screens.unplugged", "Not plugged in")
        } else {
            t("settings.screens.picture_off", "Off")
        };
        words(over, Color32::from_gray(0x8a));
    }
    if card.main {
        let tag = parts::words(
            ui,
            &t("settings.screens.main", "Main").to_uppercase(),
            fixed_semibold(10.5),
            TEXT_DIM,
            0.6,
            f32::INFINITY,
        );
        let place = Rect::from_min_size(picture.min + vec2(6.0, 6.0), vec2(tag.size().x + 10.0, 16.0));
        painter.rect_filled(place, 4.0, Color32::from_rgba_unmultiplied(12, 12, 12, 209));
        painter.rect_stroke(place, 4.0, Stroke::new(1.0, LINE), StrokeKind::Inside);
        painter.galley(place.center() - tag.size() / 2.0, tag, TEXT_DIM);
    }
    if identify && going {
        painter.rect_stroke(picture, PICTURE_CORNER, Stroke::new(2.0, colour), StrokeKind::Outside);
    } else if going {
        painter.rect_stroke(picture, PICTURE_CORNER, Stroke::new(2.0, READY), StrokeKind::Outside);
    } else if hovered {
        painter.rect_stroke(picture, PICTURE_CORNER, Stroke::new(2.0, TEXT_FAINT), StrokeKind::Outside);
    }
}

/// The dashed card that adds a screen. True when clicked.
fn add_card(ui: &egui::Ui, rect: Rect) -> bool {
    let response = ui.interact(rect, ui.scope_id().with("add a screen"), Sense::click());
    let hovered = response.hovered();
    let edge = if hovered { TEXT_FAINT } else { LINE };
    let mut outline = parts::rounded_outline(rect.shrink(0.75), CARD_CORNER);
    if let Some(first) = outline.first().copied() {
        outline.push(first);
    }
    ui.painter().extend(egui::Shape::dashed_line(&outline, Stroke::new(1.5, edge), 5.0, 4.0));
    let ink = if hovered { TEXT } else { TEXT_FAINT };
    let room = rect.width() - 32.0;
    let title = centred(ui, t("settings.screens.add", "Add a screen"), semibold(14.0), TEXT, room);
    let about = centred(
        ui,
        t("settings.screens.add.about", "Choose where it goes and how it looks"),
        FontId::proportional(12.0),
        ink,
        room,
    );
    let height = 20.0 + 6.0 + title.size().y + 6.0 + about.size().y;
    let mut y = rect.center().y - height / 2.0;
    let plus = Rect::from_center_size(pos2(rect.center().x, y + 10.0), vec2(20.0, 20.0));
    crate::icons::paint(ui.painter(), crate::icons::Icon::Plus, plus, ink);
    y += 26.0;
    let title_height = title.size().y;
    ui.painter().galley(pos2(rect.center().x, y), title, TEXT);
    y += title_height + 6.0;
    ui.painter().galley(pos2(rect.center().x, y), about, ink);
    response.clicked()
}

/// Words wrapped at `wrap`, each line in the middle: drawn at the middle of where they go.
fn centred(ui: &egui::Ui, text: &str, font: FontId, ink: Color32, wrap: f32) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple(text.to_owned(), font, ink, wrap);
    job.halign = egui::Align::Center;
    ui.ctx().fonts_mut(|fonts| fonts.layout_job(job))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::home::settings::Pane;

    #[test]
    fn a_screen_is_going_out_off_or_in_trouble() {
        assert_eq!(tone(&Output::Window, true, true), Tone::Going);
        assert_eq!(tone(&Output::Window, true, false), Tone::Off, "its kind's switch is off");
        assert_eq!(tone(&Output::Off, false, true), Tone::Off, "set to go nowhere");
        assert_eq!(tone(&Output::Display("3DTV".to_owned()), false, true), Tone::Trouble);
        assert_eq!(tone(&Output::Ndi("Stream".to_owned()), false, true), Tone::Trouble, "amber, never Off's grey");
    }

    /// Settings at Screens, with a screen going to a window (the main one), one set to go
    /// nowhere, one on a display that isn't plugged in, and a stage screen in a window.
    fn state() -> HomeState {
        let mut state = HomeState { preferences_pane: Pane::Screens, ..HomeState::default() };
        let stage = Screen { kind: Kind::Stage, ..Screen::new("Confidence monitor".to_owned(), Output::Window) };
        state.screens = vec![
            Screen::new("Main projector".to_owned(), Output::Window),
            Screen::new("Spare".to_owned(), Output::Off),
            Screen::new("Lobby".to_owned(), Output::Display("3DTV".to_owned())),
            stage,
        ];
        state.screen_status = vec![
            ("In a window".to_owned(), true),
            ("Off".to_owned(), false),
            ("Disconnected: \"3DTV\" is not plugged in.".to_owned(), false),
            ("In a window".to_owned(), true),
        ];
        state.stage_on = true;
        state
    }

    /// Every piece of words a frame of Settings drew, `width` wide, and where.
    fn drawn(state: &mut HomeState, show: &Show, width: f32, events: Vec<egui::Event>) -> Vec<(String, Rect)> {
        fn walk(shape: &egui::Shape, into: &mut Vec<(String, Rect)>) {
            match shape {
                egui::Shape::Text(text) => into.push((text.galley.text().to_owned(), text.visual_bounding_rect())),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| walk(shape, into)),
                _ => {}
            }
        }
        let ctx = crate::studio::tests::styled();
        let mut words = Vec::new();
        for round in 0..2 {
            let raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(width, 3000.0))),
                events: if round == 1 { events.clone() } else { Vec::new() },
                ..egui::RawInput::default()
            };
            let output = ctx.run_ui(raw, |ui| super::super::settings_ui(ui, state, show));
            words.clear();
            output.shapes.iter().for_each(|clipped| walk(&clipped.shape, &mut words));
            output.drop_without_applying_deltas();
        }
        words
    }

    /// Clicks at `at`: pressed in one frame, let go in the next.
    fn click(state: &mut HomeState, show: &Show, at: egui::Pos2) {
        let ctx = crate::studio::tests::styled();
        let press = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        for events in [vec![], vec![egui::Event::PointerMoved(at), press(true)], vec![press(false)]] {
            let raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(1100.0, 3000.0))),
                events,
                ..egui::RawInput::default()
            };
            ctx.run_ui(raw, |ui| super::super::settings_ui(ui, state, show)).drop_without_applying_deltas();
        }
    }

    fn place(words: &[(String, Rect)], wanted: &str) -> Rect {
        words.iter().rev().find(|(said, _)| said == wanted).map(|(_, rect)| *rect).unwrap_or_else(|| panic!("{wanted}"))
    }

    /// The On/Off key on the same row as a card's name.
    fn key_beside(words: &[(String, Rect)], name: &str) -> egui::Pos2 {
        let row = place(words, name);
        words
            .iter()
            .find(|(said, rect)| {
                (said == "On" || said == "Off")
                    && (rect.center().y - row.center().y).abs() < 4.0
                    && rect.min.x > row.max.x
            })
            .map(|(_, rect)| rect.center())
            .unwrap_or_else(|| panic!("no key beside {name}"))
    }

    #[test]
    fn screens_is_the_designs_cards() {
        let show = Show::new(Vec::new());
        let mut state = state();
        let words = drawn(&mut state, &show, 1100.0, vec![]);
        let has = |wanted: &str| words.iter().any(|(said, _)| said == wanted);
        assert!(has("2 of 4 going out \u{b7} 1 needs attention"), "the summary, with the trouble counted");
        for wanted in [
            "Identify",
            "TEST PATTERN",
            "EVERYONE WATCHING SEES THESE",
            "ALL AUDIENCE SCREENS",
            "ALL STAGE SCREENS",
            "DETAILS",
            "MAIN",
            "Main projector",
            "Spare",
            "Lobby",
            "Confidence monitor",
            "Audience \u{b7} A window",
            "Stage \u{b7} A window",
            "Use a window for now",
            "Add a screen",
        ] {
            assert!(has(wanted), "{wanted}");
        }
        // Audience screens first, then stage screens, three across.
        assert!(place(&words, "Confidence monitor").min.y > place(&words, "Main projector").max.y);
        // Details adds the technical lines.
        assert!(!has("Solid \u{b7} Main theme () \u{b7} every layer"));
        state.screens_details = true;
        let words = drawn(&mut state, &show, 1100.0, vec![]);
        assert!(words.iter().any(|(said, _)| said.starts_with("Solid \u{b7} ")), "a solid screen's look");
        assert!(words.iter().any(|(said, _)| said == "3DTV \u{b7} not plugged in"));
    }

    #[test]
    fn a_cards_keys_do_what_the_screens_window_does() {
        let show = Show::new(Vec::new());
        let mut state = state();
        // On to Off: the screen goes nowhere, as Off under Output.
        let words = drawn(&mut state, &show, 1100.0, vec![]);
        click(&mut state, &show, key_beside(&words, "Main projector"));
        assert_eq!(state.screens[0].output, Output::Off);
        assert!(std::mem::take(&mut state.screens_changed));
        // Off asks where it goes, in the Screens window.
        let words = drawn(&mut state, &show, 1100.0, vec![]);
        click(&mut state, &show, key_beside(&words, "Spare"));
        assert_eq!(state.screens[1].output, Output::Off, "nothing is guessed");
        assert!(state.settings_open && state.screen_selected == 1 && state.screens_to_front);
        state.settings_open = false;
        // A display that isn't plugged in: a window, for now.
        let words = drawn(&mut state, &show, 1100.0, vec![]);
        click(&mut state, &show, place(&words, "Use a window for now").center());
        assert_eq!(state.screens[2].output, Output::Window);
        // A click on a card opens it in the Screens window.
        let words = drawn(&mut state, &show, 1100.0, vec![]);
        click(&mut state, &show, place(&words, "Confidence monitor").center() - vec2(0.0, 60.0));
        assert!(state.settings_open && state.screen_selected == 3);
        // Add a screen: a new audience screen that goes nowhere, opened to be set up.
        let words = drawn(&mut state, &show, 1100.0, vec![]);
        click(&mut state, &show, place(&words, "Add a screen").center());
        assert_eq!(state.screens.len(), 5);
        assert_eq!((state.screens[4].kind, &state.screens[4].output), (Kind::Audience, &Output::Off));
        assert_eq!(state.screen_selected, 4);
        // Details and the stage screens' switch.
        let words = drawn(&mut state, &show, 1100.0, vec![]);
        let details = place(&words, "DETAILS");
        let key = words
            .iter()
            .find(|(said, rect)| said == "Off" && (rect.center().y - details.center().y).abs() < 4.0)
            .map(|(_, rect)| rect.center())
            .expect("Details' key");
        click(&mut state, &show, key);
        assert!(state.screens_details);
    }

    #[test]
    fn screens_fits_the_narrowest_settings_window() {
        use crate::i18n::Language;
        for language in [Language::English, Language::Russian, Language::Ukrainian, Language::Spanish] {
            crate::i18n::with_language(language, || {
                let show = Show::new(Vec::new());
                let mut state = state();
                state.screens_details = true;
                // The least Settings window: the page's words end 28 inside its panel.
                let right = 820.0 - crate::studio::GAP - 28.0;
                for (words, rect) in drawn(&mut state, &show, 820.0, vec![]).into_iter().filter(|(w, _)| !w.is_empty())
                {
                    assert!(rect.max.x <= right + 0.5, "{language:?}: \u{201c}{words}\u{201d} ends at {}", rect.max.x);
                }
            });
        }
    }
}
