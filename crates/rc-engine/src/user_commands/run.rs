//! `usercmd.run` and `start_menu.run` (RC-100-86): an own command or a Start menu entry run for
//! a panel. A program gets its placeholders filled in from both panels (`params.rs`), its list
//! files written (`list_files.rs`) and starts (`launch.rs`); `?` in front of its parameters hands
//! them to the window to change first ([`EngineEvent::UserCommandAsk`]), which runs the command
//! again with them. Internal rows run one after the other through the registry, a row only the
//! window has goes to the window ([`EngineEvent::WindowRow`]); own commands call each other at
//! most [`MAX_DEPTH`] deep. A button of a bar (RC-100-87) runs its command here too
//! ([`Engine::run_command_ref`]), with files dropped on it after a program's parameters.
//!
//! Over MCP (`read_guard::from_mcp`) the gate's class holds here too, whatever changed since it
//! was judged: a program that starts as administrator or asks, parameters from the caller, and
//! a row stricter than `Exec` are refused as `gui_only` (and dropped files); each row goes through
//! [`Engine::dispatch_mcp`], and a window row answers `user_command_window_only`.

use std::{future::Future, path::PathBuf, pin::Pin};

use crate::recover::LogPanic as _;

use super::{
    AskRequest, AskSource, CommandKind, CommandRef, MAX_DEPTH, StartItemId, StartItemKind,
    UserCommandError, UserCommandsOutcome, answer,
    launch::{Launch, Started, start},
    list_files::write_lists,
    params::{
        Expanded, PanelSide, ParamContext, expand_posix, expand_windows, lists_in, marks_only,
    },
    tc_names,
};
use crate::{
    Command, CommandClass, Engine, EngineError, EngineEvent, Keymap, Outcome, PanelId, PanelState,
    associations::file_params, commands::elevate::unc_folder, dispatch::call_token,
    import::total_commander_path, mcp_questions::current_pins, read_guard::from_mcp, with_mcp_pins,
};

/// One run: the panel it is for, the command that started it and what it did.
struct Run {
    panel: PanelId,
    /// `usercmd.run`, `start_menu.run`, `button_bar.run`, `file.open` or `file.run_association`,
    /// for a refusal over MCP.
    id: &'static str,
    started: usize,
    asking: bool,
    /// A file association's run (RC-100-91): what the placeholders read, the file standing in
    /// for the entry under the cursor; `%1` and `%2` are filled in too. `None` reads the panels.
    context: Option<ParamContext>,
    /// Files dropped on a button: the first program started gets them after its parameters.
    files: Vec<String>,
}

/// A file association's command to run (RC-100-91): for which panel, as which command, what the
/// placeholders read and whose parameters the window asks for (`?`).
pub(crate) struct FileRun {
    pub panel: PanelId,
    /// `file.open` or `file.run_association`.
    pub id: &'static str,
    pub context: ParamContext,
    pub source: AskSource,
    /// What the question names.
    pub title: String,
}

/// The boxed future of [`Engine::run_action`]: own commands call each other.
type RunFuture<'a> = Pin<Box<dyn Future<Output = Result<(), EngineError>> + Send + 'a>>;

/// Where an action comes from: whose parameters the window asks for, and what it names.
struct Origin {
    source: AskSource,
    title: String,
}

impl Engine {
    /// `usercmd.run`: the own command `name` for `panel`; `params` replace a program's own.
    pub(crate) async fn run_user_command(
        &self,
        name: &str,
        panel: Option<PanelId>,
        params: Option<String>,
    ) -> Result<Outcome, EngineError> {
        let command = self.user_command(name).await?;
        let mut run = Run {
            panel: self.resolve(panel),
            id: "usercmd.run",
            started: 0,
            asking: false,
            context: None,
            files: Vec::new(),
        };
        let origin = Origin {
            title: title_of(&command.tooltip, &command.name),
            source: AskSource::Own {
                name: command.name.clone(),
            },
        };
        self.run_action(&mut run, origin, &command.action, params, 0)
            .await?;
        Ok(ran(&run))
    }

