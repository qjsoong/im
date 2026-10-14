//! The interactive picker.
//!
//! `im` with no subcommand opens a candidate list on the controlling terminal
//! and writes the text you choose to stdout. The UI deliberately does not touch
//! stdout, so it composes like any other filter:
//!
//! ```console
//! $ im > note.txt        # the interface is on /dev/tty, the result is redirected
//! $ title=$(im)          # command substitution works too
//! ```
//!
//! # Keys
//!
//! Everything is bound in `im.toml` under `[keys]`; the defaults are the usual
//! ones. `Enter` commits the highlighted candidate and, once nothing is left to
//! type, finishes. Typing `vsgo` and pressing `1` selects the first candidate on
//! the page, so the common case is two keystrokes: the code, then the choice.
//!
//! Committing only clears the part of the input the candidate accounted for, so
//! a longer phrase can be built by typing everything at once: `vsgovsgo`
//! followed by two selections yields two words.

use std::fs::{File, OpenOptions};

use anyhow::{Context as _, Result};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{cursor, execute, terminal};
use ratatui::Frame;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Layout, Position};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

use im_core::{
    Action, Candidate, CodeIndex, Key, KeyEvent, Keymap, LookupOptions, Mods, Schema,
    Session,
};

/// Space between two candidate cells.
const GAP: usize = 2;

/// Options for one picker run.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Text shown before the input, `>` by default.
    pub prompt: Option<String>,
    /// Code to start with, as if it had been typed.
    pub initial: Option<String>,
}

/// Run the picker, returning the accepted text.
///
/// `Ok(None)` means the user cancelled: nothing should be printed and the
/// process should exit with [`crate::EXIT_CANCELLED`].
pub fn run(
    schema: &Schema,
    index: &CodeIndex,
    page_size: usize,
    options: LookupOptions,
    config: &im_core::Config,
    picker_options: &Options,
) -> Result<Option<String>> {
    let mut picker =
        Picker::new(schema, index, page_size, options, config, picker_options);

    let mut session = TerminalSession::start()?;
    let result = picker.event_loop(session.terminal_mut());
    session.finish()?;

    match result? {
        Step::Accept | Step::Continue => Ok(Some(picker.accepted)),
        Step::Cancel => Ok(None),
    }
}

/// What the picker decided to do after a keystroke.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    /// Keep going.
    Continue,
    /// Finish and emit what has been accepted.
    Accept,
    /// Abandon the composition; emit nothing.
    Cancel,
}

/// Everything the picker needs, with no I/O at all.
struct Picker<'a> {
    session: Session<'a>,
    accepted: String,
    keymap: &'a Keymap,
    prompt: String,
    /// Set when a commit made the session idle, so the loop can stop after the
    /// final frame has been drawn.
    finished: bool,
}

impl<'a> Picker<'a> {
    fn new(
        schema: &'a Schema,
        index: &'a CodeIndex,
        page_size: usize,
        options: LookupOptions,
        config: &'a im_core::Config,
        picker_options: &Options,
    ) -> Self {
        let mut session = Session::with_options(schema, index, page_size, options);
        if let Some(initial) = &picker_options.initial {
            session.push_str(initial);
        }
        Self {
            session,
            accepted: String::new(),
            keymap: config.keymap(),
            prompt: picker_options
                .prompt
                .clone()
                .unwrap_or_else(|| ">".to_owned()),
            finished: false,
        }
    }

    /// Apply one keystroke.
    fn handle(&mut self, event: &KeyEvent) -> Step {
        // With nothing typed there are no candidates, so a digit is more useful
        // as input than as a selection that cannot select anything.
        if let Some(action) = self.keymap.action(event)
            && !(self.session.is_idle() && matches!(action, Action::Select(_)))
        {
            return self.apply(action);
        }
        if let Key::Char(ch) = event.key
            && (event.mods.is_empty() || event.mods == Mods::SHIFT)
        {
            self.session.push(ch);
        }
        Step::Continue
    }

