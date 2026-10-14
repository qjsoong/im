//! Input schemes.
//!
//! A schema is a small TOML file that says *how keystrokes become codes* and
//! *which dictionaries to use*. Everything that makes 全拼 different from
//! 自然码 lives here; nothing in the engine is scheme-specific.
//!
//! ```toml
//! id = "double_pinyin"
//! name = "自然码双拼"
//! kind = "syllable"
//! dictionaries = ["pinyin"]
//!
//! [speller]
//! initials = ["b", "p", "m", "zh", "ch", "sh"]
//!
//! [speller.initial_keys]
//! zh = "v"
//!
//! [speller.finals]
//! ong = "s"
//! ```
//!
//! Schemas are discovered in this order, later entries overriding earlier ones
//! with the same `id`:
//!
//! 1. the ones compiled into `im-core`,
//! 2. `<data dir>/im/schemas/*.toml` — shipped by a distribution,
//! 3. `<config dir>/im/schemas/*.toml` — written by the user.

pub mod builtin;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::dict::Dictionary;
use crate::error::{Error, Result};
use crate::index::CodeIndex;
use crate::paths::Paths;
use crate::speller::Speller;

/// How a schema turns a spelling into a code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SchemaKind {
    /// Pinyin-like: the spelling is a sequence of syllables that the
    /// [`Speller`] folds (full pinyin is the identity case).
    #[default]
    Syllable,
    /// Code-table-like (wubi, cangjie, stroke): the spelling *is* the code and
    /// is typed literally.
    Table,
}

impl SchemaKind {
    /// The string used in schema files.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Syllable => "syllable",
            Self::Table => "table",
        }
    }
}

fn default_dictionaries() -> Vec<String> {
    vec!["pinyin".to_owned()]
}

/// One input scheme.
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct Schema {
    /// Stable identifier, for example `double_pinyin`. This is what
    /// `im.toml` and the command line refer to.
    pub id: String,
    /// Human-readable name shown to users.
    pub name: String,
    /// One or two sentences describing the scheme.
    #[serde(default)]
    pub description: String,
    /// How spellings become codes.
    #[serde(default)]
    pub kind: SchemaKind,
    /// Dictionary names to load, resolved to `<data dir>/im/dict/<name>.tsv`.
    #[serde(default = "default_dictionaries")]
    pub dictionaries: Vec<String>,
    /// Candidates per page, overriding the value from `im.toml` when set.
    #[serde(default)]
    pub page_size: Option<usize>,
    /// Initial/final folding tables.
    #[serde(default)]
    pub speller: Speller,
}

impl Schema {
    /// Parse a schema from TOML text.
    pub fn parse(text: &str) -> Result<Self> {
        let mut schema: Schema = toml::from_str(text).map_err(|e| Error::Schema {
            id: "<unknown>".to_owned(),
            message: e.message().to_owned(),
        })?;
        schema.validate()?;
        Ok(schema)
    }

    /// Read and parse a schema file.
    pub fn read(path: &Path) -> Result<Self> {
        let text =
            std::fs::read_to_string(path).map_err(|e| Error::io(path.display(), e))?;
        Self::parse(&text).map_err(|e| match e {
            Error::Schema { id, message } => Error::Schema {
                id: if id == "<unknown>" {
                    path.display().to_string()
                } else {
                    id
                },
                message,
            },
            other => other,
        })
    }

    fn validate(&mut self) -> Result<()> {
        if self.id.trim().is_empty() {
            return Err(Error::schema("<unknown>", "id must not be empty"));
        }
        if self.name.trim().is_empty() {
            return Err(Error::schema(&self.id, "name must not be empty"));
        }
        if let Some(0) = self.page_size {
            return Err(Error::schema(&self.id, "page_size must be at least 1"));
        }
        Ok(())
    }

    /// Serialize back to TOML; used by `im schema show`.
    pub fn to_toml(&self) -> Result<String> {
        toml::to_string_pretty(self).map_err(|e| Error::Schema {
            id: self.id.clone(),
            message: format!("could not serialize schema: {e}"),
        })
    }
    /// Fold one spelling into a code under this schema.
    pub fn fold(&self, spelling: &str) -> String {
        match self.kind {
            SchemaKind::Syllable => self.speller.fold(spelling),
            SchemaKind::Table => spelling.split_whitespace().collect(),
        }
    }

