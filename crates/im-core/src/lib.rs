//! Core of `im`: dictionaries, code indexes, schemas and composition sessions.
//!
//! `im-core` knows nothing about terminals, X11 or any other front end. It turns
//! a stream of keystrokes into a list of candidates and a commit decision; the
//! front ends decide how to render or inject the result.
//!
//! # The model
//!
//! Everything funnels through one idea: a **code** is a short ASCII string.
//!
//! ```text
//!   data/dict/pinyin.tsv          text      spelling      weight
//!                                 中国      zhong guo     12345
//!                                   |
//!                                   |  Schema::speller().fold()
//!                                   v
//!   CodeIndex                     "zhongguo" => 中国, ...
//!                                   |
//!                                   |  Session::push('v') ...
//!                                   v
//!   Candidate                     text, spelling, consumed, weight, kind
//! ```
//!
//! Because the code is produced by *folding* the spelling, one dictionary
//! serves full pinyin, 自然码双拼, 小鹤双拼 or any other scheme — only the
//! [`Speller`] tables change. Table-based schemes (wubi, cangjie) are the
//! degenerate case where the spelling *is* the code.
//!
//! # Example
//!
//! ```
//! use im_core::{Dictionary, Origin, Session, schema};
//!
//! let schema = schema::builtin::get("double_pinyin")?;
//! let dict = Dictionary::parse(
//!     "中\tzhong\t9000\n国\tguo\t8000\n中国\tzhong guo\t7000\n",
//!     Origin::System,
//! )?;
//! let index = schema.index([dict])?;
//!
//! let mut session = Session::new(&schema, &index);
//! for c in "vsgo".chars() {
//!     assert!(session.push(c));
//! }
//! assert_eq!(session.candidates()[0].text, "中国");
//! # Ok::<(), im_core::Error>(())
//! ```

#![forbid(unsafe_code)]

pub mod config;
pub mod dict;
pub mod error;
pub mod index;
pub mod key;
pub mod keymap;
pub mod loader;
pub mod paths;
pub mod schema;
pub mod session;
mod speller;
mod text;

pub use config::{Config, InputMode, PreeditStyle, XimConfig};
pub use dict::{Dictionary, Entry, Origin};
pub use error::{Error, Result};
pub use index::{Candidate, CodeIndex, LookupOptions, MatchKind};
pub use key::{Key, KeyEvent, Mods, Named};
pub use keymap::{Action, Keymap};
pub use loader::{Loader, Request};
pub use paths::Paths;
pub use schema::{Schema, SchemaKind};
pub use session::{Commit, Session};
pub use speller::Speller;
