//! Data only: presentations, slides, text boxes, groups and themes, plus the plain-text song
//! parser. No drawing, no windows.

mod action;
mod build;
mod format;
mod import;
mod link;
mod pro_file;
mod rtf;
mod softprojector;
mod song_text;
mod theme;
mod transition;
mod types;

pub use action::{ClearWhat, ShowAction, TimerDo};
pub use build::{Build, BuildClock, BuildEffect};
pub use format::{FORMAT_VERSION, from_json, to_json};
pub use import::{decode_text, import_song_text};
pub use link::TextLink;
#[cfg(any(test, feature = "test-documents"))]
#[doc(hidden)]
pub use pro_file::example as pro_example;
pub use pro_file::{LARGEST_PRO_FILE, ProDocument, ProMedia, read_pro_document, read_pro_file};
pub use rtf::rtf_to_text;
pub use softprojector::from_softprojector;
pub use song_text::{
    GROUP_KEYS, clean_box_text, group_color, groups_for_key, key_for_group, parse_song_text, to_song_text,
};
pub use theme::{
    Align, BIBLE_TEMPLATE, BIBLE_THEME_ID, BoxStyle, Caps, Gradient, GradientKind, LINE_HEIGHT, LineFill,
    LineTransform, ListStyle, Scaling, Scroll, Shadow, SlideRect, TextStyle, Theme, ThemeTemplate, VAlign, fonts_used,
    missing_fonts,
};
pub use transition::{Direction, Transition, TransitionKind};
pub use types::{
    Arrangement, Copyright, Group, ImageFit, MOST_TIMES, MediaItem, Object, ObjectKind, Pass, Presentation, Rgba, Role,
    Run, Shape, Slide, StageShows, StrokeLook, Sung,
};
pub use uuid::Uuid;

/// Slides are authored at this size and scaled to the screen.
pub const SLIDE_WIDTH: f32 = 1920.0;
/// See [`SLIDE_WIDTH`].
pub const SLIDE_HEIGHT: f32 = 1080.0;
