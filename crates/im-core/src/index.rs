//! The searchable index: codes to candidates.
//!
//! The index is a plain sorted array of records. Lookups are binary searches,
//! which keeps the structure tiny, cache friendly and — more importantly —
//! easy to reason about: "sorted by code" is the entire data invariant.
//!
//! Codes only ever contain `[a-z0-9]`. That restriction is what lets the prefix
//! search use the `code` + `'{'` upper bound trick, because `'{'` sorts after
//! every character the alphabet can contain.

use std::cmp::Ordering;

use crate::dict::{Dictionary, Entry, Origin};
use crate::error::Result;
use crate::schema::Schema;

/// How well a candidate matches what has been typed so far.
///
/// Candidates are emitted in this order, so the first page always shows the
/// most literal reading of the input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MatchKind {
    /// The candidate's whole code is exactly what was typed.
    Exact,
    /// The candidate's code is a shorter prefix of what was typed, so part of
    /// the input is still unconsumed — 中国 after typing `zhongguox`.
    Backoff,
    /// What was typed is a prefix of the candidate's code, so the candidate is
    /// the completion of an unfinished code — 中国 after typing `zhong`.
    Completion,
}

impl MatchKind {
    /// Short, stable name used by `im query` and the TUI.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::Backoff => "backoff",
            Self::Completion => "completion",
        }
    }
}

/// One match handed back to a front end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// The text to commit.
    pub text: String,
    /// The spelling of the underlying dictionary entry, for previews.
    pub spelling: String,
    /// The full code of the underlying dictionary entry.
    pub code: String,
    /// How many bytes of the (normalized) input this candidate accounts for.
    /// Anything beyond it stays in the buffer for the next lookup.
    pub consumed: usize,
    /// Ranking weight, including the [`Origin`] bonus.
    pub weight: u32,
    /// Where the entry came from.
    pub origin: Origin,
    /// How the candidate matched.
    pub kind: MatchKind,
}

/// Knobs for [`CodeIndex::lookup`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LookupOptions {
    /// Maximum number of candidates to return.
    pub limit: usize,
    /// Whether to offer completions of unfinished codes.
    pub completion: bool,
}

impl Default for LookupOptions {
    fn default() -> Self {
        Self {
            limit: 100,
            completion: true,
        }
    }
}

/// One entry of the sorted code array.
#[derive(Debug, Clone)]
struct Record {
    code: Box<str>,
    entry: u32,
}

/// A schema applied to a dictionary: the thing `Session` searches.
#[derive(Debug)]
pub struct CodeIndex {
    schema_id: String,
    dict: Dictionary,
    /// Sorted by code, then by descending ranking weight, then by text.
    records: Vec<Record>,
}

impl CodeIndex {
    /// Fold every entry of `dict` through `schema` and sort the result.
    pub fn build(schema: &Schema, dict: Dictionary) -> Result<Self> {
        let mut records = Vec::with_capacity(dict.len());
        for (index, entry) in dict.entries().iter().enumerate() {
            let code = sanitize_code(&schema.fold(&entry.spelling));
            if code.is_empty() {
                continue;
            }
            records.push(Record {
                code: code.into_boxed_str(),
                entry: index as u32,
            });
        }
        // Sorting must happen before the dictionary is moved into `Self`.
        let entries = dict.entries();
        records.sort_unstable_by(|a, b| compare_records(a, b, entries));
        Ok(Self {
            schema_id: schema.id.clone(),
            dict,
            records,
        })
    }

    /// Identifier of the schema the index was built for.
    pub fn schema_id(&self) -> &str {
        &self.schema_id
    }

    /// Number of indexed codes (one per distinct dictionary spelling).
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// True when no dictionary entry produced a code.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Every candidate whose code is exactly `input`, best first.
    pub fn exact(&self, input: &str) -> Vec<Candidate> {
        let input = sanitize_code(input);
        let (start, end) = self.equal_range(&input);
        (start..end)
            .map(|i| self.candidate(i, input.len(), MatchKind::Exact))
            .collect()
    }

    /// Find candidates for `input`.
    ///
    /// The result is ordered by [`MatchKind`], then by weight, then by code
    /// length, then by text — a total order, so paging is stable.
    pub fn lookup(&self, input: &str, options: LookupOptions) -> Vec<Candidate> {
        let input = sanitize_code(input);
        if input.is_empty() || options.limit == 0 || self.records.is_empty() {
            return Vec::new();
        }

        let mut out: Vec<Candidate> = Vec::new();
        // Exact matches first, then progressively shorter prefixes of the input.
        for consumed in (1..=input.len()).rev() {
            let (start, end) = self.equal_range(&input[..consumed]);
            if start == end {
                continue;
            }
            let kind = if consumed == input.len() {
                MatchKind::Exact
            } else {
                MatchKind::Backoff
            };
            for index in start..end {
                if out.len() >= options.limit {
                    break;
                }
                out.push(self.candidate(index, consumed, kind));
            }
            if out.len() >= options.limit {
                break;
            }
        }

        if options.completion && out.len() < options.limit {
            let budget = options.limit - out.len();
            out.extend(self.completions(&input, budget));
        }

        dedup_by_text(out)
    }

