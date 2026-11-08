//! The keys of the dialogs and menus that come after the intercepts (`intercepts.rs`) in
//! [`App::apply`]: the archive and checksum dialogs, the database dialog, the properties, the
//! quick connection, the drive menu, the context menu and the tabs' popups and hotlist dialog -
//! over the main window, or in a modal window of their own (RC-080-23) -, the tree dialog and,
//! last, a folder tree of the main window that has the keyboard (RC-100-88). The first one open
//! takes the key; a tree takes the letters typed too.

use super::{App, Effect, Message, cmdline::CmdLineMsg, tree::TreeId};

impl App {
    /// The effects of the dialog or menu that takes `message`; `None` for a message none of
    /// them takes, or while none is open.
    pub(super) fn dialog_key(&mut self, message: &Message) -> Option<Vec<Effect>> {
        let effects = match message {
            Message::Key(chord) if self.tree.dialog.is_some() => self.tree_dialog_key(chord),
            Message::CmdLine(CmdLineMsg::Typed(_, text)) if self.tree.dialog.is_some() => {
                self.tree_typed(TreeId::Dialog, text)
            }
            // "Select duplicate files" (RC-100-98), modal.
            Message::Key(chord) if self.tools.select_open() => self.select_duplicates_key(chord),
            Message::Key(chord) if self.archive.is_some() => self.archive_key(chord),
            Message::Key(chord) if self.checksum_has_keys() => self.checksum_key(chord, false),
            Message::KeyCaptured(chord) if self.checksum_has_keys() => {
                self.checksum_key(chord, true)
            }
            // The database dialog, then the open drive menu, has the keyboard.
            Message::Key(chord) if self.database.is_some() => self.database_key(chord),
            Message::Key(chord) if self.properties.is_some() => self.properties_key(chord),
            Message::Key(chord) if self.quick.is_some() => self.quick_connect_key(chord),
            Message::Key(chord) if self.drive_bar.menu.is_some() => self.drive_menu_key(chord),
            Message::Key(chord) if self.context_menu.is_some() => self.context_menu_key(chord),
            Message::Key(chord) if self.drawn_bar_has_key(chord) => self.drawn_bar_key(chord),
            Message::Key(chord) if self.tabs_have_keys() => self.tabs_key(chord),
            Message::Key(chord) if self.keyed_tree().is_some() => self.keyed_tree_key(chord),
            Message::CmdLine(CmdLineMsg::Typed(_, text)) if self.keyed_tree().is_some() => {
                self.keyed_tree_typed(text)
            }
            _ => return None,
        };
        Some(effects)
    }
}
