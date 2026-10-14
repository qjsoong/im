//! End-to-end tests for the `im` binary.
//!
//! Each test runs the real executable with `IM_CONFIG_HOME` and `IM_DATA_HOME`
//! pointing at a temporary directory, so nothing on the machine running the
//! suite can influence the result. That is also why those variables exist: they
//! name `im` directories rather than XDG base directories.

use std::path::PathBuf;
use std::process::{Command, Output};

/// A throwaway pair of configuration and data directories.
struct Sandbox {
    dir: tempfile::TempDir,
    config: PathBuf,
    data: PathBuf,
}

impl Sandbox {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let config = dir.path().join("config");
        let data = dir.path().join("data");
        std::fs::create_dir_all(&config).unwrap_or_else(|e| panic!("{e}"));
        std::fs::create_dir_all(&data).unwrap_or_else(|e| panic!("{e}"));
        Self { dir, config, data }
    }

    fn write(&self, relative: &str, contents: &str) -> PathBuf {
        let path = self.dir.path().join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap_or_else(|e| panic!("{e}"));
        }
        std::fs::write(&path, contents).unwrap_or_else(|e| panic!("{e}"));
        path
    }

    fn run(&self, args: &[&str]) -> Output {
        self.run_with_stdin(args, "")
    }

    fn run_with_stdin(&self, args: &[&str], stdin: &str) -> Output {
        use std::io::Write as _;
        let mut child = Command::new(env!("CARGO_BIN_EXE_im"))
            .args(args)
            .env("IM_CONFIG_HOME", &self.config)
            .env("IM_DATA_HOME", &self.data)
            // Keep the machine's system directories out of the search path.
            .env("IM_CONFIG_DIRS", &self.config)
            .env("IM_DATA_DIRS", &self.data)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap_or_else(|e| panic!("spawning im: {e}"));
        child
            .stdin
            .as_mut()
            .unwrap_or_else(|| panic!("no stdin"))
            .write_all(stdin.as_bytes())
            .unwrap_or_else(|e| panic!("{e}"));
        child
            .wait_with_output()
            .unwrap_or_else(|e| panic!("waiting for im: {e}"))
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap_or_else(|e| panic!("{e}"))
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).unwrap_or_else(|e| panic!("{e}"))
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "im failed with {:?}\nstderr: {}",
        output.status.code(),
        stderr(output)
    );
}

/// Stdout of a finished run, one line per element.
fn out_lines(output: &Output) -> Vec<String> {
    stdout(output).lines().map(str::to_owned).collect()
}

#[test]
fn queries_the_dictionary_compiled_into_the_binary() {
    let sandbox = Sandbox::new();
    let output = sandbox.run(&["query", "-n", "5", "zhongguo"]);
    assert_success(&output);
    let found = out_lines(&output);
    assert_eq!(found[0], "中国", "got {found:?}");
}

#[test]
fn reads_codes_from_stdin_one_per_line() {
    let sandbox = Sandbox::new();
    let output = sandbox.run_with_stdin(&["query", "-n", "1"], "zhongguo\n\nni\n");
    assert_success(&output);
    let found = out_lines(&output);
    assert_eq!(found.len(), 2, "blank lines are skipped: {found:?}");
    assert_eq!(found[0], "中国");
}

#[test]
fn a_limit_of_one_is_a_single_record() {
    let sandbox = Sandbox::new();
    let output = sandbox.run(&["query", "-n", "1", "zhong"]);
    assert_success(&output);
    let found = out_lines(&output);
    assert_eq!(found.len(), 1, "got {found:?}");
}

#[test]
fn tsv_output_is_tab_separated() {
    let sandbox = Sandbox::new();
    let output = sandbox.run(&["query", "--format", "tsv", "-n", "1", "zhongguo"]);
    assert_success(&output);
    let row = stdout(&output);
    let fields: Vec<&str> = row.trim_end().split('\t').collect();
    assert_eq!(fields.len(), 6, "got {fields:?}");
    assert_eq!(fields[0], "zhongguo");
    assert_eq!(fields[1], "exact");
    assert_eq!(fields[2], "中国");
    assert_eq!(fields[5], "system");
}

