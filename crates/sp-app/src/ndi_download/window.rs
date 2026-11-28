//! The Install NDI window: one sheet at one size over the window it was opened from, with a
//! step line at its top (1 Download · 2 Licence · 3 Install; on Windows, where NDI's own
//! installer shows the licence, 1 Download · 2 Install). What NDI is; the download with its
//! bar and Cancel; NDI's licence to accept; installing; then what was installed and the two
//! things it is for. A failure says why at the step it happened, with Try Again. Esc puts the
//! window away and what is under way carries on: the card's row shows it again.
//!
//! The design's measures (the prototype's `.ni` sheet): 460 × 435 whatever the step, so
//! nothing jumps; the keys' row keeps its height at a step with no key.

use egui::{Align2, Color32, FontId, Key, Modifiers, Rect, Sense, pos2, vec2};

use super::row::{failed_line, failed_under, installed_line, megabytes};
use super::{LICENCE_URL, NdiDownload, Place, Stage, Step, way};
use crate::i18n::{t, tf};
use crate::quick_help::Hint;
use crate::studio::{
    BUTTON, CARD, CARE, Flat, LINE, PANEL, READY, SELECTED, TEXT, TEXT_DIM, TEXT_FAINT, fixed, fixed_semibold,
    flat_button, semibold,
};

/// The sheet's size, as the design's.
pub const SIZE: egui::Vec2 = vec2(460.0, 435.0);
const SIDE: f32 = 18.0;
/// The title's row.
const HEAD: f32 = 58.0;
/// The step line: its circles, the room under them and the line.
const STEPS: f32 = 37.0;
/// The body, at every step.
const BODY: f32 = 268.0;

/// A key of the window's foot.
struct FootKey {
    words: &'static str,
    main: bool,
    asks: Asked,
    /// Greyed because something is on screen (Windows): it says so under the pointer.
    held: bool,
}

/// What a key of the window asks for, done once the drawing is over.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Asked {
    Cancel,
    Begin,
    Accept,
    Finish,
    /// Install, once a download has waited (Windows).
    Install,
}

/// Which step the line marks: none before the download (-1), each step's place, or past the
/// last when it is done.
fn step_at(stage: &Stage, steps: usize) -> isize {
    let last = steps as isize - 1;
    match stage {
        Stage::Idle | Stage::About | Stage::Removing => -1,
        Stage::Fetching => 0,
        Stage::Licence(_) => 1,
        Stage::Installing | Stage::Held(_) => last,
        Stage::Done { .. } => last + 1,
        Stage::Failed { step, .. } => match step {
            Step::Download => 0,
            Step::Install => last,
        },
    }
}

/// The steps' names: three where the app installs NDI itself, two where NDI's installer does.
fn step_names(installer: bool) -> Vec<&'static str> {
    let download = t("integrations.ndi.step.download", "Download");
    let install = t("integrations.ndi.step.install", "Install");
    if installer {
        vec![download, install]
    } else {
        vec![download, t("integrations.ndi.step.licence", "Licence"), install]
    }
}

/// The keys of a step's foot, left to right.
fn foot_keys(stage: &Stage, held: bool, installer: bool) -> Vec<FootKey> {
    let cancel =
        FootKey { words: t("integrations.ndi.key.cancel", "Cancel"), main: false, asks: Asked::Cancel, held: false };
    match stage {
        Stage::About => vec![
            cancel,
            FootKey { words: t("integrations.ndi.key.download", "Download"), main: true, asks: Asked::Begin, held },
        ],
        Stage::Fetching => vec![cancel],
        Stage::Held(_) => vec![
            cancel,
            FootKey { words: t("integrations.ndi.key.install_now", "Install"), main: true, asks: Asked::Install, held },
        ],
        Stage::Licence(_) => {
            vec![
                cancel,
                FootKey {
                    words: t("integrations.ndi.key.accept", "Accept"),
                    main: true,
                    asks: Asked::Accept,
                    held: false,
                },
            ]
        }
        Stage::Installing if installer => {
            vec![FootKey {
                words: t("integrations.ndi.key.stop", "Stop Waiting"),
                main: false,
                asks: Asked::Cancel,
                held: false,
            }]
        }
        Stage::Done { .. } => {
            vec![FootKey {
                words: t("integrations.ndi.key.done", "Done"),
                main: true,
                asks: Asked::Finish,
                held: false,
            }]
        }
        Stage::Failed { .. } => vec![
            FootKey { words: t("integrations.ndi.key.close", "Close"), main: false, asks: Asked::Cancel, held: false },
            FootKey { words: t("integrations.ndi.key.again", "Try Again"), main: true, asks: Asked::Begin, held },
        ],
        Stage::Installing | Stage::Idle | Stage::Removing => Vec::new(),
    }
}

