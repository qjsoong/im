//! The XIM server handler: everything the daemon does with a keystroke.
//!
//! [`Handler::react`] is deliberately free of X11: it takes a keystroke and
//! per-input-context state and returns a [`Reaction`]. The trait methods below
//! only turn that reaction into XIM requests. That split is what makes the
//! interesting half testable without an X server.

use x11rb::protocol::xproto::EventMask;
use x11rb::rust_connection::RustConnection;
use xim::x11rb::X11rbServer;
use xim::{InputStyle, Server, ServerError, ServerHandler, UserInputContext};

use im_core::{
    Action, CodeIndex, Config, InputMode, Key, KeyEvent, Keymap, LookupOptions, Mods,
    PreeditStyle, Schema, Session,
};

use crate::keyboard::Keyboard;
use crate::preedit;

/// What came of a keystroke.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reaction {
    /// The input method used the keystroke. The application should not see it.
    Consumed {
        /// Text to draw as the preedit, which may be empty.
        preedit: String,
        /// Text to insert into the application, if the key completed a word.
        commit: Option<String>,
    },
    /// The application should handle the keystroke normally.
    Pass,
}

/// Per-input-context state.
///
/// One of these exists per window that asks for an input context. XIM has no
/// per-window mode of its own, so the mode is global: a user who switches to
/// ASCII means it everywhere.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InputContextData {
    input: String,
    highlight: usize,
}

impl InputContextData {
    fn is_idle(&self) -> bool {
        self.input.is_empty()
    }

    fn clear(&mut self) {
        self.input.clear();
        self.highlight = 0;
    }
}

/// State shared by every input context.
pub struct Handler {
    schema: Schema,
    index: CodeIndex,
    keymap: Keymap,
    page_size: usize,
    options: LookupOptions,
    preedit: PreeditStyle,
    toggle: KeyEvent,
    mode: InputMode,
    keyboard: Keyboard,
    /// Number of live input contexts; logged, and the hook a future version
    /// would use to exit when nothing is using the daemon.
    contexts: usize,
}

impl Handler {
    /// Build a handler from an already loaded engine.
    pub fn new(
        schema: Schema,
        index: CodeIndex,
        config: &Config,
        page_size: usize,
        options: LookupOptions,
        keyboard: Keyboard,
    ) -> Self {
        // `Config::finish` validated the binding, so this cannot fail here.
        let toggle = KeyEvent::parse(&config.xim.toggle_key).unwrap_or(KeyEvent {
            key: Key::Char('\0'),
            mods: Mods::NONE,
        });
        Self {
            schema,
            index,
            keymap: config.keymap().clone(),
            page_size,
            options,
            preedit: config.xim.preedit,
            toggle,
            mode: config.xim.initial_mode,
            keyboard,
            contexts: 0,
        }
    }

    /// The mode currently in effect.
    pub fn mode(&self) -> InputMode {
        self.mode
    }

    /// Replace the cached keyboard mapping.
    pub fn reload_keyboard(
        &mut self,
        connection: &RustConnection,
    ) -> anyhow::Result<()> {
        self.keyboard = Keyboard::load(connection)?;
        Ok(())
    }

