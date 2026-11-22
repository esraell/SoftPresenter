//! Settings › Integrations: the services SoftPresenter works with, each a row with its state,
//! opening a page of its own to connect it: VoxLive, and Canva Sync.

use egui::{Align, Align2, Color32, FontId, Layout, Rect, Sense, Stroke, pos2, vec2};

use crate::studio::{CARD, HOVER, LINE, TEXT, TEXT_DIM, TEXT_FAINT};
use crate::voxlive::{Status, View};

/// VoxLive's own colour, for its mark; and its lighter shade, for words on a dark ground.
pub const VOXLIVE: Color32 = Color32::from_rgb(44, 93, 124);
pub const VOXLIVE_LIGHT: Color32 = Color32::from_rgb(127, 169, 198);

/// The green of something connected.
const GOOD: Color32 = Color32::from_rgb(52, 199, 89);

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
pub fn pane(ui: &mut egui::Ui, view: &mut View, canva: &mut crate::canva::View) {
    if view.look.is_some() {
        look_page(ui, view);
    } else if view.open {
        voxlive_page(ui, view);
    } else if canva.open {
        canva_page(ui, canva);
    } else {
        list(ui, view, canva);
    }
}

/// A service's mark: a rounded square of its colour with its initial.
fn mark(ui: &mut egui::Ui, size: f32, colour: Color32, letter: &str) {
    let (place, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    ui.painter().rect_filled(place, size * 0.24, colour);
    ui.painter().text(
        place.center(),
        Align2::CENTER_CENTER,
        letter,
        crate::studio::semibold(size * 0.52),
        Color32::WHITE,
    );
}

/// A dot and a word or two for how a service stands.
fn state_words(status: &Status) -> (Color32, String) {
    match status {
        Status::Connected => (GOOD, "Connected".to_owned()),
        Status::Checking => (TEXT_FAINT, "Connecting…".to_owned()),
        Status::NoKey => (TEXT_FAINT, "Not connected".to_owned()),
        Status::Refused(_) => (crate::studio::TALLY, "Key refused".to_owned()),
        Status::Trouble(_) => (Color32::from_rgb(230, 170, 60), "Can't reach it".to_owned()),
    }
}

fn list(ui: &mut egui::Ui, view: &mut View, canva: &mut crate::canva::View) {
    ui.label(egui::RichText::new("Services SoftPresenter works with. Click one to connect it.").color(TEXT_DIM));
    ui.add_space(12.0);
    let voxlive = (VOXLIVE, "V", "VoxLive", "Live translation of the sermon, as captions on your screens");
    if service_row(ui, voxlive, state_words(&view.status), "Connect VoxLive, or see its screens") {
        view.open = true;
    }
    ui.add_space(8.0);
    let about = (CANVA, "C", "Canva Sync", "The announcements video, fresh from your Canva presentation");
    if service_row(ui, about, canva_state(canva), "Connect Canva Sync, choose the presentation, or sync now") {
        canva.open = true;
    }
}

/// A service's row in the list: its mark, name and what it does, and how it stands. True when
/// clicked.
fn service_row(
    ui: &mut egui::Ui,
    (colour, letter, name, about): (Color32, &str, &str, &str),
    (dot, words): (Color32, String),
    hint: &str,
) -> bool {
    let width = ui.available_width().min(560.0);
    let (row, response) = ui.allocate_exact_size(vec2(width, 60.0), Sense::click());
    ui.painter().rect_filled(row, 10.0, if response.hovered() { HOVER } else { CARD });
    let mut inside = ui.new_child(
        egui::UiBuilder::new().max_rect(row.shrink2(vec2(14.0, 10.0))).layout(Layout::left_to_right(Align::Center)),
    );
    mark(&mut inside, 36.0, colour, letter);
    inside.add_space(10.0);
    inside.vertical(|ui| {
        ui.add_space(2.0);
        ui.label(egui::RichText::new(name).font(crate::studio::semibold(14.0)).color(TEXT));
        ui.label(egui::RichText::new(about).size(11.5).color(TEXT_DIM));
    });
    let right = Rect::from_min_max(row.right_top() - vec2(170.0, 0.0), row.max - vec2(14.0, 0.0));
    ui.painter().text(
        right.right_center() - vec2(0.0, 0.0),
        Align2::RIGHT_CENTER,
        "›",
        FontId::proportional(18.0),
        TEXT_FAINT,
    );
    let text = ui.painter().text(
        right.right_center() - vec2(18.0, 0.0),
        Align2::RIGHT_CENTER,
        &words,
        FontId::proportional(12.0),
        TEXT_DIM,
    );
    ui.painter().circle_filled(text.left_center() - vec2(9.0, 0.0), 3.5, dot);
    response.on_hover_text(hint).clicked()
}

fn voxlive_page(ui: &mut egui::Ui, view: &mut View) {
    if ui.add(egui::Button::new(egui::RichText::new("‹ Integrations").color(TEXT_DIM)).frame(false)).clicked() {
        view.open = false;
        view.typed.clear();
        view.problem = None;
    }
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        mark(ui, 44.0, VOXLIVE, "V");
        ui.add_space(6.0);
        ui.vertical(|ui| {
            ui.label(egui::RichText::new("VoxLive").font(crate::studio::semibold(18.0)).color(TEXT));
            let (dot, words) = state_words(&view.status);
            ui.horizontal(|ui| {
                let (place, _) = ui.allocate_exact_size(vec2(8.0, 14.0), Sense::hover());
                ui.painter().circle_filled(place.center(), 3.5, dot);
                ui.label(egui::RichText::new(words).size(12.0).color(TEXT_DIM));
            });
        });
    });
    ui.add_space(10.0);
    let width = ui.available_width().min(560.0);
    ui.set_max_width(width);
    ui.label(
        egui::RichText::new(
            "VoxLive translates the sermon as it is preached, and shows it as captions on its screens. Connected, \
             SoftPresenter shows your church's VoxLive screens as the slides of a VoxLive item (under LIBRARY, and \
             in playlists) and in the VoxLive tab beside the screens: one click puts the captions on screen in place \
             of a verse, and a click on the verse brings it back.",
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
                ui.label(egui::RichText::new("Key").color(TEXT_DIM));
                ui.label(egui::RichText::new(hint.as_str()).monospace().color(TEXT));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .button("Disconnect")
                        .on_hover_text(
                            "Forget the key on this computer (it stays in VoxLive until you revoke it there)",
                        )
                        .clicked()
                    {
                        view.request = Some(Request::Disconnect);
                    }
                    if ui.button("Find Screens Again").clicked() {
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
}

/// Canva's colour, for Canva Sync's mark, and a lighter shade for words on a dark ground.
pub const CANVA: Color32 = Color32::from_rgb(0, 160, 170);
pub const CANVA_LIGHT: Color32 = Color32::from_rgb(110, 214, 220);

/// A dot and a word or two for how Canva Sync stands.
fn canva_state(view: &crate::canva::View) -> (Color32, String) {
    use crate::canva::Status as S;
    match &view.status {
        S::Reading => (TEXT_FAINT, "…".to_owned()),
        S::NotConnected => (TEXT_FAINT, "Not connected".to_owned()),
        S::Refused(_) => (crate::studio::TALLY, "Sign-in refused".to_owned()),
        S::Doubtful(_) => (Color32::from_rgb(230, 170, 60), "Sign-in in doubt".to_owned()),
        S::Connected if view.phase.busy() => (Color32::from_rgb(230, 170, 60), "Syncing".to_owned()),
        S::Connected if view.settings.design.is_none() || view.settings.divider.is_none() => {
            (GOOD, "Connected: choose the design".to_owned())
        }
        S::Connected => (GOOD, "Connected".to_owned()),
    }
}

/// Canva Sync's page: connect, choose the design and its divider page, where the video goes,
/// and Sync Now.
fn canva_page(ui: &mut egui::Ui, view: &mut crate::canva::View) {
    use crate::canva::{Request as R, Status as S};
    if ui.add(egui::Button::new(egui::RichText::new("‹ Integrations").color(TEXT_DIM)).frame(false)).clicked() {
        view.open = false;
        view.typed_secret.clear();
    }
    ui.add_space(6.0);
    let (dot, words) = canva_state(view);
    ui.horizontal(|ui| {
        mark(ui, 44.0, CANVA, "C");
        ui.add_space(6.0);
        ui.vertical(|ui| {
            ui.label(egui::RichText::new("Canva Sync").font(crate::studio::semibold(18.0)).color(TEXT));
            ui.horizontal(|ui| {
                let (place, _) = ui.allocate_exact_size(vec2(8.0, 14.0), Sense::hover());
                ui.painter().circle_filled(place.center(), 3.5, dot);
                ui.label(egui::RichText::new(words).size(12.0).color(TEXT_DIM));
            });
        });
    });
    ui.add_space(10.0);
    let width = ui.available_width().min(560.0);
    ui.set_max_width(width);
    ui.label(
        egui::RichText::new(
            "Keeps the announcements video fresh from your church's Canva presentation. The pages above a divider \
             page are exported as a 1080p video into the library's Media › Canva Sync folder, which the media \
             playlist Announcements follows, so the video joins the media bin by itself. Nothing happens while \
             SoftPresenter is closed.",
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
            ui.label(egui::RichText::new("Reading the sign-in kept on this computer…").color(TEXT_FAINT));
        }
        S::NotConnected | S::Refused(_) => canva_connect(ui, view, &mut request),
        S::Connected | S::Doubtful(_) => {
            if let S::Doubtful(why) = &view.status {
                ui.label(egui::RichText::new(why.as_str()).color(Color32::from_rgb(230, 170, 60)));
                ui.add_space(4.0);
            }
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Signed in").color(TEXT_DIM));
                if let Some(hint) = &view.client_hint {
                    ui.label(egui::RichText::new(format!("integration {hint}")).monospace().color(TEXT_FAINT));
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .button("Disconnect")
                        .on_hover_text(
                            "Forget the sign-in on this computer (it stays in Canva until you take it back there)",
                        )
                        .clicked()
                    {
                        request = Some(R::Disconnect);
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
            crate::studio::caption(ui, "SYNC");
            ui.horizontal(|ui| {
                let ready =
                    view.settings.design.is_some() && view.settings.divider.is_some() && !view.settings.hold_syncs;
                if ui.add_enabled(ready && !view.phase.busy(), egui::Button::new("Sync Now")).clicked() {
                    request = Some(R::SyncNow);
                }
                if view.phase.busy() {
                    ui.label(egui::RichText::new(view.phase.words()).color(CANVA_LIGHT));
                    if ui.button("Stop").on_hover_text("The video in the media bin stays as it was").clicked() {
                        request = Some(R::Stop);
                    }
                }
            });
            if let (Some(at), Some(file)) = (&view.settings.updated_at, &view.settings.updated_file) {
                let when = chrono::DateTime::parse_from_rfc3339(at)
                    .map(|at| at.with_timezone(&chrono::Local).format("%A %-d %B, %-I:%M %p").to_string())
                    .unwrap_or_default();
                ui.label(egui::RichText::new(format!("Last updated {when}: {file}")).size(12.0).color(TEXT_DIM));
            }
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(
                    "To fetch a fresh video each time SoftPresenter opens, make a macro with the step Sync from Canva \
                     and tick Run When SoftPresenter Opens. A new video replaces the old one; one playing takes the \
                     new one over at the end of its pass, so the loop carries on.",
                )
                .size(11.5)
                .color(TEXT_FAINT),
            );
        }
    }
    if request.is_some() {
        view.request = request;
    }
}

/// How to connect: the integration's ID and secret and Sign In, or the old script's sign-in.
fn canva_connect(ui: &mut egui::Ui, view: &mut crate::canva::View, request: &mut Option<crate::canva::Request>) {
    use crate::canva::{Phase, Request as R};
    if let crate::canva::Status::Refused(why) = &view.status {
        ui.label(egui::RichText::new(why.as_str()).color(crate::studio::TALLY));
        ui.add_space(6.0);
    }
    crate::studio::caption(ui, "CONNECT");
    ui.label(
        egui::RichText::new(
            "Make an integration of your church's own in Canva's developer portal (canva.com/developers, Your \
             integrations). Under Scopes tick design:content:read; under Authentication add the redirect URL below; \
             then copy its client ID and make a secret. The secret and the sign-in are kept in this Mac's Keychain \
             and sent only to Canva.",
        )
        .size(12.0)
        .color(TEXT_DIM),
    );
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(view.redirect.as_str()).monospace().color(TEXT));
        if ui.small_button("Copy").clicked() {
            ui.ctx().copy_text(view.redirect.clone());
        }
    });
    ui.add_space(6.0);
    egui::Grid::new("canva connect").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
        ui.label(egui::RichText::new("Client ID").color(TEXT_DIM));
        ui.add(egui::TextEdit::singleline(&mut view.typed_id).desired_width(300.0));
        ui.end_row();
        ui.label(egui::RichText::new("Client secret").color(TEXT_DIM));
        ui.add(egui::TextEdit::singleline(&mut view.typed_secret).password(true).desired_width(300.0));
        ui.end_row();
    });
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        if view.phase == Phase::SigningIn {
            ui.label(egui::RichText::new(view.phase.words()).color(CANVA_LIGHT));
            if ui.button("Stop").clicked() {
                *request = Some(R::Stop);
            }
        } else {
            let filled = !view.typed_id.trim().is_empty() && !view.typed_secret.trim().is_empty();
            if ui
                .add_enabled(filled && !view.phase.busy(), egui::Button::new("Sign In with Canva"))
                .on_hover_text("Opens Canva's sign-in in your browser")
                .clicked()
            {
                *request = Some(R::SignIn(view.typed_id.trim().to_owned(), std::mem::take(&mut view.typed_secret)));
            }
        }
    });
    if view.old_here {
        ui.add_space(12.0);
        crate::studio::caption(ui, "THE OLD SYNC SCRIPT");
        ui.label(
            egui::RichText::new(
                "This Mac has the sign-in of the old Canva sync script. SoftPresenter can take it over: it reads it \
                 from there itself, keeps it in the Keychain, and takes the script's token file away. Then switch \
                 the old script off.",
            )
            .size(12.0)
            .color(TEXT_DIM),
        );
        if ui.add_enabled(!view.phase.busy(), egui::Button::new("Use the Sign-in from the Old Sync Script")).clicked() {
            *request = Some(R::Import);
        }
    }
}

