//! Timing harness for the dictionary load path.
//!
//! `im` reads a dictionary, folds every spelling through a schema and sorts the
//! result before it can answer anything, so this is the cost of every start.
//! Run it after touching `Dictionary`, `Speller` or `CodeIndex`:
//!
//! ```console
//! $ cargo run --release -p im-core --example timing
//! ```
//!
//! The dictionary must have been generated first (see `xtask/examples`).

use std::time::Instant;

use im_core::{Dictionary, Origin};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = "data/dict/pinyin.tsv";
    let source = std::fs::read_to_string(path)
        .map_err(|e| format!("{path}: {e} — run `cargo run -p xtask -- dict build`"))?;
    println!("{path}: {} bytes", source.len());

    let start = Instant::now();
    let dict = Dictionary::parse(&source, Origin::System)?;
    println!("parse:  {:>8.1?}  {} entries", start.elapsed(), dict.len());

    for id in ["pinyin", "double_pinyin", "xiaohe"] {
        let schema = im_core::schema::builtin::get(id)?;
        let start = Instant::now();
        let index = schema.index([dict.clone()])?;
        println!(
            "index [{id}]: {:>8.1?}  {} codes",
            start.elapsed(),
            index.len()
        );

        let start = Instant::now();
        let mut total = 0usize;
        for code in ["zhong", "guo", "ni", "hao"] {
            total += index.lookup(code, im_core::LookupOptions::default()).len();
        }
        println!(
            "lookup (4 codes): {:>8.1?}  {total} candidates",
            start.elapsed()
        );
    }
    Ok(())
}
