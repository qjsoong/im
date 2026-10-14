# im — a Unix input method

[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)
[![Rust](https://img.shields.io/badge/rust-1.89%2B-orange.svg)](https://www.rust-lang.org)

**im** is a small Chinese input method for Linux/X11 built out of ordinary
Unix parts: a text-configured engine, a terminal interface you can pipe from,
and an X11 front end that talks XIM so it does not need a desktop environment.

It exists because input methods have grown into desktop suites. `im` aims for
the opposite: configuration you can read, a dictionary you can `grep`, a
candidate picker you can use over SSH, and no GUI settings dialog anywhere.

```console
$ echo zhongguo | im query -n 1
中国

$ im --schema double_pinyin query --format tsv vsgo
vsgo	exact	中国	zhong guo	12271	system
vsgo	backoff	中	zhong	15402	system
vsgo	backoff	种	zhong	12930	system

$ im          # interactive picker: type, choose, print to stdout
```

## What it does

- **Full pinyin** (`pinyin`) and **double pinyin**, with 自然码 (`double_pinyin`)
  and 小鹤 (`xiaohe`) built in. A new scheme is a TOML file, not a patch.
- **Text configuration.** `im.toml`, layered from built-in defaults through
  `/etc/xdg/im/im.toml` to `~/.config/im/im.toml`, key by key. `im config show`
  prints the merged result.
- **Tab-separated dictionaries.** `text<TAB>spelling<TAB>weight`, one record per
  line, sorted, diffable and mergeable with `sort` and `comm`.
- **A candidate picker in your terminal.** `im` opens a TUI, reads keystrokes and
  writes the chosen text to stdout, so it composes with the rest of the shell.
- **XIM for the desktop.** `im-xim` is an X Input Method server: set
  `XMODIFIERS=@im=im` and any XIM-capable toolkit — xterm, st, GTK 3, Qt,
  Emacs — can type Chinese, with no bus, no daemon manager and no desktop
  environment involved.
- **Small and predictable.** No network, no database, no background indexer. The
  engine is a sorted array of codes and a dozen binary searches.

## Status

Early, but complete enough to use. The engine, the shipped dictionary, the
terminal picker, the query interface and the XIM server all work and are covered
by tests; there is no tagged release yet. Until binaries are published, install
from source:

```bash
cargo install --git https://github.com/qjsoong/im im-cli
```

The minimum supported Rust version is 1.89.

## How it works

A **code** is a short ASCII string, and that is the whole data model:

```text
data/dict/pinyin.tsv      text      spelling      weight
                          中国      zhong guo     12271
                            |
                            |  Schema::speller().fold()
                            v
                          "zhongguo"  (full pinyin)   "vsgo"  (自然码双拼)
```

Because the code is produced by *folding* a spelling, the same dictionary serves
every scheme: only the folding tables change. Full pinyin is the identity case,
and a code table like wubi is the case where the spelling already *is* the code.
Lookup is then a binary search over the sorted codes, preferring an exact match,
then a shorter prefix of the input, then a completion of an unfinished code.

## Documentation

| Document                                | What is in it                              |
| --------------------------------------- | ------------------------------------------ |
| [`docs/config.md`](docs/config.md)      | every configuration option and its default |
| [`docs/schemas.md`](docs/schemas.md)    | how to write a new input scheme            |
| [`docs/dictionary.md`](docs/dictionary.md) | dictionary format and the user dictionary |
| [`docs/design.md`](docs/design.md)      | the architecture and why it is shaped this way |

## Repository layout

| Crate                | Responsibility                                       |
| -------------------- | ---------------------------------------------------- |
| `crates/im-core`     | dictionaries, code index, schemas, composition state |
| `crates/im-cli`      | the `im` binary: terminal picker and query commands   |
| `crates/im-xim`      | the `im-xim` X Input Method server                    |
| `xtask`              | development tasks, including the dictionary pipeline  |

## Contributing

See [`CONTRIBUTING.md`](CONTRIBUTING.md). Bug reports with a reproducible
dictionary excerpt and the exact keystrokes are the most useful kind.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.

The generated dictionary in `data/dict/` is the exception: it is a derived work
of sources that include CC-CEDICT, so it is distributed under
**Creative Commons Attribution-ShareAlike 4.0**. The code that builds it is MIT
OR Apache-2.0 like everything else. See [`data/dict/NOTICE.md`](data/dict/NOTICE.md)
for the attribution and for what share-alike means in practice.
