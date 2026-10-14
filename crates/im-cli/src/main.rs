//! `im` — Chinese input from the command line.
//!
//! ```
//! im                      # interactive picker, chosen text goes to stdout
//! im query vsgo           # print the candidates for a code
//! im schema list          # what schemes are installed
//! im config show          # the configuration after layering
//! im dict add 词 "ci"     # teach it a word
//! ```
//!
//! The picker draws on the controlling terminal rather than on stdout, so it
//! can be used in a pipeline: `im > out.txt` or `x=$(im)` both work.

mod context;
mod dict_cmd;
mod query;
mod tui;

use std::io::{self, IsTerminal, Write};
use std::process::ExitCode;

use anyhow::Result;
use clap::{Args, Parser, Subcommand};

use crate::context::Context;

/// Exit code used when the user abandons a composition, matching the shell's
/// convention for a process killed by `SIGINT`.
const EXIT_CANCELLED: u8 = 130;

/// Exit code for a runtime failure.
const EXIT_FAILURE: u8 = 1;

#[derive(Debug, Parser)]
#[command(
    name = "im",
    version,
    about = "Chinese input from the command line",
    long_about = "Chinese input from the command line.\n\n\
                  With no subcommand, `im` opens an interactive candidate picker on \
                  the controlling terminal and writes the text you choose to stdout. \
                  Schemas, dictionaries and key bindings are plain text files; see \
                  `im config show` for the effective configuration.",
    after_help = "EXAMPLES:\n  \
                  im                      pick candidates interactively\n  \
                  im query vsgo           candidates for a 自然码 code\n  \
                  echo vsgo | im query -n 1   the single best match\n  \
                  im --schema pinyin      pick using full pinyin\n  \
                  im schema show double_pinyin",
    disable_help_subcommand = true
)]
struct Cli {
    #[command(flatten)]
    global: GlobalArgs,

    #[command(subcommand)]
    command: Option<Command>,
}

/// Options that apply to every subcommand.
#[derive(Debug, Args)]
struct GlobalArgs {
    /// Input scheme to use, overriding `schema` from the configuration.
    #[arg(short, long, global = true, value_name = "ID")]
    schema: Option<String>,

    /// Candidates shown on one page.
    #[arg(long, global = true, value_name = "N")]
    page_size: Option<usize>,

    /// Upper bound on candidates kept for one input.
    #[arg(long, global = true, value_name = "N")]
    max_candidates: Option<usize>,

    /// Only offer candidates whose code is complete; do not complete
    /// unfinished codes.
    #[arg(long, global = true)]
    no_completion: bool,

    /// Load these dictionaries instead of the ones the schema names.
    #[arg(long, global = true, value_name = "NAME")]
    dictionary: Vec<String>,

    /// Text shown before the input in the picker.
    #[arg(long, global = true, value_name = "TEXT")]
    prompt: Option<String>,

    /// Code to start the picker with, as if it had been typed.
    #[arg(long, global = true, value_name = "CODE")]
    initial: Option<String>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Search for candidates and print them; reads codes from the arguments,
    /// or one code per line from stdin.
    Query(query::QueryArgs),

    /// Inspect or select input schemes.
    #[command(subcommand)]
    Schema(SchemaCommand),

    /// Inspect the merged configuration.
    #[command(subcommand)]
    Config(ConfigCommand),

    /// Manage your own dictionary of extra words.
    #[command(subcommand)]
    Dict(dict_cmd::DictCommand),
}

#[derive(Debug, Subcommand)]
enum SchemaCommand {
    /// List the schemes available on this machine.
    List,

    /// Print a scheme's definition, exactly as `im` loads it.
    Show {
        /// Scheme identifier.
        id: String,
    },
}

#[derive(Debug, Subcommand)]
enum ConfigCommand {
    /// Print the configuration after layering the defaults, system and user
    /// files.
    Show {
        /// Print the built-in defaults instead of the merged result.
        #[arg(long)]
        default: bool,
    },

    /// Print the files that contribute to the configuration, lowest
    /// precedence first.
    Path,
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            // A broken pipe is how `im query ... | head -1` ends; it is not a
            // failure worth reporting.
            if is_broken_pipe(&error) {
                return ExitCode::SUCCESS;
            }
            eprintln!("im: {error:#}");
            ExitCode::from(EXIT_FAILURE)
        }
    }
}

fn run() -> Result<u8> {
    let cli = Cli::parse();
    let context = Context::load()?;

    match cli.command {
        Some(Command::Schema(SchemaCommand::List)) => {
            list_schemas(&context, cli.global.schema.as_deref())
        }
        Some(Command::Schema(SchemaCommand::Show { id })) => {
            let schema = context.schema(Some(id.as_str()))?;
            print!("{}", schema.to_toml()?);
            Ok(0)
        }
        Some(Command::Config(ConfigCommand::Show { default })) => {
            if default {
                print!("{}", im_core::config::DEFAULT_TOML);
            } else {
                print!("{}", context.config.to_toml());
            }
            Ok(0)
        }
        Some(Command::Config(ConfigCommand::Path)) => {
            for file in &context.config_files {
                println!("{}", file.display());
            }
            if context.config_files.is_empty() {
                println!("(no configuration file; using the built-in defaults)");
            }
            Ok(0)
        }
        Some(Command::Dict(command)) => dict_cmd::run(&context, &cli.global, command),
        Some(Command::Query(args)) => {
            query::run(&context, &cli.global, &args)?;
            Ok(0)
        }
        None => run_picker(&context, &cli.global),
    }
}

fn run_picker(context: &Context, global: &GlobalArgs) -> Result<u8> {
    let schema = context.schema(global.schema.as_deref())?;
    let index = context.index(&schema, global)?;
    let page_size = context.page_size(&schema, global);
    let lookup_options = context.lookup_options(global);

    if !io::stdin().is_terminal() && !context.has_controlling_terminal() {
        anyhow::bail!(
            "im needs a terminal to draw on; use `im query` when there is none"
        );
    }

    let options = tui::Options {
        prompt: global.prompt.clone(),
        initial: global.initial.clone(),
    };
    match tui::run(
        &schema,
        &index,
        page_size,
        lookup_options,
        &context.config,
        &options,
    )? {
        Some(text) => {
            let mut stdout = io::BufWriter::new(io::stdout().lock());
            writeln!(stdout, "{text}")?;
            stdout.flush()?;
            Ok(0)
        }
        None => Ok(EXIT_CANCELLED),
    }
}

fn list_schemas(context: &Context, active: Option<&str>) -> Result<u8> {
    let active = active.unwrap_or(&context.config.schema);
    for schema in context.catalog.iter() {
        let marker = if schema.id == active { '*' } else { ' ' };
        let dictionaries = schema.dictionaries.join(", ");
        println!(
            "{marker} {:<16} {:<12} {}  [{}]",
            schema.id, schema.name, schema.description, dictionaries
        );
    }
    Ok(0)
}

fn is_broken_pipe(error: &anyhow::Error) -> bool {
    error
        .chain()
        .filter_map(|cause| cause.downcast_ref::<io::Error>())
        .any(|io| io.kind() == io::ErrorKind::BrokenPipe)
}
