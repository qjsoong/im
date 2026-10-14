//! Finding and loading everything a front end needs.
//!
//! The engine itself does no I/O. Every front end does the same four things
//! before it can do anything useful — find the directories, load and layer the
//! configuration, pick a schema, load its dictionaries and build the index —
//! so that work lives here, once, and `im` and `im-xim` behave identically.
//!
//! This module uses [`anyhow`] because it is glue: a failure here is a message
//! for a person ("dictionary `pinyin` not found, looked in …"), not something a
//! caller can branch on. The engine keeps its own typed [`Error`](crate::Error).

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};

use crate::config::Config;
use crate::dict::{Dictionary, Origin};
use crate::index::{CodeIndex, LookupOptions};
use crate::paths::Paths;
use crate::schema::{Catalog, Schema};

/// What a front end is asking for on this run.
#[derive(Debug, Clone, Default)]
pub struct Request {
    /// Schema to use; `None` means "the one in the configuration".
    pub schema: Option<String>,
    /// Dictionary names or file paths. Empty means "the ones the schema names".
    pub dictionaries: Vec<String>,
    /// Candidates per page, overriding the schema and the configuration.
    pub page_size: Option<usize>,
    /// Upper bound on candidates, overriding the configuration.
    pub max_candidates: Option<usize>,
    /// Do not offer completions of unfinished codes.
    pub no_completion: bool,
}

/// Configuration, schemas and dictionaries, resolved and ready to load from.
#[derive(Debug, Clone)]
pub struct Loader {
    paths: Paths,
    config: Config,
    config_files: Vec<PathBuf>,
    catalog: Catalog,
}

impl Loader {
    /// Resolve everything from the environment.
    pub fn from_env() -> Result<Self> {
        Self::with_paths(Paths::from_env())
    }

    /// Resolve everything from explicitly chosen directories.
    pub fn with_paths(paths: Paths) -> Result<Self> {
        let (config, config_files) = Config::load(&paths)
            .with_context(|| format!("loading configuration ({})", paths.describe()))?;
        let catalog = Catalog::load(&paths)
            .with_context(|| format!("loading schemas ({})", paths.describe()))?;
        Ok(Self {
            paths,
            config,
            config_files,
            catalog,
        })
    }

    /// Directories that were searched.
    pub fn paths(&self) -> &Paths {
        &self.paths
    }

    /// The merged configuration.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Configuration files that contributed, lowest precedence first.
    pub fn config_files(&self) -> &[PathBuf] {
        &self.config_files
    }

    /// Every schema that can be selected.
    pub fn catalog(&self) -> &Catalog {
        &self.catalog
    }

    /// The schema to use: the one asked for, otherwise the configured one.
    pub fn schema(&self, id: Option<&str>) -> Result<Schema> {
        let id = id.unwrap_or(&self.config.schema);
        self.catalog
            .get(id)
            .cloned()
            .with_context(|| format!("schema {id:?} is not available"))
    }

    /// Candidates per page, with the request winning over the schema, and the
    /// schema over the configuration.
    pub fn page_size(&self, schema: &Schema, request: &Request) -> usize {
        request
            .page_size
            .or(schema.page_size)
            .unwrap_or(self.config.page_size)
            .max(1)
    }

    /// Lookup limits implied by the configuration and the request.
    pub fn lookup_options(&self, request: &Request) -> LookupOptions {
        let mut options = self.config.lookup_options();
        if let Some(limit) = request.max_candidates {
            options.limit = limit.max(1);
        }
        if request.no_completion {
            options.completion = false;
        }
        options
    }

    /// Build the search index for `schema`.
    ///
    /// Every dictionary the schema names is loaded from every data directory,
    /// so a small supplemental file can extend a large one. The user's own
    /// dictionary is always loaded last, and its records replace the shipped
    /// ones outright.
    pub fn index(&self, schema: &Schema, request: &Request) -> Result<CodeIndex> {
        let mut dictionaries =
            self.system_dictionaries(schema, &request.dictionaries)?;
        if let Some(dict) = self.user_dictionary()? {
            dictionaries.push(dict);
        }
        schema
            .index(dictionaries)
            .with_context(|| format!("building the index for schema {:?}", schema.id))
    }

