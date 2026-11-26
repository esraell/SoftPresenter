//! Makes a SoftPresenter Bible file from a Bible in another program's format, read exactly as
//! the app reads a file dropped on its window (Zefania, OSIS, Beblia, `<b>`/`<c>`/`<v>` XML,
//! USFM, USX or SoftProjector's `.spb`; a file, a zip or a folder), with what the app's own list
//! of Bibles needs set by hand:
//!
//! cargo run -p sp-bible --example import -- IN OUT.spbible id=npu "name=..." short=НПУ \
//!     language=uk "terms=CC BY-SA 4.0" "credit=..." "source=eBible.org ukronpu" [numbering=synodal] [fit=loose]
//!
//! Anything left out is taken from the file. It says what came in.

use sp_bible::Numbering;
use sp_bible::import::{About, Limits, read_path};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [input, output, settings @ ..] = args.as_slice() else {
        return eprintln!("usage: import IN OUT.spbible key=value ...");
    };
    let get = |key: &str| {
        settings.iter().find_map(|s| s.strip_prefix(key).and_then(|rest| rest.strip_prefix('='))).map(str::to_owned)
    };
    let imported = match read_path(std::path::Path::new(input), &Limits::NORMAL) {
        Ok(imported) => imported,
        Err(e) => return eprintln!("{input}: {e}"),
    };
    let given = About {
        name: get("name").unwrap_or_default(),
        short: get("short").unwrap_or_default(),
        language: get("language").unwrap_or_default(),
        terms: get("terms").unwrap_or_default(),
    };
    let Some(about) = imported.details(&given) else {
        return eprintln!(
            "{input} doesn't say its {}: give them as name=, short= and language=.",
            imported.missing_flags(&given)
        );
    };
    let id = get("id").unwrap_or_else(|| sp_bible::import::id_from(&about.short, &about.name));
    let mut bible = imported.bible(&id, &about);
    if let Some(credit) = get("credit") {
        bible.info.credit = credit;
    }
    if let Some(source) = get("source") {
        bible.info.source = source;
    }
    if let Some(numbering) = get("numbering") {
        let Some(numbering) = Numbering::from_name(&numbering) else {
            return eprintln!("numbering= must be english, synodal or original");
        };
        bible.info.numbering = numbering;
    }
    if let Some(fit) = get("fit") {
        bible.info.loose = fit == "loose";
    }
    let bytes = bible.to_bytes();
    match std::fs::write(output, &bytes) {
        Ok(()) => println!("{output}: {} ({} bytes)", imported.summary(&about.name), bytes.len()),
        Err(e) => eprintln!("Could not write {output}: {e}"),
    }
}
