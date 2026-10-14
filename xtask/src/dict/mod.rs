//! Pinyin dictionary data pipeline.
//!
//! Three stages, one per `xtask dict` action:
//!
//! * [`fetch`] downloads the upstream data files into `target/dict-src`.
//! * [`build`] turns those files into `data/dict/pinyin.tsv` and
//!   `data/dict/syllables.txt`.
//! * [`check`] validates the generated files offline, so CI can guard the
//!   frozen format without network access.

pub mod build;
pub mod check;
pub mod fetch;
pub mod pinyin;
pub mod sources;

use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// One line of `data/dict/pinyin.tsv`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    /// Han-only word.
    pub text: String,
    /// Lowercase pinyin syllables separated by single spaces.
    pub spelling: String,
    /// Score in `0..=65535`; a greater weight means a more common word.
    pub weight: u16,
}

/// Canonical order of dictionary records.
///
/// Records are ordered by spelling, then by weight descending, then by text,
/// comparing byte strings. The order is total because `(text, spelling)` is
/// unique, which makes the generated file reproducible.
pub fn compare_records(left: &Record, right: &Record) -> Ordering {
    left.spelling
        .as_bytes()
        .cmp(right.spelling.as_bytes())
        .then_with(|| right.weight.cmp(&left.weight))
        .then_with(|| left.text.as_bytes().cmp(right.text.as_bytes()))
}

/// Locations read and written by the `dict` actions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    /// Directory holding the downloaded upstream files.
    pub src_dir: PathBuf,
    /// Generated dictionary, `data/dict/pinyin.tsv`.
    pub dict: PathBuf,
    /// Generated syllable list, `data/dict/syllables.txt`.
    pub syllables: PathBuf,
}

impl Paths {
    /// Paths of the workspace that contains this crate.
    pub fn workspace() -> Result<Self> {
        let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let root = crate_dir
            .parent()
            .context("xtask has no parent directory")?;
        Ok(Self::with_root(root))
    }

    /// Paths inside `root`, which is treated as the workspace root.
    pub fn with_root(root: &Path) -> Self {
        Self {
            src_dir: root.join("target/dict-src"),
            dict: root.join("data/dict/pinyin.tsv"),
            syllables: root.join("data/dict/syllables.txt"),
        }
    }

    /// Path of the upstream file `name` inside the download directory.
    pub fn source(&self, name: &str) -> PathBuf {
        self.src_dir.join(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(text: &str, spelling: &str, weight: u16) -> Record {
        Record {
            text: text.to_owned(),
            spelling: spelling.to_owned(),
            weight,
        }
    }

    #[test]
    fn compare_orders_by_spelling_then_weight_then_text() {
        let mut records = [
            record("你", "ni", 1),
            record("乙", "yi", 10),
            record("一", "yi", 20),
            record("爱", "ai", 1),
        ];
        records.sort_by(compare_records);
        let order: Vec<(&str, u16)> = records
            .iter()
            .map(|record| (record.text.as_str(), record.weight))
            .collect();
        assert_eq!(order, [("爱", 1), ("你", 1), ("一", 20), ("乙", 10)]);
    }

    #[test]
    fn compare_is_a_total_order_for_distinct_records() {
        let left = record("一", "yi", 5);
        let right = record("乙", "yi", 5);
        assert_eq!(compare_records(&left, &right), Ordering::Less);
        assert_eq!(compare_records(&right, &left), Ordering::Greater);
        assert_eq!(compare_records(&left, &left.clone()), Ordering::Equal);
    }

    #[test]
    fn paths_are_relative_to_the_given_root() {
        let paths = Paths::with_root(Path::new("/tmp/root"));
        assert_eq!(
            paths.source("jieba-dict.txt"),
            Path::new("/tmp/root/target/dict-src/jieba-dict.txt")
        );
        assert_eq!(paths.dict, Path::new("/tmp/root/data/dict/pinyin.tsv"));
        assert_eq!(
            paths.syllables,
            Path::new("/tmp/root/data/dict/syllables.txt")
        );
    }
}
