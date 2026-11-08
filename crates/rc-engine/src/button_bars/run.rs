//! `button_bar.run` (RC-100-87): one button of a bar run for a panel through the own commands'
//! run (`user_commands/run.rs`) - a program with its placeholders filled in, internal rows, an
//! own command. Files dropped on a button that starts a program go after its parameters, one
//! argument each, under the same rules as the placeholders' values (the batch-file check
//! included); a button with internal rows takes none, a sub-bar's button runs nothing. Over MCP
//! a call with files or parameters, and a button whose command starts as administrator, asks or
//! runs a row stricter than `Exec`, is the person's (`ESCALATIONS`, `judge_class`).

use std::path::Path;

use super::{ButtonAction, ButtonBarError, ButtonBarId, ButtonBarsOutcome, answer};
use crate::{
    AskSource, Command, Engine, EngineError, Outcome, PanelId,
    user_commands::{CommandKind, CommandRef, MAX_FIELD},
};

/// The most files one drop hands to a button.
pub const MAX_DROPPED_FILES: usize = 1024;

impl Engine {
    /// The command of button `button` (its place among the bar's items, from 0) of bar `bar`,
    /// and what the window names it by.
    pub(crate) async fn button_command(
        &self,
        bar: ButtonBarId,
        button: usize,
    ) -> Result<(CommandRef, String), EngineError> {
        let found = self.button_bar(bar).await?;
        let item = found
            .spec
            .items
            .get(button)
            .and_then(super::BarItem::button)
            .ok_or(ButtonBarError::NoButton { button })?;
        match &item.action {
            ButtonAction::Command(action) => {
                let title = if item.tooltip.is_empty() {
                    action.command.clone()
                } else {
                    item.tooltip.clone()
                };
                Ok((action.clone(), title))
            }
            ButtonAction::SubBar(_) => Err(ButtonBarError::SubBar.into()),
        }
    }

    /// `button_bar.run`.
    pub(crate) async fn run_button(
        &self,
        bar: ButtonBarId,
        button: usize,
        panel: Option<PanelId>,
        params: Option<String>,
        files: Vec<String>,
    ) -> Result<Outcome, EngineError> {
        let (action, title) = self.button_command(bar, button).await?;
        let files = checked_files(files)?;
        if !files.is_empty() && action.kind == CommandKind::Internal {
            return Err(ButtonBarError::NoFiles.into());
        }
        let source = AskSource::Button {
            bar,
            button,
            files: files.clone(),
        };
        let (started, asking) = self
            .run_command_ref(&action, title, source, panel, params, files)
            .await?;
        Ok(answer(ButtonBarsOutcome::Ran { started, asking }))
    }

    /// Whether what is stored makes an MCP call of `button_bar.run` a person's to make; `false`
    /// for a button that does not read (the run refuses it anyway).
    pub(crate) async fn stored_button_needs_person(&self, command: &Command) -> bool {
        let Command::RunButton { bar, button, .. } = command else {
            return false;
        };
        match self.button_command(*bar, *button).await {
            Ok((action, _)) => self.action_needs_person(&action, 0).await,
            Err(_) => false,
        }
    }
}

/// The dropped files: each an absolute local path without a control character, at most
/// [`MAX_FIELD`] characters, at most [`MAX_DROPPED_FILES`] of them.
fn checked_files(files: Vec<String>) -> Result<Vec<String>, EngineError> {
    if files.len() > MAX_DROPPED_FILES {
        return Err(ButtonBarError::FileInvalid.into());
    }
    for file in &files {
        let fits = !file.is_empty()
            && file.chars().count() <= MAX_FIELD
            && !file.chars().any(char::is_control)
            && Path::new(file).is_absolute();
        if !fits {
            return Err(ButtonBarError::FileInvalid.into());
        }
    }
    Ok(files)
}

#[cfg(test)]
#[path = "run_tests.rs"]
mod tests;