/// The old script must go now that its sign-in is here: what to type to switch it off.
fn old_script_warning(ui: &mut egui::Ui, view: &crate::canva::View, request: &mut Option<crate::canva::Request>) {
    egui::Frame::new().fill(Color32::from_rgb(60, 32, 30)).corner_radius(8.0).inner_margin(10.0).show(ui, |ui| {
        let words = if view.settings.hold_syncs {
            "Switch the old sync script off now. It and SoftPresenter can't both use the sign-in: whichever \
             refreshes it next breaks the other's. Its token file couldn't be taken away, so it could still refresh \
             it: nothing is synced until you click It's Switched Off. In Terminal on this Mac:"
        } else {
            "Switch the old sync script off now. It and SoftPresenter can't both use the sign-in: whichever \
             refreshes it next breaks the other's. Its token file is gone, so it can't refresh any more, but it \
             should not be left running. In Terminal on this Mac:"
        };
        ui.label(egui::RichText::new(words).color(TEXT));
        ui.add_space(4.0);
        for step in &view.old_steps {
            ui.label(egui::RichText::new(step.as_str()).monospace().size(12.0).color(TEXT));
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button("Copy the Steps").clicked() {
                ui.ctx().copy_text(view.old_steps.join("\n"));
            }
            if ui.button("It's Switched Off").clicked() {
                *request = Some(crate::canva::Request::OldScriptOff);
            }
        });
    });
}

