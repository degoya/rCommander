//! The settings: a typed schema ([`SETTINGS`]) over the key/value table of `rc-db`, and the
//! `settings.get` and `settings.set` commands.
//!
//! A stored value that does not read or does not fit its row counts as the row's default, with
//! a warning that names the key and why (never the value): a bad value never stops the app.
//! `settings.set` refuses every protected key ([`is_protected`]: `mcp.*`, `vault.*`, `plugins.*`,
//! `plugin.*`) for every caller, so an agent cannot loosen its own permissions; the window
//! changes those through [`Engine::set_protected_setting`], which is no command and so out of
//! MCP's reach. The keymap's deviations are such a protected row; `keymap.rs` reads the keymap
//! in force from it (`keymap.list`) and stores it from the window (`keymap.set`, RC-100-21).

mod keymap;
mod schema;

use std::collections::BTreeMap;

use rc_db::{Db, DbError};
use rc_term::ShellProfile;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use schema::AGENT_MAX_KEYS_KEY;
pub use schema::LANGUAGE_KEY;
pub use schema::archive::DECODER_MEMORY_KEY;
pub use schema::backup::BACKUP_KEEP_KEY;
pub use schema::checksum::CHECKSUM_ENTER_KEY;
pub use schema::drives::{FOLLOW_ON_OWN_DRIVE_KEY, FOLLOW_OTHER_PANEL_KEY};
pub use schema::ftp::FTP_RESOLVE_LINKS_KEY;
pub use schema::layout::{
    DATE_WIDTH_KEY, EXT_WIDTH_KEY, MENU_BAR_KEY, OWNER_COLUMN_KEY, OWNER_WIDTH_KEY,
    PACKED_WIDTH_KEY, PANEL_SPLIT_KEY, RIGHTS_WIDTH_KEY, SIZE_WIDTH_KEY,
};
pub use schema::login_scripts::WARN_SECRET_PROMPT_KEY;
pub use schema::sort::{SORT_NAMES_KEY, SORT_NUMBERS_KEY};
pub use schema::terminal::{
    COPY_CTRL_C_KEY, COPY_ON_SELECT_KEY, COPY_RIGHT_CLICK_KEY, END_PROGRAMS_KEY, OSC52_COPY_KEY,
    PASTE_CTRL_V_KEY, PASTE_IMAGE_KEY, SHIFT_SELECTS_KEY,
};
pub use schema::viewer::{
    DOS_EXTENSIONS_DEFAULT, DOS_EXTENSIONS_KEY, DOS_TIGHT_LINES_KEY, MARKDOWN_PREVIEW_KEY,
    PICTURE_MEGAPIXELS_KEY,
};
pub use schema::viewer::{
    TEXT_ZOOM_KEEP_KEY, TEXT_ZOOM_KEY, VIEWER_MAX_FONT_SIZE, VIEWER_MIN_FONT_SIZE,
};
pub use schema::{ALTERNATE_ROWS_KEY, EDITOR_KEY, FKEY_BAR_KEY, MKDIR_PREFILL_KEY, VIEWER_KEY};
pub use schema::{
    ARCHIVE_ENTER_KEY, Choice, FILE_ICONS_KEY, FOLLOW_PANEL_KEY, INTEGRATION_KEY, LEFT_START_KEY,
    MCP_EXEC_SESSION_MINUTES_KEY, MCP_HEADLESS_CODE_KEY, PANEL_FOLLOWS_KEY,
    QUICK_SEARCH_ANYWHERE_KEY, QUICK_SEARCH_CASE_KEY, QUICK_SEARCH_FILTERS_KEY,
    QUICK_SEARCH_KEYS_KEY, RIGHT_START_KEY, SELECT_FOLDERS_KEY, SETTINGS, SHELL_KEY,
    SPACE_MOVES_DOWN_KEY, SettingKind, SettingPage, SettingSpec, THEME_KEY, is_protected, setting,
};
pub use schema::{
    ASSOCIATIONS_KEY, CRASH_LIMIT_KEY, DEVELOPMENT_MODE_KEY, ENABLED_KEY as PLUGINS_ENABLED_KEY,
    OUTPUT_LIMIT_KEY, parse_associations,
};
pub use schema::{CLICK_CLEARS_MARKS_KEY, TAB_DRAG_KEY, VIEWER_TOKEN_COLORS_KEY};
pub use schema::{
    COLOUR_KEYS, COLOUR_RULES_KEY, COLOUR_TOKENS, ColourRule, MAX_COLOUR_RULES, colour_key,
    colour_value, parse_colour_rules, write_colour_rules,
};
pub use schema::{
    CONFIRM_QUIT_KEY, CONNECTIONS_FOCUS_LIST_KEY, CONTEXT_MENU_KEYS_KEY, DIALOG_WINDOWS_KEY,
    F10_KEY, GPU_LOSS_NOTICE_KEY, KEYMAP_KEY, ONE_JOB_PER_ITEM_KEY, PERMANENT_WITHOUT_TRASH_KEY,
    QUEUE_WINDOW_OWN_KEY, STRIP_ROWS_KEY, UNDERLINE_ACCESS_KEYS_KEY,
};
pub use schema::{
    DEFAULT_FONT_SIZE, LIST_FONT_KEY, LIST_FONT_SIZE_KEY, MARK_HIDDEN_KEY, MAX_FONT_SIZE,
    MIN_FONT_SIZE, NAME_CUT_KEY, NAME_TIP_DELAY_KEY, TERMINAL_FONT_KEY, TERMINAL_FONT_SIZE_KEY,
    VIEWER_FONT_KEY, VIEWER_FONT_SIZE_KEY,
};
pub use schema::{DRIVE_LINE_BUTTONS_KEY, DRIVE_LINE_VOLUME_KEY};
pub use schema::{
    EXPORT_CONNECTIONS_KEY, EXPORT_ENCRYPTED_KEY, EXPORT_HOTLIST_KEY, EXPORT_SETTINGS_KEY,
    EXPORT_TRUST_KEY,
};
pub use schema::{HIDE_SINGLE_LANE_KEY, SHOW_AT_ONCE_KEY, TOOLS_AT_ONCE_KEY, TOTAL_SPEED_KEY};
pub use schema::{
    TRANSFER_BUFFER_KEY, TRANSFER_PER_CONNECTION_KEY, TRANSFER_RETRY_COUNT_KEY,
    TRANSFER_RETRY_DELAY_KEY, TRANSFER_TOTAL_KEY,
};
pub use schema::{
    UPDATE_AUTO_KEY, UPDATE_CHANNEL_KEY, UPDATE_CHECK_KEY, UPDATE_IDLE_KEY, UPDATE_INTERVAL_KEY,
    UPDATE_PROXY_KEY, UPDATE_RETENTION_KEY, UPDATE_SOURCE_KEY, UPDATE_WHATS_NEW_KEY,
};

