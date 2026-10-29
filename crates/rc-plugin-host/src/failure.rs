//! The failures the host reports for a plugin, with their stable codes, and what a stopped call
//! becomes.
//!
//! Copied and adapted from rDownloader's `crates/rd-plugin-host/src/component/convert.rs`
//! (`component_failure`, `describe_component_error`) at commit 26c8592a6, with the codes of
//! `docs/plugins.md` ("Process model": `plugin_crashed`, `plugin_timed_out`, `plugin_out_of_fuel`,
//! `plugin_out_of_memory`, `plugin_denied`, plus `plugin_suspended`).

use rc_plugin_types::{Failure, FailureKind};

/// The host's failure codes; the window translates them.
pub mod codes {
    /// A trap: `unreachable`, a panic, an out-of-bounds access.
    pub const PLUGIN_CRASHED: &str = "plugin_crashed";
    /// The epoch deadline (or the caller's wall clock) ended the call.
    pub const PLUGIN_TIMED_OUT: &str = "plugin_timed_out";
    /// The call spent its instruction budget.
    pub const PLUGIN_OUT_OF_FUEL: &str = "plugin_out_of_fuel";
    /// The store asked for more memory than the manifest allows.
    pub const PLUGIN_OUT_OF_MEMORY: &str = "plugin_out_of_memory";
    /// The component imports an interface its manifest does not grant.
    pub const PLUGIN_DENIED: &str = "plugin_denied";
    /// Too many failures in a short time; the person resumes it.
    pub const PLUGIN_SUSPENDED: &str = "plugin_suspended";
    /// The caller stopped the call (Quick View moved on).
    pub const PLUGIN_CANCELLED: &str = "plugin_cancelled";
    /// The plugin answered something the host refuses (a duplicate output, a broken raster).
    pub const PLUGIN_BAD_OUTPUT: &str = "plugin_bad_output";
    /// The plugin wrote more than the output limit allows.
    pub const PLUGIN_OUTPUT_TOO_LARGE: &str = "plugin_output_too_large";
    /// One write larger than a window (1 MiB).
    pub const PLUGIN_WRITE_TOO_LARGE: &str = "plugin_write_too_large";
    /// The output's target could not be written.
    pub const PLUGIN_SINK_FAILED: &str = "plugin_sink_failed";
    /// The source could not be read.
    pub const PLUGIN_SOURCE_FAILED: &str = "plugin_source_failed";
    /// A handle this call was not given.
    pub const PLUGIN_UNKNOWN_HANDLE: &str = "plugin_unknown_handle";
    /// Not a component of this kind's world, or not one at all.
    pub const PLUGIN_INVALID: &str = "plugin_invalid";
    /// An earlier call of this session stopped; the document has to be opened again.
    pub const PLUGIN_SESSION_CLOSED: &str = "plugin_session_closed";
    /// The caller asked for an output the document does not offer.
    pub const PLUGIN_OUTPUT_NOT_OFFERED: &str = "plugin_output_not_offered";
    /// The caller asked for more than one call may carry (lines, page size, needle).
    pub const PLUGIN_REQUEST_TOO_LARGE: &str = "plugin_request_too_large";
    /// A tree node the plugin never handed out.
    pub const PLUGIN_UNKNOWN_NODE: &str = "plugin_unknown_node";
    /// The tree went deeper than the host follows.
    pub const PLUGIN_TREE_TOO_DEEP: &str = "plugin_tree_too_deep";
    /// The host itself could not set the call up.
    pub const PLUGIN_HOST_FAILED: &str = "plugin_host_failed";
    /// An `http` request to a target the manifest does not grant (RC-100-33).
    pub const PLUGIN_HTTP_DENIED: &str = "plugin_http_denied";
    /// The granted target did not answer: nothing listens, the connection broke, it timed out.
    pub const PLUGIN_HTTP_FAILED: &str = "plugin_http_failed";
    /// An HTTP answer larger than the manifest's `max_response_bytes`.
    pub const PLUGIN_HTTP_TOO_LARGE: &str = "plugin_http_too_large";
}

