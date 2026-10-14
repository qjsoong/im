//! `im query` — the non-interactive half of the interface.
//!
//! Reading codes from stdin and writing candidates to stdout, one record per
//! line, is what makes `im` usable from a script or on the far end of a pipe:
//!
//! ```console
//! $ echo vsgo | im query -n 1
//! 中国
//! $ im query vsgo | fzf
//! ```

use std::io::{self, BufRead, Write};

use anyhow::{Context as _, Result};
use clap::{Args, ValueEnum};

use im_core::Candidate;

use crate::{GlobalArgs, context::Context};

/// Output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Format {
    /// One candidate per line, the most likely first.
    Text,
    /// Tab-separated records: code, kind, text, spelling, weight, origin.
    Tsv,
}

#[derive(Debug, Args)]
pub struct QueryArgs {
    /// Codes to look up. When omitted, one code is read from each line of
    /// stdin.
    #[arg(value_name = "CODE")]
    codes: Vec<String>,

    /// Print only the first N candidates for each code.
    #[arg(short = 'n', long, value_name = "N")]
    limit: Option<usize>,

    /// How to format the output.
    #[arg(short = 'F', long, value_enum, default_value_t = Format::Text)]
    format: Format,

    /// Print the total number of matches for each code to stderr.
    #[arg(long)]
    count: bool,
}

/// Run a batch of lookups.
pub fn run(context: &Context, global: &GlobalArgs, args: &QueryArgs) -> Result<()> {
    let schema = context.schema(global.schema.as_deref())?;
    let index = context.index(&schema, global)?;
    let mut options = context.lookup_options(global);
    if let Some(limit) = args.limit {
        options.limit = limit.max(1);
    }

    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());

    if args.codes.is_empty() {
        let stdin = io::stdin();
        for line in stdin.lock().lines() {
            let line = line.context("reading a code from stdin")?;
            let code = line.trim();
            if code.is_empty() {
                continue;
            }
            lookup(&mut out, &index, code, args, options)?;
        }
    } else {
        for code in &args.codes {
            lookup(&mut out, &index, code, args, options)?;
        }
    }
    out.flush()?;
    Ok(())
}

fn lookup(
    out: &mut impl Write,
    index: &im_core::CodeIndex,
    code: &str,
    args: &QueryArgs,
    options: im_core::LookupOptions,
) -> Result<()> {
    let candidates = index.lookup(code, options);
    if args.count {
        eprintln!("{code}\t{}", candidates.len());
    }
    for candidate in &candidates {
        match args.format {
            Format::Text => writeln!(out, "{}", candidate.text)?,
            Format::Tsv => writeln!(out, "{}", tsv_row(code, candidate))?,
        }
    }
    Ok(())
}

fn tsv_row(code: &str, candidate: &Candidate) -> String {
    format!(
        "{code}\t{}\t{}\t{}\t{}\t{}",
        candidate.kind.as_str(),
        candidate.text,
        candidate.spelling,
        candidate.weight,
        candidate.origin,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use im_core::{Dictionary, Origin, schema};

    fn index() -> im_core::CodeIndex {
        let schema =
            schema::builtin::get("double_pinyin").unwrap_or_else(|e| panic!("{e}"));
        let dict = Dictionary::parse(
            "中\tzhong\t900\n国\tguo\t800\n中国\tzhong guo\t700\n",
            Origin::System,
        )
        .unwrap_or_else(|e| panic!("{e}"));
        schema.index([dict]).unwrap_or_else(|e| panic!("{e}"))
    }

    #[test]
    fn tsv_rows_are_tab_separated_and_stable() {
        let index = index();
        let candidates = index.lookup("vsgo", im_core::LookupOptions::default());
        let row = tsv_row("vsgo", &candidates[0]);
        let fields: Vec<&str> = row.split('\t').collect();
        assert_eq!(
            fields,
            vec!["vsgo", "exact", "中国", "zhong guo", "700", "system"]
        );
    }

    #[test]
    fn text_format_prints_one_candidate_per_line() {
        let index = index();
        let mut out: Vec<u8> = Vec::new();
        let args = QueryArgs {
            codes: Vec::new(),
            limit: Some(2),
            format: Format::Text,
            count: false,
        };
        let options = im_core::LookupOptions {
            limit: 2,
            completion: true,
        };
        lookup(&mut out, &index, "vs", &args, options)
            .unwrap_or_else(|e| panic!("{e}"));
        let text = String::from_utf8(out).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(text.lines().count(), 2);
        assert!(text.starts_with("中\n"), "{text}");
    }
}
