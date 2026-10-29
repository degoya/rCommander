//! The login-script editor (RC-050-08): the list of scripts, one script's steps to add, reorder
//! and remove, a pattern checked as it is typed, the secret a step sends, and a test run against
//! pasted sample output. A "Send secret" step without a password prompt before it is marked, as
//! the setting `login_scripts.warn_secret_prompt` says (RC-050-13).
//!
//! Like [`super::App::apply`], [`Editor::apply`] only changes state and answers what is left to
//! do ([`Action`]): an engine command to dispatch or a test run to start. The window runs it and
//! feeds the answer back as a [`Message`] (`login_scripts/window.rs`). Drawing is
//! `login_scripts/view.rs`.

mod draft;
mod view;
mod window;

use rc_engine::{
    Command, LoginScriptId, LoginScriptInfo, SecretParam, login_script::Rehearsal,
    login_script::TEMPLATES,
};
use rc_vault::{ExposeSecret as _, SecretString};

pub(crate) use draft::{Draft, SecretWarning, SourceChoice, StepKind, StepRow};

/// What the editor shows.
#[derive(Debug, Default)]
pub(crate) struct Editor {
    pub(crate) scripts: Vec<LoginScriptInfo>,
    pub(crate) draft: Draft,
    /// Sample output for the test run.
    pub(crate) sample: String,
    pub(crate) rehearsal: Option<Rehearsal>,
    /// The label and value of a secret to store with the script. The value is held as a
    /// secret (wiped on drop, never in `Debug`) and leaves the editor when it is stored.
    pub(crate) secret_label: String,
    pub(crate) secret_value: SecretString,
    /// The code of the last failure, shown under the form.
    pub(crate) error: Option<String>,
    /// The setting `login_scripts.warn_secret_prompt`: a secret typed without a password prompt
    /// is marked (RC-050-13).
    pub(crate) warn_prompt: bool,
}

