//! Other systems: pictures in formats beyond the common few aren't read there yet.

use std::path::Path;

use crate::Frame;

pub fn picture(_path: &Path, _max_side: u32) -> Result<Frame, String> {
    Err("This picture format can be opened on macOS only, for now.".to_owned())
}

pub fn pdf_pages(
    _path: &Path,
    _max_width: u32,
    _max_height: u32,
    _most_pages: usize,
    _each: impl FnMut(usize, usize, Frame) -> bool,
) -> Result<usize, String> {
    Err("PDF files can be turned into slides on macOS only, for now. Export the slides as pictures instead.".to_owned())
}

/// The account's home folder from the system's own records: on macOS only, for now.
pub fn home_folder() -> Option<std::path::PathBuf> {
    None
}

/// Display names come from the window system on other systems.
pub fn display_names() -> Vec<(u32, String)> {
    Vec::new()
}
