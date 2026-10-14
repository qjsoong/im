//! Rendering the composition as the preedit string.
//!
//! Under XIM the *application* draws the preedit, which means the string below
//! is the entire visible interface while typing: there is no window for
//! `im-xim` to draw candidates in. So the candidates go in the preedit, the way
//! a terminal multiplexer puts its status line in the terminal.
//!
//! ```text
//! 1.中国 2.[种] 3.中国话 (1/2) | vsgo
//!  ^      ^                ^     ^
//!  number candidate        page  keys typed
//! ```

use im_core::{PreeditStyle, Session};

/// Build the preedit string for the current state of `session`.
pub fn render(session: &Session<'_>, style: PreeditStyle) -> String {
    match style {
        PreeditStyle::Off => String::new(),
        PreeditStyle::Keys => session.input().to_owned(),
        PreeditStyle::Candidates => render_candidates(session),
    }
}

fn render_candidates(session: &Session<'_>) -> String {
    let mut out = String::new();
    let (start, end) = session.page_bounds();
    if start < end {
        for (offset, candidate) in session.candidates()[start..end].iter().enumerate() {
            if offset > 0 {
                out.push(' ');
            }
            let number = offset + 1;
            if start + offset == session.highlight() {
                out.push_str(&format!("{number}.[{}]", candidate.text));
            } else {
                out.push_str(&format!("{number}.{}", candidate.text));
            }
        }
        if session.page_count() > 1 {
            out.push_str(&format!(
                " ({}/{})",
                session.page() + 1,
                session.page_count()
            ));
        }
    }
    if !session.input().is_empty() {
        if !out.is_empty() {
            out.push_str(" | ");
        }
        out.push_str(session.input());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use im_core::{Dictionary, LookupOptions, Origin, Schema, schema};

    fn fixture() -> (Schema, im_core::CodeIndex) {
        let schema =
            schema::builtin::get("double_pinyin").unwrap_or_else(|e| panic!("{e}"));
        let dict = Dictionary::parse(
            "中\tzhong\t900\n国\tguo\t800\n中国\tzhong guo\t700\n中心\tzhong xin\t600\n\
             中国话\tzhong guo hua\t500\n种\tzhong\t400\n",
            Origin::System,
        )
        .unwrap_or_else(|e| panic!("{e}"));
        let index = schema.index([dict]).unwrap_or_else(|e| panic!("{e}"));
        (schema, index)
    }

    #[test]
    fn lists_candidates_with_the_highlighted_one_bracketed() {
        let (schema, index) = fixture();
        let mut session = Session::new(&schema, &index);
        session.push_str("vs");
        assert_eq!(
            render(&session, PreeditStyle::Candidates),
            "1.[中] 2.种 3.中国 4.中心 5.中国话 | vs"
        );
    }

    #[test]
    fn marks_the_selection_wherever_it_moves() {
        let (schema, index) = fixture();
        let mut session = Session::new(&schema, &index);
        session.push_str("vs");
        session.next_candidate();
        assert_eq!(
            render(&session, PreeditStyle::Candidates),
            "1.中 2.[种] 3.中国 4.中心 5.中国话 | vs"
        );
    }

    #[test]
    fn only_the_current_page_is_shown() {
        let (schema, index) = fixture();
        let mut session =
            Session::with_options(&schema, &index, 2, LookupOptions::default());
        session.push_str("vs");
        assert_eq!(
            render(&session, PreeditStyle::Candidates),
            "1.[中] 2.种 (1/3) | vs"
        );
        session.next_page();
        assert_eq!(
            render(&session, PreeditStyle::Candidates),
            "1.[中国] 2.中心 (2/3) | vs"
        );
    }

    #[test]
    fn keys_style_shows_only_what_was_typed() {
        let (schema, index) = fixture();
        let mut session = Session::new(&schema, &index);
        session.push_str("vsgo");
        assert_eq!(render(&session, PreeditStyle::Keys), "vsgo");
        assert_eq!(render(&session, PreeditStyle::Off), "");
    }

    #[test]
    fn an_input_with_no_candidates_still_shows_the_keys() {
        let (schema, index) = fixture();
        let mut session = Session::new(&schema, &index);
        session.push_str("qqqq");
        assert!(session.candidates().is_empty());
        assert_eq!(render(&session, PreeditStyle::Candidates), "qqqq");
    }

    #[test]
    fn an_empty_session_renders_nothing() {
        let (schema, index) = fixture();
        let session = Session::new(&schema, &index);
        assert_eq!(render(&session, PreeditStyle::Candidates), "");
    }
}
