//! The settings dialog's pages ([`Page`]), in its order.

/// The dialog's pages, in its order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Appearance,
    Folders,
    /// What F10 does (RC-080-17).
    Keyboard,
    Terminal,
    Vault,
    /// The trusted SSH host keys and FTPS certificates (RC-050-07).
    HostKeys,
    Retention,
    Mcp,
    /// The external viewer and editor (RC-010-07).
    Programs,
    /// Copies and moves: buffer and how many run at once (RC-060-05).
    Transfers,
    /// The installed plugins and their switches (RC-100-09).
    Plugins,
    /// Imports from other programs: Total Commander's (RC-050-10).
    Import,
    /// The self-update: its state, the offer and its notes, the switches (RC-100-05).
    Updates,
    /// The build and the licences (RC-010-19).
    About,
}

impl Page {
    pub const ALL: [Self; 14] = [
        Self::Appearance,
        Self::Folders,
        Self::Keyboard,
        Self::Terminal,
        Self::Vault,
        Self::HostKeys,
        Self::Retention,
        Self::Mcp,
        Self::Programs,
        Self::Transfers,
        Self::Plugins,
        Self::Import,
        Self::Updates,
        Self::About,
    ];
}
