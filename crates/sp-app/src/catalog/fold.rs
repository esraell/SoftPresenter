//! Folding text for searching: what is typed and what is searched are made alike, so a search
//! ignores case, tells ё from е as nobody does, and is not thrown by punctuation, the stress
//! marks some Russian and Ukrainian texts carry, or the several apostrophes.

/// The text in lower case with ё written е, every run of anything but letters and digits made
/// one space, combining marks (stress marks) dropped, and no space at either end.
pub fn fold(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut gap = true;
    for c in text.chars() {
        // Combining marks (U+0300 to U+036F) sit on a letter: they are not words.
        if ('\u{300}'..='\u{36f}').contains(&c) {
            continue;
        }
        if c.is_alphanumeric() {
            for lower in c.to_lowercase() {
                out.push(if lower == 'ё' { 'е' } else { lower });
            }
            gap = false;
        } else if !gap {
            out.push(' ');
            gap = true;
        }
    }
    if out.ends_with(' ') {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn case_yo_stress_marks_and_punctuation_fold_away() {
        assert_eq!(fold("  Свет НАД рекой!  "), "свет над рекой");
        assert_eq!(fold("Всё, что ЕЩЁ"), fold("все что еще"));
        assert_eq!(fold("Благода\u{301}ть"), "благодать");
        assert_eq!(fold("Ім'я"), fold("Ім\u{2019}я"));
        assert_eq!(fold("Amazing Grace"), "amazing grace");
        assert_eq!(fold("..."), "");
    }
}
