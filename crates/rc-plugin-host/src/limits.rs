//! The sandbox budget a manifest's `[limits]` asks for, with rDownloader's defaults and ceilings.
//!
//! Copied and adapted from rDownloader's `crates/rd-plugin-host/src/lib.rs` (`PluginLimits`,
//! `DEFAULT_*`) and `src/manifest.rs` (`MAX_*`) at commit 26c8592a6. Not taken: the wait budget,
//! since rCommander's `host` has no `wait`. New: `max_concurrent`, and a timeout the plugin kind
//! sets when the manifest does not (`docs/plugins.md`, "Process model": a viewer page 5 s).

use serde::{Deserialize, Serialize};

/// Default instruction budget of one call. Rendering a page or a document costs a few million
/// instructions, so the budget is generous; the epoch deadline remains the wall-clock guard.
pub const DEFAULT_FUEL: u64 = 2_000_000_000;
/// Default linear memory of one store (64 MiB).
pub const DEFAULT_MEMORY_BYTES: u64 = 64 * 1024 * 1024;
/// Default compute time of one call for a kind that names none (15 s).
pub const DEFAULT_TIMEOUT_MILLISECONDS: u64 = 15_000;
/// Default compute time of one viewer call (5 s): a page on screen, not a batch job.
pub const DEFAULT_VIEWER_TIMEOUT_MILLISECONDS: u64 = 5_000;
/// Default compute time of one secret source call (10 s): a person waits on it while connecting
/// (RC-100-33).
pub const DEFAULT_SECRETS_TIMEOUT_MILLISECONDS: u64 = 10_000;
/// Default size of one HTTP answer a plugin may read (8 MiB), for the kinds that get `http`.
pub const DEFAULT_MAX_RESPONSE_BYTES: u64 = 8 * 1024 * 1024;
/// How many stores the host runs at once for one plugin.
pub const DEFAULT_MAX_CONCURRENT: u32 = 1;

/// Hard ceilings for what a manifest may ask for itself. The limits are the plugin author's
/// request, so a manifest asking for 8 GiB or a day of compute is refused, not granted.
pub const MAX_MEMORY_BYTES: u64 = 512 * 1024 * 1024;
/// The most fuel a manifest may declare: twenty times [`DEFAULT_FUEL`].
pub const MAX_FUEL: u64 = 20 * DEFAULT_FUEL;
/// The longest compute time of one call a manifest may declare (30 minutes).
pub const MAX_TIMEOUT_MILLISECONDS: u64 = 30 * 60 * 1000;
/// The largest HTTP answer a manifest may let its plugin read (64 MiB).
pub const MAX_RESPONSE_BYTES: u64 = 64 * 1024 * 1024;
/// The most stores of one plugin at once.
pub const MAX_CONCURRENT: u32 = 16;

/// Resource limits applied per store (memory) and per call (fuel, time).
///
/// Every field has a default, so a manifest names only the limits it changes.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginLimits {
    #[serde(default = "default_memory_bytes")]
    pub memory_bytes: u64,
    #[serde(default = "default_fuel")]
    pub fuel: u64,
    /// Absent: the plugin kind's default ([`DEFAULT_VIEWER_TIMEOUT_MILLISECONDS`] for a viewer).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_milliseconds: Option<u64>,
    #[serde(default = "default_max_response_bytes")]
    pub max_response_bytes: u64,
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent: u32,
}

const fn default_memory_bytes() -> u64 {
    DEFAULT_MEMORY_BYTES
}

const fn default_fuel() -> u64 {
    DEFAULT_FUEL
}

const fn default_max_response_bytes() -> u64 {
    DEFAULT_MAX_RESPONSE_BYTES
}

const fn default_max_concurrent() -> u32 {
    DEFAULT_MAX_CONCURRENT
}

impl Default for PluginLimits {
    fn default() -> Self {
        Self {
            memory_bytes: DEFAULT_MEMORY_BYTES,
            fuel: DEFAULT_FUEL,
            timeout_milliseconds: None,
            max_response_bytes: DEFAULT_MAX_RESPONSE_BYTES,
            max_concurrent: DEFAULT_MAX_CONCURRENT,
        }
    }
}

impl PluginLimits {
    /// The compute time of one call: the manifest's, or `kind_default`.
    #[must_use]
    pub fn timeout_or(&self, kind_default: u64) -> u64 {
        self.timeout_milliseconds.unwrap_or(kind_default)
    }

    /// Every limit non-zero and within its ceiling; the message names the first that is not.
    pub fn check(&self) -> Result<(), String> {
        let timeout = self.timeout_milliseconds.unwrap_or(1);
        for (name, value, ceiling) in [
            ("limits.memory_bytes", self.memory_bytes, MAX_MEMORY_BYTES),
            ("limits.fuel", self.fuel, MAX_FUEL),
            (
                "limits.timeout_milliseconds",
                timeout,
                MAX_TIMEOUT_MILLISECONDS,
            ),
            (
                "limits.max_response_bytes",
                self.max_response_bytes,
                MAX_RESPONSE_BYTES,
            ),
            (
                "limits.max_concurrent",
                u64::from(self.max_concurrent),
                u64::from(MAX_CONCURRENT),
            ),
        ] {
            if value == 0 {
                return Err(format!("{name} must be greater than zero"));
            }
            if value > ceiling {
                return Err(format!("{name} must not exceed {ceiling}"));
            }
        }
        Ok(())
    }
}
