//! The logos of the services and assistants Settings connects with, each in a tile of its own:
//! the owner's own marks (VoxLive's, drawn here; TimelyChurch's), and the marks of companies whose
//! terms allow a small, unaltered mark naming a service the operator can connect to. Those are
//! kept as their companies provide them in `assets/logos/third-party/` (the SVG as provided, and
//! the picture drawn from it), are not covered by SoftPresenter's licence, and are recorded in
//! docs/licences/logos.md. A service without a logo shows its initials.

use egui::{Align2, Color32, Pos2, Rect, Stroke, StrokeKind, pos2, vec2};

use crate::studio::{LINE, RADIUS_CARD, TEXT_DIM, semibold};

/// A logo the app carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Logo {
    /// VoxLive's mark (the owner's own): three waves from a gold dot.
    VoxLive,
    /// TimelyChurch's (the owner's own).
    TimelyChurch,
    /// Canva's icon, for Canva Sync, a registered Canva integration (canva.dev's brand rules).
    Canva,
    /// Grok's logomark (xAI's, light), naming Grok accurately and implying no endorsement.
    Grok,
    /// Claude's spark (Anthropic's press kit), used without Anthropic's approval.
    Claude,
    /// OpenAI's Blossom in white (OpenAI's brand kit), for ChatGPT, used without OpenAI's approval.
    ChatGpt,
    /// Worship Leader App's note (its own site), used without its owner's approval.
    WorshipLeaderApp,
    /// NDI's own logo, the light one for dark grounds (NDI's logo package), for NDI's card.
    Ndi,
}

impl Logo {
    /// The picture's bytes and a name for its texture; VoxLive's is drawn instead.
    fn picture(self) -> Option<(&'static str, &'static [u8])> {
        match self {
            Logo::VoxLive => None,
            Logo::TimelyChurch => Some(("logo timelychurch", include_bytes!("../../../assets/logos/timelychurch.png"))),
            Logo::Canva => Some(("logo canva", include_bytes!("../../../assets/logos/third-party/canva.png"))),
            Logo::Grok => Some(("logo grok", include_bytes!("../../../assets/logos/third-party/grok.png"))),
            Logo::Claude => Some(("logo claude", include_bytes!("../../../assets/logos/third-party/claude.png"))),
            Logo::ChatGpt => Some(("logo chatgpt", include_bytes!("../../../assets/logos/third-party/chatgpt.png"))),
            Logo::Ndi => Some(("logo ndi", include_bytes!("../../../assets/logos/third-party/ndi.png"))),
            Logo::WorshipLeaderApp => Some((
                "logo worship leader app",
                include_bytes!("../../../assets/logos/third-party/worship-leader-app.png"),
            )),
        }
    }
}

/// What a tile shows: a logo, or a name's initials where there is no logo.
#[derive(Clone, Copy, Debug)]
pub enum Mark<'a> {
    Logo(Logo),
    Initials(&'a str),
}

impl<'a> Mark<'a> {
    /// The mark for a name with no logo: its first letters, two at most ("Worship Leader App":
    /// WL).
    pub fn initials_of(name: &str) -> String {
        name.split_whitespace().filter_map(|word| word.chars().next()).take(2).collect()
    }
}

