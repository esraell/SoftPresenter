//! The state remote pages are sent: named parts (what is on screen, the playlists, the timers…),
//! each a piece of JSON, with a version number that goes up by one whenever any part changes.
//!
//! A page is sent the whole state when it connects (a snapshot), then each change as the parts
//! that changed. Each part remembers the version it last changed at, so a page that reconnects
//! saying the version it had is sent exactly the parts that changed since, and nothing else.
//! The versions count from 1 each time Remote Control is switched on, under a new `epoch` (a
//! random word): a page from before, or from another start, is sent a snapshot instead.

use std::collections::BTreeMap;
use std::sync::Arc;

/// One part of the state: the version it last changed at, and its JSON, written once and sent
/// to every page as it is.
#[derive(Clone, Debug)]
struct Part {
    changed: u64,
    json: Arc<str>,
}

#[derive(Debug)]
pub struct Hub {
    epoch: String,
    version: u64,
    parts: BTreeMap<String, Part>,
}

impl Hub {
    pub fn new(epoch: String) -> Hub {
        Hub { epoch, version: 0, parts: BTreeMap::new() }
    }

    pub fn epoch(&self) -> &str {
        &self.epoch
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    /// Takes the parts as they are now. Those whose JSON differs from what was kept are kept
    /// in its place, all under one new version; true when anything changed.
    pub fn publish<'a>(&mut self, parts: impl IntoIterator<Item = (&'a str, String)>) -> bool {
        let mut changed = Vec::new();
        for (name, json) in parts {
            if self.parts.get(name).is_none_or(|part| *part.json != *json) {
                changed.push((name.to_owned(), json));
            }
        }
        if changed.is_empty() {
            return false;
        }
        self.version += 1;
        for (name, json) in changed {
            self.parts.insert(name, Part { changed: self.version, json: Arc::from(json) });
        }
        true
    }

    /// The parts that changed after `version` (every part, from 0), by name. `None` when the
    /// version is from another epoch or ahead of this one, and a snapshot is wanted instead.
    pub fn since(&self, epoch: &str, version: u64) -> Option<Vec<(String, Arc<str>)>> {
        if epoch != self.epoch || version > self.version {
            return None;
        }
        Some(
            self.parts
                .iter()
                .filter(|(_, part)| part.changed > version)
                .map(|(name, part)| (name.clone(), part.json.clone()))
                .collect(),
        )
    }

    /// Every part, by name.
    pub fn all(&self) -> Vec<(String, Arc<str>)> {
        self.parts.iter().map(|(name, part)| (name.clone(), part.json.clone())).collect()
    }
}

/// The JSON object `{"name": part, …}` of some parts, written without parsing them again.
pub fn parts_object(parts: &[(String, Arc<str>)]) -> String {
    let mut out = String::from("{");
    for (at, (name, json)) in parts.iter().enumerate() {
        if at > 0 {
            out.push(',');
        }
        out.push_str(&serde_json::to_string(name).unwrap_or_default());
        out.push(':');
        out.push_str(json);
    }
    out.push('}');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn publish(hub: &mut Hub, parts: &[(&str, &str)]) -> bool {
        hub.publish(parts.iter().map(|(name, json)| (*name, (*json).to_owned())))
    }

    #[test]
    fn a_version_goes_up_once_for_each_change_and_never_for_none() {
        let mut hub = Hub::new("e1".to_owned());
        assert!(publish(&mut hub, &[("live", "1"), ("timers", "[]")]));
        assert_eq!(hub.version(), 1);
        assert!(!publish(&mut hub, &[("live", "1"), ("timers", "[]")]), "nothing changed");
        assert_eq!(hub.version(), 1);
        assert!(publish(&mut hub, &[("live", "2"), ("timers", "[]")]));
        assert_eq!(hub.version(), 2);
    }

    #[test]
    fn a_page_that_comes_back_is_sent_exactly_what_changed_since() {
        let mut hub = Hub::new("e1".to_owned());
        publish(&mut hub, &[("live", "1"), ("timers", "[]"), ("playlists", "{}")]);
        let had = hub.version();
        publish(&mut hub, &[("live", "2"), ("timers", "[]"), ("playlists", "{}")]);
        publish(&mut hub, &[("live", "2"), ("timers", "[1]"), ("playlists", "{}")]);
        let since = hub.since("e1", had).expect("same epoch");
        let names: Vec<&str> = since.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["live", "timers"]);
        assert_eq!(&*since[0].1, "2");
        assert!(hub.since("e1", hub.version()).expect("same epoch").is_empty(), "up to date");
        assert_eq!(hub.since("e1", 0).expect("same epoch").len(), 3, "from 0, everything");
        // Another start, or a version from the future: a snapshot instead.
        assert!(hub.since("e0", had).is_none());
        assert!(hub.since("e1", hub.version() + 1).is_none());
    }

    #[test]
    fn parts_are_written_as_one_object() {
        let parts = vec![("live".to_owned(), Arc::from("{\"a\":1}")), ("timers".to_owned(), Arc::from("[]"))];
        let object = parts_object(&parts);
        assert_eq!(object, "{\"live\":{\"a\":1},\"timers\":[]}");
        assert!(serde_json::from_str::<serde_json::Value>(&object).is_ok());
        assert_eq!(parts_object(&[]), "{}");
    }
}
