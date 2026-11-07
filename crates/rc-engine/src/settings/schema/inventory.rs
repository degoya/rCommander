//! Server inventories (RC-100-39): how often a source synced before is synced again. 60 minutes
//! by default; 0 syncs only when the person asks. On the Transfers page, beside the other
//! settings of remote connections (the Plugins page holds `plugins.*` alone); not protected - it
//! only decides how often an address the person confirmed is asked.

use super::{SettingKind, SettingPage, SettingSpec};

/// The minutes between two syncs of an inventory source; 0 for none.
pub const INVENTORY_REFRESH_MINUTES_KEY: &str = "inventory.refresh_minutes";

pub(super) const REFRESH_MINUTES: SettingSpec = SettingSpec {
    key: INVENTORY_REFRESH_MINUTES_KEY,
    page: SettingPage::Transfers,
    kind: SettingKind::Int {
        min: 0,
        max: 10_080,
        default: 60,
    },
    label: "setting.inventory.refresh_minutes",
    help: "setting.inventory.refresh_minutes.help",
    protected: false,
    restart: false,
};
