//! Turning pinyin spellings into keystroke codes.
//!
//! A [`Speller`] is everything that distinguishes full pinyin from a
//! double-pinyin scheme. It folds a spelling such as `zhong guo` into the exact
//! key sequence a user has to type.
//!
//! * **Full pinyin** has no tables at all: the code is the spelling with the
//!   syllable separators removed, so `zhong guo` becomes `zhongguo`.
//! * **Double pinyin** replaces the initial and the final of each syllable with
//!   one key each: `zhong` becomes `vs` in 自然码 (`zh`→`v`, `ong`→`s`).
//!
//! `ü` is always written `v` in dictionary data, and after `j q x y` a written
//! `u` is folded to `v` first, so `ju`, `jue`, `juan` and `jun` fold as if they
//! were spelled `jv`, `jve`, `jvan` and `jvn`.

use std::collections::BTreeMap;

use serde::Deserialize;

/// The initial/final replacement tables of one input scheme.
///
/// Deserialized from the `[speller]` table of a schema file, for example:
///
/// ```toml
/// [speller]
/// initials = ["b", "p", "m", "zh", "ch", "sh"]
///
/// [speller.initial_keys]
/// zh = "v"
/// ch = "i"
/// sh = "u"
///
/// [speller.finals]
/// ong = "s"
///
/// [speller.null_initials]
/// a = "aa"
/// ```
#[derive(Debug, Clone, Default, Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct Speller {
    /// Pinyin initials, matched longest-first. When empty the speller is the
    /// identity and the code is the spelling without separators.
    #[serde(default)]
    initials: Vec<String>,
    /// Keys for initials that differ from their pinyin spelling.
    #[serde(default)]
    initial_keys: BTreeMap<String, String>,
    /// Final of a syllable to the key that produces it.
    #[serde(default)]
    finals: BTreeMap<String, String>,
    /// Whole-syllable keys for syllables that have no initial, for example
    /// `an = "aj"` in 自然码.
    #[serde(default)]
    null_initials: BTreeMap<String, String>,
}

impl Speller {
    /// The identity speller used by full pinyin: no folding at all.
    pub fn identity() -> Self {
        Self::default()
    }

    /// True when this speller does not rewrite anything.
    pub fn is_identity(&self) -> bool {
        self.initials.is_empty() && self.finals.is_empty()
    }

    /// Pinyin initials, longest first, as configured.
    pub fn initials(&self) -> &[String] {
        &self.initials
    }

    /// Fold one spelling into a code.
    ///
    /// Syllables that the tables do not cover are passed through unchanged, so
    /// a dictionary may contain entries for a scheme that does not know about
    /// them without producing a hard error.
    pub fn fold(&self, spelling: &str) -> String {
        let mut out = String::with_capacity(spelling.len());
        for syllable in spelling.split_whitespace() {
            if self.is_identity() {
                out.push_str(syllable);
            } else {
                self.fold_syllable_into(syllable, &mut out);
            }
        }
        out
    }

    /// Fold a single syllable, for tests and for rendering previews.
    pub fn fold_syllable(&self, syllable: &str) -> String {
        let mut out = String::with_capacity(syllable.len());
        if self.is_identity() {
            out.push_str(syllable);
        } else {
            self.fold_syllable_into(syllable, &mut out);
        }
        out
    }

    fn fold_syllable_into(&self, syllable: &str, out: &mut String) {
        let Some(initial) = self.longest_initial(syllable) else {
            let whole = self.whole_syllable(syllable);
            out.push_str(&whole);
            return;
        };
        let rest = &syllable[initial.len()..];
        let rest = normalize_umlaut(initial, rest);
        out.push_str(&self.initial_key(initial));
        match self.finals.get(rest.as_ref()) {
            Some(key) => out.push_str(key),
            // Unknown finals degrade gracefully to their own spelling.
            None => out.push_str(&rest),
        }
    }

    /// Longest configured initial that is a *proper* prefix of `syllable`.
    ///
    /// Requiring a proper prefix means `an` is treated as a whole syllable
    /// rather than as the initial `a` plus a missing final. Taking the longest
    /// match rather than the first makes the result independent of the order
    /// the initials happen to be listed in a schema file.
    fn longest_initial<'a>(&'a self, syllable: &str) -> Option<&'a str> {
        self.initials
            .iter()
            .map(String::as_str)
            .filter(|init| syllable.len() > init.len() && syllable.starts_with(init))
            .max_by_key(|init| init.len())
    }

    fn initial_key(&self, initial: &str) -> String {
        self.initial_keys
            .get(initial)
            .cloned()
            .unwrap_or_else(|| initial.to_owned())
    }

    fn whole_syllable(&self, syllable: &str) -> String {
        if let Some(key) = self.null_initials.get(syllable) {
            return key.clone();
        }
        let folded = normalize_umlaut("", syllable);
        if let Some(key) = self.null_initials.get(folded.as_ref()) {
            return key.clone();
        }
        syllable.to_owned()
    }
}