    /// Completions of the deepest prefix of `input` that indexes anything.
    fn completions(&self, input: &str, budget: usize) -> Vec<Candidate> {
        if budget == 0 {
            return Vec::new();
        }
        let Some(deepest) = (1..=input.len())
            .rev()
            .find(|&len| self.has_code_with_prefix(&input[..len]))
        else {
            return Vec::new();
        };
        let prefix = &input[..deepest];
        let (start, end) = self.prefix_range(prefix);
        // Records whose code equals the prefix are already in the result.
        let pool: Vec<usize> = (start..end)
            .filter(|&i| self.records[i].code.as_ref() != prefix)
            .collect();
        let mut pool = pool;
        if pool.len() > budget {
            let entries = self.dict.entries();
            pool.select_nth_unstable_by(budget, |&a, &b| {
                compare_ranking(&self.records[a], &self.records[b], entries)
            });
            pool.truncate(budget);
        }
        let entries = self.dict.entries();
        pool.sort_by(|&a, &b| {
            compare_ranking(&self.records[a], &self.records[b], entries)
        });
        pool.into_iter()
            .map(|i| self.candidate(i, input.len(), MatchKind::Completion))
            .collect()
    }

    fn candidate(&self, index: usize, consumed: usize, kind: MatchKind) -> Candidate {
        let record = &self.records[index];
        let entry = &self.dict.entries()[record.entry as usize];
        Candidate {
            text: entry.text.clone(),
            spelling: entry.spelling.clone(),
            code: record.code.to_string(),
            consumed,
            weight: entry.ranking_weight(),
            origin: entry.origin,
            kind,
        }
    }

    /// Half-open range of records whose code is exactly `code`.
    ///
    /// `code` + `'\0'` sorts after `code` itself and before any longer string
    /// that starts with it, which is exactly the upper bound we need.
    fn equal_range(&self, code: &str) -> (usize, usize) {
        let start = self.lower_bound(code);
        let end = self.lower_bound(&format!("{code}\0"));
        (start, end)
    }

    /// Half-open range of records whose code starts with `code`.
    fn prefix_range(&self, code: &str) -> (usize, usize) {
        if code.is_empty() {
            return (0, self.records.len());
        }
        let start = self.lower_bound(code);
        // `{` sorts after every character the code alphabet can contain.
        let end = self.lower_bound(&format!("{code}{{"));
        (start, end)
    }

    fn has_code_with_prefix(&self, code: &str) -> bool {
        let (start, end) = self.prefix_range(code);
        start < end
    }

    /// First record whose code is `>= code`.
    fn lower_bound(&self, code: &str) -> usize {
        self.records.partition_point(|r| r.code.as_ref() < code)
    }
}