use crate::{Engine, EngineError, EngineEvent, Outcome};

/// One setting as `settings.get` and `settings.set` report it: its value and its row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingValue {
    /// The key, such as `appearance.theme`; written `setting`, since the MCP mask hides the
    /// value of every field named `key` (an API key's usual name).
    #[serde(rename = "setting")]
    pub key: String,
    /// The value in force: the stored one, or the default.
    pub value: Value,
    pub default: Value,
    /// `bool`, `int`, `choice`, `id`, `path` or `font`.
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<i64>,
    /// The values a `choice` takes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<String>,
    /// Only a person in the rCommander window changes it.
    pub protected: bool,
    /// It takes effect at the next start.
    pub restart: bool,
}

impl SettingValue {
    fn of(spec: &SettingSpec, value: Value) -> Self {
        let (min, max) = match spec.kind {
            SettingKind::Int { min, max, .. } => (Some(min), Some(max)),
            _ => (None, None),
        };
        let choices = match spec.kind {
            SettingKind::Choice { choices, .. } => {
                choices.iter().map(|c| c.value.to_owned()).collect()
            }
            _ => Vec::new(),
        };
        Self {
            key: spec.key.to_owned(),
            value,
            default: spec.default_value(),
            kind: spec.type_name().to_owned(),
            min,
            max,
            choices,
            protected: spec.protected,
            restart: spec.restart,
        }
    }
}