/// After `j q x y` a written `u` is really `ü`.
fn normalize_umlaut<'a>(initial: &str, rest: &'a str) -> std::borrow::Cow<'a, str> {
    if matches!(initial, "j" | "q" | "x" | "y") && rest.starts_with('u') {
        let mut owned = String::with_capacity(rest.len());
        owned.push('v');
        owned.push_str(&rest[1..]);
        return std::borrow::Cow::Owned(owned);
    }
    std::borrow::Cow::Borrowed(rest)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn natural_code() -> Speller {
        let source = include_str!("../data/schemas/double_pinyin.toml");
        let file =
            crate::schema::Schema::parse(source).unwrap_or_else(|e| panic!("{e}"));
        file.speller
    }

    #[test]
    fn identity_only_strips_separators() {
        let speller = Speller::identity();
        assert_eq!(speller.fold("zhong guo"), "zhongguo");
        assert_eq!(speller.fold("xi an"), "xian");
        assert_eq!(speller.fold("ni"), "ni");
    }

    #[test]
    fn natural_code_folds_initials() {
        let speller = natural_code();
        assert_eq!(speller.fold_syllable("zhong"), "vs");
        assert_eq!(speller.fold_syllable("chi"), "ii");
        assert_eq!(speller.fold_syllable("shi"), "ui");
        assert_eq!(speller.fold_syllable("ri"), "ri");
        assert_eq!(speller.fold_syllable("zhi"), "vi");
    }

    #[test]
    fn natural_code_folds_finals() {
        let speller = natural_code();
        let cases = [
            ("ba", "ba"),
            ("bang", "bh"),
            ("bai", "bl"),
            ("bei", "bz"),
            ("bao", "bk"),
            ("ben", "bf"),
            ("beng", "bg"),
            ("bie", "bx"),
            ("biao", "bc"),
            ("bian", "bm"),
            ("bin", "bn"),
            ("bing", "by"),
            ("biu", "bq"),
            ("bo", "bo"),
            ("bu", "bu"),
            ("bong", "bs"),
            ("duo", "do"),
            ("dui", "dv"),
            ("duan", "dr"),
            ("dun", "dp"),
            ("diao", "dc"),
            ("diu", "dq"),
            ("dong", "ds"),
        ];
        for (syllable, expected) in cases {
            assert_eq!(speller.fold_syllable(syllable), expected, "{syllable}");
        }
    }

    #[test]
    fn natural_code_handles_umlaut() {
        let speller = natural_code();
        let cases = [
            ("ju", "jv"),
            ("jue", "jt"),
            ("juan", "jr"),
            ("jun", "jp"),
            ("qu", "qv"),
            ("xu", "xv"),
            ("yu", "yv"),
            ("yue", "yt"),
            ("yuan", "yr"),
            ("yun", "yp"),
            ("nv", "nv"),
            ("lve", "lt"),
        ];
        for (syllable, expected) in cases {
            assert_eq!(speller.fold_syllable(syllable), expected, "{syllable}");
        }
    }

    #[test]
    fn natural_code_handles_whole_syllables() {
        let speller = natural_code();
        let cases = [
            ("a", "aa"),
            ("o", "oo"),
            ("e", "ee"),
            ("er", "er"),
            ("ai", "al"),
            ("ei", "ez"),
            ("ao", "ak"),
            ("ou", "ob"),
            ("an", "aj"),
            ("en", "ef"),
            ("ang", "ah"),
            ("eng", "eg"),
        ];
        for (syllable, expected) in cases {
            assert_eq!(speller.fold_syllable(syllable), expected, "{syllable}");
        }
    }

    #[test]
    fn natural_code_handles_y_and_w_initials() {
        let speller = natural_code();
        let cases = [
            ("yi", "yi"),
            ("ya", "ya"),
            ("ye", "ye"),
            ("yao", "yk"),
            ("you", "yb"),
            ("yan", "yj"),
            ("yin", "yn"),
            ("yang", "yh"),
            ("ying", "yy"),
            ("yong", "ys"),
            ("wu", "wu"),
            ("wa", "wa"),
            ("wo", "wo"),
            ("wai", "wl"),
            ("wei", "wz"),
            ("wan", "wj"),
            ("wen", "wf"),
            ("wang", "wh"),
            ("weng", "wg"),
        ];
        for (syllable, expected) in cases {
            assert_eq!(speller.fold_syllable(syllable), expected, "{syllable}");
        }
    }

    #[test]
    fn unknown_syllables_pass_through() {
        let speller = natural_code();
        assert_eq!(speller.fold_syllable("n"), "n");
        assert_eq!(speller.fold_syllable("hm"), "hm");
        assert_eq!(speller.fold("n hm"), "nhm");
    }
}