/// A logo's tile: a square with the card's corners and a faint edge, `fill` inside, and the mark in
/// its middle, a picture at two thirds of the tile (so Canva's icon keeps the clear room its rules
/// ask for), VoxLive's waves at three quarters, or initials in the heavier face.
pub fn tile(ui: &egui::Ui, rect: Rect, mark: Mark, fill: Color32) {
    let painter = ui.painter();
    painter.rect_filled(rect, RADIUS_CARD, fill);
    painter.rect_stroke(rect, RADIUS_CARD, Stroke::new(1.0, LINE), StrokeKind::Inside);
    match mark {
        Mark::Initials(letters) => {
            // 15 in a tile of 48, 19 in one of 64, 13 in one of 36, as the design sets them.
            let size = (rect.width() * 0.25 + 3.0).round();
            painter.text(rect.center(), Align2::CENTER_CENTER, letters, semibold(size), TEXT_DIM);
        }
        Mark::Logo(Logo::VoxLive) => voxlive(painter, Rect::from_center_size(rect.center(), rect.size() * 0.78)),
        Mark::Logo(logo) => {
            if let Some(texture) = texture(ui.ctx(), logo) {
                // NDI's logo is wide and carries its own safety margins in its file, so it
                // takes the tile's whole width, as tall as its shape makes it (1047 to 550).
                let picture = if logo == Logo::Ndi {
                    Rect::from_center_size(rect.center(), vec2(rect.width(), rect.width() * 550.0 / 1047.0))
                } else {
                    Rect::from_center_size(rect.center(), rect.size() * 0.66)
                };
                painter.image(texture, picture, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            }
        }
    }
}

/// The logo's picture as a texture of this window's, made the first time it is drawn there.
fn texture(ctx: &egui::Context, logo: Logo) -> Option<egui::TextureId> {
    let (name, bytes) = logo.picture()?;
    let id = egui::Id::unique(name);
    if let Some(handle) = ctx.data(|data| data.get_temp::<egui::TextureHandle>(id)) {
        return Some(handle.id());
    }
    let picture = image::load_from_memory_with_format(bytes, image::ImageFormat::Png).ok()?.to_rgba8();
    let size = [picture.width() as usize, picture.height() as usize];
    let image = egui::ColorImage::from_rgba_unmultiplied(size, picture.as_raw());
    // Drawn smaller than it is made, so it is read from smaller copies of itself, never jagged.
    let options = egui::TextureOptions::LINEAR.with_mipmap_mode(Some(egui::TextureFilter::Linear));
    let handle = ctx.load_texture(name, image, options);
    let texture = handle.id();
    ctx.data_mut(|data| data.insert_temp(id, handle));
    Some(texture)
}

/// VoxLive's mark in `rect`, as its own artwork draws it on a grid of 48: three waves, the last
/// fainter, spreading from a gold dot.
fn voxlive(painter: &egui::Painter, rect: Rect) {
    const WAVE: Color32 = Color32::from_rgb(0xf3, 0xf0, 0xe8);
    const DOT: Color32 = Color32::from_rgb(0xd4, 0xa2, 0x4a);
    let unit = rect.width() / 48.0;
    let at = |x: f32, y: f32| -> Pos2 { rect.min + vec2(x * unit, y * unit) };
    for (from, bend, to, width, strength) in [
        ((15.0, 16.0), (21.0, 24.0), (15.0, 32.0), 2.2, 1.0),
        ((21.0, 13.0), (29.0, 24.0), (21.0, 35.0), 2.0, 1.0),
        ((28.0, 10.0), (38.0, 24.0), (28.0, 38.0), 1.8, 0.55),
    ] {
        let points: Vec<Pos2> = (0..=20)
            .map(|step| {
                let t = step as f32 / 20.0;
                let u = 1.0 - t;
                let x = u * u * from.0 + 2.0 * u * t * bend.0 + t * t * to.0;
                let y = u * u * from.1 + 2.0 * u * t * bend.1 + t * t * to.1;
                at(x, y)
            })
            .collect();
        painter.add(egui::Shape::line(points, Stroke::new(width * unit, WAVE.gamma_multiply(strength))));
    }
    painter.circle_filled(at(13.0, 24.0), 2.4 * unit, DOT);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_logo_is_a_picture_the_app_can_read_or_is_drawn() {
        for logo in [
            Logo::VoxLive,
            Logo::TimelyChurch,
            Logo::Canva,
            Logo::Grok,
            Logo::Claude,
            Logo::ChatGpt,
            Logo::WorshipLeaderApp,
        ] {
            if let Some((_, bytes)) = logo.picture() {
                let picture = image::load_from_memory_with_format(bytes, image::ImageFormat::Png).unwrap();
                assert!(picture.width() >= 64 && picture.width() == picture.height(), "{logo:?} is square");
            }
        }
        assert_eq!(Mark::initials_of("Worship Leader App"), "WL");
        assert_eq!(Mark::initials_of("Claude"), "C");
    }
}
