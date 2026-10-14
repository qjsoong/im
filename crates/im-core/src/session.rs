//! One composition: the keystrokes typed so far, and what they could mean.
//!
//! A [`Session`] is deliberately dumb and synchronous. Front ends push keys in,
//! read the candidate list out, and call [`Session::commit`] when the user
//! picks one. Because committing only clears the part of the input the
//! candidate accounted for, the same type drives a one-shot "pick a word"
//! prompt and an incremental desktop input method.
//!
//! ```
//! use im_core::{Dictionary, Origin, Session, schema};
//!
//! let schema = schema::builtin::get("double_pinyin")?;
//! let dict = Dictionary::parse("中国\tzhong guo\t700\n", Origin::System)?;
//! let index = schema.index([dict])?;
//! let mut session = Session::new(&schema, &index);
//!
//! assert!(!session.push('!'));            // not a code character
//! for c in "vsgo".chars() {
//!     session.push(c);
//! }
//! let committed = session.commit(0).expect("a candidate");
//! assert_eq!(committed.text, "中国");
//! assert_eq!(committed.rest, "");         // nothing left to type
//! # Ok::<(), im_core::Error>(())
//! ```

use crate::index::{Candidate, CodeIndex, LookupOptions};
use crate::schema::Schema;

/// What a front end should emit after the user picked a candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    /// Text to insert.
    pub text: String,
    /// Spelling of the underlying dictionary entry, useful for history and for
    /// learning user preferences.
    pub spelling: String,
    /// Input that was *not* accounted for and is now the new input. A front end
    /// usually just keeps going with the session; it is exposed so that a
    /// one-shot prompt can report what it dropped.
    pub rest: String,
}

/// The composition state machine.
#[derive(Debug, Clone)]
pub struct Session<'a> {
    schema: &'a Schema,
    index: &'a CodeIndex,
    input: String,
    candidates: Vec<Candidate>,
    highlight: usize,
    page_size: usize,
    options: LookupOptions,
}

impl<'a> Session<'a> {
    /// Start an empty composition with the default page size and limits.
    pub fn new(schema: &'a Schema, index: &'a CodeIndex) -> Self {
        Self::with_options(schema, index, 9, LookupOptions::default())
    }

    /// Start an empty composition with explicit settings.
    pub fn with_options(
        schema: &'a Schema,
        index: &'a CodeIndex,
        page_size: usize,
        options: LookupOptions,
    ) -> Self {
        Self {
            schema,
            index,
            input: String::new(),
            candidates: Vec::new(),
            highlight: 0,
            page_size: page_size.max(1),
            options,
        }
    }

