# Configuration

`im.toml` is the only configuration file. It is plain TOML, it is merged from a
few fixed places, and every value in it has a default that is documented in the
file itself. There is no GUI settings dialog and no registry.

The built-in defaults live in `crates/im-core/data/im.toml` and are compiled
into the library; `im config show --default` prints them, comments included.

## Where the files live and how they merge

Configuration is layered, lowest precedence first:

1. the built-in defaults,
2. `im.toml` in each system configuration directory, in the order
   `$XDG_CONFIG_DIRS` lists them with the *earlier* directory taking precedence,
3. `im.toml` in the user configuration directory, `$XDG_CONFIG_HOME/im` (by
   default `~/.config/im`).

Missing files are skipped. A system directory is `/etc/xdg/im` by default.

The files are merged key by key, not replaced wholesale:

- Scalars (`schema`, `page_size`, `max_candidates`, `completion`) are replaced
  by the higher-precedence file.
- Tables are merged recursively, so `[keys]` from two files both take effect.
- Arrays are replaced, never concatenated. A `[keys]` list therefore replaces
  the built-in list for that one action.

That last rule is the one that surprises people, so it is worth an example:

```toml
# ~/.config/im/im.toml
schema = "double_pinyin"

[keys]
next_candidate = ["Tab", "C-n", "Down"]
```

After this file, `next_candidate` has exactly the three bindings listed — the
same three the built-in file had, so nothing changed — while `commit`, `cancel`
and the other actions keep their defaults. If the list had been `["Tab"]`, `C-n`
and `Down` would no longer move the highlight. There is no "and the defaults
too".

`im config show` prints the merged result as TOML; `im config path` prints the
files that contributed to it.

## Every key

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `schema` | string | `"pinyin"` | input scheme used when `--schema` is absent |
| `page_size` | integer >= 1 | `9` | candidates shown on one page |
| `max_candidates` | integer >= 1 | `100` | candidates kept for one input |
| `completion` | boolean | `true` | offer completions of unfinished codes |
| `keys` | table | see below | keystroke bindings, by action name |
| `xim` | table | see below | settings used only by `im-xim` |

Two of these are tables, and both merge key by key against the built-in values,
so a file can set one field of `[xim]` or one binding of `[keys]` and inherit
the rest.

Unknown keys are rejected rather than ignored, so a typo fails loudly:

```text
im: loading configuration (...): /home/u/.config/im/im.toml: unknown field
`pagesize`, expected one of `schema`, `page_size`, `max_candidates`,
`completion`, `keys`, `xim`
```

Error excerpts in this document drop the `(config: […]; data: […])` list that
`im` prints after the command, and any continuation lines.

`page_size` and `max_candidates` must be at least 1; `0` is reported as
`page_size must be at least 1` or `max_candidates must be at least 1`. A schema
may carry its own `page_size`, which overrides the global one when set; see
[`schemas.md`](schemas.md).

`max_candidates` bounds one lookup, which is why `im query` stops after 100
records by default. The command line can override it per invocation with
`--max-candidates`, `im query -n`, `--page-size` and `--no-completion`.

## The `[keys]` table

Each entry maps an action name to the list of keystrokes that perform it. The
action names are fixed; listing one *replaces* that action's built-in bindings.
The built-in bindings are:

| Action | Built-in bindings | What it does |
| --- | --- | --- |
| `commit` | `Enter`, `space` | commit the highlighted candidate |
| `commit_raw` | `C-Enter` | finish, emitting the raw input as typed |
| `cancel` | `Escape`, `C-c`, `C-g` | abandon the composition |
| `clear` | `C-u` | throw away the input |
| `backspace` | `Backspace`, `C-h` | delete one input character |
| `next_candidate` | `Down`, `C-n`, `Tab` | highlight the next candidate |
| `prev_candidate` | `Up`, `C-p`, `S-Tab` | highlight the previous one |
| `next_page` | `Page_Down`, `C-f`, `=` | move to the next page |
| `prev_page` | `Page_Up`, `C-b`, `-` | move to the previous page |
| `select` | `1`–`9` | commit the n-th candidate on the page |

A few of those need a sentence more. `commit` with nothing typed ends the
composition and accepts what has already been committed. `cancel` clears the
input first if something is typed, and abandons the composition only when the
prompt is already empty. `select` is positional: the n-th binding commits the
n-th candidate on the current page, so six digits means six selectable
candidates even though nine may be shown.

