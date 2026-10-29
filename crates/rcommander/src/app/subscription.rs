//! What the window listens to: input, the engine's events, the job queue's results, the
//! system's light or dark mode, the menu bar's choices and the screens of the terminals the
//! panels show.
//!
//! The job queue is one stream: it runs every job to its end before it takes the next, so
//! commands reach the engine in the order the keys were pressed (Tab, then ArrowDown, moves the
//! cursor in the panel Tab made active). The engine's events come on a stream of their own, so
//! the changes an MCP client makes arrive the same way as the window's own.

use std::{
    hash::{Hash, Hasher},
    sync::PoisonError,
};

use iced::{
    Subscription,
    event::{self, Event},
    futures::{Stream, stream},
    keyboard, mouse, system, window,
};
use rc_engine::Engine;
use tokio::sync::broadcast::error::RecvError;

use super::{
    App, JobQueue, Message, queue_window::QueueMsg, run_job, terminal_pane::widget_message,
    window_state::WindowChange,
};
use crate::keymap;

impl App {
    pub fn subscription(&self) -> Subscription<Message> {
        // Every event, captured or not: a terminal captures the keys it handles - the toggle
        // among them - and every click inside it.
        // A key a widget used is told apart, so a prompt's typing never runs a panel's key.
        let input = event::listen_with(|event, status, id| match event {
            Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                modified_key,
                physical_key,
                modifiers,
                text,
                ..
            }) => keymap::pressed(&key, &modified_key, physical_key, modifiers).map(|chord| {
                match status {
                    event::Status::Captured => Message::KeyCaptured(chord),
                    // A typed character may be the command line's (RC-010-08).
                    event::Status::Ignored => super::cmdline::key_message(chord, text.as_deref()),
                }
            }),
            Event::Mouse(mouse::Event::ButtonPressed(_)) => Some(Message::Press),
            Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => Some(
                Message::FileOps(super::file_ops::FileOpsMsg::Modifiers(modifiers)),
            ),
            Event::Window(window::Event::Moved(at)) => {
                Some(Message::WindowChanged(id, WindowChange::Moved(at)))
            }
            Event::Window(window::Event::Resized(size)) => {
                Some(Message::WindowChanged(id, WindowChange::Resized(size)))
            }
            Event::Window(window::Event::CloseRequested) => Some(Message::CloseRequested(id)),
            // Which window has the keyboard: a modeless dialog's own window - the queue, the
            // connection manager, find files, compare - takes keys only then.
            Event::Window(window::Event::Focused) => {
                Some(Message::Queue(QueueMsg::Focused(id, true)))
            }
            Event::Window(window::Event::Unfocused) => {
                Some(Message::Queue(QueueMsg::Focused(id, false)))
            }
            Event::Window(window::Event::FileDropped(path)) => {
                Some(super::drag::file_dropped(path))
            }
            _ => None,
        });
        let mut all = vec![
            input,
            Subscription::run_with(EngineEvents(self.engine.clone()), engine_events),
            Subscription::run_with(
                Jobs {
                    engine: self.engine.clone(),
                    queue: self.queue.clone(),
                },
                job_results,
            ),
        ];
        all.push(super::drive_bar::poll());
        all.push(super::cmdline::presses());
        // A lost graphics device, once (gpu_loss.rs).
        all.push(super::quit::gpu_lost());
        // The entries chosen in the menu bar (RC-080-18).
        all.push(super::palette::menu_bar::subscription());
        // The self-update's start and the look of "install when idle" (RC-100-05, RC-100-07).
        all.push(super::update_ui::subscription(&self.engine));
        // The pointer's moves only while a press on a row may become a drag (RC-080-01).
        if self.drag.tracking() {
            all.push(event::listen_with(super::drag::pointer_events));
        }
        // And while a tab is pressed, which may become a drag (RC-010-24).
        if self.tab_pressed() {
            all.push(event::listen_with(super::tabs::pointer_events));
        }
        // And while a column's or the divider's grip is held (RC-010-20).
        if self.resize.grabbing() {
            all.push(event::listen_with(super::resize::pointer_events));
        }
        // And while Ctrl is held and a viewer shows a file: the wheel sizes its text (RC-020-12).
        if self.wheel_sizes_text() {
            all.push(event::listen_with(super::viewer::wheel_events));
        }
        if self.follow_system {
            all.push(system::theme_changes().map(Message::SystemTheme));
        }
        all.extend(
            self.terms
                .widgets
                .values()
                .map(|widget| widget.subscription().map(widget_message)),
        );
        Subscription::batch(all)
    }
}

/// The engine whose events the window follows; one per window, so its identity is constant.
struct EngineEvents(Engine);

impl Hash for EngineEvents {
    fn hash<H: Hasher>(&self, state: &mut H) {
        "rcommander.engine-events".hash(state);
    }
}

fn engine_events(events: &EngineEvents) -> impl Stream<Item = Message> + use<> {
    stream::unfold(events.0.subscribe(), |mut receiver| async move {
        match receiver.recv().await {
            Ok(event) => Some((Message::Engine(event), receiver)),
            Err(RecvError::Lagged(_)) => Some((Message::EngineLagged, receiver)),
            Err(RecvError::Closed) => None,
        }
    })
}

/// The job queue and the engine it runs on; one per window.
struct Jobs {
    engine: Engine,
    queue: JobQueue,
}

impl Hash for Jobs {
    fn hash<H: Hasher>(&self, state: &mut H) {
        "rcommander.jobs".hash(state);
    }
}

fn job_results(jobs: &Jobs) -> impl Stream<Item = Message> + use<> {
    let engine = jobs.engine.clone();
    let receiver = jobs
        .queue
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take();
    stream::unfold((engine, receiver), |(engine, mut receiver)| async move {
        let job = receiver.as_mut()?.recv().await?;
        let message = run_job(&engine, job).await;
        Some((message, (engine, receiver)))
    })
}
