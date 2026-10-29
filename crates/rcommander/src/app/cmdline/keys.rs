//! Which keys the command line, the quick search and the quick filter take before the keymap.
//!
//! A key reaches [`App::cmdline_key`] only when no dialog, menu or prompt took it. Its character
//! (if it typed one) then goes, in this order, to an open quick search or filter, to a new quick
//! search when the setting's keys started one, or to the command line - which takes it while it
//! holds text, or when the keymap binds the key to nothing (Space selects on an empty line, as
//! Num+ asks for a mask). Keys without a character work the line: Enter runs it, Escape empties
//! it, Backspace shortens it, ArrowUp and ArrowDown walk its history while it has the keyboard.

use rc_engine::{
    Chord, Command, CursorTarget, PanelId, PanelView,
    quick_search::{NameMatch, Seek, find, quick_pattern},
};

use super::{CmdLineMsg, Search, SearchKeys, SearchKind, wrap};
use crate::app::{App, Effect, Message};

/// The message of a key press the window did not use: [`CmdLineMsg::Typed`] when it typed a
/// character, the plain key otherwise.
pub(in crate::app) fn key_message(chord: Chord, text: Option<&str>) -> Message {
    match typed_text(&chord, text) {
        Some(text) => wrap(CmdLineMsg::Typed(chord, text)),
        None => Message::Key(chord),
    }
}

/// The text a key press typed: a printable character without Ctrl or Alt, or with both (AltGr is
/// Ctrl+Alt on Windows) when it is not the key's own letter - `@` from AltGr+Q is typed,
/// Ctrl+Alt+Q is a quick search.
pub(in crate::app) fn typed_text(chord: &Chord, text: Option<&str>) -> Option<String> {
    let text = text.filter(|text| !text.is_empty() && !text.chars().any(char::is_control))?;
    let typed = match (chord.ctrl, chord.alt) {
        (false, false) => true,
        (true, true) => text.to_lowercase() != chord.key,
        _ => false,
    };
    typed.then(|| text.to_owned())
}

/// The one character a chord's key is, as a quick search types it.
fn letter(chord: &Chord) -> Option<String> {
    let mut chars = chord.key.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if chord.shift => Some(c.to_uppercase().collect()),
        (Some(c), None) => Some(c.to_string()),
        _ => None,
    }
}

impl App {
    /// The keys taken before the keymap; `None` leaves `chord` to it. `captured`: a text field
    /// used the key already.
    pub(in crate::app) fn cmdline_key(
        &mut self,
        chord: &Chord,
        captured: bool,
    ) -> Option<Vec<Effect>> {
        let typed = self.cmdline.pending.take();
        if self.pane(self.active).state.view != PanelView::Files {
            self.cmdline.search = None;
            self.cmdline.list = None;
            return None;
        }
        if let Some(effects) = self.list_key(chord) {
            return Some(effects);
        }
        // A search that ends leaves the key to what follows.
        if let Some(effects) = self.search_key(chord, typed.as_deref()) {
            return Some(effects);
        }
        if captured {
            // The field's own Escape took its keyboard; the line goes too.
            return (chord.key == "Escape" && self.cmdline.focused).then(|| self.clear_line());
        }
        if let Some(effects) = self.start_search(chord, typed.as_deref()) {
            return Some(effects);
        }
        self.line_key(chord, typed)
    }

    /// Ctrl+S (`panel.filter` from a key): the filter of `panel` is typed where the line is.
    pub(in crate::app) fn open_filter(&mut self, panel: PanelId) -> Vec<Effect> {
        let text = self
            .pane(panel)
            .state
            .filter
            .pattern
            .clone()
            .unwrap_or_default();
        self.cmdline.search = Some(Search {
            panel,
            kind: SearchKind::Filter,
            text,
            quick: false,
        });
        self.searching()
    }

    /// The search takes the line's place: the field and its list are gone, and with them its
    /// keyboard.
    fn searching(&mut self) -> Vec<Effect> {
        self.cmdline.list = None;
        self.cmdline.hovered = false;
        self.leave_field()
    }

    /// A key while the Alt+F8 list is open: the arrows move in it, Enter takes the line, Escape
    /// closes it; any other key closes it and goes on.
    fn list_key(&mut self, chord: &Chord) -> Option<Vec<Effect>> {
        let at = self.cmdline.list?;
        let last = self.cmdline.history.len().saturating_sub(1);
        match chord.key.as_str() {
            "ArrowDown" if chord.is_plain() => self.cmdline.list = Some((at + 1).min(last)),
            "ArrowUp" if chord.is_plain() => self.cmdline.list = Some(at.saturating_sub(1)),
            "Enter" if chord.is_plain() => return Some(self.pick_history(at)),
            "Escape" => {
                self.cmdline.list = None;
                return Some(Vec::new());
            }
            _ => {
                self.cmdline.list = None;
                return None;
            }
        }
        Some(self.history_in_view())
    }

