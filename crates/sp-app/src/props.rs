//! Props: things shown over the slides on the audience screens until they are taken off, such as
//! a logo's name in a corner or a "LIVE" tag. A prop is a slide of its own, made in the editor.
//! Props are kept in `props.json`.

use serde::{Deserialize, Serialize};
use sp_model::{Object, Role, Slide, SlideRect, TextStyle, Uuid};

use crate::i18n::{t, tf};

/// The place an editor draft says it belongs to when it is a prop being edited; `slide` then
/// says which prop.
pub const PROP_SLOT: usize = usize::MAX - 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Prop {
    pub id: Uuid,
    pub name: String,
    pub slide: Slide,
    /// Taken off the screen by itself after this many seconds; `None` stays until cleared.
    #[serde(default)]
    pub auto_clear: Option<u32>,
}

impl Prop {
    /// A new prop: a small box of words in the top left corner, to be changed in the editor.
    pub fn new(name: String) -> Prop {
        let builtin = sp_model::Theme::builtin();
        let mut look = builtin.templates[0].boxes[0].clone();
        look.size = 56.0;
        look.min_size = 20.0;
        look.align = sp_model::Align::Left;
        look.outline_width = 0.0;
        let mut words = Object::text(Role("prop".to_owned()), name.clone());
        words.name = t("tools.prop.words_object", "Words").to_owned();
        words.frame = Some(SlideRect { x: 60.0, y: 40.0, w: 700.0, h: 110.0 });
        if let Some(style) = words.text_style_mut() {
            *style = TextStyle::all(&look);
        }
        Prop { id: Uuid::new_v4(), name, slide: Slide::new(vec![words]), auto_clear: None }
    }

    /// A copy with ids of its own.
    pub fn duplicate(&self) -> Prop {
        let mut copy = self.clone();
        copy.id = Uuid::new_v4();
        copy.name = tf("tools.copy_name", "{name} copy", &[("name", &self.name)]);
        copy.slide.id = Uuid::new_v4();
        for object in &mut copy.slide.objects {
            object.id = Uuid::new_v4();
        }
        copy
    }
}

/// The props on screen and a message, as one layer over the slide: the props in the order they
/// were put on, the message on top.
pub fn overlay(props: &[Slide], message: Option<&Slide>) -> Option<Slide> {
    if props.is_empty() && message.is_none() {
        return None;
    }
    let objects = props.iter().chain(message).flat_map(|slide| slide.objects.iter().cloned()).collect();
    Some(Slide::new(objects))
}

/// A song's copyright line, small along the bottom of the screen.
pub fn copyright_slide(line: &str) -> Slide {
    let builtin = sp_model::Theme::builtin();
    let mut look = builtin.templates[0].boxes[0].clone();
    look.size = 24.0;
    look.min_size = 14.0;
    look.weight = 500;
    look.fill = sp_model::Rgba::rgb(220, 220, 220);
    look.align = sp_model::Align::Left;
    look.outline_width = 0.0;
    let mut words = Object::text(Role("copyright".to_owned()), line);
    words.frame = Some(SlideRect { x: 60.0, y: 1020.0, w: 1800.0, h: 44.0 });
    if let Some(style) = words.text_style_mut() {
        *style = TextStyle::all(&look);
    }
    Slide::new(vec![words])
}

/// The test patterns that can be put on the audience screens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TestPattern {
    ColorBars,
    Grid,
    White,
}

