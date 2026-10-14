//! Loading everything a front end needs: directories, configuration, schemas
//! and dictionaries.
//!
//! Discovery follows the XDG Base Directory specification. See
//! [`im_core::paths`] for the layout and for the environment variables that
//! override it.

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};

use im_core::{
    CodeIndex, Config, Dictionary, LookupOptions, Origin, Paths, Schema,
    schema::Catalog,
};

use crate::GlobalArgs;

/// The resolved state of one `im` invocation.
pub struct Context {
    /// Directories that are searched for configuration and data.
    pub paths: Paths,
    /// The merged configuration.
    pub config: Config,
    /// Configuration files that contributed, lowest precedence first.
    pub config_files: Vec<PathBuf>,
    /// Every schema that can be selected.
    pub catalog: Catalog,
}

impl Context {
    /// Load the configuration and schema catalog from the environment.
    pub fn load() -> Result<Self> {
        let paths = Paths::from_env();
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

    /// The schema to use: the one named on the command line, otherwise the one
    /// named in the configuration.
    pub fn schema(&self, id: Option<&str>) -> Result<Schema> {
        let id = id.unwrap_or(&self.config.schema);
        self.catalog
            .get(id)
            .cloned()
            .with_context(|| format!("schema {id:?} is not available"))
    }

    /// Build the search index for `schema`.
    ///
    /// Every dictionary the schema names is loaded from every data directory,
    /// with later directories shadowing earlier ones only for identical
    /// `(text, spelling)` pairs — dictionaries are merged, so a small
    /// supplemental file can extend a large one. The user's own dictionary is
    /// always loaded last and always wins.
    pub fn index(&self, schema: &Schema, global: &GlobalArgs) -> Result<CodeIndex> {
        let mut dictionaries = self.system_dictionaries(schema, global)?;
        if let Some(dict) = self.user_dictionary()? {
            dictionaries.push(dict);
        }
        let index = schema.index(dictionaries).with_context(|| {
            format!("building the index for schema {:?}", schema.id)
        })?;
        Ok(index)
    }

    /// Load the dictionaries a schema names, without the user's own.
    pub fn system_dictionaries(
        &self,
        schema: &Schema,
        global: &GlobalArgs,
    ) -> Result<Vec<Dictionary>> {
        let names: Vec<String> = if global.dictionary.is_empty() {
            schema.dictionaries.clone()
        } else {
            global.dictionary.clone()
        };

        let mut dictionaries = Vec::new();
        for name in &names {
            // `--dictionary ./mine.tsv` is accepted as well as a name, so a
            // dictionary can be tried out without installing it anywhere.
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

    /// The user dictionary, loading it if it exists.
    pub fn user_dictionary(&self) -> Result<Option<Dictionary>> {
        let Some(path) = self.paths.user_dictionary_file() else {
            return Ok(None);
        };
        Dictionary::read_if_exists(&path, Origin::User)
            .with_context(|| format!("reading {}", path.display()))
    }

    /// Candidates per page, with the command line winning over the schema and
    /// the schema over the configuration.
    pub fn page_size(&self, schema: &Schema, global: &GlobalArgs) -> usize {
        global
            .page_size
            .or(schema.page_size)
            .unwrap_or(self.config.page_size)
            .max(1)
    }

    /// Lookup limits for this invocation.
    pub fn lookup_options(&self, global: &GlobalArgs) -> LookupOptions {
        let mut options = self.config.lookup_options();
        if let Some(limit) = global.max_candidates {
            options.limit = limit.max(1);
        }
        if global.no_completion {
            options.completion = false;
        }
        options
    }

    /// Whether `/dev/tty` can be opened, which is what the picker draws on.
    pub fn has_controlling_terminal(&self) -> bool {
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/tty")
            .is_ok()
    }
}

/// Whether a `--dictionary` value names a file rather than a dictionary.
fn is_path(value: &str) -> bool {
    value.contains('/') || value.ends_with(".tsv")
}

/// The dictionary compiled into the binary, when this build has one.
fn embedded_dictionary(name: &str) -> Result<Option<Dictionary>> {
    let Some((embedded_name, source)) = im_core::dict::embedded_dictionary() else {
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

    fn context_with(data: &std::path::Path, config: &std::path::Path) -> Context {
        let paths =
            Paths::new(vec![config.to_path_buf()], vec![data.to_path_buf()], None);
        let (config, config_files) =
            Config::load(&paths).unwrap_or_else(|e| panic!("{e}"));
        let catalog = Catalog::load(&paths).unwrap_or_else(|e| panic!("{e}"));
        Context {
            paths,
            config,
            config_files,
            catalog,
        }
    }

    fn write(path: &std::path::Path, contents: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap_or_else(|e| panic!("{e}"));
        }
        std::fs::write(path, contents).unwrap_or_else(|e| panic!("{e}"));
    }

    fn no_args() -> GlobalArgs {
        GlobalArgs {
            schema: None,
            page_size: None,
            max_candidates: None,
            no_completion: false,
            dictionary: Vec::new(),
            prompt: None,
            initial: None,
        }
    }

    #[test]
    fn loads_a_shipped_dictionary() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let data = dir.path().join("data");
        write(&data.join("dict/pinyin.tsv"), "中\tzhong\t900\n");
        let context = context_with(&data, &dir.path().join("config"));
        let schema = context.schema(None).unwrap_or_else(|e| panic!("{e}"));
        let index = context
            .index(&schema, &no_args())
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(index.exact("zhong").len(), 1);
    }

    #[test]
    fn the_dictionary_compiled_into_im_is_the_fallback() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let context =
            context_with(&dir.path().join("data"), &dir.path().join("config"));
        let schema = context.schema(None).unwrap_or_else(|e| panic!("{e}"));
        let index = context
            .index(&schema, &no_args())
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
    fn an_unknown_dictionary_names_the_directories_it_searched() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let context =
            context_with(&dir.path().join("data"), &dir.path().join("config"));
        let schema = context.schema(None).unwrap_or_else(|e| panic!("{e}"));
        let mut args = no_args();
        args.dictionary = vec!["mine".to_owned()];
        let error = context
            .index(&schema, &args)
            .err()
            .unwrap_or_else(|| panic!("expected an error"));
        let message = format!("{error:#}");
        assert!(message.contains("mine"), "{message}");
        assert!(message.contains("dict/mine.tsv"), "{message}");
    }

    #[test]
    fn a_dictionary_can_be_given_as_a_path() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let context =
            context_with(&dir.path().join("data"), &dir.path().join("config"));
        let schema = context.schema(None).unwrap_or_else(|e| panic!("{e}"));
        let path = dir.path().join("mine.tsv");
        write(&path, "词\tci\t10\n");
        let mut args = no_args();
        args.dictionary = vec![path.display().to_string()];
        let index = context
            .index(&schema, &args)
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(index.exact("ci").len(), 1);
    }