    /// A key while a quick search or filter is typed; `None` when it ended it and the key goes
    /// on.
    fn search_key(&mut self, chord: &Chord, typed: Option<&str>) -> Option<Vec<Effect>> {
        let search = self.cmdline.search.as_ref()?;
        if search.panel != self.active {
            self.cmdline.search = None;
            return None;
        }
        let (kind, quick) = (search.kind, search.quick);
        if let Some(typed) = typed {
            return Some(self.extend_search(typed));
        }
        if quick
            && self.search_chord(chord)
            && let Some(more) = letter(chord)
        {
            return Some(self.extend_search(&more));
        }
        match (kind, chord.key.as_str()) {
            (_, "Backspace") if chord.is_plain() => {
                if let Some(search) = self.cmdline.search.as_mut() {
                    search.text.pop();
                }
                Some(match kind {
                    SearchKind::Find => Vec::new(),
                    SearchKind::Filter => self.filter_typed(),
                })
            }
            (SearchKind::Find, "ArrowDown") if chord.is_plain() => Some(self.seek(Seek::Next)),
            (SearchKind::Find, "ArrowUp") if chord.is_plain() => Some(self.seek(Seek::Previous)),
            (SearchKind::Find, "Escape") => {
                self.cmdline.search = None;
                Some(Vec::new())
            }
            (SearchKind::Filter, "Escape") => {
                self.cmdline.search = None;
                Some(self.dispatch_filter(None))
            }
            (SearchKind::Filter, "Enter") if chord.is_plain() => {
                self.cmdline.search = None;
                Some(Vec::new())
            }
            _ => {
                // Ends the search, keeps the filter; the key does what it does in the panel.
                self.cmdline.search = None;
                None
            }
        }
    }

    /// Whether `chord` is the quick search's key with a letter (Ctrl+Alt, or Alt alone).
    fn search_chord(&self, chord: &Chord) -> bool {
        match self.cmdline.options.keys {
            SearchKeys::CtrlAlt => chord.ctrl && chord.alt,
            SearchKeys::Alt => chord.alt && !chord.ctrl,
            SearchKeys::Letters => false,
        }
    }

    /// A quick search started by its keys - or by a typed letter while the setting says letters
    /// and the line is empty without the keyboard. A key the keymap binds keeps its command.
    fn start_search(&mut self, chord: &Chord, typed: Option<&str>) -> Option<Vec<Effect>> {
        let letters = self.cmdline.options.keys == SearchKeys::Letters
            && self.cmdline.text.is_empty()
            && !self.cmdline.focused;
        let text = match typed {
            Some(typed) if letters && !chord.ctrl && !chord.alt => typed.to_owned(),
            None if self.search_chord(chord) => letter(chord)?,
            _ => return None,
        };
        if self.keymap.lookup(chord, PanelView::Files).is_some() {
            return None;
        }
        let kind = if self.cmdline.options.filters {
            SearchKind::Filter
        } else {
            SearchKind::Find
        };
        self.cmdline.search = Some(Search {
            panel: self.active,
            kind,
            text: String::new(),
            quick: true,
        });
        let mut effects = self.searching();
        effects.extend(self.extend_search(&text));
        Some(effects)
    }

    /// `typed` added to the search: the filter follows; the quick search moves the cursor to the
    /// match - or, filtering, hides what does not match -, and a character no name continues with
    /// is not taken, as in Total Commander.
    fn extend_search(&mut self, typed: &str) -> Vec<Effect> {
        let Some(search) = self.cmdline.search.as_mut() else {
            return Vec::new();
        };
        let wanted = format!("{}{typed}", search.text);
        if search.kind == SearchKind::Filter {
            let anywhere = self.cmdline.options.matching.anywhere;
            if search.quick && !self.any_match(&quick_pattern(&wanted, anywhere)) {
                return Vec::new();
            }
            if let Some(search) = self.cmdline.search.as_mut() {
                search.text = wanted;
            }
            return self.filter_typed();
        }
        let matcher = NameMatch::quick_search(&wanted, self.cmdline.options.matching);
        let pane = self.pane(self.active);
        let found =
            matcher.and_then(|matcher| find(pane.entries(), &matcher, pane.cursor(), Seek::Here));
        match found {
            Some(index) => {
                if let Some(search) = self.cmdline.search.as_mut() {
                    search.text = wanted;
                }
                self.cursor_to(index)
            }
            None => Vec::new(),
        }
    }

