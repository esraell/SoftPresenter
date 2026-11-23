//! Filling in linked text: each text box linked to something (the time, a timer, the song's
//! title…) shows its value now in place of its words, on the screens and in the preview.

use std::borrow::Cow;
use std::time::Instant;

use sp_model::{Slide, TextLink};

use crate::timers::Timer;

/// What linked text can show, as it is now.
#[derive(Clone, Copy, Debug, Default)]
pub struct Values<'a> {
    pub timers: &'a [Timer],
    pub title: Option<&'a str>,
    pub group: Option<&'a str>,
    pub next: Option<&'a Slide>,
    pub stage_message: Option<&'a str>,
    pub copyright: Option<&'a str>,
}

/// What a link shows now, on a slide.
pub fn value(link: TextLink, slide: &Slide, values: &Values) -> String {
    let owned = |text: Option<&str>| text.unwrap_or("").to_owned();
    match link {
        TextLink::Clock => chrono::Local::now().format("%-I:%M").to_string(),
        TextLink::Date => chrono::Local::now().format("%A, %-d %B %Y").to_string(),
        TextLink::Timer { id } => values
            .timers
            .iter()
            .find(|timer| timer.id == id)
            .map_or_else(String::new, |timer| timer.text(Instant::now(), crate::timers::seconds_into_day())),
        TextLink::SongTitle => owned(values.title),
        TextLink::Group => owned(values.group),
        TextLink::SlideLabel => owned(slide.label.as_deref()),
        TextLink::NextSlide => values.next.map_or_else(String::new, |next| {
            next.texts()
                .into_iter()
                .map(|(_, text)| text)
                .filter(|text| !text.trim().is_empty())
                .collect::<Vec<_>>()
                .join("\n")
        }),
        TextLink::Notes => owned(slide.notes.as_deref()),
        TextLink::StageMessage => owned(values.stage_message),
        TextLink::Copyright => owned(values.copyright),
    }
}

/// The slide with each linked text box showing its value now. A slide with no links is
/// handed back as it is.
pub fn fill<'s>(slide: &'s Slide, values: &Values) -> Cow<'s, Slide> {
    if !slide.objects.iter().any(|object| object.link.is_some()) {
        return Cow::Borrowed(slide);
    }
    let mut filled = slide.clone();
    for object in &mut filled.objects {
        if let Some(link) = object.link {
            object.set_text(&value(link, slide, values));
        }
    }
    Cow::Owned(filled)
}

/// True when a slide has linked text that changes as time passes.
pub fn ticks(slide: &Slide) -> bool {
    slide.objects.iter().any(|object| object.link.is_some_and(TextLink::ticks))
}

/// The words a linked box is given in the editor, so its picture says what it shows.
pub fn placeholder(link: TextLink, timer_name: Option<&str>) -> String {
    match (link, timer_name) {
        (TextLink::Timer { .. }, Some(name)) => format!("[Timer: {name}]"),
        (link, _) => format!("[{}]", link.name()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sp_model::{Object, Role};

    #[test]
    fn linked_boxes_show_their_values_and_others_keep_their_words() {
        let mut timer = Timer::new("Sermon".to_owned());
        timer.start(Instant::now());
        let mut clock = Object::text(Role::primary(), "[Clock]");
        clock.link = Some(TextLink::Clock);
        let mut countdown = Object::text(Role::translation(), "[Timer]");
        countdown.link = Some(TextLink::Timer { id: timer.id });
        let mut title = Object::text(Role("title".to_owned()), "[Song Title]");
        title.link = Some(TextLink::SongTitle);
        let plain = Object::text(Role("words".to_owned()), "Welcome");
        let slide = Slide::new(vec![clock, countdown, title, plain]);
        let timers = [timer];
        let values = Values { timers: &timers, title: Some("Amazing Grace"), ..Values::default() };
        let filled = fill(&slide, &values);
        let words: Vec<&str> = filled.objects.iter().filter_map(|o| o.as_text()).collect();
        assert!(words[0].contains(':') && !words[0].contains('['), "{}", words[0]);
        assert!(words[1].starts_with("00:"), "{}", words[1]);
        assert_eq!(&words[2..], ["Amazing Grace", "Welcome"]);
        assert!(ticks(&slide));
        // A slide with no links is not copied.
        let unlinked = Slide::new(vec![Object::text(Role::primary(), "Hi")]);
        assert!(matches!(fill(&unlinked, &values), Cow::Borrowed(_)));
        assert!(!ticks(&unlinked));
        assert_eq!(placeholder(TextLink::Timer { id: sp_model::Uuid::nil() }, Some("Sermon")), "[Timer: Sermon]");
    }
}
