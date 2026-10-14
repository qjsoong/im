//! Offline validation of the generated dictionary files.
//!
//! `dict check` re-reads `data/dict/pinyin.tsv` and
//! `data/dict/syllables.txt` and verifies every frozen rule of the format,
//! so CI can guard the data without network access. The first violation
//! aborts the command with the file name and line number.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};

use super::{Paths, Record, compare_records, pinyin};

/// Validate `data/dict/pinyin.tsv` and `data/dict/syllables.txt`.
pub fn check(paths: &Paths) -> Result<()> {
    let (records, syllables) = check_dict(&paths.dict)?;
    let listed = check_syllable_list(&paths.syllables)?;

    if let Some(missing) = syllables.difference(&listed).next() {
        bail!(
            "{}: missing syllable `{missing}` used by {}",
            paths.syllables.display(),
            paths.dict.display()
        );
    }
    if let Some(unexpected) = listed.difference(&syllables).next() {
        bail!(
            "{}: syllable `{unexpected}` does not occur in {}",
            paths.syllables.display(),
            paths.dict.display()
        );
    }

    eprintln!(
        "dict: {} and {} are valid ({records} records, {} syllables)",
        paths.dict.display(),
        paths.syllables.display(),
        syllables.len()
    );
    Ok(())
}

/// Validate the dictionary file, returning its record count and syllables.
fn check_dict(path: &Path) -> Result<(usize, BTreeSet<String>)> {
    let contents = read_utf8(path)?;
    let mut count = 0;
    let mut syllables = BTreeSet::new();
    let mut previous: Option<Record> = None;

    for (index, line) in contents.split('\n').enumerate() {
        let number = index + 1;
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        check_line_ending(path, number, line)?;
        let record = parse_record(path, number, line)?;
        if let Some(previous) = &previous {
            if previous.text == record.text && previous.spelling == record.spelling {
                bail!(
                    "{}:{number}: duplicate record for `{}` with spelling `{}`, \
                     it already appeared with weight {}",
                    path.display(),
                    record.text,
                    record.spelling,
                    previous.weight
                );
            }
            if compare_records(previous, &record) != std::cmp::Ordering::Less {
                bail!(
                    "{}:{number}: `{}` with spelling `{}` is out of order, it must \
                     sort before `{}` with spelling `{}`",
                    path.display(),
                    record.text,
                    record.spelling,
                    previous.text,
                    previous.spelling
                );
            }
        }
        for syllable in pinyin::syllables(&record.spelling) {
            syllables.insert(syllable.to_owned());
        }
        count += 1;
        previous = Some(record);
    }

    if count == 0 {
        bail!("{}: no records found", path.display());
    }
    Ok((count, syllables))
}

/// Validate one record line against the format rules.
fn parse_record(path: &Path, number: usize, line: &str) -> Result<Record> {
    let fields: Vec<&str> = line.split('\t').collect();
    if fields.len() != 3 {
        bail!(
            "{}:{number}: expected 3 TAB-separated fields, found {}",
            path.display(),
            fields.len()
        );
    }
    let (text, spelling, weight) = (fields[0], fields[1], fields[2]);

    if text.is_empty() {
        bail!("{}:{number}: the text field is empty", path.display());
    }
    for character in text.chars() {
        if !pinyin::is_han(character) {
            bail!(
                "{}:{number}: text `{text}` contains non-Han character U+{:04X}",
                path.display(),
                character as u32
            );
        }
    }
    if !pinyin::is_valid_spelling(spelling) {
        bail!(
            "{}:{number}: spelling `{spelling}` must be lowercase pinyin \
             syllables separated by single spaces",
            path.display()
        );
    }
    let characters = text.chars().count();
    let syllable_count = pinyin::syllables(spelling).count();
    if characters != syllable_count {
        bail!(
            "{}:{number}: text `{text}` has {characters} characters but \
             spelling `{spelling}` has {syllable_count} syllables",
            path.display()
        );
    }

    Ok(Record {
        text: text.to_owned(),
        spelling: spelling.to_owned(),
        weight: parse_weight(path, number, weight)?,
    })
}

/// Validate the weight field.
fn parse_weight(path: &Path, number: usize, raw: &str) -> Result<u16> {
    if raw.is_empty() || !raw.bytes().all(|byte| byte.is_ascii_digit()) {
        bail!(
            "{}:{number}: weight `{raw}` is not a decimal integer",
            path.display()
        );
    }
    if raw.len() > 1 && raw.starts_with('0') {
        bail!(
            "{}:{number}: weight `{raw}` has a leading zero",
            path.display()
        );
    }
    let Ok(value) = raw.parse::<u32>() else {
        bail!(
            "{}:{number}: weight `{raw}` is not a 32-bit integer",
            path.display()
        );
    };
    if value > u32::from(u16::MAX) {
        bail!(
            "{}:{number}: weight {value} is outside 0..=65535",
            path.display()
        );
    }
    Ok(value as u16)
}

