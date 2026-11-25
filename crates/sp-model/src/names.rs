//! Names that come from outside: a file's name, a zip entry's, or the name a document carries
//! inside it. Such a name can arrive garbled: UTF-8 read by a program as an older encoding and
//! written down that way, so "Хвала" reads "╨Ñ╨▓╨░╨╗╨░" and "life’s" reads "lifeΓÇÖs". Zip
//! tools did this to names for years: a zip that doesn't say its names are UTF-8 is read as the
//! IBM PC's code page 437, as the zip format says, while many tools wrote UTF-8 without saying.
//!
//! [`zip_entry_name`] reads a zip entry's name from its bytes the way the format says (UTF-8
//! when the bytes are UTF-8, else code page 437, or code page 866 for a name that is plainly
//! Russian in it), and [`repair_garbled`] puts a garbled name back, leaving alone every name
//! that isn't one: a name is only changed when reading its letters back as the bytes they came
//! from gives UTF-8 that holds letters.

/// The characters of code page 437 for the bytes 0x80 to 0xFF.
const CP437_HIGH: [char; 128] = [
    'Ç', 'ü', 'é', 'â', 'ä', 'à', 'å', 'ç', 'ê', 'ë', 'è', 'ï', 'î', 'ì', 'Ä', 'Å', //
    'É', 'æ', 'Æ', 'ô', 'ö', 'ò', 'û', 'ù', 'ÿ', 'Ö', 'Ü', '¢', '£', '¥', '₧', 'ƒ', //
    'á', 'í', 'ó', 'ú', 'ñ', 'Ñ', 'ª', 'º', '¿', '⌐', '¬', '½', '¼', '¡', '«', '»', //
    '░', '▒', '▓', '│', '┤', '╡', '╢', '╖', '╕', '╣', '║', '╗', '╝', '╜', '╛', '┐', //
    '└', '┴', '┬', '├', '─', '┼', '╞', '╟', '╚', '╔', '╩', '╦', '╠', '═', '╬', '╧', //
    '╨', '╤', '╥', '╙', '╘', '╒', '╓', '╫', '╪', '┘', '┌', '█', '▄', '▌', '▐', '▀', //
    'α', 'ß', 'Γ', 'π', 'Σ', 'σ', 'µ', 'τ', 'Φ', 'Θ', 'Ω', 'δ', '∞', 'φ', 'ε', '∩', //
    '≡', '±', '≥', '≤', '⌠', '⌡', '÷', '≈', '°', '∙', '·', '√', 'ⁿ', '²', '■', '\u{a0}',
];

/// One byte of code page 866 (the DOS code page for Russian) as the character it stands for.
fn cp866(byte: u8) -> char {
    const LAST: [char; 16] = ['Ё', 'ё', 'Є', 'є', 'Ї', 'ї', 'Ў', 'ў', '°', '∙', '·', '√', '№', '¤', '■', '\u{a0}'];
    match byte {
        0x00..=0x7F => char::from(byte),
        // А to Я, then а to п.
        0x80..=0xAF => char::from_u32(0x0410 + u32::from(byte - 0x80)).unwrap_or('\u{fffd}'),
        // The same box drawing as code page 437.
        0xB0..=0xDF => CP437_HIGH[usize::from(byte - 0x80)],
        // р to я.
        0xE0..=0xEF => char::from_u32(0x0440 + u32::from(byte - 0xE0)).unwrap_or('\u{fffd}'),
        _ => LAST[usize::from(byte - 0xF0)],
    }
}

/// One byte of code page 437 as the character it stands for.
pub(crate) fn cp437(byte: u8) -> char {
    if byte < 0x80 { char::from(byte) } else { CP437_HIGH[usize::from(byte - 0x80)] }
}

/// A zip entry's name from its bytes: UTF-8 when they are UTF-8 (whether or not the zip says
/// so, as many tools never did), else code page 437 as the zip format says, or code page 866
/// when the name is plainly Russian in it (nearly all its letters bytes that are Cyrillic
/// letters there), which is what zip tools on Russian Windows wrote.
pub fn zip_entry_name(raw: &[u8]) -> String {
    if let Ok(name) = std::str::from_utf8(raw) {
        return name.to_owned();
    }
    let cyrillic = raw.iter().filter(|&&b| matches!(b, 0x80..=0xAF | 0xE0..=0xF7)).count();
    let plain = raw.iter().filter(|b| b.is_ascii_alphabetic()).count();
    if cyrillic > plain { raw.iter().map(|&b| cp866(b)).collect() } else { raw.iter().map(|&b| cp437(b)).collect() }
}

