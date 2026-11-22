//! Actions: things a macro does when it is run, or a slide does when it goes on screen.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// What a Clear action takes off the screens.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClearWhat {
    All,
    Slide,
    Media,
    Props,
    Messages,
}

impl ClearWhat {
    pub const ALL: [ClearWhat; 5] =
        [ClearWhat::All, ClearWhat::Slide, ClearWhat::Media, ClearWhat::Props, ClearWhat::Messages];

    pub fn name(self) -> &'static str {
        match self {
            ClearWhat::All => "Clear All",
            ClearWhat::Slide => "Clear Slide",
            ClearWhat::Media => "Clear Media",
            ClearWhat::Props => "Clear Props",
            ClearWhat::Messages => "Clear Messages",
        }
    }
}

/// What a Timer action does to its timer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimerDo {
    Start,
    Stop,
    Reset,
}

impl TimerDo {
    pub const ALL: [TimerDo; 3] = [TimerDo::Start, TimerDo::Stop, TimerDo::Reset];

    pub fn name(self) -> &'static str {
        match self {
            TimerDo::Start => "Start",
            TimerDo::Stop => "Stop",
            TimerDo::Reset => "Reset",
        }
    }
}

/// One thing a macro or a slide does. Messages, props and timers are named by their ids, a song
/// by its library key.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "do", rename_all = "snake_case")]
pub enum ShowAction {
    Clear {
        what: ClearWhat,
    },
    Message {
        id: Uuid,
        show: bool,
    },
    Prop {
        id: Uuid,
        show: bool,
    },
    Timer {
        id: Uuid,
        act: TimerDo,
    },
    Audience {
        on: bool,
    },
    Song {
        key: String,
    },
    /// Puts a picture from the media bin behind the words, by its id.
    Media {
        id: Uuid,
    },
    /// Plays something on the Announcements layer, going round until it is cleared.
    Announce {
        what: Announced,
    },
    /// Fetches a fresh announcements video from Canva (Settings › Integrations › Canva Sync).
    CanvaSync,
}

/// What an Announce action plays on the Announcements layer: a song (by its library key), a
/// picture or video from the media bin, or a media playlist (by their ids).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Announced {
    Song { key: String },
    Media { id: Uuid },
    MediaPlaylist { id: Uuid },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_announce_step_keeps_what_it_plays_in_its_file() {
        let id = Uuid::from_u128(7);
        for what in [Announced::Song { key: "12".to_owned() }, Announced::Media { id }, Announced::MediaPlaylist { id }]
        {
            let step = ShowAction::Announce { what };
            let text = serde_json::to_string(&step).unwrap();
            assert!(text.starts_with(r#"{"do":"announce","what":{"kind":"#), "{text}");
            assert_eq!(serde_json::from_str::<ShowAction>(&text).unwrap(), step);
        }
        assert_eq!(serde_json::to_string(&ShowAction::CanvaSync).unwrap(), r#"{"do":"canva_sync"}"#);
    }
}