/// Validate the syllable list file, returning the syllables it lists.
fn check_syllable_list(path: &Path) -> Result<BTreeSet<String>> {
    let contents = read_utf8(path)?;
    let mut listed = BTreeSet::new();
    let mut previous: Option<String> = None;

    for (index, line) in contents.split('\n').enumerate() {
        let number = index + 1;
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        check_line_ending(path, number, line)?;
        if !pinyin::is_syllable(line) {
            bail!(
                "{}:{number}: `{line}` is not a single lowercase pinyin syllable",
                path.display()
            );
        }
        if let Some(previous) = &previous {
            match previous.as_str().cmp(line) {
                std::cmp::Ordering::Less => {}
                std::cmp::Ordering::Equal => {
                    bail!("{}:{number}: duplicate syllable `{line}`", path.display())
                }
                std::cmp::Ordering::Greater => bail!(
                    "{}:{number}: `{line}` is out of order, it must sort \
                     before `{previous}`",
                    path.display()
                ),
            }
        }
        previous = Some(line.to_owned());
        listed.insert(line.to_owned());
    }

    if listed.is_empty() {
        bail!("{}: no syllables found", path.display());
    }
    Ok(listed)
}

/// Reject CRLF and bare carriage returns.
fn check_line_ending(path: &Path, number: usize, line: &str) -> Result<()> {
    if line.ends_with('\r') {
        bail!(
            "{}:{number}: carriage return, the file must use LF line endings",
            path.display()
        );
    }
    Ok(())
}

