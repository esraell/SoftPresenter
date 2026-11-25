//! TimelyChurch's Worship API, version 1: the requests and replies, as TimelyChurch described
//! them in its answer of November 2026, before its OpenAPI file. Every name the answer didn't
//! spell out is marked `assumed:` in its comment (`grep -n "assumed:" wire.rs` lists them), and
//! the likeliest other spellings are taken too (`alias`), so the OpenAPI file's names can be
//! put in by changing only this file.
//!
//! Every field may be missing or `null` without losing the rest: an empty name, no items, no
//! translations. Ids may come as numbers or as strings.

use serde::{Deserialize, Deserializer, Serialize};

/// Where the device's requests go, under the church's TimelyChurch address: version 1 of the
/// Worship API, as TimelyChurch's answer wrote the paths. A version 2 would be a new set of
/// shapes beside these.
const ROOT: &str = "/api/v1/worship";

/// `POST`: a pairing code for a device token.
pub fn pair_path() -> String {
    format!("{ROOT}/devices/pair")
}

/// `GET`: the services between two dates, `YYYY-MM-DD` (assumed: query names `from` and `to`,
/// dates without times, both days included).
pub fn services_path(from: &str, to: &str) -> String {
    format!("{ROOT}/services?from={from}&to={to}")
}

/// `GET`: a song, in an arrangement or its default (assumed: query name `arrangement`, left out
/// for the default).
pub fn song_path(song: &Id, arrangement: Option<&Id>) -> String {
    match arrangement {
        Some(arrangement) => format!("{ROOT}/songs/{song}?arrangement={arrangement}"),
        None => format!("{ROOT}/songs/{song}"),
    }
}

/// `POST`: a song shown, with an `Idempotency-Key` header.
pub fn song_uses_path() -> String {
    format!("{ROOT}/song-uses")
}

/// A field's value, or its empty value when it is `null`.
fn or_empty<'de, D: Deserializer<'de>, T: Deserialize<'de> + Default>(from: D) -> Result<T, D::Error> {
    Ok(Option::<T>::deserialize(from)?.unwrap_or_default())
}

/// An id, as TimelyChurch sends it: a number or a string (assumed: integers, as its other tools
/// give, unique in one church). Only letters, figures, `-`, `_` and `.` ever go into an address
/// ([`Id::is_safe`]).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct Id(pub String);

impl Id {
    pub fn new(id: impl Into<String>) -> Id {
        Id(id.into())
    }

    #[cfg(test)]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Fit to go into an address and a key as it is.
    pub fn is_safe(&self) -> bool {
        !self.0.is_empty()
            && self.0.len() <= 64
            && self.0.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    }
}

impl std::fmt::Display for Id {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Id {
    fn deserialize<D: Deserializer<'de>>(from: D) -> Result<Id, D::Error> {
        Ok(match serde_json::Value::deserialize(from)? {
            serde_json::Value::String(text) => Id(text.trim().to_owned()),
            serde_json::Value::Number(number) => Id(number.to_string()),
            _ => Id::default(),
        })
    }
}

/// Names, from a list of them or one line of them separated by commas.
fn names<'de, D: Deserializer<'de>>(from: D) -> Result<Vec<String>, D::Error> {
    Ok(match Option::<serde_json::Value>::deserialize(from)? {
        Some(serde_json::Value::String(line)) => {
            line.split([',', ';']).map(str::trim).filter(|name| !name.is_empty()).map(str::to_owned).collect()
        }
        Some(serde_json::Value::Array(list)) => list.iter().filter_map(name_in).collect(),
        _ => Vec::new(),
    })
}

/// A name, from a string or an object that names something (`{"name": …}`).
fn name_in(value: &serde_json::Value) -> Option<String> {
    let text = match value {
        serde_json::Value::String(text) => text.as_str(),
        serde_json::Value::Object(fields) => ["name", "section", "label", "title"]
            .iter()
            .find_map(|field| fields.get(*field).and_then(serde_json::Value::as_str))?,
        _ => return None,
    };
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// Lines, where a line may be `null` (a translation's untranslated line) or missing.
fn lines<'de, D: Deserializer<'de>>(from: D) -> Result<Vec<Option<String>>, D::Error> {
    Ok(Option::<Vec<Option<serde_json::Value>>>::deserialize(from)?
        .unwrap_or_default()
        .into_iter()
        .map(|line| match line {
            Some(serde_json::Value::String(text)) => Some(text),
            _ => None,
        })
        .collect())
}

/// What a device sends to pair: the code an administrator made in TimelyChurch (Setlists ›
/// Song Sync), the computer's name and the app's name and version.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PairRequest {
    /// assumed: "pairing_code".
    pub pairing_code: String,
    /// assumed: "device_name".
    pub device_name: String,
    /// assumed: "app_name".
    pub app_name: String,
    /// assumed: "app_version".
    pub app_version: String,
}