Two behaviours of the terminal picker are worth knowing when rebinding:

- With nothing typed there are no candidates, so a `select` binding is treated
  as input instead: pressing `1` on an empty prompt types the digit.
- The picker's footer shows hard-coded hints (`Enter select · Tab next · 1-9
  pick · Esc clear`), not the bindings you configure.

An unknown action name is an error that lists every valid one:

```text
im: loading configuration (...): /home/u/.config/im/im.toml: invalid key
binding "nope": unknown action; expected one of commit, commit_raw, cancel,
clear, backspace, next_candidate, prev_candidate, next_page, prev_page, select
```

## The `[xim]` table

These settings are read by `im-xim`, the X11 front end. They live in `im.toml`
so that a person does not have to remember which of several files a setting
belongs to.

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `toggle_key` | string | `"C-space"` | Chinese/ASCII toggle |
| `initial_mode` | string | `"chinese"` | mode `im-xim` starts in |
| `preedit` | string | `"candidates"` | XIM preedit contents |
| `window` | boolean | `true` | draw a candidate window |
| `font_size` | float | `18.0` | font size for that window, 4 to 200 pixels |

`toggle_key` is a keystroke in the same grammar as the `[keys]` bindings below,
and `im` validates it even though it does not use it: a bad value is reported
as `xim.toggle_key: unknown key "nonsense-key"` rather than ignored until
`im-xim` starts. Pressing the toggle key while composing also drops what has
been typed, so it doubles as "never mind".

`initial_mode` is `"chinese"` or `"ascii"`. `preedit` controls how much of the
composition the application is asked to draw, and is `"candidates"`, `"keys"`
or `"off"`: `"candidates"` writes the candidate list with the highlighted one
bracketed, `"keys"` writes only the keys typed, and `"off"` writes nothing.

The preedit is only half the story. XIM has the *application* draw it, and most
clients — `st`, `xterm`, GTK 3 — ask for `XIMPreeditNothing`, which means they
draw nothing at all and expect the input method to put a candidate list on the
screen itself. That is what `window` is for, and turning it off leaves an input
method with no visible candidates in exactly those clients. `font_size` is in
pixels; raise it on a high-density display. If no font with Chinese coverage can
be found the daemon logs a warning and carries on without a window rather than
refusing to start.

## The key-binding grammar

A binding is an optional sequence of modifiers, written before the key. The
separator is `+` if the string contains one, otherwise `-`, so `C-n`, `Ctrl+n`
and `C+n` all describe the same keystroke, and the modifier names are
interchangeable:

| Modifier   | Accepted spellings                |
| ---------- | --------------------------------- |
| Control    | `c`, `ctrl`, `control`            |
| Shift      | `s`, `shift`                      |
| Alt / Meta | `a`, `alt`, `m`, `meta`           |
| Super      | `super`, `win`, `cmd`, `os`       |

Modifiers are case-insensitive and order does not matter. A binding with no
modifiers is just a key name.

The key itself is one of:

- **A single character**, case-insensitive: `n`, `1`, `=`. A lone `-` or `+` is
  a character, not a separator.
- **A named key**, with these aliases:

  | Key        | Accepted names                                          |
  | ---------- | ------------------------------------------------------- |
  | Enter      | `enter`, `return`, `ret`, `cr`                          |
  | Escape     | `esc`, `escape`                                         |
  | Tab        | `tab`                                                   |
  | Backspace  | `backspace`, `bs`                                       |
  | Delete     | `delete`, `del`                                         |
  | Insert     | `insert`, `ins`                                         |
  | Up/Down    | `up`, `down`                                            |
  | Left/Right | `left`, `right`                                         |
  | Home/End   | `home`, `end`                                           |
  | Page Up    | `pageup`, `page_up`, `pgup`, `prior`                    |
  | Page Down  | `pagedown`, `page_down`, `pgdn`, `next`                 |
  | Space      | `space`, `spc`                                          |
  | Function   | `f1`, `f2`, … (any number above 0)                      |

- **A symbol name**, so a binding copied from another input method keeps
  working: `minus` (`-`), `equal`/`equals` (`=`), `comma` (`,`),
  `period`/`dot` (`.`), `slash` (`/`), `semicolon` (`;`),
  `apostrophe`/`quote` (`'`), `bracketleft`/`leftbracket` (`[`),
  `bracketright`/`rightbracket` (`]`), `backslash` (`\`), `grave`/`backtick`
  (`` ` ``).

