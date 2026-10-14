//! Parsers for the upstream pinyin data files.
//!
//! The upstream files are line oriented text:
//!
//! * `mozillazg/pinyin-data` `pinyin.txt`: `U+XXXX: pinyin1,pinyin2  # 字`
//! * `mozillazg/phrase-pinyin-data` phrase files: `词语: pīn yīn`
//! * `fxsjy/jieba` `dict.txt`: `词语 频率 词性`
//!
//! Comments start with `#` and run to the end of the line. Lines that cannot
//! be expressed in the dictionary format are dropped while loading, because
//! the upstream files also cover characters and words the format rejects
//! (Extension B ideographs, kana, phrases containing digits, and so on).

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use super::pinyin;

/// A word with a validated, tone-free pinyin spelling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reading {
    /// Han-only word.
    pub text: String,
    /// Lowercase pinyin syllables separated by single spaces.
    pub spelling: String,
}

impl Reading {
    /// Validate `text` with its raw, still tone-marked pinyin `raw`.
    ///
    /// Returns `None` unless `text` is non-empty and Han-only and `raw`
    /// yields exactly one pinyin syllable per character of `text`.
    pub fn parse(text: &str, raw: &str) -> Option<Self> {
        if text.is_empty() || !text.chars().all(pinyin::is_han) {
            return None;
        }
        let mut words = Vec::new();
        for syllable in raw.split_whitespace() {
            words.push(pinyin::normalize_syllable(syllable)?);
        }
        if words.len() != text.chars().count() {
            return None;
        }
        Some(Self {
            text: text.to_owned(),
            spelling: words.join(" "),
        })
    }
}

/// How much of an upstream file the loaders could use.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct LoadStats {
    /// Data lines, that is non-comment and non-blank lines.
    pub data_lines: usize,
    /// Readings that passed validation.
    pub readings: usize,
    /// Data lines rejected because text or pinyin left the format.
    pub rejected_lines: usize,
}

/// Read `path` and validate every line of it.
fn load<F>(path: &Path, mut parse: F) -> Result<(Vec<Reading>, LoadStats)>
where
    F: FnMut(&str) -> Vec<Reading>,
{
    let contents = fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let mut readings = Vec::new();
    let mut stats = LoadStats::default();
    for line in contents.lines() {
        if strip_comment(line).is_none() {
            continue;
        }
        stats.data_lines += 1;
        let parsed = parse(line);
        if parsed.is_empty() {
            stats.rejected_lines += 1;
        } else {
            stats.readings += parsed.len();
            readings.extend(parsed);
        }
    }
    Ok((readings, stats))
}

/// Load the single-character readings of `pinyin-data/pinyin.txt`.
pub fn load_char_readings(path: &Path) -> Result<(Vec<Reading>, LoadStats)> {
    load(path, parse_char_line)
}

/// Load the phrase readings of a `phrase-pinyin-data` file.
pub fn load_phrase_readings(path: &Path) -> Result<(Vec<Reading>, LoadStats)> {
    load(path, |line| parse_phrase_line(line).into_iter().collect())
}

/// Load the hand-curated readings of `phrase-pinyin-data/pinyin.txt`,
/// keyed by word.
pub fn load_curated_readings(
    path: &Path,
) -> Result<(HashMap<String, HashSet<String>>, LoadStats)> {
    let (readings, stats) =
        load(path, |line| parse_phrase_line(line).into_iter().collect())?;
    let mut curated: HashMap<String, HashSet<String>> = HashMap::new();
    for reading in readings {
        curated
            .entry(reading.text)
            .or_default()
            .insert(reading.spelling);
    }
    Ok((curated, stats))
}

/// Load the word frequencies of `jieba/dict.txt`.
///
/// When a word occurs more than once, the greatest frequency wins.
pub fn load_frequencies(path: &Path) -> Result<HashMap<String, u64>> {
    let contents = fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let mut frequencies: HashMap<String, u64> = HashMap::new();
    for line in contents.lines() {
        if let Some((word, freq)) = parse_freq_line(line) {
            frequencies
                .entry(word)
                .and_modify(|current| *current = (*current).max(freq))
                .or_insert(freq);
        }
    }
    Ok(frequencies)
}

/// Remove a trailing `#` comment and surrounding whitespace.
///
/// Returns `None` for blank lines and for lines that are only a comment.
fn strip_comment(line: &str) -> Option<&str> {
    let body = line
        .split_once('#')
        .map_or(line, |(before, _)| before)
        .trim();
    if body.is_empty() { None } else { Some(body) }
}

