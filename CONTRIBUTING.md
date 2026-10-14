# Contributing to im

Thanks for taking the time to contribute. This document covers the development
workflow, the checks CI runs, and what a good change looks like.

## Getting started

```bash
git clone https://github.com/qjsoong/im
cd im
cargo build
cargo test --workspace
```

The workspace requires Rust 1.89 or newer (`rust-toolchain.toml` pins the
channel; `rustfmt` and `clippy` are part of it).

## Repository layout

| Crate            | Responsibility                                        |
| ---------------- | ----------------------------------------------------- |
| `crates/im-core` | dictionaries, code index, schemas, composition state   |
| `crates/im-cli`  | the `im` binary: terminal picker and query commands    |
| `crates/im-xim`  | the `im-xim` X Input Method server                     |
| `xtask`          | development tasks, including the dictionary pipeline   |

`im-core` has no dependency on terminals, X11 or any specific desktop. Front
ends push keystrokes in and read candidates out; if a change needs something
front-end specific, it belongs in a front-end crate.

## Before opening a pull request

Run the same checks as CI:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p xtask -- dict check     # validates data/dict/pinyin.tsv
```

## Changing a shipped dictionary

`data/dict/pinyin.tsv` is generated, not edited. Change the pipeline in
`xtask/src/dict/` and regenerate:

```bash
cargo run -p xtask -- dict fetch
cargo run -p xtask -- dict build
```

The output must be byte-for-byte reproducible, so a review is a `git diff` of
the data. Give the reason for a curatorial change (a wrong reading, a missing
word) in the commit message; a diff that adds ten thousand words without an
explanation will be asked for one.

## Adding an input scheme

Schemes are data. Add `crates/im-core/data/schemas/<id>.toml`, register it in
`crates/im-core/src/schema/builtin.rs`, and add the folding cases you care about
to the tests in `crates/im-core/src/speller.rs`. If a scheme cannot be expressed
with the existing tables, that is a design discussion worth having in an issue
first.

## Commit messages

Conventional Commits, one logical change per commit, imperative mood:

```text
feat(core): support table-based schemes

Spellings that are already codes need no folding, so `SchemaKind::Table`
skips the speller entirely. This is what makes wubi possible without new
engine code.
```

Prefixes in use: `feat`, `fix`, `docs`, `test`, `refactor`, `perf`, `build`,
`ci`, `chore`. Scopes follow the crate or area: `core`, `cli`, `xim`, `dict`,
`config`, `schema`.

Tests and documentation for a change belong in the same commit as the change.
Unrelated formatting does not.

## Reporting a bug

For an input bug, include:

- the schema (`im schema show <id>`),
- the exact keys pressed,
- what you expected and what you got.

For a dictionary bug, include the line from `data/dict/pinyin.tsv` or the entry
you added to `user.tsv`.

## License

By contributing you agree that your contribution is licensed under the same
terms as the project: MIT OR Apache-2.0 for code, and CC BY-SA 4.0 for the
generated files under `data/dict/`, which are a derived work of share-alike
upstream data. See [`data/dict/NOTICE.md`](../data/dict/NOTICE.md). Do not add
a new upstream data source to the dictionary pipeline without recording its
license there first.
