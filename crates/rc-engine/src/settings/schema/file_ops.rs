//! Two rows of the file operations, both Total Commander's behaviour made a choice.
//!
//! Links to folders (RC-080-63, TC's `CopyLinks` and `IgnoreLinks` in `wincmd.ini`): a copy or a
//! move to another file system copies the folder a link leads to as a plain folder (`contents`,
//! the default, as TC's), makes the link anew at the target with its target unchanged (`link`),
//! or passes it over (`skip`). A move within one file system renames the link whatever is
//! chosen. It holds for the jobs started after the change.
//!
//! The roll-back of a failed multi-rename (RC-080-64): on, the default, a run that stops at a
//! failing step while entries are under rCommander's temporary names (a swap's or a ring's
//! middle) reverses its steps at once, so every name is as before; off, the run stops as Total
//! Commander's does, and the message names each entry left under a temporary name, for Undo.
//! Neither is protected: either way no file is lost or overwritten.
//!
//! Shift+F6 on several selected entries (RC-110-04): on, the default, it opens the rename dialog
//! with the mask `*.*`, as Total Commander does; off, it always renames the entry under the
//! cursor in the list.

use super::{Choice, SettingKind, SettingPage, SettingSpec};

/// What a copy or a move does with a link to a folder: `contents`, `link` or `skip`.
pub const COPY_LINKS_KEY: &str = "ops.copy_links";
/// Whether a failed multi-rename with entries under temporary names is reversed at once.
pub const RENAME_UNDO_ON_FAILURE_KEY: &str = "rename.undo_on_failure";
/// Whether Shift+F6 on two or more selected entries opens the rename dialog with a mask.
pub const RENAME_SELECTION_DIALOG_KEY: &str = "rename.selection_dialog";

const COPY_LINKS_CHOICES: &[Choice] = &[
    Choice {
        value: "contents",
        label: "setting.ops.copy_links.contents",
    },
    Choice {
        value: "link",
        label: "setting.ops.copy_links.link",
    },
    Choice {
        value: "skip",
        label: "setting.ops.copy_links.skip",
    },
];

pub(super) const COPY_LINKS: SettingSpec = SettingSpec {
    key: COPY_LINKS_KEY,
    page: SettingPage::Transfers,
    kind: SettingKind::Choice {
        choices: COPY_LINKS_CHOICES,
        default: "contents",
    },
    label: "setting.ops.copy_links",
    help: "setting.ops.copy_links.help",
    protected: false,
    restart: false,
};

pub(super) const RENAME_UNDO_ON_FAILURE: SettingSpec = SettingSpec {
    key: RENAME_UNDO_ON_FAILURE_KEY,
    page: SettingPage::Transfers,
    kind: SettingKind::Bool { default: true },
    label: "setting.rename.undo_on_failure",
    help: "setting.rename.undo_on_failure.help",
    protected: false,
    restart: false,
};

pub(super) const RENAME_SELECTION_DIALOG: SettingSpec = SettingSpec {
    key: RENAME_SELECTION_DIALOG_KEY,
    page: SettingPage::Transfers,
    kind: SettingKind::Bool { default: true },
    label: "setting.rename.selection_dialog",
    help: "setting.rename.selection_dialog.help",
    protected: false,
    restart: false,
};
