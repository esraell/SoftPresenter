//! The two sheets that bring a presentation in: **Import Slides** (Import › Slides…: pictures
//! as slides, or a presentation) and **Import “Sermon.pptx”** (a presentation file: as slides,
//! read straight from the file, or as pictures, drawn by an app on this computer that opens it).
//! Each is two cards side by side. Enter takes the ringed card of the second, a click takes
//! either, Escape or Cancel closes; every key goes to the sheet, none to the show under it.

use std::path::PathBuf;

use egui::{Align2, Color32, Frame, Id, Layout, Pos2, Rect, Sense, Stroke, StrokeKind, pos2, vec2};

use super::HomeState;
use crate::deck::ImportAs;
use crate::icons::Icon;
use crate::studio::{CARD, Flat, HOVER, LINE, PANEL, TEXT, TEXT_DIM, TEXT_FAINT};

/// What the card for slides says of itself.
pub const SLIDES_LINE: &str =
    "Words stay words, ready to edit. Read straight from the file in a moment. Charts and tables come in as pictures";
/// What the card for pictures says of itself.
pub const PICTURES_LINE: &str = "The exact look, each slide a picture. An app on this computer that can open the file draws it unseen, and is closed again";

/// A presentation file waiting to be told how to come in, and what can be done with it. What can
/// be done is worked out where the files are looked at (so the sheet itself is only drawn), and
/// what was kept is how this kind of file came in last time, if it ever has.
#[derive(Clone, Debug, PartialEq)]
pub struct AskImport {
    pub path: PathBuf,
    /// The other presentation files chosen with it, asked about one at a time after it.
    pub later: Vec<PathBuf>,
    /// The file can be read straight into slides.
    pub slides_ok: bool,
    /// An app on this computer can open the file to draw it.
    pub pictures_ok: bool,
    /// How this kind of file came in last time; `None` the first time.
    pub kept: Option<ImportAs>,
}

/// What was answered: the file, the ones still to ask about, and how it comes in (`None` for the
/// OK of a file that can't come in at all).
#[derive(Clone, Debug, PartialEq)]
pub struct ImportAnswer {
    pub path: PathBuf,
    pub later: Vec<PathBuf>,
    pub how: Option<ImportAs>,
}

/// What Import › Slides… was answered with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlidesChoice {
    /// Pictures chosen together, one slide each.
    Pictures,
    /// A presentation file or a PDF.
    Presentation,
}

impl AskImport {
    fn usable(&self, how: ImportAs) -> bool {
        match how {
            ImportAs::Slides => self.slides_ok,
            ImportAs::Pictures => self.pictures_ok,
        }
    }

    /// The card ringed, which Enter takes: what came in last time if it can be done now, else
    /// slides (a `.pptx` the first time), else pictures; `None` when neither can be done.
    pub fn ringed(&self) -> Option<ImportAs> {
        self.kept
            .filter(|how| self.usable(*how))
            .or_else(|| [ImportAs::Slides, ImportAs::Pictures].into_iter().find(|how| self.usable(*how)))
    }

    /// The file's name.
    pub fn name(&self) -> String {
        self.path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned())
    }

    /// The file's ending as people write it (".key", lower case).
    fn ending(&self) -> String {
        self.path.extension().map_or_else(String::new, |e| format!(".{}", e.to_string_lossy().to_lowercase()))
    }

    /// The kind of file the choice is kept for, as a plural: ".pptx files".
    pub fn kind_words(&self) -> String {
        match crate::deck::kind_of(&self.path) {
            "pptx" if self.ending() == ".ppsx" => ".pptx and .ppsx files".to_owned(),
            "pptx" => ".pptx files".to_owned(),
            "ppt" => ".ppt and .pps files".to_owned(),
            other => format!(".{other} files"),
        }
    }

    /// Why a card can't be used, in place of its line and as its hover.
    pub fn reason(&self, how: ImportAs) -> Option<String> {
        if self.usable(how) {
            return None;
        }
        Some(match how {
            ImportAs::Slides => "Only .pptx and .ppsx files can come in as slides".to_owned(),
            ImportAs::Pictures => {
                format!("No app on this computer can open {} files to draw them", self.ending())
            }
        })
    }

    /// With neither possible, the whole sheet: why the file can't come in, and what would do.
    pub fn nothing_words(&self) -> String {
        format!(
            "No app on this computer can open {} files to draw them, so \u{201c}{}\u{201d} can\u{2019}t come in. \
             Saved as .pptx, it could come in as slides.",
            self.ending(),
            self.name()
        )
    }

    /// The foot of the sheet.
    pub fn foot(&self) -> String {
        format!("Enter takes the one ringed. The choice is kept for {}", self.kind_words())
    }
}