    /// Rebuild a session over the stored input.
    ///
    /// [`Session`] borrows the schema and the index, which the handler owns, so
    /// one is built for the length of a keystroke rather than stored. That
    /// costs one lookup per key press, which measured at tens of microseconds.
    fn session<'a>(&'a self, data: &InputContextData) -> Session<'a> {
        let mut session = Session::with_options(
            &self.schema,
            &self.index,
            self.page_size,
            self.options,
        );
        session.set_input(&data.input);
        session.set_highlight(data.highlight);
        session
    }

    fn store(&self, data: &mut InputContextData, session: &Session<'_>) {
        data.input.clear();
        data.input.push_str(session.input());
        data.highlight = session.highlight();
    }

    /// Decide what a keystroke means, with no X11 involved.
    pub fn react(&mut self, data: &mut InputContextData, key: &KeyEvent) -> Reaction {
        if *key == self.toggle {
            self.mode = self.mode.toggled();
            data.clear();
            log::info!(
                "switched to {}",
                match self.mode {
                    InputMode::Chinese => "Chinese",
                    InputMode::Ascii => "ASCII",
                }
            );
            return self.consumed(data, None);
        }
        if self.mode == InputMode::Ascii {
            return Reaction::Pass;
        }

        if let Some(action) = self.keymap.action(key)
            && self.action_does_something(data, action)
        {
            return self.apply(data, action);
        }

        // A code character. Shift is allowed so that a user typing an uppercase
        // letter, or typing with Caps Lock on, still gets the letter.
        //
        // While composing, any code character continues the code; with nothing
        // typed, only a character that can *begin* one does. Otherwise the
        // first key of a number would be swallowed in Chinese mode.
        if let Key::Char(ch) = key.key
            && (key.mods.is_empty() || key.mods == Mods::SHIFT)
            && Session::accepts(ch)
            && (!data.is_idle() || self.index.can_start(ch))
        {
            let mut session = self.session(data);
            session.push(ch);
            self.store(data, &session);
            return self.consumed(data, None);
        }

        Reaction::Pass
    }

    /// Whether an action would do anything right now.
    ///
    /// An input method must not swallow keys it has no use for: `Backspace` in
    /// an empty composition, or `Enter` when nothing is being composed, have to
    /// reach the application.
    fn action_does_something(&self, data: &InputContextData, action: Action) -> bool {
        let session = self.session(data);
        match action {
            Action::Commit | Action::CommitRaw | Action::Cancel | Action::Clear => {
                !data.is_idle()
            }
            Action::Backspace => !data.is_idle(),
            Action::NextCandidate | Action::PrevCandidate => session.has_candidates(),
            Action::NextPage | Action::PrevPage => session.page_count() > 1,
            Action::Select(position) => {
                let (start, end) = session.page_bounds();
                start + position < end
            }
        }
    }

    fn apply(&mut self, data: &mut InputContextData, action: Action) -> Reaction {
        match action {
            Action::Commit => self.commit_highlighted(data),
            Action::CommitRaw => {
                let mut session = self.session(data);
                let raw = session.commit_raw();
                self.store(data, &session);
                self.consumed(data, raw)
            }
            Action::Cancel => {
                data.clear();
                self.consumed(data, None)
            }
            Action::Clear | Action::Backspace => {
                let mut session = self.session(data);
                if matches!(action, Action::Clear) {
                    session.clear();
                } else {
                    session.backspace();
                }
                self.store(data, &session);
                self.consumed(data, None)
            }
            Action::NextCandidate
            | Action::PrevCandidate
            | Action::NextPage
            | Action::PrevPage => {
                let mut session = self.session(data);
                match action {
                    Action::NextCandidate => session.next_candidate(),
                    Action::PrevCandidate => session.prev_candidate(),
                    Action::NextPage => session.next_page(),
                    _ => session.prev_page(),
                }
                self.store(data, &session);
                self.consumed(data, None)
            }
            Action::Select(position) => {
                let mut session = self.session(data);
                let (start, _) = session.page_bounds();
                let commit = session.commit(start + position);
                self.store(data, &session);
                self.consumed(data, commit.map(|c| c.text))
            }
        }
    }

    /// Commit the highlighted candidate, or the input verbatim when nothing
    /// matches it — the same fallback to direct input a traditional input
    /// method has.
    fn commit_highlighted(&mut self, data: &mut InputContextData) -> Reaction {
        let mut session = self.session(data);
        let commit = session.commit_highlighted();
        let text = match commit {
            Some(commit) => Some(commit.text),
            None => session.commit_raw(),
        };
        self.store(data, &session);
        self.consumed(data, text)
    }

    /// Build the reaction for a keystroke the input method handled.
    fn consumed(&self, data: &InputContextData, commit: Option<String>) -> Reaction {
        let session = self.session(data);
        Reaction::Consumed {
            preedit: preedit::render(&session, self.preedit),
            commit,
        }
    }
}

impl ServerHandler<X11rbServer<&RustConnection>> for Handler {
    type InputContextData = InputContextData;
    type InputStyleArray = [InputStyle; 1];

    fn new_ic_data(
        &mut self,
        _server: &mut X11rbServer<&RustConnection>,
        _style: InputStyle,
    ) -> Result<Self::InputContextData, ServerError> {
        Ok(InputContextData::default())
    }

    /// Only styles the daemon can actually honour: the application draws the
    /// preedit, and there is no status area because there is no window to draw
    /// one in.
    fn input_styles(&self) -> Self::InputStyleArray {
        [InputStyle::PREEDIT_CALLBACKS | InputStyle::STATUS_NOTHING]
    }

    /// Ask for every key press. The handler consumes only the ones it can use
    /// and hands the rest straight back, which is the only way an input method
    /// can watch for its own toggle key.
    fn filter_events(&self) -> u32 {
        u32::from(EventMask::KEY_PRESS)
    }