/// Read `path` as UTF-8, reporting the line of the first encoding error.
fn read_utf8(path: &Path) -> Result<String> {
    let bytes =
        fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    match std::str::from_utf8(&bytes) {
        Ok(contents) => Ok(contents.to_owned()),
        Err(error) => {
            let line = 1 + bytes[..error.valid_up_to()]
                .iter()
                .filter(|&&byte| byte == b'\n')
                .count();
            bail!("{}:{line}: file is not valid UTF-8", path.display())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: &str = "# header\n#\n";
    const VALID_DICT: &str =
        "# header\n啊\ta\t0\n一\tyi\t7000\n中国\tzhong guo\t1099\n";
    const VALID_SYLLABLES: &str = "# header\na\nguo\nyi\nzhong\n";

    /// Write `dict` and `syllables` below a temporary workspace root.
    fn workspace(dict: &str, syllables: &str) -> Result<(tempfile::TempDir, Paths)> {
        let dir = tempfile::tempdir()?;
        let data_dir = dir.path().join("data/dict");
        fs::create_dir_all(&data_dir)?;
        fs::write(data_dir.join("pinyin.tsv"), dict)?;
        fs::write(data_dir.join("syllables.txt"), syllables)?;
        let paths = Paths::with_root(dir.path());
        Ok((dir, paths))
    }

    /// Run `check` and return the message of the first violation.
    fn violation(dict: &str, syllables: &str) -> Result<String> {
        let (_dir, paths) = workspace(dict, syllables)?;
        let Err(error) = check(&paths) else {
            panic!("expected a violation for:\n{dict}");
        };
        Ok(format!("{error:#}"))
    }

    #[test]
    fn accepts_a_valid_pair() -> Result<()> {
        let (_dir, paths) = workspace(VALID_DICT, VALID_SYLLABLES)?;
        check(&paths)
    }

    #[test]
    fn accepts_the_generated_files() -> Result<()> {
        check(&Paths::workspace()?)
    }

    #[test]
    fn rejects_a_missing_field() -> Result<()> {
        assert!(
            violation("# header\n啊\ta\n", VALID_SYLLABLES)?.contains("pinyin.tsv:2")
        );
        assert!(
            violation("# header\n啊\ta\t0\t\n", VALID_SYLLABLES)?
                .contains("pinyin.tsv:2")
        );
        Ok(())
    }

    #[test]
    fn rejects_non_han_text() -> Result<()> {
        assert!(violation("# h\nA\ta\t1\n", VALID_SYLLABLES)?.contains("pinyin.tsv:2"));
        assert!(violation("# h\n№\tno\t1\n", VALID_SYLLABLES)?.contains("non-Han"));
        assert!(
            violation("# h\n一 二\tyi er\t1\n", VALID_SYLLABLES)?.contains("non-Han")
        );
        Ok(())
    }

    #[test]
    fn rejects_a_bad_spelling() -> Result<()> {
        assert!(
            violation("# h\n一\tyi3\t1\n", VALID_SYLLABLES)?.contains("pinyin.tsv:2")
        );
        assert!(
            violation("# h\n一\tYi\t1\n", VALID_SYLLABLES)?.contains("pinyin.tsv:2")
        );
        assert!(
            violation("# h\n中国\tzhong  guo\t1\n", VALID_SYLLABLES)?
                .contains("pinyin.tsv:2")
        );
        assert!(violation("# h\n一\t\t1\n", VALID_SYLLABLES)?.contains("pinyin.tsv:2"));
        Ok(())
    }

    #[test]
    fn rejects_a_syllable_count_mismatch() -> Result<()> {
        let message = violation("# h\n中国\tzhong\t1\n", VALID_SYLLABLES)?;
        assert!(message.contains("pinyin.tsv:2"), "{message}");
        assert!(message.contains("2 characters but"), "{message}");
        Ok(())
    }

    #[test]
    fn rejects_a_bad_weight() -> Result<()> {
        assert!(
            violation("# h\n一\tyi\tx\n", VALID_SYLLABLES)?.contains("pinyin.tsv:2")
        );
        assert!(
            violation("# h\n一\tyi\t-1\n", VALID_SYLLABLES)?.contains("pinyin.tsv:2")
        );
        assert!(
            violation("# h\n一\tyi\t007\n", VALID_SYLLABLES)?.contains("leading zero")
        );
        assert!(
            violation("# h\n一\tyi\t65536\n", VALID_SYLLABLES)?.contains("outside")
        );
        assert!(
            violation("# h\n一\tyi\t70000000000\n", VALID_SYLLABLES)?
                .contains("32-bit")
        );
        Ok(())
    }

    #[test]
    fn rejects_unsorted_records() -> Result<()> {
        let message = violation("# h\n一\tyi\t7000\n啊\ta\t0\n", VALID_SYLLABLES)?;
        assert!(message.contains("pinyin.tsv:3"), "{message}");
        assert!(message.contains("out of order"), "{message}");

        let message = violation("# h\n乙\tyi\t7000\n一\tyi\t7000\n", VALID_SYLLABLES)?;
        assert!(message.contains("pinyin.tsv:3"), "{message}");
        assert!(message.contains("out of order"), "{message}");
        Ok(())
    }

    #[test]
    fn rejects_duplicated_records() -> Result<()> {
        let message = violation("# h\n一\tyi\t7000\n一\tyi\t7000\n", VALID_SYLLABLES)?;
        assert!(message.contains("pinyin.tsv:3"), "{message}");
        assert!(message.contains("duplicate record"), "{message}");

        let message = violation("# h\n一\tyi\t7000\n一\tyi\t6999\n", VALID_SYLLABLES)?;
        assert!(message.contains("duplicate record"), "{message}");
        Ok(())
    }

    #[test]
    fn rejects_crlf_and_invalid_utf8() -> Result<()> {
        assert!(
            violation("# h\r\n一\tyi\t1\r\n", VALID_SYLLABLES)?
                .contains("carriage return")
        );

        let (_dir, paths) = workspace(VALID_DICT, VALID_SYLLABLES)?;
        fs::write(&paths.dict, b"# h\n\xff\n")?;
        let Err(error) = check(&paths) else {
            panic!("expected an encoding violation");
        };
        assert!(format!("{error:#}").contains("pinyin.tsv:2: file is not valid UTF-8"));
        Ok(())
    }

    #[test]
    fn checks_the_syllable_list() -> Result<()> {
        assert!(
            violation(VALID_DICT, "# h\na\nguo\nyi\n")?
                .contains("missing syllable `zhong`")
        );
        assert!(
            violation(VALID_DICT, "# h\na\nguo\nxi\nyi\nzhong\n")?
                .contains("does not occur")
        );

        let message = violation(VALID_DICT, "# h\nzhong\na\nguo\nyi\n")?;
        assert!(message.contains("syllables.txt:3"), "{message}");
        assert!(message.contains("out of order"), "{message}");

        let message = violation(VALID_DICT, "# h\na\na\nguo\nyi\nzhong\n")?;
        assert!(message.contains("syllables.txt:3"), "{message}");
        assert!(message.contains("duplicate syllable"), "{message}");

        let message = violation(VALID_DICT, "# h\na\nguo\nYI\nzhong\n")?;
        assert!(message.contains("syllables.txt:4"), "{message}");
        assert!(message.contains("lowercase"), "{message}");
        Ok(())
    }

    #[test]
    fn rejects_empty_documents() -> Result<()> {
        assert!(violation(HEADER, VALID_SYLLABLES)?.contains("no records found"));
        assert!(violation(VALID_DICT, HEADER)?.contains("no syllables found"));
        Ok(())
    }
}
