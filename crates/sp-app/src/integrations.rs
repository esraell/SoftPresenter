//! Settings › Integrations: the services SoftPresenter works with, each the same card (the
//! owner, 2026-11-25: "the integration cards should be the same and that goes for voxlive"): its
//! logo, name and one line, a light with how it stands, one key. A card opens that service's
//! page in place, every page the same way: the way back, its logo and how it stands, what it
//! brings, then the account. Worship Leader App's free songbooks (where the song catalog is
//! offered, `songbooks`), VoxLive, Canva Sync, and TimelyChurch (where it is offered,
//! `timelychurch`); a service not offered yet is greyed as coming soon.

use egui::{Align, Align2, Color32, FontId, Layout, Rect, Sense, Stroke, pos2, vec2};

use crate::home::settings::parts;
use crate::i18n::{t, tf, tn};
use crate::logos::{self, Logo, Mark};
use crate::studio::{CARD, CARE, HOVER, LINE, PANEL, TEXT, TEXT_DIM, TEXT_FAINT, semibold};
use crate::voxlive::{Status, View};

/// VoxLive's own colour's lighter shade, for words on a dark ground.
pub const VOXLIVE_LIGHT: Color32 = Color32::from_rgb(127, 169, 198);

/// The green of something connected.
const GOOD: Color32 = crate::studio::READY;

/// A card under the pointer: a step lighter than the well.
const CARD_HOVER: Color32 = Color32::from_rgb(0x28, 0x28, 0x28);

/// What the Integrations pane asks of the app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    /// Take this key, and check it.
    Connect(String),
    /// Forget the key here.
    Disconnect,
    /// Find the screens again now.
    Refresh,
    /// Open a screen's look to change it, by the screen's id.
    OpenLook(String),
    /// Save the look's changes to VoxLive; `anyway` even if the screen changed there since.
    SaveLook { anyway: bool },
    /// Close the look (changes not saved are let go).
    CloseLook,
}

/// The pane: the list of services, or the page of the one open, or a screen's look.
/// `songbooks` is given only where the song catalog is offered; TimelyChurch's card is greyed
/// as coming soon unless it is offered (`timelychurch.offered`).
pub fn pane(
    ui: &mut egui::Ui,
    view: &mut View,
    canva: &mut crate::canva::View,
    mut songbooks: Option<&mut crate::songbooks::View>,
    timelychurch: &mut crate::timelychurch::View,
) {
    let songbooks_open = songbooks.as_ref().is_some_and(|songbooks| songbooks.open);
    if view.look.is_some() {
        look_page(ui, view);
    } else if view.open {
        voxlive_page(ui, view);
    } else if canva.open {
        canva_page(ui, canva);
    } else if timelychurch.offered && timelychurch.open {
        timelychurch_page(ui, timelychurch);
    } else if let (true, Some(songbooks)) = (songbooks_open, songbooks.as_deref_mut()) {
        songbooks_page(ui, songbooks);
    } else {
        list(ui, view, canva, songbooks, timelychurch);
    }
}

/// A dot and a word or two for how a service stands.
fn state_words(status: &Status) -> (Color32, String) {
    match status {
        Status::Connected => (GOOD, t("integrations.state.connected", "Connected").to_owned()),
        Status::Checking => (TEXT_FAINT, t("integrations.state.connecting", "Connecting…").to_owned()),
        Status::NoKey => (TEXT_FAINT, t("integrations.state.not_connected", "Not connected").to_owned()),
        Status::Refused(_) => (crate::studio::TALLY, t("integrations.voxlive.state.refused", "Key refused").to_owned()),
        Status::Trouble(_) => {
            (Color32::from_rgb(230, 170, 60), t("integrations.voxlive.state.trouble", "Can't reach it").to_owned())
        }
    }
}

/// A service's card in the list, the same for every service: its logo, its name and one line,
/// then a light with how it stands and one key.
struct Service<'a> {
    mark: Mark<'a>,
    name: &'a str,
    about: &'a str,
    /// How it stands: the light's colour (faint for off) and the words.
    state: (Color32, String),
    /// Its key's words: Connect, Open, or Soon for a service that isn't working yet.
    key: &'a str,
    /// Greyed, as "Coming soon", until it works.
    soon: bool,
}

/// The list: a line saying what it is, then a card for each service, the same for all of them,
/// and a card saying more will come. A card opens that service's page.
fn list(
    ui: &mut egui::Ui,
    view: &mut View,
    canva: &mut crate::canva::View,
    songbooks: Option<&mut crate::songbooks::View>,
    timelychurch: &mut crate::timelychurch::View,
) {
    let about = t("integrations.list.about", "Other services SoftPresenter works with. Every one opens the same way.");
    let width = ui.available_width();
    let line = parts::words(ui, about, FontId::proportional(13.0), TEXT_FAINT, 0.0, width);
    let (place, _) = ui.allocate_exact_size(vec2(width, line.size().y), Sense::hover());
    ui.painter().galley(place.min, line, TEXT_FAINT);
    ui.add_space(14.0 - ui.spacing().item_spacing.y);
    let connect = t("integrations.connect", "Connect");
    let open = t("integrations.open", "Open");
    let soon_key = t("integrations.soon", "Soon");
    let soon_words = || (CARE, t("integrations.state.soon", "Coming soon").to_owned());
    let songbooks_offered = songbooks.is_some();
    let songbooks_state = songbooks.as_deref().map(crate::songbooks::state_words);
    let services = [
        Service {
            mark: Mark::Logo(Logo::WorshipLeaderApp),
            name: crate::catalog::SOURCE_NAME,
            about: t("integrations.songbooks.service", "Free songbooks, one for each language"),
            state: match &songbooks_state {
                Some((have, words)) => (if *have { GOOD } else { TEXT_FAINT }, words.clone()),
                None => soon_words(),
            },
            key: if songbooks_offered { open } else { soon_key },
            soon: !songbooks_offered,
        },
        Service {
            mark: Mark::Logo(Logo::VoxLive),
            name: "VoxLive",
            about: t("integrations.voxlive.about.short", "Live translation captions on a screen"),
            state: state_words(&view.status),
            key: if view.status == Status::NoKey { connect } else { open },
            soon: false,
        },
        Service {
            mark: Mark::Logo(Logo::Canva),
            name: "Canva Sync",
            about: t("integrations.canva.about.short", "The announcements video, fresh from your Canva presentation"),
            state: canva_state(canva),
            key: if canva.status == crate::canva::Status::NotConnected { connect } else { open },
            soon: false,
        },
        Service {
            mark: Mark::Logo(Logo::TimelyChurch),
            name: crate::timelychurch::SOURCE_NAME,
            about: t("integrations.timelychurch.about.short", "Service plans and set lists from your church planner"),
            state: if timelychurch.offered { timelychurch_state(timelychurch) } else { soon_words() },
            key: if !timelychurch.offered {
                soon_key
            } else if matches!(
                timelychurch.status,
                crate::timelychurch::Status::NotConnected | crate::timelychurch::Status::Off
            ) {
                connect
            } else {
                open
            },
            soon: !timelychurch.offered,
        },
    ];
    // Two across where there is room for 240 each, one above another where there isn't.
    const BETWEEN: f32 = 12.0;
    let across = (((width + BETWEEN) / (240.0 + BETWEEN)).floor() as usize).clamp(1, 2);
    let each = (width - BETWEEN * (across - 1) as f32) / across as f32;
    let tallest = services.iter().map(|service| card_height(ui, service, each)).fold(176.0, f32::max);
    let count = services.len() + 1;
    let rows = count.div_ceil(across);
    let (grid, _) = ui.allocate_exact_size(vec2(width, rows as f32 * (tallest + BETWEEN) - BETWEEN), Sense::hover());
    let mut opened = None;
    for (index, service) in services.iter().enumerate() {
        let corner =
            grid.min + vec2((index % across) as f32 * (each + BETWEEN), (index / across) as f32 * (tallest + BETWEEN));
        if service_card(ui, Rect::from_min_size(corner, vec2(each, tallest)), service, index) {
            opened = Some(index);
        }
    }
    let index = services.len();
    let corner =
        grid.min + vec2((index % across) as f32 * (each + BETWEEN), (index / across) as f32 * (tallest + BETWEEN));
    more_to_come(ui, Rect::from_min_size(corner, vec2(each, tallest)));
    match opened {
        Some(0) => {
            if let Some(songbooks) = songbooks {
                songbooks.open = true;
            }
        }
        Some(1) => view.open = true,
        Some(2) => canva.open = true,
        Some(3) => timelychurch.open = true,
        _ => {}
    }
}

/// How tall a service's card must be for its line to fit, at `width`.
fn card_height(ui: &egui::Ui, service: &Service, width: f32) -> f32 {
    let text = width - 32.0 - 48.0 - 12.0;
    let name = parts::words(ui, service.name, semibold(16.0), TEXT, 0.0, text);
    let about = parts::words(ui, service.about, FontId::proportional(13.0), TEXT_DIM, 0.0, text);
    16.0 + (name.size().y + 3.0 + about.size().y).max(48.0) + 14.0 + 43.0 + 16.0
}

