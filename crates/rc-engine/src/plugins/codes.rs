//! The stable codes a viewer plugin's failure shows as in the window (`error.<code>`): the host's
//! own, and one for everything a plugin names itself, which is its text, not rCommander's.

use rc_plugin_host::codes;

/// A failure the plugin reported in its own words.
pub const PLUGIN_FAILED: &str = "plugin_failed";

/// Every code a viewer plugin's failure shows as; each has its `error.<code>` text.
pub const ALL_CODES: &[&str] = &[
    PLUGIN_FAILED,
    codes::PLUGIN_CRASHED,
    codes::PLUGIN_TIMED_OUT,
    codes::PLUGIN_OUT_OF_FUEL,
    codes::PLUGIN_OUT_OF_MEMORY,
    codes::PLUGIN_DENIED,
    codes::PLUGIN_SUSPENDED,
    codes::PLUGIN_CANCELLED,
    codes::PLUGIN_BAD_OUTPUT,
    codes::PLUGIN_OUTPUT_TOO_LARGE,
    codes::PLUGIN_WRITE_TOO_LARGE,
    codes::PLUGIN_SINK_FAILED,
    codes::PLUGIN_SOURCE_FAILED,
    codes::PLUGIN_UNKNOWN_HANDLE,
    codes::PLUGIN_INVALID,
    codes::PLUGIN_SESSION_CLOSED,
    codes::PLUGIN_OUTPUT_NOT_OFFERED,
    codes::PLUGIN_REQUEST_TOO_LARGE,
    codes::PLUGIN_UNKNOWN_NODE,
    codes::PLUGIN_TREE_TOO_DEEP,
    codes::PLUGIN_HOST_FAILED,
];

/// The code of a failure the host answered: its own code, or [`PLUGIN_FAILED`].
#[must_use]
pub fn failure_code(code: Option<&str>) -> &'static str {
    code.and_then(|code| ALL_CODES.iter().find(|known| **known == code).copied())
        .unwrap_or(PLUGIN_FAILED)
}

#[cfg(test)]
#[path = "codes_tests.rs"]
mod tests;
