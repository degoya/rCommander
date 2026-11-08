//! The internal associations' settings (RC-100-91), on the Programs page beside the viewer and
//! the editor: the rules (`files.associations`, JSON text checked by
//! `rc_engine::associations::parse_rules`, the window "Internal associations..." edits them),
//! whether Enter and the context menu use them, and whether the menu offers the commands of every
//! matching rule (Total Commander's "Load extra commands from all matching types"). All three are
//! protected like `files.viewer`: which program runs is a person's choice, never an agent's.

use super::{SettingKind, SettingPage, SettingSpec};

/// The rules, in the order they are tried; empty for none.
pub const FILE_ASSOCIATIONS_KEY: &str = "files.associations";
/// Whether Enter and the context menu use the rules; off, the system's association alone counts.
pub const ASSOCIATIONS_ENABLED_KEY: &str = "files.associations_enabled";
/// Whether the context menu offers the named commands of every rule whose mask matches, not
/// only the first's.
pub const ASSOCIATIONS_ALL_MATCHES_KEY: &str = "files.associations_all_matches";

pub(super) const FILE_ASSOCIATIONS: SettingSpec = SettingSpec {
    key: FILE_ASSOCIATIONS_KEY,
    page: SettingPage::Programs,
    kind: SettingKind::FileAssociations { default: "" },
    label: "setting.files.associations",
    help: "setting.files.associations.help",
    protected: true,
    restart: false,
};

pub(super) const ENABLED: SettingSpec = SettingSpec {
    key: ASSOCIATIONS_ENABLED_KEY,
    page: SettingPage::Programs,
    kind: SettingKind::Bool { default: true },
    label: "setting.files.associations_enabled",
    help: "setting.files.associations_enabled.help",
    protected: true,
    restart: false,
};

pub(super) const ALL_MATCHES: SettingSpec = SettingSpec {
    key: ASSOCIATIONS_ALL_MATCHES_KEY,
    page: SettingPage::Programs,
    kind: SettingKind::Bool { default: false },
    label: "setting.files.associations_all_matches",
    help: "setting.files.associations_all_matches.help",
    protected: true,
    restart: false,
};