/// One card: a title and one line (or the reason it can't be used), ringed when it is the one
/// Enter takes, marked "Last time" when it is what was kept. Says whether it was clicked.
fn card(ui: &mut egui::Ui, at: Pos2, title: &str, line: &str, ringed: bool, last: bool, usable: bool) -> bool {
    let size = vec2(222.0, 112.0);
    let rect = Rect::from_min_size(at, size);
    let id = ui.scope_id().with(("import card", title));
    let response = ui.interact(rect, id, if usable { Sense::click() } else { Sense::hover() });
    let fill = if usable && response.hovered() { HOVER } else { CARD };
    ui.painter().rect_filled(rect, crate::studio::RADIUS_CARD, fill);
    if ringed {
        ui.painter().rect_stroke(rect, crate::studio::RADIUS_CARD, Stroke::new(2.0, TEXT), StrokeKind::Inside);
    }
    let (head, body) = if usable { (TEXT, TEXT_DIM) } else { (TEXT_FAINT, TEXT_FAINT) };
    ui.painter().text(rect.min + vec2(14.0, 13.0), Align2::LEFT_TOP, title, crate::studio::semibold(13.0), head);
    if last {
        crate::studio::paint_spaced(
            ui,
            pos2(rect.max.x - 14.0, rect.min.y + 14.0),
            Align2::RIGHT_TOP,
            "LAST TIME",
            crate::studio::fixed(10.0),
            TEXT_FAINT,
            0.6,
        );
    }
    let words = ui.painter().layout(line.to_owned(), egui::FontId::proportional(12.0), body, size.x - 28.0);
    ui.painter().with_clip_rect(rect.shrink(1.0)).galley(rect.min + vec2(14.0, 35.0), words, body);
    if usable {
        response.clicked()
    } else {
        response.on_hover_text(line);
        false
    }
}

/// The frame every sheet here has.
fn frame() -> Frame {
    Frame::new().fill(PANEL).stroke(Stroke::new(1.0, LINE)).corner_radius(14.0).inner_margin(egui::Margin {
        left: 18,
        right: 18,
        top: 16,
        bottom: 16,
    })
}

/// The title row: the title, and a close key on the right. Says whether it was closed.
fn title_row(ui: &mut egui::Ui, title: &str) -> bool {
    let mut closed = false;
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(title).font(crate::studio::semibold(15.0)).color(TEXT));
        ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
            let (close, _) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::hover());
            if crate::studio::quiet_icon(ui, close, ui.scope_id().with("close"), Icon::Close, 11.0, 0.7, None)
                .on_hover_text("Close")
                .clicked()
            {
                closed = true;
            }
        });
    });
    closed
}

/// The foot: faint words in the fixed face, capitals, and a plain key on the right.
fn foot_row(ui: &mut egui::Ui, words: &str, key: &str) -> bool {
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::hover());
    let font = egui::FontId::proportional(12.5);
    let wide = crate::studio::flat_button_width(ui, &[key], &font, 13.0);
    let button = Rect::from_min_size(pos2(row.max.x - wide, row.min.y), vec2(wide, 30.0));
    let pressed =
        crate::studio::flat_button(ui, button, ui.scope_id().with("foot key"), key, font, Flat::Plain, true).clicked();
    if !words.is_empty() {
        let galley = crate::studio::spaced(ui, &words.to_uppercase(), crate::studio::fixed(11.0), TEXT_FAINT, 0.6);
        let at = pos2(row.min.x, row.center().y - galley.size().y / 2.0);
        let room = Rect::from_min_max(row.min, pos2(button.min.x - 8.0, row.max.y));
        ui.painter().with_clip_rect(room).galley(at, galley, TEXT_FAINT);
    }
    pressed
}

/// Takes the key if it was pressed this frame, so nothing under the sheet sees it.
fn take(ui: &egui::Ui, key: egui::Key) -> bool {
    ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, key))
}

