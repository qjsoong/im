# Pinyin dictionary data

`data/dict/` holds the pinyin dictionary that `im` ships with. Both files are
generated from upstream data; do not edit them by hand.

| File | Contents |
| --- | --- |
| `pinyin.tsv` | 164,785 records: word, tone-free pinyin spelling, weight |
| `syllables.txt` | the 420 distinct pinyin syllables used by `pinyin.tsv` |

## Format of `pinyin.tsv`

UTF-8 with LF line endings. Lines starting with `#` are comments, blank lines
are ignored, and every other line has three TAB-separated fields:

```text
text<TAB>spelling<TAB>weight
中国	zhong guo	12271
```

- `text` is a word of Han characters only, from U+3400..U+4DBF,
  U+4E00..U+9FFF or U+F900..U+FAFF. ASCII, digits, punctuation, whitespace,
  kana and emoji never occur in this field.
- `spelling` is lowercase ASCII pinyin without tones, with syllables separated
  by single spaces. `ü` is written as `v`, so `女` is `nv`, `略` is `lve` and
  `绿` is `lv`. The number of syllables always equals the number of characters
  in `text`.
- `weight` is an integer in 0..=65535; a greater weight means a more common
  word.

Records are unique per `(text, spelling)` pair and ordered by `spelling` (byte
order), then `weight` descending, then `text` (byte order). That order is
total, so regenerating the file from the same sources is byte-for-byte
reproducible.

`syllables.txt` lists the distinct syllables of the same generation run, one
per line, in byte order, after a short `#` header.

## How `weight` is derived

```text
weight = min(65535, round(1000 * ln(max(jieba_freq, 1)))
             + 3000 * single_char + 500 * curated)
```

- `jieba_freq` is the frequency of the word in the jieba word list, or 1 when
  the list does not contain it (which contributes 0).
- `single_char` is 1 for one-character records. The +3000 bonus keeps single
  characters, which a user reaches by typing one syllable, ahead of rarely
  used multi-character words.
- `curated` is 1 when the same reading also occurs in the hand-curated
  `phrase-pinyin-data` `pinyin.txt`. The +500 bonus marks hand-checked
  readings.

The logarithm compresses the corpus frequency range of 2..883,634 into roughly
700..13,700, so a word used ten times more often is only marginally heavier.
Ties are resolved by the record order described above.

## What is included

- Every single-character reading upstream has a pinyin for: 34,161 records.
- Every two-character word the jieba word list knows: 72,996 records.
- Words of three or more characters with a jieba frequency of at least 3:
  57,628 records. Longer entries need corpus evidence because they take more
  space and are reached less often than two-character words.

`dict build` fails instead of writing a file larger than 4 MiB (4,194,304
bytes); the current file is 3,781,225 bytes (3.61 MiB). Upstream covers far
more characters than this format accepts, so 17,723 of its 44,435
single-character lines are dropped for using a character outside those three
blocks (16,298 Extension B-F and compatibility-supplement ideographs, 1,354
Extension G ideographs, 70 private-use characters, and `〇`), as are 33 phrase
lines whose phrase contains such a character.

## Regenerating

```sh
cargo run -p xtask -- dict fetch   # download sources into target/dict-src
cargo run -p xtask -- dict build   # write data/dict/pinyin.tsv and syllables.txt
cargo run -p xtask -- dict check   # validate both files, offline
```

`dict fetch` needs `curl` on `PATH`, retries each download three times, and
skips sources that are already complete. `dict build` takes about 3 seconds and
prints the counts it read and filtered, plus the file size, to stderr.

`dict check` re-reads both files and validates every rule above, reporting the
file and line number of the first violation. It uses no network access, so CI
can run it on any checkout that contains `data/dict/`.

## Sources

The data is derived from three MIT-licensed projects, fetched from their
`master` branches:

| Local file | Upstream | Version | SHA-256 of the fetched file |
| --- | --- | --- | --- |
| `pinyin-data-pinyin.txt` | mozillazg/pinyin-data | 0.15.0 | `621f8ca9eff8519f47e2b17b564fd318161e13bca07eea8c8e04993cd5d3b52e` |
| `phrase-pinyin-data-large_pinyin.txt` | mozillazg/phrase-pinyin-data | 0.19.0 | `4cf565635a092f3911a7f560fc604aa1a05c95a121488fb28562f3dd2d1441f7` |
| `phrase-pinyin-data-pinyin.txt` | mozillazg/phrase-pinyin-data | 0.19.0 | `dcc769607c220b312fea3e71cb63421298b4b891b1f7356a95ab58f2c96fff81` |
| `jieba-dict.txt` | fxsjy/jieba | no version header | `7197c3211ddd98962b036cdf40324d1ea2bfaa12bd028e68faa70111a88e12a8` |

Because `dict fetch` follows `master`, regenerating at a later date can pick up
upstream changes and produce a different file.

## License

These files are **Creative Commons Attribution-ShareAlike 4.0**, not MIT like
the rest of the repository, because one of the sources they incorporate
(CC-CEDICT, reached through the `cc_cedict.txt` files upstream) is share-alike.
See `NOTICE.md` for the full attribution and for what that means in practice.
If you need a permissively licensed dictionary, `xtask/src/dict/` is written so
that you can drop in a different source without touching the engine.
