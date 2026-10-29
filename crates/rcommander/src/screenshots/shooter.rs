//! The window of `--screenshots`: the real app inside a thin program that walks the scenes.
//!
//! Every message of the app passes through unchanged ([`Shot::App`]); between them the shooter
//! runs a scene's steps one after the other with a pause for the engine's answers, waits the
//! scene's settle time, photographs the scene's window (retrying while it is still on its way)
//! and writes the PNG off the UI thread. A step that cannot run, a window that never shows and
//! a picture that cannot be written end the run with the reason in the report.

use std::{
    sync::{Arc, Mutex, PoisonError},
    time::Duration,
};

use iced::{Element, Subscription, Task, Theme, window, window::Screenshot};
use rc_engine::{Chord, CommandClass, EngineEvent, McpPanelContext, McpQuestion, McpSessionOffer};

use super::{
    Setup,
    out::{self, OutDir, Taken},
    scenes::{Ask, Capture, Look, Opening, Plan, Step},
};
use crate::{
    app::{App, Effect, Message, dialog_windows::DialogWindow, settings::SettingsMessage},
    theme::{self, ThemeChoice},
};

/// The pause after a step, for the engine's answer and the redraw.
const GAP: Duration = Duration::from_millis(250);
/// How often and how long a window still on its way is waited for.
const RETRY: Duration = Duration::from_millis(250);
const MAX_TRIES: u32 = 40;
/// The ids of the questions shown, counted down from the top so no call's id is taken.
const FIRST_QUESTION: u64 = u64::MAX;
/// How many selected names a question shows, as the engine's do.
const SHOWN_ENTRIES: usize = 5;

/// What the run took, and why it stopped early.
#[derive(Debug, Default)]
pub struct Report {
    pub shots: Vec<Taken>,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
#[allow(
    clippy::large_enum_variant,
    reason = "the app's messages pass through as the app takes them, unboxed"
)]
pub enum Shot {
    App(Message),
    /// The next step of the scene, or its shot after the last one.
    Step,
    Shoot,
    Captured(Screenshot),
    Saved(Result<Taken, String>),
}

pub struct Shooter {
    app: App,
    plan: Arc<Plan>,
    out: OutDir,
    /// The scene and its next step: 0 switches the look, 1 to n are the steps.
    at: usize,
    step: usize,
    tries: u32,
    /// The questions shown in this scene, closed after its shot.
    raised: Vec<u64>,
    asked: u64,
    report: Arc<Mutex<Report>>,
    done: bool,
}

/// Runs the window until the last scene is taken or one fails.
///
/// # Errors
///
/// iced cannot open the window.
pub fn show(
    setup: Setup,
    plan: Plan,
    out: OutDir,
    report: Arc<Mutex<Report>>,
) -> Result<(), iced::Error> {
    let plan = Arc::new(plan);
    iced::daemon(
        move || {
            Shooter::boot(
                setup.clone(),
                Arc::clone(&plan),
                out.clone(),
                Arc::clone(&report),
            )
        },
        Shooter::update,
        Shooter::view,
    )
    .title(Shooter::title)
    .theme(Shooter::theme)
    .subscription(Shooter::subscription)
    .default_font(theme::UI_FONT)
    .font(theme::icon::FONT_BYTES)
    .run()
}

/// `message` after `delay`.
fn after(delay: Duration, message: Shot) -> Task<Shot> {
    Task::perform(tokio::time::sleep(delay), move |()| message)
}

impl Shooter {
    fn boot(
        setup: Setup,
        plan: Arc<Plan>,
        out: OutDir,
        report: Arc<Mutex<Report>>,
    ) -> (Self, Task<Shot>) {
        let (app, opened) = App::boot(setup);
        let startup = plan.startup;
        let shooter = Self {
            app,
            plan,
            out,
            at: 0,
            step: 0,
            tries: 0,
            raised: Vec::new(),
            asked: 0,
            report,
            done: false,
        };
        (
            shooter,
            Task::batch([opened.map(Shot::App), after(startup, Shot::Step)]),
        )
    }

