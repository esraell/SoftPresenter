//! Messages: short notices shown over the slides on the audience screens, such as "Parents of
//! child 12, please come to the nursery". A message's words can hold tokens that are filled in
//! as it is shown: `{Clock}` for the time, `{Timer: name}` for a timer. Messages are kept in
//! `messages.json`.

use std::time::Instant;

use serde::{Deserialize, Serialize};
use sp_model::{Align, Object, Rgba, Role, Slide, SlideRect, TextStyle, Uuid, VAlign};

use crate::i18n::{t, tf};
use crate::timers::Timer;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub id: Uuid,
    pub name: String,
    pub text: String,
    /// Taken off the screen by itself after this many seconds; `None` waits to be cleared.
    #[serde(default)]
    pub dismiss_after: Option<u32>,
    /// Where and how the message is drawn.
    #[serde(default)]
    pub look: Look,
}

impl Message {
    pub fn new(name: String) -> Message {
        Message { id: Uuid::new_v4(), name, text: String::new(), dismiss_after: None, look: Look::default() }
    }

    /// A copy with an id of its own.
    pub fn duplicate(&self) -> Message {
        Message {
            id: Uuid::new_v4(),
            name: tf("tools.copy_name", "{name} copy", &[("name", &self.name)]),
            ..self.clone()
        }
    }
}

/// Where and how a message is drawn over the slides.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Look {
    /// A dark band across the bottom of the screen.
    #[default]
    BottomBand,
    /// A dark band across the top of the screen.
    TopBand,
    /// A dark box in the middle of the screen, in larger words.
    Centre,
    /// Words on a dark strip in the lower left, like a name caption.
    LowerThird,
    /// Small outlined words along the bottom edge, with no band.
    Caption,
}

impl Look {
    pub const ALL: [Look; 5] = [Look::BottomBand, Look::TopBand, Look::Centre, Look::LowerThird, Look::Caption];

    pub fn name(self) -> &'static str {
        match self {
            Look::BottomBand => t("tools.message.look.bottom", "Band at the Bottom"),
            Look::TopBand => t("tools.message.look.top", "Band at the Top"),
            Look::Centre => t("tools.message.look.centre", "Box in the Middle"),
            Look::LowerThird => t("tools.message.look.lower_third", "Lower Third"),
            Look::Caption => t("tools.message.look.caption", "Caption, No Band"),
        }
    }
}

/// The token for the time of day.
pub const CLOCK_TOKEN: &str = "{Clock}";

/// The token for a timer's value.
pub fn timer_token(name: &str) -> String {
    format!("{{Timer: {name}}}")
}

/// Writes a timer's new name into every message that shows it, so `{Timer: old}` becomes
/// `{Timer: new}`. Returns true when a message changed.
pub fn rename_timer(messages: &mut [Message], old: &str, new: &str) -> bool {
    let (was, now) = (timer_token(old), timer_token(new));
    let mut changed = false;
    for message in messages.iter_mut().filter(|message| message.text.contains(&was)) {
        message.text = message.text.replace(&was, &now);
        changed = true;
    }
    changed
}

/// True when the words have a token whose value changes as time passes.
pub fn has_tokens(text: &str) -> bool {
    text.contains(CLOCK_TOKEN) || text.contains("{Timer:")
}

/// The words with each token replaced by its value now.
pub fn fill_in(text: &str, timers: &[Timer], now: Instant, seconds_into_day: u32) -> String {
    let mut out = text.replace(CLOCK_TOKEN, &chrono::Local::now().format("%-I:%M").to_string());
    for timer in timers {
        let token = timer_token(&timer.name);
        if out.contains(&token) {
            out = out.replace(&token, &timer.text(now, seconds_into_day));
        }
    }
    out
}

