# im-cli

The `im` command line interface: a terminal candidate picker and a query tool
for the [`im`](https://github.com/qjsoong/im) input method.

```console
$ echo vsgo | im query -n 1
中国

$ im query --format tsv vsgo
vsgo	exact	中国	zhong guo	700	system
vsgo	backoff	中	zhong	900	system

$ im                    # interactive picker; the chosen text goes to stdout
$ im --schema 小鹤 query hc
好
```

The picker draws on the controlling terminal rather than on stdout, so it works
in a pipeline: `im > note.txt` and `title=$(im)` both behave.

## Subcommands

| Command | What it does |
| --- | --- |
| *(none)* | Interactive picker |
| `im query [CODE]...` | Print candidates; reads one code per line from stdin |
| `im schema list` / `show <id>` | Inspect the input schemes available |
| `im config show` / `path` | Inspect the layered configuration |
| `im dict add` / `list` / `remove` / `path` | Manage your own dictionary |

## License

MIT OR Apache-2.0, except for the dictionary compiled into the binary, which is
CC BY-SA 4.0. See
[`data/dict/NOTICE.md`](https://github.com/qjsoong/im/blob/main/data/dict/NOTICE.md).
