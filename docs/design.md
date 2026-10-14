# Design

`im` is a Chinese input method built out of ordinary Unix parts: a text
configuration file, a tab-separated dictionary you can `grep`, a candidate
picker that draws on the terminal and writes to stdout, and an engine that
never touches a terminal, an X server or an event loop.

This document explains the data model that holds those pieces together and why
the engine is shaped the way it is. The file formats have their own references:
[`config.md`](config.md), [`schemas.md`](schemas.md) and
[`dictionary.md`](dictionary.md).

## One idea: a code

Everything funnels through one sentence: **a code is a short ASCII string**.

A dictionary record is a word, the way it is spelled, and how common it is. The
engine never searches for spellings; it searches for codes. A schema *folds*
each spelling into the exact keystrokes a user has to type, and the index is a
sorted array of the resulting codes.

```text
data/dict/pinyin.tsv      text      spelling        weight
                          中国      zhong guo       12271
                            |
                            |  Schema::speller().fold()
                            v
                          中国      "zhongguo"   (pinyin)
                          中国      "vsgo"       (自然码双拼)
```

The spelling is pinyin in both cases. Only the folding changes. That is the
whole trick, and it is why one dictionary serves every scheme:

- **Full pinyin** (`pinyin`) has no tables at all. Folding just deletes the
  spaces, so `zhong guo` becomes `zhongguo`. This is the identity case.
- **Double pinyin** (`double_pinyin`, `xiaohe`) replaces the initial and the
  final of each syllable with one key each, so `zhong guo` becomes `vsgo`.
- **A code table** (wubi, cangjie, stroke) is the degenerate case: the spelling
  *is* the code, and folding is a no-op. Such a scheme is `kind = "table"` and
  ships with its own dictionary whose `spelling` column holds codes.

Because the dictionary only ever stores spellings, adding a scheme is a data
change rather than an engine change. 自然码 and 小鹤 are two TOML files in
`crates/im-core/data/schemas/` and not a line of Rust between them.

## Three kinds of match

`CodeIndex::lookup` returns candidates in three blocks, in this order:

| Kind         | Meaning                                                     |
| ------------ | ----------------------------------------------------------- |
| `exact`      | the record's whole code is what was typed                   |
| `backoff`    | the record's code is a proper prefix of what was typed      |
| `completion` | what was typed is a prefix of the record's code             |

`backoff` candidates are emitted from the longest prefix down to the shortest.
`completion` candidates all share the deepest prefix of the input that indexes
anything, and are ranked by weight like everything else. Within one code group
records are ordered by ranking weight (descending), then by the length of the
code (shorter first), then by load order; the list is then deduplicated by text,
keeping the best occurrence of each word. That is a total order, so the same
input always yields the same list and paging through it is stable.

### A worked example

The shipped dictionary is `data/dict/pinyin.tsv` (164,785 records). Under full
pinyin, every one of these commands is a plain string search over the folded
codes:

```console
$ im --schema pinyin query --format tsv -n 3 zhong
zhong	exact	中	zhong	15402	system
zhong	exact	种	zhong	12930	system
zhong	exact	重	zhong	12663	system
```

Typing `zhong` matches 86 records whose code is exactly `zhong`; after those,
the completion block offers words that start with the same keys:

```console
$ im --schema pinyin query --format tsv -n 100000 zhong \
    | awk -F'\t' '$2 == "completion"' | head -3
zhong	completion	中国	zhong guo	12271	system
zhong	completion	重要	zhong yao	11034	system
zhong	completion	中心	zhong xin	10085	system
```

Typing a whole two-syllable code gives an exact match first and then falls back
to shorter prefixes of what was typed:

```console
$ im --schema pinyin query --format tsv -n 8 zhongguo
zhongguo	exact	中国	zhong guo	12271	system
zhongguo	backoff	中古	zhong gu	5427	system
zhongguo	backoff	钟鼓	zhong gu	4431	system
zhongguo	backoff	忠骨	zhong gu	2446	system
zhongguo	backoff	中谷	zhong gu	1099	system
zhongguo	backoff	终古	zhong gu	1099	system
zhongguo	backoff	中	zhong	15402	system
zhongguo	backoff	种	zhong	12930	system
```

The records with code `zhong gu` come before those with code `zhong`: the
longer prefix of the input is the more literal reading. A typo is handled the
same way — `zhongxq` indexes nothing, but `zhongx` does, so the candidates are
the 86 characters for `zhong` followed by the completions of `zhongx`:

```console
$ im --schema pinyin query --format tsv -n 100000 zhongxq \
    | awk -F'\t' '$2 == "completion"' | head -2
zhongxq	completion	中心	zhong xin	10085	system
zhongxq	completion	中学	zhong xue	9029	system
```

Switch the schema to 自然码 and the same dictionary answers different keys with
the same words. `vs` folds to the same code as `zhong`, so it produces exactly
the same 86 exact matches and 935 completions:

```console
$ im --schema double_pinyin query --format tsv -n 3 vs
vs	exact	中	zhong	15402	system
vs	exact	种	zhong	12930	system
vs	exact	重	zhong	12663	system

$ im --schema double_pinyin query --format tsv -n 4 vsgo
vsgo	exact	中国	zhong guo	12271	system
vsgo	backoff	中	zhong	15402	system
vsgo	backoff	种	zhong	12930	system
vsgo	backoff	重	zhong	12663	system
```

`vsgo` is `zhong guo` compressed to two keys per syllable. It produces one
exact match and 86 backoff candidates, but no fourth-level backoff, because
there is no word spelled `zhong gu`.

## `consumed`, and why committing is incremental

Every candidate carries `consumed`: the number of bytes of the normalized input
it accounts for. `Session::commit` removes exactly those bytes from the front of
the input buffer and re-searches what is left, returning the remainder as
`Commit::rest`.

| Kind         | `consumed`                                            |
| ------------ | ----------------------------------------------------- |
| `exact`      | the whole input                                       |
| `backoff`    | the length of the prefix that equals the record's code|
| `completion` | the whole input                                       |

This is what lets a phrase be typed as one run of keys rather than one word at
a time. Type `zhongguo`, pick 中, and the session keeps `guo` as its input for
the next lookup; type `vsgo`, pick 中, and `go` is left. The picker starts with
`> 中国` highlighted, so selecting it commits the whole word and finishes;
selecting 中 instead commits one word and keeps composing. Both behaviours fall
out of the same `consumed` rule, which is why `Commit` also reports the
remainder: a one-shot prompt such as the picker can tell what it dropped.

## Why a sorted array

The index is a `Vec` of records sorted by code. Lookup is a binary search, and
that is the entire data invariant — there is no trie, no hash map and no
compressed structure to reason about. Three things follow.

- The array is compact and scanned in order, so a search touches few cache
  lines. It is also cheap to build: no insertion into a prefix tree, just one
  sort.
- "Sorted by code" is easy to test. `CodeIndex::build` sorts and the only other
  operation that touches the array is `partition_point`.
- Prefix ranges need no per-node bookkeeping. A half-open range of codes with a
  given prefix is the range between two `lower_bound` calls.

That last trick is worth spelling out, because it depends on a property of
codes. Codes only ever contain `[a-z0-9]`, guaranteed by `Speller::validate`
rejecting table values outside that alphabet. So:

- `code` + `'\0'` sorts after `code` itself and before any longer string with
  the same prefix, which gives the half-open range of codes *equal* to `code`.
- `code` + `'{'` sorts after every string starting with `code`, because `{`
  (U+007B) is greater than `z` (U+007A) and than every digit, which gives the
  range of codes *starting with* `code`.