/// A service's card, drawn in `rect`: the well, lighter under the pointer; its logo, name and
/// line at the top; along its foot, under a line, its light, how it stands in the fixed-width
/// face, and its key. A service coming soon is greyed and takes no clicks. True when clicked.
fn service_card(ui: &mut egui::Ui, rect: Rect, service: &Service, index: usize) -> bool {
    let sense = if service.soon { Sense::hover() } else { Sense::click() };
    let response = ui.interact(rect, ui.scope_id().with(("service", index)), sense);
    let hovered = response.hovered() && !service.soon;
    let mut painter = ui.painter().clone();
    if service.soon {
        painter.set_opacity(0.7);
    }
    painter.rect_filled(rect, crate::studio::RADIUS_CARD, if hovered { CARD_HOVER } else { CARD });
    let inner = rect.shrink(16.0);
    let logo = Rect::from_min_size(inner.min, vec2(48.0, 48.0));
    let mut tile_ui = ui.new_child(egui::UiBuilder::new().max_rect(logo));
    tile_ui.set_opacity(if service.soon { 0.7 } else { 1.0 });
    logos::tile(&tile_ui, logo, service.mark, PANEL);
    let text = inner.width() - 48.0 - 12.0;
    let name = parts::words(ui, service.name, semibold(16.0), TEXT, 0.0, text);
    let about = parts::words(ui, service.about, FontId::proportional(13.0), TEXT_DIM, 0.0, text);
    let left = logo.max.x + 12.0;
    let name_height = name.size().y;
    painter.galley(pos2(left, inner.min.y), name, TEXT);
    painter.galley(pos2(left, inner.min.y + name_height + 3.0), about, TEXT_DIM);
    // The foot: a line, then the light, the words and the key, 30 high.
    let foot = Rect::from_min_max(pos2(inner.min.x, inner.max.y - 43.0), inner.max);
    painter.hline(foot.x_range(), foot.min.y + 0.5, Stroke::new(1.0, LINE));
    let middle = foot.max.y - 15.0;
    let (colour, words) = &service.state;
    let light = pos2(foot.min.x + 3.5, middle);
    if *colour == GOOD {
        crate::studio::paint_light(&painter, light, Some(GOOD));
    } else if *colour == TEXT_FAINT {
        crate::studio::paint_light(&painter, light, None);
    } else {
        painter.circle_filled(light, 3.5, *colour);
    }
    let key_width = crate::studio::widest(ui, &[service.key], &FontId::proportional(12.5)) + 26.0;
    let key = Rect::from_min_size(pos2(foot.max.x - key_width, middle - 15.0), vec2(key_width, 30.0));
    let room = key.min.x - 9.0 - (light.x + 3.5 + 9.0);
    let readout = parts::one_line(ui, &words.to_uppercase(), crate::studio::fixed(11.0), TEXT_DIM, room);
    painter.galley(pos2(light.x + 3.5 + 9.0, middle - readout.size().y / 2.0), readout, TEXT_DIM);
    // The key is part of the card, which takes the click.
    crate::studio::paint_flat(&painter, key, 9.0, crate::studio::Flat::Plain, hovered);
    painter.text(key.center(), Align2::CENTER_CENTER, service.key, FontId::proportional(12.5), TEXT);
    let hint = match index {
        0 => t("integrations.songbooks.hint", "Download songbooks, or remove them"),
        1 => t("integrations.voxlive.row.hint", "Connect VoxLive, or see its screens"),
        2 => t("integrations.canva.row.hint", "Connect Canva Sync, choose the presentation, or sync now"),
        _ => t("integrations.timelychurch.row.hint", "Connect TimelyChurch, or see its services"),
    };
    let hint = if service.soon { t("integrations.state.soon", "Coming soon") } else { hint };
    response.on_hover_text(hint).clicked()
}

/// The card after the services: a dashed outline saying a new service is a new card here.
fn more_to_come(ui: &egui::Ui, rect: Rect) {
    let painter = ui.painter();
    let outline = rect.shrink(0.5);
    let corners =
        [outline.left_top(), outline.right_top(), outline.right_bottom(), outline.left_bottom(), outline.left_top()];
    painter.extend(egui::Shape::dashed_line(&corners, Stroke::new(1.0, crate::studio::BUTTON_HOVER), 4.0, 3.0));
    let title =
        parts::words(ui, t("integrations.more", "More to come"), semibold(13.0), TEXT_DIM, 0.0, rect.width() - 32.0);
    let line = parts::words(
        ui,
        t("integrations.more.about", "A new service is a new card here"),
        FontId::proportional(13.0),
        TEXT_FAINT,
        0.0,
        rect.width() - 32.0,
    );
    let height = title.size().y + 4.0 + line.size().y;
    let top = rect.center().y - height / 2.0;
    let title_height = title.size().y;
    painter.galley(pos2(rect.center().x - title.size().x / 2.0, top), title, TEXT_DIM);
    painter.galley(pos2(rect.center().x - line.size().x / 2.0, top + title_height + 4.0), line, TEXT_FAINT);
}

/// The head of a service's page, the same for every service: the way back to the list, then its
/// logo, its name, and a light with how it stands and what it brings. True when the way back was
/// clicked.
fn page_head(ui: &mut egui::Ui, mark: Mark, name: &str, (colour, words): (Color32, String), what: &str) -> bool {
    use crate::icons::{Icon, paint};
    let back_words = t("integrations.back_to", "Integrations");
    let font = FontId::proportional(13.0);
    let wide = crate::studio::widest(ui, &[back_words], &font) + 8.0 + 12.0 + 6.0 + 12.0;
    let (back, response) = ui.allocate_exact_size(vec2(wide, 32.0), Sense::click());
    let ink = if response.hovered() { TEXT } else { TEXT_DIM };
    ui.painter().rect_filled(back, 16.0, CARD);
    paint(
        ui.painter(),
        Icon::ChevronLeft,
        Rect::from_center_size(pos2(back.min.x + 14.0, back.center().y), vec2(12.0, 12.0)),
        ink,
    );
    ui.painter().text(pos2(back.min.x + 26.0, back.center().y), Align2::LEFT_CENTER, back_words, font, ink);
    let clicked = response.clicked();
    ui.add_space(14.0);
    let width = ui.available_width();
    let text = width - 64.0 - 12.0;
    let title = parts::words(ui, name, semibold(22.0), TEXT, -0.44, text);
    let readout = parts::words(ui, &words.to_uppercase(), crate::studio::fixed(11.0), TEXT, 0.44, f32::INFINITY);
    let about_room = (text - 16.0 - readout.size().x - 9.0).max(80.0);
    let about = parts::words(ui, what, FontId::proportional(13.0), TEXT_DIM, 0.0, about_room);
    let height = (title.size().y + 4.0 + about.size().y.max(readout.size().y)).max(64.0);
    let (place, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let logo = Rect::from_min_size(pos2(place.min.x, place.center().y - 32.0), vec2(64.0, 64.0));
    logos::tile(ui, logo, mark, CARD);
    let left = logo.max.x + 12.0;
    let block = title.size().y + 4.0 + about.size().y.max(readout.size().y);
    let top = place.center().y - block / 2.0;
    let title_height = title.size().y;
    ui.painter().galley(pos2(left, top), title, TEXT);
    let line = top + title_height + 4.0;
    let middle = line + readout.size().y / 2.0;
    if colour == GOOD {
        crate::studio::paint_light(ui.painter(), pos2(left + 3.5, middle), Some(GOOD));
    } else if colour == TEXT_FAINT {
        crate::studio::paint_light(ui.painter(), pos2(left + 3.5, middle), None);
    } else {
        ui.painter().circle_filled(pos2(left + 3.5, middle), 3.5, colour);
    }
    let readout_width = readout.size().x;
    ui.painter().galley(pos2(left + 16.0, line), readout, TEXT);
    ui.painter().galley(pos2(left + 16.0 + readout_width + 9.0, line), about, TEXT_DIM);
    ui.add_space(16.0);
    clicked
}

/// Settings › Integrations › Worship Leader App: its songbooks, to download or remove.
fn songbooks_page(ui: &mut egui::Ui, view: &mut crate::songbooks::View) {
    let (have, words) = crate::songbooks::state_words(view);
    let what = t("integrations.songbooks.service", "Free songbooks, one for each language");
    let state = (if have { GOOD } else { TEXT_FAINT }, words);
    if page_head(ui, Mark::Logo(Logo::WorshipLeaderApp), crate::catalog::SOURCE_NAME, state, what) {
        view.open = false;
    }
    let width = ui.available_width().min(560.0);
    ui.set_max_width(width);
    crate::songbooks::settings_page(ui, view);
}

fn voxlive_page(ui: &mut egui::Ui, view: &mut View) {
    let what = t("integrations.voxlive.about.short", "Live translation captions on a screen");
    if page_head(ui, Mark::Logo(Logo::VoxLive), "VoxLive", state_words(&view.status), what) {
        view.open = false;
        view.typed.clear();
        view.problem = None;
    }
    let width = ui.available_width().min(560.0);
    ui.set_max_width(width);
    ui.label(
        egui::RichText::new(
            t("integrations.voxlive.about", "VoxLive translates the sermon as it is preached, and shows it as captions on its screens. Connected, SoftPresenter shows your church's VoxLive screens as the slides of a VoxLive item (under LIBRARY, and in playlists) and in the VoxLive tab beside the screens: one click puts the captions on screen in place of a verse, and a click on the verse brings it back."),
        )
        .color(TEXT_DIM),
    );
    ui.add_space(12.0);
    match (&view.status, &view.key_hint) {
        (Status::NoKey, _) | (_, None) => connect_form(ui, view),
        (status, Some(hint)) => {
            if let Status::Refused(why) | Status::Trouble(why) = status {
                ui.label(egui::RichText::new(why.as_str()).color(crate::studio::TALLY));
                ui.add_space(6.0);
            }
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(t("integrations.voxlive.key", "Key")).color(TEXT_DIM));
                ui.label(egui::RichText::new(hint.as_str()).monospace().color(TEXT));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .button(t("integrations.disconnect", "Disconnect"))
                        .on_hover_text(t(
                            "integrations.voxlive.disconnect.hint",
                            "Forget the key on this computer (it stays in VoxLive until you revoke it there)",
                        ))
                        .clicked()
                    {
                        ask_to_disconnect(ui, "voxlive");
                    }
                    if ui.button(t("integrations.voxlive.refresh", "Find Screens Again")).clicked() {
                        view.request = Some(Request::Refresh);
                    }
                });
            });
            if matches!(status, Status::Refused(_)) {
                ui.add_space(8.0);
                connect_form(ui, view);
            }
            ui.add_space(14.0);
            screens(ui, view);
        }
    }
    let note = t(
        "integrations.voxlive.disconnect.note",
        "The key is forgotten on this computer; it stays in VoxLive until you revoke it there.",
    );
    if disconnect_answered(ui, "voxlive", t("integrations.voxlive.disconnect.question", "Disconnect VoxLive?"), note) {
        view.request = Some(Request::Disconnect);
    }
}

