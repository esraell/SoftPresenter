//! Settings › Integrations: the services SoftPresenter works with, each the same card (the
//! owner, 2026-11-25: "the integration cards should be the same and that goes for voxlive"): its
//! logo, name and one line, a light with how it stands, one key. A card opens that service's
//! page in place, every page the same way, in the cards and rows of the rest of Settings: the way
//! back, its logo and how it stands, what it brings, then what the service shows first, then the
//! presentation or the connection, then the account; every hint is a line on the page. Worship Leader App's free songbooks (where the song catalog is
//! offered, `songbooks`), VoxLive, Canva Sync, TimelyChurch (where it is offered,
//! `timelychurch`) and NDI (installing it, its updates and removing it, `ndi_download`); a
//! service not offered yet is greyed as coming soon. The last card asks which other app or
//! device a church uses, and opens the Request an Integration window (`integration_request`).

use egui::{Align2, Color32, FontId, Rect, Sense, Stroke, pos2, vec2};

use crate::home::settings::parts;
use crate::i18n::{t, tf, tn};
use crate::logos::{self, Logo, Mark};
use crate::quick_help::Hint;
use crate::studio::{CARD, CARE, LINE, PANEL, TEXT, TEXT_DIM, TEXT_FAINT, semibold};
use crate::voxlive::{Status, View};

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

/// NDI's card: installing it, as far as that has got; whether NDI is on this computer; and
/// whether its page is open.
pub struct Ndi<'a> {
    pub download: &'a mut crate::ndi_download::NdiDownload,
    pub found: bool,
    pub open: &'a mut bool,
}

/// The pane: the list of services, or the page of the one open, or a screen's look.
/// `songbooks` is given only where the song catalog is offered; TimelyChurch's card is greyed
/// as coming soon unless it is offered (`timelychurch.offered`). The list's last card opens the
/// window of `ask`. True when NDI's page asked for Settings › Screens.
pub fn pane(
    ui: &mut egui::Ui,
    view: &mut View,
    canva: &mut crate::canva::View,
    mut songbooks: Option<&mut crate::songbooks::View>,
    timelychurch: &mut crate::timelychurch::View,
    ndi: Ndi<'_>,
    ask: &mut crate::integration_request::IntegrationRequest,
) -> bool {
    let songbooks_open = songbooks.as_ref().is_some_and(|songbooks| songbooks.open);
    if *ndi.open {
        return ndi_page(ui, ndi);
    }
    if view.look.is_some() {
        look_page(ui, view);
    } else if view.open {
        voxlive_page(ui, view);
        scrolled_to_end(ui);
    } else if canva.open {
        canva_page(ui, canva);
        scrolled_to_end(ui);
    } else if timelychurch.offered && timelychurch.open {
        timelychurch_page(ui, timelychurch);
        scrolled_to_end(ui);
    } else if let (true, Some(songbooks)) = (songbooks_open, songbooks.as_deref_mut()) {
        songbooks_page(ui, songbooks);
    } else {
        list(ui, view, canva, songbooks, timelychurch, ndi, ask);
    }
    false
}

/// How NDI stands, for its card and the head of its page: not installed, installed (with the
/// version where it is known), or that an update may be out, in amber.
fn ndi_state(ndi: &mut Ndi<'_>) -> (Color32, String) {
    let row = crate::ndi_download::row::row(ndi.download, ndi.found);
    let installed = ndi.found || row.act != Some(crate::ndi_download::row::Act::Open);
    if row.amber {
        (CARE, t("integrations.ndi.state.update", "Update may be out").to_owned())
    } else if !installed {
        (TEXT_FAINT, t("integrations.ndi.state.none", "Not installed").to_owned())
    } else {
        match ndi.download.version() {
            Some(version) => {
                (GOOD, tf("integrations.ndi.state.installed", "Installed \u{b7} {version}", &[("version", &version)]))
            }
            None => (GOOD, t("integrations.ndi.state.installed.unversioned", "Installed").to_owned()),
        }
    }
}

/// Settings › Integrations › NDI: how NDI stands, with the key that installs it, checks for a
/// newer one or brings an update in; where it is used; removing it; and NDI's trademark line.
/// True when Screens was asked for.
fn ndi_page(ui: &mut egui::Ui, mut ndi: Ndi<'_>) -> bool {
    let state = ndi_state(&mut ndi);
    let what = t("integrations.ndi.what", "Send a screen over the network, and bring a camera into a slide");
    if page_head(ui, Mark::Logo(Logo::Ndi), "NDI", state, what) {
        *ndi.open = false;
    }
    let mut to_screens = false;
    parts::card(
        ui,
        "NDI",
        t("integrations.ndi.card.about", "Video over the network: screens out, cameras in"),
        None,
        |ui| to_screens = crate::ndi_download::row::rows(ui, ndi.download, ndi.found),
    );
    // NDI's site, linked wherever NDI is offered, as NDI's SDK asks.
    ui.hyperlink_to(egui::RichText::new("ndi.video").size(12.5), sp_ndi::DOWNLOAD_URL)
        .hint(t("quickhelp.ndi.page", "Open NDI\u{2019}s download page in your browser."));
    to_screens
}

/// Testing aid: `SOFTPRESENTER_CAPTURE_PREFERENCES=voxlive-end` (`canva-end`,
/// `timelychurch-end`) opens that service's page scrolled to its end, to see its last cards in a
/// capture.
fn scrolled_to_end(ui: &mut egui::Ui) {
    if std::env::var("SOFTPRESENTER_CAPTURE_PREFERENCES").is_ok_and(|page| page.ends_with("-end")) {
        ui.scroll_to_cursor(Some(egui::Align::BOTTOM));
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
/// and a last card that asks for another. A card opens that service's page; the last opens the
/// Request an Integration window.
fn list(
    ui: &mut egui::Ui,
    view: &mut View,
    canva: &mut crate::canva::View,
    songbooks: Option<&mut crate::songbooks::View>,
    timelychurch: &mut crate::timelychurch::View,
    mut ndi: Ndi<'_>,
    ask: &mut crate::integration_request::IntegrationRequest,
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
                None => (TEXT_FAINT, String::new()),
            },
            key: open,
            soon: false,
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
        Service {
            mark: Mark::Logo(Logo::Ndi),
            name: "NDI",
            about: t("integrations.ndi.what", "Send a screen over the network, and bring a camera into a slide"),
            state: ndi_state(&mut ndi),
            key: open,
            soon: false,
        },
    ];
    // Worship Leader App's card only where the song catalog is offered, as before (releases
    // don't offer it until Worship Leader App answers).
    let shown: Vec<(usize, &Service)> =
        services.iter().enumerate().filter(|(id, _)| *id != 0 || songbooks_offered).collect();
    // Two across where there is room for 240 each, one above another where there isn't.
    const BETWEEN: f32 = 12.0;
    let across = (((width + BETWEEN) / (240.0 + BETWEEN)).floor() as usize).clamp(1, 2);
    let each = (width - BETWEEN * (across - 1) as f32) / across as f32;
    let tallest = shown
        .iter()
        .map(|(_, service)| card_height(ui, service, each))
        .fold(request_card_height(ui, each).max(176.0), f32::max);
    let count = shown.len() + 1;
    let rows = count.div_ceil(across);
    let (grid, _) = ui.allocate_exact_size(vec2(width, rows as f32 * (tallest + BETWEEN) - BETWEEN), Sense::hover());
    let mut opened = None;
    for (index, (id, service)) in shown.iter().enumerate() {
        let corner =
            grid.min + vec2((index % across) as f32 * (each + BETWEEN), (index / across) as f32 * (tallest + BETWEEN));
        if service_card(ui, Rect::from_min_size(corner, vec2(each, tallest)), service, *id) {
            opened = Some(*id);
        }
    }
    let index = shown.len();
    let corner =
        grid.min + vec2((index % across) as f32 * (each + BETWEEN), (index / across) as f32 * (tallest + BETWEEN));
    if request_card(ui, Rect::from_min_size(corner, vec2(each, tallest))) {
        ask.open();
    }
    match opened {
        Some(0) => {
            if let Some(songbooks) = songbooks {
                songbooks.open = true;
            }
        }
        Some(1) => view.open = true,
        Some(2) => canva.open = true,
        Some(3) => timelychurch.open = true,
        Some(4) => *ndi.open = true,
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
        3 => t("integrations.timelychurch.row.hint", "Connect TimelyChurch, or see its services"),
        _ => t("integrations.ndi.row.hint", "Install NDI, check for a newer one, or remove it"),
    };
    response.hint_or_greyed(hint, service.soon.then(|| t("integrations.state.soon", "Coming soon"))).clicked()
}

/// The last card's question and its line, each line of them centred, laid out for a card
/// `width` wide (drawn from the card's middle).
fn request_card_parts(ui: &egui::Ui, width: f32) -> [std::sync::Arc<egui::Galley>; 2] {
    let room = width - 48.0;
    let centred = |text: &str, font: FontId, ink: Color32| {
        let mut job = egui::text::LayoutJob::simple(text.to_owned(), font, ink, room);
        job.halign = egui::Align::Center;
        ui.painter().layout_job(job)
    };
    [
        centred(t("integrations.ask.card", "Don\u{2019}t see what your church uses?"), semibold(13.0), TEXT_DIM),
        centred(
            t("integrations.ask.card.about", "Tell us which app or device, and what it should do."),
            FontId::proportional(13.0),
            TEXT_FAINT,
        ),
    ]
}

/// The last card's key.
fn request_key() -> &'static str {
    t("integrations.ask.key", "Request an Integration\u{2026}")
}

/// How tall the last card must be for its words and key to fit, at `width`.
fn request_card_height(ui: &egui::Ui, width: f32) -> f32 {
    let [title, line] = request_card_parts(ui, width);
    16.0 + title.size().y + 8.0 + line.size().y + 16.0 + 30.0 + 16.0
}