/// The window's first step: what NDI is, in four facts.
fn facts(installer: bool) -> [String; 4] {
    let size = crate::i18n::count(way().megabytes());
    let (licence, how) = if installer {
        (
            t(
                "integrations.ndi.fact.licence.installer",
                "NDI\u{2019}s installer shows you NDI\u{2019}s licence before it installs anything.",
            ),
            t(
                "integrations.ndi.fact.how.installer",
                "NDI\u{2019}s own installer does the installing. Windows asks for an administrator\u{2019}s permission; SoftPresenter never asks for a password.",
            ),
        )
    } else {
        (
            t("integrations.ndi.fact.licence", "You accept NDI\u{2019}s licence before anything is installed."),
            t("integrations.ndi.fact.how", "SoftPresenter keeps NDI in its own folder, and can remove it again."),
        )
    };
    [
        t("integrations.ndi.fact.free", "It is free, and made by NDI, not by SoftPresenter.").to_owned(),
        tf(
            "integrations.ndi.fact.size",
            "The download is about {size} MB, from NDI\u{2019}s own server.",
            &[("size", &size)],
        ),
        licence.to_owned(),
        how.to_owned(),
    ]
}

/// The window, over the window of `ctx`, while it is shown there.
pub fn window(ctx: &egui::Context, download: &mut NdiDownload, place: Place) {
    download.poll();
    if download.shown != Some(place) {
        return;
    }
    if matches!(download.stage, Stage::Idle | Stage::Removing) {
        download.shown = None;
        return;
    }
    let screen = ctx.content_rect();
    let sheet = Rect::from_center_size(screen.center(), SIZE);
    let keys = foot_keys(&download.stage, download.held(), download.installer());
    let mut asked = None;
    let mut hide = ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape));
    if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Enter)) {
        asked = keys.iter().find(|key| key.main && !key.held).map(|key| key.asks);
    }
    let area = egui::Area::new(egui::Id::unique("install ndi"))
        .order(egui::Order::Foreground)
        .fixed_pos(screen.min)
        .constrain(false);
    area.show(ctx, |ui| {
        ui.set_clip_rect(screen);
        // The window under it takes the pointer, so nothing there is clicked.
        ui.allocate_rect(screen, Sense::click());
        let painter = ui.painter().clone();
        crate::studio::paint_sheet(&painter, screen, sheet);
        let title = if download.update {
            t("integrations.ndi.window.update", "Update NDI")
        } else {
            t("integrations.ndi.window.install", "Install NDI")
        };
        painter.text(pos2(sheet.min.x + SIDE, sheet.min.y + 29.0), Align2::LEFT_CENTER, title, semibold(15.0), TEXT);
        let cross = Rect::from_min_size(pos2(sheet.max.x - 42.0, sheet.min.y + 14.0), vec2(30.0, 30.0));
        let cross_response = ui.interact(cross, ui.scope_id().with("close install ndi"), Sense::click());
        if cross_response.hovered() {
            painter.rect_filled(cross, 7.0, CARD);
        }
        crate::icons::paint(
            &painter,
            crate::icons::Icon::Close,
            Rect::from_center_size(cross.center(), vec2(14.0, 14.0)),
            TEXT_DIM,
        );
        if cross_response.hint(t("integrations.ndi.window.close", "Close (Esc)")).clicked() {
            hide = true;
        }
        let inner = Rect::from_min_max(
            pos2(sheet.min.x + SIDE, sheet.min.y + HEAD),
            pos2(sheet.max.x - SIDE, sheet.max.y - SIDE),
        );
        steps_line(ui, inner, &download.stage, download.installer());
        let body = Rect::from_min_size(pos2(inner.min.x, inner.min.y + STEPS + 16.0), vec2(inner.width(), BODY - 16.0));
        let mut inside =
            ui.new_child(egui::UiBuilder::new().max_rect(body).layout(egui::Layout::top_down(egui::Align::Min)));
        inside.set_clip_rect(body);
        inside.spacing_mut().item_spacing = vec2(0.0, 10.0);
        step_body(&mut inside, body, download);
        // The foot: the keys at the right, their row kept where a step has none.
        let mut right = inner.max.x;
        for (index, key) in keys.iter().enumerate().rev() {
            let font = FontId::proportional(12.5);
            let wide = crate::studio::widest(ui, &[key.words], &font) + 26.0;
            let at = Rect::from_min_size(pos2(right - wide, inner.max.y - 30.0), vec2(wide, 30.0));
            let look = if key.main { Flat::Main } else { Flat::Plain };
            let id = ui.scope_id().with(("install ndi key", index));
            let pressed = flat_button(ui, at, id, key.words, font, look, !key.held);
            // Greyed, never live and refusing: the reason is under the pointer.
            let pressed = if key.held { pressed.hint(super::row::on_screen_words()) } else { pressed };
            if pressed.clicked() && !key.held {
                asked = Some(key.asks);
            }
            right = at.min.x - 8.0;
        }
    });
    let wake = {
        let ctx = ctx.clone();
        move || ctx.request_repaint()
    };
    match asked {
        Some(Asked::Cancel) => download.cancel(),
        Some(Asked::Begin) => download.begin(wake),
        Some(Asked::Accept) => download.accept(wake),
        Some(Asked::Finish) => download.finish(),
        Some(Asked::Install) => download.install_downloaded(wake),
        None if hide => download.hide(),
        None => {}
    }
    if download.busy() {
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
}