    /// `start_menu.run`: the entry `id` for `panel`.
    pub(crate) async fn run_start_item(
        &self,
        id: StartItemId,
        panel: Option<PanelId>,
        params: Option<String>,
    ) -> Result<Outcome, EngineError> {
        let item = self.start_item(id).await?;
        let action = match (item.kind, item.action) {
            (StartItemKind::Item, Some(action)) => action,
            _ => {
                return Err(UserCommandError::StartInvalid {
                    reason: "only an entry runs, no submenu or separator".to_owned(),
                }
                .into());
            }
        };
        let mut run = Run {
            panel: self.resolve(panel),
            id: "start_menu.run",
            started: 0,
            asking: false,
            context: None,
            files: Vec::new(),
        };
        let origin = Origin {
            title: item.title,
            source: AskSource::Start { id },
        };
        self.run_action(&mut run, origin, &action, params, 0)
            .await?;
        Ok(ran(&run))
    }

    /// Runs `action` at call depth `depth`.
    fn run_action<'a>(
        &'a self,
        run: &'a mut Run,
        origin: Origin,
        action: &'a CommandRef,
        params: Option<String>,
        depth: usize,
    ) -> RunFuture<'a> {
        Box::pin(async move {
            match action.kind {
                CommandKind::Program => {
                    Box::pin(self.run_program(run, origin, action, params)).await
                }
                CommandKind::Own => {
                    let target = self.user_command(&action.command).await?;
                    if depth + 1 > MAX_DEPTH {
                        return Err(UserCommandError::Recursion { name: target.name }.into());
                    }
                    let origin = Origin {
                        title: title_of(&target.tooltip, &target.name),
                        source: AskSource::Own {
                            name: target.name.clone(),
                        },
                    };
                    self.run_action(run, origin, &target.action, params, depth + 1)
                        .await
                }
                CommandKind::Internal => {
                    for row in action.rows() {
                        // Boxed, as every step of the recursion: a debug build's test thread has 2 MiB of
                        // stack, and the registry's futures are large.
                        Box::pin(self.run_row(run, row, depth)).await?;
                    }
                    Ok(())
                }
            }
        })
    }

    /// One internal row: another own command, a Total Commander name, a keymap row or a row
    /// only the window has.
    async fn run_row(&self, run: &mut Run, row: &str, depth: usize) -> Result<(), EngineError> {
        let lower = row.to_ascii_lowercase();
        if lower.starts_with(super::OWN_PREFIX) {
            let target = self.user_command(&lower).await?;
            if depth + 1 > MAX_DEPTH {
                return Err(UserCommandError::Recursion { name: target.name }.into());
            }
            let origin = Origin {
                title: title_of(&target.tooltip, &target.name),
                source: AskSource::Own {
                    name: target.name.clone(),
                },
            };
            return self
                .run_action(run, origin, &target.action, None, depth + 1)
                .await;
        }
        let name = if lower.starts_with("cm_") {
            tc_names::row_of(&lower).ok_or_else(|| UserCommandError::UnknownRow {
                row: row.to_owned(),
            })?
        } else {
            row
        };
        match row_command(name) {
            Some(RowCommand::Binding(command)) => {
                // Over MCP no row stricter than the run's own class.
                if from_mcp()
                    && command.effective_class().stricter(CommandClass::Exec) != CommandClass::Exec
                {
                    return Err(EngineError::GuiOnly { command: run.id });
                }
                self.dispatch_row(*command).await
            }
            Some(RowCommand::Window(name)) => {
                if from_mcp() {
                    return Err(UserCommandError::WindowOnly { row: name }.into());
                }
                self.publish(EngineEvent::WindowRow { name });
                Ok(())
            }
            None => Err(UserCommandError::UnknownRow {
                row: row.to_owned(),
            }
            .into()),
        }
    }

    /// Dispatches a row's command as a task of its own: the registry's `match` is polled on a
    /// fresh stack instead of on top of the one it was called from (a debug build's test thread
    /// has 2 MiB; RC-010-27). Over MCP through [`Engine::dispatch_mcp`], which refuses what is
    /// `GuiOnly` and runs it under the read judge again. Task-locals do not cross the spawn, so
    /// the call's MCP pins (a panel moved since the question answers `panel_changed`) and its
    /// cancellation are carried in.
    async fn dispatch_row(&self, command: Command) -> Result<(), EngineError> {
        let engine = self.clone();
        let mcp = from_mcp();
        let pins = current_pins();
        let cancel = call_token();
        tokio::spawn(async move {
            let run = async {
                if mcp {
                    engine.dispatch_mcp_cancellable(command, cancel).await
                } else {
                    engine.dispatch_cancellable(command, cancel).await
                }
            };
            match pins {
                Some(pins) => with_mcp_pins(pins, run).await,
                None => run.await,
            }
        })
        .await
        .log_panic("user_commands.row")
        .map_err(|error| UserCommandError::Failed {
            source: std::io::Error::other(error),
        })??;
        Ok(())
    }

    /// A program: asked for first (`?`), or started with its placeholders filled in.
    async fn run_program(
        &self,
        run: &mut Run,
        origin: Origin,
        action: &CommandRef,
        params: Option<String>,
    ) -> Result<(), EngineError> {
        if from_mcp()
            && (action.elevated || action.asks() || params.is_some() || !run.files.is_empty())
        {
            return Err(EngineError::GuiOnly { command: run.id });
        }
        let template = match params {
            Some(params) => params,
            None if action.asks() => {
                self.publish(EngineEvent::UserCommandAsk(Box::new(AskRequest {
                    source: origin.source,
                    panel: run.panel,
                    params: action.params_to_ask().to_owned(),
                    title: origin.title,
                })));
                run.asking = true;
                return Ok(());
            }
            None => action.params.clone(),
        };
        // An association's `%1` and `%2` (RC-100-91), and the file as the cursor's entry.
        let (template, mut context) = match &run.context {
            Some(context) => (file_params(&template), context.clone()),
            None => (template, self.param_context(run.panel)),
        };
        let kinds = lists_in(&template);
        if !kinds.is_empty() {
            let folder = context
                .source_dir
                .clone()
                .ok_or(UserCommandError::LocalOnly { panel: "source" })?;
            let folder = with_separator(&folder, context.separator);
            let names = context.names(PanelSide::Source, marks_only(&template));
            context.lists = write_lists(&kinds, &folder, &names).await?;
        }
        let mut expanded = if cfg!(windows) {
            expand_windows(&template, &context)?
        } else {
            expand_posix(&template, &context)?
        };
        add_files(&mut expanded, std::mem::take(&mut run.files));
        // The batch-file check reads the program as found (`launch::resolve_checked`).
        let program = expand_variables(&action.command);
        let mut folder = match expand_variables(&action.folder).trim() {
            "" => context.source_dir,
            folder => Some(folder.to_owned()),
        };
        if action.elevated
            && cfg!(windows)
            && let Some(local) = &folder
        {
            // The elevated program does not see the session's drive letters.
            let drives = self.inner.drives.list(false).await;
            folder = Some(unc_folder(local, &drives));
        }
        let folder = folder.map(PathBuf::from);
        let started = start(Launch {
            program,
            args: expanded.args,
            values: expanded.values,
            folder,
            show: action.show,
            elevated: action.elevated,
        })
        .await?;
        match started {
            Started::Started => run.started += 1,
            Started::Cancelled => self.publish(EngineEvent::RunAsAdminCancelled),
        }
        Ok(())
    }

    /// What the placeholders read from both panels now: `panel` is the source.
    pub(crate) fn param_context(&self, panel: PanelId) -> ParamContext {
        let state = self.state();
        let (source_dir, source_cursor, source_marked) = side_of(state.panel(panel));
        let (target_dir, target_cursor, target_marked) = side_of(state.panel(panel.other()));
        ParamContext {
            source_dir,
            source_cursor,
            source_marked,
            target_dir,
            target_cursor,
            target_marked,
            lists: Vec::new(),
            separator: std::path::MAIN_SEPARATOR,
        }
    }
}

