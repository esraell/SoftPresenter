//! The app's icons, drawn with lines and shapes so they are sharp at any size and never depend
//! on which fonts a computer has. Every icon is drawn inside a square.

use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, pos2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
// The whole set is kept, though parts of the app that use some of them are not built yet.
#[cfg_attr(not(test), allow(dead_code))]
pub enum Icon {
    Search,
    Theme,
    Play,
    Pause,
    Pencil,
    Reflow,
    Book,
    More,
    Picture,
    Look,
    Live,
    Status,
    Plus,
    ChevronUp,
    ChevronDown,
    ChevronLeft,
    ChevronRight,
    Gear,
    Document,
    Library,
    Note,
    Message,
    Layers,
    Megaphone,
    Slide,
    Camera,
    Close,
    Back,
    Forward,
    Undo,
    Redo,
    ToEnd,
    Flag,
    Stage,
    Clock,
    SlideShow,
    Notes,
    Arrangement,
    Timeline,
    Target,
    Transition,
    Cut,
    Dissolve,
    Grid,
    Rows,
    Options,
    Sidebar,
    Trash,
    /// Two arrows chasing each other round: plays again from the start.
    Loop,
    /// A speaker with sound coming from it, and the same speaker struck through.
    Sound,
    Muted,
    /// Back to the beginning: a bar and a triangle pointing at it.
    ToStart,
    /// A tick: the one that is open, or in use.
    Check,
    /// An arrow coming down onto a line: bring something in.
    Import,
    /// A list with a dot before each line: a playlist.
    Playlist,
    /// A table: a heading row and columns.
    Table,
    /// A frame with a play mark: a video.
    Film,
    /// Pictures one behind another: a media playlist.
    Stack,
    /// Three lines, the last shorter: the notices' status button.
    Notices,
}

/// Every icon, so a test can draw them all.
#[cfg(test)]
pub const ALL: [Icon; 59] = [
    Icon::Search,
    Icon::Theme,
    Icon::Play,
    Icon::Pause,
    Icon::Pencil,
    Icon::Reflow,
    Icon::Book,
    Icon::More,
    Icon::Picture,
    Icon::Look,
    Icon::Live,
    Icon::Status,
    Icon::Plus,
    Icon::ChevronUp,
    Icon::ChevronDown,
    Icon::ChevronLeft,
    Icon::ChevronRight,
    Icon::Gear,
    Icon::Document,
    Icon::Library,
    Icon::Note,
    Icon::Message,
    Icon::Layers,
    Icon::Megaphone,
    Icon::Slide,
    Icon::Camera,
    Icon::Close,
    Icon::Back,
    Icon::Forward,
    Icon::Undo,
    Icon::Redo,
    Icon::ToEnd,
    Icon::Flag,
    Icon::Stage,
    Icon::Clock,
    Icon::SlideShow,
    Icon::Notes,
    Icon::Arrangement,
    Icon::Timeline,
    Icon::Target,
    Icon::Transition,
    Icon::Cut,
    Icon::Dissolve,
    Icon::Grid,
    Icon::Rows,
    Icon::Options,
    Icon::Sidebar,
    Icon::Trash,
    Icon::Loop,
    Icon::Sound,
    Icon::Muted,
    Icon::ToStart,
    Icon::Check,
    Icon::Import,
    Icon::Playlist,
    Icon::Table,
    Icon::Film,
    Icon::Stack,
    Icon::Notices,
];

/// Draws inside a square using coordinates from 0 to 1.
struct Pen<'a> {
    painter: &'a Painter,
    square: Rect,
    stroke: Stroke,
}

