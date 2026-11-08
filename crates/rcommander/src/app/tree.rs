//! The folder trees of the window (RC-100-88), after Total Commander: the tree in a panel's place
//! (Ctrl+F8, `PanelView::Tree`), whose cursor the other panel follows; the separate tree beside
//! the panels (Ctrl+Shift+F8, `panels.separate_tree`) - one for the active panel, or one beside
//! each - which follows its panel's folder and leads it; and the tree dialog (Alt+F10,
//! `tree/dialog.rs`), which sends the active panel to the folder chosen.
//!
//! Each tree is a [`DirTree`] of the folders read so far (`tree/model.rs`); the levels come from
//! the engine off the UI thread ([`TreeCall`]), lazily and kept there for every tree. A panel
//! follows a cursor that rested for [`FOLLOW_PAUSE`], or only after Enter with
//! `tree.open_on_enter` on ([`Follow`]). The keys while a tree has the keyboard are
//! `tree/keys.rs`, the drawing `tree/view.rs`; the width of the separate tree is dragged at its
//! edge as the divider is (`resize.rs`).

pub(super) mod dialog;
mod keys;
pub(super) mod model;
mod view;

use iced::Task;
use rc_db::Db;
use rc_engine::{
    Engine, PanelChange, PanelId, PanelView, VfsPath,
    settings::{SEPARATE_TREE_KEY, TREE_OPEN_ON_ENTER_KEY, stored_bool, stored_setting},
    shown_path,
    tree::{FOLLOW_PAUSE, Follow, SeparateTree, TreeLevel, tree_root},
};

use self::dialog::TreeDialog;
pub(super) use self::dialog::{
    body as dialog_body, overlay as dialog_overlay, title as dialog_title,
};
use self::model::{Change, DirTree};
pub(super) use self::view::{panel_body, separate_beside, separate_one};
use super::{App, Effect, Message};
use crate::theme;

/// Which tree a message is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeId {
    /// The tree in the panel's place (Ctrl+F8).
    Panel(PanelId),
    /// The separate tree of the panel.
    Separate(PanelId),
    /// The tree dialog (Alt+F10).
    Dialog,
}

