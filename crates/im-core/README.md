# im-core

The input engine behind [`im`](https://github.com/qjsoong/im), with no
dependency on terminals, X11 or any specific desktop.

It turns keystrokes into candidates and a commit decision; front ends decide how
to draw that or where to inject it.

```rust
use im_core::{Dictionary, Origin, Session, schema};

let schema = schema::builtin::get("double_pinyin")?;
let dict = Dictionary::parse(
    "中\tzhong\t9000\n国\tguo\t8000\n中国\tzhong guo\t7000\n",
    Origin::System,
)?;
let index = schema.index([dict])?;

let mut session = Session::new(&schema, &index);
for key in "vsgo".chars() {
    session.push(key);
}
let commit = session.commit_highlighted().expect("a candidate");
assert_eq!(commit.text, "中国");
# Ok::<(), im_core::Error>(())
```

Everything funnels through one idea: a **code** is a short ASCII string.
A dictionary stores a word with its pinyin spelling; a schema folds that
spelling into the exact keys a user types. Full pinyin is the identity case,
自然码双拼 is a pair of initial/final tables, and a code table like wubi is the
case where the spelling already is the code. One dictionary therefore serves
every scheme.

See [`docs/design.md`](https://github.com/qjsoong/im/blob/main/docs/design.md)
for why it is shaped this way.

## Feature flags

- `embed-dictionary` — compile `data/dict/pinyin.tsv` into the binary so that a
  front end works with no data files installed. Off by default.

## License

MIT OR Apache-2.0, except for the generated dictionary, which is
CC BY-SA 4.0. See
[`data/dict/NOTICE.md`](https://github.com/qjsoong/im/blob/main/data/dict/NOTICE.md).
