//! TimelyChurch's worship data, as its tools showed it in November 2026. The device interface
//! may name or shape things differently; these are the fields the mapping uses. Every one may
//! be missing or `null` (an empty name, no songs, a zero) without losing the rest. Fields about
//! people (team leads, leaders, rehearsal answers) are left out on purpose: they are never read.

use serde::{Deserialize, Deserializer};

/// A field's value, or its empty value when TimelyChurch sends `null`.
fn or_empty<'de, D: Deserializer<'de>, T: Deserialize<'de> + Default>(from: D) -> Result<T, D::Error> {
    Ok(Option::<T>::deserialize(from)?.unwrap_or_default())
}

/// A setlist: the songs a worship team sings at one service, in order.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Setlist {
    #[serde(deserialize_with = "or_empty")]
    pub id: u64,
    #[serde(deserialize_with = "or_empty")]
    pub name: String,
    /// The service's date, `YYYY-MM-DD`.
    pub event_date: Option<String>,
    /// The calendar event the setlist is for, which a service plan can be for too.
    pub linked_event_id: Option<u64>,
    #[serde(deserialize_with = "or_empty")]
    pub is_active: bool,
    #[serde(deserialize_with = "or_empty")]
    pub songs: Vec<SetlistSong>,
}

/// One song of a setlist. It has no id of its own: its place (`order`) and its song say which.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct SetlistSong {
    /// Its place in the setlist, from 1.
    #[serde(deserialize_with = "or_empty")]
    pub order: u32,
    #[serde(deserialize_with = "or_empty")]
    pub song_id: u64,
    #[serde(deserialize_with = "or_empty")]
    pub title: String,
    /// The arrangement the team sings; `null` for the song's default. Only `null` has been
    /// seen, so it is read as a number or an object with an `id`.
    pub arrangement: Option<serde_json::Value>,
    /// The key the team sings it in (`Ebm`, `C#`).
    pub target_key: Option<String>,
    pub notes: Option<String>,
    /// The part of the service it belongs to, by the label a service plan gives that part.
    pub worship_spot: Option<String>,
}

impl SetlistSong {
    /// The arrangement's id, when the setlist names one.
    pub fn arrangement_id(&self) -> Option<u64> {
        match self.arrangement.as_ref()? {
            serde_json::Value::Number(number) => number.as_u64(),
            serde_json::Value::Object(fields) => fields.get("id").and_then(serde_json::Value::as_u64),
            _ => None,
        }
    }
}

/// A service plan: the whole running order of a service.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct ServicePlan {
    #[serde(deserialize_with = "or_empty")]
    pub id: u64,
    #[serde(deserialize_with = "or_empty")]
    pub title: String,
    pub date: Option<String>,
    /// `HH:MM:SS`, in the church's time.
    pub time: Option<String>,
    /// `draft` is the only one seen.
    pub status: Option<String>,
    pub event_id: Option<u64>,
    #[serde(deserialize_with = "or_empty")]
    pub items: Vec<PlanItem>,
}

/// One item of a service plan. Like a setlist's songs, it has no id of its own.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct PlanItem {
    #[serde(deserialize_with = "or_empty")]
    pub order: u32,
    /// `song`, `worship` (a place for the setlist's songs), `section` (a heading), `scripture`,
    /// `prayer`, `sermon`, `offering`, `announcement`, and perhaps others.
    #[serde(rename = "type", deserialize_with = "or_empty")]
    pub kind: String,
    #[serde(deserialize_with = "or_empty")]
    pub title: String,
    pub song_id: Option<u64>,
    pub song_title: Option<String>,
    pub scripture_reference: Option<String>,
    /// The label of the part of the service a `worship` item holds.
    pub worship_spot: Option<String>,
}

/// A song in the church's library: its details and its arrangements. Its words are in an
/// arrangement's chart ([`Chart`]); the plain `lyrics` it can also carry are not used (they can
/// be missing, or carry ChordPro directives).
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Song {
    #[serde(deserialize_with = "or_empty")]
    pub id: u64,
    #[serde(deserialize_with = "or_empty")]
    pub title: String,
    pub artist: Option<String>,
    pub composer: Option<String>,
    /// The song's number in the CCLI catalogue, as text ("1234567").
    pub ccli_number: Option<serde_json::Value>,
    pub copyright_info: Option<String>,
    pub default_key: Option<String>,
    pub notes: Option<String>,
    #[serde(deserialize_with = "or_empty")]
    pub arrangements: Vec<Arrangement>,
}

