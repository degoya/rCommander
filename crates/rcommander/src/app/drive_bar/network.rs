//! The network drive dialogs (RC-010-16), opened from the drive menu, a drive's context menu or
//! the command palette (`actions.rs`).
//!
//! "Map network drive" takes a folder `\\server\share`, a free drive letter (the last free one
//! proposed, as Windows' own dialog does) and "Reconnect at sign-in". Connecting sends
//! `drives.map_network`; Windows then asks for the user and the password in its own dialog and
//! keeps them - this window never sees them - so the command runs off the window's thread for as
//! long as the person takes. "Disconnect network drive" lists the network drives, the one it was
//! opened for chosen; when files on it are open the engine refuses with `network_drive_in_use`
//! and the dialog asks whether to disconnect anyway, which sends the command again with `force`.
//! What the engine refuses (`network_path_invalid`, `drive_letter_in_use`, ...) shows in the
//! dialog; closing Windows' credential dialog just returns to the form.

use iced::{
    Task,
    widget::{Id, operation},
};
use rc_engine::{Chord, Command, Drive, PanelId, suggest_letter};

use super::{
    DriveMessage,
    actions::{ContextKey, DriveAction, is_network},
};
use crate::{
    app::{App, Effect, Message},
    i18n::error_text,
};

/// The id of the map dialog's folder field, focused when it opens.
pub const REMOTE_FIELD: &str = "rcommander.network.remote";

/// Where a dialog stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    Editing,
    /// The command is on its way: Windows may be asking for the credentials.
    Running,
    Failed(String),
}

/// The map dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapForm {
    pub remote: String,
    /// The letters no drive has, A to Z.
    pub letters: Vec<char>,
    /// The chosen letter; `None` when every letter is taken.
    pub letter: Option<char>,
    pub reconnect: bool,
    pub phase: Phase,
}

impl MapForm {
    pub fn new(drives: &[Drive]) -> Self {
        let letters = free_letters(drives);
        let letter = suggest_letter(drives).or_else(|| letters.last().copied());
        Self {
            remote: String::new(),
            letters,
            letter,
            // Windows' own dialog keeps a drive by default.
            reconnect: true,
            phase: Phase::Editing,
        }
    }
}

/// The disconnect dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnmapForm {
    /// The network drives: id and folder.
    pub drives: Vec<(String, String)>,
    pub cursor: usize,
    /// The engine answered that files on the chosen drive are open: the dialog asks whether to
    /// disconnect anyway.
    pub in_use: bool,
    pub phase: Phase,
}

impl UnmapForm {
    /// The network drives of `drives`, the cursor on `chosen` when it is one of them.
    pub fn new(drives: &[Drive], chosen: Option<&str>) -> Self {
        let drives: Vec<(String, String)> = drives
            .iter()
            .filter(|drive| is_network(drive))
            .map(|drive| (drive.id.clone(), drive.remote.clone().unwrap_or_default()))
            .collect();
        let cursor = chosen
            .and_then(|id| drives.iter().position(|(drive, _)| drive == id))
            .unwrap_or(0);
        Self {
            drives,
            cursor,
            in_use: false,
            phase: Phase::Editing,
        }
    }

    /// The chosen drive's id.
    pub fn chosen(&self) -> Option<&str> {
        self.drives.get(self.cursor).map(|(id, _)| id.as_str())
    }

    /// Another drive is chosen: the question about open files was about the last one.
    fn choose(&mut self, index: usize) {
        if index < self.drives.len() && index != self.cursor {
            self.cursor = index;
            self.in_use = false;
        }
        self.phase = Phase::Editing;
    }
}

/// An open dialog and the panel it was opened for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkDialog {
    pub panel: PanelId,
    pub form: NetworkForm,
    /// Which dialog this is: the answer of a command a dialog closed meanwhile sent is not this
    /// one's (RC-080-44).
    pub generation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkForm {
    Map(MapForm),
    Unmap(UnmapForm),
}

