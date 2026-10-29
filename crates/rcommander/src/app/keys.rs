//! Key presses and the one-line prompts that take a panel's keys for a moment.
//!
//! A key goes to the keymap; its command goes to the engine, through the active panel's `..`
//! row. A key a widget already used - a terminal's, or a prompt's text field's - runs only the
//! bindings that apply everywhere (Ctrl+`), so typing never moves a panel and the Enter that
//! submits a prompt opens nothing.
//!
//! Two kinds of command need words first, as in Total Commander: Num+ and Num- ask for a mask
//! ("Select group", "Unselect group": `panel.select_by_mask` and `panel.deselect_by_mask`
//! without one), and a click beside the path's segments asks for a path to go to. The prompt
//! sits in the panel; Enter runs the command, Escape drops it. A mask prompt starts with the
//! last mask; ArrowUp and ArrowDown walk the kept masks (`mru::MASKS`), and the newest
//! ones are offered beside the field.
//!
//! Escape while a drive or share keeps the panel waiting stops the wait (`panel.cancel_io`,
//! RC-080-04), before any other use of the key.

use iced::{
    Task,
    widget::{Id, operation},
};
use rc_engine::{Chord, Command, PanelId, PanelView};

use super::{App, Effect, mru};

/// The id of the prompt's text field, to focus it.
pub const PROMPT_ID: &str = "rcommander.prompt";

/// What a prompt asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptKind {
    /// A mask to select by (Num+).
    Select,
    /// A mask to deselect by (Num-).
    Deselect,
    /// A directory for the panel.
    Path,
}

/// An open prompt and what was typed so far.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    pub panel: PanelId,
    pub kind: PromptKind,
    pub text: String,
    /// The kept mask shown, by its place in the history; `None` while typed.
    recalled: Option<usize>,
}

impl App {
    /// A key press; `captured` when a widget handled it already.
    pub(super) fn on_key(&mut self, chord: Chord, captured: bool) -> Vec<Effect> {
        if self.prompt.is_some() {
            let plain = chord.is_plain();
            return match chord.key.as_str() {
                "Escape" if plain => {
                    self.prompt = None;
                    Vec::new()
                }
                // A field that lost the keyboard to a click still submits; one that has it
                // submitted already.
                "Enter" if plain && !captured => self.submit_prompt(),
                "ArrowUp" if plain => self.recall(1),
                "ArrowDown" if plain => self.recall(-1),
                _ => Vec::new(),
            };
        }
        // Escape while a drive keeps the panel waiting comes before everything else (RC-080-04).
        if !captured && let Some(effects) = self.io_escape(&chord) {
            return effects;
        }
        // The command line, the quick search and the quick filter come first (RC-010-08).
        if let Some(effects) = self.cmdline_key(&chord, captured) {
            return effects;
        }
        let active = self.active;
        if !captured && let Some(effects) = self.counting_escape(&chord) {
            return effects;
        }
        // Ctrl+C, Ctrl+X and Ctrl+V: files on the clipboard (RC-080-01).
        if !captured && let Some(effects) = self.clipboard_key(&chord) {
            return effects;
        }
        // `Terminal` asks the keymap for the bindings that apply everywhere only.
        let view = if captured {
            PanelView::Terminal
        } else {
            self.pane(active).state.view
        };
        // Ctrl+Shift+F, the window's own key: the active panel's connection closes.
        if let Some(effects) = self.disconnect_key(&chord, view) {
            return effects;
        }
        // Ctrl+Shift+P, a window key of the keymap: the command palette (RC-080-05).
        if self.keymap.window_key(&chord, view) == Some(super::palette::OPEN_KEY) {
            return self.open_palette();
        }
        // Ctrl+Shift+A, also from a terminal: the panel's tabs or sessions (RC-080-16).
        if self.keymap.window_key(&chord, view) == Some(super::tabs::LIST_KEY) {
            return self.open_tab_list(active);
        }
        let Some(command) = self.keymap.lookup(&chord, view).cloned() else {
            return Vec::new();
        };
        self.run_bound(command)
    }

    /// A binding's command for the active panel, as its key runs it - also from the command
    /// palette for a binding whose keys were all taken away (RC-100-21).
    pub(super) fn run_bound(&mut self, command: Command) -> Vec<Effect> {
        let active = self.active;
        // Alt+Down and Ctrl+D open a popup; Ctrl+Up and Ctrl+Left/Right read the `..` row.
        if let Some(effects) = self.tabs_command(&command) {
            return effects;
        }
        match command {
            Command::SelectByMask(rc_engine::MaskRequest { mask: None, .. }) => {
                return self.open_prompt(active, PromptKind::Select);
            }
            Command::DeselectByMask(rc_engine::MaskRequest { mask: None, .. }) => {
                return self.open_prompt(active, PromptKind::Deselect);
            }
            Command::Filter {
                pattern: None,
                last: false,
                ..
            } => return self.open_filter(active),
            // Alt+F5 and Alt+F9 ask in their dialog first (RC-030-03, RC-030-04).
            Command::ArchivePack { target: None, .. }
            | Command::ArchiveExtract { target: None, .. } => {
                return self.open_archive_dialog(active, command);
            }
            // Ctrl+N asks for the server first (RC-060-02).
            Command::QuickConnect(request) if request.host.is_empty() => {
                self.open_quick_connect();
                return Vec::new();
            }
            _ => {}
        }
        let Some(command) = self.pane_mut(active).through_parent_row(command) else {
            return Vec::new();
        };
        // Enter on a checksum file checks it (RC-020-10).
        if let Some(effects) = self.checksum_enter(active, &command) {
            return effects;
        }
        if let Some(effects) = self.properties_command(active, &command) {
            return effects;
        }
        vec![Effect::Dispatch(active, vec![command])]
    }