impl Pen<'_> {
    fn at(&self, x: f32, y: f32) -> Pos2 {
        pos2(self.square.min.x + x * self.square.width(), self.square.min.y + y * self.square.height())
    }

    fn len(&self, v: f32) -> f32 {
        v * self.square.width()
    }

    fn line(&self, points: &[(f32, f32)]) {
        let points: Vec<Pos2> = points.iter().map(|(x, y)| self.at(*x, *y)).collect();
        self.painter.add(Shape::line(points, self.stroke));
    }

    fn closed(&self, points: &[(f32, f32)]) {
        let points: Vec<Pos2> = points.iter().map(|(x, y)| self.at(*x, *y)).collect();
        self.painter.add(Shape::closed_line(points, self.stroke));
    }

    fn filled(&self, points: &[(f32, f32)]) {
        let points: Vec<Pos2> = points.iter().map(|(x, y)| self.at(*x, *y)).collect();
        self.painter.add(Shape::convex_polygon(points, self.stroke.color, Stroke::NONE));
    }

    fn circle(&self, x: f32, y: f32, r: f32) {
        self.painter.circle_stroke(self.at(x, y), self.len(r), self.stroke);
    }

    fn dot(&self, x: f32, y: f32, r: f32) {
        self.painter.circle_filled(self.at(x, y), self.len(r), self.stroke.color);
    }

    fn rect(&self, x0: f32, y0: f32, x1: f32, y1: f32, radius: f32) {
        let rect = Rect::from_min_max(self.at(x0, y0), self.at(x1, y1));
        self.painter.rect_stroke(rect, self.len(radius), self.stroke, StrokeKind::Middle);
    }

    fn fill_rect(&self, x0: f32, y0: f32, x1: f32, y1: f32, radius: f32) {
        let rect = Rect::from_min_max(self.at(x0, y0), self.at(x1, y1));
        self.painter.rect_filled(rect, self.len(radius), self.stroke.color);
    }

    /// Part of a circle, from one angle to another in turns (0 is right, 0.25 is down).
    fn arc(&self, x: f32, y: f32, r: f32, from: f32, to: f32) {
        let steps = 16;
        let points: Vec<Pos2> = (0..=steps)
            .map(|i| {
                let angle = (from + (to - from) * i as f32 / steps as f32) * std::f32::consts::TAU;
                self.at(x + r * angle.cos(), y + r * angle.sin())
            })
            .collect();
        self.painter.add(Shape::line(points, self.stroke));
    }
}