/// A lighter shade of Canva's colour, for Canva Sync's words on a dark ground.
pub const CANVA_LIGHT: Color32 = Color32::from_rgb(110, 214, 220);

/// A dot and a word or two for how Canva Sync stands.
fn canva_state(view: &crate::canva::View) -> (Color32, String) {
    use crate::canva::Status as S;
    match &view.status {
        S::Reading => (TEXT_FAINT, "…".to_owned()),
        S::NotConnected => (TEXT_FAINT, t("integrations.state.not_connected", "Not connected").to_owned()),
        S::Refused(_) => (crate::studio::TALLY, t("integrations.canva.state.refused", "Sign-in refused").to_owned()),
        S::Doubtful(_) => {
            (Color32::from_rgb(230, 170, 60), t("integrations.canva.state.doubt", "Sign-in in doubt").to_owned())
        }
        S::Connected if view.phase.busy() => {
            (Color32::from_rgb(230, 170, 60), t("integrations.canva.state.syncing", "Syncing").to_owned())
        }
        S::Connected if view.settings.design.is_none() || view.settings.divider.is_none() => {
            (GOOD, t("integrations.canva.state.choose", "Connected: choose the design").to_owned())
        }
        S::Connected => (GOOD, t("integrations.state.connected", "Connected").to_owned()),
    }
}

/// Canva Sync's page: connect, choose the design and its divider page, where the video goes,
/// and Sync Now.
fn canva_page(ui: &mut egui::Ui, view: &mut crate::canva::View) {
    use crate::canva::{Request as R, Status as S};
    let what = t("integrations.canva.about.short", "The announcements video, fresh from your Canva presentation");
    if page_head(ui, Mark::Logo(Logo::Canva), "Canva Sync", canva_state(view), what) {
        view.open = false;
        view.typed_secret.clear();
    }
    let width = ui.available_width().min(560.0);
    ui.set_max_width(width);
    ui.label(
        egui::RichText::new(
            t("integrations.canva.about", "Keeps the announcements video fresh from your church's Canva presentation. The pages above a divider page are exported as a 1080p video into the library's Media › Canva Sync folder, which the media playlist Announcements follows, so the video joins the media bin by itself. Nothing happens while SoftPresenter is closed."),
        )
        .color(TEXT_DIM),
    );
    ui.add_space(8.0);
    if let Some((said, good)) = &view.said {
        let colour = if *good { CANVA_LIGHT } else { crate::studio::TALLY };
        ui.label(egui::RichText::new(said.as_str()).size(12.0).color(colour));
        ui.add_space(6.0);
    }
    let mut request = None;
    match &view.status {
        S::Reading => {
            ui.label(
                egui::RichText::new(t("integrations.canva.reading", "Reading the sign-in kept on this computer…"))
                    .color(TEXT_FAINT),
            );
        }
        S::NotConnected | S::Refused(_) => canva_connect(ui, view, &mut request),
        S::Connected | S::Doubtful(_) => {
            if let S::Doubtful(why) = &view.status {
                ui.label(egui::RichText::new(why.as_str()).color(Color32::from_rgb(230, 170, 60)));
                ui.add_space(4.0);
            }
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(t("integrations.canva.signed_in", "Signed in")).color(TEXT_DIM));
                if let Some(hint) = &view.client_hint {
                    ui.label(
                        egui::RichText::new(tf("integrations.canva.client", "integration {hint}", &[("hint", hint)]))
                            .monospace()
                            .color(TEXT_FAINT),
                    );
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .button(t("integrations.disconnect", "Disconnect"))
                        .on_hover_text(t(
                            "integrations.canva.disconnect.hint",
                            "Forget the sign-in on this computer (it stays in Canva until you take it back there)",
                        ))
                        .clicked()
                    {
                        ask_to_disconnect(ui, "canva");
                    }
                });
            });
            if view.settings.old_script_warning {
                ui.add_space(8.0);
                old_script_warning(ui, view, &mut request);
            }
            ui.add_space(12.0);
            canva_design(ui, view, &mut request);
            ui.add_space(12.0);
            crate::studio::caption(ui, t("integrations.canva.sync.caption", "SYNC"));
            ui.horizontal(|ui| {
                let ready =
                    view.settings.design.is_some() && view.settings.divider.is_some() && !view.settings.hold_syncs;
                if ui
                    .add_enabled(
                        ready && !view.phase.busy(),
                        egui::Button::new(t("integrations.canva.sync_now", "Sync Now")),
                    )
                    .clicked()
                {
                    request = Some(R::SyncNow);
                }
                if view.phase.busy() {
                    ui.label(egui::RichText::new(view.phase.words()).color(CANVA_LIGHT));
                    if ui
                        .button(t("integrations.canva.stop", "Stop"))
                        .on_hover_text(t("integrations.canva.stop.hint", "The video in the media bin stays as it was"))
                        .clicked()
                    {
                        request = Some(R::Stop);
                    }
                }
            });
            if let (Some(at), Some(file)) = (&view.settings.updated_at, &view.settings.updated_file) {
                let format = t("integrations.canva.updated.format", "%A %-d %B, %-I:%M %p");
                let when = chrono::DateTime::parse_from_rfc3339(at)
                    .map(|at| written(at.with_timezone(&chrono::Local).format(format)))
                    .unwrap_or_default();
                let said =
                    tf("integrations.canva.updated", "Last updated {when}: {file}", &[("when", &when), ("file", file)]);
                ui.label(egui::RichText::new(said).size(12.0).color(TEXT_DIM));
            }
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(
                    t("integrations.canva.macro", "To fetch a fresh video each time SoftPresenter opens, make a macro with the step Sync from Canva and tick Run When SoftPresenter Opens. A new video replaces the old one; one playing takes the new one over at the end of its pass, so the loop carries on."),
                )
                .size(11.5)
                .color(TEXT_FAINT),
            );
        }
    }
    let note = t(
        "integrations.canva.disconnect.note",
        "The sign-in is forgotten on this computer; it stays in Canva until you take it back there.",
    );
    if disconnect_answered(ui, "canva", t("integrations.canva.disconnect.question", "Disconnect Canva Sync?"), note) {
        request = Some(R::Disconnect);
    }
    if request.is_some() {
        view.request = request;
    }
}

/// A lighter shade of TimelyChurch's colour, for its words on a dark ground.
pub const TIMELYCHURCH_LIGHT: Color32 = Color32::from_rgb(164, 162, 240);