    fn apply(&mut self, action: Action) -> Step {
        match action {
            Action::Commit => self.commit(),
            Action::CommitRaw => {
                if let Some(raw) = self.session.commit_raw() {
                    self.accepted.push_str(&raw);
                }
                Step::Accept
            }
            Action::Cancel => {
                if self.session.is_idle() {
                    Step::Cancel
                } else {
                    self.session.clear();
                    Step::Continue
                }
            }
            Action::Clear => {
                self.session.clear();
                Step::Continue
            }
            Action::Backspace => {
                self.session.backspace();
                Step::Continue
            }
            Action::NextCandidate => {
                self.session.next_candidate();
                Step::Continue
            }
            Action::PrevCandidate => {
                self.session.prev_candidate();
                Step::Continue
            }
            Action::NextPage => {
                self.session.next_page();
                Step::Continue
            }
            Action::PrevPage => {
                self.session.prev_page();
                Step::Continue
            }
            Action::Select(position) => self.select(position),
        }
    }

    /// Commit the highlighted candidate, or the input verbatim when nothing
    /// matches it.
    fn commit(&mut self) -> Step {
        if self.session.is_idle() {
            return Step::Accept;
        }
        if let Some(commit) = self.session.commit_highlighted() {
            self.accepted.push_str(&commit.text);
        } else if let Some(raw) = self.session.commit_raw() {
            // No candidate at all: commit what was typed, like an input method
            // falling back to direct input.
            self.accepted.push_str(&raw);
        }
        self.after_commit()
    }

    /// Commit the candidate at `position` on the current page.
    fn select(&mut self, position: usize) -> Step {
        let (start, end) = self.session.page_bounds();
        let index = start + position;
        if index >= end {
            return Step::Continue;
        }
        if let Some(commit) = self.session.commit(index) {
            self.accepted.push_str(&commit.text);
        }
        self.after_commit()
    }

    fn after_commit(&mut self) -> Step {
        if self.session.is_idle() {
            self.finished = true;
            Step::Accept
        } else {
            Step::Continue
        }
    }

    fn event_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<File>>,
    ) -> Result<Step> {
        loop {
            terminal.draw(|frame| self.draw(frame))?;
            let event = crossterm::event::read().context("reading a key")?;
            let Step::Continue = self.dispatch(event)? else {
                // One last frame so the user sees what was accepted before the
                // alternate screen goes away.
                terminal.draw(|frame| self.draw(frame))?;
                return Ok(if self.finished {
                    Step::Accept
                } else {
                    Step::Cancel
                });
            };
        }
    }

    fn dispatch(&mut self, event: Event) -> Result<Step> {
        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                Ok(self.handle(&to_key_event(key)))
            }
            Event::Resize(_, _) | Event::FocusGained | Event::FocusLost => {
                Ok(Step::Continue)
            }
            // A paste arrives as a burst of character events; consume it so it
            // becomes input rather than being interpreted as commands.
            Event::Paste(text) => {
                self.session.push_str(&text);
                Ok(Step::Continue)
            }
            _ => Ok(Step::Continue),
        }
    }

    // -- rendering ---------------------------------------------------------

    fn draw(&self, frame: &mut Frame) {
        let area = frame.area();
        let layout = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);
        let (header, body, prompt, footer) =
            (layout[0], layout[1], layout[2], layout[3]);

        self.draw_header(frame, header);
        self.draw_candidates(frame, body);
        self.draw_prompt(frame, prompt);
        self.draw_footer(frame, footer);
    }

    fn draw_header(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let left = format!(" im · {}", self.session.schema().name);
        let pages = self.session.page_count();
        let right = if pages > 1 {
            format!("{}/{} ", self.session.page() + 1, pages)
        } else {
            String::new()
        };
        let line = Line::from(vec![
            Span::styled(left, Style::new().add_modifier(Modifier::BOLD)),
            Span::raw(pad_between("", &right, area.width as usize)),
            Span::raw(right),
        ]);
        frame.render_widget(Paragraph::new(line), area);
    }

    fn draw_candidates(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let page = self.session.page_candidates();
        if page.is_empty() {
            return;
        }
        let lines = candidate_lines(
            page,
            self.session.page_bounds().0,
            self.session.page_selection(),
            area.width as usize,
        );
        frame.render_widget(Paragraph::new(lines), area);
    }

    fn draw_prompt(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let mut spans = vec![
            Span::styled(
                format!("{} ", self.prompt),
                Style::new().add_modifier(Modifier::BOLD),
            ),
            Span::raw(self.session.input().to_owned()),
        ];
        let input_width = self.prompt.width() + 1 + self.session.input().width();
        if let Some(candidate) = self.session.highlighted() {
            let hint = format!("  {}", candidate.spelling);
            if input_width + hint.width() < area.width as usize {
                spans
                    .push(Span::styled(hint, Style::new().add_modifier(Modifier::DIM)));
            }
        }
        frame.render_widget(Paragraph::new(Line::from(spans)), area);
        let x = area.x + (input_width as u16).min(area.width.saturating_sub(1));
        frame.set_cursor_position(Position::new(x, area.y));
    }

    fn draw_footer(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let hints = if self.session.is_idle() {
            "Enter done · Esc cancel"
        } else {
            "Enter select · Tab next · 1-9 pick · Esc clear"
        };
        let accepted = self.accepted.clone();
        let line = if accepted.is_empty() {
            Line::from(Span::styled(
                hints.to_owned(),
                Style::new().add_modifier(Modifier::DIM),
            ))
        } else {
            Line::from(vec![
                Span::raw(accepted.clone()),
                Span::raw(pad_between(&accepted, hints, area.width as usize)),
                Span::styled(
                    hints.to_owned(),
                    Style::new().add_modifier(Modifier::DIM),
                ),
            ])
        };
        frame.render_widget(Paragraph::new(line), area);
    }
}

