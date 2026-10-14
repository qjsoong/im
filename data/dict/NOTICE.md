# Attribution and licenses for `data/dict/`

The generated files `pinyin.tsv` and `syllables.txt` are a derived work of
three MIT-licensed upstream projects. The tables below record what was used,
what those projects are licensed under, and what their own documentation says
about where their data comes from. All upstream material was retrieved on
2026-10-14.

## Upstream sources

### mozillazg/pinyin-data

- Used file: `pinyin.txt` (upstream header: `version: 0.15.0`), fetched from
  <https://raw.githubusercontent.com/mozillazg/pinyin-data/master/pinyin.txt>.
- Contributions: every single-character reading in `pinyin.tsv`.
- License: MIT, `Copyright (c) 2016 mozillazg`
  (<https://github.com/mozillazg/pinyin-data/blob/master/LICENSE>).
- Provenance stated by the upstream `README.md`: `pinyin.txt` is the merge of
  that project's Unihan-derived files (`kTGHZ2013`, `kHanyuPinyin`,
  `kXHC1983`, `kHanyuPinlu`, `kMandarin`), hand-corrected files, GB 18030
  private-use-area characters, characters outside the CJK Unified Ideographs
  block that have pinyin, readings for Japanese kanji, the *Table of General
  Standard Chinese Characters* (`通用规范汉字表`) readings, data from
  `zdic.net` (汉典网), and `cc_cedict.txt`, which is generated from CC-CEDICT
  word and character data (`cc-cedict.org`).