/// A dot and a word or two for how TimelyChurch stands.
fn timelychurch_state(view: &crate::timelychurch::View) -> (Color32, String) {
    use crate::timelychurch::Status as S;
    match &view.status {
        S::Reading => (TEXT_FAINT, "…".to_owned()),
        S::Off | S::NotConnected => (TEXT_FAINT, t("integrations.state.not_connected", "Not connected").to_owned()),
        S::Pairing => (TEXT_FAINT, t("integrations.state.connecting", "Connecting…").to_owned()),
        S::Disconnected => {
            (crate::studio::TALLY, t("integrations.timelychurch.state.disconnected", "Disconnected").to_owned())
        }
        S::Connected if view.trouble.is_some() => {
            (Color32::from_rgb(230, 170, 60), t("integrations.timelychurch.state.trouble", "Can't reach it").to_owned())
        }
        S::Connected => (GOOD, t("integrations.state.connected", "Connected").to_owned()),
    }
}

/// When TimelyChurch was last checked, as the card says it: the time today, else the day too.
fn checked_words(at: i64) -> String {
    let Some(at) = chrono::DateTime::from_timestamp(at, 0).map(|at| at.with_timezone(&chrono::Local)) else {
        return String::new();
    };
    let format = if at.date_naive() == chrono::Local::now().date_naive() {
        t("integrations.timelychurch.checked.today_format", "%-I:%M %p")
    } else {
        t("integrations.timelychurch.checked.day_format", "%-d %b, %-I:%M %p")
    };
    written(at.format(format))
}

/// A service's date as the card lists it.
fn service_date(date: &str) -> String {
    match chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d") {
        Ok(day) => written(day.format(t("integrations.timelychurch.date_format", "%a %-d %b"))),
        Err(_) => date.to_owned(),
    }
}

/// Settings › Integrations › TimelyChurch: Connect to TimelyChurch (the church's address and a
/// pairing code); once connected, the church, when it was last checked, Check Now, Disconnect,
/// and the playlists made from its services.
fn timelychurch_page(ui: &mut egui::Ui, view: &mut crate::timelychurch::View) {
    use crate::timelychurch::{Request as R, Status as S};
    let what = t("integrations.timelychurch.about.short", "Service plans and set lists from your church planner");
    let name = crate::timelychurch::SOURCE_NAME;
    if page_head(ui, Mark::Logo(Logo::TimelyChurch), name, timelychurch_state(view), what) {
        view.open = false;
        view.typed_code.clear();
        view.problem = None;
    }
    let width = ui.available_width().min(560.0);
    ui.set_max_width(width);
    ui.label(
        egui::RichText::new(
            t("integrations.timelychurch.about", "Your church's upcoming setlists and service plans in TimelyChurch become playlists here, and their songs join the library with their translations, kept up to date during the week. A song or playlist you change here is never overwritten: TimelyChurch's newer version is only offered. SoftPresenter tells TimelyChurch which songs were shown in each service, for your church's song use report."),
        )
        .color(TEXT_DIM),
    );
    ui.add_space(12.0);
    let mut request = None;
    match &view.status {
        S::Off => {}
        S::Reading => {
            ui.label(
                egui::RichText::new(t(
                    "integrations.timelychurch.reading",
                    "Reading the connection kept on this computer…",
                ))
                .color(TEXT_FAINT),
            );
        }
        S::NotConnected | S::Pairing | S::Disconnected => {
            // Disconnected in TimelyChurch: said as TimelyChurch asked, in the language set now.
            let said = match &view.status {
                S::Disconnected => Some(crate::timelychurch::disconnected_words()),
                _ => view.trouble.clone(),
            };
            if let Some(said) = said {
                ui.label(egui::RichText::new(said).color(crate::studio::TALLY));
                ui.add_space(6.0);
            }
            timelychurch_connect(ui, view, &mut request);
        }
        S::Connected => timelychurch_connected(ui, view, &mut request),
    }
    let note = t(
        "integrations.timelychurch.disconnect.note",
        "This computer stops following TimelyChurch and forgets its connection. The songs and playlists it made stay as they are.",
    );
    let question = t("integrations.timelychurch.disconnect.question", "Disconnect TimelyChurch?");
    if disconnect_answered(ui, "timelychurch", question, note) {
        request = Some(R::Disconnect);
    }
    if request.is_some() {
        view.request = request;
    }
}

/// Connect to TimelyChurch: the church's address (unless TimelyChurch has one address for
/// every church) and the pairing code an administrator made.
fn timelychurch_connect(
    ui: &mut egui::Ui,
    view: &mut crate::timelychurch::View,
    request: &mut Option<crate::timelychurch::Request>,
) {
    crate::studio::caption(ui, t("integrations.timelychurch.connect.caption", "CONNECT TO TIMELYCHURCH"));
    ui.label(
        egui::RichText::new(
            t("integrations.timelychurch.connect.about", "In TimelyChurch, an administrator opens Setlists › Song Sync and makes a pairing code. Type your church's TimelyChurch address and the code here. The connection is kept in this computer's secure store and used only with your church's TimelyChurch."),
        )
        .size(12.0)
        .color(TEXT_DIM),
    );
    ui.add_space(6.0);
    let pairing = view.status == crate::timelychurch::Status::Pairing;
    let mut connect = false;
    egui::Grid::new("timelychurch connect").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
        if view.asks_address {
            ui.label(egui::RichText::new(t("integrations.timelychurch.address", "Address")).color(TEXT_DIM));
            ui.add(
                egui::TextEdit::singleline(&mut view.typed_address)
                    .hint_text("yourchurch.timelychurch.com")
                    .desired_width(300.0),
            );
            ui.end_row();
        }
        ui.label(egui::RichText::new(t("integrations.timelychurch.code", "Pairing code")).color(TEXT_DIM));
        let field = ui.add(egui::TextEdit::singleline(&mut view.typed_code).desired_width(300.0));
        connect |= field.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
        ui.end_row();
    });
    ui.add_space(4.0);
    let filled = !view.typed_code.trim().is_empty() && (!view.asks_address || !view.typed_address.trim().is_empty());
    ui.horizontal(|ui| {
        connect |= ui
            .add_enabled(filled && !pairing, egui::Button::new(t("integrations.timelychurch.connect", "Connect")))
            .clicked();
        if pairing {
            ui.label(egui::RichText::new(t("integrations.state.connecting", "Connecting…")).color(TIMELYCHURCH_LIGHT));
        }
    });
    if connect && filled && !pairing {
        *request = Some(crate::timelychurch::Request::Connect {
            address: view.typed_address.trim().to_owned(),
            code: std::mem::take(&mut view.typed_code),
        });
    }
    if let Some(problem) = &view.problem {
        ui.label(egui::RichText::new(problem.as_str()).size(12.0).color(crate::studio::TALLY));
    }
}

/// Connected: the church and when it was last checked, Check Now and Disconnect, and the
/// playlists made from its services.
fn timelychurch_connected(
    ui: &mut egui::Ui,
    view: &crate::timelychurch::View,
    request: &mut Option<crate::timelychurch::Request>,
) {
    use crate::timelychurch::RowState;
    if let Some(trouble) = &view.trouble {
        ui.label(egui::RichText::new(trouble.as_str()).color(Color32::from_rgb(230, 170, 60)));
        ui.add_space(6.0);
    }
    let when = if view.checking {
        t("integrations.timelychurch.checking", "checking now").to_owned()
    } else {
        match view.last_checked {
            Some(at) => tf("integrations.timelychurch.checked", "last checked {when}", &[("when", &checked_words(at))]),
            None => t("integrations.timelychurch.not_checked", "not checked yet").to_owned(),
        }
    };
    let said = tf(
        "integrations.timelychurch.connected_to",
        "Connected to {church} · {when}",
        &[("church", &view.church), ("when", &when)],
    );
    // The church's name and the time can be long in any language: the buttons go under them.
    ui.label(egui::RichText::new(said).color(TEXT));
    ui.label(egui::RichText::new(view.address.as_str()).size(11.5).color(TEXT_FAINT));
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        if ui
            .add_enabled(!view.checking, egui::Button::new(t("integrations.timelychurch.check_now", "Check Now")))
            .on_hover_text(t(
                "integrations.timelychurch.check_now.hint",
                "Ask TimelyChurch for changes now (it is asked every few minutes anyway)",
            ))
            .clicked()
        {
            *request = Some(crate::timelychurch::Request::CheckNow);
        }
        if ui
            .button(t("integrations.disconnect", "Disconnect"))
            .on_hover_text(t(
                "integrations.timelychurch.disconnect.hint",
                "Forget the connection on this computer; the songs and playlists it made stay",
            ))
            .clicked()
        {
            ask_to_disconnect(ui, "timelychurch");
        }
    });
    if view.uses_waiting > 0 {
        let waiting = tn(
            "integrations.timelychurch.uses_waiting",
            view.uses_waiting as u64,
            "{n} song shown is waiting to be told to TimelyChurch.",
            "{n} songs shown are waiting to be told to TimelyChurch.",
            &[],
        );
        ui.label(egui::RichText::new(waiting).size(11.5).color(TEXT_FAINT));
    }
    ui.add_space(14.0);
    crate::studio::caption(ui, t("integrations.timelychurch.services.caption", "UPCOMING SERVICES"));
    if view.services.is_empty() {
        let words = if view.checking || view.last_checked.is_none() {
            t("integrations.timelychurch.services.coming", "The services come in at the first check.")
        } else {
            t("integrations.timelychurch.services.none", "No upcoming services in TimelyChurch.")
        };
        ui.label(egui::RichText::new(words).color(TEXT_FAINT));
        return;
    }
    egui::Grid::new("timelychurch services").num_columns(4).spacing([14.0, 6.0]).show(ui, |ui| {
        for row in &view.services {
            let date = row.date.as_deref().map(service_date).unwrap_or_default();
            ui.label(egui::RichText::new(date).size(12.0).color(TEXT_DIM));
            ui.label(egui::RichText::new(row.name.as_str()).color(TEXT));
            let songs = tn("integrations.timelychurch.songs", row.songs as u64, "{n} song", "{n} songs", &[]);
            ui.label(egui::RichText::new(songs).size(12.0).color(TEXT_DIM));
            let (colour, state) = match row.state {
                RowState::InStep => (TEXT_FAINT, t("integrations.timelychurch.row.in_step", "Up to date")),
                RowState::ChangedHere => (TEXT_DIM, t("integrations.timelychurch.row.changed", "Changed here")),
                RowState::NewerWaiting => (
                    TIMELYCHURCH_LIGHT,
                    t("integrations.timelychurch.row.newer", "Changed here; newer in TimelyChurch"),
                ),
                RowState::Removed => (
                    Color32::from_rgb(230, 170, 60),
                    t("integrations.timelychurch.row.removed", "Removed in TimelyChurch"),
                ),
            };
            ui.label(egui::RichText::new(state).size(12.0).color(colour));
            ui.end_row();
        }
    });
}