/// The folder, the name under the cursor and the marked names (in panel order) of `panel`;
/// none of them when it shows no local folder.
fn side_of(panel: &PanelState) -> (Option<String>, Option<String>, Vec<String>) {
    // A server's or an archive's names never reach a local program (`%N`, `%S` answer
    // `user_command_local_only` as `%P` does): a server's name may even hold a `"`.
    if panel.path.backend != rc_vfs::BackendId::Local {
        return (None, None, Vec::new());
    }
    let dir = Some(panel.path.path.to_string_lossy().into_owned());
    let cursor = panel
        .entries
        .get(panel.cursor)
        .map(|entry| entry.name.clone());
    let marked = if panel.selection.is_empty() {
        Vec::new()
    } else {
        panel
            .entries
            .iter()
            .filter(|entry| panel.selection.contains(&entry.name))
            .map(|entry| entry.name.clone())
            .collect()
    };
    (dir, cursor, marked)
}

/// `files` after the parameters of `expanded`: on Windows each quoted at the end of the one text
/// (a path holds no `"`), elsewhere one argument each; every one a value for the batch-file check.
fn add_files(expanded: &mut Expanded, files: Vec<String>) {
    if files.is_empty() {
        return;
    }
    if cfg!(windows) {
        let quoted: Vec<String> = files.iter().map(|file| format!("\"{file}\"")).collect();
        let mut text = expanded.args.join(" ");
        for file in quoted {
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(&file);
        }
        expanded.args = vec![text];
    } else {
        expanded.args.extend(files.iter().cloned());
    }
    expanded.values.extend(files);
}

