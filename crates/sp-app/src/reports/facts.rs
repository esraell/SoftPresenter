//! What the operator's window tells the reporter about the show and the screens, as plain data:
//! kinds, counts and sizes. It has no place for a name: not a display's, a screen's, a file's, a
//! song's or a person's. The window builds it (see `app_facts`); the reporter's thread reads it.

/// The graphics the app chose, as the graphics library names them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GpuFacts {
    pub name: String,
    pub backend: String,
    pub driver: String,
    pub device_type: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DisplayFact {
    pub w: u32,
    pub h: u32,
    pub scale: f64,
    pub hz: Option<f32>,
    pub main: bool,
}

/// How many screens are set up for the audience and for the stage, how many are on, and how
/// many send to a display, a window, NDI and a DeckLink card.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OutputCounts {
    pub audience: u32,
    pub stage: u32,
    pub on: u32,
    pub display: u32,
    pub window: u32,
    pub ndi: u32,
    pub decklink: u32,
}

/// Everything the window knows that a report says.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Facts {
    /// The open view, as a code: `service`, `bible`, `edit`, `themes`.
    pub view: &'static str,
    pub displays: Vec<DisplayFact>,
    pub outputs: OutputCounts,
    pub ndi: bool,
    pub decklink: bool,
    /// The app's language: `en`, `ru`, `uk`, `es`.
    pub language: &'static str,
    /// What kinds of thing are on screen, as codes: `song`, `bible`, `video`, `prop`…
    pub scene: Vec<&'static str>,
    /// A video on screen: its size and rate once the reporter has found them
    /// (`width, height, pictures a second`).
    pub video_size: Option<(u32, u32, f64)>,
    /// The video's file, so the reporter can ask its size; it never leaves the computer.
    pub video_file: Option<std::path::PathBuf>,
}

/// `1080p30` for a size and rate: the shorter side, and the rate to the nearest whole number
/// (a code can't hold the dot of 29.97).
pub fn size_code(width: u32, height: u32, fps: f64) -> String {
    let lines = width.min(height).clamp(1, 99_999);
    let rate = if fps.is_finite() { fps.round().clamp(0.0, 999.0) as u32 } else { 0 };
    format!("{lines}p{rate}")
}

impl Facts {
    /// What is on screen as codes, a video with its size where that is known:
    /// `video_1080p30`, `player_ffmpeg`, `song`.
    pub fn scene_codes(&self) -> Vec<String> {
        self.scene
            .iter()
            .map(|code| match (*code, self.video_size) {
                ("video", Some((w, h, fps))) => format!("video_{}", size_code(w, h, fps)),
                (code, _) => code.to_owned(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scene_is_said_in_codes_and_a_video_by_its_size() {
        let mut facts = Facts { scene: vec!["video", "player_ffmpeg", "song"], ..Facts::default() };
        assert_eq!(facts.scene_codes(), ["video", "player_ffmpeg", "song"]);
        facts.video_size = Some((1920, 1080, 29.97));
        assert_eq!(facts.scene_codes(), ["video_1080p30", "player_ffmpeg", "song"]);
        facts.video_size = Some((720, 1280, 59.94));
        assert_eq!(facts.scene_codes()[0], "video_720p60");
        assert!(facts.scene_codes().iter().all(|code| crate::reports::check::is_code(code)));
    }
}
