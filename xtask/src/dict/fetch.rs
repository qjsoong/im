//! Downloading the upstream pinyin data sources.
//!
//! `dict fetch` shells out to `curl`, which must be on `PATH`. Every source
//! is downloaded into `target/dict-src` and skipped when it is already
//! complete, so the action can be re-run at any time.

use std::fs;
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, bail};

use super::Paths;

/// `mozillazg/pinyin-data` `pinyin.txt`: single-character readings.
pub const PINYIN_DATA: &str = "pinyin-data-pinyin.txt";
/// `mozillazg/phrase-pinyin-data` `large_pinyin.txt`: phrase readings.
pub const PHRASE_LARGE: &str = "phrase-pinyin-data-large_pinyin.txt";
/// `mozillazg/phrase-pinyin-data` `pinyin.txt`: hand-curated readings.
pub const PHRASE_CURATED: &str = "phrase-pinyin-data-pinyin.txt";
/// `fxsjy/jieba` `dict.txt`: word frequencies.
pub const JIEBA: &str = "jieba-dict.txt";

/// Upstream URL of [`PINYIN_DATA`].
const PINYIN_DATA_URL: &str = concat!(
    "https://raw.githubusercontent.com/mozillazg/pinyin-data",
    "/master/pinyin.txt"
);
/// Upstream URL of [`PHRASE_LARGE`].
const PHRASE_LARGE_URL: &str = concat!(
    "https://raw.githubusercontent.com/mozillazg/phrase-pinyin-data",
    "/master/large_pinyin.txt"
);
/// Upstream URL of [`PHRASE_CURATED`].
const PHRASE_CURATED_URL: &str = concat!(
    "https://raw.githubusercontent.com/mozillazg/phrase-pinyin-data",
    "/master/pinyin.txt"
);
/// Upstream URL of [`JIEBA`].
const JIEBA_URL: &str = concat!(
    "https://raw.githubusercontent.com/fxsjy/jieba",
    "/master/jieba/dict.txt"
);

/// Number of download attempts per source.
const ATTEMPTS: u32 = 3;

/// Pause between two download attempts.
const RETRY_DELAY: Duration = Duration::from_secs(2);

/// One file downloaded by `dict fetch`.
struct Source {
    /// File name inside `target/dict-src`.
    name: &'static str,
    /// Download URL.
    url: &'static str,
    /// Lower bound that catches truncated downloads.
    min_bytes: u64,
}

/// Sources in the order in which `dict fetch` downloads them.
const SOURCES: &[Source] = &[
    Source {
        name: PINYIN_DATA,
        url: PINYIN_DATA_URL,
        min_bytes: 256 * 1024,
    },
    Source {
        name: PHRASE_LARGE,
        url: PHRASE_LARGE_URL,
        min_bytes: 4 * 1024 * 1024,
    },
    Source {
        name: PHRASE_CURATED,
        url: PHRASE_CURATED_URL,
        min_bytes: 256 * 1024,
    },
    Source {
        name: JIEBA,
        url: JIEBA_URL,
        min_bytes: 2 * 1024 * 1024,
    },
];

/// Download every source into `paths.src_dir`.
///
/// Files that are already present and complete are left alone.
pub fn fetch_all(paths: &Paths) -> Result<()> {
    fs::create_dir_all(&paths.src_dir)
        .with_context(|| format!("failed to create {}", paths.src_dir.display()))?;
    for source in SOURCES {
        fetch_one(&paths.src_dir, source)?;
    }
    Ok(())
}

/// Fail unless every source has been downloaded.
pub fn ensure_fetched(paths: &Paths) -> Result<()> {
    for source in SOURCES {
        let path = paths.source(source.name);
        if !path.is_file() {
            bail!(
                "{} is missing; run `cargo run -p xtask -- dict fetch` first",
                path.display()
            );
        }
    }
    Ok(())
}

/// Download one source unless the destination is already complete.
fn fetch_one(dir: &Path, source: &Source) -> Result<()> {
    let dest = dir.join(source.name);
    if dest.is_file() {
        let size = fs::metadata(&dest)?.len();
        if size >= source.min_bytes {
            eprintln!(
                "fetch: {} is complete ({size} bytes), skipping",
                source.name
            );
            return Ok(());
        }
        eprintln!(
            "fetch: {} is incomplete ({size} bytes), downloading again",
            source.name
        );
    }

    let partial = dest.with_extension("part");
    let mut failure = String::new();
    for attempt in 1..=ATTEMPTS {
        let outcome = download(source.url, &partial).and_then(|()| {
            let size = fs::metadata(&partial)?.len();
            if size < source.min_bytes {
                bail!(
                    "downloaded {size} bytes, expected at least {}",
                    source.min_bytes
                );
            }
            fs::rename(&partial, &dest)?;
            Ok(size)
        });
        match outcome {
            Ok(size) => {
                eprintln!("fetch: {} ({size} bytes)", source.name);
                return Ok(());
            }
            Err(error) => failure = format!("{error:#}"),
        }
        let _ = fs::remove_file(&partial);
        if attempt < ATTEMPTS {
            eprintln!("fetch: retrying {}: {failure}", source.name);
            thread::sleep(RETRY_DELAY);
        }
    }
    bail!(
        "failed to download {} from {}: {failure}",
        source.name,
        source.url
    )
}

/// Run `curl` for one download attempt.
fn download(url: &str, dest: &Path) -> Result<()> {
    let output = Command::new("curl")
        .args([
            "--fail",
            "--location",
            "--silent",
            "--show-error",
            "--max-time",
            "600",
            "--output",
        ])
        .arg(dest)
        .arg(url)
        .output()
        .context("failed to run curl; it must be installed and on PATH")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("curl exited with {}: {}", output.status, stderr.trim());
    }
    Ok(())
}
