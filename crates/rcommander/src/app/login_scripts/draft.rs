//! The script being edited, as the form holds it: text fields that may not parse yet, each
//! checked as it is typed, and turned into a script only when every field is valid.

use rc_engine::{
    LoginScript, LoginScriptId, LoginScriptInfo, Pattern, ScriptSecretLabel, ScriptSecretRef, Step,
    login_script::{DEFAULT_STEP_TIMEOUT_MS, DEFAULT_TOTAL_TIMEOUT_MS, Template},
};

use crate::i18n::t;

/// The code a pattern that does not compile shows under its field.
pub(crate) const PATTERN_INVALID: &str = "login_script_pattern_invalid";

/// The code a number that does not parse shows.
const TIMEOUT_INVALID: &str = "login_script_timeout_invalid";

/// The kind of a step, as the editor adds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StepKind {
    WaitFor,
    SendText,
    SendSecret,
    SendLine,
    Sleep,
}

impl StepKind {
    pub(crate) const ALL: [Self; 5] = [
        Self::WaitFor,
        Self::SendText,
        Self::SendSecret,
        Self::SendLine,
        Self::Sleep,
    ];

    /// The kind's name in the window.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::WaitFor => t!("login_scripts.step.wait_for"),
            Self::SendText => t!("login_scripts.step.send_text"),
            Self::SendSecret => t!("login_scripts.step.send_secret"),
            Self::SendLine => t!("login_scripts.step.send_line"),
            Self::Sleep => t!("login_scripts.step.sleep"),
        }
    }
}

/// Which secret a `send_secret` step sends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum SourceChoice {
    #[default]
    ConnectionPassword,
    /// A secret stored with the script, by id.
    Stored(i64),
}

/// One step as the form holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StepRow {
    pub(crate) kind: StepKind,
    /// The pattern of a `wait_for`, the text of a `send_text`.
    pub(crate) text: String,
    /// A `wait_for`'s own timeout in milliseconds; empty for the script's.
    pub(crate) timeout: String,
    pub(crate) newline: bool,
    pub(crate) source: SourceChoice,
    /// A `sleep`'s milliseconds.
    pub(crate) ms: String,
    /// Why `text` is no valid pattern, as an error code; shown at the field.
    pub(crate) error: Option<&'static str>,
    /// A `wait_for` whose pattern waits for a password prompt (RC-050-13), worked out as it is
    /// typed rather than on every frame (RC-080-53).
    pub(crate) password_prompt: bool,
}

impl StepRow {
    pub(crate) fn new(kind: StepKind) -> Self {
        let mut row = Self {
            kind,
            text: String::new(),
            timeout: String::new(),
            newline: matches!(kind, StepKind::SendText | StepKind::SendSecret),
            source: SourceChoice::default(),
            ms: "500".to_owned(),
            error: None,
            password_prompt: false,
        };
        row.set_text(String::new());
        row
    }

    /// Takes the typed text; a pattern is checked at once, and whether it waits for a password.
    pub(crate) fn set_text(&mut self, text: String) {
        let pattern = (self.kind == StepKind::WaitFor).then(|| Pattern::new(&text));
        self.error = matches!(pattern, Some(Err(_))).then_some(PATTERN_INVALID);
        self.password_prompt = match &pattern {
            // A pattern that names "pass" itself waits for one too, in any language the samples
            // lack.
            Some(Ok(pattern)) => {
                text.to_lowercase().contains("pass")
                    || PASSWORD_PROMPTS
                        .iter()
                        .any(|sample| pattern.match_end(sample).is_some())
            }
            _ => false,
        };
        self.text = text;
    }

    fn from_step(step: &Step) -> Self {
        let kind = match step {
            Step::WaitFor { .. } => StepKind::WaitFor,
            Step::SendText { .. } => StepKind::SendText,
            Step::SendSecret { .. } => StepKind::SendSecret,
            Step::SendLine => StepKind::SendLine,
            Step::Sleep { .. } => StepKind::Sleep,
        };
        let mut row = Self::new(kind);
        match step {
            Step::WaitFor {
                pattern,
                timeout_ms,
            } => {
                row.set_text(pattern.as_str().to_owned());
                row.timeout = timeout_ms.map(|ms| ms.to_string()).unwrap_or_default();
            }
            Step::SendText { text, newline } => {
                row.text.clone_from(text);
                row.newline = *newline;
            }
            Step::SendSecret { source, newline } => {
                row.source = match source {
                    ScriptSecretRef::ConnectionPassword => SourceChoice::ConnectionPassword,
                    ScriptSecretRef::Stored { id } => SourceChoice::Stored(*id),
                };
                row.newline = *newline;
            }
            Step::SendLine => {}
            Step::Sleep { ms } => row.ms = ms.to_string(),
        }
        row
    }

