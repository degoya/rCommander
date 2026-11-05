//! The shapes of the settings schema: pages, choices, kinds and one setting's spec.

/// The dialog page a setting is shown on, in the dialog's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SettingPage {
    Appearance,
    Folders,
    Terminal,
    Vault,
    Mcp,
    /// The external viewer and editor (RC-010-07).
    Programs,
    /// Copies and moves: buffer and how many run at once (RC-060-05), retries of a lost
    /// connection (RC-060-07).
    Transfers,
    /// What keys do beyond the keymap: F10 (RC-080-17); and the keymap's deviations (RC-100-21).
    Keyboard,
    /// What the export of settings and connections starts with (RC-080-19).
    Exchange,
    /// Plugins: the master switch, suspension, output limits, associations (RC-100-09).
    Plugins,
    /// The self-update: checks, channel, source, proxy, automation (RC-100-05, RC-100-07).
    Updates,
}

/// One choice of a [`SettingKind::Choice`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Choice {
    /// The stored value.
    pub value: &'static str,
    /// The i18n key of its label.
    pub label: &'static str,
}

/// A setting's type, with its default and bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingKind {
    Bool {
        default: bool,
    },
    /// A whole number within `min..=max`.
    Int {
        min: i64,
        max: i64,
        default: i64,
    },
    /// One of a fixed list.
    Choice {
        choices: &'static [Choice],
        default: &'static str,
    },
    /// A short identifier: letters, digits, `.`, `_` and `-`; empty for none.
    Id {
        default: &'static str,
    },
    /// An absolute path, or empty for none.
    Path {
        default: &'static str,
    },
    /// A font family's name, or empty for the area's default (RC-010-17).
    Font {
        default: &'static str,
    },
    /// File extensions without dots, separated by spaces; empty for none (RC-020-11).
    Extensions {
        default: &'static str,
    },
    /// Extensions given to a plugin: `csv=<plugin id> tsv=<plugin id>`; empty for none
    /// (RC-100-09).
    Associations {
        default: &'static str,
    },
    /// The deviations from the standard keymap: `Ctrl+K=copy F5=`; empty for none (RC-100-21,
    /// `rc_engine::Overrides`).
    Keymap {
        default: &'static str,
    },
    /// A colour `#rrggbb`, or empty for the theme's own (RC-100-43).
    Colour {
        default: &'static str,
    },
    /// The colours by file type: one rule per line, `mask<TAB>#light[<TAB>#dark]`; empty for
    /// none (RC-100-43, `schema/colours.rs`).
    ColourRules {
        default: &'static str,
    },
    /// An `https://` folder serving the update manifests; empty for the official releases
    /// (RC-100-05).
    Source {
        default: &'static str,
    },
    /// A proxy: empty for the system's, `direct`, or `http(s)://host:port` without a login.
    Proxy {
        default: &'static str,
    },
    /// The id of a colour theme, built in or own (RC-100-50, `schema/themes.rs`).
    Theme {
        default: &'static str,
    },
    /// The themes of the WSL distributions: `Distro<TAB>light<TAB>dark` per line (RC-100-56).
    WslThemes {
        default: &'static str,
    },
}

/// One setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettingSpec {
    /// The key in the `settings` table.
    pub key: &'static str,
    pub page: SettingPage,
    pub kind: SettingKind,
    /// The i18n key of its label.
    pub label: &'static str,
    /// The i18n key of its help line.
    pub help: &'static str,
    /// Only a person in the window changes it: `settings.set` refuses it for every caller.
    pub protected: bool,
    /// It takes effect at the next start.
    pub restart: bool,
}