    /// Escape while a call on this machine's drives has waited a while (RC-080-04):
    /// `panel.cancel_io` runs at once, beside the queued commands - the next of them waits
    /// behind the one that hangs. The waiting command reports `cancelled` itself.
    fn io_escape(&self, chord: &Chord) -> Option<Vec<Effect>> {
        let plain = chord.is_plain();
        if !plain || chord.key != "Escape" || !self.engine.io_stalled() {
            return None;
        }
        let engine = self.engine.clone();
        let cancel = Command::CancelIo {
            panel: Some(self.active),
        };
        let task = Task::future(async move {
            let _ = engine.dispatch(cancel).await;
        })
        .discard();
        Some(vec![Effect::Task(task)])
    }

    /// Opens a prompt in `panel`, filled with the last mask or the panel's path, all of it
    /// selected so typing replaces it.
    pub(super) fn open_prompt(&mut self, panel: PanelId, kind: PromptKind) -> Vec<Effect> {
        let text = match kind {
            PromptKind::Path => self.location_text(&self.pane(panel).state.path),
            PromptKind::Select | PromptKind::Deselect => self
                .mask_history
                .first()
                .cloned()
                .unwrap_or_else(|| "*.*".to_owned()),
        };
        // A mask prompt starts on the newest kept mask, when there is one.
        let recalled = (kind != PromptKind::Path && !self.mask_history.is_empty()).then_some(0);
        self.prompt = Some(Prompt {
            panel,
            kind,
            text,
            recalled,
        });
        vec![focus_prompt()]
    }

    /// ArrowUp (`by` 1, older) or ArrowDown (-1, newer) in a mask prompt: the next kept mask.
    fn recall(&mut self, by: isize) -> Vec<Effect> {
        let Some(prompt) = self.prompt.as_mut().filter(|p| p.kind != PromptKind::Path) else {
            return Vec::new();
        };
        let next = match prompt.recalled {
            None if by > 0 => Some(0),
            None => None,
            Some(at) => at.checked_add_signed(by),
        };
        match next.and_then(|at| self.mask_history.get(at).map(|mask| (at, mask))) {
            Some((at, mask)) => {
                prompt.recalled = Some(at);
                prompt.text.clone_from(mask);
                vec![focus_prompt()]
            }
            None => Vec::new(),
        }
    }

    /// A kept mask clicked beside the field: it becomes the text, the field keeps the keys.
    pub(super) fn prompt_recall(&mut self, mask: String) -> Vec<Effect> {
        match &mut self.prompt {
            Some(prompt) => {
                prompt.recalled = self.mask_history.iter().position(|kept| *kept == mask);
                prompt.text = mask;
                vec![focus_prompt()]
            }
            None => Vec::new(),
        }
    }

    pub(super) fn prompt_input(&mut self, text: String) {
        if let Some(prompt) = &mut self.prompt {
            prompt.text = text;
            prompt.recalled = None;
        }
    }

    /// Enter in the prompt: its command for its panel. An empty prompt only closes.
    pub(super) fn submit_prompt(&mut self) -> Vec<Effect> {
        let Some(Prompt {
            panel, kind, text, ..
        }) = self.prompt.take()
        else {
            return Vec::new();
        };
        if text.trim().is_empty() {
            return Vec::new();
        }
        let command = match kind {
            PromptKind::Path => Command::Navigate {
                panel: Some(panel),
                path: text,
            },
            PromptKind::Select | PromptKind::Deselect => {
                mru::MASKS.remember(&mut self.mask_history, &text);
                let panel = Some(panel);
                let mask = Some(text);
                if kind == PromptKind::Select {
                    Command::SelectByMask(rc_engine::MaskRequest { panel, mask })
                } else {
                    Command::DeselectByMask(rc_engine::MaskRequest { panel, mask })
                }
            }
        };
        let mut effects = vec![Effect::Dispatch(panel, vec![command])];
        if kind != PromptKind::Path
            && let Some(db) = self.db.clone()
        {
            effects.push(Effect::Task(
                mru::MASKS.save_task(db, self.mask_history.clone()),
            ));
        }
        effects
    }
}

/// Focuses the prompt's field with its text selected, so typing replaces it.
fn focus_prompt() -> Effect {
    let id = Id::new(PROMPT_ID);
    Effect::Task(Task::batch([
        operation::focus(id.clone()),
        operation::select_all(id),
    ]))
}