    /// The step, or the code of the field that keeps it from being one.
    fn step(&self) -> Result<Step, &'static str> {
        Ok(match self.kind {
            StepKind::WaitFor => Step::WaitFor {
                pattern: Pattern::new(&self.text).map_err(|_| PATTERN_INVALID)?,
                timeout_ms: optional_millis(&self.timeout)?,
            },
            StepKind::SendText => Step::SendText {
                text: self.text.clone(),
                newline: self.newline,
            },
            StepKind::SendSecret => Step::SendSecret {
                source: match self.source {
                    SourceChoice::ConnectionPassword => ScriptSecretRef::ConnectionPassword,
                    SourceChoice::Stored(id) => ScriptSecretRef::Stored { id },
                },
                newline: self.newline,
            },
            StepKind::SendLine => Step::SendLine,
            StepKind::Sleep => Step::Sleep {
                ms: millis(&self.ms)?,
            },
        })
    }
}

/// The script in the form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Draft {
    /// `None` until it is saved.
    pub(crate) id: Option<LoginScriptId>,
    pub(crate) name: String,
    pub(crate) steps: Vec<StepRow>,
    pub(crate) step_timeout: String,
    pub(crate) total_timeout: String,
    /// The secrets stored with the saved script, by label.
    pub(crate) secrets: Vec<ScriptSecretLabel>,
}

impl Default for Draft {
    fn default() -> Self {
        Self {
            id: None,
            name: String::new(),
            steps: Vec::new(),
            step_timeout: DEFAULT_STEP_TIMEOUT_MS.to_string(),
            total_timeout: DEFAULT_TOTAL_TIMEOUT_MS.to_string(),
            secrets: Vec::new(),
        }
    }
}

impl Draft {
    pub(crate) fn from_script(script: &LoginScriptInfo) -> Self {
        Self {
            id: Some(script.id),
            name: script.name.clone(),
            steps: script.steps.iter().map(StepRow::from_step).collect(),
            step_timeout: script.step_timeout_ms.to_string(),
            total_timeout: script.total_timeout_ms.to_string(),
            secrets: script.secrets.clone(),
        }
    }

    pub(crate) fn from_template(template: &Template) -> Self {
        let script = template.script();
        Self {
            name: template_name(template.id).to_owned(),
            steps: script.steps.iter().map(StepRow::from_step).collect(),
            ..Self::default()
        }
    }

    /// The script, checked like the engine checks it; the code of the first bad field when it
    /// is none.
    pub(crate) fn script(&self) -> Result<LoginScript, &'static str> {
        let script = LoginScript {
            steps: self
                .steps
                .iter()
                .map(StepRow::step)
                .collect::<Result<_, _>>()?,
            step_timeout_ms: millis(&self.step_timeout)?,
            total_timeout_ms: millis(&self.total_timeout)?,
        };
        script.validate().map_err(|invalid| invalid.code())?;
        Ok(script)
    }

    /// Why the secret the step at `at` sends may be typed without a password prompt (RC-050-13):
    /// no "Wait for" right before it, or one whose pattern neither names "pass" nor matches one of
    /// [`PASSWORD_PROMPTS`] - a shell prompt, say, where the secret would be run as a command and
    /// kept in the history. None for another step, a fitting prompt, or a pattern that does not
    /// compile (its field says so).
    pub(crate) fn secret_warning(&self, at: usize) -> Option<SecretWarning> {
        if self.steps.get(at)?.kind != StepKind::SendSecret {
            return None;
        }
        let before = at.checked_sub(1).and_then(|before| self.steps.get(before));
        let Some(before) = before.filter(|step| step.kind == StepKind::WaitFor) else {
            return Some(SecretWarning::NoWait);
        };
        if before.error.is_some() {
            return None;
        }
        (!before.password_prompt).then_some(SecretWarning::NoPrompt)
    }
}

/// Why a "Send secret" step may type its secret where no password is asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SecretWarning {
    /// The step before is no "Wait for" (the engine refuses to save it, too).
    NoWait,
    /// The step before waits for something that is no password prompt.
    NoPrompt,
}

/// What a prompt for a secret looks like: a pattern that matches one of these waits for one.
const PASSWORD_PROMPTS: [&str; 7] = [
    "Password: ",
    "[sudo] password for user: ",
    "user@host's password: ",
    "Enter passphrase for key '/home/user/.ssh/id_ed25519': ",
    "Verification code: ",
    "PIN: ",
    "Passcode: ",
];

/// A template's name in the window.
pub(crate) fn template_name(id: &str) -> &'static str {
    match id {
        "sudo_i" => t!("login_scripts.template.sudo_i"),
        "su_root" => t!("login_scripts.template.su_root"),
        "second_password" => t!("login_scripts.template.second_password"),
        _ => t!("login_scripts.template.unknown"),
    }
}

fn millis(text: &str) -> Result<u64, &'static str> {
    text.trim().parse().map_err(|_| TIMEOUT_INVALID)
}

fn optional_millis(text: &str) -> Result<Option<u64>, &'static str> {
    if text.trim().is_empty() {
        Ok(None)
    } else {
        millis(text).map(Some)
    }
}
