//! A logger sized for a daemon with a `-v` flag.
//!
//! The `xim` crate logs through the `log` facade; this is the other end of it.
//! Nothing is written below the level `-v` asked for, because a daemon that
//! chats on stderr while you are trying to type is worse than a silent one.

use std::io::Write;

use log::{LevelFilter, Log, Metadata, Record};

/// Start logging to stderr.
///
/// `verbosity` is the number of `-v` flags: none logs warnings and errors, one
/// logs connections and input contexts, two logs protocol-level detail.
pub fn init(verbosity: u8) {
    let level = match verbosity {
        0 => LevelFilter::Warn,
        1 => LevelFilter::Info,
        _ => LevelFilter::Debug,
    };
    let logger = Box::new(Stderr);
    if log::set_boxed_logger(logger).is_ok() {
        log::set_max_level(level);
    }
}

struct Stderr;

impl Log for Stderr {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let level = record.level().as_str().to_ascii_lowercase();
        let mut stderr = std::io::stderr().lock();
        let _ = writeln!(stderr, "im-xim: {level}: {}", record.args());
    }

    fn flush(&self) {
        let _ = std::io::stderr().flush();
    }
}
