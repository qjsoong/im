//! Error type shared by every `im` crate.

/// Result alias used throughout `im-core`.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Everything that can go wrong while loading configuration, dictionaries or
/// schemas, or while composing a session.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A dictionary file could not be parsed.
    #[error("{path}:{line}: {message}")]
    Dictionary {
        /// File the problem was found in, or `<inline>` for in-memory input.
        path: String,
        /// 1-based line number.
        line: usize,
        /// What is wrong with that line.
        message: String,
    },

    /// A schema definition is invalid.
    #[error("schema {id}: {message}")]
    Schema {
        /// Identifier of the offending schema.
        id: String,
        /// What is wrong with it.
        message: String,
    },

    /// A configuration file is not valid TOML or does not match the schema.
    #[error("{path}: {source}")]
    Config {
        /// Path of the offending file.
        path: String,
        /// Underlying TOML error.
        #[source]
        source: Box<toml::de::Error>,
    },

    /// A configuration value is out of range or contradicts another value.
    #[error("{path}: {message}")]
    ConfigValue {
        /// Path of the offending file.
        path: String,
        /// What is wrong with the value.
        message: String,
    },

    /// A key binding string could not be parsed.
    #[error("{origin}: invalid key binding {binding:?}: {message}")]
    KeyBinding {
        /// Where the binding came from, for example `im.toml [keys]` .
        origin: String,
        /// The offending binding text.
        binding: String,
        /// Why it could not be parsed.
        message: String,
    },

    /// A file could not be read or a directory walked.
    #[error("{path}: {source}")]
    Io {
        /// Path involved in the failing operation.
        path: String,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// A schema or dictionary name in the configuration does not exist.
    #[error("unknown {kind} {name:?} (searched: {searched})")]
    NotFound {
        /// Either `schema` or `dictionary`.
        kind: &'static str,
        /// The name that was requested.
        name: String,
        /// Human-readable list of places that were searched.
        searched: String,
    },
}

impl Error {
    /// Build a [`Error::Dictionary`] for a specific line.
    pub(crate) fn dictionary(
        path: impl Into<String>,
        line: usize,
        message: impl Into<String>,
    ) -> Self {
        Self::Dictionary {
            path: path.into(),
            line,
            message: message.into(),
        }
    }

    /// Build a [`Error::Schema`] for a specific schema.
    pub(crate) fn schema(id: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Schema {
            id: id.into(),
            message: message.into(),
        }
    }

    /// Build a [`Error::Io`] for a specific path.
    pub(crate) fn io(path: impl std::fmt::Display, source: std::io::Error) -> Self {
        Self::Io {
            path: path.to_string(),
            source,
        }
    }

    /// Build a [`Error::ConfigValue`].
    pub(crate) fn config_value(
        path: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self::ConfigValue {
            path: path.into(),
            message: message.into(),
        }
    }
}
