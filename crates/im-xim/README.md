# im-xim

An X Input Method server for [`im`](https://github.com/qjsoong/im). Applications
that speak XIM find it through `XMODIFIERS`, so xterm, st, GTK 3, Qt and Emacs
can type Chinese with no desktop environment, no D-Bus and no per-toolkit module
involved.

```console
$ export XMODIFIERS=@im=im
$ im-xim &
```

`Ctrl+Space` switches between Chinese and ASCII. Type a code, then a digit or
`Enter` to choose a candidate.

## The candidate window

XIM has the *application* draw the preedit, and most clients ask for
`XIMPreeditNothing` — they draw nothing and expect the input method to put a
candidate list on the screen itself, at the spot location they report. `st`,
`xterm` and GTK 3 all work this way, so `im-xim` maintains a small
override-redirect window of its own:

```text
┌──────────────┐
│ vsgo         │  what was typed
│ 1 中国       │
│ 2 中         │  highlighted
│ 3 中国话     │
│ 1/2          │  only when there is more than one page
└──────────────┘
```

Glyphs are rasterised with `fontdue` and sent as one `PutImage` per change. The
only thing this needs from the system is a font file with Chinese coverage; if
none can be found the daemon logs a warning and carries on, committing text with
no candidate list to look at. Set `[xim] window = false` to ask for that on
purpose.

For clients that *do* render the preedit, the candidates are also written into
the preedit string, so both kinds of client show something.

## Running it

`im-xim` runs in the foreground and exits when the X connection goes away, so
start it from your session — `~/.xinitrc`, a systemd user unit, whatever
supervises the rest of your desktop — and let it be restarted:

```sh
XMODIFIERS=@im=im im-xim &
```

Use `--name` if something else already answers to `im`. Several XIM servers can
be registered at once; the client picks the one named by `XMODIFIERS`.

`-v` logs connections and input contexts, `-vv` logs protocol traffic.

## Configuration

`[xim]` in `im.toml` holds the toggle key, the mode to start in, what the preedit
contains, and whether and at what size to draw the window. See
[`docs/config.md`](https://github.com/qjsoong/im/blob/main/docs/config.md).

## License

MIT OR Apache-2.0, except for the dictionary compiled into the binary, which is
CC BY-SA 4.0. See
[`data/dict/NOTICE.md`](https://github.com/qjsoong/im/blob/main/data/dict/NOTICE.md).
