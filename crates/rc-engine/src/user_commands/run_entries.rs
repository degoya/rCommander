//! The runs that start from elsewhere than a command of their own: a file's internal association
//! (RC-100-91) and a button of a bar (RC-100-87); both go through [`Engine::run_action`].

use super::{FileRun, Origin, Run, ran};
use crate::{
    Engine, EngineError, Outcome, PanelId,
    user_commands::{AskSource, CommandRef},
};

impl Engine {
    /// A file association's `action` (RC-100-91), `params` in place of a program's own (the
    /// window's answer to `?`); answers what started.
    pub(crate) async fn run_for_file(
        &self,
        file: FileRun,
        action: &CommandRef,
        params: Option<String>,
    ) -> Result<Outcome, EngineError> {
        let mut run = Run {
            panel: file.panel,
            id: file.id,
            started: 0,
            asking: false,
            context: Some(file.context),
            files: Vec::new(),
        };
        let origin = Origin {
            source: file.source,
            title: file.title,
        };
        self.run_action(&mut run, origin, action, params, 0).await?;
        Ok(ran(&run))
    }

    /// `button_bar.run`: a button's `action` for `panel`, named `title`; `files` dropped on it go
    /// to the first program it starts. The programs started, and whether the window asks.
    pub(crate) async fn run_command_ref(
        &self,
        action: &CommandRef,
        title: String,
        source: AskSource,
        panel: Option<PanelId>,
        params: Option<String>,
        files: Vec<String>,
    ) -> Result<(usize, bool), EngineError> {
        let mut run = Run {
            panel: self.resolve(panel),
            id: "button_bar.run",
            started: 0,
            asking: false,
            context: None,
            files,
        };
        self.run_action(&mut run, Origin { source, title }, action, params, 0)
            .await?;
        Ok((run.started, run.asking))
    }
}
