//! The sample backgrounds SoftPresenter comes with (`assets/backgrounds`): five stills, a
//! stand-in for a camera shot, and a looping video. They are never added to anyone's media bin
//! by themselves (see `samples`); this finds the folder that holds them, so the files can be
//! opened by path.

use std::path::{Path, PathBuf};

/// The files, by the names the folder keeps them under.
pub const FILES: [&str; 7] = [
    "Still.jpg",
    "First Light.jpg",
    "Night Blue.jpg",
    "Evening Hills.jpg",
    "Olive.jpg",
    "Room Out of Focus.jpg",
    "Drifting Light.mp4",
];

/// The folder the backgrounds are in: the Mac app's `Contents/Resources/Backgrounds`, a
/// `Backgrounds` folder beside the program (where other systems' packages can put it), and, in
/// a development build or a test run, the repository's own `assets/backgrounds`.
pub fn folder() -> Option<PathBuf> {
    let mut places: Vec<PathBuf> = Vec::new();
    if let Some(dir) = std::env::current_exe().ok().and_then(|exe| exe.parent().map(Path::to_path_buf)) {
        places.push(dir.join("../Resources/Backgrounds"));
        places.push(dir.join("Backgrounds"));
    }
    // Left out of a release build altogether, so the build's own folder never goes into it.
    #[cfg(debug_assertions)]
    {
        // Testing aid: SOFTPRESENTER_TEST_ROOT=<a copy of the repository> finds the files there,
        // for test programs built on one computer and run on another.
        let root = std::env::var_os("SOFTPRESENTER_TEST_ROOT")
            .map_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."), PathBuf::from);
        places.push(root.join("assets/backgrounds"));
    }
    places.into_iter().find(|place| FILES.iter().all(|name| place.join(name).is_file()))
}

/// The path of one background, by its file name from [`FILES`].
pub fn find(name: &str) -> Option<PathBuf> {
    FILES.contains(&name).then(|| folder().map(|dir| dir.join(name))).flatten()
}

/// Whether the files are there to be added. Looked for once: the app's files don't move while it
/// runs, and the media strip asks every time it is drawn.
pub fn available() -> bool {
    static FOUND: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *FOUND.get_or_init(|| folder().is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_sample_background_is_found_by_path() {
        let folder = folder().expect("the folder of sample backgrounds");
        for name in FILES {
            let path = find(name).unwrap_or_else(|| panic!("{name} has no path"));
            assert_eq!(path, folder.join(name));
            assert!(path.is_file(), "{name} is missing from {}", folder.display());
            assert!(std::fs::metadata(&path).is_ok_and(|meta| meta.len() > 10_000), "{name} is empty");
        }
        assert!(find("Not There.jpg").is_none(), "a name that is not one of them has no path");
    }
}
