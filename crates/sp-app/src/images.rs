//! Pictures placed on slides. A chosen picture is copied into the app's Images folder, so a
//! song keeps its pictures when the original file is moved or deleted.

use std::path::{Path, PathBuf};

/// The Images folder in the app's data folder.
pub(crate) fn folder() -> Option<PathBuf> {
    crate::bible_store::data_dir().map(|dir| dir.join("Images"))
}

/// Where a picture object's file is: a whole path as it is; `Media/<name>` in the library's
/// Media folder (a video brought in by a song pack); otherwise a name in the Images folder. A
/// name that would climb out of those folders (`..`) is nothing.
pub fn resolve(file: &str) -> Option<PathBuf> {
    let path = Path::new(file);
    if file.is_empty() {
        return None;
    }
    if path.is_absolute() {
        return Some(path.to_path_buf());
    }
    if !path.components().all(|part| matches!(part, std::path::Component::Normal(_))) {
        return None;
    }
    match file.strip_prefix(crate::pack::MEDIA_PREFIX) {
        Some(name) => crate::bible_store::data_dir().map(|dir| dir.join("Media").join(name)),
        None => folder().map(|dir| dir.join(file)),
    }
}

/// Reads the picture a picture object names, for the renderer.
pub fn load(file: &str) -> Option<sp_render::Picture> {
    resolve(file).and_then(|path| crate::media::load_picture(&path).ok())
}

/// A picture copied into the Images folder: its name there, a name for the object, and its size.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Imported {
    pub file: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
}

/// Copies a picture into the Images folder under a name of its own. The error is a sentence for
/// the operator.
pub fn import(path: &Path) -> Result<Imported, String> {
    // A video stays where it is (they can be large) and is named by its whole path; its still
    // gives its shape.
    if crate::media::is_video(path) {
        let still = crate::media::load_picture(path)?;
        let name = path.file_stem().map_or_else(|| "Video".to_owned(), |stem| stem.to_string_lossy().into_owned());
        let file = path.to_string_lossy().into_owned();
        return Ok(Imported { file, name, width: still.width(), height: still.height() });
    }
    let (width, height) = measure(path)?;
    let dir = folder().ok_or_else(|| "Could not find where to keep pictures on this computer.".to_owned())?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("Could not make the Images folder: {e}"))?;
    let ending = path.extension().map_or_else(String::new, |e| format!(".{}", e.to_string_lossy().to_lowercase()));
    let file = format!("{}{ending}", sp_model::Uuid::new_v4());
    copy_into(path, &dir, &file)?;
    let name = path.file_stem().map_or_else(|| "Picture".to_owned(), |stem| stem.to_string_lossy().into_owned());
    Ok(Imported { file, name, width, height })
}

/// How big a picture is, in pixels. A picture in a format the system reads (HEIC, TIFF...) is
/// measured by decoding it.
pub fn measure(path: &Path) -> Result<(u32, u32), String> {
    let shown = path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned());
    image::image_dimensions(path).or_else(|e| {
        crate::media::load_picture(path)
            .map(|picture| (picture.width(), picture.height()))
            .map_err(|_| format!("{shown}: not a picture SoftPresenter can open ({e})."))
    })
}

/// Copies a picture into a folder under a name.
pub fn copy_into(path: &Path, folder: &Path, file: &str) -> Result<(), String> {
    let shown = path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned());
    // Copied under another name and then renamed, so a quit part way never leaves a picture that
    // is there but cut short.
    let writing = folder.join(format!("{file}.tmp"));
    std::fs::copy(path, &writing)
        .and_then(|_| std::fs::rename(&writing, folder.join(file)))
        .map_err(|e| format!("Could not copy {shown}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_whole_path_is_kept_and_a_name_is_looked_for_in_the_images_folder() {
        // A whole path as each system writes one.
        let whole = if cfg!(windows) { r"C:\pictures\logo.png" } else { "/pictures/logo.png" };
        assert_eq!(resolve(whole), Some(PathBuf::from(whole)));
        if let Some(found) = resolve("abc.png") {
            assert!(found.ends_with("Images/abc.png"), "{found:?}");
        }
        assert!(import(Path::new("/nowhere/none.png")).unwrap_err().starts_with("none.png:"));
    }
}
