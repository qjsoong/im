//! Dictionary files: the words an input method knows about.
//!
//! # Format
//!
//! A dictionary is a UTF-8, tab-separated text file. This format was chosen so
//! that dictionaries stay readable, greppable, diffable and mergeable. Each
//! record has three fields — `text`, `spelling`, `weight` — written between
//! tabs, so the sample below shows tabs as the alignment between columns:
//!
//! ```text
//! # text  spelling  weight
//! 中      zhong     11000
//! 国      guo       10000
//! 中国    zhong guo 9500
//! ```
//!
//! * Blank lines and lines starting with `#` are ignored.
//! * Fields are separated by a single tab. Trailing fields may be omitted, in
//!   which case `weight` defaults to [`DEFAULT_WEIGHT`].
//! * `spelling` is space-separated pinyin syllables. Tone marks, `ü` and
//!   `'` separators are accepted and folded into the canonical form, so
//!   `lǜ`, `lv` and `lü` are the same word. See [`crate::text`].
//! * `weight` is an integer in `0..=65535`; higher means "more likely".
//!
//! Table-based schemes (wubi, cangjie, stroke) reuse the same file and put the
//! code itself in the `spelling` column.

use std::fmt;
use std::path::Path;

use crate::error::{Error, Result};
use crate::text::{is_valid_spelling, normalize_spelling};

/// Weight used when a line does not specify one.
pub const DEFAULT_WEIGHT: u16 = 0;

/// Largest weight that can be written to a dictionary file.
pub const MAX_WEIGHT: u16 = u16::MAX;

/// Bonus added to entries that come from the user's own dictionary, so an
/// explicit user choice always outranks a shipped one.
pub const USER_BONUS: u32 = 1 << 20;

/// Where a dictionary entry came from; used to break ties and to let front ends
/// annotate candidates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum Origin {
    /// Shipped with `im` or installed system-wide.
    #[default]
    System,
    /// From the user's own dictionary, for example `~/.local/share/im/user.tsv`.
    User,
}

impl Origin {
    /// The priority bonus applied to weights from this origin.
    pub const fn bonus(self) -> u32 {
        match self {
            Self::System => 0,
            Self::User => USER_BONUS,
        }
    }
}

impl fmt::Display for Origin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::System => "system",
            Self::User => "user",
        })
    }
}

/// One dictionary record: a word, how it is spelled, and how likely it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The word that gets committed, for example `中国`.
    pub text: String,
    /// Canonical spelling, for example `zhong guo`; for table schemes this is
    /// the code itself.
    pub spelling: String,
    /// Relative likelihood in `0..=65535`.
    pub weight: u16,
    /// Where the entry was loaded from.
    pub origin: Origin,
}

impl Entry {
    /// Create an entry with an already canonical spelling.
    pub fn new(
        text: impl Into<String>,
        spelling: impl Into<String>,
        weight: u16,
    ) -> Self {
        Self {
            text: text.into(),
            spelling: spelling.into(),
            weight,
            origin: Origin::System,
        }
    }

    /// Weight including the [`Origin`] bonus, used for ranking.
    pub fn ranking_weight(&self) -> u32 {
        u32::from(self.weight) + self.origin.bonus()
    }
}

/// An ordered set of [`Entry`] values.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Dictionary {
    entries: Vec<Entry>,
}

impl Dictionary {
    /// Create an empty dictionary.
    pub fn new() -> Self {
        Self::default()
    }

    /// Parse dictionary text. `origin` is stamped onto every entry.
    ///
    /// Entries are kept in file order and are *not* deduplicated here; use
    /// [`Dictionary::merged`] or [`Dictionary::canonicalize`] when duplicates
    /// have to go away. Parsing is a single pass so that a large shipped
    /// dictionary loads quickly.
    ///
    /// Returns [`Error::Dictionary`] with the offending line number on the
    /// first malformed record.
    pub fn parse(text: &str, origin: Origin) -> Result<Self> {
        Self::parse_named(text, "<inline>", origin)
    }

    /// Like [`Dictionary::parse`] but reports `path` in error messages.
    pub fn parse_named(text: &str, path: &str, origin: Origin) -> Result<Self> {
        let mut entries = Vec::new();
        for (index, raw) in text.lines().enumerate() {
            let line = index + 1;
            let trimmed = raw.trim_end_matches(['\r', '\n']);
            let stripped = trimmed.trim_start();
            if stripped.starts_with('#') {
                continue;
            }
            let fields = parse_fields(stripped, path, line)?;
            let Some((text, spelling, weight)) = fields else {
                continue;
            };
            entries.push(Entry {
                text,
                spelling,
                weight,
                origin,
            });
        }
        Ok(Self { entries })
    }

    /// Read a dictionary file from disk.
    pub fn read(path: &Path, origin: Origin) -> Result<Self> {
        let text =
            std::fs::read_to_string(path).map_err(|e| Error::io(path.display(), e))?;
        Self::parse_named(&text, &path.display().to_string(), origin)
    }

