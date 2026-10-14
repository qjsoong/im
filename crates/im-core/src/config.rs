//! The `im.toml` configuration file.
//!
//! One file, one language, no GUI. Defaults are documented in the file itself
//! and are layered: the built-in defaults are merged with `/etc/xdg/im/im.toml`
//! and then with `~/.config/im/im.toml`, key by key, so a user file only has to
//! mention what it changes:
//!
//! ```toml
//! schema = "double_pinyin"
//!
//! [keys]
//! next_candidate = ["Tab", "C-n", "Down"]
//! ```
//!
//! `im config show` prints the merged result, which makes it easy to see where
//! a value is coming from.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

use crate::error::{Error, Result};
use crate::keymap::Keymap;
use crate::paths::Paths;
use crate::schema::Schema;

/// The built-in configuration, also used as documentation.
pub const DEFAULT_TOML: &str = include_str!("../data/im.toml");

/// Everything `im.toml` can say.
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Input scheme used when a front end does not ask for a specific one.
    pub schema: String,
    /// Candidates shown per page.
    pub page_size: usize,
    /// Upper bound on candidates kept for one input.
    pub max_candidates: usize,
    /// Whether to offer completions of unfinished codes.
    pub completion: bool,
    /// Keystroke bindings, keyed by action name.
    pub keys: BTreeMap<String, Vec<String>>,

    /// Parsed `keys`; built by [`Config::finish`].
    #[serde(skip)]
    keymap: Keymap,
}

impl Default for Config {
    fn default() -> Self {
        match Self::parse(DEFAULT_TOML, "<built-in>") {
            Ok(config) => config,
            // The built-in file is covered by tests; failing here would be a
            // bug in `im-core`, and a silent fallback is still better than a
            // panic inside a daemon.
            Err(_) => Self {
                schema: "pinyin".to_owned(),
                page_size: 9,
                max_candidates: 100,
                completion: true,
                keys: BTreeMap::new(),
                keymap: Keymap::default(),
            },
        }
    }
}

impl Config {
    /// Parse a configuration from TOML text.
    pub fn parse(text: &str, origin: &str) -> Result<Self> {
        let mut config: Config = toml::from_str(text).map_err(|e| Error::Config {
            path: origin.to_owned(),
            source: Box::new(e),
        })?;
        config.finish(origin)?;
        Ok(config)
    }

    /// Load the layered configuration for `paths`.
    ///
    /// Returns the merged configuration and the files that contributed to it,
    /// lowest precedence first.
    pub fn load(paths: &Paths) -> Result<(Self, Vec<std::path::PathBuf>)> {
        let mut value: toml::Table =
            toml::from_str(DEFAULT_TOML).map_err(|e| Error::Config {
                path: "<built-in>".to_owned(),
                source: Box::new(e),
            })?;
        let files = paths.config_files();
        for file in &files {
            let text = std::fs::read_to_string(file)
                .map_err(|e| Error::io(file.display(), e))?;
            let overlay: toml::Table =
                toml::from_str(&text).map_err(|e| Error::Config {
                    path: file.display().to_string(),
                    source: Box::new(e),
                })?;
            merge(&mut value, overlay);
        }
        let origin = files
            .last()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "<built-in>".to_owned());
        let mut config: Config =
            toml::Value::Table(value)
                .try_into()
                .map_err(|e| Error::Config {
                    path: origin.clone(),
                    source: Box::new(e),
                })?;
        config.finish(&origin)?;
        Ok((config, files))
    }

    /// Load the configuration, or the defaults when no file exists.
    pub fn load_or_default(paths: &Paths) -> Self {
        match Self::load(paths) {
            Ok((config, _)) => config,
            Err(_) => Self::default(),
        }
    }

    /// Validate values and build the keymap.
    pub fn finish(&mut self, origin: &str) -> Result<()> {
        if self.page_size == 0 {
            return Err(Error::config_value(origin, "page_size must be at least 1"));
        }
        if self.max_candidates == 0 {
            return Err(Error::config_value(
                origin,
                "max_candidates must be at least 1",
            ));
        }
        self.keymap = Keymap::from_table(&self.keys, origin)?;
        Ok(())
    }

    /// The parsed keystroke bindings.
    pub fn keymap(&self) -> &Keymap {
        &self.keymap
    }

    /// Candidates per page, letting a schema override the global value.
    pub fn page_size_for(&self, schema: &Schema) -> usize {
        schema.page_size.unwrap_or(self.page_size)
    }

    /// Lookup options derived from this configuration.
    pub fn lookup_options(&self) -> crate::index::LookupOptions {
        crate::index::LookupOptions {
            limit: self.max_candidates,
            completion: self.completion,
        }
    }

    /// Render the effective configuration as TOML.
    pub fn to_toml(&self) -> String {
        toml::to_string_pretty(self).unwrap_or_else(|_| DEFAULT_TOML.to_owned())
    }
}

