# Changelog

All notable changes to this project are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the project aims to follow [Semantic Versioning](https://semver.org/).

## [0.1.0] - 2026-10-14

### Added

- `im-core`, an input engine with no dependency on terminals or X11: tab-separated
  dictionaries, a sorted code index with exact / prefix / completion matching,
  layering TOML configuration, key bindings lowered to a front-end neutral
  `KeyEvent`, and the 全拼, 自然码双拼 and 小鹤双拼 schemas as data.
- `im`, a command line interface: `im query` for scripts and an interactive
  candidate picker that draws on `/dev/tty` and writes to stdout.
- `im-xim`, an X Input Method server: applications find it through
  `XMODIFIERS=@im=im`, so xterm, st, GTK 3, Qt and Emacs can type Chinese with
  no desktop environment, no D-Bus and no per-toolkit module. It draws its own
  candidate window, which is what clients asking for `XIMPreeditNothing`
  require.
- A `[xim]` configuration table for the X11 front end: the Chinese/ASCII toggle
  key, the mode to start in, what the XIM preedit contains, and whether and at
  what size to draw the candidate window.
- `im_core::loader`, which resolves directories, layers configuration and merges
  dictionaries, so every front end behaves identically.
- A dictionary of 164,785 words and 420 syllables generated from
  [pinyin-data](https://github.com/mozillazg/pinyin-data),
  [phrase-pinyin-data](https://github.com/mozillazg/phrase-pinyin-data) and the
  [jieba](https://github.com/fxsjy/jieba) word frequencies, with a reproducible
  pipeline in `xtask`.

[0.1.0]: https://github.com/qjsoong/im/releases/tag/v0.1.0