/// The step line: each step's number in a circle (a tick once it is done) and its name, the
/// one under way in ink, with a line under them all.
fn steps_line(ui: &egui::Ui, inner: Rect, stage: &Stage, installer: bool) {
    let painter = ui.painter();
    let names = step_names(installer);
    let at = step_at(stage, names.len());
    let middle = inner.min.y + 2.0 + 10.0;
    let mut x = inner.min.x;
    for (index, name) in names.iter().enumerate() {
        let index = index as isize;
        let (fill, mark, ink) = if index < at {
            (SELECTED, TEXT, TEXT_FAINT)
        } else if index == at {
            (TEXT, Color32::from_rgb(0x11, 0x11, 0x11), TEXT)
        } else {
            (BUTTON, TEXT_DIM, TEXT_FAINT)
        };
        let centre = pos2(x + 10.0, middle);
        painter.circle_filled(centre, 10.0, fill);
        if index < at {
            crate::icons::paint(
                painter,
                crate::icons::Icon::Check,
                Rect::from_center_size(centre, vec2(11.0, 11.0)),
                mark,
            );
        } else {
            painter.text(centre, Align2::CENTER_CENTER, (index + 1).to_string(), fixed_semibold(11.0), mark);
        }
        let words = painter.text(pos2(x + 27.0, middle), Align2::LEFT_CENTER, *name, FontId::proportional(12.5), ink);
        x = words.max.x + 18.0;
    }
    painter.hline(inner.x_range(), inner.min.y + STEPS - 0.5, egui::Stroke::new(1.0, LINE));
}

/// A step's first line, in the heavier face.
fn lead(ui: &mut egui::Ui, words: &str, ink: Color32) {
    ui.add(egui::Label::new(egui::RichText::new(words).font(semibold(16.0)).color(ink)).wrap());
}

/// A step's quieter line.
fn sub(ui: &mut egui::Ui, words: &str) {
    ui.add(egui::Label::new(egui::RichText::new(words).size(13.0).color(TEXT_DIM)).wrap());
}

/// A bar across the body, 6 high: `part` of the way, or moving where there is none to say.
fn bar(ui: &mut egui::Ui, part: Option<f32>) {
    let (track, _) = ui.allocate_exact_size(vec2(ui.available_width(), 6.0), Sense::hover());
    ui.painter().rect_filled(track, 3.0, BUTTON);
    let filled = match part {
        Some(part) => Rect::from_min_size(track.min, vec2(track.width() * part.clamp(0.0, 1.0), 6.0)),
        None => {
            // A third of the bar, going across once every 1.4 seconds.
            let along = (ui.input(|input| input.time) / 1.4).fract() as f32;
            let left = track.min.x + (track.width() * 1.33) * along - track.width() * 0.33;
            Rect::from_min_max(
                pos2(left.max(track.min.x), track.min.y),
                pos2((left + track.width() * 0.33).min(track.max.x), track.max.y),
            )
        }
    };
    if filled.width() > 0.0 {
        ui.painter().rect_filled(filled, 3.0, TEXT);
    }
}