/// Merge `overlay` into `base`, replacing scalars and arrays but recursing into
/// tables so that a user file can override one key of `[keys]` without having
/// to repeat the others.
fn merge(base: &mut toml::Table, overlay: toml::Table) {
    for (key, value) in overlay {
        match base.get_mut(&key) {
            Some(toml::Value::Table(existing)) => match value {
                toml::Value::Table(overlay) => merge(existing, overlay),
                other => {
                    base.insert(key, other);
                }
            },
            _ => {
                base.insert(key, value);
            }
        }
    }
}

/// Read a configuration file, if it exists.
pub fn read_file(path: &Path) -> Result<Option<Config>> {
    match std::fs::read_to_string(path) {
        Ok(text) => Config::parse(&text, &path.display().to_string()).map(Some),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(Error::io(path.display(), e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_defaults_are_valid() {
        let config = Config::default();
        assert_eq!(config.schema, "pinyin");
        assert!(!config.keymap().is_empty());
    }

    #[test]
    fn defaults_bind_the_documented_keys() {
        use crate::key::KeyEvent;
        use crate::keymap::Action;
        let config = Config::default();
        let keymap = config.keymap();
        assert_eq!(
            keymap.action(&KeyEvent::parse("Enter").unwrap_or(KeyEvent::ch(' '))),
            Some(Action::Commit)
        );
        assert_eq!(keymap.action(&KeyEvent::ch('3')), Some(Action::Select(2)));
    }

    #[test]
    fn unknown_keys_are_rejected() {
        assert!(Config::parse("nope = 1", "test").is_err());
        assert!(
            Config::parse("schema = \"x\"\n[keys]\nnope = [\"a\"]", "test").is_err()
        );
    }

    #[test]
    fn merges_tables_key_by_key() {
        let mut base: toml::Table = toml::from_str(
            "schema = \"pinyin\"\n[keys]\ncommit = [\"Enter\"]\nselect = [\"1\"]\n",
        )
        .unwrap_or_else(|e| panic!("{e}"));
        let overlay: toml::Table = toml::from_str("[keys]\ncommit = [\"C-j\"]\n")
            .unwrap_or_else(|e| panic!("{e}"));
        merge(&mut base, overlay);
        let text = toml::Value::Table(base).to_string();
        assert!(text.contains("C-j"), "{text}");
        assert!(!text.contains("Enter"), "{text}");
        assert!(text.contains('1'), "untouched keys survive: {text}");
    }

    #[test]
    fn schema_page_size_wins() {
        let config = Config::default();
        let schema =
            crate::schema::builtin::get("pinyin").unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(config.page_size_for(&schema), config.page_size);

        let overriding = crate::schema::Schema::parse(
            "id = \"x\"\nname = \"X\"\npage_size = 3\ndictionaries = []\n",
        )
        .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(config.page_size_for(&overriding), 3);
    }

    #[test]
    fn round_trips_through_toml() {
        let config = Config::default();
        let text = config.to_toml();
        let reparsed =
            Config::parse(&text, "round-trip").unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(reparsed.schema, config.schema);
        assert_eq!(reparsed.keys, config.keys);
    }
}