/// `folder` ending in `separator`.
fn with_separator(folder: &str, separator: char) -> String {
    if folder.ends_with(['\\', '/']) {
        folder.to_owned()
    } else {
        format!("{folder}{separator}")
    }
}

/// `text` with the known `%VAR%` filled in (`import::total_commander_path::KNOWN_VARIABLES`);
/// `%COMMANDER_PATH%` is rCommander's own folder. A name not known or not set stays as written,
/// as Total Commander leaves it. Also what a folder typed into the path bar or after the command
/// line's `cd` goes through (RC-100-92).
pub fn expand_variables(text: &str) -> String {
    let own_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.to_string_lossy().into_owned()));
    let lookup = |name: &str| -> Option<String> {
        if name.eq_ignore_ascii_case("COMMANDER_PATH") {
            return own_dir.clone();
        }
        total_commander_path::environment(name, None)
    };
    total_commander_path::expand(text, &lookup)
}

/// What a row of the keymap is.
enum RowCommand {
    /// A binding: the command it runs (boxed: a command is far larger than a name).
    Binding(Box<Command>),
    /// A key the window handles itself, by its name.
    Window(String),
}

/// The standard keymap's row `name`.
fn row_command(name: &str) -> Option<RowCommand> {
    let standard = Keymap::shared_standard();
    if let Some(binding) = standard.bindings().iter().find(|b| b.name == name) {
        return Some(RowCommand::Binding(Box::new(binding.command.clone())));
    }
    standard
        .window()
        .iter()
        .find(|key| key.name == name)
        .map(|key| RowCommand::Window(key.name.clone()))
}

/// The tooltip, or the name when there is none.
fn title_of(tooltip: &str, name: &str) -> String {
    if tooltip.trim().is_empty() {
        name.to_owned()
    } else {
        tooltip.trim().to_owned()
    }
}

/// The answer of a run.
fn ran(run: &Run) -> Outcome {
    answer(UserCommandsOutcome::Ran {
        started: run.started,
        asking: run.asking,
    })
}

#[path = "run_entries.rs"]
mod entries;

#[cfg(test)]
#[path = "run_tests.rs"]
mod tests;
