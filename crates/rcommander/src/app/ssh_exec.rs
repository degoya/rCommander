//! The "Run command" dialog (RC-060-04): one command on an SSH connection, its output and its
//! end.
//!
//! [`ExecMsg::Open`] opens it for a connection (the connection manager and the SFTP panel send
//! it). Enter or Run sends `ssh.exec` with the default limits (60 s, 1 MiB per stream); Cancel
//! drops the run, and the engine stops the command and closes its channel; Escape or Close ends
//! the dialog and a run with it. The result shows the exit code, the signal or the timeout, then
//! standard output and standard error, each with a note when it was cut off - read-only editors
//! built once when the run ends, which lay out only the lines on screen (RC-080-53). While the
//! dialog is open no key or click reaches the panels.

mod view;

use iced::{
    Task,
    task::Handle,
    widget::text_editor::{Action, Content},
};
use rc_engine::{Command, ConnectionId, Engine, ExecOutput, Outcome, SshExecRequest};

pub(super) use self::view::{COMMAND_FIELD, body, layer, title};
use super::{
    App, Effect, Message,
    escape::{enter, escape},
};

/// The open dialog.
pub(crate) struct ExecDialog {
    pub(crate) connection: ConnectionId,
    /// The connection's name, for the title.
    pub(crate) name: String,
    pub(crate) command: String,
    /// The run under way: its number and the handle that aborts it when dropped.
    running: Option<(u64, Handle)>,
    /// The number of the latest run; an answer of an earlier one is ignored.
    runs: u64,
    /// The latest run's output, or the code of its failure.
    pub(crate) result: Option<Result<ExecOutput, &'static str>>,
    /// The latest output's standard output and standard error as the dialog shows them.
    pub(crate) shown: Option<Box<[Content; 2]>>,
}

/// One stream of the output: standard output (`0`) or standard error (`1`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    Out = 0,
    Err = 1,
}

impl ExecDialog {
    pub(crate) fn is_running(&self) -> bool {
        self.running.is_some()
    }
}

#[derive(Debug, Clone)]
pub enum ExecMsg {
    /// Opens the dialog for the SSH connection `connection`, named `name`.
    Open {
        connection: ConnectionId,
        name: String,
    },
    Command(String),
    Run,
    Cancel,
    /// Run `.0` ended: its output, or the code of its failure.
    Finished(u64, Result<Box<ExecOutput>, &'static str>),
    /// Scrolling or selecting in a stream; an edit is dropped.
    Scroll(Stream, Action),
    Close,
}

impl App {
    /// While the dialog is open: Escape closes it, no other key or click reaches the window.
    pub(super) fn ssh_exec_intercept(&mut self, message: &Message) -> Option<Vec<Effect>> {
        self.ssh_exec.as_ref()?;
        match message {
            Message::Key(chord) if *chord == escape() => {
                self.ssh_exec = None;
                Some(Vec::new())
            }
            // Tab walks the focus ring; Enter is Run, the default button (RC-080-16).
            Message::Key(chord) if super::field_walk::tab(chord).is_some() => {
                super::field_walk::walk_key(chord)
            }
            Message::Key(chord) if *chord == enter() => {
                let ready = self.ssh_exec.as_ref().is_some_and(|dialog| {
                    !dialog.is_running() && !dialog.command.trim().is_empty()
                });
                Some(if ready {
                    self.on_ssh_exec(ExecMsg::Run)
                } else {
                    Vec::new()
                })
            }
            Message::Key(_) | Message::Press => Some(Vec::new()),
            _ => None,
        }
    }

    /// Opens the dialog for `connection` with `command` typed in: from the connection manager
    /// (its `name`) or the command line of an SFTP panel (named as the drive bar names it).
    pub(super) fn open_ssh_exec(
        &mut self,
        connection: ConnectionId,
        name: Option<String>,
        command: String,
    ) -> Vec<Effect> {
        let id = format!("server:{connection}");
        let name = name
            .or_else(|| {
                let drives = &self.drive_bar.drives;
                drives
                    .iter()
                    .find(|drive| drive.id == id)
                    .map(|drive| drive.label.clone())
            })
            .unwrap_or_else(|| connection.to_string());
        let mut effects = self.on_ssh_exec(ExecMsg::Open { connection, name });
        effects.extend(self.on_ssh_exec(ExecMsg::Command(command)));
        effects
    }

    pub(super) fn on_ssh_exec(&mut self, message: ExecMsg) -> Vec<Effect> {
        if let ExecMsg::Open { connection, name } = message {
            self.ssh_exec = Some(ExecDialog {
                connection,
                name,
                command: String::new(),
                running: None,
                runs: 0,
                result: None,
                shown: None,
            });
            return Vec::new();
        }
        let Some(dialog) = self.ssh_exec.as_mut() else {
            return Vec::new();
        };
        match message {
            ExecMsg::Open { .. } => {}
            ExecMsg::Command(command) => dialog.command = command,
            ExecMsg::Run if dialog.is_running() || dialog.command.trim().is_empty() => {}
            ExecMsg::Run => return vec![Effect::Task(start(dialog, self.engine.clone()))],
            // Dropping the handle aborts the run.
            ExecMsg::Cancel => {
                if dialog.running.take().is_some() {
                    dialog.result = Some(Err("cancelled"));
                }
            }
            ExecMsg::Finished(run, result) => {
                if dialog
                    .running
                    .as_ref()
                    .is_some_and(|(latest, _)| *latest == run)
                {
                    dialog.running = None;
                    let result = result.map(|output| *output);
                    dialog.shown = result.as_ref().ok().map(|output| {
                        Box::new([
                            Content::with_text(&output.stdout),
                            Content::with_text(&output.stderr),
                        ])
                    });
                    dialog.result = Some(result);
                }
            }
            ExecMsg::Scroll(stream, action) => {
                if !action.is_edit()
                    && let Some(shown) = dialog.shown.as_mut()
                {
                    shown[stream as usize].perform(action);
                }
            }
            ExecMsg::Close => self.ssh_exec = None,
        }
        Vec::new()
    }
}

/// Starts a run of the dialog's command; the task answers [`ExecMsg::Finished`].
fn start(dialog: &mut ExecDialog, engine: Engine) -> Task<Message> {
    dialog.runs += 1;
    let run = dialog.runs;
    let request = SshExecRequest {
        connection_id: dialog.connection,
        command: dialog.command.clone(),
        stdin: None,
        timeout_secs: None,
        max_output_bytes: None,
    };
    let (task, handle) = Task::perform(
        async move { engine.dispatch(Command::SshExec(request)).await },
        move |answer| {
            let result = match answer {
                Ok(Outcome::SshExec(output)) => Ok(Box::new(output)),
                Ok(_) => Err("ssh_protocol"),
                Err(error) => Err(error.code()),
            };
            Message::SshExec(ExecMsg::Finished(run, result))
        },
    )
    .abortable();
    dialog.running = Some((run, handle.abort_on_drop()));
    dialog.result = None;
    dialog.shown = None;
    task
}

#[cfg(test)]
#[path = "ssh_exec_tests.rs"]
mod tests;