    #[test]
    fn the_user_dictionary_is_merged_last() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let data = dir.path().join("data");
        write(&data.join("dict/pinyin.tsv"), "中\tzhong\t900\n");
        write(&data.join("user.tsv"), "中\tzhong\t1\n");
        let context = context_with(&data, &dir.path().join("config"));
        let schema = context.schema(None).unwrap_or_else(|e| panic!("{e}"));
        let index = context
            .index(&schema, &no_args())
            .unwrap_or_else(|e| panic!("{e}"));
        let exact = index.exact("zhong");
        assert_eq!(exact.len(), 1, "duplicates are merged: {exact:?}");
        assert_eq!(exact[0].origin, Origin::User);
    }

    #[test]
    fn command_line_overrides_the_configuration() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let context =
            context_with(&dir.path().join("data"), &dir.path().join("config"));
        let schema = context.schema(None).unwrap_or_else(|e| panic!("{e}"));

        assert_eq!(
            context.page_size(&schema, &no_args()),
            context.config.page_size
        );

        let mut args = no_args();
        args.page_size = Some(3);
        assert_eq!(context.page_size(&schema, &args), 3);

        let mut args = no_args();
        args.no_completion = true;
        assert!(!context.lookup_options(&args).completion);
        assert_eq!(context.lookup_options(&args).limit, 100);
    }
}
