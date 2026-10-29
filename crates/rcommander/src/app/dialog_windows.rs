//! Dialogs in windows of their own (RC-080-15, RC-080-23). The app runs as an iced daemon
//! (`startup.rs`); every window draws through [`App::window_view`] by its id.
//!
//! The app's state still says which dialog is open (`App::settings`, `App::queue_window`,
//! `App::connections`, ...); after every update [`App::sync_dialog_windows`] opens a window for a
//! dialog that has none and closes the window of one that closed. Which dialogs have a window is
//! [`App::wanted_windows`] (`dialog_windows/kinds.rs`, with what each window draws): the settings
//! always, the queue while `queue.window_own` is on (RC-010-14), and while
//! `appearance.dialog_windows` is on the connection manager, find files, multi-rename,
//! compare/synchronize, the checksum results, the login-script editor, each Lister and the
//! standalone dialogs - the database dialog, the properties, the quick connection, the network
//! drive dialogs, packing and unpacking, the checksum forms, the hotlist dialog and "Run
//! command" (RC-080-23), and the file operations - copy and move, a new folder or text file,
//! delete, a job's conflict and error questions, the progress window (`dialog_windows/
//! file_ops.rs`) - off, they lie over the main window as cards.
//!
//! A window opens invisible: made - or, for the small dialogs asked for again and again, taken
//! back from where it was hidden when its dialog closed ([`DialogWindow::reused`]) -, then placed,
//! owned and shown when it is ready, then booked as shown ([`DialogWindowMsg`]). A dialog that
//! closed before its window was ready lets that window go unshown; one that closes while it is
//! being shown closes it once shown, so a close never overtakes the main window's lock.
//!
//! A modal one - the settings, as Total Commander's configuration, multi-rename and the
//! standalone dialogs - locks the main window while it is open (on Windows it is disabled,
//! `platform::dialog_window`), and the main window's own layers take no input then. A modeless
//! one - every other - leaves it working: the keys go to the window that has the keyboard
//! (`dialog_windows/own.rs`, `queue_window/own.rs`). The main window owns every dialog's window
//! but Lister's, which is a window of its own as in Total Commander (`viewer/window.rs`). The
//! small questions - an MCP call, a host key, a certificate, the vault, quitting, an archive's
//! password, a large fetch, a synchronization - are windows of their own too while
//! `appearance.dialog_windows` is on (`dialog_windows/questions.rs`); off, they draw over a
//! modal dialog's window while one is open, else over the window that has the keyboard, so they
//! are never behind a dialog's window; "Run command" while it has no window, the login-script
//! editor while it has none and the unlock prompt a connection asked for alone draw over the
//! connection manager, wherever it is, else over a modal dialog's window. A file operation's
//! question has a window of its own on top, which comes to the front for each new question; as
//! a card it draws in the main window, which comes to the front when the question appears while
//! a modeless dialog's window has the keyboard ([`App::question_to_front`]). The title bar's X
//! and Alt+F4 do what Escape does in that window (the main window's quit as F10 does,
//! `quit.rs`). Each dialog window opens where it was left (`window.dialog.<name>`), the first
//! time centred over the main window. The books are `dialog_windows/book.rs`.

mod book;
mod dialog_window;
mod file_ops;
mod kinds;
mod own;
mod question_window;
mod questions;

use std::time::Instant;

use iced::{
    Element, Length, Point, Size, Task,
    widget::{Id, Space, Stack, opaque, operation},
    window::{self, Position},
};
use rc_db::Db;
use rc_engine::Command;

pub use self::book::{DialogWindow, DialogWindows};
pub(super) use self::kinds::first_field;

/// A dialog's window on its way to the screen: `since` the update that opened it, `reused` when
/// it was taken back hidden rather than made - for the time it took, in the debug log.
#[derive(Debug, Clone)]
pub enum DialogWindowMsg {
    /// The window is there, invisible: it is placed and shown if its dialog is still open.
    Ready {
        id: window::Id,
        since: Instant,
        reused: bool,
    },
    /// The window is shown and has the keyboard.
    Shown {
        id: window::Id,
        since: Instant,
        reused: bool,
    },
}
use super::{
    App, Effect, Message, connections, mcp_confirm, settings, ssh_exec,
    window_state::{Geometry, WindowChange},
};
use crate::{
    platform,
    theme::{self, Tokens},
};