    fn title(&self, id: window::Id) -> String {
        self.app.title(id)
    }

    fn theme(&self, id: window::Id) -> Option<Theme> {
        self.app.theme(id)
    }

    fn view(&self, id: window::Id) -> Element<'_, Shot> {
        self.app.window_root(id).map(Shot::App)
    }

    fn subscription(&self) -> Subscription<Shot> {
        self.app.subscription().map(Shot::App)
    }

    fn update(&mut self, message: Shot) -> Task<Shot> {
        match message {
            Shot::App(message) => self.app.update(message).map(Shot::App),
            _ if self.done => Task::none(),
            Shot::Step => self.next(),
            Shot::Shoot => self.shoot(),
            Shot::Captured(picture) => self.save(picture),
            Shot::Saved(Ok(taken)) => self.taken(taken),
            Shot::Saved(Err(error)) => self.finish(Some(error)),
        }
    }

    fn next(&mut self) -> Task<Shot> {
        let plan = Arc::clone(&self.plan);
        let Some(scene) = plan.scenes.get(self.at) else {
            return self.finish(None);
        };
        let index = self.step;
        self.step += 1;
        if index == 0 {
            let effects = scene
                .theme
                .map(|look| self.app.set_theme(choice(look)))
                .unwrap_or_default();
            return Task::batch([self.perform(effects), after(GAP, Shot::Step)]);
        }
        match scene.steps.get(index - 1) {
            Some(step) => match self.run(step) {
                Ok((task, wait)) => Task::batch([task, after(wait, Shot::Step)]),
                Err(error) => self.finish(Some(format!(
                    "scene '{}', step {index}: {error}",
                    scene.name
                ))),
            },
            None => {
                self.tries = 0;
                after(scene.settle, Shot::Shoot)
            }
        }
    }

    /// One step, and how long to wait before the next.
    fn run(&mut self, step: &Step) -> Result<(Task<Shot>, Duration), String> {
        let task = match step {
            Step::Key(chord) => {
                if let Some(id) = self.refused(chord) {
                    return Err(format!("the key {chord} runs {id}, which a scene may not"));
                }
                self.app.update(Message::Key(chord.clone())).map(Shot::App)
            }
            Step::Command(command) => {
                let effects = self.app.run_bound((**command).clone());
                self.perform(effects)
            }
            Step::Wait(wait) => return Ok((Task::none(), *wait)),
            Step::Open(Opening::Settings) => self
                .app
                .update(Message::Settings(SettingsMessage::Open))
                .map(Shot::App),
            Step::Close(capture) => {
                let id = self
                    .window_of(*capture)
                    .ok_or_else(|| format!("no {capture:?} window is open to close"))?;
                self.app.update(Message::CloseRequested(id)).map(Shot::App)
            }
            Step::Ask(ask) => {
                let question = self.question(ask);
                self.raised.push(question.id);
                let event = EngineEvent::McpQuestion {
                    question: Box::new(question),
                };
                self.app.update(Message::Engine(event)).map(Shot::App)
            }
        };
        Ok((task, GAP))
    }

    /// The command of a key bound to deleting or to a trust decision; `None` for any other.
    fn refused(&self, chord: &Chord) -> Option<&'static str> {
        let view = self.app.pane(self.app.active).state.view;
        let command = self.app.keymap.lookup(chord, view)?;
        matches!(
            command.spec().class,
            CommandClass::Destructive | CommandClass::GuiOnly
        )
        .then(|| command.id())
    }

    /// Effects as `App::update` runs them, with its two follow-ups.
    fn perform(&mut self, effects: Vec<Effect>) -> Task<Shot> {
        let mut tasks: Vec<Task<Message>> = effects
            .into_iter()
            .map(|effect| self.app.perform(effect))
            .collect();
        tasks.push(self.app.request_icons());
        tasks.push(self.app.sync_dialog_windows());
        Task::batch(tasks).map(Shot::App)
    }

    fn question(&mut self, ask: &Ask) -> McpQuestion {
        let id = FIRST_QUESTION - self.asked;
        self.asked += 1;
        let panel = self.app.active;
        let state = &self.app.pane(panel).state;
        let selection = !state.selection.is_empty();
        let entries: Vec<String> = if selection {
            state
                .selection
                .iter()
                .take(SHOWN_ENTRIES)
                .cloned()
                .collect()
        } else {
            state
                .cursor_entry()
                .map(|entry| entry.name.clone())
                .into_iter()
                .collect()
        };
        let more = if selection {
            state.selection.len().saturating_sub(entries.len())
        } else {
            0
        };
        McpQuestion {
            id,
            tool: ask.tool.clone(),
            class: ask.class,
            arguments: ask.arguments.clone(),
            panel: McpPanelContext {
                panel,
                path: state.path.path.display().to_string(),
                entries,
                selection,
                more,
            },
            session: ask.session.then_some(McpSessionOffer::Session),
        }
    }

    fn window_of(&self, capture: Capture) -> Option<window::Id> {
        let windows = &self.app.dialog_windows;
        match capture {
            Capture::Main => windows.main(),
            Capture::Settings => windows.visible(DialogWindow::Settings),
            Capture::Lister => self
                .app
                .wanted_windows()
                .into_iter()
                .filter(|kind| matches!(kind, DialogWindow::Lister(_)))
                .find_map(|kind| windows.visible(kind)),
        }
    }

    fn shoot(&mut self) -> Task<Shot> {
        let plan = Arc::clone(&self.plan);
        let Some(scene) = plan.scenes.get(self.at) else {
            return self.finish(None);
        };
        match self.window_of(scene.capture) {
            Some(id) => window::screenshot(id).map(Shot::Captured),
            None if self.tries < MAX_TRIES => {
                self.tries += 1;
                after(RETRY, Shot::Shoot)
            }
            None => self.finish(Some(format!(
                "scene '{}': its {:?} window never showed",
                scene.name, scene.capture
            ))),
        }
    }

    fn save(&mut self, picture: Screenshot) -> Task<Shot> {
        let plan = Arc::clone(&self.plan);
        let Some(scene) = plan.scenes.get(self.at) else {
            return self.finish(None);
        };
        let name = scene.name.clone();
        let path = match self.out.shot(&name) {
            Ok(path) => path,
            Err(error) => return self.finish(Some(format!("{error:#}"))),
        };
        let (width, height) = (picture.size.width, picture.size.height);
        Task::perform(
            async move {
                let written = tokio::task::spawn_blocking(move || {
                    out::write_png(&path, &picture.rgba, width, height)
                })
                .await;
                match written {
                    Ok(Ok(())) => Ok(Taken {
                        file: format!("{name}.png"),
                        name,
                        width,
                        height,
                    }),
                    Ok(Err(error)) => Err(format!("{error:#}")),
                    Err(error) => Err(error.to_string()),
                }
            },
            Shot::Saved,
        )
    }

    /// A scene's picture is written: its questions close, the next scene begins.
    fn taken(&mut self, taken: Taken) -> Task<Shot> {
        self.report
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .shots
            .push(taken);
        let mut tasks: Vec<Task<Shot>> = std::mem::take(&mut self.raised)
            .into_iter()
            .map(|question| {
                let event = EngineEvent::McpQuestionClosed { question };
                self.app.update(Message::Engine(event)).map(Shot::App)
            })
            .collect();
        self.at += 1;
        self.step = 0;
        tasks.push(after(GAP, Shot::Step));
        Task::batch(tasks)
    }

    fn finish(&mut self, error: Option<String>) -> Task<Shot> {
        self.done = true;
        if let Some(error) = error {
            self.report
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .error
                .get_or_insert(error);
        }
        iced::exit()
    }
}

fn choice(look: Look) -> ThemeChoice {
    match look {
        Look::Dark => ThemeChoice::Dark,
        Look::Light => ThemeChoice::Light,
    }
}