/// Where a page keeps that its Disconnect asked, while the question is open.
fn disconnecting_id(service: &str) -> egui::Id {
    egui::Id::unique(("disconnecting", service))
}

/// Disconnect was clicked: Undo can't connect again, so it asks first (see
/// [`disconnect_answered`]).
fn ask_to_disconnect(ui: &egui::Ui, service: &str) {
    ui.data_mut(|data| data.insert_temp(disconnecting_id(service), true));
}

/// The question Disconnect asks, while it is open: true once it is answered Disconnect.
fn disconnect_answered(ui: &egui::Ui, service: &str, question: &str, note: &str) -> bool {
    if !ui.data(|data| data.get_temp::<bool>(disconnecting_id(service))).unwrap_or(false) {
        return false;
    }
    let note = crate::home::cant_undo(note);
    let icon = crate::icons::Icon::Close;
    let Some(answer) =
        crate::studio::alert(ui.ctx(), service, icon, question, &note, t("integrations.disconnect", "Disconnect"))
    else {
        return false;
    };
    ui.data_mut(|data| data.remove::<bool>(disconnecting_id(service)));
    answer
}

/// How to connect: the integration's ID and secret and Sign In, or the old script's sign-in.
fn canva_connect(ui: &mut egui::Ui, view: &mut crate::canva::View, request: &mut Option<crate::canva::Request>) {
    use crate::canva::{Phase, Request as R};
    if let crate::canva::Status::Refused(why) = &view.status {
        ui.label(egui::RichText::new(why.as_str()).color(crate::studio::TALLY));
        ui.add_space(6.0);
    }
    crate::studio::caption(ui, t("integrations.connect.caption", "CONNECT"));
    ui.label(
        egui::RichText::new(
            if cfg!(target_os = "macos") {
                t("integrations.canva.connect.about", "Make an integration of your church's own in Canva's developer portal (canva.com/developers, Your integrations). Under Scopes tick design:content:read; under Authentication add the redirect URL below; then copy its client ID and make a secret. The secret and the sign-in are kept in this Mac's Keychain and sent only to Canva.")
            } else if cfg!(windows) {
                t("integrations.canva.connect.about.windows", "Make an integration of your church's own in Canva's developer portal (canva.com/developers, Your integrations). Under Scopes tick design:content:read; under Authentication add the redirect URL below; then copy its client ID and make a secret. The secret and the sign-in are kept in Windows' Credential Manager on this computer and sent only to Canva.")
            } else {
                t("integrations.canva.connect.about.linux", "Make an integration of your church's own in Canva's developer portal (canva.com/developers, Your integrations). Under Scopes tick design:content:read; under Authentication add the redirect URL below; then copy its client ID and make a secret. The secret and the sign-in are kept in this computer's keyring and sent only to Canva.")
            },
        )
        .size(12.0)
        .color(TEXT_DIM),
    );
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(view.redirect.as_str()).monospace().color(TEXT));
        if ui.small_button(t("integrations.copy", "Copy")).clicked() {
            ui.ctx().copy_text(view.redirect.clone());
        }
    });
    ui.add_space(6.0);
    egui::Grid::new("canva connect").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
        ui.label(egui::RichText::new(t("integrations.canva.client_id", "Client ID")).color(TEXT_DIM));
        ui.add(egui::TextEdit::singleline(&mut view.typed_id).desired_width(300.0));
        ui.end_row();
        ui.label(egui::RichText::new(t("integrations.canva.client_secret", "Client secret")).color(TEXT_DIM));
        ui.add(egui::TextEdit::singleline(&mut view.typed_secret).password(true).desired_width(300.0));
        ui.end_row();
    });
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        if view.phase == Phase::SigningIn {
            ui.label(egui::RichText::new(view.phase.words()).color(CANVA_LIGHT));
            if ui.button(t("integrations.canva.stop", "Stop")).clicked() {
                *request = Some(R::Stop);
            }
        } else {
            let filled = !view.typed_id.trim().is_empty() && !view.typed_secret.trim().is_empty();
            if ui
                .add_enabled(
                    filled && !view.phase.busy(),
                    egui::Button::new(t("integrations.canva.sign_in", "Sign In with Canva")),
                )
                .on_hover_text(t("integrations.canva.sign_in.hint", "Opens Canva's sign-in in your browser"))
                .clicked()
            {
                *request = Some(R::SignIn(view.typed_id.trim().to_owned(), std::mem::take(&mut view.typed_secret)));
            }
        }
    });
    if view.old_here {
        ui.add_space(12.0);
        crate::studio::caption(ui, t("integrations.canva.old.caption", "THE OLD SYNC SCRIPT"));
        ui.label(
            egui::RichText::new(
                t("integrations.canva.old.about", "This Mac has the sign-in of the old Canva sync script. SoftPresenter can take it over: it reads it from there itself, keeps it in the Keychain, and takes the script's token file away. Then switch the old script off."),
            )
            .size(12.0)
            .color(TEXT_DIM),
        );
        if ui
            .add_enabled(
                !view.phase.busy(),
                egui::Button::new(t("integrations.canva.old.use", "Use the Sign-in from the Old Sync Script")),
            )
            .clicked()
        {
            *request = Some(R::Import);
        }
    }
}

/// The old script must go now that its sign-in is here: what to type to switch it off.
fn old_script_warning(ui: &mut egui::Ui, view: &crate::canva::View, request: &mut Option<crate::canva::Request>) {
    egui::Frame::new().fill(Color32::from_rgb(60, 32, 30)).corner_radius(8.0).inner_margin(10.0).show(ui, |ui| {
        let words = if view.settings.hold_syncs {
            t("integrations.canva.old.warning.held", "Switch the old sync script off now. It and SoftPresenter can't both use the sign-in: whichever refreshes it next breaks the other's. Its token file couldn't be taken away, so it could still refresh it: nothing is synced until you click It's Switched Off. In Terminal on this Mac:")
        } else {
            t("integrations.canva.old.warning", "Switch the old sync script off now. It and SoftPresenter can't both use the sign-in: whichever refreshes it next breaks the other's. Its token file is gone, so it can't refresh any more, but it should not be left running. In Terminal on this Mac:")
        };
        ui.label(egui::RichText::new(words).color(TEXT));
        ui.add_space(4.0);
        for step in &view.old_steps {
            ui.label(egui::RichText::new(step.as_str()).monospace().size(12.0).color(TEXT));
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button(t("integrations.canva.old.copy", "Copy the Steps")).clicked() {
                ui.ctx().copy_text(view.old_steps.join("\n"));
            }
            if ui.button(t("integrations.canva.old.off", "It's Switched Off")).clicked() {
                *request = Some(crate::canva::Request::OldScriptOff);
            }
        });
    });
}