Two string comparisons, no traversal. If a schema were allowed to fold a
syllable to `{` or to a non-ASCII character, prefix ranges would silently become
wrong; the validation at schema load time is what makes the shortcut sound.

Typed input is cleaned before it is matched: `sanitize_code` drops everything
that is not an ASCII alphanumeric and lowercases the rest, so `Zhong Guo` and
`zhongguo` are the same query.

## The layering

`im-core` holds the model and nothing front-end specific. It reads and writes
the text files that hold configuration, schemas and dictionaries, and otherwise
it is pure logic: push a key, read a candidate list, commit one. A front end
never has to know how a schema folds or how the index is arranged.

`im-cli` is the `im` binary and adds three things on top:

- **Directory discovery and loading.** `im_core::Loader` resolves the
  XDG-style directories, merges `im.toml`, loads the schema catalog and reads
  every dictionary the chosen schema names. It is the only place that knows
  where files live, which is why two front ends can be sure they are looking at
  the same installation.
- **A batch interface.** `im query` prints candidates for codes given on the
  command line or one per line on stdin, in `text` or `tsv`, which makes the
  engine usable from a script or from the far end of a pipe.
- **A terminal picker.** The picker drives a `Session`: it translates crossterm
  key events into `im_core::KeyEvent`, looks the event up in the `Keymap` from
  `im.toml`, and draws a four-row layout (header, candidate grid, prompt,
  footer) on `/dev/tty`. The UI never writes to stdout; only the accepted text
  does, which is why `x=$(im)` and `im > note.txt` work.

The X11 front end, `im-xim`, sits at the same seam: it serves XIM, so
applications find it through `XMODIFIERS`, and it draws the candidates inside
the XIM preedit string rather than in a window of its own.

## What it costs

`cargo run --release -p im-core --example timing` measures the start-up path
against the shipped dictionary. A representative run:

```console
$ cargo run --release -p im-core --example timing
data/dict/pinyin.tsv: 3781225 bytes
parse:    22.5ms  164785 entries
index [pinyin]:   41.2ms  164785 codes
lookup (4 codes):  121.2µs  400 candidates
index [double_pinyin]:   69.5ms  164784 codes
lookup (4 codes):  101.0µs  365 candidates
index [xiaohe]:   67.7ms  164784 codes
lookup (4 codes):   88.0µs  365 candidates
```

Measured on an AMD Ryzen 9 7940H with rustc 1.99.0 in a release build, so treat
the numbers as an order of magnitude rather than a specification; they move by
roughly 20% between runs. The shape is what matters: parsing is one pass over
3.6 MiB, folding and sorting dominate, and an individual lookup is in the tens
of microseconds. Folding under a double-pinyin scheme is slower than under the
identity because each syllable is split and looked up in two tables rather than
concatenated.

Note that the double-pinyin index has one code fewer than the full-pinyin index.
Two spellings of 咯 — `lo` and `luo` — fold to the same code, and the index
keeps the higher-ranked record for a repeated `(code, text)` pair.

## Known limitations

The engine is deliberately small, and some things a mature input method does
are simply not here yet.

- **Word-level, not sentence-level.** Input is matched against whole dictionary
  codes. There is no language model, no segmentation of a long keystroke run
  into words, and no grammar; a phrase is built by committing word after word,
  each commit consuming the keys it accounted for.
- **No learning.** Committing a candidate does not reorder anything.
  `Commit::spelling` is exposed so a front end could keep history, and
  `user.tsv` lets a person raise a word's weight explicitly, but nothing adapts
  on its own.
- **No fuzzy pinyin.** `zh`/`z` and `n`/`l` are distinct; there is no table of
  near-miss spellings. A syllable a scheme's tables do not cover is passed
  through unchanged instead of being approximated.
- **No index cache.** Every run parses the dictionary and rebuilds the index.
  There is no on-disk cache to invalidate and no background daemon, which keeps
  start-up predictable and stateless, at the cost of the numbers above.