#[derive(Debug, Clone)]
pub(crate) enum Message {
    /// The scripts were read (`login_scripts.list`), or why not.
    Listed(Result<Vec<LoginScriptInfo>, &'static str>),
    Select(LoginScriptId),
    New,
    /// A new script from the template of this id.
    Template(&'static str),
    Name(String),
    StepTimeout(String),
    TotalTimeout(String),
    Add(StepKind),
    Remove(usize),
    Up(usize),
    Down(usize),
    /// The pattern of a `wait_for` or the text of a `send_text`.
    Text(usize, String),
    Timeout(usize, String),
    Newline(usize, bool),
    Source(usize, SourceChoice),
    Sleep(usize, String),
    Save,
    /// A script came back from the engine after a save or a secret change, or why not.
    Saved(Result<LoginScriptInfo, &'static str>),
    Delete,
    Deleted(Result<(), &'static str>),
    SecretLabel(String),
    SecretValue(SecretString),
    StoreSecret,
    RemoveSecret(i64),
    Sample(String),
    Rehearse,
    Rehearsed(Result<Rehearsal, &'static str>),
    /// The setting `login_scripts.warn_secret_prompt`, read.
    WarnPrompt(bool),
}

/// What the window does after a message.
#[derive(Debug)]
pub(crate) enum Action {
    None,
    /// Run this command; answer with [`Message::Saved`] for a script outcome, [`Message::Deleted`]
    /// after `login_scripts.delete`, [`Message::Listed`] after `login_scripts.list`.
    Dispatch(Command),
    /// Run `rc_engine::login_script::rehearse` and answer with [`Message::Rehearsed`].
    Rehearse(rc_engine::LoginScript, String),
}

impl Editor {
    /// The command that fills the list when the editor opens.
    pub(crate) fn open() -> (Self, Action) {
        let editor = Self {
            warn_prompt: true,
            ..Self::default()
        };
        (editor, Action::Dispatch(Command::ListLoginScripts))
    }

    pub(crate) fn apply(&mut self, message: Message) -> Action {
        match message {
            Message::Listed(Ok(scripts)) => self.scripts = scripts,
            Message::Select(id) => {
                if let Some(script) = self.scripts.iter().find(|script| script.id == id) {
                    self.draft = Draft::from_script(script);
                    self.rehearsal = None;
                    self.error = None;
                }
            }
            Message::New => self.start(Draft::default()),
            Message::Template(id) => {
                if let Some(template) = TEMPLATES.iter().find(|template| template.id == id) {
                    self.start(Draft::from_template(template));
                }
            }
            Message::Name(name) => self.draft.name = name,
            Message::StepTimeout(value) => self.draft.step_timeout = value,
            Message::TotalTimeout(value) => self.draft.total_timeout = value,
            Message::Add(kind) => self.draft.steps.push(StepRow::new(kind)),
            Message::Remove(at) => {
                if at < self.draft.steps.len() {
                    self.draft.steps.remove(at);
                }
            }
            Message::Up(at) => {
                if at > 0 && at < self.draft.steps.len() {
                    self.draft.steps.swap(at - 1, at);
                }
            }
            Message::Down(at) => {
                if at + 1 < self.draft.steps.len() {
                    self.draft.steps.swap(at, at + 1);
                }
            }
            Message::Text(at, text) => self.with_step(at, |row| row.set_text(text)),
            Message::Timeout(at, value) => self.with_step(at, |row| row.timeout = value),
            Message::Newline(at, on) => self.with_step(at, |row| row.newline = on),
            Message::Source(at, source) => self.with_step(at, |row| row.source = source),
            Message::Sleep(at, value) => self.with_step(at, |row| row.ms = value),
            Message::Save => return self.save(),
            Message::Saved(Ok(script)) => {
                self.draft = Draft::from_script(&script);
                match self.scripts.iter_mut().find(|kept| kept.id == script.id) {
                    Some(kept) => *kept = script,
                    None => self.scripts.push(script),
                }
                self.error = None;
            }
            Message::Delete => {
                if let Some(id) = self.draft.id {
                    return Action::Dispatch(Command::DeleteLoginScript { id });
                }
            }
            Message::Deleted(Ok(())) => {
                let gone = self.draft.id;
                self.scripts.retain(|script| Some(script.id) != gone);
                self.start(Draft::default());
            }
            Message::SecretLabel(label) => self.secret_label = label,
            Message::SecretValue(value) => self.secret_value = value,
            Message::StoreSecret => return self.store_secret(),
            Message::RemoveSecret(secret_id) => {
                if let Some(script_id) = self.draft.id {
                    return Action::Dispatch(Command::ClearLoginScriptSecret {
                        script_id,
                        secret_id,
                    });
                }
            }
            Message::Sample(sample) => self.sample = sample,
            Message::Rehearse => match self.draft.script() {
                Ok(script) => return Action::Rehearse(script, unescape(&self.sample)),
                Err(code) => self.error = Some(code.to_owned()),
            },
            Message::Rehearsed(Ok(rehearsal)) => self.rehearsal = Some(rehearsal),
            Message::WarnPrompt(on) => self.warn_prompt = on,
            Message::Listed(Err(code))
            | Message::Saved(Err(code))
            | Message::Deleted(Err(code))
            | Message::Rehearsed(Err(code)) => self.error = Some(code.to_owned()),
        }
        Action::None
    }

    fn start(&mut self, draft: Draft) {
        self.draft = draft;
        self.rehearsal = None;
        self.error = None;
    }

    fn with_step(&mut self, at: usize, change: impl FnOnce(&mut StepRow)) {
        if let Some(row) = self.draft.steps.get_mut(at) {
            change(row);
        }
    }

    /// `login_scripts.create` or `.update`; nothing while a field is invalid - the field shows
    /// why.
    fn save(&mut self) -> Action {
        let script = match self.draft.script() {
            Ok(script) => script,
            Err(code) => {
                self.error = Some(code.to_owned());
                return Action::None;
            }
        };
        let command = match self.draft.id {
            Some(id) => Command::UpdateLoginScript {
                id,
                patch: rc_engine::LoginScriptPatch {
                    name: Some(self.draft.name.clone()),
                    steps: Some(script.steps),
                    step_timeout_ms: Some(script.step_timeout_ms),
                    total_timeout_ms: Some(script.total_timeout_ms),
                },
            },
            None => Command::CreateLoginScript {
                script: rc_engine::LoginScriptDraft {
                    name: self.draft.name.clone(),
                    steps: script.steps,
                    step_timeout_ms: Some(script.step_timeout_ms),
                    total_timeout_ms: Some(script.total_timeout_ms),
                },
            },
        };
        Action::Dispatch(command)
    }

    /// `login_scripts.set_secret` for a saved script; the typed value leaves the editor here.
    fn store_secret(&mut self) -> Action {
        let Some(script_id) = self.draft.id else {
            self.error = Some("login_script_not_saved".to_owned());
            return Action::None;
        };
        let typed = std::mem::replace(&mut self.secret_value, SecretString::from(""));
        let value = SecretParam::new(typed.expose_secret().to_owned());
        Action::Dispatch(Command::SetLoginScriptSecret {
            script_id,
            label: std::mem::take(&mut self.secret_label),
            value,
        })
    }
}

/// The sample as typed in one line, with `\n`, `\r`, `\t`, `\e` (escape) and `\\` turned into
/// what they stand for, so pasted output with its line breaks and colours can be tried.
pub(crate) fn unescape(sample: &str) -> String {
    let mut out = String::with_capacity(sample.len());
    let mut chars = sample.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('e') => out.push('\u{1b}'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

#[cfg(test)]
#[path = "login_scripts_tests.rs"]
mod tests;
