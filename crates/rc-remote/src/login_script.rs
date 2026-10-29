//! Login scripts (RC-050-08): what a session types after it logged in, such as `sudo -i` and the
//! password it asks for.
//!
//! A script is linear - no conditions, loops or variables: wait for a prompt, send text, send a
//! stored secret, send Enter, sleep. The model here is what `rcommander.db` stores as JSON
//! ([`steps_to_json`], with [`SCHEMA_VERSION`]); a secret is only ever a reference to a vault
//! field ([`SecretRef`]), never a value. [`run_script`] plays a script against any async byte
//! stream ([`ScriptStream`]), so it is tested without a server and the SSH PTY channel of
//! RC-060-03 drives it the same way; [`rehearse`] plays it against a pasted sample for the
//! editor.
//!
//! Every input is bounded: at most [`MAX_STEPS`] steps, [`MAX_TEXT_CHARS`] per text,
//! [`MAX_PATTERN_CHARS`] per pattern compiled within a size limit ([`Pattern`]), timeouts and
//! sleeps within [`MAX_TIMEOUT_MS`] and [`MAX_SLEEP_MS`].

mod ansi;
mod engine;
mod pattern;
mod rehearse;
mod templates;

use serde::{Deserialize, Serialize};

pub use ansi::strip_ansi;
pub use engine::{
    BUFFER_LIMIT, ENTER, ScriptError, ScriptEvent, ScriptOutcome, ScriptStream, SecretError,
    SecretSource, run_script, run_script_observed,
};
pub use pattern::{MAX_PATTERN_CHARS, Pattern, PatternError};
pub use rehearse::{Rehearsal, RehearsalStep, rehearse};
pub use templates::{TEMPLATES, Template};

/// The version of the steps' JSON; a stored script of another version is refused, not guessed.
pub const SCHEMA_VERSION: i64 = 1;

/// Steps a script holds at most.
pub const MAX_STEPS: usize = 64;

/// Characters of one text to send, at most.
pub const MAX_TEXT_CHARS: usize = 1024;

/// A timeout (a step's or the whole run's) is at most ten minutes.
pub const MAX_TIMEOUT_MS: u64 = 600_000;

/// A `sleep` step waits at most a minute.
pub const MAX_SLEEP_MS: u64 = 60_000;

/// How long a `wait_for` without its own timeout waits, unless the script says otherwise.
pub const DEFAULT_STEP_TIMEOUT_MS: u64 = 15_000;

/// How long a whole run may take, unless the script says otherwise.
pub const DEFAULT_TOTAL_TIMEOUT_MS: u64 = 120_000;

/// A script: its steps in order and its timeouts. The database row adds id and name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoginScript {
    pub steps: Vec<Step>,
    /// What a `wait_for` without its own timeout waits, in milliseconds.
    pub step_timeout_ms: u64,
    /// The whole run ends with `timed_out` after this, in milliseconds.
    pub total_timeout_ms: u64,
}

impl Default for LoginScript {
    fn default() -> Self {
        Self {
            steps: Vec::new(),
            step_timeout_ms: DEFAULT_STEP_TIMEOUT_MS,
            total_timeout_ms: DEFAULT_TOTAL_TIMEOUT_MS,
        }
    }
}

/// One step. The serde form is `{"step": "<kind>", ...}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "step", rename_all = "snake_case")]
pub enum Step {
    /// Waits until the output since the last match, without ANSI sequences, matches `pattern`.
    WaitFor {
        pattern: Pattern,
        /// The script's step timeout when `None`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timeout_ms: Option<u64>,
    },
    /// Sends `text`, and Enter after it when `newline`.
    SendText {
        text: String,
        #[serde(default)]
        newline: bool,
    },
    /// Sends a stored secret, and Enter after it when `newline` (the default).
    SendSecret {
        source: SecretRef,
        #[serde(default = "enter_by_default")]
        newline: bool,
    },
    /// Sends Enter alone.
    SendLine,
    /// Waits `ms` milliseconds.
    Sleep { ms: u64 },
}

fn enter_by_default() -> bool {
    true
}

/// Which stored secret a `send_secret` step sends: never the value, only where it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SecretRef {
    /// The password stored with the connection the script runs on.
    ConnectionPassword,
    /// A secret stored with the script itself (`login_script_secrets`), by its id.
    Stored { id: i64 },
}