    fn handle_connect(
        &mut self,
        _server: &mut X11rbServer<&RustConnection>,
    ) -> Result<(), ServerError> {
        log::info!("client connected");
        Ok(())
    }

    fn handle_create_ic(
        &mut self,
        server: &mut X11rbServer<&RustConnection>,
        user_ic: &mut UserInputContext<Self::InputContextData>,
    ) -> Result<(), ServerError> {
        self.contexts += 1;
        log::info!(
            "input context created for window 0x{:x} ({} live)",
            user_ic.ic.client_win(),
            self.contexts
        );
        server.set_event_mask(&user_ic.ic, u32::from(EventMask::KEY_PRESS), 0)
    }

    fn handle_destroy_ic(
        &mut self,
        _server: &mut X11rbServer<&RustConnection>,
        user_ic: UserInputContext<Self::InputContextData>,
    ) -> Result<(), ServerError> {
        self.contexts = self.contexts.saturating_sub(1);
        log::info!(
            "input context destroyed for window 0x{:x} ({} live)",
            user_ic.ic.client_win(),
            self.contexts
        );
        Ok(())
    }

    /// XIM asks for the text that should be inserted when a composition is
    /// abandoned. There is none: half-typed keys are not a word.
    fn handle_reset_ic(
        &mut self,
        server: &mut X11rbServer<&RustConnection>,
        user_ic: &mut UserInputContext<Self::InputContextData>,
    ) -> Result<String, ServerError> {
        user_ic.user_data.clear();
        server.preedit_draw(&mut user_ic.ic, "")?;
        Ok(String::new())
    }

    fn handle_set_focus(
        &mut self,
        _server: &mut X11rbServer<&RustConnection>,
        _user_ic: &mut UserInputContext<Self::InputContextData>,
    ) -> Result<(), ServerError> {
        Ok(())
    }

    fn handle_unset_focus(
        &mut self,
        server: &mut X11rbServer<&RustConnection>,
        user_ic: &mut UserInputContext<Self::InputContextData>,
    ) -> Result<(), ServerError> {
        // Losing focus abandons the composition; a half-typed code should not
        // reappear when the window is focused again.
        user_ic.user_data.clear();
        server.preedit_draw(&mut user_ic.ic, "")
    }

    fn handle_set_ic_values(
        &mut self,
        _server: &mut X11rbServer<&RustConnection>,
        _user_ic: &mut UserInputContext<Self::InputContextData>,
    ) -> Result<(), ServerError> {
        Ok(())
    }

