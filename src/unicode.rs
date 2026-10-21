// SPDX-FileCopyrightText: The crank contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Character classes crank needs beyond what `char` itself offers, shared
//! by selector parsing and the Rhai filter sandbox so both agree on what
//! counts as part of a word.

use std::sync::LazyLock;

use regex::Regex;

/// Whether `c` is a combining mark (Unicode general category `M`), such as
/// the U+0301 COMBINING ACUTE ACCENT that follows the `e` in a decomposed
/// "é". A mark belongs to the character before it, so it's part of a word
/// even though it's neither a letter nor a digit.
#[must_use]
pub fn is_mark(c: char) -> bool {
    static MARK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\p{M}$").expect("a valid regex"));
    MARK.is_match(c.encode_utf8(&mut [0; 4]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combining_marks_are_marks() {
        assert!(is_mark('\u{301}')); // COMBINING ACUTE ACCENT
        assert!(is_mark('\u{3099}')); // COMBINING KATAKANA-HIRAGANA VOICED SOUND MARK
        assert!(is_mark('\u{fe0f}')); // VARIATION SELECTOR-16
    }

    #[test]
    fn letters_digits_punctuation_and_emoji_are_not_marks() {
        for c in [
            'a', 'é', 'ż', 'ж', '日', '한', '7', '_', '-', ' ', '🍕', '\u{200d}',
        ] {
            assert!(!is_mark(c), "{c:?}");
        }
    }
}