/// What pairing gives back: the device's token and the church's name.
#[derive(Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct PairReply {
    /// assumed: "token" (or "device_token", "access_token").
    #[serde(alias = "device_token", alias = "access_token", deserialize_with = "or_empty")]
    pub token: String,
    /// assumed: an object "church" with "id" and "name"…
    #[serde(deserialize_with = "or_empty")]
    pub church: Church,
    /// …or a field "church_name" beside the token.
    pub church_name: Option<String>,
}

/// Never shows the token, wherever a reply is printed.
impl std::fmt::Debug for PairReply {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PairReply").field("church", &self.church).field("church_name", &self.church_name).finish()
    }
}

impl PairReply {
    /// The church's name, wherever the reply put it.
    pub fn church_name(&self) -> String {
        let name = if self.church.name.trim().is_empty() {
            self.church_name.as_deref().unwrap_or("")
        } else {
            self.church.name.as_str()
        };
        super::plain(name)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Church {
    pub id: Option<Id>,
    #[serde(deserialize_with = "or_empty")]
    pub name: String,
}

/// The services list (assumed: an object with "services", or the list itself).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ServicesReply {
    pub services: Vec<Service>,
    /// How many entries of the list couldn't be read, and were left out.
    pub dropped: usize,
}

impl ServicesReply {
    /// Reads the reply's body. A service that doesn't read is left out, not the whole list.
    pub fn read(body: &str) -> Option<ServicesReply> {
        let value: serde_json::Value = serde_json::from_str(body).ok()?;
        let list = match &value {
            serde_json::Value::Array(list) => list,
            serde_json::Value::Object(fields) => {
                ["services", "data", "items"].iter().find_map(|name| fields.get(*name)?.as_array())?
            }
            _ => return None,
        };
        let services: Vec<Service> = list.iter().filter_map(|service| Service::deserialize(service).ok()).collect();
        Some(ServicesReply { dropped: list.len() - services.len(), services })
    }
}

/// One service: a setlist (the songs a team sings) or a service plan (the whole running
/// order), with its items in order.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Service {
    pub id: Id,
    /// assumed: "type", `setlist` or `plan` (or "kind"; `service_plan`).
    #[serde(rename = "type", alias = "kind", deserialize_with = "or_empty")]
    pub kind: String,
    /// assumed: "name" (or "title").
    #[serde(alias = "title", deserialize_with = "or_empty")]
    pub name: String,
    /// assumed: "date", `YYYY-MM-DD`, in the church's own time.
    pub date: Option<String>,
    /// assumed: "time", `HH:MM` or `HH:MM:SS`.
    pub time: Option<String>,
    /// The calendar event it is for, which a setlist and a plan can share (assumed: "event_id",
    /// perhaps not sent at all).
    #[serde(alias = "linked_event_id")]
    pub event_id: Option<Id>,
    /// When it last changed (assumed: "updated_at", an RFC 3339 time).
    #[serde(alias = "last_changed", alias = "changed_at", alias = "last_changed_at")]
    pub updated_at: Option<String>,
    #[serde(deserialize_with = "or_empty")]
    pub items: Vec<Item>,
}

impl Service {
    pub fn is_setlist(&self) -> bool {
        self.kind.to_lowercase().contains("setlist")
    }
}

/// One item of a service: a song, a reading, the sermon, or another place in the order.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Item {
    /// Its place, from 1 (assumed: "position", or "order"). Items come in order anyway.
    #[serde(alias = "order", deserialize_with = "or_empty")]
    pub position: u32,
    /// assumed: "type": `song`, `reading`, `sermon`, `heading`, `worship` (a place for a
    /// setlist's songs) or another word for a placeholder.
    #[serde(rename = "type", alias = "kind", deserialize_with = "or_empty")]
    pub kind: String,
    #[serde(alias = "name", deserialize_with = "or_empty")]
    pub title: String,
    /// assumed: "song_id".
    pub song_id: Option<Id>,
    /// assumed: "arrangement_id"; `null` for the song's default.
    pub arrangement_id: Option<Id>,
    /// The key it is sung in (`G`, `Ebm`).
    pub key: Option<String>,
    pub notes: Option<String>,
    /// The part of the service it belongs to (assumed: "worship_spot", as TimelyChurch's other
    /// tools name it; perhaps not sent).
    #[serde(alias = "spot")]
    pub worship_spot: Option<String>,
    /// A reading's passage (assumed: "reference", or "scripture_reference").
    #[serde(alias = "scripture_reference", alias = "passage")]
    pub reference: Option<String>,
    /// When the song last changed, if the list says (assumed: "song_updated_at"; perhaps not
    /// sent, and then songs are fetched by other signs: see `sync`).
    #[serde(alias = "song_last_changed", alias = "song_changed_at")]
    pub song_updated_at: Option<String>,
}

