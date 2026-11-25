//! SoftPresenter's own song file: one JSON document per presentation.
//!
//! The file says what it is and which version of the format it uses, so a newer file is
//! refused with a clear message instead of being half-read.

use serde::{Deserialize, Serialize};

use crate::types::Presentation;

/// The value of `"format"` in every presentation file.
const FORMAT_NAME: &str = "softpresenter-presentation";
/// What songs saved before the project was renamed say they are; they still open.
const OLD_FORMAT_NAME: &str = "presentercraft-presentation";
/// The newest version of the format this build writes and understands. A song is written as
/// version 5 only when a slide has a box for a phone's or iPad's screen (version 5 added them),
/// as version 4 only when a slide has an NDI source box, as version 3 only when it has a web page
/// box (version 3 added them); as version 2 when a
/// slide has a "go to song" action, which since version 2 names the song by its id; any other
/// song is still written as version 1, which older builds read.
pub const FORMAT_VERSION: u32 = 5;

#[derive(Serialize, Deserialize)]
struct File {
    format: String,
    version: u32,
    #[serde(flatten)]
    presentation: Presentation,
}

/// Writes a presentation as a JSON document, laid out to be read by people too.
pub fn to_json(presentation: &Presentation) -> String {
    let goes_to_songs = presentation
        .slides()
        .any(|(_, slide)| slide.actions.iter().any(|action| matches!(action, crate::ShowAction::Song { .. })));
    let shows_web = presentation
        .slides()
        .any(|(_, slide)| slide.objects.iter().any(|object| matches!(object.kind, crate::ObjectKind::Web { .. })));
    let shows_ndi = presentation
        .slides()
        .any(|(_, slide)| slide.objects.iter().any(|object| matches!(object.kind, crate::ObjectKind::Ndi { .. })));
    let shows_mirror = presentation
        .slides()
        .any(|(_, slide)| slide.objects.iter().any(|object| matches!(object.kind, crate::ObjectKind::Mirror { .. })));
    let version = if shows_mirror {
        5
    } else if shows_ndi {
        4
    } else if shows_web {
        3
    } else if goes_to_songs {
        2
    } else {
        1
    };
    let file = File { format: FORMAT_NAME.to_owned(), version, presentation: presentation.clone() };
    // A presentation has no value that JSON cannot hold, so this cannot fail.
    let mut text = serde_json::to_string_pretty(&file).unwrap_or_default();
    text.push('\n');
    text
}