/// The byte a character of code page 437 stands for, if it is one.
fn cp437_byte(c: char) -> Option<u8> {
    if c.is_ascii() {
        return u8::try_from(c).ok();
    }
    CP437_HIGH.iter().position(|&have| have == c).and_then(|at| u8::try_from(0x80 + at).ok())
}

/// The byte a character of Windows-1252 stands for, if it is one. The five bytes Windows-1252
/// leaves unused come back as themselves, as some programs read them.
fn cp1252_byte(c: char) -> Option<u8> {
    if c.is_ascii() {
        return u8::try_from(c).ok();
    }
    if matches!(c, '\u{81}' | '\u{8d}' | '\u{8f}' | '\u{90}' | '\u{9d}') {
        return u8::try_from(u32::from(c)).ok();
    }
    (0x80..=0xFF_u8).find(|&byte| crate::import::windows_1252(byte) == c)
}

/// A garbled name put back (see the module's notes), or `None` when it isn't garbled. A name
/// garbled twice over is put back twice. Never changes text that is already right: Russian,
/// Ukrainian, Spanish or English words read back as bytes are not UTF-8.
pub fn repair_garbled(name: &str) -> Option<String> {
    let mut text = name.to_owned();
    let mut changed = false;
    for _ in 0..3 {
        match repair_once(&text) {
            Some(fixed) => {
                text = fixed;
                changed = true;
            }
            None => break,
        }
    }
    changed.then_some(text)
}

fn repair_once(text: &str) -> Option<String> {
    if text.is_ascii() {
        return None;
    }
    for back in [cp437_byte as fn(char) -> Option<u8>, cp1252_byte] {
        let Some(bytes) = text.chars().map(back).collect::<Option<Vec<u8>>>() else { continue };
        let Ok(fixed) = String::from_utf8(bytes) else { continue };
        let sound = fixed.chars().all(|c| !c.is_control() && c != '\u{fffd}');
        if fixed != text && sound && fixed.chars().any(char::is_alphabetic) {
            return Some(fixed);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A name garbled the way a zip tool garbles it: its UTF-8 bytes read as code page 437.
    fn garbled(name: &str) -> String {
        name.bytes().map(cp437).collect()
    }

    #[test]
    fn names_garbled_through_code_page_437_or_windows_1252_are_put_back() {
        // Made up for this test.
        for name in ["Свет над рекой", "Світло над річкою", "Canción del río", "The river’s song", "Ёлка и йод"]
        {
            assert_eq!(repair_garbled(&garbled(name)).as_deref(), Some(name), "{}", garbled(name));
            // Read as Windows-1252, its five unused bytes as themselves (as browsers read them).
            let windows: String = name
                .bytes()
                .map(|b| match crate::import::windows_1252(b) {
                    '\u{fffd}' => char::from(b),
                    c => c,
                })
                .collect();
            assert_eq!(repair_garbled(&windows).as_deref(), Some(name), "{windows}");
        }
        assert_eq!(garbled("Свет"), "╨í╨▓╨╡╤é");
        assert_eq!(repair_garbled("lifeΓÇÖs road").as_deref(), Some("life’s road"));
        // Garbled twice over.
        assert_eq!(repair_garbled(&garbled(&garbled("Свет над рекой"))).as_deref(), Some("Свет над рекой"));
    }

    #[test]
    fn names_that_are_already_right_are_left_alone() {
        for name in [
            "Свет над рекой",
            "Ёлка, йод и щётка",
            "Світло над річкою: їжак, ґанок, є",
            "Canción del río, señor, ¿qué?",
            "Größe, café, naïve, déjà vu",
            "The river’s song — “again”",
            "Plain words",
            "Ω and π",
            "",
        ] {
            assert_eq!(repair_garbled(name), None, "{name}");
        }
    }

    #[test]
    fn zip_entry_names_are_utf8_when_they_are_else_code_page_437_or_866() {
        assert_eq!(zip_entry_name("Свет.pro".as_bytes()), "Свет.pro");
        // "Café.pro" written in code page 437.
        assert_eq!(zip_entry_name(b"Caf\x82.pro"), "Café.pro");
        // "Свет над рекой.pro" written in code page 866.
        assert_eq!(zip_entry_name(b"\x91\xa2\xa5\xe2 \xad\xa0\xa4 \xe0\xa5\xaa\xae\xa9.pro"), "Свет над рекой.pro");
    }
}