/// The sheet that asks how a presentation file comes in, for the first of those waiting.
pub fn ask_import_sheet(ui: &mut egui::Ui, state: &mut HomeState) {
    let Some(ask) = state.ask_import.clone() else { return };
    let ringed = ask.ringed();
    let mut answer: Option<Option<ImportAs>> = None;
    let mut cancelled = false;
    let id = Id::unique("import presentation sheet");
    let area = egui::Modal::default_area(id).anchor(Align2::CENTER_CENTER, vec2(0.0, -30.0)).fade_in(false);
    egui::Modal::new(id).area(area).backdrop_color(Color32::from_black_alpha(110)).frame(frame()).show(
        ui.ctx(),
        |ui| {
            ui.set_width(666.0);
            ui.spacing_mut().item_spacing = vec2(8.0, 12.0);
            cancelled |= title_row(ui, &format!("Import \u{201c}{}\u{201d}", ask.name()));
            if ringed.is_none() {
                // Neither way is possible: one paragraph, and OK.
                ui.label(egui::RichText::new(ask.nothing_words()).size(12.5).color(TEXT_DIM));
                if foot_row(ui, "", "OK") || take(ui, egui::Key::Enter) {
                    answer = Some(None);
                }
            } else {
                let (rect, _) = ui.allocate_exact_size(vec2(454.0, 112.0), Sense::hover());
                for (place, how, title, line) in [
                    (0.0, ImportAs::Slides, "As Slides", SLIDES_LINE),
                    (232.0, ImportAs::Pictures, "As Pictures", PICTURES_LINE),
                ] {
                    let reason = ask.reason(how);
                    let usable = reason.is_none();
                    let last = usable && ask.kept == Some(how);
                    let words = reason.as_deref().unwrap_or(line);
                    if card(ui, rect.min + vec2(place, 0.0), title, words, ringed == Some(how), last, usable) {
                        answer = Some(Some(how));
                    }
                }
                if foot_row(ui, &ask.foot(), "Cancel") {
                    cancelled = true;
                }
                if take(ui, egui::Key::Enter) {
                    answer = Some(ringed);
                }
            }
            if take(ui, egui::Key::Escape) {
                cancelled = true;
            }
        },
    );
    if let Some(how) = answer {
        state.ask_import = None;
        if let Some(how) = how {
            state.remember_import(crate::deck::kind_of(&ask.path), how);
        }
        state.import_answer = Some(ImportAnswer { path: ask.path, later: ask.later, how });
    } else if cancelled {
        // The rest of the files chosen with it are let go too.
        state.ask_import = None;
    }
}

/// The sheet Import › Slides… opens: pictures as slides, or a presentation.
pub fn slides_sheet(ui: &mut egui::Ui, state: &mut HomeState) {
    if !state.slides_sheet {
        return;
    }
    let mut chosen = None;
    let mut cancelled = false;
    let id = Id::unique("import slides sheet");
    let area = egui::Modal::default_area(id).anchor(Align2::CENTER_CENTER, vec2(0.0, -30.0)).fade_in(false);
    egui::Modal::new(id).area(area).backdrop_color(Color32::from_black_alpha(110)).frame(frame()).show(
        ui.ctx(),
        |ui| {
            ui.set_width(666.0);
            ui.spacing_mut().item_spacing = vec2(8.0, 12.0);
            cancelled |= title_row(ui, "Import Slides");
            let (rect, _) = ui.allocate_exact_size(vec2(454.0, 112.0), Sense::hover());
            for (place, which, title, line) in [
                (
                    0.0,
                    SlidesChoice::Pictures,
                    "Pictures as slides",
                    "Pictures chosen together, one slide each, in a presentation named after their folder",
                ),
                (
                    232.0,
                    SlidesChoice::Presentation,
                    "A presentation",
                    "A .pptx, .ppsx, .key, .odp, .ppt or .pps file, or a PDF. You choose how it comes in",
                ),
            ] {
                if card(ui, rect.min + vec2(place, 0.0), title, line, false, false, true) {
                    chosen = Some(which);
                }
            }
            if foot_row(ui, "", "Cancel") {
                cancelled = true;
            }
            if take(ui, egui::Key::Escape) {
                cancelled = true;
            }
        },
    );
    if chosen.is_some() || cancelled {
        state.slides_sheet = false;
        state.slides_choice = chosen;
    }
}