/// Reads a presentation from a JSON document. The error is a plain sentence for the operator.
pub fn from_json(text: &str) -> Result<Presentation, String> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("this is not a SoftPresenter song file ({e})"))?;
    if !matches!(value.get("format").and_then(|f| f.as_str()), Some(FORMAT_NAME | OLD_FORMAT_NAME)) {
        return Err("this is not a SoftPresenter song file".to_owned());
    }
    let version = value.get("version").and_then(serde_json::Value::as_u64).unwrap_or(0);
    if version == 0 {
        return Err("the song file does not say which version it is".to_owned());
    }
    if version > u64::from(FORMAT_VERSION) {
        return Err(format!(
            "the song file was made by a newer SoftPresenter (format {version}; this one reads up to {FORMAT_VERSION})"
        ));
    }
    serde_json::from_value::<File>(value)
        .map(|file| file.presentation)
        .map_err(|e| format!("the song file is damaged ({e})"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_song_saved_before_the_project_was_renamed_still_opens() {
        let song = crate::parse_song_text("Old", "Words\n");
        let text = to_json(&song).replace(FORMAT_NAME, OLD_FORMAT_NAME);
        assert!(text.contains("presentercraft-presentation"));
        assert_eq!(from_json(&text).map(|read| read.title), Ok("Old".to_owned()));
    }
    use crate::theme::SlideRect;
    use crate::{Role, parse_song_text};

    fn song() -> Presentation {
        let mut p = parse_song_text("Утро", "[Verse 1]\nLight rises\n--\nСвет встаёт\n\nAlone\n\n[Chorus]\nSing\n");
        // One object moved, turned and faded by hand.
        let object = &mut p.groups[0].slides[0].objects[1];
        object.frame = Some(SlideRect { x: 100.0, y: 700.5, w: 1720.0, h: 250.0 });
        object.rotation = -3.5;
        object.opacity = 0.8;
        let crate::ObjectKind::Text { style, .. } = &mut object.kind else { panic!("a text object") };
        style.fill = Some(crate::Rgba::rgb(10, 200, 30));
        style.align = Some(crate::Align::Left);
        style.size = Some(48.0);
        p.groups[0].slides[1].notes = Some("Wait for the band".to_owned());
        p.groups[1].slides[0].enabled = false;
        p
    }

    #[test]
    fn a_song_without_styled_words_is_saved_byte_for_byte_as_before() {
        let text = to_json(&song());
        assert!(!text.contains("spans"), "no spans, no key");
        // Read and written again, the file is the same bytes.
        assert_eq!(to_json(&from_json(&text).unwrap()), text);
    }

    #[test]
    fn styled_words_are_saved_in_the_song_file_and_come_back() {
        let mut p = song();
        let crate::ObjectKind::Text { spans, .. } = &mut p.groups[0].slides[0].objects[0].kind else {
            panic!("a text object")
        };
        *spans = vec![crate::TextSpan::new(
            0,
            5,
            crate::SpanStyle { weight: Some(700), italic: Some(true), ..crate::SpanStyle::default() },
        )];
        let text = to_json(&p);
        assert!(text.contains("\"spans\""));
        assert_eq!(from_json(&text).unwrap(), p);
        // A hostile file: a span past the words is cut to them, and one that is nonsense goes.
        let spans_of = |text: &str| {
            let read = from_json(text).unwrap();
            let crate::ObjectKind::Text { spans, .. } = &read.groups[0].slides[0].objects[0].kind else {
                panic!("a text object")
            };
            spans.clone()
        };
        let long = spans_of(&text.replace("\"end\": 5", "\"end\": 99999"));
        assert_eq!((long[0].start, long[0].end), (0, "Light rises".chars().count()));
        assert!(spans_of(&text.replace("\"weight\": 700", "\"weight\": 0")).is_empty());
    }

    #[test]
    fn a_song_reads_back_exactly_as_it_was_written() {
        let p = song();
        assert_eq!(from_json(&to_json(&p)).unwrap(), p);
    }

    #[test]
    fn the_file_names_its_format_and_version_and_leaves_out_defaults() {
        let text = to_json(&parse_song_text("T", "A\n"));
        assert!(text.contains(r#""format": "softpresenter-presentation""#));
        assert!(text.contains(r#""version": 1"#));
        assert!(text.contains(r#""kind": "text""#));
        assert!(text.contains(r#""role": "primary""#));
        for absent in ["frame", "rotation", "opacity", "notes", "label", "style"] {
            assert!(!text.contains(absent), "{absent} should be left out when it is the default");
        }
    }

    #[test]
    fn missing_optional_values_get_their_defaults() {
        let text = r#"{"format":"softpresenter-presentation","version":1,
            "id":"00000000-0000-0000-0000-000000000001","title":"T","groups":[{
            "id":"00000000-0000-0000-0000-000000000002","name":"V","color":{"r":1,"g":2,"b":3,"a":255},"slides":[{
            "id":"00000000-0000-0000-0000-000000000003","objects":[{
            "id":"00000000-0000-0000-0000-000000000004","name":"Main","kind":"text","text":"Hello","later":"ignored"}]}]}]}"#;
        let p = from_json(text).unwrap();
        let slide = &p.groups[0].slides[0];
        assert!(slide.enabled);
        let object = &slide.objects[0];
        assert_eq!((object.rotation, object.opacity, object.frame, object.role.clone()), (0.0, 1.0, None, None));
        assert_eq!(object.as_text(), Some("Hello"));
    }

    #[test]
    fn a_newer_file_is_refused_not_half_read() {
        let newer = format!(r#""version": {}"#, FORMAT_VERSION + 1);
        let text = to_json(&song()).replace(r#""version": 1"#, &newer);
        assert!(from_json(&text).unwrap_err().contains("newer SoftPresenter"));
    }

    #[test]
    fn a_song_with_a_web_page_box_is_written_as_version_3() {
        let mut web = song();
        web.groups[0].slides[0].objects.push(crate::Object::web("https://example.com".to_owned()));
        let text = to_json(&web);
        assert!(text.contains(r#""version": 3"#) && text.contains(r#""kind": "web""#), "{text}");
        assert_eq!(from_json(&text).unwrap(), web);
    }

    #[test]
    fn a_song_with_an_ndi_source_box_is_written_as_version_4_and_comes_back() {
        let mut ndi = song();
        let mut object = crate::Object::ndi("STREAM-PC (Camera 1)".to_owned());
        if let crate::ObjectKind::Ndi { scaling, quality, sound, .. } = &mut object.kind {
            *scaling = crate::ImageFit::Fill;
            *quality = crate::NdiQuality::Light;
            *sound = true;
        }
        ndi.groups[0].slides[0].objects.push(object);
        let text = to_json(&ndi);
        assert!(text.contains(r#""version": 4"#) && text.contains(r#""kind": "ndi""#), "{text}");
        assert!(text.contains(r#""source": "STREAM-PC (Camera 1)""#));
        assert!(text.contains(r#""scaling": "fill""#) && text.contains(r#""quality": "light""#));
        assert_eq!(from_json(&text).unwrap(), ndi);
        // The box a fresh Add gives, and no sound, writes no `sound`.
        let mut plain = song();
        plain.groups[0].slides[0].objects.push(crate::Object::ndi(String::new()));
        assert!(!to_json(&plain).contains("sound"));
        assert_eq!(from_json(&to_json(&plain)).unwrap(), plain);
    }

    #[test]
    fn a_song_with_a_phone_screen_box_is_written_as_version_5_and_comes_back() {
        let mut mirrored = song();
        let mut object = crate::Object::mirror();
        assert_eq!(object.name, "Phone or iPad Screen");
        assert!(matches!(object.kind, crate::ObjectKind::Mirror { scaling: crate::ImageFit::Fit }));
        if let crate::ObjectKind::Mirror { scaling } = &mut object.kind {
            *scaling = crate::ImageFit::Fill;
        }
        mirrored.groups[0].slides[0].objects.push(object);
        // With an NDI box beside it, the newer of the two versions is written.
        mirrored.groups[0].slides[0].objects.push(crate::Object::ndi("A (B)".to_owned()));
        let text = to_json(&mirrored);
        assert!(text.contains(r#""version": 5"#) && text.contains(r#""kind": "mirror""#), "{text}");
        assert!(text.contains(r#""scaling": "fill""#));
        assert_eq!(from_json(&text).unwrap(), mirrored);
        // A box with only its kind reads with the starting setting; it is not text, and an
        // empty copy of its slide keeps it.
        let thin = text.replace(r#""scaling": "fill""#, r#""x": 1"#);
        let read = from_json(&thin).unwrap();
        let slide = &read.groups[0].slides[0];
        let boxed =
            slide.objects.iter().find(|object| matches!(object.kind, crate::ObjectKind::Mirror { .. })).unwrap();
        assert!(matches!(boxed.kind, crate::ObjectKind::Mirror { scaling: crate::ImageFit::Fit }));
        assert!(boxed.as_text().is_none() && boxed.text_style().is_none());
        assert!(
            slide.empty_copy().objects.iter().any(|object| matches!(object.kind, crate::ObjectKind::Mirror { .. }))
        );
    }

    #[test]
    fn a_file_without_any_ndi_box_loads_unchanged_and_is_not_written_as_newer() {
        let text = to_json(&song());
        assert!(text.contains(r#""version": 1"#), "older builds still read it");
        assert_eq!(to_json(&from_json(&text).unwrap()), text);
        // A box with only its kind and source (as an earlier note of the format would have it)
        // reads with the starting settings.
        let mut ndi = song();
        ndi.groups[0].slides[0].objects.push(crate::Object::ndi("A (B)".to_owned()));
        let thin = to_json(&ndi).replace(r#""scaling": "fit","#, "").replace(r#""quality": "full""#, r#""x": 1"#);
        let read = from_json(&thin).unwrap();
        assert!(matches!(&read.groups[0].slides[0].objects.last().unwrap().kind,
            crate::ObjectKind::Ndi { scaling, quality, .. } if *scaling == crate::ImageFit::Fit && *quality == crate::NdiQuality::Full));
    }

    #[test]
    fn a_song_that_goes_to_another_song_is_written_as_version_2() {
        let mut going = song();
        going.groups[0].slides[0].actions.push(crate::ShowAction::Song { key: "1".to_owned() });
        let text = to_json(&going);
        assert!(text.contains(r#""version": 2"#), "{text}");
        assert_eq!(from_json(&text).unwrap(), going);
        assert!(to_json(&song()).contains(r#""version": 1"#), "any other song stays readable by older builds");
    }

    #[test]
    fn other_files_are_refused_in_plain_words() {
        assert!(from_json("Light rises\n").unwrap_err().contains("not a SoftPresenter song file"));
        assert!(from_json(r#"{"title":"T"}"#).unwrap_err().contains("not a SoftPresenter song file"));
        assert!(from_json(r#"{"format":"softpresenter-presentation"}"#).unwrap_err().contains("which version"));
        let damaged = r#"{"format":"softpresenter-presentation","version":1,"title":5}"#;
        assert!(from_json(damaged).unwrap_err().contains("damaged"));
    }

    #[test]
    fn roles_survive_so_other_parts_can_find_the_words() {
        let p = from_json(&to_json(&song())).unwrap();
        assert_eq!(p.slide(0).unwrap().1.text_for(&Role::translation()), Some("Свет встаёт"));
    }
}