impl NetworkDialog {
    fn phase_mut(&mut self) -> &mut Phase {
        match &mut self.form {
            NetworkForm::Map(form) => &mut form.phase,
            NetworkForm::Unmap(form) => &mut form.phase,
        }
    }
}

/// What the dialogs' controls say.
#[derive(Debug, Clone)]
pub enum NetworkMsg {
    Remote(String),
    Letter(char),
    Reconnect(bool),
    /// A network drive of the disconnect dialog was clicked.
    Choose(usize),
    Confirm,
    Close,
    /// The engine's answer to the dialog of this generation: done, or the code of its refusal.
    Finished(u64, Result<(), &'static str>),
}

/// The letters A to Z no drive of `drives` has.
pub fn free_letters(drives: &[Drive]) -> Vec<char> {
    ('A'..='Z')
        .filter(|letter| {
            let id = format!("{letter}:");
            !drives
                .iter()
                .any(|drive| drive.id.eq_ignore_ascii_case(&id))
        })
        .collect()
}

impl App {
    /// An action of the drive menu, a drive's context menu or the palette: its dialog opens,
    /// the disconnect dialog on `drive` (else the panel's own drive) when it is a network drive.
    pub(in crate::app) fn run_drive_action(
        &mut self,
        panel: PanelId,
        action: DriveAction,
        drive: Option<String>,
    ) -> Vec<Effect> {
        self.drive_bar.menu = None;
        self.drive_bar.context = None;
        let drives = &self.drive_bar.drives;
        let form = match action {
            DriveAction::MapNetwork => NetworkForm::Map(MapForm::new(drives)),
            DriveAction::UnmapNetwork => {
                let chosen = drive.or_else(|| self.current_drive(panel).map(|d| d.id.clone()));
                NetworkForm::Unmap(UnmapForm::new(drives, chosen.as_deref()))
            }
        };
        let map = matches!(form, NetworkForm::Map(_));
        let generation = self.drive_bar.next_dialog();
        self.drive_bar.dialog = Some(NetworkDialog {
            panel,
            form,
            generation,
        });
        if map {
            vec![Effect::Task(operation::focus(Id::new(REMOTE_FIELD)))]
        } else {
            Vec::new()
        }
    }

    pub(super) fn on_network(&mut self, message: NetworkMsg) -> Vec<Effect> {
        let Some(dialog) = self.drive_bar.dialog.as_mut() else {
            return Vec::new();
        };
        match message {
            NetworkMsg::Confirm => return self.confirm_network(),
            NetworkMsg::Close => {
                // A running command cannot be called back; its answer then finds no dialog, or
                // one of another generation.
                self.drive_bar.dialog = None;
            }
            NetworkMsg::Finished(generation, result) if generation != dialog.generation => {
                tracing::debug!(?result, "a closed network drive dialog's answer");
            }
            NetworkMsg::Finished(_, Ok(())) => self.drive_bar.dialog = None,
            NetworkMsg::Finished(_, Err(code)) => {
                if let NetworkForm::Unmap(form) = &mut dialog.form
                    && code == "network_drive_in_use"
                {
                    form.in_use = true;
                    form.phase = Phase::Editing;
                } else if code == "cancelled" {
                    // The person closed Windows' credential dialog.
                    *dialog.phase_mut() = Phase::Editing;
                } else {
                    *dialog.phase_mut() = Phase::Failed(error_text(code));
                }
            }
            NetworkMsg::Choose(index) => {
                if let NetworkForm::Unmap(form) = &mut dialog.form
                    && form.phase != Phase::Running
                {
                    form.choose(index);
                }
            }
            edit => {
                let NetworkForm::Map(form) = &mut dialog.form else {
                    return Vec::new();
                };
                if form.phase == Phase::Running {
                    return Vec::new();
                }
                form.phase = Phase::Editing;
                match edit {
                    NetworkMsg::Remote(remote) => form.remote = remote,
                    NetworkMsg::Letter(letter) => form.letter = Some(letter),
                    NetworkMsg::Reconnect(reconnect) => form.reconnect = reconnect,
                    _ => {}
                }
            }
        }
        Vec::new()
    }

