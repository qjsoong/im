//! Where `im` looks for configuration, dictionaries and schemas.
//!
//! The XDG Base Directory specification, applied with the usual Unix
//! convention: earlier directories win, and the user's own directory comes
//! first.
//!
//! | What          | User directory                            | System directories                         |
//! | ------------- | ----------------------------------------- | ------------------------------------------ |
//! | configuration | `$XDG_CONFIG_HOME/im`                     | `$XDG_CONFIG_DIRS/im`                      |
//! | data          | `$XDG_DATA_HOME/im`                       | `$XDG_DATA_DIRS/im`                        |
//! | cache         | `$XDG_CACHE_HOME/im`                      | —                                          |
//! | state         | `$XDG_STATE_HOME/im`                      | —                                          |
//!
//! The `IM_*` variables name *`im` directories*, not base directories, which is
//! what you want for a script, a test or a second profile:
//!
//! | Variable                       | Replaces                                     |
//! | ------------------------------ | -------------------------------------------- |
//! | `IM_CONFIG_HOME`, `IM_DATA_HOME` | the corresponding `$XDG_*_HOME/im`          |
//! | `IM_CONFIG_DIRS`, `IM_DATA_DIRS` | the corresponding `$XDG_*_DIRS` list        |
//! | `IM_CACHE_HOME`, `IM_STATE_HOME` | the corresponding `$XDG_*_HOME/im`          |
//!
//! ```console
//! $ IM_DATA_HOME=$PWD/data im query --schema double_pinyin vsgo
//! ```
//!
//! Layout inside a directory:
//!
//! ```text
//! <config dir>/im.toml          the configuration file
//! <config dir>/schemas/*.toml   user-defined input schemes
//! <data dir>/dict/*.tsv         dictionaries
//! <data dir>/schemas/*.toml     schemes shipped by a distribution
//! <data dir>/user.tsv           the user's own dictionary
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
        let config_home =
            home_dir("IM_CONFIG_HOME", "XDG_CONFIG_HOME", &home, ".config");
        let data_home =
            home_dir("IM_DATA_HOME", "XDG_DATA_HOME", &home, ".local/share");
        let cache_home = home_dir("IM_CACHE_HOME", "XDG_CACHE_HOME", &home, ".cache");
        let state_home =
            home_dir("IM_STATE_HOME", "XDG_STATE_HOME", &home, ".local/state");

        // An `IM_*_DIRS` entry already names an `im` directory; an
        // `XDG_*_DIRS` entry is a base directory and gets `/im` appended.
        let config_dirs = env_list("IM_CONFIG_DIRS").unwrap_or_else(|| {
            env_list("XDG_CONFIG_DIRS")
                .unwrap_or_else(|| split_dirs("/etc/xdg"))
                .into_iter()
                .map(|dir| dir.join("im"))
                .collect()
        });
        let data_dirs = env_list("IM_DATA_DIRS").unwrap_or_else(|| {
            env_list("XDG_DATA_DIRS")
                .unwrap_or_else(|| split_dirs("/usr/local/share:/usr/share"))
                .into_iter()
                .map(|dir| dir.join("im"))
                .collect()
        });

        Self::assemble(
            config_home,
            data_home,
            cache_home,
            state_home,
            config_dirs,
            data_dirs,
        )
    }

    fn assemble(
        user_config: Option<PathBuf>,
        user_data: Option<PathBuf>,
        cache: Option<PathBuf>,
        state: Option<PathBuf>,
        config_dirs: Vec<PathBuf>,
        data_dirs: Vec<PathBuf>,
    ) -> Self {
        let mut config = user_config.into_iter().collect::<Vec<_>>();
        config.extend(config_dirs);
        dedup(&mut config);
        let user_config = config.first().cloned();

        let mut data = user_data.into_iter().collect::<Vec<_>>();
        data.extend(data_dirs);
        dedup(&mut data);
        let user_data = data.first().cloned();

        Self {
            config_dirs: config,
            data_dirs: data,
            user_config,
            user_data,
            cache_dir: cache,
            state_dir: state,
        }
    }

    /// Build a set of directories explicitly; used by tests and by tools that
    /// know exactly which directories to read.
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
    ///
    /// This is the directory `im` writes to, which is why it is the first one
    /// rather than an arbitrary match.
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
        self.user_config.as_ref().map(|dir| dir.join("im.toml"))
    }

    /// Path of the user's dictionary, whether or not it exists.
    pub fn user_dictionary_file(&self) -> Option<PathBuf> {
        self.user_data.as_ref().map(|dir| dir.join("user.tsv"))
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

    /// Human-readable list of what was searched, for error messages.
    pub fn describe(&self) -> String {
        format!(
            "config: [{}]; data: [{}]",
            join_paths(&self.config_dirs),
            join_paths(&self.data_dirs)
        )
    }
}

