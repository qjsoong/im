//! Pinyin primitives shared by the dictionary pipeline.
//!
//! The dictionary stores tones-free, lowercase ASCII spellings. The Chinese
//! `ü` is written as `v` so that every spelling stays typeable on an ASCII
//! keyboard, and `v` is a proper pinyin syllable letter here.

/// Unicode ranges accepted as Han characters in dictionary text fields.
const HAN_RANGES: &[(u32, u32)] = &[
    (0x3400, 0x4DBF), // CJK Unified Ideographs Extension A
    (0x4E00, 0x9FFF), // CJK Unified Ideographs
    (0xF900, 0xFAFF), // CJK Compatibility Ideographs
];

/// Tone-marked letters and the plain spelling they map to.
///
/// `ü` and its marked forms become `v`, `ê` loses its circumflex, and the
/// syllabic nasals `ń`, `ň`, `ǹ`, `ḿ` lose their accent.
const TONE_MAP: &[(char, char)] = &[
    ('ā', 'a'),
    ('á', 'a'),
    ('ǎ', 'a'),
    ('à', 'a'),
    ('ē', 'e'),
    ('é', 'e'),
    ('ě', 'e'),
    ('è', 'e'),
    ('ê', 'e'),
    ('ế', 'e'),
    ('ề', 'e'),
    ('ể', 'e'),
    ('ễ', 'e'),
    ('ệ', 'e'),
    ('ī', 'i'),
    ('í', 'i'),
    ('ǐ', 'i'),
    ('ì', 'i'),
    ('ō', 'o'),
    ('ó', 'o'),
    ('ǒ', 'o'),
    ('ò', 'o'),
    ('ū', 'u'),
    ('ú', 'u'),
    ('ǔ', 'u'),
    ('ù', 'u'),
    ('ü', 'v'),
    ('ǖ', 'v'),
    ('ǘ', 'v'),
    ('ǚ', 'v'),
    ('ǜ', 'v'),
    ('ń', 'n'),
    ('ň', 'n'),
    ('ǹ', 'n'),
    ('ḿ', 'm'),
];

/// Multiplier applied to the natural logarithm of a jieba frequency.
const FREQ_SCALE: f64 = 1000.0;

/// Weight bonus for records that have a hand-curated reading.
pub const CURATED_BONUS: u32 = 500;

/// Weight bonus for single-character records.
///
/// Single characters are the entries users reach by typing one syllable, so
/// they stay ahead of rarely used multi-character words.
pub const SINGLE_CHAR_BONUS: u32 = 3000;

/// True when `c` belongs to a Han block accepted by the dictionary format.
pub fn is_han(c: char) -> bool {
    let code = c as u32;
    HAN_RANGES
        .iter()
        .any(|&(start, end)| (start..=end).contains(&code))
}

/// True for Unicode combining marks that the upstream data uses in place of
/// precomposed tone-marked letters.
fn is_combining_mark(c: char) -> bool {
    ('\u{300}'..='\u{36f}').contains(&c)
}

/// Rewrite `raw` as lowercase pinyin without tone marks.
///
/// Characters that are not tone-marked letters are kept, so callers can
/// validate the result with [`is_syllable`].
pub fn strip_tones(raw: &str) -> String {
    let mut plain = String::with_capacity(raw.len());
    for c in raw.chars() {
        for lowered in c.to_lowercase() {
            if is_combining_mark(lowered) {
                continue;
            }
            match TONE_MAP.iter().find(|&&(marked, _)| marked == lowered) {
                Some(&(_, replacement)) => plain.push(replacement),
                None => plain.push(lowered),
            }
        }
    }
    plain
}

/// True when `s` is a single lowercase ASCII pinyin syllable.
pub fn is_syllable(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|byte| byte.is_ascii_lowercase())
}

/// Normalize one raw syllable, or `None` when it is not pinyin.
pub fn normalize_syllable(raw: &str) -> Option<String> {
    let plain = strip_tones(raw);
    is_syllable(&plain).then_some(plain)
}

/// True when `s` is a sequence of pinyin syllables separated by single spaces.
pub fn is_valid_spelling(s: &str) -> bool {
    !s.is_empty() && s.split(' ').all(is_syllable)
}

/// Iterate over the syllables of a spelling.
///
/// Callers must check [`is_valid_spelling`] first; on invalid input the
/// iteration yields empty pieces.
pub fn syllables(spelling: &str) -> std::str::Split<'_, char> {
    spelling.split(' ')
}

