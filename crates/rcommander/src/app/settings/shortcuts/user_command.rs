//! The Shortcuts tab's "Add key for an own command..." and "Add key for a Start menu entry..."
//! (RC-100-86), as "Add key for a workspace..." (`workspace.rs`): a panel under the table chooses
//! an own command - "Command: Notepad", as the command palette names it - or an entry of the Start
//! menu - "Start: Notepad" -, then "Record key" takes the next key pressed as the recording of
//! "Add key" does, the question before a taken key moves included. The key becomes a row of its
//! own - an own command's named by the command itself, `em_<name>`, as Total Commander binds it,
//! an entry's `start:<id>` -, which the table shows, changes, removes and resets like any other
//! and OK or Apply store in `keys.overrides`. The own commands' window's "Add key..." opens the
//! panel with its command or entry chosen.
//!
//! Keys while the panel is open: Enter records, Escape closes it, the arrows and Delete leave the
//! table alone; Tab walks the panel's controls.

use std::fmt;

use iced::{
    Alignment, Element, Length,
    widget::{column, pick_list, row},
};
use rc_engine::{StartItem, StartItemKind, UserCommand, own_command_key, start_key_name};

use super::{Recording, Selected, ShortcutsMsg, page_message};
use crate::{
    app::{
        App, Effect, Message,
        palette::user_commands::{command_label, shown_name, start_label},
    },
    i18n::t,
    theme::{self, Tokens, fit, focus, layout},
};

/// What the panel adds a key for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandKeyKind {
    /// An own command `em_<name>`.
    Own,
    /// An entry of the Start menu.
    Start,
}

/// The open panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandPick {
    pub kind: CommandKeyKind,
    /// The key's name of the command or entry chosen; none before one is.
    pub name: Option<String>,
}

/// A command or an entry in the panel's list: its key's name and its words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOption {
    pub name: String,
    pub label: String,
}

impl fmt::Display for CommandOption {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.label)
    }
}

#[derive(Debug, Clone)]
pub enum CommandKeyMsg {
    /// "Add key for an own command..." or "...for a Start menu entry..." pressed.
    Open(CommandKeyKind),
    /// The own commands' window's "Add key...": the panel with this key's name chosen.
    OpenFor(String),
    Pick(CommandOption),
    /// "Record key": the next key pressed is the command's or the entry's.
    Record,
    Cancel,
}

/// The panel's list for `kind`: every own command of `commands` or every entry of `items`, in
/// their order, as the palette names them.
pub fn options(
    kind: CommandKeyKind,
    commands: &[UserCommand],
    items: &[StartItem],
) -> Vec<CommandOption> {
    match kind {
        CommandKeyKind::Own => commands
            .iter()
            .map(|command| CommandOption {
                name: command.name.clone(),
                label: command_label(shown_name(command)),
            })
            .collect(),
        CommandKeyKind::Start => items
            .iter()
            .filter(|item| item.kind == StartItemKind::Item)
            .map(|item| CommandOption {
                name: start_key_name(item.id),
                label: start_label(&item.title),
            })
            .collect(),
    }
}

/// What a key's `name` is for: an own command's or a Start menu entry's.
pub fn kind_of(name: &str) -> CommandKeyKind {
    if own_command_key(name).is_some() {
        CommandKeyKind::Own
    } else {
        CommandKeyKind::Start
    }
}

impl App {
    /// A message of the panel.
    pub(in crate::app::settings) fn command_key_message(
        &mut self,
        message: CommandKeyMsg,
    ) -> Vec<Effect> {
        let Some(dialog) = self.settings.as_mut() else {
            return Vec::new();
        };
        let page = &mut dialog.shortcuts;
        let idle = page.recording.is_none() && page.asking.is_none();
        match message {
            CommandKeyMsg::Open(_) | CommandKeyMsg::OpenFor(_) if !idle => {}
            CommandKeyMsg::Open(kind) => {
                page.adding = None;
                page.adding_workspace = None;
                let commands = crate::app::user_commands::known::commands();
                let items = crate::app::user_commands::known::start_menu();
                let first = options(kind, &commands, &items).into_iter().next();
                page.adding_command = Some(CommandPick {
                    kind,
                    name: first.map(|option| option.name),
                });
                dialog.note = None;
            }
            CommandKeyMsg::OpenFor(name) => {
                page.adding = None;
                page.adding_workspace = None;
                page.adding_command = Some(CommandPick {
                    kind: kind_of(&name),
                    name: Some(name),
                });
                dialog.note = None;
            }
            CommandKeyMsg::Pick(option) => {
                if let Some(adding) = page.adding_command.as_mut() {
                    adding.name = Some(option.name);
                }
            }
            CommandKeyMsg::Record => {
                if let Some(name) = page.adding_command.take().and_then(|adding| adding.name) {
                    page.selected = Some(Selected {
                        name,
                        keys: String::new(),
                    });
                    page.recording = Some(Recording::Add);
                }
            }
            CommandKeyMsg::Cancel => page.adding_command = None,
        }
        Vec::new()
    }
}

/// The panel: the command or the entry to choose among `options`, then "Record key" or "Cancel".
pub fn panel<'a>(
    adding: &CommandPick,
    options: Vec<CommandOption>,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let chosen = adding.name.as_deref();
    let selected = options
        .iter()
        .find(|option| Some(option.name.as_str()) == chosen)
        .cloned();
    let pick =
        |option: CommandOption| page_message(ShortcutsMsg::Command(CommandKeyMsg::Pick(option)));
    let (caption, help) = match adding.kind {
        CommandKeyKind::Own => (
            t!("settings.shortcuts.user_command"),
            t!("settings.shortcuts.user_command.help"),
        ),
        CommandKeyKind::Start => (
            t!("settings.shortcuts.start_item"),
            t!("settings.shortcuts.start_item.help"),
        ),
    };
    let moves = focus::moves(&options, selected.as_ref(), &pick);
    let list = pick_list(options, selected, pick)
        .text_size(theme::TEXT_SIZE)
        .width(Length::Fill);
    let list = layout::keyed(focus::Stop::pick(list, caption, moves), caption);
    let message = |message| Some(page_message(ShortcutsMsg::Command(message)));
    column![
        fit::prose(help).color(tokens.text_secondary),
        row![layout::caption(caption, None), list]
            .spacing(8)
            .align_y(Alignment::Center),
        row![
            layout::secondary_button(
                t!("settings.shortcuts.user_command.record"),
                chosen.and_then(|_| message(CommandKeyMsg::Record)),
            ),
            layout::secondary_button(
                t!("settings.shortcuts.user_command.cancel"),
                message(CommandKeyMsg::Cancel),
            ),
        ]
        .spacing(layout::BUTTON_SPACING),
    ]
    .spacing(8)
    .into()
}

#[cfg(test)]
#[path = "user_command_tests.rs"]
mod tests;