impl TestPattern {
    /// The pattern as a slide of shapes, filling the screen.
    pub fn slide(self) -> Slide {
        use sp_model::{Rgba, Shape};
        let rect = |x: f32, y: f32, w: f32, h: f32, color: Rgba| {
            let mut shape = Object::shape(Shape::Rectangle);
            shape.frame = Some(SlideRect { x, y, w, h });
            shape.fill = Some(color);
            shape
        };
        let (w, h) = (sp_model::SLIDE_WIDTH, sp_model::SLIDE_HEIGHT);
        let objects = match self {
            TestPattern::White => vec![rect(0.0, 0.0, w, h, Rgba::WHITE)],
            TestPattern::ColorBars => {
                let colors = [
                    Rgba::rgb(192, 192, 192),
                    Rgba::rgb(192, 192, 0),
                    Rgba::rgb(0, 192, 192),
                    Rgba::rgb(0, 192, 0),
                    Rgba::rgb(192, 0, 192),
                    Rgba::rgb(192, 0, 0),
                    Rgba::rgb(0, 0, 192),
                ];
                let bar = w / colors.len() as f32;
                colors
                    .iter()
                    .enumerate()
                    .map(|(index, color)| rect(bar * index as f32, 0.0, bar + 1.0, h, *color))
                    .collect()
            }
            TestPattern::Grid => {
                let mut lines = Vec::new();
                let white = Rgba::WHITE;
                let mut x = 0.0;
                while x <= w {
                    lines.push(rect(x - 1.0, 0.0, 2.0, h, white));
                    x += 120.0;
                }
                let mut y = 0.0;
                while y <= h {
                    lines.push(rect(0.0, y - 1.0, w, 2.0, white));
                    y += 120.0;
                }
                // A circle in the middle and the screen's edge, to check shape and fit.
                let mut circle = Object::shape(Shape::Ellipse);
                circle.frame = Some(SlideRect { x: (w - h) / 2.0, y: 0.0, w: h, h });
                circle.fill = None;
                circle.stroke = Some(sp_model::StrokeLook { width: 4.0, color: Rgba::rgb(255, 200, 0) });
                lines.push(circle);
                let mut edge = Object::shape(Shape::Rectangle);
                edge.frame = Some(SlideRect { x: 2.0, y: 2.0, w: w - 4.0, h: h - 4.0 });
                edge.fill = None;
                edge.stroke = Some(sp_model::StrokeLook { width: 4.0, color: Rgba::rgb(255, 60, 60) });
                lines.push(edge);
                lines
            }
        };
        Slide::new(objects)
    }
}

fn file() -> Option<std::path::PathBuf> {
    crate::bible_store::data_dir().map(|dir| dir.join("props.json"))
}

pub fn load() -> Vec<Prop> {
    file()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save(props: &[Prop]) -> Result<(), String> {
    let path = file()
        .ok_or_else(|| t("media.no_place", "Could not find where to keep settings on this computer.").to_owned())?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| {
            tf(
                "media.no_folder",
                "Could not make the app's folder: {error}",
                &[("error", &crate::notices::write_problem(&e))],
            )
        })?;
    }
    let partial = path.with_extension("json.part");
    std::fs::write(&partial, serde_json::to_string_pretty(props).unwrap_or_default())
        .and_then(|()| std::fs::rename(&partial, &path))
        .map_err(|e| {
            tf(
                "tools.props.not_saved",
                "Could not save the props: {error}",
                &[("error", &crate::notices::write_problem(&e))],
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_test_pattern_fills_the_screen_with_shapes_and_builds() {
        for pattern in [TestPattern::ColorBars, TestPattern::Grid, TestPattern::White] {
            let slide = pattern.slide();
            assert!(!slide.objects.is_empty());
            assert!(slide.objects.iter().all(|object| object.as_text().is_none()));
            let built = sp_render::build_scene(
                &mut sp_render::Fonts::new(),
                Some(&slide),
                &sp_model::Theme::builtin(),
                640,
                360,
            );
            assert!(!built.overflow);
        }
        assert_eq!(TestPattern::ColorBars.slide().objects.len(), 7);
    }

    #[test]
    fn props_and_a_message_make_one_layer_and_survive_their_file() {
        let a = Prop::new("Logo".to_owned());
        let b = a.duplicate();
        assert_ne!((a.id, a.slide.objects[0].id), (b.id, b.slide.objects[0].id));
        assert_eq!(b.name, "Logo copy");
        let message = crate::messages::slide("Hello", crate::messages::Look::default());
        let layer = overlay(&[a.slide.clone(), b.slide.clone()], Some(&message)).unwrap();
        assert_eq!(layer.objects.len(), 3);
        assert_eq!(layer.objects[2].as_text(), Some("Hello"), "the message is on top");
        assert!(overlay(&[], None).is_none());
        let saved = vec![Prop { auto_clear: Some(10), ..a }];
        assert_eq!(serde_json::from_str::<Vec<Prop>>(&serde_json::to_string(&saved).unwrap()).unwrap(), saved);
    }
}