/// Why a script is refused. [`ScriptInvalid::code`] is the stable form.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ScriptInvalid {
    #[error("a login script holds at most {max} steps")]
    TooManySteps { max: usize },
    #[error("step {step}: a text holds at most {max} characters")]
    TextTooLong { step: usize, max: usize },
    #[error("step {step:?}: a timeout or sleep lies between 1 and {max} ms")]
    TimeoutInvalid { step: Option<usize>, max: u64 },
    #[error(
        "step {step}: a secret is sent only right after a wait_for step, to the prompt it waited for"
    )]
    SecretWithoutPrompt { step: usize },
    #[error("step {step:?}: {error}")]
    Pattern {
        step: Option<usize>,
        error: PatternError,
    },
    #[error("the stored steps have schema version {version}, this build reads {SCHEMA_VERSION}")]
    SchemaUnknown { version: i64 },
    #[error("the stored steps cannot be read: {reason}")]
    Malformed { reason: String },
}

impl ScriptInvalid {
    /// Every code, in declaration order.
    pub const ALL_CODES: &[&str] = &[
        "login_script_too_many_steps",
        "login_script_text_too_long",
        "login_script_timeout_invalid",
        "login_script_secret_without_prompt",
        "login_script_pattern_invalid",
        "login_script_schema_unknown",
        "login_script_malformed",
    ];

    /// The stable code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::TooManySteps { .. } => "login_script_too_many_steps",
            Self::TextTooLong { .. } => "login_script_text_too_long",
            Self::TimeoutInvalid { .. } => "login_script_timeout_invalid",
            Self::SecretWithoutPrompt { .. } => "login_script_secret_without_prompt",
            Self::Pattern { .. } => "login_script_pattern_invalid",
            Self::SchemaUnknown { .. } => "login_script_schema_unknown",
            Self::Malformed { .. } => "login_script_malformed",
        }
    }
}

impl LoginScript {
    /// Checks every bound; a pattern was checked when it was made. A `send_secret` step comes
    /// right after a `wait_for` step: a secret typed blindly lands in whatever has the input - a
    /// shell's command line, its history, the screen (RC-080-12).
    ///
    /// # Errors
    ///
    /// The first bound broken.
    pub fn validate(&self) -> Result<(), ScriptInvalid> {
        if self.steps.len() > MAX_STEPS {
            return Err(ScriptInvalid::TooManySteps { max: MAX_STEPS });
        }
        timeout(None, self.step_timeout_ms, MAX_TIMEOUT_MS)?;
        timeout(None, self.total_timeout_ms, MAX_TIMEOUT_MS)?;
        for (index, step) in self.steps.iter().enumerate() {
            match step {
                Step::WaitFor { timeout_ms, .. } => {
                    if let Some(ms) = timeout_ms {
                        timeout(Some(index), *ms, MAX_TIMEOUT_MS)?;
                    }
                }
                Step::SendText { text, .. } => {
                    if text.chars().count() > MAX_TEXT_CHARS {
                        return Err(ScriptInvalid::TextTooLong {
                            step: index,
                            max: MAX_TEXT_CHARS,
                        });
                    }
                }
                Step::Sleep { ms } => timeout(Some(index), *ms, MAX_SLEEP_MS)?,
                Step::SendSecret { .. } => {
                    let prompted = index
                        .checked_sub(1)
                        .and_then(|before| self.steps.get(before))
                        .is_some_and(|before| matches!(before, Step::WaitFor { .. }));
                    if !prompted {
                        return Err(ScriptInvalid::SecretWithoutPrompt { step: index });
                    }
                }
                Step::SendLine => {}
            }
        }
        Ok(())
    }

    /// The secrets the steps send, each once, in step order.
    #[must_use]
    pub fn secret_refs(&self) -> Vec<SecretRef> {
        let mut refs = Vec::new();
        for step in &self.steps {
            if let Step::SendSecret { source, .. } = step
                && !refs.contains(source)
            {
                refs.push(*source);
            }
        }
        refs
    }
}

fn timeout(step: Option<usize>, ms: u64, max: u64) -> Result<(), ScriptInvalid> {
    if (1..=max).contains(&ms) {
        Ok(())
    } else {
        Err(ScriptInvalid::TimeoutInvalid { step, max })
    }
}

/// The steps as stored: a JSON array of [`SCHEMA_VERSION`].
///
/// # Errors
///
/// Never for these types; the `Result` keeps serde's signature.
pub fn steps_to_json(steps: &[Step]) -> Result<String, ScriptInvalid> {
    serde_json::to_string(steps).map_err(|error| ScriptInvalid::Malformed {
        reason: error.to_string(),
    })
}

/// The steps stored as `json` under schema `version`, every pattern compiled again.
///
/// # Errors
///
/// Another version, JSON that is not a step list, or a pattern that no longer compiles.
pub fn steps_from_json(version: i64, json: &str) -> Result<Vec<Step>, ScriptInvalid> {
    if version != SCHEMA_VERSION {
        return Err(ScriptInvalid::SchemaUnknown { version });
    }
    serde_json::from_str(json).map_err(|error| ScriptInvalid::Malformed {
        reason: error.to_string(),
    })
}

#[cfg(test)]
#[path = "login_script_tests.rs"]
mod tests;
