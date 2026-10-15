# Releasing

For whoever is holding the maintainer's keys. The short version: bump, tag,
push, then check that the release page actually carries the binaries.

## What a release is

One Git tag, `v<major>.<minor>.<patch>`, and one GitHub Release for it with the
same name. The release page is the deliverable; the tag is how it is addressed.

The project is pre-1.0. Anything may change between minor versions, and the
version number is a promise about *the data and configuration formats* rather
than the API: a `v1.0` would mean `im.toml`, the schema files and the dictionary
format are stable enough to write a book about.

## Procedure

1. Make sure `main` is green on GitHub, not just locally.

2. Move the `[Unreleased]` section of `CHANGELOG.md` under the new version and
   add a link at the bottom, following
   [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). The release notes
   are written from it.

3. Bump `version` in the `[workspace.package]` table of the root `Cargo.toml`.
   Every crate inherits it, so that is the only place.

4. Run the checks one last time, from a clean checkout if you can:

   ```bash
   cargo fmt --all --check
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace --locked
   cargo run -p xtask -- dict check
   ```

5. Commit, then tag and push the tag:

   ```bash
   git commit -am "release: v0.2.0"
   git tag -a v0.2.0 -m "im 0.2.0"
   git push origin main v0.2.0
   ```

   Pushing the tag is what starts the work. `.github/workflows/release.yml` runs
   on `v*` tags, builds both binaries with `cargo build --release --locked`,
   packages them with the licences and the dictionary, and attaches the archive
   and a `SHA256SUMS` file to the release.

6. Fill in the release notes with `gh release edit v0.2.0`, or create the
   release in the web interface before pushing the tag. Either way they must
   state the licence exception: the binaries embed a dictionary that is
   CC BY-SA 4.0 while the code is MIT OR Apache-2.0.

7. Check the result rather than assuming it:

   ```bash
   gh release view v0.2.0 --json assets -q '.assets[].name'
   ```

   The archive and `SHA256SUMS` should both be listed, and the README's install
   command should still describe what is actually there.

## Why the workflow cannot be tested by hand

`release.yml` can also be dispatched manually, which builds and packages
without attaching anything: the attach step is guarded by
`github.ref_type == 'tag'`. So a manual run proves the build, and only pushing a
tag proves the release. A tag pushed *before* the workflow existed can never
gain assets — push a new tag instead of editing history.

## Secrets

There are none. The workflow uses the automatic `GITHUB_TOKEN` with
`contents: write`, which is scoped to this repository and expires with the job.
Nothing needs a personal access token, a registry key or a signing key.

## Platforms

`x86_64-unknown-linux-gnu` only, because that is what has been run. A second
target belongs in the matrix in `release.yml` once someone can test it; adding
an entry that has never been executed would be a promise nobody checked.
