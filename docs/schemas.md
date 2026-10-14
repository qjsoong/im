# Input schemes

An input scheme is a small TOML file that says two things: how keystrokes
become codes, and which dictionaries to search. Everything that makes 全拼
different from 自然码 lives in that file; nothing in the engine is
scheme-specific.

Three schemes ship with `im`:

| id              | Name       | Kind       | Two keys per syllable?        |
| --------------- | ---------- | ---------- | ----------------------------- |
| `pinyin`        | 全拼        | `syllable` | no, the spelling is the code  |
| `double_pinyin` | 自然码双拼   | `syllable` | yes                           |
| `xiaohe`        | 小鹤双拼    | `syllable` | yes                           |

`im schema list` shows them, marking the active one with `*`, and
`im schema show <id>` prints any of them as TOML.

## File format

```toml
id = "double_pinyin"
name = "自然码双拼"
description = "Natural Code double pinyin.
    Two keys per syllable, for example 'vsgo' for 中国."
kind = "syllable"
dictionaries = ["pinyin"]
page_size = 9

[speller]
initials = ["b", "p", "m", "zh", "ch", "sh"]

[speller.initial_keys]
zh = "v"

[speller.finals]
ong = "s"

[speller.null_initials]
an = "aj"
```

| Field | Type | Default | Meaning |
| --- | --- | --- | --- |
| `id` | string | required | identifier used by `--schema` and `im.toml` |
| `name` | string | required | human-readable name |
| `description` | string | `""` | shown by `im schema list` |
| `kind` | `syllable` or `table` | `syllable` | folding mode |
| `dictionaries` | array of strings | `["pinyin"]` | dictionary names to load |
| `page_size` | integer >= 1 | unset | candidates per page |
| `speller` | table | empty | folding tables, see below |

There are no other fields. The file is parsed with
`#[serde(deny_unknown_fields)]`, so a typo is an error rather than a silently
ignored key:

```console
$ im schema show x
im: ...: schema x: unknown field `dictionaries2`, expected one of `id`,
`name`, `description`, `kind`, `dictionaries`, `page_size`, `speller`
```

`id` and `name` must not be empty, `page_size` must be at least 1, and the
speller tables must contain only typeable keys — that last rule is covered
below. A file that fails to parse stops `im` with the file named, so a broken
user schema is never loaded quietly.

`dictionaries` names dictionaries, not files: each name resolves to
`dict/<name>.tsv` inside every data directory. See
[`dictionary.md`](dictionary.md).

## `kind = "syllable"` versus `kind = "table"`

The `kind` field decides what folding means.

- **`syllable`** — the spelling is a sequence of pinyin syllables separated by
  spaces, and the `[speller]` tables fold each syllable into keys. This is
  where 全拼, 自然码 and 小鹤 live. With empty tables the fold is the identity:
  `zhong guo` becomes `zhongguo`.
- **`table`** — the spelling *is* the code. Whitespace between the pieces is
  removed and everything else is used literally, so a dictionary record

  ```text
  中国	v go	80
  ```

  is reached by typing `vgo`. This is the shape wubi, cangjie and stroke codes
  take: the `[speller]` table is ignored.

A table scheme is a normal schema plus a dictionary that holds codes:

```toml
id = "table_demo"
name = "Table demo"
kind = "table"
dictionaries = ["table_demo"]
```

```console
$ im --schema table_demo query --format tsv -n 3 vgo
vgo	exact	中国	v go	80	system
vgo	backoff	中	v	100	system
```

Backoff, completion and ranking work exactly as for a syllable scheme, because
by the time a code reaches the index the distinction has already disappeared.

## The `[speller]` tables

Only `kind = "syllable"` schemes use these. A speller with empty `initials` and
empty `finals` is treated as the identity and skips folding entirely.

| Table | Type | Meaning |
| --- | --- | --- |
| `initials` | array of strings | pinyin initials, longest match wins |
| `initial_keys` | table | initials whose key differs from the spelling |
| `finals` | table | final of a syllable to its key |
| `null_initials` | table | whole syllables that have no initial |

### How one syllable is folded

For each whitespace-separated syllable of the spelling:

1. Find the longest entry of `initials` that is a **proper prefix** of the
   syllable. "Proper" means shorter than the syllable itself, which is what
   makes `an` a whole syllable instead of the initial `a` plus a stray `n`.
   Taking the longest match, rather than the first one listed, makes the result
   independent of the order of the array.