    /// Read a dictionary if it exists, otherwise return `None`.
    pub fn read_if_exists(path: &Path, origin: Origin) -> Result<Option<Self>> {
        match std::fs::metadata(path) {
            Ok(_) => Self::read(path, origin).map(Some),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(Error::io(path.display(), e)),
        }
    }

    /// All entries, in file order.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when the dictionary holds no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Add an entry, replacing the weight of an identical `(text, spelling)`.
    ///
    /// This is a linear scan; it is meant for the handful of entries a front
    /// end edits interactively, not for merging dictionaries.
    pub fn insert(&mut self, entry: Entry) {
        if let Some(existing) = self
            .entries
            .iter_mut()
            .find(|e| e.text == entry.text && e.spelling == entry.spelling)
        {
            existing.weight = existing.weight.max(entry.weight);
            existing.origin = existing.origin.max(entry.origin);
        } else {
            self.entries.push(entry);
        }
    }

    /// Append every entry of `other` and re-canonicalize.
    pub fn extend(&mut self, other: Dictionary) {
        self.entries.extend(other.entries);
        self.canonicalize();
    }

    /// Merge dictionaries into a new one.
    ///
    /// Duplicate `(text, spelling)` pairs keep the highest weight and origin,
    /// so a user entry always beats a shipped one.
    pub fn merged(dictionaries: impl IntoIterator<Item = Dictionary>) -> Self {
        let mut dict = Self::new();
        for other in dictionaries {
            dict.entries.extend(other.entries);
        }
        dict.canonicalize();
        dict
    }

    /// Sort entries into the canonical order used when writing files.
    pub fn sort_canonical(&mut self) {
        self.entries.sort_by(canonical_cmp);
    }

    /// Sort canonically and drop duplicate `(text, spelling)` pairs.
    ///
    /// When the user's own dictionary redefines a word, the user's record wins
    /// outright — both its weight and its origin. Anything else would make
    /// `user.tsv` a suggestion rather than a statement.
    ///
    /// A sort is used rather than a hash map because dictionaries run to
    /// hundreds of thousands of entries and the result is a sorted file anyway.
    pub fn canonicalize(&mut self) {
        // Origin descending first means `dedup_by` sees the user's record first
        // and can drop the shipped one.
        self.entries.sort_by(|a, b| {
            a.text
                .cmp(&b.text)
                .then_with(|| a.spelling.cmp(&b.spelling))
                .then_with(|| b.origin.cmp(&a.origin))
                .then_with(|| b.weight.cmp(&a.weight))
        });
        self.entries
            .dedup_by(|a, b| a.text == b.text && a.spelling == b.spelling);
        self.sort_canonical();
    }

    /// Render the dictionary in the canonical, deterministic text format.
    ///
    /// Duplicates are removed, so writing a dictionary and reading it back is
    /// idempotent.
    pub fn to_tsv(&self) -> String {
        let mut dict = self.clone();
        dict.canonicalize();

        let mut out = String::with_capacity(dict.entries.len() * 24);
        out.push_str("# im dictionary\n");
        out.push_str("# text\tspelling\tweight\n");
        for entry in &dict.entries {
            out.push_str(&entry.text);
            out.push('\t');
            out.push_str(&entry.spelling);
            out.push('\t');
            out.push_str(&entry.weight.to_string());
            out.push('\n');
        }
        out
    }

    /// Write the dictionary in canonical form, creating parent directories.
    pub fn write(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| Error::io(parent.display(), e))?;
        }
        std::fs::write(path, self.to_tsv()).map_err(|e| Error::io(path.display(), e))
    }
}

/// Canonical write order: spelling, then weight descending, then text.
fn canonical_cmp(a: &Entry, b: &Entry) -> std::cmp::Ordering {
    a.spelling
        .cmp(&b.spelling)
        .then_with(|| b.weight.cmp(&a.weight))
        .then_with(|| a.text.cmp(&b.text))
}