    /// Build the searchable index for this schema.
    ///
    /// Duplicate `(text, spelling)` pairs across dictionaries are merged, with
    /// the highest weight and origin winning.
    pub fn index(
        &self,
        dictionaries: impl IntoIterator<Item = Dictionary>,
    ) -> Result<CodeIndex> {
        CodeIndex::build(self, Dictionary::merged(dictionaries))
    }
}

/// The schemas available on this machine.
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    schemas: BTreeMap<String, Schema>,
}

impl Catalog {
    /// Discover every schema reachable from `paths`, plus the built-in ones.
    pub fn load(paths: &Paths) -> Result<Self> {
        let mut catalog = Self::default();
        for (id, source) in builtin::SOURCES {
            let schema = Schema::parse(source).map_err(|e| Error::Schema {
                id: (*id).to_owned(),
                message: match e {
                    Error::Schema { message, .. } => message,
                    other => other.to_string(),
                },
            })?;
            catalog.insert(schema);
        }
        // Lowest precedence first so that user files win.
        let mut dirs: Vec<&PathBuf> = Vec::new();
        dirs.extend(paths.data_dirs().iter().rev());
        dirs.extend(paths.config_dirs().iter().rev());
        for dir in dirs {
            catalog.load_dir(&dir.join("schemas"))?;
        }
        Ok(catalog)
    }

    fn load_dir(&mut self, dir: &Path) -> Result<()> {
        let mut files: Vec<PathBuf> = match std::fs::read_dir(dir) {
            Ok(entries) => entries
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|ext| ext == "toml"))
                .collect(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(Error::io(dir.display(), e)),
        };
        files.sort();
        for file in files {
            self.insert(Schema::read(&file)?);
        }
        Ok(())
    }

    /// Add or replace a schema.
    pub fn insert(&mut self, schema: Schema) {
        self.schemas.insert(schema.id.clone(), schema);
    }

    /// Look up a schema by id.
    pub fn get(&self, id: &str) -> Result<&Schema> {
        self.schemas.get(id).ok_or_else(|| Error::NotFound {
            kind: "schema",
            name: id.to_owned(),
            searched: self.schemas.keys().cloned().collect::<Vec<_>>().join(", "),
        })
    }

    /// Every schema, ordered by id.
    pub fn iter(&self) -> impl Iterator<Item = &Schema> {
        self.schemas.values()
    }

    /// Identifiers of every known schema, ordered.
    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.schemas.keys().map(String::as_str)
    }

    /// Number of schemas.
    pub fn len(&self) -> usize {
        self.schemas.len()
    }

    /// True when no schema is available.
    pub fn is_empty(&self) -> bool {
        self.schemas.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_builtin_schemas() {
        for (id, source) in builtin::SOURCES {
            let schema = Schema::parse(source).unwrap_or_else(|e| panic!("{id}: {e}"));
            assert_eq!(&schema.id, id);
            assert!(!schema.name.is_empty());
        }
    }

    #[test]
    fn rejects_unknown_fields() {
        let err = Schema::parse("id = \"x\"\nname = \"X\"\nnope = 1\n");
        assert!(matches!(err, Err(Error::Schema { .. })), "{err:?}");
    }

    #[test]
    fn rejects_empty_page_size() {
        let err = Schema::parse("id = \"x\"\nname = \"X\"\npage_size = 0\n");
        assert!(matches!(err, Err(Error::Schema { .. })), "{err:?}");
    }

    #[test]
    fn table_schemas_use_the_spelling_as_the_code() {
        let schema = Schema::parse(
            "id = \"wubi\"\nname = \"Wubi\"\nkind = \"table\"\ndictionaries = [\"wubi\"]\n",
        );
        let schema = match schema {
            Ok(s) => s,
            Err(e) => panic!("{e}"),
        };
        assert_eq!(schema.fold("khk"), "khk");
    }
}