/// The design: its link, its pages to choose the divider from, and where the video goes.
fn canva_design(ui: &mut egui::Ui, view: &mut crate::canva::View, request: &mut Option<crate::canva::Request>) {
    use crate::canva::{Phase, Request as R};
    crate::studio::caption(ui, t("integrations.canva.design.caption", "THE PRESENTATION"));
    ui.label(
        egui::RichText::new(t(
            "integrations.canva.design.about",
            "Paste the presentation's link (in Canva, Share › Copy link), then choose its divider page.",
        ))
        .size(12.0)
        .color(TEXT_DIM),
    );
    ui.horizontal(|ui| {
        let hint = view.settings.design.clone().unwrap_or_else(|| "https://www.canva.com/design/…".to_owned());
        let field = ui.add(egui::TextEdit::singleline(&mut view.typed_link).hint_text(hint).desired_width(300.0));
        let entered = field.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
        let busy = view.phase.busy();
        if !view.typed_link.trim().is_empty()
            && (entered
                || ui.add_enabled(!busy, egui::Button::new(t("integrations.canva.read", "Read Pages"))).clicked())
        {
            *request = Some(R::Design(std::mem::take(&mut view.typed_link)));
        } else if view.typed_link.trim().is_empty()
            && view.settings.design.is_some()
            && ui
                .add_enabled(!busy, egui::Button::new(t("integrations.canva.read_again", "Read Pages Again")))
                .clicked()
        {
            *request = Some(R::ReadPages);
        }
    });
    if view.phase == Phase::ReadingPages {
        ui.label(egui::RichText::new(view.phase.words()).size(12.0).color(CANVA_LIGHT));
    }
    match (view.settings.divider_number, view.settings.divider.is_some()) {
        (Some(number), true) if number > 1 => {
            let (page, last) = (number.to_string(), (number - 1).to_string());
            let said = if number == 2 {
                tf(
                    "integrations.canva.divider.one",
                    "Divider: page {number}. Page 1 goes in the video; drag old slides below it in Canva.",
                    &[("number", &page)],
                )
            } else {
                tf(
                    "integrations.canva.divider.many",
                    "Divider: page {number}. Pages 1–{last} go in the video; drag old slides below it in Canva.",
                    &[("number", &page), ("last", &last)],
                )
            };
            ui.label(egui::RichText::new(said).color(TEXT));
        }
        _ if view.settings.design.is_some() => {
            ui.label(
                egui::RichText::new(t(
                    "integrations.canva.divider.none",
                    "No divider page chosen yet: read the pages and click it.",
                ))
                .color(TEXT_FAINT),
            );
        }
        _ => {}
    }
    if !view.pages.is_empty() {
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(t("integrations.canva.divider.about", "Click the divider page. Canva can't tell SoftPresenter which slides are hidden, so the divider says where the video stops."))
                .size(11.5)
                .color(TEXT_FAINT),
        );
        ui.add_space(4.0);
        let mut chosen = None;
        ui.horizontal_wrapped(|ui| {
            for page in &view.pages {
                let Some(id) = page.id.clone() else { continue };
                let size = vec2(128.0, 72.0);
                let (place, response) = ui.allocate_exact_size(size + vec2(0.0, 16.0), Sense::click());
                let picture = Rect::from_min_size(place.min, size);
                ui.painter().rect_filled(picture, 4.0, Color32::from_rgb(20, 22, 26));
                if let Some(frame) = &page.picture {
                    let texture = match view.textures.iter().find(|(have, _)| *have == id) {
                        Some((_, texture)) => texture.clone(),
                        None => {
                            let image = egui::ColorImage::from_rgba_unmultiplied(
                                [frame.width as usize, frame.height as usize],
                                &frame.rgba,
                            );
                            let texture =
                                ui.ctx().load_texture(format!("canva page {id}"), image, egui::TextureOptions::LINEAR);
                            view.textures.push((id.clone(), texture.clone()));
                            texture
                        }
                    };
                    let aspect = frame.width as f32 / frame.height.max(1) as f32;
                    let fitted = if aspect > size.x / size.y {
                        vec2(size.x, size.x / aspect)
                    } else {
                        vec2(size.y * aspect, size.y)
                    };
                    egui::Image::new((texture.id(), fitted))
                        .corner_radius(4.0)
                        .paint_at(ui, Rect::from_center_size(picture.center(), fitted));
                }
                let divider = view.settings.divider.as_deref() == Some(id.as_str());
                let below = view.settings.divider_number.is_some_and(|number| page.number > number);
                let outline = if divider {
                    Stroke::new(2.5, crate::studio::TALLY)
                } else if response.hovered() {
                    Stroke::new(1.5, TEXT_DIM)
                } else {
                    Stroke::new(1.0, LINE)
                };
                ui.painter().rect_stroke(picture, 4.0, outline, egui::StrokeKind::Outside);
                if below {
                    ui.painter().rect_filled(picture, 4.0, Color32::from_black_alpha(140));
                }
                let label = if divider {
                    tf("integrations.canva.page.divider", "{number} · divider", &[("number", &page.number.to_string())])
                } else {
                    page.number.to_string()
                };
                ui.painter().text(
                    pos2(picture.center().x, picture.max.y + 8.0),
                    Align2::CENTER_CENTER,
                    label,
                    FontId::proportional(11.0),
                    if divider { crate::studio::TALLY } else { TEXT_DIM },
                );
                if response
                    .on_hover_text(tf(
                        "integrations.canva.page.hint",
                        "Make page {number} the divider",
                        &[("number", &page.number.to_string())],
                    ))
                    .clicked()
                {
                    chosen = Some(id);
                }
            }
        });
        if let Some(id) = chosen {
            *request = Some(R::Divider(id));
        }
    }
    ui.add_space(12.0);
    crate::studio::caption(ui, t("integrations.canva.where.caption", "WHERE IT GOES"));
    ui.label(
        egui::RichText::new(
            t("integrations.canva.where.about", "Media › Canva Sync in the library, followed by the media playlist Announcements (made when the divider is chosen). One video is kept there, named with its day and time so volunteers can read it."),
        )
        .size(12.0)
        .color(TEXT_DIM),
    );
}

/// Where the key is pasted, and how to get one.
fn connect_form(ui: &mut egui::Ui, view: &mut View) {
    crate::studio::caption(ui, t("integrations.connect.caption", "CONNECT"));
    ui.label(
        egui::RichText::new(
            if cfg!(target_os = "macos") {
                t("integrations.voxlive.connect.about", "In VoxLive, open Settings › API keys and make a key with the screens permissions (read and write). Paste it here. It is kept in this Mac's Keychain and sent only to VoxLive.")
            } else if cfg!(windows) {
                t("integrations.voxlive.connect.about.windows", "In VoxLive, open Settings › API keys and make a key with the screens permissions (read and write). Paste it here. It is kept in Windows' Credential Manager on this computer and sent only to VoxLive.")
            } else {
                t("integrations.voxlive.connect.about.linux", "In VoxLive, open Settings › API keys and make a key with the screens permissions (read and write). Paste it here. It is kept in this computer's keyring and sent only to VoxLive.")
            },
        )
        .size(12.0)
        .color(TEXT_DIM),
    );
    ui.add_space(6.0);
    let mut connect = false;
    ui.horizontal(|ui| {
        let field = ui.add(
            egui::TextEdit::singleline(&mut view.typed)
                .password(true)
                .hint_text("vxk_live_…")
                .desired_width(ui.available_width() - 100.0),
        );
        connect |= field.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
        connect |= ui
            .add_enabled(!view.typed.trim().is_empty(), egui::Button::new(t("integrations.voxlive.connect", "Connect")))
            .clicked();
    });
    if connect && !view.typed.trim().is_empty() {
        view.request = Some(Request::Connect(std::mem::take(&mut view.typed)));
    }
    if let Some(problem) = &view.problem {
        ui.label(egui::RichText::new(problem.as_str()).size(12.0).color(crate::studio::TALLY));
    }
}

/// The church's VoxLive screens, as last found.
fn screens(ui: &mut egui::Ui, view: &mut View) {
    crate::studio::caption(ui, t("integrations.voxlive.screens.caption", "YOUR VOXLIVE SCREENS"));
    if view.screens.is_empty() {
        let words = match view.status {
            Status::Connected => t(
                "integrations.voxlive.screens.none",
                "No screens yet. Make one in VoxLive (Screens), then Find Screens Again.",
            ),
            _ => t("integrations.voxlive.screens.not_yet", "Not found yet."),
        };
        ui.label(egui::RichText::new(words).size(12.0).color(TEXT_FAINT));
        return;
    }
    let mut open = None;
    for screen in &view.screens {
        // A screen with a page can have its look changed: a click opens it.
        let can_open = !screen.url.is_empty() && !view.schema.is_empty();
        let sense = if can_open { Sense::click() } else { Sense::hover() };
        let (row, response) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), sense);
        if response.hovered() && can_open {
            ui.painter().rect_filled(row, 5.0, HOVER);
        }
        ui.painter().text(
            row.left_center() + vec2(4.0, 0.0),
            Align2::LEFT_CENTER,
            screen.label(),
            FontId::proportional(13.0),
            TEXT,
        );
        let kind = match screen.kind() {
            "captions" => t("integrations.voxlive.kind.captions", "Captions"),
            "Scripture" => t("integrations.voxlive.kind.scripture", "Scripture"),
            other => other,
        };
        let kind = if can_open {
            tf("integrations.voxlive.change_look", "{kind}   Change Look ›", &[("kind", kind)])
        } else {
            kind.to_owned()
        };
        ui.painter().text(
            row.right_center() - vec2(4.0, 0.0),
            Align2::RIGHT_CENTER,
            kind,
            FontId::proportional(11.5),
            VOXLIVE_LIGHT,
        );
        ui.painter().hline(row.x_range(), row.max.y, Stroke::new(1.0, LINE));
        if response
            .on_hover_text(t(
                "integrations.voxlive.change_look.hint",
                "Change how this screen looks, here; it is saved to VoxLive",
            ))
            .clicked()
        {
            open = Some(screen.id.clone());
        }
    }
    if let Some(screen) = open {
        view.request = Some(Request::OpenLook(screen));
    }
    ui.add_space(6.0);
    ui.label(
        egui::RichText::new(
            t("integrations.voxlive.screens.about", "A screen's live display is opened only on your real screens, where it shows the captions (and can start that language's translation); previews use its free test pattern."),
        )
        .size(11.5)
        .color(TEXT_FAINT),
    );
}