/// Lay candidates out in a grid that fits `width`.
fn candidate_lines(
    page: &[Candidate],
    start: usize,
    selection: usize,
    width: usize,
) -> Vec<Line<'static>> {
    let index_width = digits(start + page.len());
    let text_width = page
        .iter()
        .map(|candidate| candidate.text.width())
        .max()
        .unwrap_or(1);
    let cell_width = index_width + 1 + text_width;
    let columns = ((width + GAP) / (cell_width + GAP)).max(1);

    page.chunks(columns)
        .map(|row| {
            let mut spans: Vec<Span<'static>> = Vec::new();
            for (position, candidate) in row.iter().enumerate() {
                let number = format!("{:>index_width$} ", start + position + 1);
                let padding =
                    " ".repeat(text_width.saturating_sub(candidate.text.width()));
                let trailing = " ".repeat(GAP);
                let style = if position == selection {
                    Style::new().add_modifier(Modifier::REVERSED)
                } else {
                    Style::new()
                };
                let number_style = if position == selection {
                    style
                } else {
                    style.add_modifier(Modifier::DIM)
                };
                spans.push(Span::styled(number, number_style));
                spans.push(Span::styled(candidate.text.clone(), style));
                spans.push(Span::styled(format!("{padding}{trailing}"), style));
            }
            Line::from(spans)
        })
        .collect()
}

/// Number of decimal digits needed to print `value` (at least one).
fn digits(value: usize) -> usize {
    value.to_string().len()
}

/// Spaces that push `right` to the right edge of a `width`-wide line.
fn pad_between(left: &str, right: &str, width: usize) -> String {
    let used = left.width() + right.width();
    " ".repeat(width.saturating_sub(used))
}

/// Translate a crossterm key into the front-end neutral one.
fn to_key_event(key: crossterm::event::KeyEvent) -> KeyEvent {
    let mut mods = Mods::NONE;
    if key.modifiers.contains(KeyModifiers::SHIFT) {
        mods = mods.union(Mods::SHIFT);
    }
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        mods = mods.union(Mods::CTRL);
    }
    if key.modifiers.contains(KeyModifiers::ALT) {
        mods = mods.union(Mods::ALT);
    }
    if key.modifiers.contains(KeyModifiers::SUPER) {
        mods = mods.union(Mods::SUPER);
    }
    let key = match key.code {
        KeyCode::Char(ch) => Key::Char(ch.to_ascii_lowercase()),
        KeyCode::Enter => Key::Named(im_core::Named::Enter),
        KeyCode::Esc => Key::Named(im_core::Named::Escape),
        KeyCode::Tab => Key::Named(im_core::Named::Tab),
        // Terminals report Shift+Tab as a key of its own; the keymap spells it
        // `S-Tab`, so put the modifier back.
        KeyCode::BackTab => {
            mods = mods.union(Mods::SHIFT);
            Key::Named(im_core::Named::Tab)
        }
        KeyCode::Backspace => Key::Named(im_core::Named::Backspace),
        KeyCode::Delete => Key::Named(im_core::Named::Delete),
        KeyCode::Insert => Key::Named(im_core::Named::Insert),
        KeyCode::Up => Key::Named(im_core::Named::Up),
        KeyCode::Down => Key::Named(im_core::Named::Down),
        KeyCode::Left => Key::Named(im_core::Named::Left),
        KeyCode::Right => Key::Named(im_core::Named::Right),
        KeyCode::Home => Key::Named(im_core::Named::Home),
        KeyCode::End => Key::Named(im_core::Named::End),
        KeyCode::PageUp => Key::Named(im_core::Named::PageUp),
        KeyCode::PageDown => Key::Named(im_core::Named::PageDown),
        KeyCode::F(n) => Key::Named(im_core::Named::F(n)),
        // Keys with no meaning here; `Session::push` rejects the null character
        // so the picker ignores them.
        _ => Key::Char('\0'),
    };
    KeyEvent { key, mods }
}