#[derive(Debug, Clone)]
pub enum TreeMsg {
    /// The engine's answer for a folder's level.
    Arrived(TreeId, VfsPath, Result<TreeLevel, &'static str>),
    /// The pause after the cursor's move of a turn ended.
    Rested(TreeId, u64),
    /// A click on a row: the cursor goes there, the tree takes the keyboard.
    Click(TreeId, usize),
    /// A double-click on a row: its folder is shown now, as Enter does.
    Open(TreeId, usize),
    /// A click on a row's sign: it unfolds or folds.
    Toggle(TreeId, usize),
    Scroll(TreeId, i32),
    /// The rows' area was measured: its height.
    Viewport(TreeId, f32),
    /// `panels.separate_tree` and `tree.open_on_enter`, read.
    Settings(SeparateTree, bool),
    /// The dialog's OK.
    DialogOk,
}

/// Work for the engine a tree hands out; its answer comes back as a message.
#[derive(Debug)]
pub enum TreeCall {
    /// The level of `path` for `tree`.
    Load { tree: TreeId, path: VfsPath },
    /// `panel` shows `path`, which a tree's cursor rested on or Enter chose.
    Show { panel: PanelId, path: VfsPath },
}

impl TreeCall {
    pub async fn run(self, engine: Engine) -> Message {
        match self {
            Self::Load { tree, path } => {
                let answer = engine
                    .tree_children(&path)
                    .await
                    .map_err(|error| error.code());
                Message::Tree(TreeMsg::Arrived(tree, path, answer))
            }
            Self::Show { panel, path } => {
                let shown = engine
                    .show_folder(panel, path)
                    .await
                    .map(|_| ())
                    .map_err(|error| error.code());
                Message::Done(panel, shown)
            }
        }
    }
}

/// A tree a panel follows, or that follows its panel.
#[derive(Debug)]
pub struct PanelTree {
    pub tree: DirTree,
    pub follow: Follow,
}

/// The window's trees and their settings.
#[derive(Debug, Default)]
pub struct TreeUi {
    /// The tree each panel shows in its place, by [`PanelId::index`].
    pub panels: [Option<PanelTree>; 2],
    /// Each panel's separate tree, while `panels.separate_tree` is not off.
    pub separate: [Option<PanelTree>; 2],
    pub mode: SeparateTree,
    /// The active panel's separate tree has the keyboard, not its list (Shift+Tab).
    pub focus: bool,
    pub on_enter: bool,
    /// The tree dialog, while it is open.
    pub dialog: Option<TreeDialog>,
}

/// Whether the setting `key` is one of the trees' read here; the width is `resize.rs`'s.
pub fn concerns(key: &str) -> bool {
    key == SEPARATE_TREE_KEY || key == TREE_OPEN_ON_ENTER_KEY
}

/// The separate tree's mode and whether only Enter leads a panel, as stored.
pub async fn load(db: Db) -> (SeparateTree, bool) {
    let mode = stored_setting(&db, SEPARATE_TREE_KEY)
        .await
        .as_ref()
        .and_then(serde_json::Value::as_str)
        .map(SeparateTree::from_setting)
        .unwrap_or_default();
    (mode, stored_bool(&db, TREE_OPEN_ON_ENTER_KEY).await)
}

/// A tree of the location of `path`, its root's folders not read yet.
fn tree_of(path: &VfsPath, hidden: bool) -> DirTree {
    let root = tree_root(path);
    let label = shown_path(&root).display().to_string();
    DirTree::new(root, label, hidden)
}

/// The calls that read the levels of `paths` for `tree`.
fn loads(tree: TreeId, paths: Vec<VfsPath>) -> Vec<Effect> {
    paths
        .into_iter()
        .map(|path| Effect::Tree(TreeCall::Load { tree, path }))
        .collect()
}

impl App {
    /// The tree `id`, while it is there.
    fn tree_mut(&mut self, id: TreeId) -> Option<&mut DirTree> {
        match id {
            TreeId::Panel(panel) => self.tree.panels[panel.index()]
                .as_mut()
                .map(|kept| &mut kept.tree),
            TreeId::Separate(panel) => self.tree.separate[panel.index()]
                .as_mut()
                .map(|kept| &mut kept.tree),
            TreeId::Dialog => self.tree.dialog.as_mut().map(|dialog| &mut dialog.tree),
        }
    }

    /// The tree `id` with its following panel; none for the dialog.
    fn panel_tree_mut(&mut self, id: TreeId) -> Option<&mut PanelTree> {
        match id {
            TreeId::Panel(panel) => self.tree.panels[panel.index()].as_mut(),
            TreeId::Separate(panel) => self.tree.separate[panel.index()].as_mut(),
            TreeId::Dialog => None,
        }
    }

    pub(super) fn on_tree(&mut self, message: TreeMsg) -> Vec<Effect> {
        match message {
            TreeMsg::Arrived(id, path, answer) => {
                let next = self
                    .tree_mut(id)
                    .map(|tree| tree.arrived(&path, answer))
                    .unwrap_or_default();
                loads(id, next)
            }
            TreeMsg::Rested(id, turn) => {
                let shown = self
                    .panel_tree_mut(id)
                    .and_then(|kept| kept.follow.rested(turn));
                shown.map_or_else(Vec::new, |path| self.follow_to(id, path))
            }
            TreeMsg::Click(id, row) => {
                let mut effects = self.tree_clicked(id);
                let change = self
                    .tree_mut(id)
                    .map_or(Change::Nothing, |tree| tree.click(row));
                effects.extend(self.tree_changed(id, change));
                effects
            }
            TreeMsg::Open(id, row) => {
                let mut effects = self.tree_clicked(id);
                if let Some(tree) = self.tree_mut(id) {
                    tree.click(row);
                }
                effects.extend(self.tree_enter(id));
                effects
            }
            TreeMsg::Toggle(id, row) => {
                let change = self
                    .tree_mut(id)
                    .map_or(Change::Nothing, |tree| tree.toggle(row));
                self.tree_changed(id, change)
            }
            TreeMsg::Scroll(id, rows) => {
                if let Some(tree) = self.tree_mut(id) {
                    tree.scroll(rows);
                }
                Vec::new()
            }
            TreeMsg::Viewport(id, height) => {
                let rows = (height / theme::ROW_HEIGHT).floor().max(1.0) as usize;
                if let Some(tree) = self.tree_mut(id) {
                    tree.set_height(rows);
                }
                Vec::new()
            }
            TreeMsg::Settings(mode, on_enter) => self.tree_settings(mode, on_enter),
            TreeMsg::DialogOk => self.tree_dialog_ok(),
        }
    }

