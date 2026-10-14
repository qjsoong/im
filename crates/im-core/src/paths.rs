//! Where `im` looks for configuration, dictionaries and schemas.
//!
//! The XDG Base Directory specification, applied with the usual Unix
//! convention: earlier directories win, and the user's own directory comes
//! first. `im` never writes anywhere except the user's own directories.
//!
//! | What           | User                                  | System                        |
//! | -------------- | ------------------------------------- | ----------------------------- |
//! | configuration  | `$XDG_CONFIG_HOME/im`                 | `$XDG_CONFIG_DIRS/im`         |
//! | data           | `$XDG_DATA_HOME/im`                   | `$XDG_DATA_DIRS/im`           |
//! | cache          | `$XDG_CACHE_HOME/im`                  | —                             |
//! | state          | `$XDG_STATE_HOME/im`                  | —                             |
//!
//! `IM_CONFIG_HOME`, `IM_DATA_HOME`, `IM_CONFIG_DIRS`, `IM_DATA_DIRS`,
//! `IM_CACHE_HOME` and `IM_STATE_HOME` override the corresponding XDG variable,
//! which makes it possible to run a completely isolated `im` from a script:
//!
//! ```console
//! $ IM_CONFIG_HOME=/tmp/imtest im query vsgo
//! ```
//!
//! Layout inside a directory:
//!
//! ```text
//! <config dir>/im.toml          the configuration file
//! <config dir>/schemas/*.toml   user-defined input schemes
//! <data dir>/dict/*.tsv         dictionaries
//! <data dir>/schemas/*.toml     schemes shipped by a distribution
//! ```

use std::path::{Path, PathBuf};

/// The resolved set of directories `im` reads from.
#[derive(Debug, Clone, Default)]
pub struct Paths {
    config_dirs: Vec<PathBuf>,
    data_dirs: Vec<PathBuf>,
    user_config: Option<PathBuf>,
    user_data: Option<PathBuf>,
    cache_dir: Option<PathBuf>,
    state_dir: Option<PathBuf>,
}

impl Paths {
    /// Resolve directories from the environment.
    pub fn from_env() -> Self {
        let home = env_dir("HOME");
        let user_config = env_dir("IM_CONFIG_HOME")
            .or_else(|| env_dir("XDG_CONFIG_HOME"))
            .or_else(|| home.as_ref().map(|h| h.join(".config")));
        let user_data = env_dir("IM_DATA_HOME")
            .or_else(|| env_dir("XDG_DATA_HOME"))
            .or_else(|| home.as_ref().map(|h| h.join(".local/share")));
        let cache = env_dir("IM_CACHE_HOME")
            .or_else(|| env_dir("XDG_CACHE_HOME"))
            .or_else(|| home.as_ref().map(|h| h.join(".cache")));
        let state = env_dir("IM_STATE_HOME")
            .or_else(|| env_dir("XDG_STATE_HOME"))
            .or_else(|| home.as_ref().map(|h| h.join(".local/state")));

        let config_dirs = dir_list("IM_CONFIG_DIRS", "XDG_CONFIG_DIRS", "/etc/xdg");
        let data_dirs = dir_list(
            "IM_DATA_DIRS",
            "XDG_DATA_DIRS",
            "/usr/local/share:/usr/share",
        );

        Self::assemble(user_config, user_data, cache, state, config_dirs, data_dirs)
    }

    fn assemble(
        user_config: Option<PathBuf>,
        user_data: Option<PathBuf>,
        cache: Option<PathBuf>,
        state: Option<PathBuf>,
        config_dirs: Vec<PathBuf>,
        data_dirs: Vec<PathBuf>,
    ) -> Self {
        let user_config = user_config.map(|d| d.join("im"));
        let user_data = user_data.map(|d| d.join("im"));
        let mut config = Vec::new();
        if let Some(dir) = &user_config {
            config.push(dir.clone());
        }
        config.extend(config_dirs.into_iter().map(|d| d.join("im")));
        dedup(&mut config);

        let mut data = Vec::new();
        if let Some(dir) = &user_data {
            data.push(dir.clone());
        }
        data.extend(data_dirs.into_iter().map(|d| d.join("im")));
        dedup(&mut data);

        Self {
            config_dirs: config,
            data_dirs: data,
            user_config,
            user_data,
            cache_dir: cache.map(|d| d.join("im")),
            state_dir: state.map(|d| d.join("im")),
        }
    }

    /// Build a set of directories explicitly; used by tests and tools.
    pub fn new(
        config_dirs: Vec<PathBuf>,
        data_dirs: Vec<PathBuf>,
        cache_dir: Option<PathBuf>,
    ) -> Self {
        let user_config = config_dirs.first().cloned();
        let user_data = data_dirs.first().cloned();
        Self {
            config_dirs,
            data_dirs,
            user_config,
            user_data,
            cache_dir,
            state_dir: None,
        }
    }

