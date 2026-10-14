# Changelog

All notable changes to this project are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the project aims to follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- `im-core`, an input engine with no dependency on terminals or X11: tab-separated
  dictionaries, a sorted code index with exact / prefix / completion matching,
  layering TOML configuration, key bindings lowered to a front-end neutral
  `KeyEvent`, and the 全拼, 自然码双拼 and 小鹤双拼 schemas as data.
- `im`, a command line interface: `im query` for scripts and an interactive
  candidate picker that draws on `/dev/tty` and writes to stdout.
- A dictionary of 164,785 words and 420 syllables generated from
  [pinyin-data](https://github.com/mozillazg/pinyin-data),
  [phrase-pinyin-data](https://github.com/mozillazg/phrase-pinyin-data) and the
  [jieba](https://github.com/fxsjy/jieba) word frequencies, with a reproducible
  pipeline in `xtask`.

[Unreleased]: https://github.com/qjsoong/im/commits/main