    fn handle_forward_event(
        &mut self,
        server: &mut X11rbServer<&RustConnection>,
        user_ic: &mut UserInputContext<Self::InputContextData>,
        event: &<X11rbServer<&RustConnection> as Server>::XEvent,
    ) -> Result<bool, ServerError> {
        let Some(key) = self.keyboard.translate(event.detail, event.state.into())
        else {
            return Ok(false);
        };
        let data = &mut user_ic.user_data;
        match self.react(data, &key) {
            Reaction::Pass => Ok(false),
            Reaction::Consumed { preedit, commit } => {
                let ic = &mut user_ic.ic;
                if let Some(text) = commit {
                    // End the preedit before inserting, or the committed text
                    // lands inside it in applications that draw it inline.
                    server.preedit_draw(ic, "")?;
                    server.commit(ic, &text)?;
                }
                server.preedit_draw(ic, &preedit)?;
                Ok(true)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use im_core::{Dictionary, Origin, schema};

    fn handler() -> Handler {
        let schema =
            schema::builtin::get("double_pinyin").unwrap_or_else(|e| panic!("{e}"));
        let dict = Dictionary::parse(
            "中\tzhong\t900\n国\tguo\t800\n中国\tzhong guo\t700\n中心\tzhong xin\t600\n\
             种\tzhong\t400\n切\tqie\t300\n",
            Origin::System,
        )
        .unwrap_or_else(|e| panic!("{e}"));
        let index = schema.index([dict]).unwrap_or_else(|e| panic!("{e}"));
        let config = Config::default();
        let keyboard = Keyboard::from_keysyms(8, 4, Vec::new());
        Handler::new(schema, index, &config, 9, config.lookup_options(), keyboard)
    }

    fn key(text: &str) -> KeyEvent {
        KeyEvent::parse(text).unwrap_or_else(|e| panic!("{text}: {e}"))
    }

    fn type_keys(
        handler: &mut Handler,
        data: &mut InputContextData,
        keys: &str,
    ) -> Reaction {
        let mut last = Reaction::Pass;
        for ch in keys.chars() {
            last = handler.react(data, &KeyEvent::ch(ch));
        }
        last
    }

    #[test]
    fn typing_letters_composes_and_shows_candidates() {
        let mut handler = handler();
        let mut data = InputContextData::default();
        let reaction = type_keys(&mut handler, &mut data, "vsgo");
        let Reaction::Consumed { preedit, commit } = reaction else {
            panic!("expected the keystroke to be consumed");
        };
        assert!(commit.is_none());
        assert!(preedit.contains("[中国]"), "{preedit}");
        assert!(preedit.ends_with("| vsgo"), "{preedit}");
    }

    #[test]
    fn selecting_commits_and_ends_the_composition() {
        let mut handler = handler();
        let mut data = InputContextData::default();
        type_keys(&mut handler, &mut data, "vs");
        let reaction = handler.react(&mut data, &key("1"));
        assert_eq!(
            reaction,
            Reaction::Consumed {
                preedit: String::new(),
                commit: Some("中".to_owned()),
            }
        );
        assert!(data.is_idle());
    }

    #[test]
    fn enter_commits_the_highlighted_candidate() {
        let mut handler = handler();
        let mut data = InputContextData::default();
        type_keys(&mut handler, &mut data, "vsgo");
        let reaction = handler.react(&mut data, &key("Enter"));
        assert_eq!(
            reaction,
            Reaction::Consumed {
                preedit: String::new(),
                commit: Some("中国".to_owned()),
            }
        );
    }

    #[test]
    fn keys_that_mean_nothing_here_go_to_the_application() {
        let mut handler = handler();
        let mut data = InputContextData::default();

        // Nothing typed: every command key belongs to the application.
        for name in ["Enter", "Escape", "Backspace", "Tab", "space"] {
            assert_eq!(
                handler.react(&mut data, &key(name)),
                Reaction::Pass,
                "{name} while idle"
            );
        }

        // A digit cannot begin a pinyin code, so it is typed, not swallowed.
        for ch in "1234567890".chars() {
            assert_eq!(
                handler.react(&mut data, &KeyEvent::ch(ch)),
                Reaction::Pass,
                "{ch} while idle"
            );
            assert!(data.is_idle(), "{ch} did not start a composition");
        }

        // A letter can begin one, so it is.
        assert!(matches!(
            handler.react(&mut data, &KeyEvent::ch('v')),
            Reaction::Consumed { .. }
        ));
        data.clear();

        // While composing, a digit picks a candidate...
        data.input.push('v');
        assert!(matches!(
            handler.react(&mut data, &KeyEvent::ch('1')),
            Reaction::Consumed {
                commit: Some(_),
                ..
            }
        ));
        assert!(data.is_idle());

        // ...and when there is nothing to pick, it continues the code, which is
        // what a table scheme whose codes contain digits needs.
        data.input.push('v');
        handler.options.completion = false;
        assert!(matches!(
            handler.react(&mut data, &KeyEvent::ch('1')),
            Reaction::Consumed { commit: None, .. }
        ));
        assert_eq!(data.input, "v1");
        data.clear();

        // A key with no business in a composition.
        assert_eq!(handler.react(&mut data, &key("Left")), Reaction::Pass);
        assert_eq!(handler.react(&mut data, &key("f5")), Reaction::Pass);

        // Control and Alt combinations are the application's, except the toggle.
        assert_eq!(handler.react(&mut data, &key("C-a")), Reaction::Pass);
        assert_eq!(handler.react(&mut data, &key("M-x")), Reaction::Pass);
        assert_eq!(handler.react(&mut data, &key("C-z")), Reaction::Pass);
    }

    #[test]
    fn the_toggle_switches_to_ascii_and_back() {
        let mut handler = handler();
        let mut data = InputContextData::default();
        assert_eq!(handler.mode(), InputMode::Chinese);

        assert!(matches!(
            handler.react(&mut data, &key("C-space")),
            Reaction::Consumed { .. }
        ));
        assert_eq!(handler.mode(), InputMode::Ascii);

        // In ASCII mode every key belongs to the application.
        assert_eq!(handler.react(&mut data, &KeyEvent::ch('v')), Reaction::Pass);
        assert_eq!(handler.react(&mut data, &key("Enter")), Reaction::Pass);

        assert!(matches!(
            handler.react(&mut data, &key("C-space")),
            Reaction::Consumed { .. }
        ));
        assert_eq!(handler.mode(), InputMode::Chinese);
        assert!(matches!(
            type_keys(&mut handler, &mut data, "vs"),
            Reaction::Consumed { .. }
        ));
    }

    #[test]
    fn the_toggle_drops_a_half_typed_code() {
        let mut handler = handler();
        let mut data = InputContextData::default();
        type_keys(&mut handler, &mut data, "vs");
        assert!(!data.is_idle());
        handler.react(&mut data, &key("C-space"));
        assert!(data.is_idle(), "toggling away throws the composition out");
    }

    #[test]
    fn escape_clears_before_it_passes_through() {
        let mut handler = handler();
        let mut data = InputContextData::default();
        type_keys(&mut handler, &mut data, "vs");

        let reaction = handler.react(&mut data, &key("Escape"));
        assert!(matches!(reaction, Reaction::Consumed { .. }));
        assert!(data.is_idle(), "the first Escape clears");

        assert_eq!(
            handler.react(&mut data, &key("Escape")),
            Reaction::Pass,
            "the second Escape reaches the application"
        );
    }

    #[test]
    fn backspace_edits_the_code() {
        let mut handler = handler();
        let mut data = InputContextData::default();
        type_keys(&mut handler, &mut data, "vsg");
        assert!(matches!(
            handler.react(&mut data, &key("Backspace")),
            Reaction::Consumed { .. }
        ));
        assert_eq!(data.input, "vs");
    }

    #[test]
    fn tab_moves_the_highlight_and_paging_follows() {
        let mut handler = handler();
        let mut data = InputContextData::default();
        type_keys(&mut handler, &mut data, "vs");
        assert_eq!(data.highlight, 0);
        handler.react(&mut data, &key("Tab"));
        assert_eq!(data.highlight, 1);
        handler.react(&mut data, &key("S-Tab"));
        assert_eq!(data.highlight, 0);
        handler.react(&mut data, &key("S-Tab"));
        assert_eq!(data.highlight, 3, "wraps to the last candidate");
    }

    #[test]
    fn a_code_with_no_candidate_still_composes() {
        let mut handler = handler();
        // With completions on, any prefix of a valid code has candidates, so
        // turn them off to reach the case being tested: an input that matches
        // nothing at all.
        handler.options.completion = false;
        let mut data = InputContextData::default();
        let reaction = type_keys(&mut handler, &mut data, "qqqq");
        assert!(matches!(reaction, Reaction::Consumed { .. }));
        assert_eq!(data.input, "qqqq");
        // Enter then inserts what was typed, like any input method falling back
        // to direct input.
        assert_eq!(
            handler.react(&mut data, &key("Enter")),
            Reaction::Consumed {
                preedit: String::new(),
                commit: Some("qqqq".to_owned()),
            }
        );
    }

    #[test]
    fn committing_a_partial_match_keeps_the_rest() {
        let mut handler = handler();
        let mut data = InputContextData::default();
        type_keys(&mut handler, &mut data, "vsgo");
        // The second candidate is 中, which accounts for "vs" and leaves "go".
        let reaction = handler.react(&mut data, &key("2"));
        let Reaction::Consumed { commit, .. } = reaction else {
            panic!("expected a commit");
        };
        assert_eq!(commit.as_deref(), Some("中"));
        assert_eq!(data.input, "go");
    }

    #[test]
    fn the_preedit_style_is_honoured() {
        let mut handler = handler();
        handler.preedit = PreeditStyle::Keys;
        let mut data = InputContextData::default();
        let reaction = type_keys(&mut handler, &mut data, "vs");
        assert_eq!(
            reaction,
            Reaction::Consumed {
                preedit: "vs".to_owned(),
                commit: None,
            }
        );
    }

    #[test]
    fn shifting_an_uppercase_letter_still_composes() {
        let mut handler = handler();
        let mut data = InputContextData::default();
        let mut event = KeyEvent::ch('v');
        event.mods = Mods::SHIFT;
        assert!(matches!(
            handler.react(&mut data, &event),
            Reaction::Consumed { .. }
        ));
        assert_eq!(data.input, "v");
    }

    #[test]
    fn a_session_is_rebuilt_from_the_stored_input() {
        let handler = handler();
        let data = InputContextData {
            input: "vs".to_owned(),
            highlight: 1,
        };
        let session = handler.session(&data);
        assert_eq!(session.input(), "vs");
        assert_eq!(session.highlight(), 1);
        assert_eq!(session.candidates()[1].text, "种");
    }
}