/// Split and validate one non-comment, non-blank line.
///
/// Returns `Ok(None)` for blank lines.
fn parse_fields(
    line: &str,
    path: &str,
    number: usize,
) -> Result<Option<(String, String, u16)>> {
    if line.is_empty() {
        return Ok(None);
    }
    let mut fields = line.split('\t');
    let text = fields.next().unwrap_or_default().trim();
    let spelling_raw = fields.next().unwrap_or_default().trim();
    let weight_raw = fields.next().unwrap_or("").trim();

    if fields.next().is_some() {
        return Err(Error::dictionary(
            path,
            number,
            "expected 3 tab-separated fields",
        ));
    }
    if text.is_empty() {
        return Err(Error::dictionary(path, number, "empty text field"));
    }
    if text.chars().any(|c| c.is_whitespace()) {
        return Err(Error::dictionary(
            path,
            number,
            "text field must not contain whitespace",
        ));
    }
    if spelling_raw.is_empty() {
        return Err(Error::dictionary(path, number, "empty spelling field"));
    }
    let spelling = normalize_spelling(spelling_raw).map_err(|e| {
        Error::dictionary(
            path,
            number,
            format!("invalid spelling {spelling_raw:?}: {e}"),
        )
    })?;
    if !is_valid_spelling(&spelling) {
        return Err(Error::dictionary(
            path,
            number,
            format!("invalid spelling {spelling_raw:?}"),
        ));
    }
    let weight = if weight_raw.is_empty() {
        DEFAULT_WEIGHT
    } else {
        weight_raw.parse::<u16>().map_err(|_| {
            Error::dictionary(
                path,
                number,
                format!("weight {weight_raw:?} is not an integer in 0..={MAX_WEIGHT}"),
            )
        })?
    };
    Ok(Some((text.to_owned(), spelling, weight)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Dictionary {
        match Dictionary::parse(text, Origin::System) {
            Ok(dict) => dict,
            Err(e) => panic!("{e}"),
        }
    }

    #[test]
    fn parses_a_simple_dictionary() {
        let dict = parse(
            "# a comment\n\
             \n\
             中\tzhong\t11000\n\
             中国\tzhong guo\t9500\n\
             你\tni\n",
        );
        assert_eq!(dict.len(), 3);
        assert_eq!(dict.entries()[0].text, "中");
        assert_eq!(dict.entries()[0].weight, 11000);
        assert_eq!(dict.entries()[2].weight, DEFAULT_WEIGHT);
    }

    #[test]
    fn folds_spellings_on_load() {
        let dict = parse("女\tnǚ\t1\n略\tlüè\t1\n西安\txi'an\t1\n");
        assert_eq!(dict.entries()[0].spelling, "nv");
        assert_eq!(dict.entries()[1].spelling, "lve");
        assert_eq!(dict.entries()[2].spelling, "xi an");
    }

    #[test]
    fn reports_line_numbers() {
        let err = Dictionary::parse("中\tzhong\t1\nbad-line\n", Origin::System);
        let Err(Error::Dictionary { line, .. }) = err else {
            panic!("expected a dictionary error, got {err:?}");
        };
        assert_eq!(line, 2);
    }

    #[test]
    fn rejects_bad_weights() {
        let err = Dictionary::parse("中\tzhong\tnope\n", Origin::System);
        let Err(Error::Dictionary { message, .. }) = err else {
            panic!("expected a dictionary error, got {err:?}");
        };
        assert!(message.contains("weight"), "{message}");
    }

    #[test]
    fn rejects_bad_spellings() {
        let err = Dictionary::parse("中\tzh ong!\n", Origin::System);
        assert!(matches!(err, Err(Error::Dictionary { .. })));
    }

    #[test]
    fn sorts_dedups_and_round_trips() {
        let dict = parse("国\tguo\t1\n中\tzhong\t2\n中\tzhong\t5\n");
        assert_eq!(dict.len(), 3, "parsing keeps file order and duplicates");
        let text = dict.to_tsv();
        let reparsed = parse(&text);
        assert_eq!(reparsed.len(), 2, "writing drops duplicates");
        assert_eq!(reparsed.entries()[0].text, "国");
        assert_eq!(reparsed.entries()[1].weight, 5, "the highest weight wins");
        assert_eq!(reparsed.to_tsv(), text, "writing is deterministic");
    }

    #[test]
    fn merging_prefers_user_entries() {
        let system = parse("中\tzhong\t500\n国\tguo\t100\n");
        let mut user = parse("中\tzhong\t1\n");
        user.entries[0].origin = Origin::User;
        let merged = Dictionary::merged([system, user]);
        assert_eq!(merged.len(), 2);
        let zhong = merged
            .entries()
            .iter()
            .find(|e| e.text == "中")
            .unwrap_or_else(|| panic!("中 disappeared"));
        assert_eq!(zhong.origin, Origin::User);
        assert_eq!(
            zhong.weight, 1,
            "the user's record replaces the shipped one"
        );
        let guo = merged
            .entries()
            .iter()
            .find(|e| e.text == "国")
            .unwrap_or_else(|| panic!("国 disappeared"));
        assert_eq!(guo.origin, Origin::System);
        assert!(
            zhong.ranking_weight() > guo.ranking_weight(),
            "user entries outrank shipped ones even at a lower weight"
        );
    }

    #[test]
    fn user_entries_outrank_system_entries() {
        let system = Entry::new("中", "zhong", MAX_WEIGHT);
        let user = Entry {
            origin: Origin::User,
            ..Entry::new("中", "zhong", 0)
        };
        assert!(user.ranking_weight() > system.ranking_weight());
    }
}
