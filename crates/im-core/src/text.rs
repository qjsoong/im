//! Small text helpers shared by dictionary parsing and schema folding.

use std::fmt;

/// Characters that carry a tone mark, mapped to their toneless form.
///
/// Covers the pinyin diacritics (`āáǎà`, `ǖǘǚǜ`, …), the syllable consonants
/// `ń`/`ḿ`, and `ê`. `ü` is mapped to `v` because `v` is the ASCII spelling
/// used throughout the dictionary format and by every double-pinyin scheme.
const TONE_FOLD: &[(char, char)] = &[
    ('ā', 'a'),
    ('á', 'a'),
    ('ǎ', 'a'),
    ('à', 'a'),
    ('Ā', 'a'),
    ('Á', 'a'),
    ('Ǎ', 'a'),
    ('À', 'a'),
    ('ē', 'e'),
    ('é', 'e'),
    ('ě', 'e'),
    ('è', 'e'),
    ('ê', 'e'),
    ('Ē', 'e'),
    ('É', 'e'),
    ('Ě', 'e'),
    ('È', 'e'),
    ('Ê', 'e'),
    ('ī', 'i'),
    ('í', 'i'),
    ('ǐ', 'i'),
    ('ì', 'i'),
    ('Ī', 'i'),
    ('Í', 'i'),
    ('Ǐ', 'i'),
    ('Ì', 'i'),
    ('ō', 'o'),
    ('ó', 'o'),
    ('ǒ', 'o'),
    ('ò', 'o'),
    ('Ō', 'o'),
    ('Ó', 'o'),
    ('Ǒ', 'o'),
    ('Ò', 'o'),
    ('ū', 'u'),
    ('ú', 'u'),
    ('ǔ', 'u'),
    ('ù', 'u'),
    ('Ū', 'u'),
    ('Ú', 'u'),
    ('Ǔ', 'u'),
    ('Ù', 'u'),
    ('ǖ', 'v'),
    ('ǘ', 'v'),
    ('ǚ', 'v'),
    ('ǜ', 'v'),
    ('ü', 'v'),
    ('Ǖ', 'v'),
    ('Ǘ', 'v'),
    ('Ǚ', 'v'),
    ('Ǜ', 'v'),
    ('Ü', 'v'),
    ('ń', 'n'),
    ('ň', 'n'),
    ('ǹ', 'n'),
    ('Ń', 'n'),
    ('Ň', 'n'),
    ('Ǹ', 'n'),
    ('ḿ', 'm'),
    ('Ḿ', 'm'),
];

/// A character that cannot appear in a spelling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BadCharacter {
    /// The offending character.
    pub character: char,
}

impl fmt::Display for BadCharacter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid character {:?} in spelling", self.character)
    }
}

impl std::error::Error for BadCharacter {}

/// Rewrite `input` into the canonical spelling form.
///
/// The canonical form is lowercase ASCII, tone marks removed, `ü` written as
/// `v`, syllables separated by single spaces and `'` separators dropped. This
/// is applied to dictionaries as they are parsed, so hand-written entries may
/// use natural pinyin: `lǜ` and `lv` are the same word, and so are `xi'an` and
/// `xi an`.
///
/// Unlike a lenient cleaner, this reports the first character it does not
/// understand. A dictionary is a text file people edit by hand, so a stray
/// full-width comma should be an error with a line number rather than a word
/// that silently loses a syllable.
pub fn normalize_spelling(input: &str) -> Result<String, BadCharacter> {
    let mut out = String::with_capacity(input.len());
    let mut pending_space = false;
    for ch in input.chars() {
        if is_separator(ch) {
            pending_space = !out.is_empty();
            continue;
        }
        if ch.is_whitespace() {
            pending_space = !out.is_empty();
            continue;
        }
        let Some(folded) = fold_char(ch) else {
            return Err(BadCharacter { character: ch });
        };
        if pending_space {
            out.push(' ');
            pending_space = false;
        }
        out.push(folded);
    }
    Ok(out)
}

/// Characters that only separate syllables.
fn is_separator(ch: char) -> bool {
    matches!(ch, '\'' | '\u{2019}' | '\u{02bc}' | '\u{02b9}')
}

/// Fold one character to lowercase toneless ASCII, or `None` if it is not part
/// of the spelling alphabet at all.
fn fold_char(ch: char) -> Option<char> {
    if ch.is_ascii_alphanumeric() {
        return Some(ch.to_ascii_lowercase());
    }
    TONE_FOLD
        .iter()
        .find(|(from, _)| *from == ch)
        .map(|(_, to)| *to)
}

/// True if `s` is a valid canonical spelling: `[a-z0-9]+` groups joined by
/// single spaces.
pub fn is_valid_spelling(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let mut previous_was_space = true;
    for ch in s.chars() {
        if ch == ' ' {
            if previous_was_space {
                return false;
            }
            previous_was_space = true;
        } else if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            previous_was_space = false;
        } else {
            return false;
        }
    }
    !previous_was_space
}

#[cfg(test)]
mod tests {
    use super::*;

    fn normalize(input: &str) -> String {
        normalize_spelling(input).unwrap_or_else(|e| panic!("{input}: {e}"))
    }

    #[test]
    fn folds_tones_and_umlaut() {
        assert_eq!(normalize("zhōng guó"), "zhong guo");
        assert_eq!(normalize("lǜ"), "lv");
        assert_eq!(normalize("nǚ"), "nv");
        assert_eq!(normalize("lüè"), "lve");
        assert_eq!(normalize("ê"), "e");
        assert_eq!(normalize("ń"), "n");
        assert_eq!(normalize("ḿ"), "m");
    }

    #[test]
    fn collapses_whitespace_and_separators() {
        assert_eq!(normalize("  Xi  An "), "xi an");
        assert_eq!(normalize("xi'an"), "xi an");
        assert_eq!(normalize("xi’an"), "xi an");
        assert_eq!(normalize("zhongguo"), "zhongguo");
        assert_eq!(normalize("ni3 hao3"), "ni3 hao3");
    }

    #[test]
    fn rejects_characters_outside_the_alphabet() {
        assert_eq!(
            normalize_spelling("zh ong!").err(),
            Some(BadCharacter { character: '!' })
        );
        assert_eq!(
            normalize_spelling("a.b").err(),
            Some(BadCharacter { character: '.' })
        );
        assert_eq!(
            normalize_spelling("中").err(),
            Some(BadCharacter { character: '中' })
        );
        assert_eq!(
            normalize_spelling("a，b").err(),
            Some(BadCharacter { character: '，' })
        );
    }

    #[test]
    fn validates_spelling_alphabet() {
        assert!(is_valid_spelling("zhong guo"));
        assert!(is_valid_spelling("a"));
        assert!(!is_valid_spelling(""));
        assert!(!is_valid_spelling(" zhong"));
        assert!(!is_valid_spelling("zhong  guo"));
        assert!(!is_valid_spelling("zhong "));
        assert!(!is_valid_spelling("zhong-guo"));
    }
}