Key names are case-insensitive, `-` and `_` are interchangeable inside a name,
and leading and trailing whitespace is ignored. `Page-Up` works even though `-`
is also the modifier separator: if the parts before the last one are not
modifiers, the whole string is retried as a key name.

Three things are rejected:

- an empty binding (`""`),
- an unknown key (`C-nope`, `f0`, `ab`, `zz+z`),
- one keystroke bound to two actions. This is an error rather than a silent
  last-one-wins, because a keymap is a statement and an ambiguous one is a bug:

```text
im: loading configuration (...): /home/u/.config/im/im.toml: invalid key binding
"x": also bound to cancel; each keystroke may only do one thing
```

## Environment variables

Directories follow the XDG Base Directory specification. The `XDG_*` variables
name *base* directories and `im` appends `/im`; the `IM_*` variables name *`im`
directories* directly, which is what a script or a test wants.

| Variable | Effect |
| --- | --- |
| `IM_CONFIG_HOME` | the user configuration directory |
| `IM_DATA_HOME` | the user data directory |
| `IM_CACHE_HOME` | the cache directory |
| `IM_STATE_HOME` | the state directory |
| `IM_CONFIG_DIRS` | colon-separated system configuration directories |
| `IM_DATA_DIRS` | colon-separated system data directories |
| `XDG_CONFIG_HOME` | base directory; `im` uses `$XDG_CONFIG_HOME/im` |
| `XDG_DATA_HOME` | base directory; `im` uses `$XDG_DATA_HOME/im` |
| `XDG_CACHE_HOME` | base directory; `im` uses `$XDG_CACHE_HOME/im` |
| `XDG_STATE_HOME` | base directory; `im` uses `$XDG_STATE_HOME/im` |
| `XDG_CONFIG_DIRS` | base dirs; `im` appends `/im` to each |
| `XDG_DATA_DIRS` | base dirs; `im` appends `/im` to each |
| `HOME` | fallback for everything above |

For each user directory the `IM_*` variable wins over its `XDG_*` counterpart,
which wins over `HOME`. The fallbacks are `$HOME/.config/im`,
`$HOME/.local/share/im`, `$HOME/.cache/im` and `$HOME/.local/state/im`. Every
path must be absolute; relative and empty list entries are dropped. The default
lists are `/etc/xdg` for `XDG_CONFIG_DIRS` and
`/usr/local/share:/usr/share` for `XDG_DATA_DIRS`.

The cache and state directories are resolved and available to front ends;
nothing in this repository writes to them yet.

Inside a directory the layout is:

```text
<config dir>/im.toml          this file
<config dir>/schemas/*.toml   user-defined input schemes
<data dir>/dict/*.tsv         dictionaries
<data dir>/schemas/*.toml     schemes shipped by a distribution
<data dir>/user.tsv           your own dictionary
```

The tests use these variables to keep a machine's real configuration out of the
way, which is the recommended way to try a configuration out:

```console
$ IM_CONFIG_HOME=$PWD/conf IM_DATA_HOME=$PWD/data im config show
```

## Diagnosing a configuration

Three commands answer most questions.

`im config show` prints the configuration after merging, as TOML, with the
actions in alphabetical order and `[xim]` after them. It is the fastest way to
see whether a file was read at all.

```console
$ im config show
schema = "pinyin"
page_size = 9
max_candidates = 100
completion = true

[keys]
backspace = [
    "Backspace",
    "C-h",
]
...

[xim]
toggle_key = "C-space"
initial_mode = "chinese"
preedit = "candidates"
```

```console
$ im config show --default
```

`--default` prints the built-in file instead, comments included. Compare the two
to see exactly what your files changed.

```console
$ im config path
/home/u/.config/im/im.toml
$ im config path
(no configuration file; using the built-in defaults)
```

`im config path` lists the files that contribute, lowest precedence first. When
the list is empty, the defaults are in use.

A configuration error stops every command, including `im query`, and names the
file it came from, so a broken configuration never degrades into silently
different behaviour. `im schema list` marks the active schema with `*`, which
is a quick check that `schema` points at something that exists.