impl Item {
    /// True for an item that is a song (or names one).
    pub fn is_song(&self) -> bool {
        self.kind.eq_ignore_ascii_case("song") || (self.kind.is_empty() && self.song_id.is_some())
    }
}

/// A song in one arrangement: its sections once each, the order they are sung in, and the
/// church's published translations, lined up with the sections.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Song {
    pub id: Id,
    #[serde(alias = "name", deserialize_with = "or_empty")]
    pub title: String,
    /// assumed: "authors", a list (one line separated by commas is taken too).
    #[serde(alias = "author", deserialize_with = "names")]
    pub authors: Vec<String>,
    /// assumed: "ccli_number", a number or text.
    #[serde(alias = "ccli", alias = "ccli_song_number")]
    pub ccli_number: Option<serde_json::Value>,
    /// The copyright line (assumed: "copyright").
    #[serde(alias = "copyright_info", alias = "copyright_line")]
    pub copyright: Option<String>,
    pub key: Option<String>,
    /// The arrangement these sections and this order are (assumed: an object "arrangement"
    /// with "id", "name" and "is_default").
    pub arrangement: Option<Arrangement>,
    #[serde(deserialize_with = "or_empty")]
    pub sections: Vec<Section>,
    /// The sections' names in the order sung; a repeat is the name again (assumed: "order",
    /// strings; objects with a "name" are taken too).
    #[serde(alias = "sequence", alias = "section_order", alias = "order_sung", deserialize_with = "names")]
    pub order: Vec<String>,
    /// assumed: "translations".
    #[serde(deserialize_with = "or_empty")]
    pub translations: Vec<Translation>,
    #[serde(alias = "last_changed", alias = "changed_at", alias = "last_changed_at")]
    pub updated_at: Option<String>,
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
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Arrangement {
    pub id: Id,
    #[serde(deserialize_with = "or_empty")]
    pub name: String,
    pub key: Option<String>,
    /// assumed: "is_default"; when it isn't sent, the arrangement is taken for the default.
    #[serde(alias = "default")]
    pub is_default: Option<bool>,
}

/// One section, once: its name ("Verse 1", "Chorus") and its lines. A blank line ends a part
/// of it, which is a slide of its own.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Section {
    #[serde(alias = "label", deserialize_with = "or_empty")]
    pub name: String,
    #[serde(deserialize_with = "lines")]
    pub lines: Vec<Option<String>>,
}

/// A published translation: its language and its sections, each lined up with the song's own
/// section of the same place, line for line; an untranslated line is `null`.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Translation {
    /// A code such as `ru`, `uk`, `es` (assumed: "language"; "language_code" or "lang" taken too).
    #[serde(alias = "language_code", alias = "lang", deserialize_with = "or_empty")]
    pub language: String,
    pub title: Option<String>,
    #[serde(deserialize_with = "or_empty")]
    pub sections: Vec<TranslationSection>,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct TranslationSection {
    pub name: Option<String>,
    #[serde(deserialize_with = "lines")]
    pub lines: Vec<Option<String>>,
}

/// A song shown in a service, sent once with its own `Idempotency-Key`. Kept on disk until
/// TimelyChurch has it, so it is read back as well as written.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SongUse {
    /// assumed: "song_id".
    pub song_id: Id,
    /// assumed: "arrangement_id", `null` for the song's default.
    pub arrangement_id: Option<Id>,
    /// When it was shown, RFC 3339 with the computer's offset (assumed: "used_at").
    pub used_at: String,
    /// The service it was shown in (assumed: "service_id" and "service_type", as the services
    /// list gives them).
    pub service_id: Option<Id>,
    pub service_type: Option<String>,
    /// The key it was sung in, as the service gave it (assumed: "key").
    pub key: Option<String>,
}

/// What a refusal says, when it says anything (assumed: "error" and "message").
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Refusal {
    pub error: Option<serde_json::Value>,
    pub message: Option<String>,
}

impl Refusal {
    /// The refusal's own words, short and on one line, if it has any.
    pub fn words(body: &str) -> Option<String> {
        let refusal: Refusal = serde_json::from_str(body).ok()?;
        let error = match &refusal.error {
            Some(serde_json::Value::String(text)) => Some(text.clone()),
            Some(serde_json::Value::Object(fields)) => {
                fields.get("message").and_then(serde_json::Value::as_str).map(str::to_owned)
            }
            _ => None,
        };
        let words = refusal.message.or(error)?;
        let words: String = words.chars().filter(|c| !c.is_control()).take(200).collect();
        let words = words.trim();
        (!words.is_empty()).then(|| words.to_owned())
    }
}