    /// A click in tree `id`: a separate tree takes the keyboard and its panel becomes active; a
    /// click in the tree in a panel's place gives the keyboard back to it.
    fn tree_clicked(&mut self, id: TreeId) -> Vec<Effect> {
        let TreeId::Separate(panel) = id else {
            if matches!(id, TreeId::Panel(_)) {
                self.tree.focus = false;
            }
            return Vec::new();
        };
        self.tree.focus = true;
        if panel == self.active {
            return Vec::new();
        }
        let activate = rc_engine::Command::SwitchActivePanel { panel: Some(panel) };
        vec![Effect::Dispatch(panel, vec![activate])]
    }

    /// What a key or a click changed in tree `id`: the engine hears of a fold, a level is read,
    /// the following panel waits for the cursor to rest.
    pub(super) fn tree_changed(&mut self, id: TreeId, change: Change) -> Vec<Effect> {
        match change {
            Change::Nothing => Vec::new(),
            Change::Cursor => self.cursor_moved(id),
            Change::Unfolded { path, load } => {
                self.engine.tree_unfolded(&path);
                if load {
                    loads(id, vec![path])
                } else {
                    Vec::new()
                }
            }
            Change::Folded(path) => {
                self.engine.tree_folded(&path);
                Vec::new()
            }
        }
    }

    /// The cursor of tree `id` moved: its panel follows once it rested.
    fn cursor_moved(&mut self, id: TreeId) -> Vec<Effect> {
        let Some(kept) = self.panel_tree_mut(id) else {
            return Vec::new();
        };
        let Some(path) = kept.tree.cursor_path().cloned() else {
            return Vec::new();
        };
        let Some(turn) = kept.follow.moved(path) else {
            return Vec::new();
        };
        let pause = Task::perform(tokio::time::sleep(FOLLOW_PAUSE), move |()| {
            Message::Tree(TreeMsg::Rested(id, turn))
        });
        vec![Effect::Task(pause)]
    }

    /// Enter in tree `id`: its panel shows the folder under the cursor now.
    pub(super) fn tree_enter(&mut self, id: TreeId) -> Vec<Effect> {
        if id == TreeId::Dialog {
            return self.tree_dialog_ok();
        }
        let Some(kept) = self.panel_tree_mut(id) else {
            return Vec::new();
        };
        let Some(cursor) = kept.tree.cursor_path().cloned() else {
            return Vec::new();
        };
        let path = kept.follow.enter(cursor);
        self.follow_to(id, path)
    }

    /// The panel tree `id` leads shows `path`: the other panel of a tree in a panel, the own
    /// panel of a separate tree.
    fn follow_to(&self, id: TreeId, path: VfsPath) -> Vec<Effect> {
        let panel = match id {
            TreeId::Panel(panel) => panel.other(),
            TreeId::Separate(panel) => panel,
            TreeId::Dialog => return Vec::new(),
        };
        vec![Effect::Tree(TreeCall::Show { panel, path })]
    }

    /// The settings read: the separate trees come or go, every panel follows as they say.
    fn tree_settings(&mut self, mode: SeparateTree, on_enter: bool) -> Vec<Effect> {
        self.tree.mode = mode;
        self.tree.on_enter = on_enter;
        for kept in self
            .tree
            .panels
            .iter_mut()
            .chain(&mut self.tree.separate)
            .flatten()
        {
            kept.follow.set_on_enter(on_enter);
        }
        if mode == SeparateTree::Off {
            self.tree.separate = [None, None];
            self.tree.focus = false;
            return Vec::new();
        }
        let mut effects = Vec::new();
        for panel in PanelId::BOTH {
            effects.extend(self.separate_follows(panel));
        }
        effects
    }