/// The card after the services, in `rect`: a dashed outline with the cards' corners, brighter
/// under the pointer, and in its middle "Don't see what your church uses?", a line, and the key
/// Request an Integration…. The whole card is the key. True when clicked.
fn request_card(ui: &mut egui::Ui, rect: Rect) -> bool {
    let response = ui.interact(rect, ui.scope_id().with("request an integration card"), Sense::click()).hint(t(
        "quickhelp.integrations.request",
        "Tell the SoftPresenter project which app or device you\u{2019}d like it to work with.",
    ));
    let hovered = response.hovered();
    let painter = ui.painter();
    let mut outline = parts::rounded_outline(rect.shrink(0.5), crate::studio::RADIUS_CARD);
    outline.extend(outline.first().copied());
    let edge = if hovered { TEXT_FAINT } else { crate::studio::BUTTON_HOVER };
    painter.extend(egui::Shape::dashed_line(&outline, Stroke::new(1.0, edge), 4.0, 3.0));
    let [title, line] = request_card_parts(ui, rect.width());
    let key_words = request_key();
    let key_width = crate::studio::widest(ui, &[key_words], &FontId::proportional(12.5)) + 26.0;
    let height = title.size().y + 8.0 + line.size().y + 16.0 + 30.0;
    let top = rect.center().y - height / 2.0;
    let (title_height, line_height) = (title.size().y, line.size().y);
    painter.galley(pos2(rect.center().x, top), title, TEXT_DIM);
    painter.galley(pos2(rect.center().x, top + title_height + 8.0), line, TEXT_FAINT);
    let key = Rect::from_min_size(
        pos2(rect.center().x - key_width / 2.0, top + title_height + 8.0 + line_height + 16.0),
        vec2(key_width, 30.0),
    );
    // The key is part of the card, which takes the click.
    crate::studio::paint_flat(painter, key, 9.0, crate::studio::Flat::Plain, hovered);
    painter.text(key.center(), Align2::CENTER_CENTER, key_words, FontId::proportional(12.5), TEXT);
    response.clicked()
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
    let response = response.hint(t("quickhelp.integrations.back", "Back to the list of integrations."));
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
    // 14 under the way back and 16 under the head, as the design has them, with nothing added.
    ui.add_space(14.0 - ui.spacing().item_spacing.y);
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
    ui.add_space(16.0 - ui.spacing().item_spacing.y);
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

/// The long line saying what a service does, under its page's head: dim words in paragraph
/// width, as the design's lead.
fn lead(ui: &mut egui::Ui, text: &str) {
    let width = ui.available_width();
    let galley = parts::lines(ui, text, FontId::proportional(13.5), TEXT_DIM, width.min(620.0), 20.0);
    let (place, _) = ui.allocate_exact_size(vec2(width, galley.size().y), Sense::hover());
    ui.painter().galley(place.min, galley, TEXT_DIM);
    ui.add_space((14.0 - ui.spacing().item_spacing.y).max(0.0));
}

/// Something to take care of, in amber, at the top of a card: a key refused, a service out of
/// reach, why something typed wasn't taken.
fn warn(ui: &mut egui::Ui, text: &str) {
    parts::coloured_note(ui, text, CARE);
}

/// Settings › Integrations › VoxLive: its screens, then the connection (the key kept, Find
/// Screens Again and Disconnect); or, with no key, Connect. A key VoxLive refused shows Connect
/// too, to paste another.
fn voxlive_page(ui: &mut egui::Ui, view: &mut View) {
    let what = t("integrations.voxlive.about.short", "Live translation captions on a screen");
    if page_head(ui, Mark::Logo(Logo::VoxLive), "VoxLive", state_words(&view.status), what) {
        view.open = false;
        view.typed.clear();
        view.problem = None;
    }
    lead(
        ui,
        t(
            "integrations.voxlive.about",
            "VoxLive translates the sermon as it is preached, and shows it as captions on its screens. Connected, SoftPresenter shows your church's VoxLive screens as the slides of a VoxLive item (under LIBRARY, and in playlists) and in the VoxLive tab beside the screens: put one on screen and its captions take the place of a verse; put the verse on again to bring it back.",
        ),
    );
    match (view.status.clone(), view.key_hint.clone()) {
        (Status::NoKey, _) | (_, None) => connect_card(ui, view),
        (status, Some(hint)) => {
            screens_card(ui, view);
            connection_card(ui, view, &status, &hint);
            if matches!(status, Status::Refused(_)) {
                connect_card(ui, view);
            }
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

/// Your VoxLive screens, as last found: a row each, its kind and language, and Change Look ›
/// where its look can be changed.
fn screens_card(ui: &mut egui::Ui, view: &mut View) {
    let count = view.screens.len() as u64;
    let about = if count > 0 {
        tn("integrations.voxlive.screens.count", count, "{n} screen", "{n} screens", &[])
    } else {
        String::new()
    };
    let mut open = None;
    parts::card(ui, t("integrations.voxlive.screens.title", "Your VoxLive screens"), &about, None, |ui| {
        if view.screens.is_empty() {
            let words = match view.status {
                Status::Connected => t(
                    "integrations.voxlive.screens.none",
                    "No screens yet. Make one in VoxLive (Screens), then Find Screens Again.",
                ),
                _ => t("integrations.voxlive.screens.not_yet", "Not found yet."),
            };
            parts::note(ui, words);
        }
        let change = t("integrations.voxlive.change_look", "Change Look ›");
        let hint =
            t("integrations.voxlive.change_look.hint", "Change how this screen looks, here; it is saved to VoxLive");
        for screen in &view.screens {
            // A screen with a page can have its look changed.
            let can_open = !screen.url.is_empty() && !view.schema.is_empty();
            let kind = match screen.kind() {
                "captions" => t("integrations.voxlive.kind.captions", "Captions"),
                "Scripture" => t("integrations.voxlive.kind.scripture", "Scripture"),
                other => other,
            };
            let language = screen.language.to_uppercase();
            let line: Vec<&str> = [kind, language.as_str(), if can_open { hint } else { "" }]
                .into_iter()
                .filter(|w| !w.is_empty())
                .collect();
            let keys = if can_open { vec![parts::Key::plain(change)] } else { Vec::new() };
            if parts::keys_row(ui, &screen.id, screen.name(), &line.join(" · "), None, &keys).is_some() {
                open = Some(screen.id.clone());
            }
        }
        parts::note(
            ui,
            t(
                "integrations.voxlive.screens.about",
                "A screen's live display is opened only on your real screens, where it shows the captions (and can start that language's translation); previews use its free test pattern.",
            ),
        );
    });
    if let Some(screen) = open {
        view.request = Some(Request::OpenLook(screen));
    }
}

/// The connection: the key's start, Find Screens Again and Disconnect, and why VoxLive refused
/// it or can't be reached.
fn connection_card(ui: &mut egui::Ui, view: &mut View, status: &Status, hint: &str) {
    let title = t("integrations.connection.title", "Connection");
    parts::card(ui, title, t("integrations.voxlive.connection.about", "The key, kept on this computer"), None, |ui| {
        if let Status::Refused(why) | Status::Trouble(why) = status {
            warn(ui, why);
        }
        let line = t(
            "integrations.voxlive.disconnect.line",
            "Disconnect forgets the key on this computer (it stays in VoxLive until you revoke it there)",
        );
        let keys = [
            parts::Key::plain(t("integrations.voxlive.refresh", "Find Screens Again")),
            parts::Key::plain(t("integrations.disconnect", "Disconnect")),
        ];
        match parts::keys_row(ui, "voxlive key", t("integrations.voxlive.key", "Key"), line, Some(hint), &keys) {
            Some(0) => view.request = Some(Request::Refresh),
            Some(_) => ask_to_disconnect(ui, "voxlive"),
            None => {}
        }
    });
}

/// How to make a VoxLive key, and where this computer keeps it.
fn how_to_get_a_key() -> &'static str {
    if cfg!(target_os = "macos") {
        t(
            "integrations.voxlive.connect.about",
            "In VoxLive, open Settings › API keys and make a key with the screens permissions (read and write). Paste it here. It is kept in this Mac's Keychain and sent only to VoxLive.",
        )
    } else if cfg!(windows) {
        t(
            "integrations.voxlive.connect.about.windows",
            "In VoxLive, open Settings › API keys and make a key with the screens permissions (read and write). Paste it here. It is kept in Windows' Credential Manager on this computer and sent only to VoxLive.",
        )
    } else {
        t(
            "integrations.voxlive.connect.about.linux",
            "In VoxLive, open Settings › API keys and make a key with the screens permissions (read and write). Paste it here. It is kept in this computer's keyring and sent only to VoxLive.",
        )
    }
}

/// Connect: where the key is pasted, and how to get one; Return in the field connects too.
fn connect_card(ui: &mut egui::Ui, view: &mut View) {
    let about = t("integrations.voxlive.connect.with", "With a key made in VoxLive");
    parts::card(ui, t("integrations.connect.title", "Connect"), about, None, |ui| {
        if let Some(problem) = &view.problem {
            warn(ui, problem);
        }
        let how = how_to_get_a_key();
        let key = parts::Key::main(t("integrations.voxlive.connect", "Connect")).enabled(!view.typed.trim().is_empty());
        let label = t("integrations.voxlive.key", "Key");
        let (field, clicked) = parts::field_row(
            ui,
            "voxlive key field",
            label,
            how,
            &mut view.typed,
            (true, "vxk_live_…"),
            190.0,
            Some(key),
        );
        let entered = field.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
        if (clicked || entered) && !view.typed.trim().is_empty() {
            view.request = Some(Request::Connect(std::mem::take(&mut view.typed)));
        }
    });
}

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

/// Canva Sync's page. Signed in: the old sync script to switch off (while it must be), the sync,
/// the presentation and its divider page, where the video goes, and the account. Not signed in:
/// Connect, and the old sync script's sign-in where this Mac has it.
fn canva_page(ui: &mut egui::Ui, view: &mut crate::canva::View) {
    use crate::canva::{Request as R, Status as S};
    let what = t("integrations.canva.about.short", "The announcements video, fresh from your Canva presentation");
    if page_head(ui, Mark::Logo(Logo::Canva), "Canva Sync", canva_state(view), what) {
        view.open = false;
        view.typed_secret.clear();
    }
    lead(
        ui,
        t(
            "integrations.canva.about",
            "Keeps the announcements video fresh from your church's Canva presentation. The pages above a divider page are exported as a 1080p video into the library's Media › Canva Sync folder, which the media playlist Announcements follows, so the video joins the media bin by itself. Nothing happens while SoftPresenter is closed.",
        ),
    );
    // What the last change did, or why it couldn't, above the cards.
    if let Some((said, good)) = &view.said {
        let tone = if *good { crate::cli_install::Tone::Good } else { crate::cli_install::Tone::Trouble };
        parts::said_line(ui, Some((tone, said)), "");
    }
    let mut request = None;
    match &view.status {
        S::Reading => {
            parts::note(ui, t("integrations.canva.reading", "Reading the sign-in kept on this computer…"));
        }
        S::NotConnected | S::Refused(_) => canva_connect(ui, view, &mut request),
        S::Connected | S::Doubtful(_) => {
            if view.settings.old_script_warning {
                old_script_warning(ui, view, &mut request);
            }
            sync_card(ui, view, &mut request);
            design_card(ui, view, &mut request);
            parts::card(
                ui,
                t("integrations.canva.where.title", "Where it goes"),
                t("integrations.canva.where.sub", "In the library and the media bin"),
                None,
                |ui| {
                    parts::note(
                        ui,
                        t(
                            "integrations.canva.where.about",
                            "Media › Canva Sync in the library, followed by the media playlist Announcements (made when the divider is chosen). One video is kept there, named with its day and time so volunteers can read it.",
                        ),
                    );
                },
            );
            account_card(ui, view);
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

/// Sync: when the video was last updated and its file, with Sync Now (greyed until the divider is
/// chosen, and while the old script must be switched off); while a job runs, what it is doing,
/// with Stop. Then how to sync each time SoftPresenter opens.
fn sync_card(ui: &mut egui::Ui, view: &crate::canva::View, request: &mut Option<crate::canva::Request>) {
    use crate::canva::Request as R;
    let chosen = view.settings.design.is_some() && view.settings.divider.is_some();
    let about = if chosen {
        t("integrations.canva.sync.about", "The announcements video")
    } else {
        t("integrations.canva.sync.choose", "Choose the design first")
    };
    parts::card(ui, t("integrations.canva.sync.title", "Sync"), about, None, |ui| {
        if let crate::canva::Status::Doubtful(why) = &view.status {
            warn(ui, why);
        }
        let (sync_now, stop) = (t("integrations.canva.sync_now", "Sync Now"), t("integrations.canva.stop", "Stop"));
        let (both, other) = ([sync_now, stop], [stop]);
        if view.phase.busy() {
            let line = t("integrations.canva.stop.line", "Stop: the video in the media bin stays as it was");
            let keys = [parts::Key::plain(stop).as_wide_as(&both)];
            if parts::keys_row(ui, "canva sync", &view.phase.words(), line, None, &keys).is_some() {
                *request = Some(R::Stop);
            }
        } else {
            let (label, file) = match (&view.settings.updated_at, &view.settings.updated_file) {
                (Some(at), Some(file)) => {
                    let format = t("integrations.canva.updated.format", "%A %-d %B, %-I:%M %p");
                    let when = chrono::DateTime::parse_from_rfc3339(at)
                        .map(|at| written(at.with_timezone(&chrono::Local).format(format)))
                        .unwrap_or_default();
                    (tf("integrations.canva.updated.when", "Last updated {when}", &[("when", &when)]), file.as_str())
                }
                _ => (t("integrations.canva.not_synced", "Not synced yet").to_owned(), ""),
            };
            let ready = chosen && !view.settings.hold_syncs;
            let keys = [parts::Key::main(sync_now).enabled(ready).as_wide_as(&other)];
            if parts::keys_row(ui, "canva sync", &label, file, None, &keys).is_some() {
                *request = Some(R::SyncNow);
            }
        }
        parts::note(
            ui,
            t(
                "integrations.canva.macro",
                "To fetch a fresh video each time SoftPresenter opens, make a macro with the step Sync from Canva and tick Run When SoftPresenter Opens. A new video replaces the old one; one playing takes the new one over at the end of its pass, so the loop carries on.",
            ),
        );
    });
}

/// The account: signed in, with the integration's ID, and Disconnect.
fn account_card(ui: &mut egui::Ui, view: &crate::canva::View) {
    let signed_in = t("integrations.canva.signed_in", "Signed in");
    parts::card(ui, t("integrations.canva.account", "Account"), signed_in, None, |ui| {
        let forget = t(
            "integrations.canva.disconnect.line",
            "Disconnect forgets the sign-in on this computer (it stays in Canva until you take it back there)",
        );
        let line = match &view.client_hint {
            Some(hint) => {
                format!("{} · {forget}", tf("integrations.canva.client", "integration {hint}", &[("hint", hint)]))
            }
            None => forget.to_owned(),
        };
        let keys = [parts::Key::plain(t("integrations.disconnect", "Disconnect"))];
        if parts::keys_row(ui, "canva account", signed_in, &line, None, &keys).is_some() {
            ask_to_disconnect(ui, "canva");
        }
    });
}

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
/// pairing code); once connected, the playlists made from its upcoming services, then the
/// connection (the church, when it was last checked, Check Now and Disconnect).
fn timelychurch_page(ui: &mut egui::Ui, view: &mut crate::timelychurch::View) {
    use crate::timelychurch::{Request as R, Status as S};
    let what = t("integrations.timelychurch.about.short", "Service plans and set lists from your church planner");
    let name = crate::timelychurch::SOURCE_NAME;
    if page_head(ui, Mark::Logo(Logo::TimelyChurch), name, timelychurch_state(view), what) {
        view.open = false;
        view.typed_code.clear();
        view.problem = None;
    }
    lead(
        ui,
        t(
            "integrations.timelychurch.about",
            "Your church's upcoming setlists and service plans in TimelyChurch become playlists here, and their songs join the library with their translations, kept up to date during the week. A song or playlist you change here is never overwritten: TimelyChurch's newer version is only offered. SoftPresenter tells TimelyChurch which songs were shown in each service, for your church's song use report.",
        ),
    );
    let mut request = None;
    match &view.status {
        S::Off => {}
        S::Reading => {
            parts::note(ui, t("integrations.timelychurch.reading", "Reading the connection kept on this computer…"));
        }
        S::NotConnected | S::Pairing | S::Disconnected => timelychurch_connect(ui, view, &mut request),
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
/// every church) and the pairing code an administrator made; Return in the code connects too.
fn timelychurch_connect(
    ui: &mut egui::Ui,
    view: &mut crate::timelychurch::View,
    request: &mut Option<crate::timelychurch::Request>,
) {
    let title = t("integrations.timelychurch.connect.title", "Connect to TimelyChurch");
    parts::card(ui, title, t("integrations.timelychurch.connect.with", "With a pairing code"), None, |ui| {
        // Disconnected in TimelyChurch: said as TimelyChurch asked, in the language set now.
        let said = match &view.status {
            crate::timelychurch::Status::Disconnected => Some(crate::timelychurch::disconnected_words()),
            _ => view.trouble.clone(),
        };
        if let Some(said) = said {
            warn(ui, &said);
        }
        parts::note(
            ui,
            t(
                "integrations.timelychurch.connect.about",
                "In TimelyChurch, an administrator opens Setlists › Song Sync and makes a pairing code. Type your church's TimelyChurch address and the code here. The connection is kept in this computer's secure store and used only with your church's TimelyChurch.",
            ),
        );
        let pairing = view.status == crate::timelychurch::Status::Pairing;
        if view.asks_address {
            let address = t("integrations.timelychurch.address", "Address");
            let hint = (false, "yourchurch.timelychurch.com");
            parts::field_row(ui, "timelychurch address", address, "", &mut view.typed_address, hint, 240.0, None)
                .0
                .hint(t("quickhelp.timelychurch.address", "Your church\u{2019}s TimelyChurch address."));
        }
        let code = t("integrations.timelychurch.code", "Pairing code");
        let (field, _) =
            parts::field_row(ui, "timelychurch code", code, "", &mut view.typed_code, (false, ""), 240.0, None);
        let field = field.hint(t(
            "quickhelp.timelychurch.code",
            "The pairing code an administrator made in TimelyChurch, under Setlists \u{203a} Song Sync.",
        ));
        let mut connect = field.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
        let filled =
            !view.typed_code.trim().is_empty() && (!view.asks_address || !view.typed_address.trim().is_empty());
        let lead = if pairing { t("integrations.state.connecting", "Connecting…") } else { "" };
        let key = parts::Key::main(t("integrations.timelychurch.connect", "Connect"))
            .enabled(filled && !pairing)
            .hint(t("quickhelp.timelychurch.connect", "Connect to your church\u{2019}s TimelyChurch with this code."));
        connect |= parts::keys_line(ui, "timelychurch connect", lead, &[key]).is_some();
        if connect && filled && !pairing {
            *request = Some(crate::timelychurch::Request::Connect {
                address: view.typed_address.trim().to_owned(),
                code: std::mem::take(&mut view.typed_code),
            });
        }
        if let Some(problem) = &view.problem {
            warn(ui, problem);
        }
    });
}

/// Connected: the playlists made from TimelyChurch's upcoming services, each with how it stands
/// against its service; then the church and when it was last checked, its address, Check Now and
/// Disconnect, and the songs shown that are still to be told.
fn timelychurch_connected(
    ui: &mut egui::Ui,
    view: &crate::timelychurch::View,
    request: &mut Option<crate::timelychurch::Request>,
) {
    use crate::timelychurch::RowState;
    let about = if view.services.is_empty() {
        String::new()
    } else {
        tf(
            "integrations.timelychurch.services.count",
            "{n} in TimelyChurch",
            &[("n", &crate::i18n::count(view.services.len() as u64))],
        )
    };
    parts::card(ui, t("integrations.timelychurch.services.title", "Upcoming services"), &about, None, |ui| {
        if view.services.is_empty() {
            let words = if view.checking || view.last_checked.is_none() {
                t("integrations.timelychurch.services.coming", "The services come in at the first check.")
            } else {
                t("integrations.timelychurch.services.none", "No upcoming services in TimelyChurch.")
            };
            parts::note(ui, words);
        }
        for row in &view.services {
            let date = row.date.as_deref().map(service_date).unwrap_or_default();
            let songs = tn("integrations.timelychurch.songs", row.songs as u64, "{n} song", "{n} songs", &[]);
            let line = if date.is_empty() { songs } else { format!("{date} · {songs}") };
            let (colour, state) = match row.state {
                RowState::InStep => (GOOD, t("integrations.timelychurch.row.in_step", "Up to date")),
                RowState::ChangedHere => (TEXT_FAINT, t("integrations.timelychurch.row.changed", "Changed here")),
                RowState::NewerWaiting => {
                    (CARE, t("integrations.timelychurch.row.newer", "Changed here; newer in TimelyChurch"))
                }
                RowState::Removed => (CARE, t("integrations.timelychurch.row.removed", "Removed in TimelyChurch")),
            };
            // How it stands, at the right in the fixed-width face's capitals, wrapped where the
            // row's words would otherwise have too little room.
            let room = (ui.available_width() - 16.0 - parts::LABEL_ROOM).max(80.0);
            let caps = crate::studio::fixed_semibold(11.0);
            let state = parts::words(ui, &state.to_uppercase(), caps, colour, 0.66, room);
            parts::row(ui, &row.name, &line, state.size(), |ui, place| {
                ui.painter().galley(pos2(place.max.x - state.size().x, place.min.y), state.clone(), colour);
            });
        }
    });
    let about = if view.trouble.is_some() {
        t("integrations.timelychurch.state.trouble", "Can't reach it")
    } else {
        t("integrations.state.connected", "Connected")
    };
    parts::card(ui, t("integrations.connection.title", "Connection"), about, None, |ui| {
        if let Some(trouble) = &view.trouble {
            warn(ui, trouble);
        }
        let when = if view.checking {
            t("integrations.timelychurch.checking", "checking now").to_owned()
        } else {
            match view.last_checked {
                Some(at) => {
                    tf("integrations.timelychurch.checked", "last checked {when}", &[("when", &checked_words(at))])
                }
                None => t("integrations.timelychurch.not_checked", "not checked yet").to_owned(),
            }
        };
        let said = tf(
            "integrations.timelychurch.connected_to",
            "Connected to {church} · {when}",
            &[("church", &view.church), ("when", &when)],
        );
        let hints = t(
            "integrations.timelychurch.connection.line",
            "Check Now asks TimelyChurch for changes now (it is asked every few minutes anyway). Disconnect forgets the connection on this computer; the songs and playlists it made stay",
        );
        let keys = [
            parts::Key::plain(t("integrations.timelychurch.check_now", "Check Now")).enabled(!view.checking),
            parts::Key::plain(t("integrations.disconnect", "Disconnect")),
        ];
        let address = (!view.address.is_empty()).then_some(view.address.as_str());
        match parts::keys_row(ui, "timelychurch connection", &said, hints, address, &keys) {
            Some(0) => *request = Some(crate::timelychurch::Request::CheckNow),
            Some(_) => ask_to_disconnect(ui, "timelychurch"),
            None => {}
        }
        if view.uses_waiting > 0 {
            let waiting = tn(
                "integrations.timelychurch.uses_waiting",
                view.uses_waiting as u64,
                "{n} song shown is waiting to be told to TimelyChurch.",
                "{n} songs shown are waiting to be told to TimelyChurch.",
                &[],
            );
            parts::note(ui, &waiting);
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

/// Connect: how to make the church's own integration in Canva, the redirect URL to give it with
/// Copy, its client ID and secret, and Sign In with Canva (or, while the browser is waited for,
/// Stop). Then, where this Mac has the old sync script's sign-in, the way to take it over.
fn canva_connect(ui: &mut egui::Ui, view: &mut crate::canva::View, request: &mut Option<crate::canva::Request>) {
    use crate::canva::{Phase, Request as R};
    let about = t("integrations.canva.connect.sub", "Canva Sync uses an integration your church makes in Canva");
    parts::card(ui, t("integrations.connect.title", "Connect"), about, None, |ui| {
        if let crate::canva::Status::Refused(why) = &view.status {
            warn(ui, why);
        }
        parts::note(
            ui,
            if cfg!(target_os = "macos") {
                t(
                    "integrations.canva.connect.about",
                    "Make an integration of your church's own in Canva's developer portal (canva.com/developers, Your integrations). Under Scopes tick design:content:read; under Authentication add the redirect URL below; then copy its client ID and make a secret. The secret and the sign-in are kept in this Mac's Keychain and sent only to Canva.",
                )
            } else if cfg!(windows) {
                t(
                    "integrations.canva.connect.about.windows",
                    "Make an integration of your church's own in Canva's developer portal (canva.com/developers, Your integrations). Under Scopes tick design:content:read; under Authentication add the redirect URL below; then copy its client ID and make a secret. The secret and the sign-in are kept in Windows' Credential Manager on this computer and sent only to Canva.",
                )
            } else {
                t(
                    "integrations.canva.connect.about.linux",
                    "Make an integration of your church's own in Canva's developer portal (canva.com/developers, Your integrations). Under Scopes tick design:content:read; under Authentication add the redirect URL below; then copy its client ID and make a secret. The secret and the sign-in are kept in this computer's keyring and sent only to Canva.",
                )
            },
        );
        let redirect = t("integrations.canva.redirect", "Redirect URL");
        let copy = [parts::Key::plain(t("integrations.copy", "Copy")).hint(t(
            "quickhelp.canva.copy_redirect",
            "Copy the redirect URL, to paste in Canva\u{2019}s developer portal.",
        ))];
        if parts::keys_row(ui, "canva redirect", redirect, "", Some(view.redirect.as_str()), &copy).is_some() {
            ui.ctx().copy_text(view.redirect.clone());
        }
        let id = t("integrations.canva.client_id", "Client ID");
        parts::field_row(ui, "canva client id", id, "", &mut view.typed_id, (false, ""), 190.0, None).0.hint(t(
            "quickhelp.canva.client_id",
            "Your Canva integration\u{2019}s client ID, from Canva\u{2019}s developer portal.",
        ));
        let secret = t("integrations.canva.client_secret", "Client secret");
        parts::field_row(ui, "canva client secret", secret, "", &mut view.typed_secret, (true, ""), 190.0, None)
            .0
            .hint(t(
                "quickhelp.canva.client_secret",
                "The secret made for your Canva integration. It is kept in this computer\u{2019}s keyring.",
            ));
        if view.phase == Phase::SigningIn {
            let stop = [parts::Key::plain(t("integrations.canva.stop", "Stop"))];
            if parts::keys_line(ui, "canva sign in", &view.phase.words(), &stop).is_some() {
                *request = Some(R::Stop);
            }
        } else {
            let filled = !view.typed_id.trim().is_empty() && !view.typed_secret.trim().is_empty();
            let sign_in = parts::Key::main(t("integrations.canva.sign_in", "Sign In with Canva"))
                .enabled(filled && !view.phase.busy());
            let hint = t("integrations.canva.sign_in.hint", "Opens Canva's sign-in in your browser");
            if parts::keys_line(ui, "canva sign in", hint, &[sign_in]).is_some() {
                *request = Some(R::SignIn(view.typed_id.trim().to_owned(), std::mem::take(&mut view.typed_secret)));
            }
        }
    });
    if view.old_here {
        let title = t("integrations.canva.old.title", "The old sync script");
        parts::card(ui, title, t("integrations.canva.old.found", "Found on this Mac"), None, |ui| {
            parts::note(
                ui,
                t(
                    "integrations.canva.old.about",
                    "This Mac has the sign-in of the old Canva sync script. SoftPresenter can take it over: it reads it from there itself, keeps it in the Keychain, and takes the script's token file away. Then switch the old script off.",
                ),
            );
            let key = parts::Key::plain(t("integrations.canva.old.use", "Use the Sign-in from the Old Sync Script"))
                .enabled(!view.phase.busy())
                .hint(t("quickhelp.canva.old_use", "Take over the old sync script\u{2019}s sign-in."));
            if parts::keys_line(ui, "canva old use", "", &[key]).is_some() {
                *request = Some(R::Import);
            }
        });
    }
}

/// The old script must go now that its sign-in is here: what to type to switch it off, Copy the
/// Steps, and It's Switched Off. First on the page, as nothing is synced until it is off when
/// its token file couldn't be taken away.
fn old_script_warning(ui: &mut egui::Ui, view: &crate::canva::View, request: &mut Option<crate::canva::Request>) {
    let title = t("integrations.canva.old.title", "The old sync script");
    parts::card(ui, title, t("integrations.canva.old.found", "Found on this Mac"), None, |ui| {
        let words = if view.settings.hold_syncs {
            t(
                "integrations.canva.old.warning.held",
                "Switch the old sync script off now. It and SoftPresenter can't both use the sign-in: whichever refreshes it next breaks the other's. Its token file couldn't be taken away, so it could still refresh it: nothing is synced until you click It's Switched Off. In Terminal on this Mac:",
            )
        } else {
            t(
                "integrations.canva.old.warning",
                "Switch the old sync script off now. It and SoftPresenter can't both use the sign-in: whichever refreshes it next breaks the other's. Its token file is gone, so it can't refresh any more, but it should not be left running. In Terminal on this Mac:",
            )
        };
        warn(ui, words);
        parts::code(ui, &view.old_steps.join("\n"));
        ui.add_space(12.0);
        let keys = [
            parts::Key::plain(t("integrations.canva.old.copy", "Copy the Steps")).hint(t(
                "quickhelp.canva.old_copy",
                "Copy the steps that switch the old script off, to paste in Terminal.",
            )),
            parts::Key::plain(t("integrations.canva.old.off", "It's Switched Off"))
                .hint(t("quickhelp.canva.old_off", "Say the old script is switched off, so syncing can go on.")),
        ];
        match parts::keys_line(ui, "canva old script", "", &keys) {
            Some(0) => ui.ctx().copy_text(view.old_steps.join("\n")),
            Some(_) => *request = Some(crate::canva::Request::OldScriptOff),
            None => {}
        }
    });
}

/// The presentation: its link with Read Pages (Read Pages Again once there is a design, Return in
/// the field too), its pages as a strip of numbered tiles to click the divider in, and what the
/// divider leaves in the video.
fn design_card(ui: &mut egui::Ui, view: &mut crate::canva::View, request: &mut Option<crate::canva::Request>) {
    use crate::canva::{Phase, Request as R};
    let about = t("integrations.canva.design.sub", "Its link and its divider page");
    parts::card(ui, t("integrations.canva.design.title", "The presentation"), about, None, |ui| {
        let busy = view.phase.busy();
        let (read, read_again) =
            (t("integrations.canva.read", "Read Pages"), t("integrations.canva.read_again", "Read Pages Again"));
        let wordings = [read, read_again];
        // A link typed is read with Read Pages; with none typed, the design chosen is read again.
        let reading_new = !view.typed_link.trim().is_empty();
        let key = match (reading_new, view.settings.design.is_some()) {
            (true, _) => parts::Key::plain(read).enabled(!busy),
            (false, true) => parts::Key::plain(read_again).enabled(!busy),
            (false, false) => parts::Key::plain(read).enabled(false),
        };
        let hint = view.settings.design.clone().unwrap_or_else(|| "https://www.canva.com/design/…".to_owned());
        let how = t(
            "integrations.canva.design.about",
            "Paste the presentation's link (in Canva, Share › Copy link), then choose its divider page.",
        );
        let (field, clicked) = parts::field_row(
            ui,
            "canva link",
            t("integrations.canva.link", "Link"),
            how,
            &mut view.typed_link,
            (false, hint.as_str()),
            230.0,
            Some(key.as_wide_as(&wordings)),
        );
        let entered = field.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
        let typed = !view.typed_link.trim().is_empty();
        if typed && (entered || (clicked && reading_new)) {
            *request = Some(R::Design(std::mem::take(&mut view.typed_link)));
        } else if clicked && !reading_new && view.settings.design.is_some() {
            *request = Some(R::ReadPages);
        }
        if view.phase == Phase::ReadingPages {
            parts::coloured_note(ui, &view.phase.words(), TEXT_DIM);
        }
        if !view.pages.is_empty()
            && let Some(id) = pages_strip(ui, view)
        {
            *request = Some(R::Divider(id));
        }
        divider_words(ui, view);
    });
}

/// The design's pages as a strip of numbered 16:9 tiles, wrapping: each its picture, the pages
/// above the divider as they are and those under it dimmed, the divider ringed and named. A click
/// makes a page the divider: its id.
fn pages_strip(ui: &mut egui::Ui, view: &mut crate::canva::View) -> Option<String> {
    const TILE: egui::Vec2 = egui::vec2(96.0, 54.0);
    const BETWEEN: f32 = 8.0;
    let crate::canva::View { pages, textures, settings, .. } = view;
    let pages: Vec<&crate::canva::PageView> = pages.iter().filter(|page| page.id.is_some()).collect();
    if pages.is_empty() {
        return None;
    }
    let width = ui.available_width();
    let across = (((width + BETWEEN) / (TILE.x + BETWEEN)).floor() as usize).max(1);
    let caps = crate::studio::fixed_semibold(11.0);
    let label_height = parts::words(ui, "0", caps.clone(), TEXT_DIM, 0.44, f32::INFINITY).size().y;
    let slot = TILE.y + 5.0 + label_height;
    let rows = pages.len().div_ceil(across);
    ui.add_space(2.0);
    let (place, _) =
        ui.allocate_exact_size(vec2(width, rows as f32 * slot + (rows - 1) as f32 * BETWEEN), Sense::hover());
    let mut chosen = None;
    for (index, page) in pages.iter().enumerate() {
        let Some(id) = page.id.clone() else { continue };
        let corner =
            place.min + vec2((index % across) as f32 * (TILE.x + BETWEEN), (index / across) as f32 * (slot + BETWEEN));
        let picture = Rect::from_min_size(corner, TILE);
        let response = ui.interact(
            Rect::from_min_size(corner, vec2(TILE.x, slot)),
            ui.scope_id().with(("canva page", &id)),
            Sense::click(),
        );
        let number = page.number.to_string();
        // Its name for a screen reader, which the pointer's tip used to say.
        let named = tf("integrations.canva.page.hint", "Make page {number} the divider", &[("number", &number)]);
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &named));
        let response = response.hint(named.as_str());
        let divider = settings.divider.as_deref() == Some(id.as_str());
        let above = settings.divider_number.is_some_and(|divider| page.number < divider);
        let below = settings.divider_number.is_some_and(|divider| page.number > divider);
        let ground = if divider {
            Color32::from_gray(0x1a)
        } else if above {
            Color32::from_gray(0x2b)
        } else {
            CARD
        };
        ui.painter().rect_filled(picture, 6.0, ground);
        if let Some(frame) = &page.picture {
            let texture = match textures.iter().find(|(have, _)| *have == id) {
                Some((_, texture)) => texture.clone(),
                None => {
                    let image = egui::ColorImage::from_rgba_unmultiplied(
                        [frame.width as usize, frame.height as usize],
                        &frame.rgba,
                    );
                    let texture =
                        ui.ctx().load_texture(format!("canva page {id}"), image, egui::TextureOptions::LINEAR);
                    textures.push((id.clone(), texture.clone()));
                    texture
                }
            };
            let aspect = frame.width as f32 / frame.height.max(1) as f32;
            let fitted =
                if aspect > TILE.x / TILE.y { vec2(TILE.x, TILE.x / aspect) } else { vec2(TILE.y * aspect, TILE.y) };
            egui::Image::new((texture.id(), fitted))
                .corner_radius(6.0)
                .paint_at(ui, Rect::from_center_size(picture.center(), fitted));
        }
        if below {
            ui.painter().rect_filled(picture, 6.0, Color32::from_black_alpha(140));
        }
        if divider {
            ui.painter().rect_stroke(picture, 6.0, Stroke::new(2.0, TEXT), egui::StrokeKind::Outside);
        } else {
            let ring = if response.hovered() { TEXT_FAINT } else { LINE };
            ui.painter().rect_stroke(picture, 6.0, Stroke::new(1.0, ring), egui::StrokeKind::Inside);
        }
        let label = if divider {
            tf("integrations.canva.page.divider", "{number} · divider", &[("number", &number)])
        } else {
            number
        };
        let ink = if divider { TEXT } else { TEXT_DIM };
        let label = parts::words(ui, &label, caps.clone(), ink, 0.44, f32::INFINITY);
        // Under its tile, kept inside the strip where it is wider than the tile.
        let left = (picture.center().x - label.size().x / 2.0).clamp(place.min.x, place.max.x - label.size().x);
        ui.painter().galley(pos2(left, picture.max.y + 5.0), label, ink);
        if response.clicked() {
            chosen = Some(id);
        }
    }
    ui.add_space(10.0);
    chosen
}

/// Which pages go in the video, in ink, then how the divider is chosen, dim, as one paragraph.
fn divider_words(ui: &mut egui::Ui, view: &crate::canva::View) {
    let said = match (view.settings.divider_number, view.settings.divider.is_some()) {
        (Some(number), true) if number > 1 => {
            let (page, last) = (number.to_string(), (number - 1).to_string());
            Some(if number == 2 {
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
            })
        }
        _ if view.settings.design.is_some() => Some(
            t("integrations.canva.divider.none", "No divider page chosen yet: read the pages and click it.").to_owned(),
        ),
        _ => None,
    };
    // How to choose it, once there are pages to click.
    let how = (!view.pages.is_empty()).then(|| {
        t(
            "integrations.canva.divider.about",
            "Click the divider page. Canva can't tell SoftPresenter which slides are hidden, so the divider says where the video stops.",
        )
    });
    if said.is_none() && how.is_none() {
        return;
    }
    let width = ui.available_width();
    let mut job = egui::text::LayoutJob::default();
    let format = |font: FontId, color: Color32| egui::TextFormat {
        font_id: font,
        color,
        line_height: Some(18.0),
        ..Default::default()
    };
    if let Some(said) = &said {
        job.append(said, 0.0, format(semibold(12.5), TEXT));
    }
    if let Some(how) = how {
        job.append(how, if said.is_some() { 4.0 } else { 0.0 }, format(FontId::proportional(12.5), TEXT_DIM));
    }
    job.wrap.max_width = width.min(620.0);
    let galley = ui.ctx().fonts_mut(|fonts| fonts.layout_job(job));
    let (place, _) = ui.allocate_exact_size(vec2(width, galley.size().y), Sense::hover());
    ui.painter().galley(place.min, galley, TEXT);
    ui.add_space(12.0);
}

/// A screen's look: a preview, a control for each setting of its mode, and Save to VoxLive.
fn look_page(ui: &mut egui::Ui, view: &mut View) {
    let schema = view.schema.clone();
    let Some(look) = view.look.as_mut() else { return };
    let mut request = None;
    if ui
        .add(egui::Button::new(egui::RichText::new("‹ VoxLive").color(TEXT_DIM)).frame(false))
        .hint(t("quickhelp.integrations.back_voxlive", "Back to VoxLive\u{2019}s page."))
        .clicked()
    {
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
                label.hint(about);
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
            .hint(t("integrations.look.save.hint", "The screen changes at once, wherever it is showing"))
            .clicked()
        {
            request = Some(Request::SaveLook { anyway: false });
        }
        if ui
            .add_enabled(changes > 0 && !look.saving, egui::Button::new(t("integrations.look.revert", "Revert")))
            .hint(t(
                "quickhelp.integrations.revert",
                "Put back the settings as VoxLive has them, dropping your changes.",
            ))
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
                .hint(t(
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
                .hint(t("integrations.look.save_mine.hint", "Your changed settings go over VoxLive's"))
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
            egui::ComboBox::from_id_salt(("voxlive look", key))
                .selected_text(words(current))
                .width(180.0)
                .show_ui(ui, |ui| {
                    for value in &values {
                        ui.selectable_value(&mut chosen, value.clone(), words(value));
                    }
                })
                .response
                .hint(t("quickhelp.integrations.look_setting", "This setting of how VoxLive\u{2019}s captions look."));
            Some(chosen)
        }
        "color" => {
            let hex = current.as_str().unwrap_or("#000000");
            let mut colour = colour_of(hex);
            ui.color_edit_button_srgba(&mut colour)
                .hint(t("quickhelp.integrations.look_setting", "This setting of how VoxLive\u{2019}s captions look."));
            Some(serde_json::Value::String(hex_of(colour, hex.len() > 7)))
        }
        "boolean" => {
            let mut on = current.as_bool().unwrap_or(false);
            ui.checkbox(&mut on, "")
                .hint(t("quickhelp.integrations.look_setting", "This setting of how VoxLive\u{2019}s captions look."));
            Some(serde_json::Value::Bool(on))
        }
        "number" => {
            let min = field.get("min").and_then(serde_json::Value::as_f64).unwrap_or(0.0);
            let max = field.get("max").and_then(serde_json::Value::as_f64).unwrap_or(100.0).max(min);
            let step = field.get("step").and_then(serde_json::Value::as_f64).unwrap_or(1.0);
            let unit = field.get("unit").and_then(serde_json::Value::as_str).unwrap_or("");
            let mut number = current.as_f64().unwrap_or(min).clamp(min, max);
            let suffix = if unit.is_empty() { String::new() } else { format!(" {unit}") };
            ui.add(egui::Slider::new(&mut number, min..=max).step_by(step).suffix(suffix))
                .hint(t("quickhelp.integrations.look_setting", "This setting of how VoxLive\u{2019}s captions look."));
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
            let mut download =
                crate::ndi_download::NdiDownload::in_folder(std::env::temp_dir().join("softpresenter-no-ndi"));
            let ndi = Ndi { download: &mut download, found: false, open: &mut false };
            let mut ask = crate::integration_request::IntegrationRequest::default();
            pane(ui, &mut voxlive, &mut canva, songbooks.as_deref_mut(), timelychurch, ndi, &mut ask);
        });
        drawn.textures_delta.clear();
        let mut words = Vec::new();
        drawn.shapes.iter().for_each(|clipped| walk(&clipped.shape, &mut words));
        words
    }

    #[test]
    fn the_songbooks_are_listed_only_where_the_catalog_is_offered() {
        crate::i18n::with_language(crate::i18n::Language::English, || {
            let without = words_drawn(None);
            assert!(without.iter().any(|w| w == "VoxLive") && without.iter().any(|w| w == "Canva Sync"), "{without:?}");
            assert!(!without.iter().any(|w| w.contains("Worship Leader App")), "{without:?}");
            assert_eq!(without.iter().filter(|w| *w == "COMING SOON").count(), 1, "TimelyChurch only: {without:?}");
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
            for words in ["Connect to TimelyChurch", "Address", "Pairing code", "Connect"] {
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
                "REMOVED IN TIMELYCHURCH",
            ] {
                assert!(connected.iter().any(|w| w == words), "{words}: {connected:?}");
            }
            assert!(!connected.iter().any(|w| w == "Pairing code"), "{connected:?}");
        });
    }

    /// The three services' views a page is drawn from.
    #[derive(Default)]
    struct Services {
        voxlive: View,
        canva: crate::canva::View,
        timelychurch: crate::timelychurch::View,
    }

    /// Draws the pane twice (the second with every size known) in a page `width` wide, with what
    /// the pointer and keys did in `events` on the second, and gives the words drawn and where.
    fn drawn(
        ctx: &egui::Context,
        services: &mut Services,
        width: f32,
        events: Vec<egui::Event>,
    ) -> Vec<(String, Rect)> {
        fn walk(shape: &egui::Shape, into: &mut Vec<(String, Rect)>) {
            match shape {
                egui::Shape::Text(text) => into.push((text.galley.text().to_owned(), text.visual_bounding_rect())),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| walk(shape, into)),
                _ => {}
            }
        }
        let mut words = Vec::new();
        for round in 0..2 {
            let raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(width, 4000.0))),
                events: if round == 1 { events.clone() } else { Vec::new() },
                ..egui::RawInput::default()
            };
            let output = ctx.run_ui(raw, |ui| {
                egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| {
                    let Services { voxlive, canva, timelychurch } = services;
                    let mut download =
                        crate::ndi_download::NdiDownload::in_folder(std::env::temp_dir().join("softpresenter-no-ndi"));
                    let ndi = Ndi { download: &mut download, found: false, open: &mut false };
                    let mut ask = crate::integration_request::IntegrationRequest::default();
                    pane(ui, voxlive, canva, None, timelychurch, ndi, &mut ask);
                });
            });
            words.clear();
            output.shapes.iter().for_each(|clipped| walk(&clipped.shape, &mut words));
            output.drop_without_applying_deltas();
        }
        words
    }

    fn has(words: &[(String, Rect)], wanted: &str) -> bool {
        words.iter().any(|(said, _)| said == wanted)
    }

    /// Where the words `wanted` are drawn (the last place, when there are several).
    fn at(words: &[(String, Rect)], wanted: &str) -> Rect {
        words.iter().rev().find(|(said, _)| said == wanted).map(|(_, rect)| *rect).unwrap_or_else(|| {
            panic!("{wanted} isn't drawn: {:?}", words.iter().map(|(said, _)| said).collect::<Vec<_>>())
        })
    }

    /// Presses and lets go at `point`.
    fn click_at(ctx: &egui::Context, services: &mut Services, point: egui::Pos2) {
        let press = |pressed| egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        drawn(ctx, services, 740.0, vec![egui::Event::PointerMoved(point), press(true)]);
        drawn(ctx, services, 740.0, vec![press(false)]);
    }

    /// Clicks the words `name` where the page draws them.
    fn click(ctx: &egui::Context, services: &mut Services, name: &str) {
        let words = drawn(ctx, services, 740.0, vec![]);
        click_at(ctx, services, at(&words, name).center());
    }

    /// Clicks into the field at the right end of the row named `label` (or, for a field with a
    /// key after it, on the words `label` drawn in it while it is empty), and types `text`.
    fn type_into(ctx: &egui::Context, services: &mut Services, label: &str, text: &str) {
        let words = drawn(ctx, services, 740.0, vec![]);
        let place = at(&words, label);
        // A field alone is 190 wide at the row's right end, inside the card's margin.
        let point =
            if place.min.x > 300.0 { place.center() } else { egui::pos2(740.0 - 18.0 - 60.0, place.center().y) };
        click_at(ctx, services, point);
        drawn(ctx, services, 740.0, vec![egui::Event::Text(text.to_owned())]);
    }

    fn press_return(ctx: &egui::Context, services: &mut Services) {
        let key = egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        drawn(ctx, services, 740.0, vec![key]);
    }

    /// Whether Disconnect asked its question; the question is closed again, so the page takes
    /// clicks after it.
    fn asked_to_disconnect(ctx: &egui::Context, service: &str) -> bool {
        let asked = ctx.data(|data| data.get_temp::<bool>(disconnecting_id(service))).unwrap_or(false);
        ctx.data_mut(|data| data.remove::<bool>(disconnecting_id(service)));
        asked
    }

    fn screen(id: &str, name: &str, language: &str, mode: &str, url: &str) -> crate::voxlive::Screen {
        crate::voxlive::Screen {
            id: id.to_owned(),
            name: name.to_owned(),
            language: language.to_owned(),
            mode: mode.to_owned(),
            url: url.to_owned(),
            ..Default::default()
        }
    }

    /// VoxLive connected, with a screen whose look can be changed and one whose can't.
    fn voxlive_connected() -> Services {
        let mut services = Services::default();
        let voxlive = &mut services.voxlive;
        voxlive.open = true;
        voxlive.status = Status::Connected;
        voxlive.key_hint = Some("vxk_live_abc…".to_owned());
        voxlive.screens = vec![
            screen("one", "Main captions", "ru", "captions", "https://voxlive.app/display/one"),
            screen("two", "Scripture", "uk", "reference", ""),
        ];
        voxlive.schema = vec![("bg".to_owned(), serde_json::json!({"type": "color"}))];
        services
    }

    /// Canva Sync signed in, its design read, page 3 of 5 the divider, and a video made.
    fn canva_connected() -> Services {
        let mut services = Services::default();
        let canva = &mut services.canva;
        canva.open = true;
        canva.status = crate::canva::Status::Connected;
        canva.client_hint = Some("OC-AZq3…".to_owned());
        canva.redirect = "http://127.0.0.1:47812/canva".to_owned();
        canva.settings.design = Some("DAtestDesign1".to_owned());
        canva.settings.divider = Some("p3".to_owned());
        canva.settings.divider_number = Some(3);
        canva.settings.updated_at = Some("2026-11-29T08:12:00+00:00".to_owned());
        canva.settings.updated_file = Some("Announcements 2026-11-29 0812.mp4".to_owned());
        canva.pages = (1..=5)
            .map(|number| crate::canva::PageView { number, id: Some(format!("p{number}")), picture: None })
            .collect();
        services
    }

    fn canva_signed_out() -> Services {
        let mut services = Services::default();
        let canva = &mut services.canva;
        canva.open = true;
        canva.status = crate::canva::Status::NotConnected;
        canva.redirect = "http://127.0.0.1:47812/canva".to_owned();
        canva.old_here = true;
        services
    }

    /// TimelyChurch connected, with a service in each state.
    fn timelychurch_connected() -> Services {
        use crate::timelychurch::{RowState, ServiceRow};
        let mut services = Services::default();
        let card = &mut services.timelychurch;
        card.offered = true;
        card.open = true;
        card.status = crate::timelychurch::Status::Connected;
        card.church = "Demo Church".to_owned();
        card.address = "https://demo.timelychurch.test".to_owned();
        card.uses_waiting = 1;
        let row = |name: &str, date: &str, songs, state| ServiceRow {
            name: name.to_owned(),
            date: Some(date.to_owned()),
            songs,
            state,
        };
        card.services = vec![
            row("Sunday Morning", "2026-11-29", 5, RowState::InStep),
            row("Prayer Meeting", "2026-12-02", 3, RowState::ChangedHere),
            row("Sunday Evening", "2026-12-06", 6, RowState::NewerWaiting),
            row("Youth Night", "2026-12-04", 2, RowState::Removed),
        ];
        services
    }

    fn timelychurch_signed_out() -> Services {
        let mut services = Services::default();
        let card = &mut services.timelychurch;
        card.offered = true;
        card.open = true;
        card.status = crate::timelychurch::Status::NotConnected;
        card.asks_address = true;
        services
    }

    #[test]
    fn voxlive_shows_its_screens_then_the_connection_and_every_hint_as_a_line() {
        crate::i18n::with_language(crate::i18n::Language::English, || {
            let ctx = crate::studio::tests::styled();
            let mut services = voxlive_connected();
            let words = drawn(&ctx, &mut services, 740.0, vec![]);
            for wanted in [
                "Your VoxLive screens",
                "2 SCREENS",
                "Main captions",
                "Captions · RU · Change how this screen looks, here; it is saved to VoxLive",
                "Scripture · UK",
                "Connection",
                "THE KEY, KEPT ON THIS COMPUTER",
                "Key",
                "vxk_live_abc…",
                "Find Screens Again",
                "Disconnect",
                "Disconnect forgets the key on this computer (it stays in VoxLive until you revoke it there)",
            ] {
                assert!(has(&words, wanted), "{wanted}");
            }
            assert_eq!(
                words.iter().filter(|(said, _)| said == "Change Look ›").count(),
                1,
                "a screen without a page has none"
            );
            assert!(
                at(&words, "Your VoxLive screens").max.y < at(&words, "Connection").min.y,
                "the screens come first"
            );
            assert!(
                !has(&words, "With a key made in VoxLive".to_uppercase().as_str()),
                "no Connect while the key is good"
            );
            // A key VoxLive refused: why, in the connection, and Connect to paste another.
            services.voxlive.status = Status::Refused("VoxLive doesn't know that key.".to_owned());
            let words = drawn(&ctx, &mut services, 740.0, vec![]);
            assert!(has(&words, "VoxLive doesn't know that key.") && has(&words, "WITH A KEY MADE IN VOXLIVE"));
            // No key: only Connect.
            services.voxlive.status = Status::NoKey;
            services.voxlive.key_hint = None;
            services.voxlive.problem = Some("That isn't a VoxLive API key.".to_owned());
            let words = drawn(&ctx, &mut services, 740.0, vec![]);
            assert!(has(&words, "Connect") && has(&words, "WITH A KEY MADE IN VOXLIVE") && has(&words, "vxk_live_…"));
            assert!(has(&words, "That isn't a VoxLive API key."));
            assert!(!has(&words, "Your VoxLive screens") && !has(&words, "Find Screens Again"));
        });
    }

    #[test]
    fn voxlives_keys_ask_the_app_what_they_asked_before() {
        crate::i18n::with_language(crate::i18n::Language::English, || {
            let ctx = crate::studio::tests::styled();
            let mut services = voxlive_connected();
            click(&ctx, &mut services, "Change Look ›");
            assert_eq!(services.voxlive.request.take(), Some(Request::OpenLook("one".to_owned())));
            click(&ctx, &mut services, "Find Screens Again");
            assert_eq!(services.voxlive.request.take(), Some(Request::Refresh));
            click(&ctx, &mut services, "Disconnect");
            assert_eq!(services.voxlive.request, None, "it asks first");
            assert!(asked_to_disconnect(&ctx, "voxlive"));
            // With no key: typing one and Connect, or Return in the field.
            let mut services = Services::default();
            services.voxlive.open = true;
            click(&ctx, &mut services, "Connect");
            assert_eq!(services.voxlive.request, None, "nothing typed, nothing asked");
            type_into(&ctx, &mut services, "vxk_live_…", "vxk_test_key");
            click(&ctx, &mut services, "Connect");
            assert_eq!(services.voxlive.request.take(), Some(Request::Connect("vxk_test_key".to_owned())));
            assert!(services.voxlive.typed.is_empty(), "the key isn't kept once asked");
            type_into(&ctx, &mut services, "vxk_live_…", "vxk_other_key");
            press_return(&ctx, &mut services);
            assert_eq!(services.voxlive.request.take(), Some(Request::Connect("vxk_other_key".to_owned())));
        });
    }

    #[test]
    fn canva_sync_shows_the_sync_the_presentation_where_it_goes_and_the_account() {
        crate::i18n::with_language(crate::i18n::Language::English, || {
            let ctx = crate::studio::tests::styled();
            let mut services = canva_connected();
            let words = drawn(&ctx, &mut services, 740.0, vec![]);
            for wanted in [
                "Sync",
                "THE ANNOUNCEMENTS VIDEO",
                "Announcements 2026-11-29 0812.mp4",
                "Sync Now",
                "The presentation",
                "ITS LINK AND ITS DIVIDER PAGE",
                "Link",
                "Read Pages Again",
                "1",
                "2",
                "3 · divider",
                "4",
                "5",
                "Where it goes",
                "Account",
                "Signed in",
                "integration OC-AZq3… · Disconnect forgets the sign-in on this computer (it stays in Canva until you take it back there)",
            ] {
                assert!(has(&words, wanted), "{wanted}");
            }
            assert!(words.iter().any(|(said, _)| said.starts_with("Last updated ")));
            assert!(words.iter().any(|(said, _)| said.starts_with("Divider: page 3. Pages 1–2 go in the video")));
            let order = ["Sync", "The presentation", "Where it goes", "Account"].map(|title| at(&words, title).min.y);
            assert!(order.is_sorted(), "{order:?}");
            assert!(!has(&words, "The old sync script"), "only while it must be switched off");
            // Nothing chosen yet: Sync Now waits for the design.
            services.canva.settings = crate::canva::Settings::default();
            services.canva.pages.clear();
            let words = drawn(&ctx, &mut services, 740.0, vec![]);
            assert!(
                has(&words, "CHOOSE THE DESIGN FIRST") && has(&words, "Not synced yet") && has(&words, "Read Pages")
            );
        });
    }

    #[test]
    fn canva_syncs_keys_ask_the_app_what_they_asked_before() {
        use crate::canva::{Phase, Request as R};
        crate::i18n::with_language(crate::i18n::Language::English, || {
            let ctx = crate::studio::tests::styled();
            let mut services = canva_connected();
            click(&ctx, &mut services, "Sync Now");
            assert!(matches!(services.canva.request.take(), Some(R::SyncNow)));
            click(&ctx, &mut services, "Read Pages Again");
            assert!(matches!(services.canva.request.take(), Some(R::ReadPages)));
            click(&ctx, &mut services, "2");
            assert!(matches!(services.canva.request.take(), Some(R::Divider(id)) if id == "p2"));
            type_into(&ctx, &mut services, "DAtestDesign1", "https://www.canva.com/design/DAnew/view");
            press_return(&ctx, &mut services);
            assert!(
                matches!(services.canva.request.take(), Some(R::Design(link)) if link == "https://www.canva.com/design/DAnew/view")
            );
            type_into(&ctx, &mut services, "DAtestDesign1", "https://www.canva.com/design/DAother/view");
            click(&ctx, &mut services, "Read Pages");
            assert!(
                matches!(services.canva.request.take(), Some(R::Design(link)) if link == "https://www.canva.com/design/DAother/view")
            );
            assert!(services.canva.typed_link.is_empty(), "the link typed is taken once asked");
            click(&ctx, &mut services, "Disconnect");
            assert!(services.canva.request.is_none(), "it asks first");
            assert!(asked_to_disconnect(&ctx, "canva"));
            // Syncing: what it is doing, and Stop with its line.
            services.canva.phase = Phase::Starting;
            let words = drawn(&ctx, &mut services, 740.0, vec![]);
            assert!(has(&words, "Starting…") && has(&words, "Stop: the video in the media bin stays as it was"));
            click(&ctx, &mut services, "Stop");
            assert!(matches!(services.canva.request.take(), Some(R::Stop)));
            // The old script to switch off: first on the page, and nothing synced while it holds.
            services.canva.phase = Phase::Idle;
            services.canva.settings.old_script_warning = true;
            services.canva.settings.hold_syncs = true;
            services.canva.old_steps = vec!["launchctl unload old-script.plist".to_owned()];
            let words = drawn(&ctx, &mut services, 740.0, vec![]);
            assert!(at(&words, "The old sync script").max.y < at(&words, "Sync").min.y);
            click(&ctx, &mut services, "Sync Now");
            assert!(services.canva.request.take().is_none());
            click(&ctx, &mut services, "It's Switched Off");
            assert!(matches!(services.canva.request.take(), Some(R::OldScriptOff)));
        });
    }

    #[test]
    fn canva_signed_out_connects_with_the_churchs_own_integration() {
        use crate::canva::Request as R;
        crate::i18n::with_language(crate::i18n::Language::English, || {
            let ctx = crate::studio::tests::styled();
            let mut services = canva_signed_out();
            let words = drawn(&ctx, &mut services, 740.0, vec![]);
            for wanted in [
                "Connect",
                "CANVA SYNC USES AN INTEGRATION YOUR CHURCH MAKES IN CANVA",
                "Redirect URL",
                "http://127.0.0.1:47812/canva",
                "Copy",
                "Client ID",
                "Client secret",
                "Opens Canva's sign-in in your browser",
                "Sign In with Canva",
                "The old sync script",
                "Use the Sign-in from the Old Sync Script",
            ] {
                assert!(has(&words, wanted), "{wanted}");
            }
            click(&ctx, &mut services, "Sign In with Canva");
            assert!(services.canva.request.take().is_none(), "greyed until both are typed");
            type_into(&ctx, &mut services, "Client ID", "OC-test-client");
            type_into(&ctx, &mut services, "Client secret", "a-secret");
            click(&ctx, &mut services, "Sign In with Canva");
            assert!(
                matches!(services.canva.request.take(), Some(R::SignIn(id, secret)) if id == "OC-test-client" && secret == "a-secret")
            );
            assert!(services.canva.typed_secret.is_empty(), "the secret isn't kept once asked");
            click(&ctx, &mut services, "Use the Sign-in from the Old Sync Script");
            assert!(matches!(services.canva.request.take(), Some(R::Import)));
            // Waiting for the browser: what it waits for, and Stop.
            services.canva.phase = crate::canva::Phase::SigningIn;
            click(&ctx, &mut services, "Stop");
            assert!(matches!(services.canva.request.take(), Some(R::Stop)));
        });
    }

    #[test]
    fn timelychurch_shows_its_services_then_the_connection() {
        crate::i18n::with_language(crate::i18n::Language::English, || {
            let ctx = crate::studio::tests::styled();
            let mut services = timelychurch_connected();
            let words = drawn(&ctx, &mut services, 740.0, vec![]);
            for wanted in [
                "Upcoming services",
                "4 IN TIMELYCHURCH",
                "Sunday Morning",
                "UP TO DATE",
                "CHANGED HERE",
                "CHANGED HERE; NEWER IN TIMELYCHURCH",
                "REMOVED IN TIMELYCHURCH",
                "Connection",
                "CONNECTED",
                "Connected to Demo Church · not checked yet",
                "https://demo.timelychurch.test",
                "Check Now",
                "Disconnect",
                "1 song shown is waiting to be told to TimelyChurch.",
            ] {
                assert!(has(&words, wanted), "{wanted}");
            }
            assert!(words.iter().any(|(said, _)| said.ends_with(" · 5 songs")), "the date and the songs");
            assert!(has(
                &words,
                "Check Now asks TimelyChurch for changes now (it is asked every few minutes anyway). Disconnect forgets the connection on this computer; the songs and playlists it made stay"
            ));
            assert!(at(&words, "Upcoming services").max.y < at(&words, "Connection").min.y);
            click(&ctx, &mut services, "Check Now");
            assert_eq!(services.timelychurch.request.take(), Some(crate::timelychurch::Request::CheckNow));
            click(&ctx, &mut services, "Disconnect");
            assert_eq!(services.timelychurch.request, None, "it asks first");
            assert!(asked_to_disconnect(&ctx, "timelychurch"));
            // Checking now: Check Now is greyed.
            services.timelychurch.checking = true;
            click(&ctx, &mut services, "Check Now");
            assert_eq!(services.timelychurch.request.take(), None);
        });
    }

    #[test]
    fn timelychurch_connects_with_the_address_and_the_pairing_code() {
        use crate::timelychurch::Request as R;
        crate::i18n::with_language(crate::i18n::Language::English, || {
            let ctx = crate::studio::tests::styled();
            let mut services = timelychurch_signed_out();
            let words = drawn(&ctx, &mut services, 740.0, vec![]);
            assert!(has(&words, "Connect to TimelyChurch") && has(&words, "WITH A PAIRING CODE"));
            assert!(has(&words, "yourchurch.timelychurch.com"), "the address's hint");
            click(&ctx, &mut services, "Connect");
            assert_eq!(services.timelychurch.request.take(), None, "greyed until both are typed");
            type_into(&ctx, &mut services, "Address", "http://127.0.0.1:4320");
            type_into(&ctx, &mut services, "Pairing code", "DEMO-2026");
            press_return(&ctx, &mut services);
            let connect = R::Connect { address: "http://127.0.0.1:4320".to_owned(), code: "DEMO-2026".to_owned() };
            assert_eq!(services.timelychurch.request.take(), Some(connect.clone()));
            assert!(services.timelychurch.typed_code.is_empty(), "the code isn't kept once asked");
            type_into(&ctx, &mut services, "Pairing code", "DEMO-2026");
            click(&ctx, &mut services, "Connect");
            assert_eq!(services.timelychurch.request.take(), Some(connect));
            // Connecting: said beside the key, which is greyed.
            services.timelychurch.status = crate::timelychurch::Status::Pairing;
            type_into(&ctx, &mut services, "Pairing code", "DEMO-2026");
            let words = drawn(&ctx, &mut services, 740.0, vec![]);
            assert!(has(&words, "Connecting…"));
            click(&ctx, &mut services, "Connect");
            assert_eq!(services.timelychurch.request.take(), None);
        });
    }

    /// Every state of the three pages the tests draw.
    fn every_page() -> Vec<(&'static str, Services)> {
        let mut refused = voxlive_connected();
        refused.voxlive.status = Status::Refused("VoxLive doesn't know that key: check it in VoxLive.".to_owned());
        let mut out_of_reach = voxlive_connected();
        out_of_reach.voxlive.status = Status::Trouble(crate::notices::unreachable("VoxLive"));
        let mut signed_out = Services::default();
        signed_out.voxlive.open = true;
        let mut warned = canva_connected();
        warned.canva.settings.old_script_warning = true;
        warned.canva.old_steps = vec!["launchctl bootout gui/501/com.church.canva-sync".to_owned()];
        warned.canva.status = crate::canva::Status::Doubtful("Canva may have used up the sign-in.".to_owned());
        let mut syncing = canva_connected();
        syncing.canva.phase = crate::canva::Phase::Downloading { done: 64, total: Some(100) };
        let mut signing_in = canva_signed_out();
        signing_in.canva.phase = crate::canva::Phase::SigningIn;
        let mut trouble = timelychurch_connected();
        trouble.timelychurch.trouble = Some(crate::notices::unreachable("TimelyChurch"));
        vec![
            ("voxlive", voxlive_connected()),
            ("voxlive refused", refused),
            ("voxlive out of reach", out_of_reach),
            ("voxlive signed out", signed_out),
            ("canva", canva_connected()),
            ("canva warned", warned),
            ("canva syncing", syncing),
            ("canva signed out", canva_signed_out()),
            ("canva signing in", signing_in),
            ("timelychurch", timelychurch_connected()),
            ("timelychurch trouble", trouble),
            ("timelychurch signed out", timelychurch_signed_out()),
        ]
    }

    #[test]
    fn every_page_fits_the_narrowest_settings_page_with_nothing_drawn_over_anything() {
        use crate::i18n::Language;
        for language in Language::ALL {
            crate::i18n::with_language(language, || {
                for (name, mut services) in every_page() {
                    // The least Settings window leaves its page 490 wide; the usual, 740.
                    for width in [490.0, 740.0] {
                        let ctx = crate::studio::tests::styled();
                        let words: Vec<(String, Rect)> = drawn(&ctx, &mut services, width, vec![])
                            .into_iter()
                            .filter(|(said, _)| !said.trim().is_empty())
                            .collect();
                        for (said, rect) in &words {
                            assert!(
                                rect.max.x <= width + 0.5,
                                "{language:?} {name} at {width}: \u{201c}{said}\u{201d} ends at {}",
                                rect.max.x
                            );
                        }
                        for (index, (first, one)) in words.iter().enumerate() {
                            for (second, other) in &words[index + 1..] {
                                assert!(
                                    !one.shrink(0.5).intersects(other.shrink(0.5)),
                                    "{language:?} {name} at {width}: \u{201c}{first}\u{201d} {one:?} and \u{201c}{second}\u{201d} {other:?} overlap"
                                );
                            }
                        }
                    }
                }
            });
        }
    }

    #[test]
    fn a_row_with_a_field_keeps_its_height() {
        crate::i18n::with_language(crate::i18n::Language::Russian, || {
            let ctx = crate::studio::tests::styled();
            for width in [490.0, 740.0] {
                let mut services = canva_signed_out();
                let words = drawn(&ctx, &mut services, width, vec![]);
                let (id, secret) = (
                    at(&words, t("integrations.canva.client_id", "Client ID")),
                    at(&words, t("integrations.canva.client_secret", "Client secret")),
                );
                assert!(
                    secret.min.y >= id.max.y + 24.0,
                    "at {width}: the secret's row at {} under {}",
                    secret.min.y,
                    id.max.y
                );
                let sign_in = at(&words, t("integrations.canva.sign_in", "Sign In with Canva"));
                assert!(sign_in.min.y >= secret.max.y + 12.0, "at {width}");
                let mut services = Services::default();
                services.voxlive.open = true;
                let words = drawn(&ctx, &mut services, width, vec![]);
                let key = at(&words, t("integrations.voxlive.key", "Key"));
                let how = at(&words, how_to_get_a_key());
                assert!(how.min.y >= key.max.y - 0.5, "at {width}: the row's line is under its name");
            }
        });
    }
}
