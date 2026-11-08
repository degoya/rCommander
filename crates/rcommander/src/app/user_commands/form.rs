//! The window's forms (RC-100-86): an own command's - its name, its command and its tooltip - and
//! a Start menu row's - its kind, its title and an entry's command -, both over the one command
//! model (`rc_engine::CommandRef`: kind, command, parameters, start folder, window, run as
//! administrator). A form is what is typed; the engine checks it when it is saved
//! (`usercmd.save`, `start_menu.save`) and says why it does not fit.

use rc_engine::{
    CommandKind, CommandRef, Show, StartItem, StartItemDraft, StartItemId, StartItemKind,
    UserCommand, UserCommandDraft, UserCommandId, user_commands::symbols,
};

use super::list::Place;
use crate::app::button_bar::SymbolKey;

/// A change of the command's fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CmdMsg {
    Kind(CommandKind),
    Command(String),
    Params(String),
    Folder(String),
    Show(Show),
    Elevated(bool),
}

/// `message` applied to `action`.
pub fn edit(action: &mut CommandRef, message: CmdMsg) {
    match message {
        CmdMsg::Kind(kind) => {
            action.kind = kind;
            if kind != CommandKind::Program {
                action.elevated = false;
            }
        }
        CmdMsg::Command(text) => action.command = text,
        CmdMsg::Params(text) => action.params = text,
        CmdMsg::Folder(text) => action.folder = text,
        CmdMsg::Show(show) => action.show = show,
        CmdMsg::Elevated(on) => action.elevated = on && action.kind == CommandKind::Program,
    }
}

/// A change of an own command's form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OwnMsg {
    Name(String),
    Tooltip(String),
    Cmd(CmdMsg),
    /// The symbol's file, as typed or chosen with Browse...
    IconPath(String),
    /// The icon's index in it, as typed; what is no number is 0, as on the button bar.
    IconIndex(String),
}

/// The form of an own command: a stored one (`id`) or a new one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnForm {
    pub id: Option<UserCommandId>,
    pub name: String,
    pub action: CommandRef,
    pub tooltip: String,
    /// The symbol: a picture, or a program, library or icon file with the icon's index - what a
    /// button that runs the command shows (RC-100-87); empty for none.
    pub icon_path: String,
    pub icon_index: i64,
    pub dirty: bool,
}

impl OwnForm {
    /// A new command: `em_` to type the rest of its name after, a program.
    pub fn blank() -> Self {
        Self {
            id: None,
            name: rc_engine::user_commands::OWN_PREFIX.to_owned(),
            action: CommandRef::default(),
            tooltip: String::new(),
            icon_path: String::new(),
            icon_index: 0,
            dirty: true,
        }
    }

    /// The stored `command`.
    pub fn of(command: &UserCommand) -> Self {
        Self {
            id: Some(command.id),
            name: command.name.clone(),
            action: command.action.clone(),
            tooltip: command.tooltip.clone(),
            icon_path: command.icon_path.clone(),
            icon_index: command.icon_index,
            dirty: false,
        }
    }

    pub fn update(&mut self, message: OwnMsg) {
        match message {
            OwnMsg::Name(name) => self.name = name,
            OwnMsg::Tooltip(tooltip) => self.tooltip = tooltip,
            OwnMsg::Cmd(message) => edit(&mut self.action, message),
            OwnMsg::IconPath(path) => self.icon_path = path,
            OwnMsg::IconIndex(text) => self.icon_index = text.trim().parse().unwrap_or(0),
        }
        self.dirty = true;
    }

    /// The symbol to preview at `size` pixels, read as the button bar reads its own
    /// (`button_bar::symbols`); `None` without a file - also for a symbol of the collection,
    /// which is a glyph ([`Self::collection_glyph`]).
    pub fn symbol_key(&self, size: u16) -> Option<SymbolKey> {
        let path = self.icon_path.trim();
        let file = !path.is_empty() && !symbols::is_reference(path);
        file.then(|| SymbolKey {
            path: path.to_owned(),
            index: self.icon_index,
            size,
        })
    }

    /// The glyph of the symbol of the collection the form names (RC-110-10); `None` for a file,
    /// nothing or a name the collection does not have.
    pub fn collection_glyph(&self) -> Option<char> {
        crate::theme::symbols::glyph_of(&self.icon_path)
    }

    /// A symbol of the collection chosen (`rc-symbol:<name>`): it replaces the file, its index 0.
    pub fn choose_symbol(&mut self, reference: String) {
        self.icon_path = reference;
        self.icon_index = 0;
        self.dirty = true;
    }

    /// What `usercmd.save` stores.
    pub fn draft(&self) -> UserCommandDraft {
        UserCommandDraft {
            name: self.name.trim().to_owned(),
            action: self.action.clone(),
            tooltip: self.tooltip.clone(),
            icon_path: self.icon_path.trim().to_owned(),
            icon_index: self.icon_index,
        }
    }
}

/// A change of a Start menu row's form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartMsg {
    Kind(StartItemKind),
    Title(String),
    Cmd(CmdMsg),
}

/// The form of a row of the Start menu: a stored one (`id`) or a new one and its place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartForm {
    pub id: Option<StartItemId>,
    /// Where a new row goes; a stored one keeps its place.
    pub place: Place,
    pub kind: StartItemKind,
    pub title: String,
    /// An entry's command; kept while the kind is another, so switching back loses nothing.
    pub action: CommandRef,
    pub dirty: bool,
}

impl StartForm {
    /// A new entry at `place`.
    pub fn new(place: Place) -> Self {
        Self {
            id: None,
            place,
            kind: StartItemKind::Item,
            title: String::new(),
            action: CommandRef::default(),
            dirty: true,
        }
    }

    /// The stored `item`.
    pub fn of(item: &StartItem) -> Self {
        Self {
            id: Some(item.id),
            place: Place {
                parent: item.parent,
                position: None,
            },
            kind: item.kind,
            title: item.title.clone(),
            action: item.action.clone().unwrap_or_default(),
            dirty: false,
        }
    }

    pub fn update(&mut self, message: StartMsg) {
        match message {
            StartMsg::Kind(kind) => self.kind = kind,
            StartMsg::Title(title) => self.title = title,
            StartMsg::Cmd(message) => edit(&mut self.action, message),
        }
        self.dirty = true;
    }

    /// Whether the form shows the command's fields: an entry's.
    pub fn has_command(&self) -> bool {
        self.kind == StartItemKind::Item
    }

    /// What `start_menu.save` stores: the title of an entry or a submenu, an entry's command.
    pub fn draft(&self) -> StartItemDraft {
        StartItemDraft {
            kind: self.kind,
            title: match self.kind {
                StartItemKind::Separator => String::new(),
                _ => self.title.clone(),
            },
            action: self.has_command().then(|| self.action.clone()),
        }
    }
}

#[cfg(test)]
#[path = "form_tests.rs"]
mod tests;
