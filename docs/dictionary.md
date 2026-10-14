# Dictionaries

A dictionary is a UTF-8, tab-separated text file. That format was chosen so
that dictionaries stay readable, greppable, diffable and mergeable with the
usual Unix tools: `grep` finds a word, `sort` tidies a file, `comm` compares
two.

A dictionary is not tied to a scheme. It stores pinyin spellings; the schema
folds them into codes when the index is built. See [`schemas.md`](schemas.md)
for the other half of that arrangement, and [`design.md`](design.md) for why it
is split this way.

## File format

Blank lines and lines whose first non-whitespace character is `#` are ignored.
Every other line has three tab-separated fields: `text`, `spelling`, `weight`.

```text
# text  spelling  weight
中      zhong     15402
中国    zhong guo 12271
你      ni        13749
```

- **`text`** is the word that gets committed. It must not be empty and must not
  contain whitespace.
- **`spelling`** is the pronunciation, as space-separated pinyin syllables. For
  a table-based scheme it is the code itself, written the same way. It cannot
  be empty.
- **`weight`** is an integer in `0..=65535`, higher meaning more likely. The
  third field may be omitted entirely, in which case the weight is `0`.

Fields are trimmed, so `中\t  zhong  \t 5 ` is the same record as
`中\tzhong\t5`.

### Canonical spellings

Spellings are normalized when the file is read, so hand-written pinyin does not
have to be in the stored form. The canonical form is lowercase ASCII, tones
removed, `ü` written as `v`, and syllables separated by single spaces.

| Written | Stored | Why |
| --- | --- | --- |
| `zhōng guó` | `zhong guo` | tone marks are folded away |
| `lǜ`, `lü`, `lv` | `lv` | `ü` is written `v` |
| `nǚ` | `nv` | same, after an initial |
| `xi'an`, `xi’an` | `xi an` | `'` separates syllables |
| `  Xi  An  ` | `xi an` | case folded, spaces collapsed |
| `ê`, `ń`, `ḿ` | `e`, `n`, `m` | marked consonants lose the mark |

The four characters treated as separators are `'` (U+0027), `’` (U+2019), `ʼ`
(U+02BC) and `ʹ` (U+02B9); any Unicode whitespace separates syllables as well,
and runs of separators collapse to one space. Everything else has to be an
ASCII letter or digit. Digits are part of the alphabet, so a spelling that
contains one keeps it: `ni3` is stored and typed as `ni3`. The generated
dictionary never contains a digit in a spelling.

### What is rejected

The first bad line stops the load with its file and line number. The full
message is `<path>:<line>: <message>`, and the command line adds the context of
the file it was reading:

```console
$ im --dictionary /tmp/mine.tsv query zhong
im: reading /tmp/mine.tsv: /tmp/mine.tsv:1: weight "nope" is not an integer in
0..=65535
```

| Problem | Message |
| --- | --- |
| more than three fields | `expected 3 tab-separated fields` |
| only a `text` field, or an empty spelling | `empty spelling field` |
| whitespace inside the text field | `text field must not contain whitespace` |
| a character outside the spelling alphabet | see below |
| a spelling that normalizes to nothing | `invalid spelling "'"` |
| a weight that is not an integer in range | see below |

The two long messages are worth seeing in full:

```text
invalid spelling "zh ong!": invalid character '!' in spelling
weight "nope" is not an integer in 0..=65535
```

Because each line is trimmed of leading whitespace before it is split, a line
that is blank apart from spaces or tabs is skipped rather than reported.

## Weight

`weight` is a `u16`: `0` to `65535`, higher is more likely. Entries in the
shipped dictionary use the whole range — `啊` is `12990`, `中国` is `12271`, and
a rare character is `3000`. A user entry starts at `0` unless you pass `-w`.

Weights are only ever compared with each other. They order the candidates for
one code, and they break ties between two records that fold to the same code;
nothing else depends on their absolute value.

## Origin and the user bonus

Every entry carries an `Origin`, either `system` (shipped or installed) or
`user` (from your own dictionary). The ranking weight is the record's weight
plus a bonus for its origin: `0` for a system entry, and `1 << 20` (1,048,576)
for a user entry.

That bonus is larger than the largest possible weight, so a user entry always
outranks a shipped one, even at weight `0`. It is what makes `user.tsv` a
statement rather than a suggestion.

## Search order and merging

A schema names its dictionaries, for example `dictionaries = ["pinyin"]`. For
each name, in order:

1. `dict/<name>.tsv` is loaded from every data directory, lowest precedence
   first. A file in a higher-precedence directory does not replace the lower
   one; both are loaded and merged, so a small supplemental file can extend a
   large installed one.
2. If no data directory has such a file, the copy of that dictionary compiled
   into the binary is used. This build embeds `pinyin`, so `im` answers queries
   with nothing installed at all. An empty search is an error that names the
   directories it looked in.
3. `user.tsv` from the user's data directory is loaded last.

`--dictionary NAME` replaces the schema's list, and `--dictionary ./mine.tsv`
loads a file from a path instead of by name, which is how to try a dictionary
out without installing it.

All the loaded dictionaries are folded and sorted into a single index. Records
are keyed by `(code, text)`; within one code the highest-ranked record for a
text is the one that survives, where ranking is weight descending, then shorter
code, then load order. Two consequences:

- A user entry with the same `(text, spelling)` as a shipped one gives the same
  code, so the user's record **replaces** the shipped record outright — its
  weight is used, not the larger of the two.
- A user entry with a *different* spelling gives a different code, so the word
  becomes reachable a second way instead of replacing the first.