    /// ArrowDown or ArrowUp in a quick search: the next or previous match, if there is one.
    fn seek(&mut self, seek: Seek) -> Vec<Effect> {
        let Some(search) = &self.cmdline.search else {
            return Vec::new();
        };
        let Some(matcher) = NameMatch::quick_search(&search.text, self.cmdline.options.matching)
        else {
            return Vec::new();
        };
        let pane = self.pane(self.active);
        // From the `..` row the first entry is the next one.
        let (cursor, seek) = match seek {
            Seek::Next if pane.on_parent() => (0, Seek::Here),
            _ => (pane.cursor(), seek),
        };
        match find(pane.entries(), &matcher, cursor, seek) {
            Some(index) => self.cursor_to(index),
            None => Vec::new(),
        }
    }

    /// The cursor of the active panel onto entry `index`, on screen and in the engine.
    fn cursor_to(&mut self, index: usize) -> Vec<Effect> {
        let panel = self.active;
        let pane = self.pane_mut(panel);
        let Some(name) = pane.entries().get(index).map(|entry| entry.name.clone()) else {
            return Vec::new();
        };
        pane.point_at(index + usize::from(pane.has_parent_row()));
        let to = Command::SetCursor {
            panel: Some(panel),
            target: CursorTarget::Name(name),
        };
        vec![Effect::Dispatch(panel, vec![to])]
    }

    /// The filter as typed so far, for the panel it is typed for; a quick search's text as the
    /// names it finds.
    fn filter_typed(&mut self) -> Vec<Effect> {
        let anywhere = self.cmdline.options.matching.anywhere;
        let text = self.cmdline.search.as_ref().map(|search| {
            if search.quick {
                quick_pattern(&search.text, anywhere)
            } else {
                search.text.clone()
            }
        });
        self.dispatch_filter(text)
    }

    /// Whether the filter `pattern` keeps any entry of the active panel's whole listing.
    fn any_match(&self, pattern: &str) -> bool {
        let Some(matcher) = NameMatch::filter(pattern) else {
            return true;
        };
        let listed = self.pane(self.active).state.listed();
        listed.iter().any(|entry| matcher.matches(&entry.name))
    }

    /// `panel.filter` for the active panel; `None` shows every entry.
    fn dispatch_filter(&self, pattern: Option<String>) -> Vec<Effect> {
        let panel = self.active;
        let filter = Command::Filter {
            panel: Some(panel),
            pattern,
            last: false,
        };
        vec![Effect::Dispatch(panel, vec![filter])]
    }

    /// The command line's keys, and its typed characters.
    fn line_key(&mut self, chord: &Chord, typed: Option<String>) -> Option<Vec<Effect>> {
        let enabled = self.line_enabled();
        if let Some(typed) = typed {
            let takes = enabled
                && (!self.cmdline.text.is_empty()
                    || self.keymap.lookup(chord, PanelView::Files).is_none());
            return takes.then(|| self.type_into_line(&typed));
        }
        let has_text = !self.cmdline.text.trim().is_empty();
        let (ctrl, shift, alt) = (chord.ctrl, chord.shift, chord.alt);
        match chord.key.as_str() {
            "Enter" if !ctrl && !alt && has_text && enabled => Some(self.run_line(shift)),
            "Enter" if ctrl && !alt && enabled => Some(self.insert_name(shift)),
            "p" if ctrl && !shift && !alt && enabled => Some(self.insert_folder()),
            "F8" if alt && !ctrl && !shift && enabled => Some(self.open_list()),
            "Escape"
                if chord.is_plain() && (!self.cmdline.text.is_empty() || self.cmdline.focused) =>
            {
                Some(self.clear_line())
            }
            "Escape"
                if chord.is_plain() && self.pane(self.active).state.filter.pattern.is_some() =>
            {
                Some(self.dispatch_filter(None))
            }
            "Backspace" if chord.is_plain() && !self.cmdline.text.is_empty() => {
                self.cmdline.text.pop();
                self.cmdline.recalled = None;
                if self.cmdline.text.is_empty() {
                    return Some(self.leave_field());
                }
                Some(Vec::new())
            }
            "ArrowUp" if chord.is_plain() && self.cmdline.focused => Some(self.recall_line(1)),
            "ArrowDown" if chord.is_plain() && self.cmdline.focused => Some(self.recall_line(-1)),
            _ => None,
        }
    }
}

#[cfg(test)]
#[path = "keys_tests.rs"]
mod tests;
