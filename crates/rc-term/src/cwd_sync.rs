//! A session's folder between the shell and the panel (RC-040-04): the folder the shell reports
//! (OSC 7, OSC 9;9), where its prompt stands (OSC 133), and the `cd` that follows the panel.
//!
//! The one rule above all: the app types into a shell only when the shell has marked that it
//! waits at an empty prompt. A shell that never marked its prompt (no shell integration) gets
//! nothing, ever; while a command runs, or while something is typed on the command line, the
//! `cd` waits and is typed at the next prompt. The prompt counts as left as soon as a line is
//! entered (Enter in the session's input), so even a shell that marks only the prompt's start
//! (`cmd`) never gets the `cd` into a program it started.
//!
//! [`CwdSync`] is the state alone, without a terminal, so the rules are tested directly; the
//! session ([`TermSession::follow`]) feeds it the program's OSC payloads and its input, and
//! writes what it decides while holding it, so a key typed meanwhile cannot slip between the
//! check and the `cd`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{
    Platform, ShellPaths, ShellSyntax, TermError, TermSession, cd_line,
    osc::{OscEvent, PromptMark, parse_osc},
    panel_to_shell_path, shell_fits, shell_to_panel_path,
};

/// Where a shell's prompt stands, from its OSC 133 marks and the input it got.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptState {
    /// The shell never marked its prompt: no shell integration, nothing is typed into it.
    #[default]
    Unknown,
    /// It waits at its prompt.
    AtPrompt,
    /// A line was entered: a command or a program runs until the next prompt.
    Running,
}

/// Why the shell is not (yet) in the panel's folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FollowStatus {
    /// The shell does not mark its prompt; the `cd` waits for a mark that may never come.
    NoIntegration,
    /// A program runs; the `cd` follows at the next prompt.
    Running,
    /// Something is typed on the command line; the `cd` follows at the prompt after it.
    LineNotEmpty,
    /// The shell cannot be sent there: no path for the folder in a WSL distribution, a name the
    /// shell cannot be given safely, or a shell the app does not type into.
    Unreachable,
    /// The folder is on another system than the shell: a WSL distribution's share for a shell of
    /// this machine, Windows or another distribution for a WSL shell. The shell stays where it
    /// is; the terminal toggle opens a shell of the folder's system.
    OtherSystem,
}

/// What a request to follow a folder came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "result", content = "reason")]
pub enum FollowOutcome {
    /// The shell is there already.
    Already,
    /// The `cd` was typed.
    Sent,
    /// The `cd` waits for the next prompt.
    Waiting(FollowStatus),
    /// Nothing is sent, now or later.
    Unreachable,
}

/// What OSC payloads changed: viewers hear of it.
#[derive(Debug, Default)]
pub(crate) struct Changes {
    pub folder: bool,
    /// The prompt state or the follow status.
    pub prompt: bool,
    /// A waiting `cd`, typed now.
    pub write: Option<Vec<u8>>,
}

/// The folder and prompt of one session.
#[derive(Debug)]
pub(crate) struct CwdSync {
    syntax: Option<ShellSyntax>,
    paths: ShellPaths,
    hosts: Vec<String>,
    /// The folder the session started in, or where the app last sent it.
    folder: PathBuf,
    /// The folder the shell last reported, as it named it.
    reported: Option<String>,
    prompt: PromptState,
    /// Something was typed since the last Enter.
    dirty: bool,
    /// The panel folder a `cd` waits to go to.
    pending: Option<PathBuf>,
    status: Option<FollowStatus>,
}

impl CwdSync {
    /// A session started in `folder`, of a shell the app does not type into until
    /// [`Self::configure`]; `hosts` are this machine's names for OSC 7.
    pub fn new(folder: PathBuf, hosts: Vec<String>) -> Self {
        Self {
            syntax: None,
            paths: ShellPaths::Native(Platform::CURRENT),
            hosts,
            folder,
            reported: None,
            prompt: PromptState::Unknown,
            dirty: false,
            pending: None,
            status: None,
        }
    }

    /// The shell's syntax (`None`: never typed into) and how it names folders.
    pub fn configure(&mut self, syntax: Option<ShellSyntax>, paths: ShellPaths) {
        self.syntax = syntax;
        self.paths = paths;
    }

    /// The folder the shell last reported, as a panel's folder.
    pub fn reported(&self) -> Option<PathBuf> {
        self.reported
            .as_deref()
            .and_then(|reported| shell_to_panel_path(reported, &self.paths))
    }

    /// The best known folder: the reported one, else where it started or was sent.
    pub fn current(&self) -> PathBuf {
        self.reported().unwrap_or_else(|| self.folder.clone())
    }

    /// Whether the shell belongs in the panel folder `target`: it is there already, or it is a
    /// shell of the folder's system ([`shell_fits`]).
    pub fn fits(&self, target: &Path) -> bool {
        let windows = !matches!(self.paths, ShellPaths::Native(Platform::Unix));
        same_folder(&self.current(), target, windows) || shell_fits(&self.paths, target)
    }

    /// Records the folder without telling the shell.
    pub fn set_folder(&mut self, folder: &Path) {
        folder.clone_into(&mut self.folder);
        self.reported = None;
    }

    pub fn prompt(&self) -> PromptState {
        self.prompt
    }

    pub fn status(&self) -> Option<FollowStatus> {
        self.status
    }