impl Song {
    /// The CCLI number's digits; empty when it has none.
    pub fn ccli(&self) -> String {
        match &self.ccli_number {
            Some(serde_json::Value::String(text)) => text.chars().filter(char::is_ascii_digit).collect(),
            Some(serde_json::Value::Number(number)) => number.to_string(),
            _ => String::new(),
        }
    }

    /// The arrangement with this id, or the song's default when there is no id.
    pub fn arrangement(&self, id: Option<u64>) -> Option<&Arrangement> {
        match id {
            Some(id) => self.arrangements.iter().find(|arrangement| arrangement.id == id),
            None => self.arrangements.iter().find(|arrangement| arrangement.is_default).or(self.arrangements.first()),
        }
    }
}

/// One arrangement of a song: a chart of its own, in a key. It is not an order of sections.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Arrangement {
    #[serde(deserialize_with = "or_empty")]
    pub id: u64,
    #[serde(deserialize_with = "or_empty")]
    pub name: String,
    pub key: Option<String>,
    #[serde(deserialize_with = "or_empty")]
    pub is_default: bool,
    /// `chordpro` (chords in square brackets in the words) or `chords_above` (chords on a line
    /// of their own above the words).
    pub chart_format: Option<String>,
}

/// An arrangement's chart: its sections in the order they are sung, every repeat written where
/// it comes.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Chart {
    #[serde(deserialize_with = "or_empty")]
    pub song_id: u64,
    #[serde(deserialize_with = "or_empty")]
    pub arrangement_id: u64,
    pub key: Option<String>,
    #[serde(deserialize_with = "or_empty")]
    pub sections: Vec<ChartSection>,
}

/// One section of a chart. The name is free text ("Verse 1", "Chorus - 2x", "Припев"), or
/// `null` for lines before any named section. A line holds ChordPro chords in square brackets
/// (`[G]words`); an empty line ends a verse's part. A section with a name and no lines means
/// "sing that section again here".
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct ChartSection {
    pub name: Option<String>,
    #[serde(deserialize_with = "or_empty")]
    pub lines: Vec<String>,
}

/// A song's translation into one language: the song's lines, each with its translated line.
/// TimelyChurch shares the sections out into pieces of at most the slide style's lines per
/// slide, so a section's name can come more than once; the pieces are only for showing.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Translation {
    #[serde(deserialize_with = "or_empty")]
    pub song_id: u64,
    /// A lower-case code (`ru`, `es`, `pt-br`).
    #[serde(deserialize_with = "or_empty")]
    pub language: String,
    /// Always `null` so far; perhaps `draft` or `approved`.
    pub status: Option<String>,
    pub title_translated: Option<String>,
    #[serde(deserialize_with = "or_empty")]
    pub sections: Vec<TranslationSection>,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct TranslationSection {
    pub name: Option<String>,
    pub caption: Option<String>,
    #[serde(deserialize_with = "or_empty")]
    pub rows: Vec<TranslationRow>,
}

/// One line of the song (without its chords) and its translation; `translated` is empty (or
/// perhaps `null`) for a line nobody has translated.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct TranslationRow {
    #[serde(deserialize_with = "or_empty")]
    pub source: String,
    #[serde(deserialize_with = "or_empty")]
    pub translated: String,
}

/// The church's saved look for congregation slides, and whether it ever saved one (when not,
/// the look is TimelyChurch's own defaults).
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct StyleReply {
    #[serde(deserialize_with = "or_empty")]
    pub style: SlideStyle,
    #[serde(deserialize_with = "or_empty")]
    pub has_default: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct SlideStyle {
    /// Colours as `#RRGGBB`.
    #[serde(deserialize_with = "or_empty")]
    pub background: String,
    #[serde(deserialize_with = "or_empty")]
    pub original_colour: String,
    #[serde(deserialize_with = "or_empty")]
    pub translation_colour: String,
    #[serde(deserialize_with = "or_empty")]
    pub original_size: u32,
    #[serde(deserialize_with = "or_empty")]
    pub translation_size: u32,
    #[serde(deserialize_with = "or_empty")]
    pub gap: u32,
    /// `original_first` is the only one seen.
    #[serde(deserialize_with = "or_empty")]
    pub layout: String,
    #[serde(deserialize_with = "or_empty")]
    pub lines_per_slide: u32,
}