/// The value of the schema row `key` in `db`: the stored one when it reads and fits, the
/// default otherwise. `None` for a key the schema does not have.
pub async fn stored_setting(db: &Db, key: &str) -> Option<Value> {
    let spec = setting(key)?;
    Some(effective(spec, db.get::<Value>(key).await))
}

/// The switch `key` (a `Bool` row) in `db`: stored, or its default when nothing valid is
/// stored; `false` for a key that is no `Bool` row of the schema.
pub async fn stored_bool(db: &Db, key: &str) -> bool {
    stored_setting(db, key)
        .await
        .as_ref()
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// The stored JSON text of `key` as a value, as [`Db::get`] reads it.
fn parsed(key: &str, text: Option<&String>) -> Result<Option<Value>, DbError> {
    text.map(|text| {
        serde_json::from_str(text).map_err(|source| DbError::Json {
            key: key.to_owned(),
            source,
        })
    })
    .transpose()
}

/// What a stored value comes to: itself when valid, the default with a warning otherwise.
fn effective(spec: &SettingSpec, stored: Result<Option<Value>, DbError>) -> Value {
    match stored {
        Ok(None) => spec.default_value(),
        Ok(Some(value)) => match spec.check(&value) {
            Ok(value) => value,
            Err(reason) => {
                tracing::warn!(
                    key = spec.key,
                    %reason,
                    "a stored setting is not valid; its default applies"
                );
                spec.default_value()
            }
        },
        Err(error) => {
            tracing::warn!(
                key = spec.key,
                %error,
                "a stored setting cannot be read; its default applies"
            );
            spec.default_value()
        }
    }
}

/// The shell named `wanted` when this machine has it, else the first one found.
fn pick_shell(shells: Vec<ShellProfile>, wanted: Option<&str>) -> Option<ShellProfile> {
    let wanted = wanted.filter(|id| !id.is_empty());
    let at = wanted
        .and_then(|id| shells.iter().position(|shell| shell.id == id))
        .unwrap_or(0);
    shells.into_iter().nth(at)
}

impl Engine {
    /// `settings.get`: every setting, or the one named `key`.
    pub(crate) async fn get_settings(&self, key: Option<&str>) -> Result<Outcome, EngineError> {
        let db = self.db()?;
        let specs: Vec<&SettingSpec> = match key {
            Some(key) => vec![setting(key).ok_or_else(|| EngineError::SettingUnknown {
                key: key.to_owned(),
            })?],
            None => SETTINGS.iter().collect(),
        };
        // One query for every row asked (`Db::get_json_many`, RC-080-50); a failing read leaves
        // each its default, as a row that cannot be read does.
        let keys: Vec<&str> = specs.iter().map(|spec| spec.key).collect();
        let stored = db.get_json_many(&keys).await.unwrap_or_else(|error| {
            tracing::warn!(%error, "the stored settings cannot be read; their defaults apply");
            BTreeMap::new()
        });
        let settings = specs
            .into_iter()
            .map(|spec| {
                let value = effective(spec, parsed(spec.key, stored.get(spec.key)));
                SettingValue::of(spec, value)
            })
            .collect();
        Ok(Outcome::Settings { settings })
    }

    /// `settings.set`: stores a valid value of an unprotected setting and announces it.
    pub(crate) async fn set_setting(
        &self,
        key: &str,
        value: &Value,
    ) -> Result<Outcome, EngineError> {
        if is_protected(key) {
            return Err(EngineError::SettingProtected {
                key: key.to_owned(),
            });
        }
        let stored = self.store_setting(key, value).await?;
        Ok(Outcome::Settings {
            settings: vec![stored],
        })
    }

    /// Stores a valid value of any setting, protected ones included: the window's way to the MCP
    /// switch, its port and when the vault locks. Not a command, so MCP cannot reach it.
    ///
    /// # Errors
    ///
    /// The key is unknown, the value invalid, or the database fails.
    pub async fn set_protected_setting(
        &self,
        key: &str,
        value: &Value,
    ) -> Result<SettingValue, EngineError> {
        self.store_setting(key, value).await
    }

    async fn store_setting(&self, key: &str, value: &Value) -> Result<SettingValue, EngineError> {
        let spec = setting(key).ok_or_else(|| EngineError::SettingUnknown {
            key: key.to_owned(),
        })?;
        let value = spec
            .check(value)
            .map_err(|reason| EngineError::SettingInvalid {
                key: key.to_owned(),
                reason,
            })?;
        let db = self.db()?;
        let number = value
            .as_u64()
            .map(|number| u32::try_from(number).unwrap_or(u32::MAX));
        // The running vault takes a new idle lock or longest time unlocked at once; it stores
        // them too.
        let applied = match (self.vault(), number) {
            (Some(vault), Some(minutes)) if spec.key == rc_vault::IDLE_LOCK_KEY => {
                Some(vault.set_idle_lock_minutes(minutes).await)
            }
            (Some(vault), Some(hours)) if spec.key == rc_vault::MAX_UNLOCKED_KEY => {
                Some(vault.set_max_unlocked_hours(hours).await)
            }
            _ => None,
        };
        match applied {
            // The setting's write failing is the database's; any other refusal keeps its code.
            Some(result) => result.map_err(|error| match error {
                rc_vault::VaultError::Db(error) => EngineError::Database(error),
                other => EngineError::from(other),
            })?,
            None => db.set(spec.key, &value).await?,
        }
        if [SORT_NAMES_KEY, SORT_NUMBERS_KEY].contains(&spec.key) {
            self.name_order_changed().await;
        }
        // Archives open from now on under the new decoder memory (RC-080-35).
        if spec.key == DECODER_MEMORY_KEY {
            self.read_archive_limits().await;
        }
        // The switch, development mode and the crash limit reach the installed plugins at once.
        if spec.key.starts_with("plugins.") {
            self.plugin_settings_changed().await;
        }
        self.publish(EngineEvent::SettingChanged {
            key: spec.key.to_owned(),
        });
        Ok(SettingValue::of(spec, value))
    }

    /// The shell a terminal opened without one runs: the setting [`SHELL_KEY`] when this machine
    /// has that shell, the first one found otherwise.
    pub(crate) async fn default_shell(&self, shells: Vec<ShellProfile>) -> Option<ShellProfile> {
        let wanted = match self.db() {
            Ok(db) => stored_setting(db, SHELL_KEY).await,
            Err(_) => None,
        };
        pick_shell(shells, wanted.as_ref().and_then(Value::as_str))
    }

    /// Hands the `transfers.*` settings to the operations queue (RC-060-05, the retries of
    /// RC-060-07). Read before each copy or move is submitted, so a change counts from the next
    /// one; without a database the queue keeps its defaults.
    pub(crate) async fn apply_transfer_settings(&self) {
        let Ok(db) = self.db() else {
            return;
        };
        let mut numbers = [0_usize; 5];
        for (number, key) in numbers.iter_mut().zip([
            TRANSFER_BUFFER_KEY,
            TRANSFER_PER_CONNECTION_KEY,
            TRANSFER_TOTAL_KEY,
            TRANSFER_RETRY_COUNT_KEY,
            TRANSFER_RETRY_DELAY_KEY,
        ]) {
            *number = stored_setting(db, key)
                .await
                .as_ref()
                .and_then(Value::as_u64)
                .and_then(|value| usize::try_from(value).ok())
                .unwrap_or(1);
        }
        let [buffer_kib, per_connection, total, attempts, delay_secs] = numbers;
        self.inner
            .ops
            .set_transfer_limits(buffer_kib.saturating_mul(1024), per_connection, total);
        self.inner.ops.set_retry(rc_ops::RetryPolicy {
            attempts: u32::try_from(attempts).unwrap_or(u32::MAX),
            delay: std::time::Duration::from_secs(u64::try_from(delay_secs).unwrap_or(1)),
        });
    }

    /// The value of the switch `key` (a `Bool` row): stored, or its default when nothing is
    /// stored or no database is attached.
    pub(crate) async fn setting_on(&self, key: &str) -> bool {
        match self.db() {
            Ok(db) => stored_bool(db, key).await,
            Err(_) => setting(key)
                .map(SettingSpec::default_value)
                .as_ref()
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }
    }
}

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "settings_get_tests.rs"]
mod get_tests;