    /// The drive bar's dialogs and context menu take the keys while they are open: a dialog
    /// every key and the clicks beside it. `None` for a message they leave.
    pub(in crate::app) fn drive_bar_intercept(&mut self, message: &Message) -> Option<Vec<Effect>> {
        if self.drive_bar.dialog.is_some() {
            return match message {
                // A key a stop of the focus ring took is not the dialog's again (RC-080-16).
                Message::KeyCaptured(chord) if crate::app::field_walk::taken(chord) => {
                    Some(Vec::new())
                }
                Message::Key(chord) | Message::KeyCaptured(chord) => Some(self.network_key(chord)),
                Message::Press => Some(Vec::new()),
                _ => None,
            };
        }
        let context = self.drive_bar.context.as_mut()?;
        let Message::Key(chord) = message else {
            return None;
        };
        let panel = context.panel;
        let drive = context.drive.clone();
        Some(match context.key(chord) {
            ContextKey::Stay => Vec::new(),
            ContextKey::Close => {
                self.drive_bar.context = None;
                Vec::new()
            }
            ContextKey::Run(action) => self.run_drive_action(panel, action, Some(drive)),
        })
    }

    /// A key while a dialog is open: Escape closes, Enter confirms, Tab walks the fields, the
    /// arrows choose a network drive to disconnect; the fields take the rest.
    fn network_key(&mut self, chord: &Chord) -> Vec<Effect> {
        if let Some(effects) = crate::app::field_walk::walk_key(chord) {
            return effects;
        }
        if !chord.is_plain() {
            return Vec::new();
        }
        match chord.key.as_str() {
            "Escape" => self.drive_bar.dialog = None,
            "Enter" => return self.confirm_network(),
            step @ ("ArrowUp" | "ArrowDown") => {
                if let Some(NetworkDialog {
                    form: NetworkForm::Unmap(form),
                    ..
                }) = self.drive_bar.dialog.as_mut()
                    && form.phase != Phase::Running
                {
                    let index = if step == "ArrowUp" {
                        form.cursor.saturating_sub(1)
                    } else {
                        form.cursor + 1
                    };
                    form.choose(index);
                }
            }
            _ => {}
        }
        Vec::new()
    }

    /// Sends the dialog's command off the window's thread. Enter on a form that already failed
    /// tries again; nothing while one is on its way.
    fn confirm_network(&mut self) -> Vec<Effect> {
        let Some(dialog) = self.drive_bar.dialog.as_mut() else {
            return Vec::new();
        };
        let command = match &dialog.form {
            NetworkForm::Map(form) => {
                let Some(letter) = form.letter else {
                    return Vec::new();
                };
                Command::MapNetworkDrive {
                    remote: form.remote.clone(),
                    letter: letter.to_string(),
                    reconnect: form.reconnect,
                }
            }
            NetworkForm::Unmap(form) => {
                let Some(drive) = form.chosen() else {
                    return Vec::new();
                };
                Command::UnmapNetworkDrive {
                    drive: drive.to_owned(),
                    force: form.in_use,
                }
            }
        };
        let phase = dialog.phase_mut();
        if *phase == Phase::Running {
            return Vec::new();
        }
        *phase = Phase::Running;
        let generation = dialog.generation;
        let engine = self.engine.clone();
        let job = Task::perform(
            async move {
                engine
                    .dispatch(command)
                    .await
                    .map(|_| ())
                    .map_err(|error| error.code())
            },
            move |result| {
                Message::Drive(DriveMessage::Network(NetworkMsg::Finished(
                    generation, result,
                )))
            },
        );
        vec![Effect::Task(job)]
    }
}

#[cfg(test)]
#[path = "network_tests.rs"]
mod tests;