/// Strip anything that is not `[a-z0-9]` and lowercase ASCII letters.
fn sanitize_code(code: &str) -> String {
    code.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// Build order: code ascending, then ranking.
fn compare_records(a: &Record, b: &Record, entries: &[Entry]) -> Ordering {
    a.code
        .cmp(&b.code)
        .then_with(|| compare_ranking(a, b, entries))
}

/// Ranking order: weight descending, then shorter code first, then text.
fn compare_ranking(a: &Record, b: &Record, entries: &[Entry]) -> Ordering {
    let wa = entries[a.entry as usize].ranking_weight();
    let wb = entries[b.entry as usize].ranking_weight();
    wb.cmp(&wa)
        .then_with(|| a.code.len().cmp(&b.code.len()))
        .then_with(|| {
            entries[a.entry as usize]
                .text
                .cmp(&entries[b.entry as usize].text)
        })
}

/// Keep the best-ranked occurrence of each distinct text.
///
/// A word can be reachable through several spellings (`xi an` and `xian`), and
/// showing it twice would be noise. Lists are short — they are bounded by
/// [`LookupOptions::limit`] — so the quadratic scan is cheaper than a set.
fn dedup_by_text(candidates: Vec<Candidate>) -> Vec<Candidate> {
    let mut out: Vec<Candidate> = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        if out.iter().any(|kept| kept.text == candidate.text) {
            continue;
        }
        out.push(candidate);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::builtin;

    fn index(schema_id: &str, source: &str) -> CodeIndex {
        let schema = builtin::get(schema_id).unwrap_or_else(|e| panic!("{e}"));
        let dict =
            Dictionary::parse(source, Origin::System).unwrap_or_else(|e| panic!("{e}"));
        schema.index([dict]).unwrap_or_else(|e| panic!("{e}"))
    }

    fn texts(candidates: &[Candidate]) -> Vec<&str> {
        candidates.iter().map(|c| c.text.as_str()).collect()
    }

    const SAMPLE: &str = "\
中\tzhong\t900\n\
国\tguo\t800\n\
中国\tzhong guo\t700\n\
中心\tzhong xin\t600\n\
种\tzhong\t500\n\
重\tzhong\t400\n\
果\tguo\t300\n\
过\tguo\t350\n\
说\tshuo\t200\n\
";

    #[test]
    fn exact_match_ranks_first() {
        let index = index("pinyin", SAMPLE);
        let found = index.lookup("zhongguo", LookupOptions::default());
        assert_eq!(texts(&found)[0], "中国");
        assert_eq!(found[0].kind, MatchKind::Exact);
        assert_eq!(found[0].consumed, 8);
    }

    #[test]
    fn backoff_keeps_the_unconsumed_input() {
        let index = index("pinyin", SAMPLE);
        let found = index.lookup("zhongguox", LookupOptions::default());
        assert_eq!(texts(&found), vec!["中国", "中", "种", "重"]);
        assert!(found.iter().all(|c| c.kind == MatchKind::Backoff));
        assert_eq!(found[0].consumed, 8, "中国 accounts for 'zhongguo'");
        assert_eq!(found[1].consumed, 5, "中 accounts for 'zhong'");
    }

    #[test]
    fn a_typo_still_offers_the_completion_of_the_valid_prefix() {
        let index = index("pinyin", SAMPLE);
        let found = index.lookup("zhongxq", LookupOptions::default());
        // 中/种/重 are readable as the first five keys, 中心 as the first six.
        assert_eq!(texts(&found), vec!["中", "种", "重", "中心"]);
        let completion = found
            .iter()
            .find(|c| c.text == "中心")
            .unwrap_or_else(|| panic!("中心 not offered"));
        assert_eq!(completion.kind, MatchKind::Completion);
        assert_eq!(
            completion.consumed, 7,
            "committing a completion clears the whole buffer"
        );
    }

    #[test]
    fn completion_finishes_unfinished_codes() {
        let index = index("pinyin", SAMPLE);
        let found = index.lookup("zhong", LookupOptions::default());
        assert_eq!(texts(&found), vec!["中", "种", "重", "中国", "中心"]);
        assert_eq!(found[0].kind, MatchKind::Exact);
        assert_eq!(found[3].kind, MatchKind::Completion);
        // Completions account for the whole input, so nothing is left over.
        assert_eq!(found[3].consumed, 5);
    }

    #[test]
    fn completion_can_be_disabled() {
        let index = index("pinyin", SAMPLE);
        let options = LookupOptions {
            completion: false,
            ..LookupOptions::default()
        };
        let found = index.lookup("zhong", options);
        assert_eq!(texts(&found), vec!["中", "种", "重"]);
    }

    #[test]
    fn limit_is_respected() {
        let index = index("pinyin", SAMPLE);
        let options = LookupOptions {
            limit: 2,
            ..LookupOptions::default()
        };
        assert_eq!(index.lookup("zhong", options).len(), 2);
    }

    #[test]
    fn unknown_input_yields_nothing() {
        let index = index("pinyin", SAMPLE);
        assert!(index.lookup("qqq", LookupOptions::default()).is_empty());
        assert!(index.lookup("", LookupOptions::default()).is_empty());
    }

    #[test]
    fn double_pinyin_folds_the_dictionary() {
        let index = index("double_pinyin", SAMPLE);
        assert_eq!(index.schema_id(), "double_pinyin");
        let found = index.lookup("vsgo", LookupOptions::default());
        assert_eq!(texts(&found)[0], "中国");
        // 说 is "uo" in 自然码: sh -> u, uo -> o.
        assert_eq!(
            texts(&index.lookup("uo", LookupOptions::default()))[0],
            "说"
        );
        // The full-pinyin spelling of the same word must not match.
        assert!(
            index
                .lookup("zhongguo", LookupOptions::default())
                .is_empty()
        );
    }

    #[test]
    fn distinct_spellings_of_one_word_are_merged() {
        let index = index("pinyin", "中国\tzhong guo\t1\n中国\tzhongguo\t2\n");
        let found = index.lookup("zhongguo", LookupOptions::default());
        assert_eq!(texts(&found), vec!["中国"]);
        assert_eq!(found[0].weight, 2);
    }

    #[test]
    fn ordering_is_total_and_stable() {
        let index = index("pinyin", SAMPLE);
        let a = index.lookup("z", LookupOptions::default());
        let b = index.lookup("z", LookupOptions::default());
        assert_eq!(texts(&a), texts(&b));
    }
}