#[test]
fn double_pinyin_folds_the_keys() {
    let sandbox = Sandbox::new();
    let output =
        sandbox.run(&["--schema", "double_pinyin", "query", "-n", "3", "vsgo"]);
    assert_success(&output);
    let found = out_lines(&output);
    assert_eq!(found[0], "中国", "got {found:?}");

    // The full-pinyin spelling of the same word is not a double-pinyin code,
    // so 中国 must not appear; what it does match is the initial-letters
    // behaviour, where a short prefix lists common words.
    let output = sandbox.run(&["--schema", "double_pinyin", "query", "zhongguo"]);
    assert_success(&output);
    assert!(!stdout(&output).contains("中国"), "{}", stdout(&output));
}

#[test]
fn xiaohe_is_a_second_scheme_with_no_code_change() {
    let sandbox = Sandbox::new();
    // 好 is "hc" in 小鹤 and "hk" in 自然码.
    let xiaohe = sandbox.run(&["--schema", "xiaohe", "query", "-n", "1", "hc"]);
    assert_success(&xiaohe);
    assert_eq!(out_lines(&xiaohe)[0], "好");

    let natural = sandbox.run(&["--schema", "double_pinyin", "query", "-n", "1", "hk"]);
    assert_success(&natural);
    assert_eq!(out_lines(&natural)[0], "好");
}

#[test]
fn a_user_dictionary_wins_over_the_shipped_one() {
    let sandbox = Sandbox::new();
    // The shipped dictionary knows 中 as "zhong"; give it a different reading
    // and check that the user's record is the one offered.
    sandbox.write("data/user.tsv", "中\tzzz\t1\n");
    let output = sandbox.run(&["query", "-n", "1", "zzz"]);
    assert_success(&output);
    assert_eq!(out_lines(&output)[0], "中");

    let output = sandbox.run(&["query", "--format", "tsv", "-n", "1", "zzz"]);
    assert_success(&output);
    assert!(
        stdout(&output).contains("\tuser"),
        "the origin should say user: {}",
        stdout(&output)
    );
}

#[test]
fn a_dictionary_can_be_tried_out_from_a_path() {
    let sandbox = Sandbox::new();
    let path = sandbox.write("mine.tsv", "试\tzzzz\t100\n");
    let output = sandbox.run(&[
        "--dictionary",
        &path.display().to_string(),
        "query",
        "-n",
        "1",
        "zzzz",
    ]);
    assert_success(&output);
    assert_eq!(out_lines(&output)[0], "试");
}

#[test]
fn a_user_schema_shadows_a_built_in_one() {
    let sandbox = Sandbox::new();
    sandbox.write(
        "config/schemas/pinyin.toml",
        "id = \"pinyin\"\nname = \"我的全拼\"\nkind = \"syllable\"\ndictionaries = [\"pinyin\"]\n",
    );
    let output = sandbox.run(&["schema", "list"]);
    assert_success(&output);
    assert!(
        stdout(&output).contains("我的全拼"),
        "config_dir/schemas should shadow the built-in schema: {}",
        stdout(&output)
    );
}

#[test]
fn configuration_layers_over_the_defaults() {
    let sandbox = Sandbox::new();
    sandbox.write(
        "config/im.toml",
        "# only change one key\n\n[keys]\ncommit = [\"C-j\"]\n",
    );
    let output = sandbox.run(&["config", "show"]);
    assert_success(&output);
    let shown = stdout(&output);
    assert!(shown.contains("\"C-j\""), "{shown}");
    assert!(
        !shown.contains("\"Enter\""),
        "listing an action replaces its bindings: {shown}"
    );
    // The keys the user did not mention are still there.
    assert!(shown.contains("cancel"), "{shown}");
}

#[test]
fn configuration_paths_lists_what_is_read() {
    let sandbox = Sandbox::new();
    let path = sandbox.write("config/im.toml", "page_size = 5\n");
    let output = sandbox.run(&["config", "path"]);
    assert_success(&output);
    assert_eq!(stdout(&output).trim_end(), path.display().to_string());
}

#[test]
fn schema_show_prints_loadable_toml() {
    let sandbox = Sandbox::new();
    let output = sandbox.run(&["schema", "show", "double_pinyin"]);
    assert_success(&output);
    let shown = stdout(&output);
    assert!(shown.contains("id = \"double_pinyin\""), "{shown}");
    assert!(shown.contains("[speller.finals]"), "{shown}");
}