    /// Load the dictionaries a schema names, without the user's own.
    ///
    /// A name that looks like a path is read directly, so a dictionary can be
    /// tried out without installing it anywhere.
    pub fn system_dictionaries(
        &self,
        schema: &Schema,
        names: &[String],
    ) -> Result<Vec<Dictionary>> {
        let names: Vec<String> = if names.is_empty() {
            schema.dictionaries.clone()
        } else {
            names.to_vec()
        };

        let mut dictionaries = Vec::new();
        for name in &names {
            if is_path(name) {
                let path = Path::new(name);
                dictionaries.push(
                    Dictionary::read(path, Origin::System)
                        .with_context(|| format!("reading {}", path.display()))?,
                );
                continue;
            }
            let files = self.dictionary_files(name);
            if files.is_empty() {
                if let Some(dict) = embedded_dictionary(name)? {
                    dictionaries.push(dict);
                    continue;
                }
                bail!(
                    "dictionary {name:?} not found (looked for dict/{name}.tsv in {})",
                    self.describe_data_dirs()
                );
            }
            for file in files {
                let dict = Dictionary::read(&file, Origin::System)
                    .with_context(|| format!("reading {}", file.display()))?;
                dictionaries.push(dict);
            }
        }
        Ok(dictionaries)
    }

    /// The user's own dictionary, if it exists.
    pub fn user_dictionary(&self) -> Result<Option<Dictionary>> {
        let Some(path) = self.paths.user_dictionary_file() else {
            return Ok(None);
        };
        Dictionary::read_if_exists(&path, Origin::User)
            .with_context(|| format!("reading {}", path.display()))
    }

    /// Where the user's own dictionary is written.
    pub fn user_dictionary_file(&self) -> Result<PathBuf> {
        self.paths
            .user_dictionary_file()
            .context("cannot find a home directory for your dictionary")
    }

    /// Every existing `dict/<name>.tsv`, lowest precedence first, so that
    /// merging in order gives the user's directory the last word.
    fn dictionary_files(&self, name: &str) -> Vec<PathBuf> {
        let mut files: Vec<PathBuf> = self
            .paths
            .data_dirs()
            .iter()
            .map(|dir| dir.join("dict").join(format!("{name}.tsv")))
            .filter(|path| path.is_file())
            .collect();
        files.reverse();
        files
    }

    fn describe_data_dirs(&self) -> String {
        self.paths
            .data_dirs()
            .iter()
            .map(|dir| dir.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// Whether a dictionary argument names a file rather than a dictionary.
fn is_path(value: &str) -> bool {
    value.contains('/') || value.ends_with(".tsv")
}

/// The dictionary compiled into the binary, when this build has one.
fn embedded_dictionary(name: &str) -> Result<Option<Dictionary>> {
    let Some((embedded_name, source)) = crate::dict::embedded_dictionary() else {
        return Ok(None);
    };
    if name != embedded_name {
        return Ok(None);
    }
    let dict = Dictionary::parse(source, Origin::System)
        .context("parsing the dictionary compiled into this build")?;
    Ok(Some(dict))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap_or_else(|e| panic!("{e}"));
        }
        std::fs::write(path, contents).unwrap_or_else(|e| panic!("{e}"));
    }

    /// A loader reading only from `root`, so nothing installed on the machine
    /// can influence the result.
    fn loader_in(root: &Path) -> Loader {
        let paths =
            Paths::new(vec![root.join("config")], vec![root.join("data")], None);
        Loader::with_paths(paths).unwrap_or_else(|e| panic!("{e}"))
    }

    #[test]
    fn the_dictionary_compiled_into_im_is_the_fallback() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let loader = loader_in(dir.path());
        let schema = loader.schema(None).unwrap_or_else(|e| panic!("{e}"));
        let index = loader
            .index(&schema, &Request::default())
            .unwrap_or_else(|e| panic!("{e}"));
        assert!(
            !index.is_empty(),
            "im ships a dictionary so it works with nothing installed"
        );
        let exact = index.exact("zhongguo");
        assert!(
            exact.iter().any(|candidate| candidate.text == "中国"),
            "expected 中国 in the shipped dictionary: {exact:?}"
        );
    }

