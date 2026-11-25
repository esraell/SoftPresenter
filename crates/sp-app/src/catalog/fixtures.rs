//! Song packs written for tests: the catalog's own, and the window's (`catalog_songs`). The
//! words are made up for the tests.

use std::io::Write;

/// A song file written for a test: its title, its sections (name and words) and its versions
/// in other languages (a language code and its sections).
pub fn xml(title: &str, sections: &[(&str, &str)], versions: &[(&str, &[(&str, &str)])]) -> String {
    let part = |sections: &[(&str, &str)]| {
        sections
            .iter()
            .map(|(name, words)| {
                format!("<section title=\"{name}\" capitalise=\"true\"><lyrics>{words}\n</lyrics></section>")
            })
            .collect::<String>()
    };
    let more: String = versions
        .iter()
        .map(|(lang, sections)| {
            format!("<translationoptions><lang>{lang}</lang><lyrics>{}</lyrics></translationoptions>", part(sections))
        })
        .collect();
    format!("<song><title>{title}</title><author>Автор</author><lyrics>{}</lyrics>{more}</song>", part(sections))
}

/// A song pack (a zip) of these files, by name and contents.
pub fn pack(files: &[(String, String)]) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut out));
        let options = zip::write::SimpleFileOptions::default();
        for (name, body) in files {
            zip.start_file(format!("uk/{name}"), options).expect("start");
            zip.write_all(body.as_bytes()).expect("write");
        }
        zip.finish().expect("finish");
    }
    out
}