/// What a step shows in the window's body.
fn step_body(ui: &mut egui::Ui, body: Rect, download: &mut NdiDownload) {
    match &download.stage {
        Stage::About => {
            lead(
                ui,
                t(
                    "integrations.ndi.about.lead",
                    "NDI sends video over your network: a screen to a live stream, or a camera into a slide.",
                ),
                TEXT,
            );
            ui.add_space(4.0);
            ui.spacing_mut().item_spacing.y = 9.0;
            for fact in facts(download.installer()) {
                let room = ui.available_width() - 18.0;
                let words = ui.painter().layout(fact, FontId::proportional(13.5), TEXT_DIM, room);
                let (place, _) = ui.allocate_exact_size(vec2(ui.available_width(), words.size().y), Sense::hover());
                // The dot sits at the middle of the fact's first line.
                ui.painter().circle_filled(pos2(place.min.x + 6.0, place.min.y + 9.0), 3.0, SELECTED);
                ui.painter().galley(pos2(place.min.x + 18.0, place.min.y), words, TEXT_DIM);
            }
            // On Windows with something on screen: why Download is greyed, in amber, at the
            // step's foot.
            if download.held() {
                let words = ui.painter().layout(
                    super::row::held_words().to_owned(),
                    FontId::proportional(13.0),
                    CARE,
                    body.width(),
                );
                ui.painter().galley(pos2(body.min.x, body.max.y - words.size().y), words, CARE);
            }
        }
        Stage::Held(_) => {
            lead(ui, super::row::downloaded_words(), TEXT);
            if download.held() {
                ui.add(egui::Label::new(egui::RichText::new(super::row::held_words()).size(13.0).color(CARE)).wrap());
            } else {
                sub(ui, super::row::install_opens_words());
            }
        }
        Stage::Fetching => {
            let shared = &download.shared;
            if shared.checking.load(std::sync::atomic::Ordering::Relaxed) {
                lead(ui, t("integrations.ndi.reading", "Checking the download\u{2026}"), TEXT);
                bar(ui, None);
                return;
            }
            let done = shared.done.load(std::sync::atomic::Ordering::Relaxed);
            let total = shared.total.load(std::sync::atomic::Ordering::Relaxed);
            let part = if total > 0 { (done as f32 / total as f32).clamp(0.0, 1.0) } else { 0.0 };
            let percent = crate::i18n::percent((part * 100.0).floor() as u64);
            lead(
                ui,
                &tf("integrations.ndi.downloading", "Downloading NDI\u{2026} {percent}", &[("percent", &percent)]),
                TEXT,
            );
            bar(ui, (total > 0).then_some(part));
            let of = if total > 0 {
                tf(
                    "integrations.ndi.of",
                    "{done} of {total} MB",
                    &[("done", &megabytes(done)), ("total", &megabytes(total))],
                )
            } else {
                tf("integrations.ndi.of.unsized", "{done} MB", &[("done", &megabytes(done))])
            };
            ui.add(egui::Label::new(egui::RichText::new(of).font(fixed(13.0)).color(TEXT_DIM)));
        }
        Stage::Licence(ready) => {
            lead(ui, t("integrations.ndi.licence.lead", "NDI\u{2019}s licence"), TEXT);
            match ready.licence() {
                Some(licence) => {
                    sub(
                        ui,
                        t(
                            "integrations.ndi.licence.own_words",
                            "These are NDI\u{2019}s own words, as NDI ships them. Nothing is installed until you accept.",
                        ),
                    );
                    licence_box(ui, body, licence, &mut download.licence_focused);
                }
                None => {
                    sub(
                        ui,
                        t(
                            "integrations.ndi.licence.by_installing",
                            "NDI is NDI\u{2019}s own software, not SoftPresenter\u{2019}s. By installing it you accept NDI\u{2019}s licence. Nothing is installed until you accept.",
                        ),
                    );
                    licence_link(ui);
                }
            }
        }
        Stage::Installing if download.installer() => {
            lead(ui, t("integrations.ndi.installer", "NDI\u{2019}s installer is open. Follow its steps."), TEXT);
            sub(
                ui,
                t(
                    "integrations.ndi.installer.about",
                    "It shows you NDI\u{2019}s licence, and Windows asks for an administrator\u{2019}s permission. SoftPresenter never asks for a password. This window goes on by itself when NDI\u{2019}s installer finishes.",
                ),
            );
        }
        Stage::Installing => {
            lead(ui, t("integrations.ndi.installing", "Installing NDI\u{2026}"), TEXT);
            bar(ui, None);
            sub(ui, t("integrations.ndi.installing.about", "This takes a few seconds."));
        }
        Stage::Done { version, waits } => done_step(ui, version.as_deref(), *waits),
        Stage::Failed { problem, .. } => {
            lead(ui, &failed_line(problem, false), CARE);
            sub(ui, failed_under(problem));
        }
        Stage::Idle | Stage::Removing => {}
    }
}