```console
$ printf '中\tzzz\t1\n' > "${XDG_DATA_HOME:-~/.local/share}/im/user.tsv"
$ im query --format tsv -n 1 zzz
zzz	exact	中	zzz	1048577	user
$ im query --format tsv -n 1 zhong
zhong	exact	中	zhong	15402	system
```

The `1048577` is the user's weight `1` plus the `1 << 20` bonus.

## The user dictionary

Your own dictionary is a normal dictionary file:

```text
$XDG_DATA_HOME/im/user.tsv          by default ~/.local/share/im/user.tsv
```

It is always loaded last, for every schema, and it is rewritten in canonical
form after each edit: the two header lines, then one record per line sorted by
spelling, then weight descending, then text, with duplicate `(text, spelling)`
pairs removed. That keeps it a good `git diff` target.

Four commands manage it.

```console
$ im dict path
/home/u/.local/share/im/user.tsv
```

`im dict path` prints the file's location, whether or not it exists.

```console
$ im dict add 词 "ci"
/home/u/.local/share/im/user.tsv
$ im dict add 中国          # no spelling: reuse the shipped one
/home/u/.local/share/im/user.tsv
$ im dict list
词	ci	0
中国	zhong guo	12271
```

`im dict add TEXT [SPELLING] [-w WEIGHT]` adds a word or raises the weight of
one you already added. When the spelling is omitted, the word is looked up in
the loaded dictionaries and the spelling and weight found there are reused; if
it is not known anywhere, the command fails and asks for the spelling rather
than guessing. Adding a word that already exists keeps the higher weight.

```console
$ im dict remove 词
/home/u/.local/share/im/user.tsv
$ im dict remove 无
im: "无" is not in your dictionary
```

`im dict remove TEXT` drops every entry for that word. Removing a word that is
not there is not an error; it reports it on stderr and exits successfully.

## How the shipped dictionary is generated

`data/dict/` holds two generated files:

| File             | Contents                                            |
| ---------------- | --------------------------------------------------- |
| `pinyin.tsv`     | 164,785 records: word, toneless pinyin, weight       |
| `syllables.txt`  | the 420 distinct pinyin syllables `pinyin.tsv` uses  |

`pinyin.tsv` is 3,781,225 bytes (3.61 MiB) and is compiled into `im` by
`include_str!`. It is derived from three upstream projects:

- `mozillazg/pinyin-data` — every single-character reading;
- `mozillazg/phrase-pinyin-data` — every multi-character reading, and the
  hand-curated list used for a bonus;
- `fxsjy/jieba` — word frequencies, which give the weights their order.

The selection rules are frozen: every single-character reading is kept, every
two-character word the jieba word list knows is kept, and a word of three or
more characters needs a corpus frequency of at least 3. The weight is

```text
weight = min(65535, round(1000 * ln(max(jieba_freq, 1)))
             + 3000 * single_char + 500 * curated)
```

so frequency is compressed logarithmically, single characters get a bonus that
keeps them ahead of rare words, and a hand-checked reading gets a smaller one.
The pipeline refuses to write a file above its 4 MiB budget.

Records are deduplicated on `(text, spelling)` keeping the greatest weight, then
ordered by spelling, weight descending, then text. That order is total, so
regenerating from the same sources is byte-for-byte reproducible: a review of a
data change is a `git diff` of the file.

```console
$ cargo run -p xtask -- dict fetch   # download the sources, needs curl
$ cargo run -p xtask -- dict build   # write pinyin.tsv and syllables.txt
$ cargo run -p xtask -- dict check   # validate both files, offline
```

`dict check` re-reads both files and verifies every rule above, reporting the
file and line number of the first violation. Because it needs no network
access, CI runs it on every pull request, and it is also the right command to
run after editing a dictionary by hand. It is stricter than the loader: it
requires the canonical sort order, rejects `\r`, and insists that a spelling
contain only letters while the engine would accept a digit.

The format rules and the regeneration steps are documented in
[`data/dict/README.md`](../data/dict/README.md).

## Licence of the data

`data/dict/pinyin.tsv` and `data/dict/syllables.txt` are a derived work of the
three MIT-licensed projects above, one of which incorporates CC-CEDICT, and
CC-CEDICT is share-alike. The generated data is therefore distributed under
**Creative Commons Attribution-ShareAlike 4.0**, while the code that builds and
reads it — `im` and `xtask` — remains **MIT OR Apache-2.0**.

If you redistribute a modified dictionary, say where it came from and licence
your version under the same terms. The full attribution, and what share-alike
means in practice, is in
[`data/dict/NOTICE.md`](../data/dict/NOTICE.md).

## Growing your own dictionary

Hand-written entries belong in `user.tsv`, which is what `im dict add` edits:

```text
词	ci	200
囧	jiong	10
```

Because the file is read back and rewritten in canonical form, the simplest way
to add several words at once is to append lines and let the next `im dict add`
or `im dict remove` tidy them up. Sorting and deduplicating it by hand is safe
for the same reason: the loader keeps file order and imposes no order of its
own, and the index resolves duplicates by ranking, so running `sort` or `uniq`
over a dictionary can change how tidy the file looks but never which words the
program finds.

A larger body of words that is not personal — a technical glossary, a set of
names — is better kept as a dictionary file of its own, for example
`~/.local/share/im/dict/mine.tsv`, and named from a schema:

```toml
# ~/.config/im/schemas/pinyin.toml
id = "pinyin"
name = "全拼 + 我的词库"
kind = "syllable"
dictionaries = ["pinyin", "mine"]
```

Both files are then loaded and merged. This is the same mechanism a
distribution uses to install extra dictionaries, and it keeps the words
reusable across schemes: `mine.tsv` stores spellings, so 自然码 and 小鹤 reach
the same words without a second copy.
