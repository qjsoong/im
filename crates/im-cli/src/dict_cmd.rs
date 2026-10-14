//! `im dict` — the user's own dictionary.
//!
//! Everything here is plain text: the file is a normal `im` dictionary and is
//! rewritten in canonical (sorted, deduplicated) form after each edit, so it
//! stays a good `git diff` target.

use anyhow::{Context as _, Result, bail};
use clap::Subcommand;

use im_core::{Entry, Origin};

use crate::{GlobalArgs, context::Context};

#[derive(Debug, Subcommand)]
pub enum DictCommand {
    /// Print the path of your own dictionary.
    Path,

    /// Print what your own dictionary contains.
    List,

    /// Add a word, or update the weight of one you already added.
    Add {
        /// The word to add.
        text: String,

        /// Pinyin spelling, for example `zhong guo`. Omitted when the word is
        /// already known to one of the shipped dictionaries, whose spelling and
        /// weight are then reused.
        spelling: Option<String>,

        /// Ranking weight, 0..=65535. Higher wins among your own words.
        #[arg(short, long, default_value_t = 0)]
        weight: u16,
    },

    /// Remove every entry for a word from your own dictionary.
    Remove {
        /// The word to remove.
        text: String,
    },
}

/// Dispatch one `im dict` subcommand.
pub fn run(context: &Context, global: &GlobalArgs, command: DictCommand) -> Result<u8> {
    match command {
        DictCommand::Path => {
            let path = context
                .paths
                .user_dictionary_file()
                .context("cannot find a home directory for your dictionary")?;
            println!("{}", path.display());
        }
        DictCommand::List => {
            let Some(dict) = context.user_dictionary()? else {
                return Ok(0);
            };
            for entry in dict.entries() {
                println!("{}\t{}\t{}", entry.text, entry.spelling, entry.weight);
            }
        }
        DictCommand::Add {
            text,
            spelling,
            weight,
        } => add(context, global, &text, spelling.as_deref(), weight)?,
        DictCommand::Remove { text } => remove(context, &text)?,
    }
    Ok(0)
}

fn add(
    context: &Context,
    global: &GlobalArgs,
    text: &str,
    spelling: Option<&str>,
    weight: u16,
) -> Result<()> {
    let path = context
        .paths
        .user_dictionary_file()
        .context("cannot find a home directory for your dictionary")?;

    let mut dictionary = context.user_dictionary()?.unwrap_or_default();
    let (spelling, weight) = match spelling {
        Some(spelling) => (spelling.to_owned(), weight),
        None => {
            let known = find_known(context, global, text)?.with_context(|| {
                format!(
                    "{text:?} is not in any loaded dictionary; pass its pinyin spelling, \
                     for example `im dict add {text} \"...\"`"
                )
            })?;
            (known.spelling.clone(), weight.max(known.weight))
        }
    };

    dictionary.insert(Entry {
        text: text.to_owned(),
        spelling,
        weight,
        origin: Origin::User,
    });
    dictionary
        .write(&path)
        .with_context(|| format!("writing {}", path.display()))?;
    println!("{}", path.display());
    Ok(())
}

fn remove(context: &Context, text: &str) -> Result<()> {
    let Some(path) = context.paths.user_dictionary_file() else {
        bail!("cannot find a home directory for your dictionary");
    };
    let Some(mut dictionary) = context.user_dictionary()? else {
        return Ok(());
    };
    let before = dictionary.len();
    dictionary.retain(|entry| entry.text != text);
    if dictionary.len() == before {
        eprintln!("im: {text:?} is not in your dictionary");
        return Ok(());
    }
    dictionary
        .write(&path)
        .with_context(|| format!("writing {}", path.display()))?;
    println!("{}", path.display());
    Ok(())
}

/// Find `text` in the shipped dictionaries, so its spelling can be reused.
fn find_known(
    context: &Context,
    global: &GlobalArgs,
    text: &str,
) -> Result<Option<Entry>> {
    let schema = context.schema(global.schema.as_deref())?;
    let dictionaries = context.system_dictionaries(&schema, global)?;
    let mut found: Option<Entry> = None;
    for dictionary in dictionaries {
        for entry in dictionary.entries() {
            if entry.text != text {
                continue;
            }
            let better = found.as_ref().is_none_or(|best| entry.weight > best.weight);
            if better {
                found = Some(entry.clone());
            }
        }
    }
    Ok(found)
}