    /// The schema driving this composition.
    pub fn schema(&self) -> &'a Schema {
        self.schema
    }

    /// The index being searched.
    pub fn index(&self) -> &'a CodeIndex {
        self.index
    }

    /// The raw input typed so far.
    pub fn input(&self) -> &str {
        &self.input
    }

    /// Candidates for the current input, best first.
    pub fn candidates(&self) -> &[Candidate] {
        &self.candidates
    }

    /// Index of the highlighted candidate within [`Session::candidates`].
    pub fn highlight(&self) -> usize {
        self.highlight
    }

    /// The highlighted candidate, if there is one.
    pub fn highlighted(&self) -> Option<&Candidate> {
        self.candidates.get(self.highlight)
    }

    /// Candidates per page.
    pub fn page_size(&self) -> usize {
        self.page_size
    }

    /// Change the page size; the highlight stays on the same candidate.
    pub fn set_page_size(&mut self, page_size: usize) {
        self.page_size = page_size.max(1);
    }

    /// Current lookup options.
    pub fn options(&self) -> LookupOptions {
        self.options
    }

    /// Replace the lookup options and recompute candidates.
    pub fn set_options(&mut self, options: LookupOptions) {
        self.options = options;
        self.refresh();
    }

    /// True when nothing has been typed.
    pub fn is_idle(&self) -> bool {
        self.input.is_empty()
    }

    /// True when at least one candidate is available.
    pub fn has_candidates(&self) -> bool {
        !self.candidates.is_empty()
    }

    /// Can this character be part of a code?
    ///
    /// Front ends use this to decide whether a keystroke belongs to the
    /// composition or to the application underneath.
    pub fn accepts(ch: char) -> bool {
        ch.is_ascii_alphanumeric()
    }

    /// Append a keystroke. Returns `false` when the character is not part of
    /// the code alphabet, in which case nothing changed.
    pub fn push(&mut self, ch: char) -> bool {
        if !Self::accepts(ch) {
            return false;
        }
        self.input.push(ch.to_ascii_lowercase());
        self.refresh();
        true
    }

    /// Append a whole string, skipping characters that
    /// [`Session::accepts`] rejects. Returns how many were accepted.
    pub fn push_str(&mut self, text: &str) -> usize {
        let mut accepted = 0;
        for ch in text.chars() {
            if self.push(ch) {
                accepted += 1;
            }
        }
        accepted
    }

    /// Delete the last keystroke. Returns `false` when the input was empty.
    pub fn backspace(&mut self) -> bool {
        if self.input.pop().is_none() {
            return false;
        }
        self.refresh();
        true
    }

    /// Throw away the input.
    pub fn clear(&mut self) {
        self.input.clear();
        self.refresh();
    }

    /// Replace the input wholesale; used by front ends that edit the buffer
    /// directly (for example a line editor in the TUI).
    pub fn set_input(&mut self, input: &str) {
        self.input.clear();
        self.push_str(input);
    }

    /// Position of the current page, zero-based.
    pub fn page(&self) -> usize {
        self.highlight / self.page_size
    }

    /// Number of pages; always at least 1.
    pub fn page_count(&self) -> usize {
        self.candidates.len().div_ceil(self.page_size).max(1)
    }

    /// Half-open bounds of the current page within
    /// [`Session::candidates`].
    pub fn page_bounds(&self) -> (usize, usize) {
        let start = self.page() * self.page_size;
        let end = (start + self.page_size).min(self.candidates.len());
        (start.min(self.candidates.len()), end)
    }

    /// Candidates on the current page.
    pub fn page_candidates(&self) -> &[Candidate] {
        let (start, end) = self.page_bounds();
        &self.candidates[start..end]
    }

    /// Position of the highlight within the current page.
    pub fn page_selection(&self) -> usize {
        self.highlight.saturating_sub(self.page() * self.page_size)
    }

    /// Move the highlight by `delta`, wrapping around the whole candidate list.
    pub fn move_highlight(&mut self, delta: isize) {
        let len = self.candidates.len();
        if len == 0 {
            self.highlight = 0;
            return;
        }
        let len = len as isize;
        let current = self.highlight.min(len as usize - 1) as isize;
        self.highlight = (current + delta).rem_euclid(len) as usize;
    }

    /// Highlight the next candidate.
    pub fn next_candidate(&mut self) {
        self.move_highlight(1);
    }

    /// Highlight the previous candidate.
    pub fn prev_candidate(&mut self) {
        self.move_highlight(-1);
    }

    /// Move to the next page, wrapping around.
    pub fn next_page(&mut self) {
        self.move_page(1);
    }

    /// Move to the previous page, wrapping around.
    pub fn prev_page(&mut self) {
        self.move_page(-1);
    }

    fn move_page(&mut self, delta: isize) {
        let count = self.page_count() as isize;
        let page = (self.page() as isize + delta).rem_euclid(count) as usize;
        let offset = self.highlight - self.page() * self.page_size;
        self.highlight = (page * self.page_size + offset)
            .min(self.candidates.len().saturating_sub(1));
    }

    /// Highlight the candidate at `index` within the candidate list.
    pub fn set_highlight(&mut self, index: usize) {
        if index < self.candidates.len() {
            self.highlight = index;
        }
    }

    /// Highlight the candidate at `index` within the current page, clamped.
    pub fn set_page_selection(&mut self, index: usize) {
        let (start, end) = self.page_bounds();
        if start >= end {
            return;
        }
        let target = start + index;
        if target < end {
            self.highlight = target;
        }
    }

    /// Commit the candidate at `index` in [`Session::candidates`].
    ///
    /// The part of the input the candidate did not account for stays in the
    /// buffer and is re-searched, so committing 中 from `zhongguo` leaves `guo`.
    pub fn commit(&mut self, index: usize) -> Option<Commit> {
        let candidate = self.candidates.get(index)?.clone();
        let consumed = candidate.consumed.min(self.input.len());
        let rest = self.input[consumed..].to_owned();
        self.input = rest.clone();
        self.highlight = 0;
        self.refresh();
        Some(Commit {
            text: candidate.text,
            spelling: candidate.spelling,
            rest,
        })
    }

    /// Commit the highlighted candidate.
    pub fn commit_highlighted(&mut self) -> Option<Commit> {
        self.commit(self.highlight)
    }

    /// Take the input as it was typed and reset.
    ///
    /// Returns `None` when nothing was typed.
    pub fn commit_raw(&mut self) -> Option<String> {
        if self.input.is_empty() {
            return None;
        }
        let raw = std::mem::take(&mut self.input);
        self.highlight = 0;
        self.refresh();
        Some(raw)
    }

    /// Recompute the candidate list for the current input.
    pub fn refresh(&mut self) {
        self.candidates = if self.input.is_empty() {
            Vec::new()
        } else {
            self.index.lookup(&self.input, self.options)
        };
        if self.highlight >= self.candidates.len() {
            self.highlight = self.candidates.len().saturating_sub(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dict::{Dictionary, Origin};
    use crate::schema::builtin;

    const SAMPLE: &str = "\
中\tzhong\t900\n\
国\tguo\t800\n\
中国\tzhong guo\t700\n\
中心\tzhong xin\t600\n\
种\tzhong\t500\n\
说\tshuo\t200\n";

    fn session() -> (Schema, CodeIndex) {
        let schema = builtin::get("double_pinyin").unwrap_or_else(|e| panic!("{e}"));
        let dict =
            Dictionary::parse(SAMPLE, Origin::System).unwrap_or_else(|e| panic!("{e}"));
        let index = schema.index([dict]).unwrap_or_else(|e| panic!("{e}"));
        (schema, index)
    }

    fn texts(session: &Session<'_>) -> Vec<String> {
        session
            .candidates()
            .iter()
            .map(|c| c.text.clone())
            .collect()
    }

    #[test]
    fn pushing_keys_narrows_the_candidates() {
        let (schema, index) = session();
        let mut session = Session::new(&schema, &index);
        assert!(session.is_idle());
        assert!(!session.push('!'));
        assert!(session.push('v'));
        assert!(session.push('s'));
        assert_eq!(session.input(), "vs");
        assert!(texts(&session).contains(&"中".to_owned()));
    }

    #[test]
    fn backspace_and_clear_reset_the_search() {
        let (schema, index) = session();
        let mut session = Session::new(&schema, &index);
        session.push_str("vsgo");
        assert!(!session.is_idle());
        assert!(session.backspace());
        assert_eq!(session.input(), "vsg");
        session.clear();
        assert!(session.is_idle());
        assert!(session.candidates().is_empty());
        assert!(!session.backspace());
    }

    #[test]
    fn committing_keeps_the_unconsumed_input() {
        let (schema, index) = session();
        let mut session = Session::new(&schema, &index);
        session.push_str("vsgo");
        let first = session.commit(0).unwrap_or_else(|| panic!("no candidate"));
        assert_eq!(first.text, "中国");
        assert_eq!(first.rest, "");
        assert!(session.is_idle());
    }

    #[test]
    fn committing_a_partial_match_leaves_the_rest() {
        let (schema, index) = session();
        let mut session = Session::new(&schema, &index);
        // "vs" is 中, "go" is 国; the completion 中国 would swallow both.
        session.push_str("vsgo");
        let index_of_zhong = session
            .candidates()
            .iter()
            .position(|c| c.text == "中")
            .unwrap_or_else(|| panic!("中 not offered"));
        let commit = session
            .commit(index_of_zhong)
            .unwrap_or_else(|| panic!("no candidate"));
        assert_eq!(commit.text, "中");
        assert_eq!(commit.rest, "go");
        assert_eq!(session.input(), "go");
        assert_eq!(texts(&session)[0], "国");
    }

    #[test]
    fn commit_raw_returns_the_typed_keys() {
        let (schema, index) = session();
        let mut session = Session::new(&schema, &index);
        session.push_str("hello");
        assert_eq!(session.commit_raw().as_deref(), Some("hello"));
        assert!(session.is_idle());
        assert_eq!(session.commit_raw(), None);
    }

    #[test]
    fn paging_and_highlight_wrap() {
        let (schema, index) = session();
        let mut session = Session::new(&schema, &index);
        session.set_page_size(2);
        session.push_str("vs");
        let count = session.candidates().len();
        assert!(count >= 3, "need at least three candidates, got {count}");
        assert_eq!(session.page(), 0);
        assert_eq!(session.page_selection(), 0);

        session.next_candidate();
        assert_eq!(session.page_selection(), 1);
        session.next_candidate();
        assert_eq!(session.page(), 1, "stepping past the page moves on");
        assert_eq!(session.page_selection(), 0);

        session.prev_candidate();
        assert_eq!(session.page(), 0);
        assert_eq!(session.page_selection(), 1);

        session.next_page();
        assert_eq!(session.page(), 1);
        session.next_page();
        assert_eq!(session.page(), 0, "paging wraps");
    }

    #[test]
    fn page_selection_is_clamped_to_the_page() {
        let (schema, index) = session();
        let mut session = Session::new(&schema, &index);
        session.set_page_size(2);
        session.push_str("vs");
        session.set_page_selection(1);
        assert_eq!(session.page_selection(), 1);
        session.set_page_selection(9);
        assert_eq!(session.page_selection(), 1, "out of range is ignored");
    }

    #[test]
    fn page_size_is_at_least_one() {
        let (schema, index) = session();
        let mut session = Session::with_options(
            &schema,
            &index,
            0,
            LookupOptions {
                limit: 100,
                completion: false,
            },
        );
        assert_eq!(session.page_size(), 1);
        session.push_str("vs");
        assert!(!session.page_candidates().is_empty());
    }
}