/// Draws an icon centred in `rect`, as large as fits, in one colour.
pub fn paint(painter: &Painter, icon: Icon, rect: Rect, color: Color32) {
    let side = rect.width().min(rect.height());
    let square = Rect::from_center_size(rect.center(), egui::vec2(side, side));
    let p = Pen { painter, square, stroke: Stroke::new((side / 11.0).clamp(1.0, 2.0), color) };
    match icon {
        Icon::Search => {
            p.circle(0.43, 0.43, 0.27);
            p.line(&[(0.63, 0.63), (0.88, 0.88)]);
        }
        // A circle split down the middle, with lines across its right half: one look, several
        // ways.
        Icon::Theme => {
            p.circle(0.5, 0.5, 0.346);
            p.line(&[(0.5, 0.154), (0.5, 0.846)]);
            p.line(&[(0.5, 0.346), (0.792, 0.346)]);
            p.line(&[(0.5, 0.5), (0.846, 0.5)]);
            p.line(&[(0.5, 0.654), (0.792, 0.654)]);
        }
        Icon::Play => p.filled(&[(0.28, 0.16), (0.84, 0.5), (0.28, 0.84)]),
        Icon::Pause => {
            p.fill_rect(0.24, 0.18, 0.42, 0.82, 0.03);
            p.fill_rect(0.58, 0.18, 0.76, 0.82, 0.03);
        }
        Icon::Pencil => {
            p.closed(&[(0.14, 0.86), (0.2, 0.64), (0.66, 0.18), (0.82, 0.34), (0.36, 0.8)]);
            p.line(&[(0.56, 0.28), (0.72, 0.44)]);
        }
        Icon::Reflow => {
            p.line(&[(0.14, 0.24), (0.86, 0.24)]);
            p.line(&[(0.14, 0.5), (0.62, 0.5)]);
            p.line(&[(0.14, 0.76), (0.86, 0.76)]);
            p.line(&[(0.74, 0.42), (0.86, 0.5), (0.74, 0.58)]);
        }
        Icon::Book => {
            p.line(&[(0.5, 0.26), (0.5, 0.84)]);
            p.line(&[(0.5, 0.26), (0.36, 0.18), (0.1, 0.18), (0.1, 0.76), (0.36, 0.76), (0.5, 0.84)]);
            p.line(&[(0.5, 0.26), (0.64, 0.18), (0.9, 0.18), (0.9, 0.76), (0.64, 0.76), (0.5, 0.84)]);
        }
        Icon::More => {
            for x in [0.2, 0.5, 0.8] {
                p.dot(x, 0.5, 0.075);
            }
        }
        Icon::Picture => {
            p.rect(0.1, 0.18, 0.9, 0.82, 0.08);
            p.dot(0.34, 0.38, 0.07);
            p.line(&[(0.14, 0.76), (0.4, 0.54), (0.56, 0.68), (0.7, 0.5), (0.88, 0.72)]);
        }
        Icon::Look => {
            p.circle(0.38, 0.5, 0.26);
            p.circle(0.62, 0.5, 0.26);
        }
        Icon::Live => {
            p.dot(0.5, 0.5, 0.1);
            p.arc(0.5, 0.5, 0.26, -0.12, 0.12);
            p.arc(0.5, 0.5, 0.26, 0.38, 0.62);
            p.arc(0.5, 0.5, 0.42, -0.1, 0.1);
            p.arc(0.5, 0.5, 0.42, 0.4, 0.6);
        }
        Icon::Status => {
            p.circle(0.5, 0.5, 0.38);
            p.dot(0.5, 0.31, 0.055);
            p.line(&[(0.5, 0.45), (0.5, 0.72)]);
        }
        Icon::Plus => {
            p.line(&[(0.5, 0.2), (0.5, 0.8)]);
            p.line(&[(0.2, 0.5), (0.8, 0.5)]);
        }
        Icon::ChevronUp => p.line(&[(0.24, 0.62), (0.5, 0.36), (0.76, 0.62)]),
        Icon::ChevronDown => p.line(&[(0.24, 0.38), (0.5, 0.64), (0.76, 0.38)]),
        Icon::ChevronLeft => p.line(&[(0.62, 0.24), (0.36, 0.5), (0.62, 0.76)]),
        Icon::ChevronRight => p.line(&[(0.38, 0.24), (0.64, 0.5), (0.38, 0.76)]),
        Icon::Gear => {
            p.circle(0.5, 0.5, 0.15);
            p.circle(0.5, 0.5, 0.3);
            for i in 0..8 {
                let a = i as f32 / 8.0 * std::f32::consts::TAU;
                p.line(&[(0.5 + 0.3 * a.cos(), 0.5 + 0.3 * a.sin()), (0.5 + 0.43 * a.cos(), 0.5 + 0.43 * a.sin())]);
            }
        }
        Icon::Document => {
            p.rect(0.12, 0.2, 0.88, 0.8, 0.07);
            p.line(&[(0.26, 0.4), (0.74, 0.4)]);
            p.line(&[(0.26, 0.6), (0.6, 0.6)]);
        }
        Icon::Library => {
            p.rect(0.12, 0.34, 0.88, 0.86, 0.07);
            p.line(&[(0.2, 0.24), (0.8, 0.24)]);
            p.line(&[(0.28, 0.14), (0.72, 0.14)]);
        }
        Icon::Note => {
            p.dot(0.3, 0.74, 0.13);
            p.dot(0.72, 0.64, 0.13);
            p.line(&[(0.42, 0.74), (0.42, 0.22), (0.84, 0.14), (0.84, 0.64)]);
        }
        Icon::Message => {
            p.closed(&[(0.1, 0.46), (0.9, 0.12), (0.62, 0.9), (0.46, 0.58)]);
            p.line(&[(0.46, 0.58), (0.9, 0.12)]);
        }
        Icon::Layers => {
            p.closed(&[(0.5, 0.12), (0.9, 0.34), (0.5, 0.56), (0.1, 0.34)]);
            p.line(&[(0.1, 0.52), (0.5, 0.74), (0.9, 0.52)]);
            p.line(&[(0.1, 0.7), (0.5, 0.92), (0.9, 0.7)]);
        }
        Icon::Megaphone => {
            p.closed(&[(0.12, 0.4), (0.3, 0.4), (0.74, 0.16), (0.74, 0.84), (0.3, 0.6), (0.12, 0.6)]);
            p.line(&[(0.26, 0.62), (0.34, 0.88)]);
            p.arc(0.74, 0.5, 0.16, -0.18, 0.18);
        }
        Icon::Slide => {
            p.rect(0.1, 0.2, 0.9, 0.8, 0.07);
            p.line(&[(0.28, 0.42), (0.72, 0.42)]);
            p.line(&[(0.36, 0.6), (0.64, 0.6)]);
        }
        Icon::Camera => {
            p.rect(0.08, 0.26, 0.64, 0.74, 0.08);
            p.closed(&[(0.64, 0.44), (0.92, 0.28), (0.92, 0.72), (0.64, 0.56)]);
        }
        Icon::Close => {
            p.line(&[(0.24, 0.24), (0.76, 0.76)]);
            p.line(&[(0.76, 0.24), (0.24, 0.76)]);
        }
        // A circle turning back (anticlockwise), open at the upper left: from the top round the
        // right, the bottom and the left, with the head at the top pointing left.
        Icon::Back => {
            p.arc(0.5, 0.54, 0.32, -0.25, 0.56);
            p.line(&[(0.58, 0.09), (0.44, 0.22), (0.58, 0.35)]);
        }
        // A circle turning forward (clockwise), open at the upper right: from the right round the
        // bottom and the left to the top, with the head at the top pointing right.
        Icon::Forward => {
            p.arc(0.5, 0.54, 0.32, -0.06, 0.75);
            p.line(&[(0.42, 0.09), (0.56, 0.22), (0.42, 0.35)]);
        }
        // An arrow back to the left, its tail turning down and under, as a step taken back.
        Icon::Undo => {
            p.line(&[(0.34, 0.22), (0.18, 0.38), (0.34, 0.54)]);
            p.line(&[(0.18, 0.38), (0.6, 0.38)]);
            p.arc(0.6, 0.58, 0.2, -0.25, 0.25);
            p.line(&[(0.6, 0.78), (0.36, 0.78)]);
        }
        // The same turned round: a step taken again.
        Icon::Redo => {
            p.line(&[(0.66, 0.22), (0.82, 0.38), (0.66, 0.54)]);
            p.line(&[(0.82, 0.38), (0.4, 0.38)]);
            p.arc(0.4, 0.58, 0.2, 0.75, 0.25);
            p.line(&[(0.4, 0.78), (0.64, 0.78)]);
        }
        Icon::ToEnd => {
            p.line(&[(0.12, 0.5), (0.7, 0.5)]);
            p.line(&[(0.5, 0.3), (0.7, 0.5), (0.5, 0.7)]);
            p.line(&[(0.86, 0.22), (0.86, 0.78)]);
        }
        Icon::Flag => {
            p.line(&[(0.24, 0.12), (0.24, 0.9)]);
            p.closed(&[(0.24, 0.16), (0.82, 0.16), (0.66, 0.36), (0.82, 0.56), (0.24, 0.56)]);
        }
        Icon::Stage => {
            p.rect(0.1, 0.16, 0.9, 0.66, 0.07);
            p.line(&[(0.5, 0.66), (0.5, 0.84)]);
            p.line(&[(0.3, 0.86), (0.7, 0.86)]);
        }
        Icon::Clock => {
            p.circle(0.5, 0.5, 0.38);
            p.line(&[(0.5, 0.26), (0.5, 0.5), (0.68, 0.6)]);
        }
        Icon::SlideShow => {
            p.rect(0.1, 0.2, 0.9, 0.8, 0.07);
            p.filled(&[(0.42, 0.36), (0.66, 0.5), (0.42, 0.64)]);
        }
        Icon::Notes => {
            p.rect(0.16, 0.12, 0.84, 0.88, 0.07);
            for y in [0.34, 0.5, 0.66] {
                p.line(&[(0.3, y), (0.7, y)]);
            }
        }
        Icon::Arrangement => {
            p.rect(0.08, 0.36, 0.34, 0.64, 0.05);
            p.rect(0.4, 0.36, 0.62, 0.64, 0.05);
            p.rect(0.68, 0.36, 0.92, 0.64, 0.05);
        }
        Icon::Timeline => {
            p.line(&[(0.08, 0.5), (0.92, 0.5)]);
            p.line(&[(0.26, 0.3), (0.26, 0.7)]);
            p.line(&[(0.52, 0.2), (0.52, 0.8)]);
            p.line(&[(0.76, 0.34), (0.76, 0.66)]);
        }
        Icon::Target => {
            p.circle(0.5, 0.5, 0.38);
            p.dot(0.5, 0.5, 0.13);
        }
        Icon::Transition => {
            p.rect(0.08, 0.18, 0.62, 0.66, 0.06);
            p.rect(0.38, 0.36, 0.92, 0.84, 0.06);
        }
        Icon::Cut => {
            p.fill_rect(0.14, 0.22, 0.44, 0.78, 0.04);
            p.fill_rect(0.56, 0.22, 0.86, 0.78, 0.04);
        }
        Icon::Dissolve => {
            p.rect(0.14, 0.18, 0.86, 0.82, 0.06);
            p.filled(&[(0.14, 0.82), (0.86, 0.18), (0.86, 0.82)]);
        }
        Icon::Grid => {
            for (x, y) in [(0.14, 0.14), (0.56, 0.14), (0.14, 0.56), (0.56, 0.56)] {
                p.fill_rect(x, y, x + 0.3, y + 0.3, 0.04);
            }
        }
        Icon::Rows => {
            for y in [0.24, 0.5, 0.76] {
                p.line(&[(0.14, y), (0.86, y)]);
            }
        }
        Icon::Options => {
            p.circle(0.5, 0.5, 0.38);
            for x in [0.32, 0.5, 0.68] {
                p.dot(x, 0.5, 0.05);
            }
        }
        Icon::Sidebar => {
            p.rect(0.1, 0.18, 0.9, 0.82, 0.07);
            p.line(&[(0.38, 0.18), (0.38, 0.82)]);
        }
        Icon::Trash => {
            p.line(&[(0.14, 0.24), (0.86, 0.24)]);
            p.line(&[(0.38, 0.24), (0.4, 0.12), (0.6, 0.12), (0.62, 0.24)]);
            p.closed(&[(0.22, 0.24), (0.28, 0.88), (0.72, 0.88), (0.78, 0.24)]);
            p.line(&[(0.42, 0.38), (0.43, 0.74)]);
            p.line(&[(0.58, 0.38), (0.57, 0.74)]);
        }
        Icon::Loop => {
            p.line(&[(0.16, 0.58), (0.16, 0.4), (0.28, 0.28), (0.8, 0.28)]);
            p.line(&[(0.68, 0.16), (0.8, 0.28), (0.68, 0.4)]);
            p.line(&[(0.84, 0.42), (0.84, 0.6), (0.72, 0.72), (0.2, 0.72)]);
            p.line(&[(0.32, 0.6), (0.2, 0.72), (0.32, 0.84)]);
        }
        Icon::Sound | Icon::Muted => {
            p.filled(&[(0.1, 0.38), (0.28, 0.38), (0.5, 0.18), (0.5, 0.82), (0.28, 0.62), (0.1, 0.62)]);
            if icon == Icon::Sound {
                p.arc(0.5, 0.5, 0.18, -0.12, 0.12);
                p.arc(0.5, 0.5, 0.33, -0.13, 0.13);
            } else {
                p.line(&[(0.64, 0.36), (0.9, 0.64)]);
                p.line(&[(0.9, 0.36), (0.64, 0.64)]);
            }
        }
        Icon::ToStart => {
            p.fill_rect(0.18, 0.2, 0.28, 0.8, 0.02);
            p.filled(&[(0.82, 0.2), (0.34, 0.5), (0.82, 0.8)]);
        }
        Icon::Check => p.line(&[(0.18, 0.54), (0.4, 0.76), (0.82, 0.26)]),
        Icon::Import => {
            p.line(&[(0.5, 0.12), (0.5, 0.62)]);
            p.line(&[(0.3, 0.44), (0.5, 0.64), (0.7, 0.44)]);
            p.line(&[(0.18, 0.84), (0.82, 0.84)]);
        }
        Icon::Playlist => {
            for y in [0.24, 0.5, 0.76] {
                p.dot(0.16, y, 0.06);
                p.line(&[(0.34, y), (0.88, y)]);
            }
        }
        Icon::Table => {
            p.rect(0.12, 0.16, 0.88, 0.84, 0.08);
            p.line(&[(0.12, 0.4), (0.88, 0.4)]);
            p.line(&[(0.42, 0.4), (0.42, 0.84)]);
        }
        Icon::Film => {
            p.rect(0.1, 0.2, 0.9, 0.8, 0.08);
            p.filled(&[(0.42, 0.36), (0.66, 0.5), (0.42, 0.64)]);
        }
        Icon::Stack => {
            p.rect(0.1, 0.34, 0.74, 0.86, 0.06);
            p.line(&[(0.26, 0.24), (0.84, 0.24), (0.84, 0.7)]);
            p.line(&[(0.4, 0.14), (0.92, 0.14), (0.92, 0.58)]);
        }
        Icon::Notices => {
            p.line(&[(0.2, 0.3), (0.8, 0.3)]);
            p.line(&[(0.2, 0.5), (0.8, 0.5)]);
            p.line(&[(0.2, 0.7), (0.6, 0.7)]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every icon can be drawn at small and large sizes, and in a square of no size at all.
    #[test]
    fn every_icon_draws_at_any_size() {
        let ctx = egui::Context::default();
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            for icon in ALL {
                for side in [0.0, 9.0, 16.0, 64.0] {
                    let square = Rect::from_min_size(pos2(10.0, 10.0), egui::vec2(side, side));
                    paint(ui.painter(), icon, square, Color32::WHITE);
                }
            }
        });
        output.drop_without_applying_deltas();
    }

    #[test]
    fn the_list_of_all_icons_has_no_repeats() {
        for (i, a) in ALL.iter().enumerate() {
            assert!(!ALL[i + 1..].contains(a), "{a:?} is listed twice");
        }
    }
}