    #[test]
    fn a_dictionary_on_disk_shadows_the_embedded_one() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        write(&dir.path().join("data/dict/pinyin.tsv"), "只\tzhi\t1\n");
        let loader = loader_in(dir.path());
        let schema = loader.schema(None).unwrap_or_else(|e| panic!("{e}"));
        let index = loader
            .index(&schema, &Request::default())
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(index.len(), 1, "only the file was read");
        assert!(index.exact("zhongguo").is_empty());
    }

    #[test]
    fn the_user_dictionary_is_merged_last() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        write(&dir.path().join("data/dict/pinyin.tsv"), "中\tzhong\t900\n");
        write(&dir.path().join("data/user.tsv"), "中\tzhong\t1\n");
        let loader = loader_in(dir.path());
        let schema = loader.schema(None).unwrap_or_else(|e| panic!("{e}"));
        let index = loader
            .index(&schema, &Request::default())
            .unwrap_or_else(|e| panic!("{e}"));
        let exact = index.exact("zhong");
        assert_eq!(exact.len(), 1, "duplicates are merged: {exact:?}");
        assert_eq!(exact[0].origin, Origin::User);
    }

    #[test]
    fn an_unknown_dictionary_names_the_directories_it_searched() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let loader = loader_in(dir.path());
        let schema = loader.schema(None).unwrap_or_else(|e| panic!("{e}"));
        let request = Request {
            dictionaries: vec!["mine".to_owned()],
            ..Request::default()
        };
        let error = loader
            .index(&schema, &request)
            .err()
            .unwrap_or_else(|| panic!("expected an error"));
        let message = format!("{error:#}");
        assert!(message.contains("mine"), "{message}");
        assert!(message.contains("dict/mine.tsv"), "{message}");
    }

    #[test]
    fn a_dictionary_can_be_given_as_a_path() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let loader = loader_in(dir.path());
        let schema = loader.schema(None).unwrap_or_else(|e| panic!("{e}"));
        let path = dir.path().join("mine.tsv");
        write(&path, "词\tci\t10\n");
        let request = Request {
            dictionaries: vec![path.display().to_string()],
            ..Request::default()
        };
        let index = loader
            .index(&schema, &request)
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(index.exact("ci").len(), 1);
    }

    #[test]
    fn a_user_schema_shadows_a_built_in_one() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        write(
            &dir.path().join("config/schemas/pinyin.toml"),
            "id = \"pinyin\"\nname = \"我的全拼\"\nkind = \"syllable\"\ndictionaries = [\"pinyin\"]\n",
        );
        let loader = loader_in(dir.path());
        let schema = loader.schema(None).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(schema.name, "我的全拼");
    }

    #[test]
    fn an_unknown_schema_lists_what_is_available() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let loader = loader_in(dir.path());
        let error = loader
            .schema(Some("nope"))
            .err()
            .unwrap_or_else(|| panic!("expected an error"));
        let message = format!("{error:#}");
        assert!(message.contains("nope"), "{message}");
        assert!(message.contains("double_pinyin"), "{message}");
    }

    #[test]
    fn the_request_overrides_the_configuration() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let loader = loader_in(dir.path());
        let schema = loader.schema(None).unwrap_or_else(|e| panic!("{e}"));

        let default = Request::default();
        assert_eq!(
            loader.page_size(&schema, &default),
            loader.config().page_size
        );
        assert_eq!(
            loader.lookup_options(&default).limit,
            loader.config().max_candidates
        );

        let request = Request {
            page_size: Some(3),
            max_candidates: Some(7),
            no_completion: true,
            ..Request::default()
        };
        assert_eq!(loader.page_size(&schema, &request), 3);
        let options = loader.lookup_options(&request);
        assert_eq!(options.limit, 7);
        assert!(!options.completion);
    }

    #[test]
    fn is_path_tells_a_name_from_a_file() {
        assert!(is_path("./mine.tsv"));
        assert!(is_path("/tmp/mine.tsv"));
        assert!(is_path("mine.tsv"));
        assert!(!is_path("pinyin"));
    }
}
