//! Data only: presentations, slides, text boxes, groups and themes, plus the plain-text song
//! parser. No drawing, no windows.

mod action;
pub mod bounded_zip;
mod build;
mod format;
mod import;
mod link;
mod names;
mod pro_file;
mod rtf;
mod softprojector;
mod song_text;
mod spans;
pub mod sqlite_read;
mod text_map;
mod theme;
mod tidy;
mod transition;
mod types;

pub use action::{Announced, ClearWhat, ShowAction, TimerDo};
pub use build::{Build, BuildClock, BuildEffect};
pub use format::{FORMAT_VERSION, from_json, to_json};
pub use import::{
    MOST_LINES, decode_text, import_song_text, section_name, share_out, without_chords, year_and_publisher,
};
pub use link::TextLink;
pub use names::{repair_garbled, zip_entry_name};
#[cfg(any(test, feature = "test-documents"))]
#[doc(hidden)]
pub use pro_file::example as pro_example;
pub use pro_file::{LARGEST_PRO_FILE, ProDocument, ProMedia, read_pro_document, read_pro_file};
pub use rtf::rtf_to_text;
pub use softprojector::{SongbookProblem, SongbookSong, from_softprojector, read_songbook};
/// Small songbooks made for tests (`tests/songbooks`), for other crates' tests.
#[cfg(any(test, feature = "test-documents"))]
#[doc(hidden)]
pub mod songbook_examples {
    /// A whole database of several songbooks, 226 songs.
    pub const DATABASE: &[u8] = include_bytes!("../tests/songbooks/database.sqlite");
    /// One exported songbook, "Zpěvník pro zkoušku", 7 songs.
    pub const SONGBOOK: &[u8] = include_bytes!("../tests/songbooks/songbook.sps");
}
pub use song_text::{
    GROUP_KEYS, clean_box_text, group_color, group_name_in_english, groups_for_key, key_for_group, parse_song_text,
    to_song_text,
};
pub use spans::{
    Drawn, MOST_SPANS, SpanMix, SpanStyle, TextSpan, byte_ranges, clear_in_spans, edit_text, look_of_range, restyle,
    restyle_range, smallest_edit, style_at,
};
pub use theme::{
    Align, BIBLE_TEMPLATE, BIBLE_THEME_ID, BUILTIN_TEMPLATES, BUILTIN_THEME, BoxStyle, Caps, DEFAULT_FONT_FAMILY,
    Gradient, GradientKind, LINE_HEIGHT, LineFill, LineTransform, ListStyle, Scaling, Scroll, Shadow, SlideRect,
    TextStyle, Theme, ThemeTemplate, ThemeVersion, VAlign, Version, all_versions, fonts_used, missing_fonts,
};
pub use tidy::{
    CharFixes, LeftWhole, MOST_TIDY_LINES, Split, SplitWhy, Tidied, TidyOptions, TitleFix, WholeWhy, first_line,
    tidy_song, tidy_text, tidy_title, words_key,
};
pub use transition::{Direction, Transition, TransitionKind};
pub use types::{
    Arrangement, CatalogOrigin, Copyright, Group, ImageFit, MOST_TIMES, MediaItem, NdiQuality, Object, ObjectKind,
    Pass, Presentation, Rgba, Role, Run, Shape, Slide, StageShows, StrokeLook, Sung, ndi_computer, ndi_short_name,
};
pub use uuid::Uuid;

/// Slides are authored at this size and scaled to the screen.
pub const SLIDE_WIDTH: f32 = 1920.0;
/// See [`SLIDE_WIDTH`].
pub const SLIDE_HEIGHT: f32 = 1080.0;
