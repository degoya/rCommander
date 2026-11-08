//! The vault's rows: when it locks by itself, whether the Windows credential manager is a secret
//! source, whether connections take their secrets from shared credentials, and whether a PAM
//! system's checkout reason is kept. All are protected: no agent may keep the vault open longer
//! (RC-080-11), open another store of secrets (RC-100-37), send a credential's secrets elsewhere
//! (RC-050-12) or keep a reason the person gave for one checkout (RC-100-38).

use super::{SettingKind, SettingPage, SettingSpec};
use crate::{
    SHARED_CREDENTIALS_KEY,
    secret_sources::{CHECKOUT_KEEP_REASON_KEY, WINCRED_SOURCE_KEY},
    session_lock::LOCK_ON_SESSION_LOCK_KEY,
};

/// Minutes without the person's use; `0` never.
pub(super) const IDLE_LOCK: SettingSpec = SettingSpec {
    key: rc_vault::IDLE_LOCK_KEY,
    page: SettingPage::Vault,
    kind: SettingKind::Int {
        min: 0,
        max: 1440,
        default: rc_vault::DEFAULT_IDLE_LOCK_MINUTES as i64,
    },
    label: "setting.vault.idle_lock_minutes",
    help: "setting.vault.idle_lock_minutes.help",
    protected: true,
    restart: false,
};

/// Hours from the unlock, use or no use; `0` sets no limit. At most a week.
pub(super) const MAX_UNLOCKED: SettingSpec = SettingSpec {
    key: rc_vault::MAX_UNLOCKED_KEY,
    page: SettingPage::Vault,
    kind: SettingKind::Int {
        min: 0,
        max: 168,
        default: rc_vault::DEFAULT_MAX_UNLOCKED_HOURS as i64,
    },
    label: "setting.vault.max_unlocked_hours",
    help: "setting.vault.max_unlocked_hours.help",
    protected: true,
    restart: false,
};

/// Lock when the session locks or the machine suspends.
pub(super) const LOCK_ON_SESSION_LOCK: SettingSpec = SettingSpec {
    key: LOCK_ON_SESSION_LOCK_KEY,
    page: SettingPage::Vault,
    kind: SettingKind::Bool { default: true },
    label: "setting.vault.lock_on_session_lock",
    help: "setting.vault.lock_on_session_lock.help",
    protected: true,
    restart: false,
};

/// Whether the Windows credential manager is offered as a secret source; it has no effect on
/// other platforms.
pub(super) const WINCRED_SOURCE: SettingSpec = SettingSpec {
    key: WINCRED_SOURCE_KEY,
    page: SettingPage::Vault,
    kind: SettingKind::Bool { default: true },
    label: "setting.vault.wincred_source",
    help: "setting.vault.wincred_source.help",
    protected: true,
    restart: false,
};

/// Whether a connection that points at a shared credential takes its user and secrets from it;
/// off, every connection uses its own and the manager offers no credentials (RC-050-12).
pub(super) const SHARED_CREDENTIALS: SettingSpec = SettingSpec {
    key: SHARED_CREDENTIALS_KEY,
    page: SettingPage::Vault,
    kind: SettingKind::Bool { default: true },
    label: "setting.vault.shared_credentials",
    help: "setting.vault.shared_credentials.help",
    protected: true,
    restart: false,
};

/// Whether the reason given for a PAM system's checkout is kept for the connection's later
/// checkouts until rCommander ends; off, one checkout uses it up (RC-100-38).
pub(super) const CHECKOUT_KEEP_REASON: SettingSpec = SettingSpec {
    key: CHECKOUT_KEEP_REASON_KEY,
    page: SettingPage::Vault,
    kind: SettingKind::Bool { default: true },
    label: "setting.vault.checkout_keep_reason",
    help: "setting.vault.checkout_keep_reason.help",
    protected: true,
    restart: false,
};