/// Longest trap reason kept as a parameter: one line, never a backtrace.
const MAX_REASON_CHARS: usize = 200;

/// A failure with a host code.
pub(crate) fn coded(kind: FailureKind, code: &str) -> Failure {
    Failure::coded(kind, code)
}

/// The host could not set a call up; `detail` goes to the log, not to the caller.
pub(crate) fn host_failed(detail: impl std::fmt::Display) -> Failure {
    tracing::warn!(%detail, "a plugin call could not be set up");
    coded(FailureKind::Other, codes::PLUGIN_HOST_FAILED)
}

/// The plugin's answer broke a rule; `what` names which.
pub(crate) fn bad_output(what: &str) -> Failure {
    coded(FailureKind::BadData, codes::PLUGIN_BAD_OUTPUT).with_param("what", what)
}

/// What a call that did not return becomes: `error` is Wasmtime's, `None` the caller's wall
/// clock. A cancelled call is the caller's doing, whatever stopped it.
pub(crate) fn stopped(error: Option<&wasmtime::Error>, cancelled: bool) -> Failure {
    if cancelled {
        return coded(FailureKind::Cancelled, codes::PLUGIN_CANCELLED);
    }
    let Some(error) = error else {
        return coded(FailureKind::Other, codes::PLUGIN_TIMED_OUT);
    };
    let (code, reason) = describe(error);
    coded(FailureKind::Other, code).with_param("reason", reason)
}

/// A stable code and one line: the trap reason matters to the person, the backtrace does not.
fn describe(error: &wasmtime::Error) -> (&'static str, String) {
    if let Some(trap) = error.downcast_ref::<wasmtime::Trap>() {
        return match trap {
            wasmtime::Trap::OutOfFuel => (
                codes::PLUGIN_OUT_OF_FUEL,
                "compute budget (fuel) exhausted".to_owned(),
            ),
            wasmtime::Trap::Interrupt => {
                (codes::PLUGIN_TIMED_OUT, "time limit exceeded".to_owned())
            }
            wasmtime::Trap::UnreachableCodeReached => (
                codes::PLUGIN_CRASHED,
                "crashed (panic or unreachable)".to_owned(),
            ),
            other => (
                codes::PLUGIN_CRASHED,
                readable(&other.to_string(), MAX_REASON_CHARS),
            ),
        };
    }
    let reason = readable(&error.root_cause().to_string(), MAX_REASON_CHARS);
    // A refused memory growth (`StoreLimits::trap_on_grow_failure`) is an error of the limiter,
    // not a `Trap`, so what it says is the only way to tell it apart (rDownloader's reading).
    if reason.to_ascii_lowercase().contains("memory") {
        (codes::PLUGIN_OUT_OF_MEMORY, reason)
    } else {
        (codes::PLUGIN_CRASHED, reason)
    }
}

/// `text` clipped to `limit` characters, with control, zero-width and direction characters
/// spelled out (`\n`, `\u{202e}`): the person sees what the plugin sent, not what it would show.
///
/// The rule of `rc_engine::mcp_questions::readable` (RC-080-12), over the one list of such
/// characters, [`rc_rename::invisible`] (RC-080-43).
#[must_use]
pub fn readable(text: &str, limit: usize) -> String {
    let mut shown = String::with_capacity(text.len().min(limit + 16));
    for (kept, c) in text.chars().enumerate() {
        if kept == limit {
            shown.push('\u{2026}');
            break;
        }
        if matches!(c, '\r' | '\n' | '\t') {
            shown.extend(c.escape_default());
        } else if rc_rename::invisible(c) {
            shown.extend(c.escape_unicode());
        } else {
            shown.push(c);
        }
    }
    shown
}

#[cfg(test)]
#[path = "failure_tests.rs"]
mod tests;