/// NDI's licence, unedited, in a box that fills the rest of the body, scrolls, and takes the
/// keyboard (the arrows, Page Up and Down, Home and End, Space).
fn licence_box(ui: &mut egui::Ui, body: Rect, licence: &str, focused: &mut bool) {
    let place = Rect::from_min_max(pos2(body.min.x, ui.cursor().min.y), body.max);
    ui.painter().rect_filled(place, 9.0, PANEL);
    let response = ui.interact(place, ui.scope_id().with("ndi licence box"), Sense::focusable_noninteractive());
    if !*focused {
        response.request_focus();
        *focused = true;
    }
    if response.has_focus() {
        ui.painter().rect_stroke(
            place.expand(2.0),
            11.0,
            egui::Stroke::new(2.0, TEXT_FAINT),
            egui::StrokeKind::Outside,
        );
    }
    let page = place.height() - 40.0;
    let moved = ui.input(|input| {
        let pressed = |key| input.key_pressed(key);
        if pressed(Key::ArrowDown) {
            40.0
        } else if pressed(Key::ArrowUp) {
            -40.0
        } else if pressed(Key::PageDown) || pressed(Key::Space) {
            page
        } else if pressed(Key::PageUp) {
            -page
        } else if pressed(Key::End) {
            f32::INFINITY
        } else if pressed(Key::Home) {
            f32::NEG_INFINITY
        } else {
            0.0
        }
    });
    let inner = place.shrink2(vec2(14.0, 12.0));
    let mut inside = ui.new_child(egui::UiBuilder::new().max_rect(inner));
    inside.set_clip_rect(place);
    let mut scroll = egui::ScrollArea::vertical().id_salt("ndi licence").auto_shrink([false, false]);
    if moved == f32::NEG_INFINITY {
        scroll = scroll.vertical_scroll_offset(0.0);
    } else if moved == f32::INFINITY {
        scroll = scroll.vertical_scroll_offset(f32::MAX / 4.0);
    }
    scroll.show(&mut inside, |ui| {
        if moved.is_finite() && moved != 0.0 {
            ui.scroll_with_delta(vec2(0.0, -moved));
        }
        ui.add(egui::Label::new(egui::RichText::new(licence).size(12.5).color(TEXT_DIM)).wrap());
    });
}