/// The design: its link, its pages to choose the divider from, and where the video goes.
fn canva_design(ui: &mut egui::Ui, view: &mut crate::canva::View, request: &mut Option<crate::canva::Request>) {
    use crate::canva::{Phase, Request as R};
    crate::studio::caption(ui, "THE PRESENTATION");
    ui.label(
        egui::RichText::new(
            "Paste the presentation's link (in Canva, Share › Copy link), then choose its divider page.",
        )
        .size(12.0)
        .color(TEXT_DIM),
    );
    ui.horizontal(|ui| {
        let hint = view.settings.design.clone().unwrap_or_else(|| "https://www.canva.com/design/…".to_owned());
        let field = ui.add(egui::TextEdit::singleline(&mut view.typed_link).hint_text(hint).desired_width(300.0));
        let entered = field.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
        let busy = view.phase.busy();
        if !view.typed_link.trim().is_empty()
            && (entered || ui.add_enabled(!busy, egui::Button::new("Read Pages")).clicked())
        {
            *request = Some(R::Design(std::mem::take(&mut view.typed_link)));
        } else if view.typed_link.trim().is_empty()
            && view.settings.design.is_some()
            && ui.add_enabled(!busy, egui::Button::new("Read Pages Again")).clicked()
        {
            *request = Some(R::ReadPages);
        }
    });
    if view.phase == Phase::ReadingPages {
        ui.label(egui::RichText::new(view.phase.words()).size(12.0).color(CANVA_LIGHT));
    }
    match (view.settings.divider_number, view.settings.divider.is_some()) {
        (Some(number), true) if number > 1 => {
            let above = if number == 2 { "Page 1 goes".to_owned() } else { format!("Pages 1–{} go", number - 1) };
            ui.label(
                egui::RichText::new(format!(
                    "Divider: page {number}. {above} in the video; drag old slides below it in Canva."
                ))
                .color(TEXT),
            );
        }
        _ if view.settings.design.is_some() => {
            ui.label(egui::RichText::new("No divider page chosen yet: read the pages and click it.").color(TEXT_FAINT));
        }
        _ => {}
    }
    if !view.pages.is_empty() {
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new("Click the divider page. Canva can't tell SoftPresenter which slides are hidden, so the divider says where the video stops.")
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
                let label = if divider { format!("{} · divider", page.number) } else { page.number.to_string() };
                ui.painter().text(
                    pos2(picture.center().x, picture.max.y + 8.0),
                    Align2::CENTER_CENTER,
                    label,
                    FontId::proportional(11.0),
                    if divider { crate::studio::TALLY } else { TEXT_DIM },
                );
                if response.on_hover_text(format!("Make page {} the divider", page.number)).clicked() {
                    chosen = Some(id);
                }
            }
        });
        if let Some(id) = chosen {
            *request = Some(R::Divider(id));
        }
    }
    ui.add_space(12.0);
    crate::studio::caption(ui, "WHERE IT GOES");
    ui.label(
        egui::RichText::new(
            "Media › Canva Sync in the library, followed by the media playlist Announcements (made when the divider \
             is chosen). One video is kept there, named with its day and time so volunteers can read it.",
        )
        .size(12.0)
        .color(TEXT_DIM),
    );
}

