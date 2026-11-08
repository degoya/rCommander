//! The tree dialog (Alt+F10, RC-100-88), Total Commander's `cm_CdTree`: the folder tree of the
//! active panel's drive, unfolded to the panel's folder. Enter or OK sends the panel to the folder
//! under the cursor, Escape or Cancel closes the dialog, F2 reads a folder again, typed letters
//! jump among the folders read - lazily, without a scan of the drive (the deviation from Total
//! Commander's `treeinfo.wc` is reasoned in the job file). A modal dialog in a window of its own
//! (RC-080-23), or a card over the main window while `appearance.dialog_windows` is off; its
//! list and its buttons are the stops of its focus ring.

use iced::{
    Element, Length,
    widget::{column, container},
};
use rc_engine::{Chord, PanelId};

use super::{DirTree, TreeCall, TreeId, TreeMsg, loads, tree_of, view::tree_list};
use crate::{
    app::{App, Effect, Message, field_walk},
    i18n::t,
    theme::{self, Tokens, focus, layout},
};

/// The card's height over the main window, the list's room with the buttons.
const CARD_HEIGHT: f32 = 440.0;

/// The open dialog: the panel it was opened for and its tree.
#[derive(Debug)]
pub struct TreeDialog {
    pub panel: PanelId,
    pub tree: DirTree,
}

impl App {
    /// Alt+F10: the dialog opens on the active panel's drive, unfolded to its folder.
    pub(in crate::app) fn open_tree_dialog(&mut self) -> Vec<Effect> {
        let panel = self.active;
        let state = &self.pane(panel).state;
        let (path, hidden) = (state.path.clone(), state.show_hidden);
        let mut tree = tree_of(&path, hidden);
        let paths = tree.reveal(&path);
        self.tree.dialog = Some(TreeDialog { panel, tree });
        loads(TreeId::Dialog, paths)
    }

    /// A key while the dialog is open: Tab walks its ring, Escape closes it, the cursor keys,
    /// Enter and F2 are its tree's.
    pub(in crate::app) fn tree_dialog_key(&mut self, chord: &Chord) -> Vec<Effect> {
        if let Some(effects) = field_walk::walk_key(chord) {
            return effects;
        }
        if chord.is_plain() && chord.key == "Escape" {
            self.tree.dialog = None;
            return Vec::new();
        }
        self.tree_nav_key(TreeId::Dialog, chord).unwrap_or_default()
    }

    /// Enter or OK: the dialog closes and its panel shows the folder under the cursor.
    pub(super) fn tree_dialog_ok(&mut self) -> Vec<Effect> {
        let Some(dialog) = self.tree.dialog.take() else {
            return Vec::new();
        };
        let Some(path) = dialog.tree.cursor_path().cloned() else {
            return Vec::new();
        };
        vec![Effect::Tree(TreeCall::Show {
            panel: dialog.panel,
            path,
        })]
    }
}

/// The dialog's title, in its window's title bar or its card's head.
pub fn title() -> &'static str {
    t!("tree.dialog.title")
}

/// The dialog's content: the tree, one stop of the ring, over the button row.
pub fn body<'a>(dialog: &'a TreeDialog, tokens: &'static Tokens) -> Element<'a, Message> {
    let list = container(tree_list(TreeId::Dialog, &dialog.tree, true, tokens))
        .padding(1)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| theme::panel(tokens, true));
    // The arrows move the tree's cursor in the dialog's own keys.
    let list = focus::choice(list, t!("tree.dialog.list"), None, None);
    let ok = layout::primary_button(t!("tree.dialog.ok"), Some(Message::Tree(TreeMsg::DialogOk)));
    let cancel = layout::secondary_button(t!("tree.dialog.cancel"), Some(Message::close_dialog()));
    column![list, layout::button_row(Some(ok), [cancel], [])]
        .spacing(12)
        .height(Length::Fill)
        .into()
}

/// The dialog as a card over the main window.
pub fn overlay<'a>(dialog: &'a TreeDialog, tokens: &'static Tokens) -> Element<'a, Message> {
    let content = container(body(dialog, tokens)).height(CARD_HEIGHT);
    Message::overlay(title(), content, layout::MEDIUM_DIALOG_WIDTH, tokens)
}