/// Where the download carries no licence text (a Mac's): a box naming NDI's licence, with a
/// key that opens NDI's own page for it in the browser.
fn licence_link(ui: &mut egui::Ui) {
    ui.add_space(6.0 - ui.spacing().item_spacing.y.min(6.0));
    let key_words = t("integrations.ndi.licence.read", "Read NDI\u{2019}s Licence \u{2197}");
    let font = FontId::proportional(12.5);
    let key_wide = crate::studio::widest(ui, &[key_words], &font) + 26.0;
    let width = ui.available_width();
    let room = (width - 28.0 - 12.0 - key_wide).max(120.0);
    let words = ui.painter().layout(
        t("integrations.ndi.licence.on_site", "NDI\u{2019}s licence is on NDI\u{2019}s own site.").to_owned(),
        FontId::proportional(13.0),
        TEXT_DIM,
        room,
    );
    let height = words.size().y.max(30.0) + 28.0;
    let (place, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    ui.painter().rect_filled(place, 9.0, PANEL);
    let words_height = words.size().y;
    ui.painter().galley(pos2(place.min.x + 14.0, place.center().y - words_height / 2.0), words, TEXT_DIM);
    let key = Rect::from_min_size(pos2(place.max.x - 14.0 - key_wide, place.center().y - 15.0), vec2(key_wide, 30.0));
    if flat_button(ui, key, ui.scope_id().with("read ndi licence"), key_words, font, Flat::Plain, true).clicked() {
        ui.ctx().open_url(egui::OpenUrl::new_tab(LICENCE_URL));
    }
}

/// The last step: a round tick, what is installed, and the two things it is for, each with
/// where to do it.
fn done_step(ui: &mut egui::Ui, version: Option<&str>, waits: bool) {
    ui.spacing_mut().item_spacing.y = 6.0;
    done_tick(ui);
    ui.add_space(6.0);
    lead(ui, &installed_line(version), TEXT);
    sub(
        ui,
        if waits {
            t("integrations.ndi.next_start", "Screens sent over NDI use it the next time SoftPresenter starts.")
        } else {
            t("integrations.ndi.ready", "It is ready to use now.")
        },
    );
    ui.add_space(10.0 - 6.0);
    ui.spacing_mut().item_spacing.y = 8.0;
    for (name, how) in [
        (
            t("integrations.ndi.for.send", "Send a screen over your network"),
            t("integrations.ndi.for.send.how", "Settings \u{203a} Screens: choose NDI for where a screen goes."),
        ),
        (
            t("integrations.ndi.for.camera", "Show a camera in a slide"),
            t("integrations.ndi.for.camera.how", "Edit: add an NDI Source box to a slide."),
        ),
    ] {
        let room = ui.available_width() - 24.0;
        let name = ui.painter().layout(name.to_owned(), semibold(13.5), TEXT, room);
        let how = ui.painter().layout(how.to_owned(), FontId::proportional(12.5), TEXT_DIM, room);
        let (place, _) = ui
            .allocate_exact_size(vec2(ui.available_width(), name.size().y + 2.0 + how.size().y + 20.0), Sense::hover());
        ui.painter().rect_filled(place, 9.0, PANEL);
        let name_height = name.size().y;
        ui.painter().galley(pos2(place.min.x + 12.0, place.min.y + 10.0), name, TEXT);
        ui.painter().galley(pos2(place.min.x + 12.0, place.min.y + 10.0 + name_height + 2.0), how, TEXT_DIM);
    }
}

/// A sheet's mark of something done: a green tick in a round of green on the sheet, 40 across
/// (the Install NDI window's last step, Request an Integration once it is sent).
pub(crate) fn done_tick(ui: &mut egui::Ui) {
    let (tick, _) = ui.allocate_exact_size(vec2(40.0, 40.0), Sense::hover());
    let ground = Color32::from_rgb(
        (f32::from(READY.r()) * 0.18 + f32::from(PANEL.r()) * 0.82) as u8,
        (f32::from(READY.g()) * 0.18 + f32::from(PANEL.g()) * 0.82) as u8,
        (f32::from(READY.b()) * 0.18 + f32::from(PANEL.b()) * 0.82) as u8,
    );
    ui.painter().circle_filled(tick.center(), 20.0, ground);
    crate::icons::paint(
        ui.painter(),
        crate::icons::Icon::Check,
        Rect::from_center_size(tick.center(), vec2(18.0, 18.0)),
        READY,
    );
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::super::check::Marks;
    use super::super::sdk::{self, Problem, fixture};
    use super::super::{Ready, Taken};
    use super::*;

    fn fresh(name: &str) -> NdiDownload {
        let data = std::env::temp_dir().join(format!("softpresenter-ndi-window-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        let mut download = NdiDownload { data: Some(data), ..NdiDownload::default() };
        download.open(Place::Settings);
        download
    }

    fn licence_stage(folder: &std::path::Path) -> Stage {
        std::fs::create_dir_all(folder).unwrap();
        let path = folder.join("sdk.tar.gz");
        std::fs::write(&path, fixture::download(&fixture::sdk_items())).unwrap();
        let contents = sdk::read(&path, "x86_64", &AtomicBool::new(false)).unwrap();
        Stage::Licence(Box::new(Ready {
            download: PathBuf::from("/nowhere"),
            taken: Taken::Sdk(contents),
            marks: Marks::default(),
        }))
    }

    /// The words the window draws, with the keys pressed first. (A sheet's first frame only
    /// measures it, so it is drawn twice: the keys go to the second.)
    fn drawn(download: &mut NdiDownload, keys: &[Key]) -> Vec<String> {
        let ctx = egui::Context::default();
        let input = |keys: &[Key]| egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1100.0, 700.0))),
            events: keys
                .iter()
                .map(|key| egui::Event::Key {
                    key: *key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                })
                .collect(),
            ..Default::default()
        };
        let mut first = ctx.run_ui(input(&[]), |ui| window(ui.ctx(), download, Place::Settings));
        first.textures_delta.clear();
        let mut output = ctx.run_ui(input(keys), |ui| window(ui.ctx(), download, Place::Settings));
        output.textures_delta.clear();
        let mut words = Vec::new();
        for clipped in &output.shapes {
            collect(&clipped.shape, &mut words);
        }
        words
    }

    fn collect(shape: &egui::Shape, into: &mut Vec<String>) {
        match shape {
            egui::Shape::Text(text) => into.push(text.galley.text().to_owned()),
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect(shape, into)),
            _ => {}
        }
    }

    fn has(words: &[String], wanted: &str) -> bool {
        words.iter().any(|said| said.contains(wanted))
    }

    #[test]
    fn the_first_step_says_what_ndi_is_and_waits_for_download() {
        crate::i18n::with_language(crate::i18n::Language::English, || {
            let mut download = fresh("about");
            let words = drawn(&mut download, &[]);
            for wanted in ["Install NDI", "NDI sends video over your network", "It is free", "Cancel", "Download"] {
                assert!(has(&words, wanted), "{wanted}: {words:?}");
            }
            assert!(matches!(download.stage, Stage::About), "nothing is downloaded until Download is pressed");
            // Esc at the first step is its Cancel: nothing was under way, and nothing is said.
            drawn(&mut download, &[Key::Escape]);
            assert!(matches!(download.stage, Stage::Idle) && download.shown.is_none() && download.note.is_none());
        });
    }

    #[test]
    fn the_download_shows_how_far_and_esc_lets_it_carry_on() {
        crate::i18n::with_language(crate::i18n::Language::English, || {
            let mut download = fresh("downloading");
            download.stage = Stage::Fetching;
            download.shared.total.store(68_000_000, Ordering::Relaxed);
            download.shared.done.store(41_480_000, Ordering::Relaxed);
            let words = drawn(&mut download, &[]);
            assert!(has(&words, "Downloading NDI\u{2026} 61%") && has(&words, "42 of 68 MB"), "{words:?}");
            assert!(has(&words, "Cancel") && !has(&words, "Accept"));
            drawn(&mut download, &[Key::Escape]);
            assert!(matches!(download.stage, Stage::Fetching), "it carries on");
            assert!(download.shown.is_none(), "with its window put away");
            assert!(!download.shared.cancel.load(Ordering::Relaxed));
            // The card's row shows the window again.
            download.open(Place::Settings);
            assert!(has(&drawn(&mut download, &[]), "Downloading NDI"));
        });
    }

    #[test]
    fn the_licence_is_ndis_own_words_and_accept_works_as_soon_as_it_shows() {
        crate::i18n::with_language(crate::i18n::Language::English, || {
            let folder = std::env::temp_dir().join(format!("softpresenter-ndi-window-licence-{}", std::process::id()));
            let mut download = fresh("licence");
            download.stage = licence_stage(&folder);
            let words = drawn(&mut download, &[]);
            assert!(has(&words, "These are NDI\u{2019}s own words"), "{words:?}");
            // The licence as it came, every word of it, not cut or reworded.
            assert!(has(&words, fixture::LICENCE.replace("\r\n", "\n").trim_end()), "{words:?}");
            assert!(has(&words, "Accept") && has(&words, "Cancel"));
            // Accept (Enter is its key) at once, with nothing scrolled: the install starts.
            drawn(&mut download, &[Key::Enter]);
            assert!(matches!(download.stage, Stage::Installing | Stage::Failed { .. }), "accepted");
            let _ = std::fs::remove_dir_all(&folder);
        });
    }

    #[test]
    fn a_download_with_no_licence_text_links_to_ndis_own_page() {
        crate::i18n::with_language(crate::i18n::Language::English, || {
            let mut download = fresh("link");
            download.stage = Stage::Licence(Box::new(Ready {
                download: PathBuf::from("/nowhere"),
                taken: Taken::Installer,
                marks: Marks::default(),
            }));
            let words = drawn(&mut download, &[]);
            for wanted in [
                "By installing it you accept NDI\u{2019}s licence",
                "NDI\u{2019}s licence is on NDI\u{2019}s own site.",
                "Read NDI\u{2019}s Licence",
                "Accept",
            ] {
                assert!(has(&words, wanted), "{wanted}: {words:?}");
            }
            assert!(LICENCE_URL.starts_with("https://ndi."));
        });
    }

    #[test]
    fn the_last_step_says_what_is_installed_and_what_it_is_for() {
        crate::i18n::with_language(crate::i18n::Language::English, || {
            let mut download = fresh("done");
            download.stage = Stage::Done { version: Some("6.1.1".to_owned()), waits: false };
            let words = drawn(&mut download, &[]);
            for wanted in [
                "NDI 6.1.1 is installed.",
                "It is ready to use now.",
                "Send a screen over your network",
                "Settings \u{203a} Screens: choose NDI for where a screen goes.",
                "Show a camera in a slide",
                "Edit: add an NDI Source box to a slide.",
                "Done",
            ] {
                assert!(has(&words, wanted), "{wanted}: {words:?}");
            }
            download.stage = Stage::Done { version: Some("6.2.0".to_owned()), waits: true };
            assert!(has(&drawn(&mut download, &[]), "use it the next time SoftPresenter starts"));
            drawn(&mut download, &[Key::Enter]);
            assert!(matches!(download.stage, Stage::Idle) && download.shown.is_none());
        });
    }

    #[test]
    fn each_failure_says_why_at_its_step_with_try_again() {
        crate::i18n::with_language(crate::i18n::Language::English, || {
            for (problem, step, wanted) in [
                (Problem::Offline, Step::Download, "there is no internet connection"),
                (Problem::Blocked, Step::Download, "Try another network, such as a phone\u{2019}s hotspot."),
                (Problem::DiskFull, Step::Download, "this computer\u{2019}s disk is full"),
                (Problem::Damaged(String::new()), Step::Download, "the download wasn\u{2019}t what NDI usually sends"),
                (Problem::Closed, Step::Install, "NDI\u{2019}s installer was closed before it finished"),
            ] {
                let mut download = fresh("failed");
                download.stage = Stage::Failed { problem, step };
                let words = drawn(&mut download, &[]);
                assert!(has(&words, wanted) && has(&words, "Try Again") && has(&words, "Close"), "{wanted}: {words:?}");
            }
            // Where it failed is the step the line marks.
            let names = 3;
            assert_eq!(step_at(&Stage::Failed { problem: Problem::Offline, step: Step::Download }, names), 0);
            assert_eq!(step_at(&Stage::Failed { problem: Problem::Closed, step: Step::Install }, 2), 1);
            assert_eq!(step_at(&Stage::About, names), -1);
            assert_eq!(step_at(&Stage::Done { version: None, waits: false }, names), 3);
        });
    }

    #[test]
    fn the_window_is_one_size_and_fits_a_small_window() {
        crate::i18n::with_language(crate::i18n::Language::English, || {
            assert_eq!(SIZE, vec2(460.0, 435.0));
            // Its parts fit in it, the keys' row included.
            const { assert!(HEAD + STEPS + BODY + 30.0 + SIDE <= SIZE.y) };
            // A step with no key keeps the keys' row: the body is as tall at every step.
            assert!(foot_keys(&Stage::Installing, false, false).len() <= 1);
        });
    }

    #[test]
    fn on_windows_with_something_on_screen_download_is_greyed_and_says_why() {
        crate::i18n::with_language(crate::i18n::Language::English, || {
            let mut download = fresh("held");
            download.as_installer = Some(true);
            download.set_on_screen(true);
            let words = drawn(&mut download, &[]);
            assert!(has(&words, "Install it when nothing is on screen."), "{words:?}");
            assert!(foot_keys(&download.stage, download.held(), true).iter().any(|key| key.main && key.held));
            // Enter, Download's key, does nothing while it is greyed.
            drawn(&mut download, &[Key::Enter]);
            assert!(matches!(download.stage, Stage::About), "nothing is downloaded");
            // The screens clear: Download works.
            download.set_on_screen(false);
            assert!(!has(&drawn(&mut download, &[]), "Install it when nothing is on screen."));
            assert!(foot_keys(&download.stage, download.held(), true).iter().all(|key| !key.held));
        });
    }
}