    /// `panel` changed as `change` says (after missed events: as navigated).
    pub(super) fn tree_follows_panel(
        &mut self,
        panel: PanelId,
        change: PanelChange,
    ) -> Vec<Effect> {
        if matches!(
            change,
            PanelChange::Cursor | PanelChange::Selection | PanelChange::Git
        ) {
            return Vec::new();
        }
        let state = &self.pane(panel).state;
        let (view, path, hidden) = (state.view, state.path.clone(), state.show_hidden);
        let index = panel.index();
        // A terminal has the keyboard, never a tree beside it.
        if view == PanelView::Terminal && panel == self.active {
            self.tree.focus = false;
        }
        let mut effects = Vec::new();
        // The tree in the panel's place: made when the panel turned to it or moved to another
        // location, gone when it left it.
        let root = tree_root(&path);
        let made = self.tree.panels[index]
            .as_ref()
            .is_some_and(|kept| kept.tree.root() == &root && kept.tree.hidden == hidden);
        match (view == PanelView::Tree, made) {
            (true, false) => {
                let mut tree = tree_of(&path, hidden);
                let paths = tree.reveal(&path);
                self.tree.panels[index] = Some(PanelTree {
                    tree,
                    follow: Follow::new(self.tree.on_enter),
                });
                effects.extend(loads(TreeId::Panel(panel), paths));
            }
            (false, _) => self.tree.panels[index] = None,
            (true, true) => {}
        }
        // The other panel's tree knows what this one, its follower, shows now.
        if let Some(kept) = self.tree.panels[panel.other().index()].as_mut() {
            kept.follow.showing(&path);
            effects.extend(kept.tree.reload(&path).map(|path| {
                Effect::Tree(TreeCall::Load {
                    tree: TreeId::Panel(panel.other()),
                    path,
                })
            }));
        }
        effects.extend(self.separate_follows(panel));
        effects
    }

    /// `panel`'s separate tree, while the setting wants one, follows the panel's folder: made
    /// for a new location or a change of the hidden folders, else unfolded to the folder - not
    /// while its own cursor waits to lead the panel - and the folder's level read again.
    fn separate_follows(&mut self, panel: PanelId) -> Vec<Effect> {
        if self.tree.mode == SeparateTree::Off {
            return Vec::new();
        }
        let state = &self.pane(panel).state;
        let (path, hidden) = (state.path.clone(), state.show_hidden);
        let id = TreeId::Separate(panel);
        let on_enter = self.tree.on_enter;
        // A tree with the keyboard whose cursor waits to lead the panel keeps its cursor.
        let leads = self.tree.focus && panel == self.active;
        let root = tree_root(&path);
        let slot = &mut self.tree.separate[panel.index()];
        let stale = slot
            .as_ref()
            .is_none_or(|kept| kept.tree.root() != &root || kept.tree.hidden != hidden);
        if stale {
            *slot = Some(PanelTree {
                tree: tree_of(&path, hidden),
                follow: Follow::new(on_enter),
            });
        }
        let Some(kept) = slot.as_mut() else {
            return Vec::new();
        };
        let waiting = leads && kept.follow.waiting();
        if !waiting {
            kept.follow.forget();
        }
        kept.follow.showing(&path);
        let mut paths = Vec::new();
        if !waiting {
            paths.extend(kept.tree.reveal(&path));
        }
        paths.extend(kept.tree.reload(&path));
        loads(id, paths)
    }

    /// Reads `panels.separate_tree` and `tree.open_on_enter` (the start, and after a change).
    pub(super) fn tree_settings_task(&self) -> Task<Message> {
        let Some(db) = self.db.clone() else {
            return Task::none();
        };
        Task::perform(load(db), |(mode, on_enter)| {
            Message::Tree(TreeMsg::Settings(mode, on_enter))
        })
    }
}

#[cfg(test)]
#[path = "tree_tests.rs"]
mod tests;