/// A screen's look: a preview, a control for each setting of its mode, and Save to VoxLive.
fn look_page(ui: &mut egui::Ui, view: &mut View) {
    let schema = view.schema.clone();
    let Some(look) = view.look.as_mut() else { return };
    let mut request = None;
    if ui.add(egui::Button::new(egui::RichText::new("‹ VoxLive").color(TEXT_DIM)).frame(false)).clicked() {
        request = Some(Request::CloseLook);
    }
    ui.add_space(4.0);
    ui.label(egui::RichText::new(look.label.as_str()).font(crate::studio::semibold(18.0)).color(TEXT));
    ui.add_space(8.0);
    // The preview: VoxLive's own display page drawing this look, on a dark ground (a see-through
    // look shows it), never connected to the captions.
    if look.fresh
        && let Some(frame) = &look.preview
    {
        let image =
            egui::ColorImage::from_rgba_unmultiplied([frame.width as usize, frame.height as usize], &frame.rgba);
        match look.texture.as_mut() {
            Some(texture) => texture.set(image, egui::TextureOptions::LINEAR),
            None => look.texture = Some(ui.ctx().load_texture("voxlive look", image, egui::TextureOptions::LINEAR)),
        }
        look.fresh = false;
    }
    let width = ui.available_width().min(480.0);
    let (place, _) = ui.allocate_exact_size(vec2(width, width * 9.0 / 16.0), Sense::hover());
    ui.painter().rect_filled(place, 6.0, Color32::from_rgb(20, 22, 26));
    match &look.texture {
        Some(texture) => {
            egui::Image::new((texture.id(), place.size())).corner_radius(6.0).paint_at(ui, place);
        }
        None => {
            ui.painter().text(
                place.center(),
                Align2::CENTER_CENTER,
                t("integrations.look.loading", "Preview loading…"),
                FontId::proportional(12.0),
                TEXT_FAINT,
            );
            // The window is drawn again when the preview's first picture comes.
        }
    }
    ui.painter().rect_stroke(place, 6.0, Stroke::new(1.0, LINE), egui::StrokeKind::Outside);
    ui.label(
        egui::RichText::new(t(
            "integrations.look.preview",
            "Preview, with sample words. Nothing changes on your screens until you save.",
        ))
        .size(11.5)
        .color(TEXT_FAINT),
    );
    ui.add_space(10.0);
    // A control for each setting that applies to the screen's mode.
    let mode = look.style.get("mode").and_then(serde_json::Value::as_str).unwrap_or("captions").to_owned();
    egui::Grid::new("voxlive look").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
        for (key, field) in &schema {
            let applies = field.get("applies_to").and_then(serde_json::Value::as_str).unwrap_or("both");
            if !(applies == "both" || applies == mode) {
                continue;
            }
            let label = ui.label(egui::RichText::new(setting_name(key)).color(TEXT_DIM));
            if let Some(about) = field.get("description").and_then(serde_json::Value::as_str) {
                label.on_hover_text(about);
            }
            let current = look.style.get(key).cloned().or_else(|| field.get("default").cloned()).unwrap_or_default();
            if let Some(changed) = setting_control(ui, key, field, &current)
                && changed != current
            {
                look.style.insert(key.clone(), changed);
            }
            ui.end_row();
        }
    });
    ui.add_space(12.0);
    let changes = look.changes().len();
    ui.horizontal(|ui| {
        let save = egui::Button::new(if look.saving {
            t("integrations.look.saving", "Saving…")
        } else {
            t("integrations.look.save", "Save to VoxLive")
        });
        if ui
            .add_enabled(changes > 0 && !look.saving, save)
            .on_hover_text(t("integrations.look.save.hint", "The screen changes at once, wherever it is showing"))
            .clicked()
        {
            request = Some(Request::SaveLook { anyway: false });
        }
        if ui
            .add_enabled(changes > 0 && !look.saving, egui::Button::new(t("integrations.look.revert", "Revert")))
            .clicked()
        {
            look.style = look.base.clone();
            look.outcome = None;
        }
        let words = match changes {
            0 => t("integrations.look.no_changes", "No changes").to_owned(),
            n => tn("integrations.look.changed", n as u64, "{n} setting changed", "{n} settings changed", &[]),
        };
        ui.label(egui::RichText::new(words).size(12.0).color(TEXT_FAINT));
    });
    if let Some(outcome) = &look.outcome {
        ui.label(egui::RichText::new(outcome.as_str()).size(12.0).color(TEXT_DIM));
    }
    if look.changed_there.is_some() {
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(t(
                "integrations.look.changed_there",
                "This screen's look was changed in VoxLive since you opened it.",
            ))
            .color(crate::studio::TALLY),
        );
        ui.horizontal(|ui| {
            if ui
                .button(t("integrations.look.use_theirs", "Use VoxLive's"))
                .on_hover_text(t(
                    "integrations.look.use_theirs.hint",
                    "Let your changes go and start from the look VoxLive has now",
                ))
                .clicked()
                && let Some(now) = look.changed_there.clone()
            {
                look.take(&now);
                look.outcome = None;
            }
            if ui
                .button(t("integrations.look.save_mine", "Save Mine Anyway"))
                .on_hover_text(t("integrations.look.save_mine.hint", "Your changed settings go over VoxLive's"))
                .clicked()
            {
                request = Some(Request::SaveLook { anyway: true });
            }
        });
    }
    if request.is_some() {
        view.request = request;
    }
}

