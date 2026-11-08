//! The folder trees (RC-100-88): the separate tree beside the panels and its width on the
//! Appearance page, as Total Commander's `SeparateTree` and `SeparateTreeWidth`, and when a panel
//! follows a tree's cursor on the Folders page, as its "Tree: Press ENTER to open Dir in other
//! window". None is protected: `panel.cycle_separate_tree` writes the first.

use super::{Choice, SettingKind, SettingPage, SettingSpec};

/// Whether a tree stands beside the panels: `off`, `one` for both, `two`, one beside each.
pub const SEPARATE_TREE_KEY: &str = "panels.separate_tree";
/// The separate tree's width, in pixels at the default list font size; one for both trees.
pub const TREE_WIDTH_KEY: &str = "panels.tree_width";
/// Whether a panel shows the folder under a tree's cursor only after Enter, not after a pause.
pub const TREE_OPEN_ON_ENTER_KEY: &str = "tree.open_on_enter";

const SEPARATE_TREES: &[Choice] = &[
    Choice {
        value: "off",
        label: "setting.panels.separate_tree.off",
    },
    Choice {
        value: "one",
        label: "setting.panels.separate_tree.one",
    },
    Choice {
        value: "two",
        label: "setting.panels.separate_tree.two",
    },
];

/// Off by default, as in Total Commander.
pub(super) const SEPARATE_TREE: SettingSpec = SettingSpec {
    key: SEPARATE_TREE_KEY,
    page: SettingPage::Appearance,
    kind: SettingKind::Choice {
        choices: SEPARATE_TREES,
        default: "off",
    },
    label: "setting.panels.separate_tree",
    help: "setting.panels.separate_tree.help",
    protected: false,
    restart: false,
};

/// Dragged at the tree's edge as the divider is (RC-010-20).
pub(super) const TREE_WIDTH: SettingSpec = SettingSpec {
    key: TREE_WIDTH_KEY,
    page: SettingPage::Appearance,
    kind: SettingKind::Int {
        min: 120,
        max: 800,
        default: 220,
    },
    label: "setting.panels.tree_width",
    help: "setting.panels.tree_width.help",
    protected: false,
    restart: false,
};

/// Off by default, as Total Commander's option: the panel follows the cursor after a pause.
pub(super) const OPEN_ON_ENTER: SettingSpec = SettingSpec {
    key: TREE_OPEN_ON_ENTER_KEY,
    page: SettingPage::Folders,
    kind: SettingKind::Bool { default: false },
    label: "setting.tree.open_on_enter",
    help: "setting.tree.open_on_enter.help",
    protected: false,
    restart: false,
};