impl App {
    /// What window `id` shows: a dialog's window, or the main window with the dialog layers over
    /// it - the connection manager, the login-script editor over the manager, the "Run command"
    /// dialog (RC-060-04) over those, the offer to remove an unused vault (RC-050-03), a host-key
    /// question over them (RC-050-07), an FTPS certificate question beside it (RC-070-02) and
    /// the question of an MCP call over everything (RC-010-13). While a modal dialog's window is
    /// open, the main window takes no click.
    pub fn window_view(&self, id: window::Id) -> Element<'_, Message> {
        let tokens = theme::tokens(self.dark);
        let kind = self.dialog_windows.kind_of(id);
        let mut layers = match kind {
            Some(DialogWindow::Queue) => return self.queue_window_view(tokens),
            Some(kind) => self.dialog_layers(kind, tokens),
            // A dialog's window on its way out.
            None if self.dialog_windows.main().is_some_and(|main| main != id) => Vec::new(),
            None => self.main_layers(tokens),
        };
        if layers.is_empty() {
            return Space::new().into();
        }
        if self.manager_home() == kind {
            layers.extend(self.manager_layers(kind, tokens));
        }
        if self.question_home() == kind {
            layers.extend(self.question_layers(tokens));
        }
        stacked(layers)
    }

    /// The main window and the connection manager over it while it has no window of its own.
    fn main_layers(&self, tokens: &'static Tokens) -> Vec<Element<'_, Message>> {
        let mut layers = vec![self.view()];
        if self.main_locked() {
            layers.push(opaque(
                Space::new().width(Length::Fill).height(Length::Fill),
            ));
        } else if !self.windowed(DialogWindow::Connections) {
            layers.extend(
                self.connections
                    .as_ref()
                    .map(|manager| connections::layer(manager, tokens)),
            );
        }
        layers
    }

    /// The layers of the connection manager that may open without it, in the window `home`: the
    /// unlock prompt a connection asked for alone over a modal dialog's window, which the locked
    /// main window would hide (RC-080-23), the login-script editor (RC-050-08) and "Run command"
    /// (RC-060-04) while they have no window of their own.
    fn manager_layers(
        &self,
        home: Option<DialogWindow>,
        tokens: &'static Tokens,
    ) -> Vec<Element<'_, Message>> {
        let mut layers = Vec::new();
        if home.is_some_and(|kind| kind != DialogWindow::Connections) {
            // While the dialogs have windows, the prompt has one of its own (`questions.rs`).
            layers.extend(
                self.connections
                    .as_ref()
                    .filter(|manager| manager.prompt_only && !self.dialog_windows.own)
                    .map(|manager| connections::layer(manager, tokens)),
            );
        }
        if !self.windowed(DialogWindow::LoginScripts) {
            layers.extend(self.login_scripts_layer(tokens));
        }
        if !self.windowed(DialogWindow::SshExec) {
            layers.extend(ssh_exec::layer(self.ssh_exec.as_ref(), tokens));
        }
        layers
    }

    /// The small questions over any dialog, the top one last; none while the dialogs have
    /// windows, where each question has one of its own (`questions.rs`).
    fn question_layers(&self, tokens: &'static Tokens) -> Vec<Element<'_, Message>> {
        let mut layers = Vec::new();
        if self.dialog_windows.own {
            return layers;
        }
        layers.extend(self.vault_offer_layer(tokens));
        layers.extend(self.quit_layer(tokens));
        layers.extend(self.host_key_layer(tokens));
        layers.extend(self.cert_trust_layer(tokens));
        layers.extend(mcp_confirm::layer(&self.mcp_questions, tokens));
        layers
    }

    /// Where the manager's layers draw: in its window while it has one, else over a modal
    /// dialog's window while one is open, else (`None`) over the main window.
    pub(super) fn manager_home(&self) -> Option<DialogWindow> {
        let wanted = self.wanted_windows();
        if wanted.contains(&DialogWindow::Connections) {
            return Some(DialogWindow::Connections);
        }
        wanted
            .into_iter()
            .find(|kind| kind.modal() && !kind.is_question())
    }

    /// Where the small questions draw: over a modal dialog's window while one is open, else over
    /// the dialog window that has the keyboard, else (`None`) over the main window - never behind
    /// a window the main window owns.
    pub(super) fn question_home(&self) -> Option<DialogWindow> {
        let wanted = self.wanted_windows();
        if let Some(modal) = wanted
            .iter()
            .copied()
            .find(|kind| kind.modal() && !kind.is_question())
        {
            return Some(modal);
        }
        self.dialog_windows
            .focused_kind()
            .filter(|kind| kind.takes_questions() && wanted.contains(kind))
    }

    /// Opens a window for each dialog that is open without one and closes the window of each
    /// dialog that closed, and brings the main window's menu bar in step (RC-080-18); after every
    /// update.
    pub(super) fn sync_dialog_windows(&mut self) -> Task<Message> {
        let made_way = self.question_makes_way();
        let (opening, closing) = self.dialog_windows.difference(&self.wanted_windows());
        let mut tasks: Vec<Task<Message>> = closing
            .into_iter()
            .map(|id| self.close_dialog_window(id))
            .collect();
        tasks.push(made_way);
        // Before the new windows are booked: a question's window opening now comes to the front
        // as it shows.
        tasks.push(self.question_to_front());
        tasks.extend(
            opening
                .into_iter()
                .map(|kind| self.open_dialog_window(kind)),
        );
        tasks.push(self.sync_menu_bar());
        Task::batch(tasks)
    }

    /// A file operation's question (a conflict, an error) in a window of its own comes to the
    /// front with each new question while its window is shown (one opening shows in front). As
    /// a card it draws in the main window: when one appears while a modeless dialog's window has
    /// the keyboard - the queue's, a Lister's, find files' -, the main window comes to the front
    /// and takes the keyboard, so the question is seen and its keys reach it. A window the main
    /// window owns stays in front of it.
    fn question_to_front(&mut self) -> Task<Message> {
        match self.front_for_question() {
            Some(id) => window::gain_focus(id),
            None => Task::none(),
        }
    }

    /// The window a new question of the file operations wants in front: its own, or the main
    /// window.
    pub(super) fn front_for_question(&mut self) -> Option<window::Id> {
        let asking = self.file_ops.jobs.asking().map(|job| job.id());
        match self.question_window() {
            Some(kind) => {
                let new = self.dialog_windows.new_question(asking);
                self.dialog_windows.visible(kind).filter(|_| new)
            }
            None => self.dialog_windows.question(asking),
        }
    }

    /// Makes the window of `kind`, invisible, or takes back the one hidden since its dialog
    /// last closed; it shows when it is ready ([`App::on_dialog_window`]).
    fn open_dialog_window(&mut self, kind: DialogWindow) -> Task<Message> {
        let geometry = self.dialog_windows.geometry(kind, self.window.geometry());
        let since = Instant::now();
        if let Some(id) = self.dialog_windows.reopen(kind, geometry) {
            let ready = DialogWindowMsg::Ready {
                id,
                since,
                reused: true,
            };
            return Task::done(Message::DialogWindow(ready));
        }
        let (id, opened) = window::open(dialog_settings(kind, geometry));
        self.dialog_windows.opening(kind, id, geometry);
        opened.map(move |id| {
            Message::DialogWindow(DialogWindowMsg::Ready {
                id,
                since,
                reused: false,
            })
        })
    }

    /// A dialog's window on its way: ready, it is placed, owned and shown while its dialog is
    /// still open - else it goes, never shown, so it locked nothing -; shown, it closes at the
    /// next update if its dialog closed meanwhile. The debug log has the time from the update
    /// that opened it to its showing (RC-080-23).
    pub(super) fn on_dialog_window(&mut self, message: DialogWindowMsg) -> Vec<Effect> {
        match message {
            DialogWindowMsg::Ready { id, since, reused } => {
                let Some(kind) = self.dialog_windows.kind_of(id) else {
                    return Vec::new();
                };
                if !self.windowed(kind) {
                    return vec![Effect::Task(self.drop_dialog_window(id))];
                }
                self.dialog_windows.advance(id, true);
                if let DialogWindow::Question(question) = kind {
                    let show = self.show_question_window(id, question, since, reused);
                    return vec![Effect::Task(show)];
                }
                let main = self.dialog_windows.main().filter(|_| kind.owned());
                // The field the dialog gives the keyboard at its opening, again once its window
                // is there: the focus asked for with the dialog reached no window yet.
                let focus = self.opening_focus(kind);
                let shown = DialogWindowMsg::Shown { id, since, reused };
                let show = platform::dialog_window::show(id, main, kind.modal())
                    .chain(focus)
                    .chain(Task::done(Message::DialogWindow(shown)));
                vec![Effect::Task(show)]
            }
            DialogWindowMsg::Shown { id, since, reused } => {
                self.dialog_windows.advance(id, false);
                tracing::debug!(
                    window = ?self.dialog_windows.kind_of(id),
                    reused,
                    millis = since.elapsed().as_millis(),
                    "a dialog window is shown"
                );
                Vec::new()
            }
        }
    }

    /// The field the dialog of `kind` gives the keyboard when its window shows.
    fn opening_focus(&self, kind: DialogWindow) -> Task<Message> {
        if kind.file_ops() {
            return self.file_ops_focus(kind);
        }
        first_field(kind).map_or_else(Task::none, |field| operation::focus(Id::new(field)))
    }

    /// The window `id`, ready but no longer wanted, goes without being shown: hidden for the
    /// next opening, or closed.
    fn drop_dialog_window(&mut self, id: window::Id) -> Task<Message> {
        match self.dialog_windows.closed(id) {
            Some((kind, _)) if kind.reused() => {
                self.dialog_windows.park(kind, id);
                Task::none()
            }
            _ => window::close(id),
        }
    }

    /// Closes window `id` - hides it, for a dialog asked for again and again - and stores where
    /// it was; the main window takes input again with the last modal dialog's window closing.
    fn close_dialog_window(&mut self, id: window::Id) -> Task<Message> {
        if let Some(closed) = self.close_question_first(id) {
            return closed;
        }
        let closing = self.dialog_windows.closed(id);
        let modal = closing.as_ref().is_none_or(|(kind, _)| kind.modal())
            && !self.dialog_windows.modal_open();
        let stored = match (closing, self.db.clone()) {
            (Some((kind, geometry)), Some(db)) => Task::future(async move {
                // A place that cannot be stored is only missing at the next opening.
                if let Err(error) = db.set(kind.key(), &geometry).await {
                    tracing::warn!(
                        key = kind.key(),
                        %error,
                        "a dialog window's place was not stored"
                    );
                }
            })
            .discard(),
            _ => Task::none(),
        };
        let main = self.dialog_windows.main();
        let closed = match closing {
            Some((kind, _)) if kind.reused() => {
                self.dialog_windows.park(kind, id);
                platform::dialog_window::hide(id, main, modal)
            }
            _ => platform::dialog_window::close(id, main, modal),
        };
        Task::batch([stored, closed])
    }

    /// Window `id` asks to close: a dialog's window as Escape would close it, the main window
    /// as F10 quits - `app.quit`, which asks first while jobs run or wait (owner, 2026-10-18;
    /// RC-080-17), then stores the tabs and the places and ends the app (`quit.rs`).
    pub(super) fn close_requested(&mut self, id: window::Id) -> Vec<Effect> {
        match self.dialog_windows.kind_of(id) {
            // Its X closes it whichever window has the keyboard; the queue runs on.
            Some(DialogWindow::Queue) => {
                return self.on_queue(super::queue_window::QueueMsg::Close);
            }
            // Escape in that window, the top layer first: a question in the dialog, a running
            // search, then the dialog. Clicking the X gives the window the keyboard anyway.
            Some(_) => {
                self.dialog_windows.focus(id, true);
                return self.apply(Message::close_dialog());
            }
            None => {}
        }
        // A request a dialog's window sent before it closed ends nothing.
        if self.dialog_windows.main().is_none_or(|main| main == id) {
            return vec![Effect::Dispatch(self.active, vec![Command::Quit {}])];
        }
        Vec::new()
    }

    /// Window `id` moved or changed its size; `maximized` is its state at that moment.
    pub(super) fn window_seen(&mut self, id: window::Id, change: WindowChange, maximized: bool) {
        if self.dialog_windows.observe(id, change, maximized) {
            return;
        }
        if self.dialog_windows.main().is_none_or(|main| main == id) {
            self.window.observe(change, maximized);
        }
    }
}