/// A setting's name from VoxLive's key: "maxLines" is "Lines", "letterSpacing" "Letter spacing".
fn setting_name(key: &str) -> String {
    let named = match key {
        "bg" => t("integrations.look.setting.bg", "Background"),
        "text" => t("integrations.look.setting.text", "Text colour"),
        "transparent" => t("integrations.look.setting.transparent", "See-through"),
        "valign" => t("integrations.look.setting.valign", "Up and down"),
        "align" => t("integrations.look.setting.align", "Across"),
        "refPosition" => t("integrations.look.setting.ref_position", "Reference"),
        "fitHeight" => t("integrations.look.setting.fit_height", "Height used"),
        "maxWidth" => t("integrations.look.setting.max_width", "Width used"),
        "maxLines" => t("integrations.look.setting.max_lines", "Lines"),
        // The rest VoxLive has now, as the words made from their keys below say them in English.
        "mode" => t("integrations.look.setting.mode", "Mode"),
        "font" => t("integrations.look.setting.font", "Font"),
        "size" => t("integrations.look.setting.size", "Size"),
        "weight" => t("integrations.look.setting.weight", "Weight"),
        "padding" => t("integrations.look.setting.padding", "Padding"),
        "lineHeight" => t("integrations.look.setting.line_height", "Line height"),
        "letterSpacing" => t("integrations.look.setting.letter_spacing", "Letter spacing"),
        "uppercase" => t("integrations.look.setting.uppercase", "Uppercase"),
        "shadow" => t("integrations.look.setting.shadow", "Shadow"),
        "edgeSize" => t("integrations.look.setting.edge_size", "Edge size"),
        "boxOpacity" => t("integrations.look.setting.box_opacity", "Box opacity"),
        "fade" => t("integrations.look.setting.fade", "Fade"),
        "passage" => t("integrations.look.setting.passage", "Passage"),
        _ => "",
    };
    if !named.is_empty() {
        return named.to_owned();
    }
    let mut out = String::new();
    for (index, c) in key.chars().enumerate() {
        if index == 0 {
            out.extend(c.to_uppercase());
        } else if c.is_uppercase() {
            out.push(' ');
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// The control for one setting, as VoxLive describes it: a menu of its values, a colour, a tick,
/// or a slider within its range. Returns the value it now has.
fn setting_control(
    ui: &mut egui::Ui,
    key: &str,
    field: &serde_json::Value,
    current: &serde_json::Value,
) -> Option<serde_json::Value> {
    match field.get("type").and_then(serde_json::Value::as_str).unwrap_or("") {
        "enum" => {
            let values = field.get("values").and_then(serde_json::Value::as_array).cloned().unwrap_or_default();
            let labels = field.get("labels").and_then(serde_json::Value::as_object).cloned().unwrap_or_default();
            let words = |value: &serde_json::Value| {
                let raw = value.as_str().map_or_else(|| value.to_string(), str::to_owned);
                labels.get(&raw).and_then(serde_json::Value::as_str).map_or(raw, str::to_owned)
            };
            let mut chosen = current.clone();
            egui::ComboBox::from_id_salt(("voxlive look", key)).selected_text(words(current)).width(180.0).show_ui(
                ui,
                |ui| {
                    for value in &values {
                        ui.selectable_value(&mut chosen, value.clone(), words(value));
                    }
                },
            );
            Some(chosen)
        }
        "color" => {
            let hex = current.as_str().unwrap_or("#000000");
            let mut colour = colour_of(hex);
            ui.color_edit_button_srgba(&mut colour);
            Some(serde_json::Value::String(hex_of(colour, hex.len() > 7)))
        }
        "boolean" => {
            let mut on = current.as_bool().unwrap_or(false);
            ui.checkbox(&mut on, "");
            Some(serde_json::Value::Bool(on))
        }
        "number" => {
            let min = field.get("min").and_then(serde_json::Value::as_f64).unwrap_or(0.0);
            let max = field.get("max").and_then(serde_json::Value::as_f64).unwrap_or(100.0).max(min);
            let step = field.get("step").and_then(serde_json::Value::as_f64).unwrap_or(1.0);
            let unit = field.get("unit").and_then(serde_json::Value::as_str).unwrap_or("");
            let mut number = current.as_f64().unwrap_or(min).clamp(min, max);
            let suffix = if unit.is_empty() { String::new() } else { format!(" {unit}") };
            ui.add(egui::Slider::new(&mut number, min..=max).step_by(step).suffix(suffix));
            // A whole number stays whole, as VoxLive gave it.
            if number.fract() == 0.0 && current.is_i64() {
                return Some(serde_json::Value::from(number as i64));
            }
            serde_json::Number::from_f64(number).map(serde_json::Value::Number)
        }
        _ => {
            ui.label(egui::RichText::new(current.to_string()).color(TEXT_FAINT));
            None
        }
    }
}

/// A time as a table's format writes it; nothing, rather than a stop, if chrono can't read the
/// format.
fn written(time: impl std::fmt::Display) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let _ = write!(out, "{time}");
    out
}

/// A colour from VoxLive's "#RGB" … "#RRGGBBAA".
fn colour_of(hex: &str) -> Color32 {
    let digits = hex.trim_start_matches('#');
    let full: String = if digits.len() == 3 || digits.len() == 4 {
        digits.chars().flat_map(|c| [c, c]).collect()
    } else {
        digits.to_owned()
    };
    let part = |at: usize| u8::from_str_radix(full.get(at..at + 2).unwrap_or("ff"), 16).unwrap_or(255);
    Color32::from_rgba_unmultiplied(part(0), part(2), part(4), if full.len() >= 8 { part(6) } else { 255 })
}

/// A colour as "#RRGGBB" (or "#RRGGBBAA" when it had or now has some see-through).
fn hex_of(colour: Color32, with_alpha: bool) -> String {
    let [r, g, b, a] = colour.to_srgba_unmultiplied();
    if with_alpha || a < 255 { format!("#{r:02X}{g:02X}{b:02X}{a:02X}") } else { format!("#{r:02X}{g:02X}{b:02X}") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_go_to_and_from_voxlives_hex() {
        assert_eq!(hex_of(colour_of("#FFE14D"), false), "#FFE14D");
        assert_eq!(hex_of(colour_of("#fff"), false), "#FFFFFF");
        assert_eq!(hex_of(colour_of("#00000080"), true), "#00000080");
        // In English whatever the one test that sets the global language is doing meanwhile.
        crate::i18n::with_language(crate::i18n::Language::English, || {
            assert_eq!(setting_name("letterSpacing"), "Letter spacing");
            assert_eq!(setting_name("maxLines"), "Lines");
        });
    }

    #[test]
    fn disconnect_asks_first_and_only_return_disconnects() {
        let ctx = egui::Context::default();
        let frame = |key: Option<egui::Key>| {
            let events = key
                .map(|key| egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                })
                .into_iter()
                .collect();
            let mut answered = false;
            let mut drawn = ctx.run_ui(egui::RawInput { events, ..Default::default() }, |ui| {
                answered = disconnect_answered(ui, "voxlive", "Disconnect VoxLive?", "The key is forgotten.");
            });
            drawn.textures_delta.clear();
            answered
        };
        assert!(!frame(Some(egui::Key::Enter)), "not asked: Return does nothing");
        ctx.data_mut(|data| data.insert_temp(disconnecting_id("voxlive"), true));
        assert!(!frame(None), "asked, not answered");
        assert!(!frame(Some(egui::Key::Escape)), "Escape keeps the key");
        assert!(!frame(Some(egui::Key::Enter)), "and the question has gone");
        ctx.data_mut(|data| data.insert_temp(disconnecting_id("voxlive"), true));
        assert!(frame(Some(egui::Key::Enter)), "Return disconnects");
    }

    /// Every word the pane draws in one frame.
    fn words_drawn(songbooks: Option<&mut crate::songbooks::View>) -> Vec<String> {
        words_drawn_with(songbooks, None)
    }

    fn words_drawn_with(
        mut songbooks: Option<&mut crate::songbooks::View>,
        mut timelychurch: Option<&mut crate::timelychurch::View>,
    ) -> Vec<String> {
        fn walk(shape: &egui::Shape, words: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(text) => words.push(text.galley.text().to_owned()),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| walk(shape, words)),
                _ => {}
            }
        }
        let ctx = egui::Context::default();
        let (mut voxlive, mut canva) = (View::default(), crate::canva::View::default());
        let mut not_offered = crate::timelychurch::View::default();
        let mut drawn = ctx.run_ui(egui::RawInput::default(), |ui| {
            let timelychurch = timelychurch.as_deref_mut().unwrap_or(&mut not_offered);
            pane(ui, &mut voxlive, &mut canva, songbooks.as_deref_mut(), timelychurch);
        });
        drawn.textures_delta.clear();
        let mut words = Vec::new();
        drawn.shapes.iter().for_each(|clipped| walk(&clipped.shape, &mut words));
        words
    }

    #[test]
    fn the_songbooks_are_coming_soon_unless_the_catalog_is_offered() {
        crate::i18n::with_language(crate::i18n::Language::English, || {
            let without = words_drawn(None);
            assert!(without.iter().any(|w| w == "VoxLive") && without.iter().any(|w| w == "Canva Sync"), "{without:?}");
            assert!(without.iter().any(|w| w == "Worship Leader App"), "{without:?}");
            assert_eq!(
                without.iter().filter(|w| *w == "COMING SOON").count(),
                2,
                "songbooks and TimelyChurch: {without:?}"
            );
            let mut songbooks = crate::songbooks::View::default();
            let with = words_drawn(Some(&mut songbooks));
            assert!(with.iter().any(|w| w == "Worship Leader App"), "{with:?}");
            assert!(with.iter().any(|w| w == "NOT DOWNLOADED"), "{with:?}");
        });
    }

    #[test]
    fn timelychurch_is_coming_soon_unless_offered_and_its_page_follows_the_connection() {
        use crate::timelychurch::{RowState, ServiceRow, Status, View as Card};
        crate::i18n::with_language(crate::i18n::Language::English, || {
            let soon = words_drawn(None);
            assert!(soon.iter().any(|w| w == "TimelyChurch") && soon.iter().any(|w| w == "Soon"), "{soon:?}");
            let mut card = Card { offered: true, status: Status::NotConnected, asks_address: true, ..Card::default() };
            let listed = words_drawn_with(None, Some(&mut card));
            assert!(listed.iter().any(|w| w == "TimelyChurch") && listed.iter().any(|w| w == "Connect"), "{listed:?}");
            card.open = true;
            let page = words_drawn_with(None, Some(&mut card));
            for words in ["CONNECT TO TIMELYCHURCH", "Address", "Pairing code", "Connect"] {
                assert!(page.iter().any(|w| w == words), "{words}: {page:?}");
            }
            card.status = Status::Connected;
            card.church = "Demo Church".to_owned();
            card.services =
                vec![ServiceRow { name: "Sunday".to_owned(), date: None, songs: 2, state: RowState::Removed }];
            let connected = words_drawn_with(None, Some(&mut card));
            for words in [
                "Connected to Demo Church · not checked yet",
                "Check Now",
                "Disconnect",
                "2 songs",
                "Removed in TimelyChurch",
            ] {
                assert!(connected.iter().any(|w| w == words), "{words}: {connected:?}");
            }
            assert!(!connected.iter().any(|w| w == "Pairing code"), "{connected:?}");
        });
    }
}