/// Parse one line of `pinyin-data/pinyin.txt`.
///
/// Each comma-separated pinyin of the line becomes one reading; lines that
/// are comments, that carry no pinyin, or whose code point is not a Han
/// character yield no readings.
pub fn parse_char_line(line: &str) -> Vec<Reading> {
    let Some(body) = strip_comment(line) else {
        return Vec::new();
    };
    let Some((code, raw)) = body.split_once(':') else {
        return Vec::new();
    };
    let Some(code) = code.trim().strip_prefix("U+") else {
        return Vec::new();
    };
    let Ok(code) = u32::from_str_radix(code, 16) else {
        return Vec::new();
    };
    let Some(character) = char::from_u32(code) else {
        return Vec::new();
    };
    let text = character.to_string();
    let mut readings = Vec::new();
    for raw in raw.split(',') {
        if let Some(reading) = Reading::parse(&text, raw) {
            readings.push(reading);
        }
    }
    readings
}

/// Parse one line of a `phrase-pinyin-data` file.
pub fn parse_phrase_line(line: &str) -> Option<Reading> {
    let body = strip_comment(line)?;
    let (text, raw) = body.split_once(':')?;
    Reading::parse(text.trim(), raw)
}

/// Parse one `词语 频率 词性` line of `jieba/dict.txt`.
pub fn parse_freq_line(line: &str) -> Option<(String, u64)> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    if fields.len() != 3 {
        return None;
    }
    Some((fields[0].to_owned(), fields[1].parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reading(text: &str, spelling: &str) -> Reading {
        Reading {
            text: text.to_owned(),
            spelling: spelling.to_owned(),
        }
    }

    #[test]
    fn reading_parse_normalizes_tones() {
        assert_eq!(
            Reading::parse("中国", "zhōng guó"),
            Some(reading("中国", "zhong guo"))
        );
        assert_eq!(Reading::parse("女", "nǚ"), Some(reading("女", "nv")));
    }

    #[test]
    fn reading_parse_requires_one_syllable_per_character() {
        assert_eq!(Reading::parse("一丁点儿", "yī dīng diǎn"), None);
        assert_eq!(
            Reading::parse("一丁点儿", "yī dīng diǎn er"),
            Some(reading("一丁点儿", "yi ding dian er"))
        );
        assert_eq!(Reading::parse("", "yi"), None);
    }

    #[test]
    fn reading_parse_rejects_non_han_text() {
        assert_eq!(Reading::parse("C#", "see"), None);
        assert_eq!(Reading::parse("一2", "yi er"), None);
        assert_eq!(Reading::parse("一 二", "yi er"), None);
        assert_eq!(Reading::parse("〇", "ling"), None);
        assert_eq!(Reading::parse("の", "no"), None);
    }

    #[test]
    fn reading_parse_rejects_non_pinyin_syllables() {
        assert_eq!(Reading::parse("一", "yi3"), None);
        assert_eq!(Reading::parse("一", "—"), None);
    }

    #[test]
    fn parse_char_line_reads_every_reading() {
        let readings = parse_char_line("U+4E2D: zhōng,zhòng  # 中");
        assert_eq!(
            readings,
            vec![reading("中", "zhong"), reading("中", "zhong")]
        );
    }

    #[test]
    fn parse_char_line_skips_comments_and_non_han_code_points() {
        assert!(parse_char_line("# version: 0.15.0").is_empty());
        assert!(parse_char_line("").is_empty());
        assert!(parse_char_line("U+3007: líng,yuán  # 〇").is_empty());
        assert!(parse_char_line("U+20057: lǎ  # 𠁗").is_empty());
        assert!(parse_char_line("U+F900: qǐ  # 豈").len() == 1);
    }

    #[test]
    fn parse_phrase_line_reads_word_and_pinyin() {
        assert_eq!(
            parse_phrase_line("中国: zhōng guó"),
            Some(reading("中国", "zhong guo"))
        );
        assert_eq!(
            parse_phrase_line(" 北京: běi jīng  # 注释"),
            Some(reading("北京", "bei jing"))
        );
        assert_eq!(parse_phrase_line("# 注释"), None);
        assert_eq!(parse_phrase_line(""), None);
        assert_eq!(parse_phrase_line("一丁不识: yī dīng bù"), None);
    }

    #[test]
    fn parse_freq_line_reads_word_and_frequency() {
        assert_eq!(
            parse_freq_line("的 3188252 uj"),
            Some(("的".to_owned(), 3188252))
        );
        assert_eq!(
            parse_freq_line("一 217830 m"),
            Some(("一".to_owned(), 217830))
        );
        assert_eq!(parse_freq_line("的 3188252"), None);
        assert_eq!(parse_freq_line("的 x uj"), None);
        assert_eq!(parse_freq_line(""), None);
    }

    #[test]
    fn load_reports_rejected_lines() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("pinyin.txt");
        fs::write(
            &path,
            "# comment\nU+4E2D: zhōng  # 中\nU+3007: líng  # 〇\n\nU+3400: qiū  # 㐀\n",
        )?;
        let (readings, stats) = load_char_readings(&path)?;
        assert_eq!(readings.len(), 2);
        assert_eq!(stats.data_lines, 3);
        assert_eq!(stats.readings, 2);
        assert_eq!(stats.rejected_lines, 1);
        Ok(())
    }
}