/// A terminal that restores itself, on every path out including a panic.
struct TerminalSession {
    terminal: Terminal<CrosstermBackend<File>>,
}

impl TerminalSession {
    /// Take over the controlling terminal.
    fn start() -> Result<Self> {
        let tty = OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/tty")
            .context(
                "im needs a terminal to draw on; use `im query` when there is none",
            )?;
        // `Terminal::new` uses the fullscreen viewport and asks the backend for
        // its size, which crossterm reads from /dev/tty.
        let terminal = Terminal::new(CrosstermBackend::new(tty))
            .context("starting the terminal")?;
        let mut session = Self { terminal };
        session.enter()?;
        Ok(session)
    }

    fn terminal_mut(&mut self) -> &mut Terminal<CrosstermBackend<File>> {
        &mut self.terminal
    }

    fn enter(&mut self) -> Result<()> {
        terminal::enable_raw_mode().context("putting the terminal in raw mode")?;
        // The alternate screen is written through the backend's writer, so the
        // UI never touches stdout.
        execute!(
            self.terminal.backend_mut(),
            EnterAlternateScreen,
            cursor::Hide
        )
        .context("switching to the alternate screen")?;
        Ok(())
    }

    fn finish(mut self) -> Result<()> {
        self.leave()?;
        Ok(())
    }