/// Where the key is pasted, and how to get one.
fn connect_form(ui: &mut egui::Ui, view: &mut View) {
    crate::studio::caption(ui, "CONNECT");
    ui.label(
        egui::RichText::new(
            "In VoxLive, open Settings › API keys and make a key with the screens permissions (read and write). \
             Paste it here. It is kept in this Mac's Keychain and sent only to VoxLive.",
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
        connect |= ui.add_enabled(!view.typed.trim().is_empty(), egui::Button::new("Connect")).clicked();
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
    crate::studio::caption(ui, "YOUR VOXLIVE SCREENS");
    if view.screens.is_empty() {
        let words = match view.status {
            Status::Connected => "No screens yet. Make one in VoxLive (Screens), then Find Screens Again.",
            _ => "Not found yet.",
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
            "captions" => "Captions",
            other => other,
        };
        let kind = if can_open { format!("{kind}   Change Look ›") } else { kind.to_owned() };
        ui.painter().text(
            row.right_center() - vec2(4.0, 0.0),
            Align2::RIGHT_CENTER,
            kind,
            FontId::proportional(11.5),
            VOXLIVE_LIGHT,
        );
        ui.painter().hline(row.x_range(), row.max.y, Stroke::new(1.0, LINE));
        if response.on_hover_text("Change how this screen looks, here; it is saved to VoxLive").clicked() {
            open = Some(screen.id.clone());
        }
    }
    if let Some(screen) = open {
        view.request = Some(Request::OpenLook(screen));
    }
    ui.add_space(6.0);
    ui.label(
        egui::RichText::new(
            "A screen's live display is opened only on your real screens, where it shows the captions (and can \
             start that language's translation); previews use its free test pattern.",
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
                "Preview loading…",
                FontId::proportional(12.0),
                TEXT_FAINT,
            );
            // The window is drawn again when the preview's first picture comes.
        }
    }
    ui.painter().rect_stroke(place, 6.0, Stroke::new(1.0, LINE), egui::StrokeKind::Outside);
    ui.label(
        egui::RichText::new("Preview, with sample words. Nothing changes on your screens until you save.")
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
        let save = egui::Button::new(if look.saving { "Saving…" } else { "Save to VoxLive" });
        if ui
            .add_enabled(changes > 0 && !look.saving, save)
            .on_hover_text("The screen changes at once, wherever it is showing")
            .clicked()
        {
            request = Some(Request::SaveLook { anyway: false });
        }
        if ui.add_enabled(changes > 0 && !look.saving, egui::Button::new("Revert")).clicked() {
            look.style = look.base.clone();
            look.outcome = None;
        }
        let words = match changes {
            0 => "No changes".to_owned(),
            1 => "1 setting changed".to_owned(),
            n => format!("{n} settings changed"),
        };
        ui.label(egui::RichText::new(words).size(12.0).color(TEXT_FAINT));
    });
    if let Some(outcome) = &look.outcome {
        ui.label(egui::RichText::new(outcome.as_str()).size(12.0).color(TEXT_DIM));
    }
    if look.changed_there.is_some() {
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new("This screen's look was changed in VoxLive since you opened it.")
                .color(crate::studio::TALLY),
        );
        ui.horizontal(|ui| {
            if ui
                .button("Use VoxLive's")
                .on_hover_text("Let your changes go and start from the look VoxLive has now")
                .clicked()
                && let Some(now) = look.changed_there.clone()
            {
                look.take(&now);
                look.outcome = None;
            }
            if ui.button("Save Mine Anyway").on_hover_text("Your changed settings go over VoxLive's").clicked() {
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
        "bg" => "Background",
        "text" => "Text colour",
        "transparent" => "See-through",
        "valign" => "Up and down",
        "align" => "Across",
        "refPosition" => "Reference",
        "fitHeight" => "Height used",
        "maxWidth" => "Width used",
        "maxLines" => "Lines",
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
        assert_eq!(setting_name("letterSpacing"), "Letter spacing");
        assert_eq!(setting_name("maxLines"), "Lines");
    }
}
