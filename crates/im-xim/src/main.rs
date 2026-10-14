//! `im-xim` — an X Input Method server.
//!
//! Applications that speak XIM find this daemon through `XMODIFIERS`:
//!
//! ```console
//! $ export XMODIFIERS=@im=im
//! $ im-xim &
//! ```
//!
//! Then xterm, st, GTK 3, Qt and Emacs can type Chinese with no desktop
//! environment, no D-Bus and no per-toolkit module involved.
//!
//! # How the preedit works
//!
//! XIM has the *application* draw the preedit string, not the input method. That
//! is why the candidates appear inside the preedit — `1.中国 2.[种] | vsgo` —
//! rather than in a floating window: it is the only way to show them without
//! `im-xim` opening and drawing a window of its own. `[xim] preedit` in
//! `im.toml` chooses between the candidate list, the bare keys, or nothing.

mod handler;
mod keyboard;
mod logging;
mod preedit;

use std::process::ExitCode;

use anyhow::{Context as _, Result};
use clap::Parser;
use x11rb::connection::Connection;
use x11rb::protocol::Event;
use xim::XimConnections;
use xim::x11rb::X11rbServer;

use im_core::{Loader, Request};

use crate::handler::Handler;
use crate::keyboard::Keyboard;

/// Default name applications look for in `XMODIFIERS`.
const DEFAULT_NAME: &str = "im";

#[derive(Debug, Parser)]
#[command(
    name = "im-xim",
    version,
    about = "The im X11 front end: an X Input Method server",
    long_about = "An X Input Method server for im.\n\n\
                  Applications find it through the XMODIFIERS environment \
                  variable:\n\n    \
                  export XMODIFIERS=@im=im\n    \
                  im-xim &\n\n\
                  It runs in the foreground and exits when the X connection \
                  goes away, so start it from your session and let it be \
                  supervised.",
    after_help = "EXAMPLES:\n  \
                  im-xim                       serve the configured schema\n  \
                  im-xim --schema double_pinyin  serve 自然码双拼\n  \
                  im-xim -v                    log connections to stderr"
)]
struct Cli {
    /// Input scheme to serve, overriding `schema` from the configuration.
    #[arg(short, long, value_name = "ID")]
    schema: Option<String>,

    /// Candidates shown on one page.
    #[arg(long, value_name = "N")]
    page_size: Option<usize>,

    /// Upper bound on candidates kept for one input.
    #[arg(long, value_name = "N")]
    max_candidates: Option<usize>,

    /// Only offer candidates whose code is complete.
    #[arg(long)]
    no_completion: bool,

    /// Load these dictionaries instead of the ones the schema names.
    #[arg(long, value_name = "NAME")]
    dictionary: Vec<String>,

    /// Name to register as. Must match `XMODIFIERS=@im=NAME`.
    #[arg(long, value_name = "NAME", default_value = DEFAULT_NAME)]
    name: String,

    /// Print the merged configuration and exit.
    #[arg(long)]
    print_config: bool,

    /// Log more: once for connections, twice for protocol traffic.
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    logging::init(cli.verbose);

    match run(&cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("im-xim: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: &Cli) -> Result<()> {
    let loader = Loader::from_env()?;

    if cli.print_config {
        print!("{}", loader.config().to_toml());
        return Ok(());
    }

    let request = Request {
        schema: cli.schema.clone(),
        dictionaries: cli.dictionary.clone(),
        page_size: cli.page_size,
        max_candidates: cli.max_candidates,
        no_completion: cli.no_completion,
    };
    let schema = loader.schema(request.schema.as_deref())?;
    let index = loader.index(&schema, &request)?;
    let page_size = loader.page_size(&schema, &request);
    let options = loader.lookup_options(&request);

    log::info!(
        "schema {} ({}), {} codes, {} per page",
        schema.id,
        schema.name,
        index.len(),
        page_size
    );

    let (connection, screen) =
        x11rb::connect(None).context("connecting to the X server")?;
    let keyboard =
        Keyboard::load(&connection).context("reading the keyboard mapping")?;

    let mut server =
        X11rbServer::init(&connection, screen, &cli.name, xim::ALL_LOCALES)
            .with_context(|| format!("registering as XIM server {:?}", cli.name))?;
    log::info!(
        "registered as {}. Set XMODIFIERS=@im={}",
        cli.name,
        cli.name
    );

    let mut handler =
        Handler::new(schema, index, loader.config(), page_size, options, keyboard);

    log::info!(
        "starting in {} mode, {} toggles it",
        match handler.mode() {
            im_core::InputMode::Chinese => "Chinese",
            im_core::InputMode::Ascii => "ASCII",
        },
        loader.config().xim.toggle_key
    );

    let mut connections = XimConnections::new();
    loop {
        let event = connection
            .wait_for_event()
            .context("waiting for an X event")?;
        // The keyboard layout can change under us; every client receives this.
        if let Event::MappingNotify(_) = event
            && let Err(error) = handler.reload_keyboard(&connection)
        {
            log::warn!("could not reread the keyboard mapping: {error}");
        }
        server
            .filter_event(&event, &mut connections, &mut handler)
            .context("handling an XIM message")?;
    }
}
