//! Editing the tree in sight (RC-100-63): the actions of the toolbar above the tree and of its
//! context menu - rename, up, down, outdent, indent, the drive-bar star, delete, and in the menu
//! also a new connection, a new folder and a copy - each with the key that does the same
//! (`keys.rs`); dragging with the insertion line (`drop.rs`); the star of a folder. What a move
//! is, is `arrange.rs` and `drop.rs`; the menu itself `tree_menu.rs`.

use rc_engine::{Command, TreeItem};

use super::{
    ConnectionManager,
    arrange::{self, Arrange, Move},
    drop::{self, DropAt},
    jobs::Then,
    run,
    tree_menu::TreeMenu,
};
use crate::{app::Effect, i18n::t, theme::icon::Icon};

/// What the tree adds to the manager while it is edited.
#[derive(Debug, Default)]
pub struct TreeEdit {
    /// Where the running drag would drop; drawn as the insertion line or the target's highlight.
    pub hover: Option<DropAt>,
    /// The open context menu.
    pub menu: Option<TreeMenu>,
}

/// What the toolbar, the menu and the dragging say.
#[derive(Debug, Clone)]
pub enum TreeMsg {
    /// A toolbar button or a menu entry.
    Action(TreeAction),
    /// The pointer moved over a row while a drag runs, `y` pixels below its top.
    Hover(TreeItem, f32),
    /// The pointer moved below the rows while a drag runs.
    HoverEnd,
    /// Released below the rows: the dragged node goes to the end of the root.
    DropEnd,
    /// The pointer left the tree's list: a drag ends without a drop.
    Leave,
    /// A right click on a row.
    Context(TreeItem),
    /// Where the open menu stands in the tree's side, once measured; `None` when the tree is
    /// not drawn.
    Placed(Option<iced::Point>),
    /// The pointer is over the menu's entry at this index.
    MenuHover(usize),
    /// A click beside the menu.
    MenuClose,
}

/// One action on the tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeAction {
    Rename,
    Up,
    Down,
    Outdent,
    Indent,
    /// The drive-bar star of a connection or a folder.
    Star,
    Delete,
    NewConnection,
    NewFolder,
    Duplicate,
}

impl TreeAction {
    /// The toolbar's buttons, in their order.
    pub const TOOLBAR: [Self; 7] = [
        Self::Rename,
        Self::Up,
        Self::Down,
        Self::Outdent,
        Self::Indent,
        Self::Star,
        Self::Delete,
    ];

    /// The context menu's entries, in their order.
    pub const MENU: [Self; 10] = [
        Self::Rename,
        Self::Up,
        Self::Down,
        Self::Outdent,
        Self::Indent,
        Self::Star,
        Self::Delete,
        Self::NewConnection,
        Self::NewFolder,
        Self::Duplicate,
    ];

    pub const fn icon(self) -> Icon {
        match self {
            Self::Rename => Icon::Rename,
            Self::Up => Icon::MoveUp,
            Self::Down => Icon::MoveDown,
            Self::Outdent => Icon::Outdent,
            Self::Indent => Icon::Indent,
            Self::Star => Icon::Star,
            Self::Delete => Icon::Delete,
            Self::NewConnection => Icon::Add,
            Self::NewFolder => Icon::NewFolder,
            Self::Duplicate => Icon::Copy,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Rename => t!("connections.tree.rename"),
            Self::Up => t!("connections.tree.up"),
            Self::Down => t!("connections.tree.down"),
            Self::Outdent => t!("connections.tree.outdent"),
            Self::Indent => t!("connections.tree.indent"),
            Self::Star => t!("connections.tree.star"),
            Self::Delete => t!("connections.tree.delete"),
            Self::NewConnection => t!("connections.tree.new_connection"),
            Self::NewFolder => t!("connections.tree.new_folder"),
            Self::Duplicate => t!("connections.tree.duplicate"),
        }
    }

    /// The key that does the same in the tree (`keys.rs`), as the keymap writes a chord.
    pub const fn keys(self) -> &'static str {
        match self {
            Self::Rename => "F2",
            Self::Up => "Ctrl+ArrowUp",
            Self::Down => "Ctrl+ArrowDown",
            Self::Outdent => "Ctrl+ArrowLeft",
            Self::Indent => "Ctrl+ArrowRight",
            Self::Star => "Space",
            Self::Delete => "Delete",
            Self::NewConnection => "Insert",
            Self::NewFolder => "F7",
            Self::Duplicate => "Shift+F5",
        }
    }

    /// The way it arranges the tree, for the four moves.
    const fn arrange(self) -> Option<Arrange> {
        match self {
            Self::Up => Some(Arrange::Up),
            Self::Down => Some(Arrange::Down),
            Self::Outdent => Some(Arrange::Outdent),
            Self::Indent => Some(Arrange::Indent),
            _ => None,
        }
    }
}

