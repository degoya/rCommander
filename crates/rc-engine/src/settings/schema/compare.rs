//! The external compare tool (RC-100-94), Total Commander's `CompareTool=`: set, Compare by
//! content (Ctrl+Shift+F3, the overwrite question's Compare, a row of Synchronize dirs) starts it
//! with the two files instead of the diff window, Shift keeps the diff window. Both settings are
//! protected: the program that runs and what it is handed are a person's choice, never an
//! agent's.

use super::{SettingKind, SettingPage, SettingSpec};
use crate::compare::external::DEFAULT_ARGS;

/// The external compare program; empty for the diff window.
pub const COMPARE_TOOL_KEY: &str = "compare.external_tool";
/// Its arguments: `%1` the left file, `%2` the right one.
pub const COMPARE_ARGS_KEY: &str = "compare.external_args";

pub(super) const TOOL: SettingSpec = SettingSpec {
    key: COMPARE_TOOL_KEY,
    page: SettingPage::Programs,
    kind: SettingKind::Path { default: "" },
    label: "setting.compare.external_tool",
    help: "setting.compare.external_tool.help",
    protected: true,
    restart: false,
};

pub(super) const ARGS: SettingSpec = SettingSpec {
    key: COMPARE_ARGS_KEY,
    page: SettingPage::Programs,
    kind: SettingKind::Arguments {
        default: DEFAULT_ARGS,
    },
    label: "setting.compare.external_args",
    help: "setting.compare.external_args.help",
    protected: true,
    restart: false,
};