#[test]
fn dict_add_and_list_round_trip() {
    let sandbox = Sandbox::new();
    let output = sandbox.run(&["dict", "add", "词", "ci"]);
    assert_success(&output);

    let output = sandbox.run(&["dict", "list"]);
    assert_success(&output);
    let listed = out_lines(&output);
    assert_eq!(listed.len(), 1, "{listed:?}");
    assert_eq!(listed[0], "词\tci\t0");

    // And the word is searchable right away.
    let output = sandbox.run(&["query", "-n", "1", "ci"]);
    assert_success(&output);
    assert_eq!(out_lines(&output)[0], "词");
}

#[test]
fn dict_add_reuses_a_known_spelling() {
    let sandbox = Sandbox::new();
    let output = sandbox.run(&["dict", "add", "中国"]);
    assert_success(&output);
    let output = sandbox.run(&["dict", "list"]);
    assert_success(&output);
    let listed = stdout(&output);
    assert_eq!(
        listed.trim_end().split('\t').nth(1),
        Some("zhong guo"),
        "{listed}"
    );
}

#[test]
fn dict_add_without_a_spelling_explains_itself() {
    let sandbox = Sandbox::new();
    let output = sandbox.run(&["dict", "add", "没有这个词"]);
    assert!(!output.status.success());
    let message = stderr(&output);
    assert!(message.contains("pinyin spelling"), "{message}");
}

#[test]
fn dict_remove_deletes_the_entry() {
    let sandbox = Sandbox::new();
    assert_success(&sandbox.run(&["dict", "add", "词", "ci"]));
    assert_success(&sandbox.run(&["dict", "remove", "词"]));
    let output = sandbox.run(&["dict", "list"]);
    assert_success(&output);
    assert_eq!(stdout(&output), "");
}

#[test]
fn an_unknown_schema_lists_what_is_available() {
    let sandbox = Sandbox::new();
    let output = sandbox.run(&["--schema", "nope", "query", "a"]);
    assert!(!output.status.success());
    let message = stderr(&output);
    assert!(message.contains("nope"), "{message}");
    assert!(message.contains("double_pinyin"), "{message}");
}

#[test]
fn a_missing_dictionary_says_where_it_looked() {
    let sandbox = Sandbox::new();
    let output = sandbox.run(&["--dictionary", "mine", "query", "a"]);
    assert!(!output.status.success());
    let message = stderr(&output);
    assert!(message.contains("dict/mine.tsv"), "{message}");
    assert!(
        message.contains(&sandbox.data.display().to_string()),
        "{message}"
    );
}

#[test]
fn a_bad_configuration_reports_the_file() {
    let sandbox = Sandbox::new();
    let path = sandbox.write("config/im.toml", "pagesize = 5\n");
    let output = sandbox.run(&["query", "a"]);
    assert!(!output.status.success());
    let message = stderr(&output);
    assert!(message.contains(&path.display().to_string()), "{message}");
    assert!(message.contains("pagesize"), "{message}");
}

#[test]
fn a_bad_key_binding_reports_the_binding() {
    let sandbox = Sandbox::new();
    sandbox.write("config/im.toml", "[keys]\ncommit = [\"Nonsense+Key\"]\n");
    let output = sandbox.run(&["query", "a"]);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("Nonsense"), "{}", stderr(&output));
}

#[test]
fn closing_the_pipe_early_is_not_an_error() {
    let sandbox = Sandbox::new();
    // `im query` writing into a consumer that stops reading must exit quietly,
    // the way any other Unix filter does.
    let mut child = Command::new(env!("CARGO_BIN_EXE_im"))
        .args(["query", "-n", "100", "zhong"])
        .env("IM_CONFIG_HOME", &sandbox.config)
        .env("IM_DATA_HOME", &sandbox.data)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("{e}"));
    // Drop stdout without reading it.
    drop(child.stdout.take());
    let output = child.wait_with_output().unwrap_or_else(|e| panic!("{e}"));
    assert!(
        output.status.success(),
        "expected a clean exit, got {:?} with stderr {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn help_mentions_the_picker_and_the_query_subcommand() {
    let sandbox = Sandbox::new();
    let output = sandbox.run(&["--help"]);
    assert_success(&output);
    let help = stdout(&output);
    assert!(help.contains("query"), "{help}");
    assert!(help.contains("EXAMPLES"), "{help}");
}