/// Where each dialog window was left, read before the main window opens.
pub async fn load(db: &Db) -> Vec<(DialogWindow, Geometry)> {
    let mut stored = Vec::new();
    for kind in DialogWindow::ALL {
        let geometry = db.get::<Geometry>(kind.key()).await.ok().flatten();
        if let Some(geometry) = geometry.and_then(Geometry::sanitized) {
            stored.push((kind, geometry));
        }
    }
    stored
}

/// The window of `kind` at `geometry`: the app's icon, invisible until it is placed and owned
/// (`platform::dialog_window::show`), closed by the app, not by iced.
fn dialog_settings(kind: DialogWindow, geometry: Geometry) -> window::Settings {
    let position = geometry.position.map_or(Position::Centered, |(x, y)| {
        Position::Specific(Point::new(x, y))
    });
    without_drop(window::Settings {
        size: Size::new(geometry.width, geometry.height),
        position,
        maximized: geometry.maximized,
        min_size: Some(kind.min_size()),
        visible: false,
        icon: settings::window_icon(),
        exit_on_close_request: false,
        ..window::Settings::default()
    })
}

/// Files dropped on a dialog's window go nowhere: only the main window takes them.
#[cfg(windows)]
fn without_drop(settings: window::Settings) -> window::Settings {
    window::Settings {
        platform_specific: window::settings::PlatformSpecific {
            drag_and_drop: false,
            ..settings.platform_specific
        },
        ..settings
    }
}

#[cfg(not(windows))]
fn without_drop(settings: window::Settings) -> window::Settings {
    settings
}

fn stacked(mut layers: Vec<Element<'_, Message>>) -> Element<'_, Message> {
    if layers.len() == 1 {
        return layers.remove(0);
    }
    Stack::with_children(layers).into()
}

#[cfg(test)]
#[path = "dialog_windows_tests.rs"]
mod tests;