- The upstream README records the Unihan snapshot it merged as
  `Date: 2025-07-24 GMT, Unicode Version: 17.0.0`. Unihan files are distributed
  by the Unicode Consortium under the Unicode License v3
  (<https://www.unicode.org/license.txt>).
- No mention of libpinyin or libpinyin-derived data appears in the upstream
  `README.md`.

### mozillazg/phrase-pinyin-data

- Used files: `large_pinyin.txt` and `pinyin.txt` (both `version: 0.19.0`),
  fetched from
  <https://raw.githubusercontent.com/mozillazg/phrase-pinyin-data/master/large_pinyin.txt>
  and
  <https://raw.githubusercontent.com/mozillazg/phrase-pinyin-data/master/pinyin.txt>.
- Contributions: every multi-character reading in `pinyin.tsv`, plus the
  hand-curated bonus for readings that appear in the smaller `pinyin.txt`.
- License: MIT, `Copyright (c) 2017 mozillazg`
  (<https://github.com/mozillazg/phrase-pinyin-data/blob/master/LICENSE>).
- Provenance stated by the upstream `README.md`: the hand-curated `pinyin.txt`
  is the project's `overwrite.txt` applied to its curated list, and
  `large_pinyin.txt` `pinyin.txt` merges that file with `zdic_cibs.txt` and
  `zdic_cybs.txt` (from `zdic.net`, 汉典网), `cc_cedict.txt` (from
  `cc-cedict.org`), and `di.txt`. The README also states that the initial data
  is based on `phrases-dict.js` from `hotoo/pinyin` (MIT,
  <https://github.com/hotoo/pinyin/blob/master/LICENSE>) and on
  `phrases_dict.py` from `mozillazg/python-pinyin` (MIT, `Copyright (c) 2016
  mozillazg, 闲耘 <hotoo.cn@gmail.com>`,
  <https://github.com/mozillazg/python-pinyin/blob/master/LICENSE.txt>).
- No mention of libpinyin or libpinyin-derived data appears in the upstream
  `README.md`.

### fxsjy/jieba

- Used file: `jieba/dict.txt`, fetched from
  <https://raw.githubusercontent.com/fxsjy/jieba/master/jieba/dict.txt>. The
  file carries no version header.
- Contributions: word frequencies, and with them the `weight` ordering of
  every record in `pinyin.tsv`. No wording or spelling is taken from this
  project, and its part-of-speech tags (the third field, which `im` ignores)
  are not part of the derived work.
- License: MIT, `Copyright (c) 2013 Sun Junyi`
  (<https://github.com/fxsjy/jieba/blob/master/LICENSE>); the upstream
  `README.md` states "MIT License".
- The upstream README describes the word list as a general-purpose
  segmentation dictionary and states that part-of-speech tags follow
  ICTCLAS-compatible conventions. It does not claim a specific corpus
  provenance for `dict.txt`, and it does not mention libpinyin or
  libpinyin-derived data.

### libpinyin

None of the three upstream `README.md` files mentions libpinyin or any
libpinyin-derived data, so no libpinyin attribution is included here.

## License of the generated files

`data/dict/pinyin.tsv`, `data/dict/syllables.txt` and the `xtask dict` pipeline
that produces them are a **derived work of all the sources above**, and one of
those sources carries a share-alike term:

- the readings that entered through `cc_cedict.txt` come from CC-CEDICT, which
  is licensed **Creative Commons Attribution-ShareAlike 4.0**
  (<https://creativecommons.org/licenses/by-sa/4.0/>).

A derivative of CC BY-SA material has to be distributed under CC BY-SA, and the
MIT grant from `mozillazg` cannot relicense the CC-CEDICT content that project
also redistributes. So the derived data files are distributed under
**Creative Commons Attribution-ShareAlike 4.0**, not MIT. In practice this
means: use them, modify them, redistribute them, including commercially — but
if you redistribute a modified dictionary you must say where it came from and
license your version under the same terms.

The *code* of `im`, including `xtask`, remains MIT OR Apache-2.0; only the data
files in this directory are CC BY-SA.

Attribution, as CC BY-SA 4.0 section 3(a) requires. The derived file incorporates
material from:

- CC-CEDICT (<https://cc-cedict.org/>), CC BY-SA 4.0, and the CC-CEDICT editor
  team.
- The Unihan Database (`kTGHZ2013`, `kHanyuPinyin`, `kXHC1983`, `kHanyuPinlu`,
  `kMandarin`), © Unicode, Inc., distributed under the Unicode License v3
  (<https://www.unicode.org/license.txt>), which permits use, modification and
  redistribution provided the copyright notice is retained. Unihan snapshot
  used upstream: 2025-07-24, Unicode 17.0.0.
- `zdic.net` (汉典网) readings, as redistributed by both `mozillazg` projects.
- `通用规范汉字表` (2013) readings, and the `kMandarin_8105.txt`,
  `kMandarin_overwrite.txt`, `GBK_PUA.txt`, `nonCJKUI.txt` and `kanji.txt`
  hand-corrected files of `mozillazg/pinyin-data`.
- `hotoo/pinyin` and `mozillazg/python-pinyin`, MIT, for the initial phrase list.
- `fxsjy/jieba`, MIT, Copyright (c) 2013 Sun Junyi, for word frequencies.
- `mozillazg/pinyin-data`, MIT, Copyright (c) 2016 mozillazg.
- `mozillazg/phrase-pinyin-data`, MIT, Copyright (c) 2017 mozillazg.

The MIT notices above are reproduced in full because the MIT license requires
it, even though the combined result is distributed under CC BY-SA 4.0:

> The MIT License (MIT)
>
> Copyright (c) 2016 mozillazg (mozillazg/pinyin-data)
> Copyright (c) 2017 mozillazg (mozillazg/phrase-pinyin-data)
> Copyright (c) 2013 Sun Junyi (fxsjy/jieba)
> Copyright (c) 2016 mozillazg, 闲耘 <hotoo.cn@gmail.com>
>   (hotoo/pinyin, mozillazg/python-pinyin)
>
> Permission is hereby granted, free of charge, to any person obtaining a copy
> of this software and associated documentation files (the "Software"), to deal
> in the Software without restriction, including without limitation the rights
> to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
> copies of the Software, and to permit persons to whom the Software is
> furnished to do so, subject to the following conditions:
>
> The above copyright notice and this permission notice shall be included in
> all copies or substantial portions of the Software.
>
> THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
> IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
> FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
> AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
> LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
> OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
> SOFTWARE.

This is a statement of what the upstream projects say about their own data, not
legal advice. If CC BY-SA is a problem for your use, the pipeline in
`xtask/src/dict/` is written so that a different source can be dropped in
without touching the engine — see `docs/dictionary.md`.