/// Score a dictionary record into `0..=65535`.
///
/// `freq` is the jieba frequency of the word, or 1 when the word does not
/// occur in the jieba dictionary. `single_char` marks one-character records
/// and `curated` marks readings that also appear in the hand-curated
/// upstream phrase data.
///
/// The weight is `min(65535, round(1000 * ln(freq).max(0)) + 3000 *
/// single_char + 500 * curated)`: it grows logarithmically with corpus
/// frequency, so a word used ten times more often is only marginally
/// heavier, and ties are broken by the canonical record order.
pub fn weight(freq: u64, single_char: bool, curated: bool) -> u16 {
    let logged = if freq > 1 { (freq as f64).ln() } else { 0.0 };
    let mut total = (FREQ_SCALE * logged).round() as u32;
    if single_char {
        total += SINGLE_CHAR_BONUS;
    }
    if curated {
        total += CURATED_BONUS;
    }
    total.min(u32::from(u16::MAX)) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_tones_removes_ascii_tone_marks() {
        assert_eq!(strip_tones("zhōng"), "zhong");
        assert_eq!(strip_tones("dì"), "di");
        assert_eq!(strip_tones("hǎo"), "hao");
        assert_eq!(strip_tones("wǔ"), "wu");
        assert_eq!(strip_tones("lǜ"), "lv");
    }

    #[test]
    fn strip_tones_writes_umlaut_as_v() {
        assert_eq!(strip_tones("nǚ"), "nv");
        assert_eq!(strip_tones("lüe"), "lve");
        assert_eq!(strip_tones("lǖ"), "lv");
        assert_eq!(strip_tones("ü"), "v");
    }

    #[test]
    fn strip_tones_keeps_characters_that_are_not_pinyin() {
        assert_eq!(strip_tones("绿"), "绿");
        assert_eq!(strip_tones("A1"), "a1");
    }

    #[test]
    fn strip_tones_handles_e_with_circumflex() {
        assert_eq!(strip_tones("ê"), "e");
        assert_eq!(strip_tones("ế"), "e");
        assert_eq!(strip_tones("ề"), "e");
    }

    #[test]
    fn strip_tones_handles_syllabic_nasals() {
        assert_eq!(strip_tones("ń"), "n");
        assert_eq!(strip_tones("ň"), "n");
        assert_eq!(strip_tones("ǹ"), "n");
        assert_eq!(strip_tones("ḿ"), "m");
    }

    #[test]
    fn strip_tones_lowercases_and_drops_combining_marks() {
        assert_eq!(strip_tones("ZHŌNG"), "zhong");
        assert_eq!(strip_tones("n\u{304}"), "n");
        assert_eq!(strip_tones("e\u{301}"), "e");
    }

    #[test]
    fn normalize_syllable_rejects_non_pinyin() {
        assert_eq!(normalize_syllable("zhōng"), Some("zhong".to_owned()));
        assert_eq!(normalize_syllable("lv"), Some("lv".to_owned()));
        assert_eq!(normalize_syllable("ni3"), None);
        assert_eq!(normalize_syllable(""), None);
        assert_eq!(normalize_syllable("n!"), None);
        assert_eq!(normalize_syllable("yi ding"), None);
    }

    #[test]
    fn is_han_accepts_exactly_the_three_blocks() {
        assert!(is_han('㐀')); // U+3400, first Extension A ideograph
        assert!(is_han('\u{4DBF}'));
        assert!(is_han('一')); // U+4E00
        assert!(is_han('\u{9FFF}'));
        assert!(is_han('豈')); // U+F900, first compatibility ideograph
        assert!(is_han('\u{FAFF}'));

        assert!(!is_han('\u{33FF}'));
        assert!(!is_han('〇')); // U+3007
        assert!(!is_han('\u{4DC0}'));
        assert!(!is_han('\u{A000}'));
        assert!(!is_han('\u{F8FF}'));
        assert!(!is_han('\u{FB00}'));
        assert!(!is_han('の'));
        assert!(!is_han('A'));
        assert!(!is_han('1'));
        assert!(!is_han(' '));
        assert!(!is_han('😀'));
        assert!(!is_han('\u{20000}')); // Extension B, outside the format
    }

    #[test]
    fn is_valid_spelling_requires_single_spaces() {
        assert!(is_valid_spelling("yi"));
        assert!(is_valid_spelling("zhong guo"));
        assert!(is_valid_spelling("nv hai"));

        assert!(!is_valid_spelling(""));
        assert!(!is_valid_spelling(" yi"));
        assert!(!is_valid_spelling("yi "));
        assert!(!is_valid_spelling("yi  ding"));
        assert!(!is_valid_spelling("yi ding3"));
        assert!(!is_valid_spelling("nv\u{304} hai"));
    }

    #[test]
    fn syllables_splits_on_spaces() {
        assert_eq!(syllables("zhong guo").collect::<Vec<_>>(), ["zhong", "guo"]);
        assert_eq!(syllables("yi").count(), 1);
    }

    #[test]
    fn weight_is_monotonic_in_frequency() {
        let mut previous = 0;
        for freq in [1u64, 2, 3, 10, 100, 1_000, 1_000_000, u64::MAX] {
            let current = weight(freq, false, false);
            assert!(current >= previous, "freq {freq}: {current} < {previous}");
            previous = current;
        }
        assert_eq!(weight(1, false, false), 0);
        assert_eq!(weight(100, false, false), 4605);
    }

    #[test]
    fn weight_applies_bonuses_and_caps() {
        assert_eq!(u32::from(weight(1, true, false)), SINGLE_CHAR_BONUS);
        assert_eq!(u32::from(weight(1, false, true)), CURATED_BONUS);
        assert_eq!(
            weight(1, true, true),
            (SINGLE_CHAR_BONUS + CURATED_BONUS) as u16
        );
        assert_eq!(
            weight(u64::MAX, true, true),
            weight(u64::MAX, false, false) + 3500
        );
    }

    #[test]
    fn rare_single_characters_outweigh_rare_multi_character_words() {
        let rare_character = weight(1, true, false);
        let rare_word = weight(3, false, false);
        assert!(rare_character > rare_word);
    }
}
