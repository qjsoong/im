//! Mapping keystrokes to editor actions.
//!
//! The keymap is shared by every front end, so rebinding `Page_Down` to `C-f`
//! works the same in the terminal UI and in the X11 daemon. It is read from the
//! `[keys]` table of `im.toml`:
//!
//! ```toml
//! [keys]
//! commit = ["Enter", "space"]
//! cancel = ["Escape", "C-c"]
//! select = ["1", "2", "3", "4", "5"]
//! ```
//!
//! Every binding listed for an action *replaces* the built-in bindings for that
//! action; there is no implicit "and the defaults too".

use std::collections::BTreeMap;

use crate::error::{Error, Result};
use crate::key::KeyEvent;

/// What a keystroke does inside a composition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Action {
    /// Commit the highlighted candidate; with an empty input, finish the
    /// composition and accept what has been committed so far.
    Commit,
    /// Finish the composition, emitting the raw input as it was typed. This is
    /// the escape hatch for typing ASCII without leaving the input method.
    CommitRaw,
    /// Abandon the composition. Nothing is emitted.
    Cancel,
    /// Throw away the input, keeping candidates already committed.
    Clear,
    /// Delete one input character.
    Backspace,
    /// Move the highlight down one candidate.
    NextCandidate,
    /// Move the highlight up one candidate.
    PrevCandidate,
    /// Move to the next page of candidates.
    NextPage,
    /// Move to the previous page of candidates.
    PrevPage,
    /// Commit the candidate at this position on the current page.
    Select(usize),
}

impl Action {
    /// Action names accepted in `[keys]`, in the order they are documented.
    pub const NAMES: &'static [&'static str] = &[
        "commit",
        "commit_raw",
        "cancel",
        "clear",
        "backspace",
        "next_candidate",
        "prev_candidate",
        "next_page",
        "prev_page",
        "select",
    ];

    /// The configuration name of this action, without the `select` index.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Commit => "commit",
            Self::CommitRaw => "commit_raw",
            Self::Cancel => "cancel",
            Self::Clear => "clear",
            Self::Backspace => "backspace",
            Self::NextCandidate => "next_candidate",
            Self::PrevCandidate => "prev_candidate",
            Self::NextPage => "next_page",
            Self::PrevPage => "prev_page",
            Self::Select(_) => "select",
        }
    }
}

/// Keystrokes to actions.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Keymap {
    bindings: Vec<(KeyEvent, Action)>,
}

impl Keymap {
    /// Build a keymap from the `[keys]` table.
    ///
    /// `origin` names the source (usually the config file path) in errors.
    ///
    /// Binding one keystroke to two actions is rejected rather than silently
    /// resolved: in a configuration file it is always a mistake, and the error
    /// message is the fastest way to find it.
    pub fn from_table(
        table: &BTreeMap<String, Vec<String>>,
        origin: &str,
    ) -> Result<Self> {
        let mut bindings: Vec<(KeyEvent, Action)> = Vec::new();
        for (name, entries) in table {
            for (position, entry) in entries.iter().enumerate() {
                let action =
                    action_for(name, position).ok_or_else(|| Error::KeyBinding {
                        origin: origin.to_owned(),
                        binding: name.clone(),
                        message: format!(
                            "unknown action; expected one of {}",
                            Action::NAMES.join(", ")
                        ),
                    })?;
                let event = KeyEvent::parse(entry).map_err(|e| Error::KeyBinding {
                    origin: origin.to_owned(),
                    binding: entry.clone(),
                    message: e.message,
                })?;
                if let Some((_, previous)) =
                    bindings.iter().find(|(bound, _)| *bound == event)
                {
                    return Err(Error::KeyBinding {
                        origin: origin.to_owned(),
                        binding: entry.clone(),
                        message: format!(
                            "also bound to {}; each keystroke may only do one thing",
                            previous.name()
                        ),
                    });
                }
                bindings.push((event, action));
            }
        }
        Ok(Self { bindings })
    }

    /// The action bound to a keystroke, if any.
    pub fn action(&self, event: &KeyEvent) -> Option<Action> {
        self.bindings
            .iter()
            .find(|(bound, _)| bound == event)
            .map(|(_, action)| *action)
    }

    /// Every binding, in declaration order.
    pub fn bindings(&self) -> &[(KeyEvent, Action)] {
        &self.bindings
    }

    /// True when nothing is bound.
    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }
}

/// Resolve a configuration action name plus its position within the list.
///
/// Only `select` uses the position, turning the n-th binding into
/// `Action::Select(n)`.
fn action_for(name: &str, position: usize) -> Option<Action> {
    Some(match name {
        "select" => Action::Select(position),
        "commit" => Action::Commit,
        "commit_raw" => Action::CommitRaw,
        "cancel" => Action::Cancel,
        "clear" => Action::Clear,
        "backspace" => Action::Backspace,
        "next_candidate" => Action::NextCandidate,
        "prev_candidate" => Action::PrevCandidate,
        "next_page" => Action::NextPage,
        "prev_page" => Action::PrevPage,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(pairs: &[(&str, &[&str])]) -> BTreeMap<String, Vec<String>> {
        pairs
            .iter()
            .map(|(name, entries)| {
                (
                    (*name).to_owned(),
                    entries.iter().map(|e| (*e).to_owned()).collect(),
                )
            })
            .collect()
    }

    #[test]
    fn maps_actions_positionally_for_select() {
        let keymap = Keymap::from_table(&table(&[("select", &["1", "2"])]), "test")
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(keymap.action(&KeyEvent::ch('1')), Some(Action::Select(0)));
        assert_eq!(keymap.action(&KeyEvent::ch('2')), Some(Action::Select(1)));
        assert_eq!(keymap.action(&KeyEvent::ch('3')), None);
    }

    #[test]
    fn rejects_unknown_actions_and_bindings() {
        assert!(Keymap::from_table(&table(&[("nope", &["a"])]), "test").is_err());
        assert!(
            Keymap::from_table(&table(&[("commit", &["nonsense-key"])]), "test")
                .is_err()
        );
    }

    #[test]
    fn rejects_one_key_bound_to_two_actions() {
        let err = Keymap::from_table(
            &table(&[("commit", &["x"]), ("cancel", &["x"])]),
            "test",
        );
        let Err(Error::KeyBinding { message, .. }) = err else {
            panic!("expected a binding error, got {err:?}");
        };
        assert!(message.contains("also bound to"), "{message}");
    }
}