    /// Configuration directories, highest precedence first.
    pub fn config_dirs(&self) -> &[PathBuf] {
        &self.config_dirs
    }

    /// Data directories, highest precedence first.
    pub fn data_dirs(&self) -> &[PathBuf] {
        &self.data_dirs
    }

    /// The user's own configuration directory, if one could be determined.
    pub fn user_config_dir(&self) -> Option<&Path> {
        self.user_config.as_deref()
    }

    /// The user's own data directory, if one could be determined.
    pub fn user_data_dir(&self) -> Option<&Path> {
        self.user_data.as_deref()
    }

    /// The cache directory, if one could be determined.
    pub fn cache_dir(&self) -> Option<&Path> {
        self.cache_dir.as_deref()
    }

    /// The state directory, if one could be determined.
    pub fn state_dir(&self) -> Option<&Path> {
        self.state_dir.as_deref()
    }

    /// Path of the user's configuration file, whether or not it exists.
    pub fn user_config_file(&self) -> Option<PathBuf> {
        self.user_config.as_ref().map(|d| d.join("im.toml"))
    }

    /// Path of the user's dictionary, whether or not it exists.
    pub fn user_dictionary_file(&self) -> Option<PathBuf> {
        self.user_data.as_ref().map(|d| d.join("user.tsv"))
    }

    /// Every existing configuration file, lowest precedence first.
    pub fn config_files(&self) -> Vec<PathBuf> {
        let mut files: Vec<PathBuf> = self
            .config_dirs
            .iter()
            .map(|dir| dir.join("im.toml"))
            .filter(|path| path.is_file())
            .collect();
        files.reverse();
        files
    }

    /// Directories that were searched, for error messages.
    pub fn describe(&self) -> String {
        let config: Vec<String> = self
            .config_dirs
            .iter()
            .map(|d| d.display().to_string())
            .collect();
        let data: Vec<String> = self
            .data_dirs
            .iter()
            .map(|d| d.display().to_string())
            .collect();
        format!(
            "config: [{}]; data: [{}]",
            config.join(", "),
            data.join(", ")
        )
    }
}

/// Read an environment variable and require it to be an absolute directory.
fn env_dir(name: &str) -> Option<PathBuf> {
    let value = std::env::var_os(name)?;
    if value.is_empty() {
        return None;
    }
    let path = PathBuf::from(value);
    path.is_absolute().then_some(path)
}

/// Read a colon-separated directory list, falling back to `default`.
fn dir_list(app_var: &str, xdg_var: &str, default: &str) -> Vec<PathBuf> {
    let raw = std::env::var(app_var)
        .ok()
        .filter(|v| !v.is_empty())
        .or_else(|| std::env::var(xdg_var).ok().filter(|v| !v.is_empty()))
        .unwrap_or_else(|| default.to_owned());
    split_dirs(&raw)
}

/// Split a colon-separated list, dropping relative and empty entries.
fn split_dirs(raw: &str) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = raw
        .split(':')
        .filter(|part| !part.is_empty())
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .collect();
    dedup(&mut dirs);
    dirs
}

fn dedup(paths: &mut Vec<PathBuf>) {
    let mut seen: Vec<PathBuf> = Vec::with_capacity(paths.len());
    paths.retain(|path| {
        if seen.contains(path) {
            false
        } else {
            seen.push(path.clone());
            true
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_split_on_colons_and_drop_relative_and_empty_entries() {
        assert_eq!(
            split_dirs("/a::/b:/c:relative"),
            vec![
                PathBuf::from("/a"),
                PathBuf::from("/b"),
                PathBuf::from("/c")
            ]
        );
        assert_eq!(split_dirs(""), Vec::<PathBuf>::new());
        assert_eq!(split_dirs("relative"), Vec::<PathBuf>::new());
    }

    #[test]
    fn lists_fall_back_to_the_default() {
        let dirs = dir_list("IM_TEST_MISSING", "IM_TEST_MISSING_XDG", "/x:/y");
        assert_eq!(dirs, vec![PathBuf::from("/x"), PathBuf::from("/y")]);
    }

    #[test]
    fn user_directory_comes_first() {
        let paths = Paths::new(
            vec![PathBuf::from("/home/u/.config/im")],
            vec![PathBuf::from("/usr/share/im")],
            None,
        );
        assert_eq!(paths.config_dirs()[0], PathBuf::from("/home/u/.config/im"));
        assert_eq!(
            paths.user_config_file(),
            Some(PathBuf::from("/home/u/.config/im/im.toml"))
        );
        assert_eq!(
            paths.user_dictionary_file(),
            Some(PathBuf::from("/usr/share/im/user.tsv"))
        );
    }
}