/// A message as a layer over the slide, drawn in its look.
pub fn slide(words: &str, look_of: Look) -> Slide {
    let builtin = sp_model::Theme::builtin();
    let mut look = builtin.templates[0].boxes[0].clone();
    look.size = 60.0;
    look.min_size = 28.0;
    look.weight = 600;
    look.fill = Rgba::WHITE;
    look.align = Align::Center;
    look.valign = VAlign::Middle;
    look.outline_width = 0.0;
    look.shadow = false;
    look.margins = [60.0, 16.0, 60.0, 16.0];
    let band = Some(Rgba { r: 0, g: 0, b: 0, a: 200 });
    let (frame, fill) = match look_of {
        Look::BottomBand => (SlideRect { x: 0.0, y: 890.0, w: 1920.0, h: 190.0 }, band),
        Look::TopBand => (SlideRect { x: 0.0, y: 0.0, w: 1920.0, h: 190.0 }, band),
        Look::Centre => {
            look.size = 84.0;
            (SlideRect { x: 260.0, y: 340.0, w: 1400.0, h: 400.0 }, Some(Rgba { r: 12, g: 12, b: 14, a: 242 }))
        }
        Look::LowerThird => {
            look.align = Align::Left;
            look.size = 54.0;
            (SlideRect { x: 80.0, y: 820.0, w: 1100.0, h: 150.0 }, band)
        }
        Look::Caption => {
            look.size = 40.0;
            look.shadow = true;
            look.outline_width = 3.0;
            (SlideRect { x: 0.0, y: 980.0, w: 1920.0, h: 90.0 }, None)
        }
    };
    let mut object = Object::text(Role("message".to_owned()), words);
    object.frame = Some(frame);
    object.fill = fill;
    if let Some(style) = object.text_style_mut() {
        *style = TextStyle::all(&look);
    }
    Slide::new(vec![object])
}

fn file() -> Option<std::path::PathBuf> {
    crate::bible_store::data_dir().map(|dir| dir.join("messages.json"))
}

pub fn load() -> Vec<Message> {
    file()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save(messages: &[Message]) -> Result<(), String> {
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
    std::fs::write(&partial, serde_json::to_string_pretty(messages).unwrap_or_default())
        .and_then(|()| std::fs::rename(&partial, &path))
        .map_err(|e| {
            tf(
                "tools.messages.not_saved",
                "Could not save the messages: {error}",
                &[("error", &crate::notices::write_problem(&e))],
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_timers_new_name_is_written_into_the_messages_that_show_it() {
        let mut messages = vec![Message::new("A".to_owned()), Message::new("B".to_owned())];
        messages[0].text = format!("{} then {}", timer_token("Sermon"), timer_token("Sermon"));
        messages[1].text = format!("{} at {CLOCK_TOKEN}", timer_token("Sermon notes"));
        assert!(rename_timer(&mut messages, "Sermon", "Talk"));
        assert_eq!(messages[0].text, "{Timer: Talk} then {Timer: Talk}");
        assert_eq!(messages[1].text, "{Timer: Sermon notes} at {Clock}", "another timer's token is left");
        assert!(!rename_timer(&mut messages, "Sermon", "Talk"), "nothing left to change");
    }

    #[test]
    fn tokens_are_filled_in_with_the_time_and_timers() {
        let now = Instant::now();
        let timer = Timer::new("Service".to_owned());
        let text = format!("Starts in {} · it is {CLOCK_TOKEN}", timer_token("Service"));
        assert!(has_tokens(&text));
        let filled = fill_in(&text, &[timer], now, 0);
        assert!(filled.starts_with("Starts in 00:05:00 · it is "), "{filled}");
        assert!(!filled.contains('{'));
        assert!(!has_tokens("Welcome"));
        assert_eq!(fill_in("Welcome", &[], now, 0), "Welcome");
        // A message is one box, in a band along the bottom unless its look says otherwise.
        let band = slide("Welcome", Look::default());
        assert_eq!(band.objects.len(), 1);
        assert!(band.objects[0].frame.is_some_and(|frame| frame.y > 800.0));
        for look in Look::ALL {
            let drawn = slide("Welcome", look);
            let frame = drawn.objects[0].frame.unwrap();
            assert!(frame.x >= 0.0 && frame.y >= 0.0 && frame.x + frame.w <= 1920.0 && frame.y + frame.h <= 1080.0);
        }
        assert!(slide("Welcome", Look::TopBand).objects[0].frame.is_some_and(|frame| frame.y == 0.0));
        let copy = Message::new("M".to_owned()).duplicate();
        assert_eq!(copy.name, "M copy");
        let saved = vec![Message { text: "Hi".to_owned(), dismiss_after: Some(10), ..Message::new("M".to_owned()) }];
        assert_eq!(serde_json::from_str::<Vec<Message>>(&serde_json::to_string(&saved).unwrap()).unwrap(), saved);
    }
}