2. If no initial matched, look the whole syllable up in `null_initials` and use
   that key. If it is not there either, the syllable is passed through
   unchanged.
3. Otherwise strip the initial from the front. If the initial is `j`, `q`, `x`
   or `y` and what remains starts with `u`, rewrite that `u` as `v`.
4. Append the initial's key — from `initial_keys` if the initial is listed
   there, otherwise the initial itself — followed by the key of the remaining
   final from `finals`.
5. If `finals` has no entry for that final, append the final unchanged. Unknown
   syllables and finals degrade to their own spelling rather than being an
   error, so a dictionary may contain words a scheme does not know about.

Two consequences of the order of these steps are easy to trip over:

- A final is only looked up after an initial has been stripped, so a scheme
  that lists `finals` but no `initials` never reaches the final table.
- The `j q x y` rule applies to the remainder *after* the initial was split
  off, and only to those four initials. `ju` is not "j + u" but "j + ü", so it
  folds as if it were spelled `jv`; `lv` and `nv` are written with `v`
  directly.

### Worked derivation: 自然码

Take the shipped record for 中国:

```text
中国	zhong guo	12271
```

The spelling has two syllables. Fold each one through the tables in
`crates/im-core/data/schemas/double_pinyin.toml`.

**`zhong`.** The listed initials include `z`, `h` and `zh`. Only `z` and `zh`
are proper prefixes of `zhong`, and the longest of those is `zh`.
`initial_keys` maps `zh` to `v`. The remainder is `ong`, and `finals` maps
`ong` to `s`. So `zhong` folds to **`vs`**.

**`guo`.** The only proper-prefix initial is `g`, which has no `initial_keys`
entry, so it keeps its own letter. The remainder is `uo`, and `finals` maps
`uo` to `o`. So `guo` folds to **`go`**.

Concatenating the syllables gives the code for 中国, **`vsgo`**:

```console
$ im --schema double_pinyin query --format tsv -n 1 vsgo
vsgo	exact	中国	zhong guo	12271	system
```

A syllable with no initial goes through `null_initials` instead. `an` has no
initial in the list, and `null_initials` gives it `aj`, so 安 (`an`) is reached
by typing `aj`.

The `j q x y` rule shows up in the ü-series. `jun` splits into the initial `j`
and the remainder `un`; `un` is mapped to `p`, so `jun` folds to `jp` and 军 is
the first match for that code:

```console
$ im --schema double_pinyin query --format tsv -n 1 jp
jp	exact	军	jun	13211	system
```

Meanwhile `ju` splits into `j` and `u`, the `u` becomes `v`, and `finals` maps
`v` to `v`, so 句 folds to `jv` — the `u` never appears.

### How 小鹤 differs

小鹤 uses the same `initials` and the same `initial_keys`, so `zh`, `ch` and
`sh` are still `v`, `i` and `u`. It changes which key each final gets. 15 of
the 35 finals differ:

| final | 自然码 | 小鹤 |
| ----- | ------ | ---- |
| `ai`  | `l`    | `d`  |
| `ei`  | `z`    | `w`  |
| `ao`  | `k`    | `c`  |
| `ou`  | `b`    | `z`  |
| `ia`  | `w`    | `x`  |
| `ie`  | `x`    | `p`  |
| `iao` | `c`    | `n`  |
| `in`  | `n`    | `b`  |
| `iang`| `d`    | `l`  |
| `ing` | `y`    | `k`  |
| `ua`  | `w`    | `x`  |
| `uai` | `y`    | `k`  |
| `un`  | `p`    | `y`  |
| `uang`| `d`    | `l`  |
| `vn`  | `p`    | `y`  |

`null_initials` differs in four places for the same reason: 小鹤 spells `ai` as
`ad`, `ei` as `ew`, `ao` as `ac` and `ou` as `oz`, where 自然码 has `al`, `ez`,
`ak` and `ob`.

The finals that both schemes agree on include `ong` and `uo`, so 中国 is
`vsgo` under both — a coincidence of the two schemes, not a shared rule. Where
they disagree, one key changes. `ao` is `k` in 自然码 and `c` in 小鹤, so 好
(`hao`) is `hk` under one and `hc` under the other:

```console
$ im --schema double_pinyin query -n 1 hk
好
$ im --schema xiaohe query -n 1 hc
好
```

The whole difference between the two schemes is the two tables above; there is
no code that knows either name.

### The validation rule

Every value in `initial_keys`, `finals` and `null_initials`, and every entry of
`initials`, must be a non-empty sequence of lowercase ASCII letters and digits.
Codes only ever contain `[a-z0-9]`, and the index relies on that when it
computes prefix ranges, so a table value outside the alphabet would create a
code that can never be typed. The check happens when the schema is loaded:

```console
$ im schema show x
im: ...: schema x: [speller.finals] ong = "S" is not a sequence of lowercase
letters or digits
```

The same rule rejects an empty value (`zh = ""`) and an uppercase initial
(`initials = ["ZH"]`).

## Adding a scheme

A scheme is a file, and there are two places to put it.

- **User scheme.** Write `$XDG_CONFIG_HOME/im/schemas/<id>.toml`, by default
  `~/.config/im/schemas/<id>.toml`. This is the normal way to try a scheme out.
- **Distribution scheme.** Install `<data dir>/im/schemas/<id>.toml`, for
  example `/usr/share/im/schemas/`. These are searched after the built-ins and
  before the user's directory.
- **Built-in scheme.** Add `crates/im-core/data/schemas/<id>.toml` and register
  the id with its `include_str!` source in the `SOURCES` array in
  `crates/im-core/src/schema/builtin.rs`. Built-ins are compiled into the
  library, which is why `im` works with no files installed at all.

Schemas are discovered by id and a later source replaces an earlier one
outright: a user file at `~/.config/im/schemas/pinyin.toml` shadows the built-in
`pinyin` completely, it does not merge with it. That is how a distribution or a
person can replace a shipped scheme without renaming it:

```console
$ cat ~/.config/im/schemas/pinyin.toml
id = "pinyin"
name = "我的全拼"
kind = "syllable"
dictionaries = ["pinyin"]
$ im schema list
...
* pinyin           我的全拼           ...
```

A user file has to carry every field it needs; nothing is inherited from the
file it shadows.

### Trying a new scheme

Start from an existing one, either by copying the file in
`crates/im-core/data/schemas/` or by saving `im schema show <id>`, then give it
a new `id` and put it in the user directory:

```console
$ mkdir -p ~/.config/im/schemas
$ cp crates/im-core/data/schemas/double_pinyin.toml \
      ~/.config/im/schemas/my_scheme.toml
$ $EDITOR ~/.config/im/schemas/my_scheme.toml   # set id = "my_scheme"
$ im schema show my_scheme
$ im --schema my_scheme query --format tsv -n 2 hk
```

Write the file with an editor or a move rather than redirecting a command's
output straight into the directory: the shell creates the file before `im`
starts, and a truncated schema is a parse error.

Check the two commands that matter:

- `im schema show <id>` prints the schema exactly as `im` loaded it, as
  loadable TOML. If it prints, the file parsed and validated.
- `im --schema <id> query <keys>` shows the candidates for a code, which is the
  fastest way to confirm that a folding change did what you expected. Add
  `--format tsv` to see the underlying spelling and kind of each match.

If you are contributing the scheme to the repository, `CONTRIBUTING.md` asks
for the file to be registered in `builtin.rs` and for the folding cases you care
about to be added to the tests in `crates/im-core/src/speller.rs`, which is
where the built-in schemes are pinned down.

## When a table is not enough

The tables express one shape of scheme: each syllable becomes an initial key
plus a final key, chosen from fixed tables. A scheme that needs anything else —
a different segmentation rule, a stateful keystroke sequence, a per-syllable
lookup that depends on the syllable's position, or a code with its own
weighting rules — cannot be expressed as data, and needs an engine change.

The boundary is `Schema::fold_into` in `crates/im-core/src/schema.rs`, which
dispatches on `kind`, and `Speller::fold_syllable_into` in
`crates/im-core/src/speller.rs`, which is the only place a syllable is split.
Adding a new `kind` means extending `SchemaKind`, adding a branch to
`fold_into`, extending the validation that keeps the `[a-z0-9]` invariant, and
documenting the new fields here. Everything downstream of the fold — the index,
the three match kinds, the session and every front end — is untouched by that
change, which is the point of putting the scheme at the very front of the
pipeline.