fn join_paths(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Resolve an `im` directory from `app_var`, or `xdg_var` + `im`, or
/// `$HOME` + `fallback`.
fn home_dir(
    app_var: &str,
    xdg_var: &str,
    home: &Option<PathBuf>,
    fallback: &str,
) -> Option<PathBuf> {
    env_dir(app_var)
        .or_else(|| env_dir(xdg_var).map(|dir| dir.join("im")))
        .or_else(|| home.as_ref().map(|dir| dir.join(fallback).join("im")))
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

/// Read a colon-separated list from an environment variable.
fn env_list(name: &str) -> Option<Vec<PathBuf>> {
    let value = std::env::var(name).ok().filter(|v| !v.is_empty())?;
    Some(split_dirs(&value))
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
    fn user_directories_come_first_and_are_the_only_writable_ones() {
        let paths = Paths::new(
            vec![
                PathBuf::from("/home/u/.config/im"),
                PathBuf::from("/etc/xdg/im"),
            ],
            vec![
                PathBuf::from("/home/u/.local/share/im"),
                PathBuf::from("/usr/share/im"),
            ],
            Some(PathBuf::from("/home/u/.cache/im")),
        );
        assert_eq!(paths.config_dirs()[0], PathBuf::from("/home/u/.config/im"));
        assert_eq!(
            paths.user_config_dir(),
            Some(Path::new("/home/u/.config/im"))
        );
        assert_eq!(
            paths.user_config_file(),
            Some(PathBuf::from("/home/u/.config/im/im.toml"))
        );
        assert_eq!(
            paths.user_dictionary_file(),
            Some(PathBuf::from("/home/u/.local/share/im/user.tsv"))
        );
        assert_eq!(paths.cache_dir(), Some(Path::new("/home/u/.cache/im")));
    }

    #[test]
    fn home_dir_falls_back_from_the_app_variable_to_xdg_to_home() {
        let home = Some(PathBuf::from("/home/u"));
        // None of the environment variables below are set in the test
        // environment, so the chain falls through to `$HOME`.
        assert_eq!(
            home_dir("IM_TEST_APP_HOME", "IM_TEST_XDG_HOME", &home, ".config"),
            Some(PathBuf::from("/home/u/.config/im"))
        );
        assert_eq!(
            home_dir("IM_TEST_APP_HOME", "IM_TEST_XDG_HOME", &None, ".config"),
            None
        );
    }

    #[test]
    fn assembling_keeps_precedence_order() {
        let paths = Paths::assemble(
            Some(PathBuf::from("/u/.config/im")),
            Some(PathBuf::from("/u/.local/share/im")),
            None,
            None,
            vec![PathBuf::from("/etc/xdg/im"), PathBuf::from("/u/.config/im")],
            vec![PathBuf::from("/usr/share/im")],
        );
        assert_eq!(
            paths.config_dirs(),
            &[PathBuf::from("/u/.config/im"), PathBuf::from("/etc/xdg/im")],
            "the user directory is first and duplicates are dropped"
        );
    }
}