    /// One OSC payload of the program's output.
    pub fn on_payload(&mut self, payload: &[u8], changes: &mut Changes) {
        match parse_osc(payload, &self.hosts) {
            Some(OscEvent::Folder(folder)) => {
                if self.reported.as_deref() != Some(folder.as_str()) {
                    self.reported = Some(folder);
                    changes.folder = true;
                }
            }
            Some(OscEvent::Prompt(PromptMark::PromptStart | PromptMark::InputStart)) => {
                changes.prompt |= self.set_prompt(PromptState::AtPrompt);
                if let Some(target) = self.pending.take() {
                    let before = self.status;
                    let (_, line) = self.attempt(target);
                    changes.prompt |= before != self.status;
                    if line.is_some() {
                        changes.write = line;
                    }
                }
            }
            Some(OscEvent::Prompt(PromptMark::CommandStart)) => {
                changes.prompt |= self.set_prompt(PromptState::Running);
            }
            Some(OscEvent::Prompt(PromptMark::CommandEnd)) | None => {}
        }
    }

    /// Input for the program, as typed or pasted: Enter leaves the prompt, Ctrl+C empties the
    /// line, anything else is on the line.
    pub fn on_input(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            match byte {
                b'\r' | b'\n' => {
                    self.dirty = false;
                    if self.prompt == PromptState::AtPrompt {
                        self.prompt = PromptState::Running;
                    }
                }
                0x03 => self.dirty = false,
                _ => self.dirty = true,
            }
        }
    }

    /// The shell should be in the panel folder `target`: the `cd` to type now, or why not.
    pub fn follow(&mut self, target: &Path) -> (FollowOutcome, Option<Vec<u8>>) {
        self.pending = None;
        self.attempt(target.to_path_buf())
    }

    fn attempt(&mut self, target: PathBuf) -> (FollowOutcome, Option<Vec<u8>>) {
        let windows = !matches!(self.paths, ShellPaths::Native(Platform::Unix));
        if same_folder(&self.current(), &target, windows) {
            self.status = None;
            return (FollowOutcome::Already, None);
        }
        if !shell_fits(&self.paths, &target) {
            // Never a local shell into a WSL share, never a WSL shell out of its own.
            self.status = Some(FollowStatus::OtherSystem);
            return (FollowOutcome::Unreachable, None);
        }
        let line = self.syntax.and_then(|syntax| {
            panel_to_shell_path(&target, &self.paths).and_then(|path| cd_line(syntax, &path))
        });
        let Some(line) = line else {
            self.status = Some(FollowStatus::Unreachable);
            return (FollowOutcome::Unreachable, None);
        };
        let wait = match (self.prompt, self.dirty) {
            (PromptState::Unknown, _) => Some(FollowStatus::NoIntegration),
            (PromptState::Running, _) => Some(FollowStatus::Running),
            (PromptState::AtPrompt, true) => Some(FollowStatus::LineNotEmpty),
            (PromptState::AtPrompt, false) => None,
        };
        if let Some(wait) = wait {
            self.pending = Some(target);
            self.status = Some(wait);
            return (FollowOutcome::Waiting(wait), None);
        }
        self.status = None;
        self.prompt = PromptState::Running;
        self.folder = target;
        self.reported = None;
        (FollowOutcome::Sent, Some(line.into_bytes()))
    }

    /// Whether the state changed.
    fn set_prompt(&mut self, prompt: PromptState) -> bool {
        let changed = self.prompt != prompt;
        self.prompt = prompt;
        changed
    }
}

/// Whether two panel folders are the same, ignoring a trailing separator and, for Windows
/// paths, ASCII case.
fn same_folder(a: &Path, b: &Path, windows: bool) -> bool {
    match (a.to_str(), b.to_str()) {
        (Some(a), Some(b)) => {
            let trim = |text: &str| -> String {
                let trimmed = text.trim_end_matches(['/', '\\']);
                if trimmed.is_empty() || trimmed.ends_with(':') {
                    text.to_owned()
                } else {
                    trimmed.to_owned()
                }
            };
            let (a, b) = (trim(a), trim(b));
            if windows {
                a.eq_ignore_ascii_case(&b)
            } else {
                a == b
            }
        }
        _ => a == b,
    }
}

impl TermSession {
    /// The shell should go to the panel folder `folder`: the `cd` is typed now when the shell
    /// waits at an empty prompt, kept for its next prompt when it does not, and never typed
    /// into a shell that does not mark its prompt.
    ///
    /// # Errors
    ///
    /// [`TermError::Exited`] once the program has ended, or the back end's own failure.
    pub fn follow(&self, folder: &Path) -> Result<FollowOutcome, TermError> {
        if self.state().is_exited() {
            return Err(TermError::Exited);
        }
        let mut sync = self.sync();
        let (outcome, line) = sync.follow(folder);
        if let Some(line) = line {
            // Written while the state is held: no key can come between the check and the `cd`.
            self.backend().write(line)?;
        }
        Ok(outcome)
    }

    /// Whether the shell belongs in the panel folder `folder`: it is there, or it is a shell of
    /// the folder's system - not a local shell in a WSL share, not a WSL shell outside its own.
    #[must_use]
    pub fn fits_folder(&self, folder: &Path) -> bool {
        self.sync().fits(folder)
    }

    /// The folder the shell last reported (OSC 7 or OSC 9;9), as a panel's folder.
    #[must_use]
    pub fn reported_folder(&self) -> Option<PathBuf> {
        self.sync().reported()
    }

    /// Where the shell's prompt stands.
    #[must_use]
    pub fn prompt_state(&self) -> PromptState {
        self.sync().prompt()
    }

    /// Why the shell is not in the folder the panel last sent it to; `None` when it is (or was
    /// never sent anywhere).
    #[must_use]
    pub fn follow_status(&self) -> Option<FollowStatus> {
        self.sync().status()
    }
}

#[cfg(test)]
#[path = "cwd_sync_tests.rs"]
mod tests;
