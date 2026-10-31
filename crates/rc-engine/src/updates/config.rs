//! The `update.*` settings as the self-update reads them: each through its schema row, so an
//! invalid stored value reads as the safe default (`settings::stored_setting`). The channel
//! follows the running version until a person chooses one (owner, 2026-10-20: a pre-release
//! build reads beta).

use rc_db::Db;
use rc_update::{Channel, auto_install::AutoMode};
use serde_json::Value;

use crate::settings::{
    UPDATE_AUTO_KEY, UPDATE_CHANNEL_KEY, UPDATE_CHECK_KEY, UPDATE_IDLE_KEY, UPDATE_INTERVAL_KEY,
    UPDATE_PROXY_KEY, UPDATE_RETENTION_KEY, UPDATE_SOURCE_KEY, UPDATE_WHATS_NEW_KEY,
    stored_setting,
};

/// The settings of the self-update.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UpdateConfig {
    pub check_enabled: bool,
    pub interval_hours: u32,
    pub channel: Channel,
    pub channel_chosen: bool,
    /// Empty for the official releases.
    pub source: String,
    /// Empty for the system's proxy.
    pub proxy: String,
    pub whats_new: bool,
    pub auto: AutoMode,
    pub idle_minutes: u32,
    pub retention_days: u32,
}

impl UpdateConfig {
    /// The schema's defaults, for an engine without a database: no check by itself.
    pub(crate) fn defaults(running: &str) -> Self {
        Self {
            check_enabled: false,
            interval_hours: 24,
            channel: Channel::of_version(running),
            channel_chosen: false,
            source: String::new(),
            proxy: String::new(),
            whats_new: true,
            auto: AutoMode::Off,
            idle_minutes: 30,
            retention_days: 14,
        }
    }

    /// What a package manager's installation (RC-100-52 ... RC-100-54) makes of the stored
    /// settings: the stable channel, since the tap, the bucket, winget and the repositories carry
    /// no pre-release, and no automatic install, since the manager updates it. Portable and
    /// unknown installations keep them.
    pub(crate) fn for_kind(&mut self, kind: rc_update::InstallKind) {
        if kind.update_command().is_none() {
            return;
        }
        if !kind.receives_betas() {
            self.channel = Channel::Stable;
        }
        self.auto = AutoMode::Off;
    }

    /// The settings stored in `db`.
    pub(crate) async fn read(db: &Db, running: &str) -> Self {
        let text = |value: Option<Value>| {
            value
                .and_then(|value| value.as_str().map(str::to_owned))
                .unwrap_or_default()
        };
        let number = |value: Option<Value>, fallback: u32| {
            value
                .and_then(|value| value.as_u64())
                .and_then(|value| u32::try_from(value).ok())
                .unwrap_or(fallback)
        };
        let chosen = db
            .get::<Value>(UPDATE_CHANNEL_KEY)
            .await
            .ok()
            .flatten()
            .and_then(|value| value.as_str().and_then(Channel::parse));
        Self {
            check_enabled: stored_setting(db, UPDATE_CHECK_KEY)
                .await
                .and_then(|value| value.as_bool())
                .unwrap_or(true),
            interval_hours: number(stored_setting(db, UPDATE_INTERVAL_KEY).await, 24),
            channel: chosen.unwrap_or_else(|| Channel::of_version(running)),
            channel_chosen: chosen.is_some(),
            source: text(stored_setting(db, UPDATE_SOURCE_KEY).await),
            proxy: text(stored_setting(db, UPDATE_PROXY_KEY).await),
            whats_new: stored_setting(db, UPDATE_WHATS_NEW_KEY)
                .await
                .and_then(|value| value.as_bool())
                .unwrap_or(true),
            auto: AutoMode::parse(&text(stored_setting(db, UPDATE_AUTO_KEY).await)),
            idle_minutes: number(stored_setting(db, UPDATE_IDLE_KEY).await, 30),
            retention_days: number(stored_setting(db, UPDATE_RETENTION_KEY).await, 14),
        }
    }
}