    fn leave(&mut self) -> Result<()> {
        let result = execute!(
            self.terminal.backend_mut(),
            cursor::Show,
            LeaveAlternateScreen
        )
        .context("leaving the alternate screen");
        let raw = terminal::disable_raw_mode().context("restoring the terminal");
        result.and(raw)
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        // Best effort: a failure here must not mask the real error or panic.
        let _ = self.leave();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use im_core::{Dictionary, Origin, schema};

    fn index() -> CodeIndex {
        let schema =
            schema::builtin::get("double_pinyin").unwrap_or_else(|e| panic!("{e}"));
        let dict = Dictionary::parse(
            "中\tzhong\t900\n国\tguo\t800\n中国\tzhong guo\t700\n中心\tzhong xin\t600\n\
             种\tzhong\t500\naa\ta\t1\n",
            Origin::System,
        )
        .unwrap_or_else(|e| panic!("{e}"));
        schema.index([dict]).unwrap_or_else(|e| panic!("{e}"))
    }

    fn picker<'a>(
        schema: &'a Schema,
        index: &'a CodeIndex,
        config: &'a im_core::Config,
    ) -> Picker<'a> {
        Picker::new(
            schema,
            index,
            4,
            LookupOptions::default(),
            config,
            &Options::default(),
        )
    }

    fn key(text: &str) -> KeyEvent {
        KeyEvent::parse(text).unwrap_or_else(|e| panic!("{text}: {e}"))
    }

    #[test]
    fn typing_narrows_and_enter_accepts() {
        let config = im_core::Config::default();
        let index = index();
        let schema =
            schema::builtin::get("double_pinyin").unwrap_or_else(|e| panic!("{e}"));
        let mut picker = picker(&schema, &index, &config);

        for ch in "vsgo".chars() {
            assert_eq!(picker.handle(&KeyEvent::ch(ch)), Step::Continue);
        }
        assert_eq!(picker.session.input(), "vsgo");
        assert_eq!(
            picker.session.highlighted().map(|c| c.text.as_str()),
            Some("中国")
        );

        assert_eq!(picker.handle(&key("Enter")), Step::Accept);
        assert_eq!(picker.accepted, "中国");
    }

    #[test]
    fn a_digit_selects_from_the_page_and_finishes() {
        let config = im_core::Config::default();
        let index = index();
        let schema =
            schema::builtin::get("double_pinyin").unwrap_or_else(|e| panic!("{e}"));
        let mut picker = picker(&schema, &index, &config);
        picker.session.push_str("vs");
        let second = picker.session.candidates()[1].text.clone();
        assert_eq!(picker.handle(&key("2")), Step::Accept);
        assert_eq!(picker.accepted, second);
        assert!(picker.session.is_idle(), "the code was fully consumed");
    }

    #[test]
    fn a_digit_with_nothing_typed_is_input() {
        let config = im_core::Config::default();
        let index = index();
        let schema =
            schema::builtin::get("double_pinyin").unwrap_or_else(|e| panic!("{e}"));
        let mut picker = picker(&schema, &index, &config);
        assert_eq!(picker.handle(&key("1")), Step::Continue);
        assert_eq!(picker.session.input(), "1");
    }

    #[test]
    fn escape_clears_then_cancels() {
        let config = im_core::Config::default();
        let index = index();
        let schema =
            schema::builtin::get("double_pinyin").unwrap_or_else(|e| panic!("{e}"));
        let mut picker = picker(&schema, &index, &config);
        picker.session.push_str("vs");
        assert_eq!(picker.handle(&key("Escape")), Step::Continue);
        assert!(picker.session.is_idle());
        assert_eq!(picker.handle(&key("Escape")), Step::Cancel);
    }

    #[test]
    fn backspace_and_paging_do_not_accept() {
        let config = im_core::Config::default();
        let index = index();
        let schema =
            schema::builtin::get("double_pinyin").unwrap_or_else(|e| panic!("{e}"));
        let mut picker = picker(&schema, &index, &config);
        picker.session.push_str("vs");
        assert_eq!(picker.handle(&key("Backspace")), Step::Continue);
        assert_eq!(picker.session.input(), "v");
        assert_eq!(picker.handle(&key("Page_Down")), Step::Continue);
        assert_eq!(picker.handle(&key("Tab")), Step::Continue);
    }

    #[test]
    fn a_code_with_no_candidate_is_committed_verbatim() {
        let config = im_core::Config::default();
        let index = index();
        let schema =
            schema::builtin::get("double_pinyin").unwrap_or_else(|e| panic!("{e}"));
        let mut picker = picker(&schema, &index, &config);
        picker.session.push_str("qqqq");
        assert!(picker.session.candidates().is_empty());
        assert_eq!(picker.handle(&key("Enter")), Step::Accept);
        assert_eq!(picker.accepted, "qqqq");
    }

    #[test]
    fn a_partial_commit_keeps_composing() {
        let config = im_core::Config::default();
        let index = index();
        let schema =
            schema::builtin::get("double_pinyin").unwrap_or_else(|e| panic!("{e}"));
        let mut picker = picker(&schema, &index, &config);
        // "vs" is 中, "go" is 国; 中国 would consume both.
        picker.session.push_str("vsgo");
        let zhong = picker
            .session
            .candidates()
            .iter()
            .position(|c| c.text == "中")
            .unwrap_or_else(|| panic!("中 not offered"));
        assert_eq!(picker.select(zhong), Step::Continue);
        assert_eq!(picker.accepted, "中");
        assert_eq!(picker.session.input(), "go");
        assert_eq!(picker.handle(&key("Enter")), Step::Accept);
        assert_eq!(picker.accepted, "中国");
    }

    #[test]
    fn the_grid_fits_the_width() {
        let page = index().lookup("vs", LookupOptions::default());
        let lines = candidate_lines(&page, 0, 0, 24);
        for line in &lines {
            let width: usize = line.spans.iter().map(|span| span.content.width()).sum();
            assert!(width <= 24 + GAP, "line is {width} wide: {line:?}");
        }
        assert!(!lines.is_empty());
    }

    #[test]
    fn padding_never_underflows() {
        assert_eq!(pad_between("abc", "de", 10), "     ");
        assert_eq!(pad_between("abc", "de", 2), "");
    }

    #[test]
    fn digits_counts_decimal_places() {
        assert_eq!(digits(0), 1);
        assert_eq!(digits(9), 1);
        assert_eq!(digits(10), 2);
    }
}
