//! Development tasks for the `im` input method.
//!
//! The pinyin dictionary pipeline is reproducible: `dict fetch` downloads the
//! upstream data into `target/dict-src`, `dict build` regenerates the shipped
//! files from that data, and `dict check` validates them without network
//! access.
//!
//! ```text
//! cargo run -p xtask -- dict fetch
//! cargo run -p xtask -- dict build
//! cargo run -p xtask -- dict check
//! ```

mod dict;

use anyhow::Result;
use clap::{Parser, Subcommand};

use dict::Paths;

/// Command line of the development helper.
#[derive(Debug, Parser)]
#[command(name = "xtask", about = "Development tasks for the im input method")]
struct Cli {
    /// Task group to run.
    #[command(subcommand)]
    command: Command,
}

/// Task groups provided by `xtask`.
#[derive(Debug, Subcommand)]
enum Command {
    /// Build and validate the shipped pinyin dictionary data.
    Dict {
        /// Action to run.
        #[command(subcommand)]
        action: DictAction,
    },
}

/// Actions of the `dict` task group.
#[derive(Debug, Subcommand)]
enum DictAction {
    /// Download the upstream pinyin sources into `target/dict-src`.
    Fetch,
    /// Generate `data/dict/pinyin.tsv` and `data/dict/syllables.txt`.
    Build,
    /// Validate the generated dictionary files without network access.
    Check,
}

fn main() -> Result<()> {
    let paths = Paths::workspace()?;
    match Cli::parse().command {
        Command::Dict { action } => match action {
            DictAction::Fetch => dict::fetch::fetch_all(&paths),
            DictAction::Build => dict::build::build(&paths).map(|_| ()),
            DictAction::Check => dict::check::check(&paths),
        },
    }
}