impl ConnectionManager {
    /// A message of the toolbar, the menu or the dragging.
    pub(super) fn on_tree(&mut self, message: TreeMsg) -> Vec<Effect> {
        match message {
            TreeMsg::Action(action) => {
                self.edit.menu = None;
                self.act(action)
            }
            TreeMsg::Hover(item, y) => {
                if self.drag.is_some() {
                    let folder = matches!(item, TreeItem::Folder(_));
                    let zone = drop::zone(y, crate::theme::ROW_HEIGHT, folder);
                    self.edit.hover = Some(DropAt::Row(item, zone));
                }
                Vec::new()
            }
            TreeMsg::HoverEnd => {
                if self.drag.is_some() {
                    self.edit.hover = Some(DropAt::End);
                }
                Vec::new()
            }
            TreeMsg::DropEnd => self.drop_at(DropAt::End),
            TreeMsg::Leave => {
                self.drag = None;
                self.edit.hover = None;
                Vec::new()
            }
            TreeMsg::Context(item) => {
                self.tree.cursor = Some(item);
                self.show(Some(item));
                self.open_menu()
            }
            message => self.menu_message(message),
        }
    }

    /// Whether `action` can run on the node at the cursor: the toolbar greys out the others.
    pub(super) fn can(&self, action: TreeAction) -> bool {
        let current = self.tree.current();
        if let Some(how) = action.arrange() {
            return current.is_some_and(|item| arrange::plan(&self.tree, item, how).is_some());
        }
        match action {
            TreeAction::Rename | TreeAction::Delete => current.is_some(),
            TreeAction::Star => {
                matches!(current, Some(TreeItem::Folder(_) | TreeItem::Connection(_)))
            }
            TreeAction::Duplicate => matches!(current, Some(TreeItem::Connection(_))),
            _ => true,
        }
    }

    /// Runs `action` on the node at the cursor, as its key does.
    pub(super) fn act(&mut self, action: TreeAction) -> Vec<Effect> {
        if let Some(how) = action.arrange() {
            return self.arrange(how);
        }
        match action {
            TreeAction::Rename => self.ask_rename(),
            TreeAction::Star => self.toggle_favourite(),
            TreeAction::Delete => self.ask_delete(),
            TreeAction::NewConnection => self.new_connection(),
            TreeAction::NewFolder => self.ask_name(None),
            TreeAction::Duplicate => self.duplicate(),
            TreeAction::Up | TreeAction::Down | TreeAction::Outdent | TreeAction::Indent => {
                Vec::new()
            }
        }
    }

    /// Ctrl+arrow, a toolbar button or a menu entry: the node at the cursor moves `how`.
    pub(super) fn arrange(&mut self, how: Arrange) -> Vec<Effect> {
        let Some(item) = self.tree.current() else {
            return Vec::new();
        };
        match arrange::plan(&self.tree, item, how) {
            Some(planned) => self.apply_move(item, planned),
            None => Vec::new(),
        }
    }

    /// A drag released at `at`: the dragged node moves there.
    pub(super) fn drop_at(&mut self, at: DropAt) -> Vec<Effect> {
        self.edit.hover = None;
        let Some(item) = self.drag.take() else {
            return Vec::new();
        };
        match drop::plan(&self.tree, item, at) {
            Some(planned) => self.apply_move(item, planned),
            None => Vec::new(),
        }
    }

    /// Runs `planned` for `item`: the folder it goes into opens, the cursor stays on it.
    fn apply_move(&mut self, item: TreeItem, planned: Move) -> Vec<Effect> {
        if let Some(folder) = planned.open {
            self.tree.fold(folder, Some(true));
        }
        self.tree.cursor = Some(item);
        self.pending = None;
        if self.clipboard == Some(item) {
            self.clipboard = None;
        }
        vec![run(Then::Select, planned.command)]
    }

    /// Space or the star on a folder: the drive bar shows it as a button, or no longer. The
    /// star shows at once; the tree read after the next change says what is stored.
    pub(super) fn toggle_folder_favourite(&mut self, id: rc_engine::FolderId) -> Vec<Effect> {
        let favourite = !self.tree.on_bar(id);
        let marked = &mut self.tree.tree.bar_folders;
        marked.retain(|folder| *folder != id);
        if favourite {
            marked.push(id);
        }
        vec![run(
            Then::Drives,
            Command::SetFolderFavourite { id, favourite },
        )]
    }
}

#[cfg(test)]
#[path = "tree_edit_tests.rs"]
mod tests;
