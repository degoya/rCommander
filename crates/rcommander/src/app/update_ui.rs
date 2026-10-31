//! The self-update in the window (RC-100-05 ... RC-100-07): the notice in the status bar, the
//! Updates page of the settings, the start of the engine's checks and the half-minute look of
//! "install when idle".
//!
//! The notice is quiet: one line at the status bar's left with "What's new" and "Later", never a
//! window of its own and never the focus. "What's new" opens the settings on the Updates page,
//! which shows the version, the date, the size and the release notes as plain text, and the
//! buttons: Check now, Download, Open folder (the panel goes there), Copy the download address
//! (the way past a proxy that asks for a login; nothing here starts a browser), and Install and
//! restart with what ends with it. A running, waiting or paused job holds the install back.
//! "Updated to X" shows only while X runs.
//!
//! An installation a package manager owns (Scoop, Homebrew, winget, apt, dnf; RC-100-52 ...
//! RC-100-54) never installs: the page says so with the reason and shows the manager's command,
//! and "Copy command" stands where "Install and restart" would.

use std::{
    hash::{Hash, Hasher},
    path::Path,
    time::Duration,
};

use iced::{
    Element, Subscription, Task,
    futures::{Stream, stream},
    widget::{Column, Space, button, column, row, text},
};
use rc_engine::{Command, DownloadState, Engine, EngineEvent, UpdateNotice, UpdateStatus};

use super::{
    App, Effect, Message,
    settings::{Page, SettingsMessage},
};
use crate::{
    i18n::{error_text, t},
    theme::{
        self, Tokens,
        fit::{label, label_or_glyph, prose},
        layout,
    },
};

/// How often "install when idle" looks.
const IDLE_LOOK: Duration = Duration::from_secs(30);

/// What the notice, the Updates page and the background send.
#[derive(Debug, Clone)]
pub enum UpdateMsg {
    /// The engine's self-update started (the subscription's first message).
    Started,
    /// Half a minute passed: "install when idle" looks.
    Tick,
    /// The look ended; a hand-over ends the window through the engine's `Quit`.
    Looked,
    /// "What's new": the settings on the Updates page.
    WhatsNew,
    /// "Later": the notice goes until the next check.
    Later,
    CheckNow,
    Download,
    /// The panel goes to the folder of the downloaded file.
    OpenFolder,
    /// The artifact's address to the clipboard.
    CopyAddress,
    /// The package manager's update command to the clipboard.
    CopyCommand,
    Install,
    /// The engine's status, read for the Updates page.
    Status(Box<UpdateStatus>),
}

fn wrap(message: UpdateMsg) -> Message {
    Message::Update(message)
}

/// Reads the engine's status for the Updates page.
pub(super) fn status_task(engine: &Engine) -> Effect {
    let engine = engine.clone();
    Effect::Task(Task::perform(
        async move { engine.update_status().await },
        |status| wrap(UpdateMsg::Status(Box::new(status))),
    ))
}

impl App {
    /// The self-update's share of every message: its own, the engine's update events, the
    /// menu's "Check for updates...", and every key and click for the quiet of `when_idle`.
    pub(super) fn update_intercept(&mut self, message: &Message) -> Option<Vec<Effect>> {
        match message {
            Message::Update(message) => Some(self.on_update(message.clone())),
            Message::Engine(EngineEvent::Update | EngineEvent::UpdateProgress { .. }) => {
                Some(match self.settings {
                    Some(_) => vec![status_task(&self.engine)],
                    None => Vec::new(),
                })
            }
            Message::Settings(SettingsMessage::OpenPage(Page::Updates)) => {
                let mut effects = self.settings_message(SettingsMessage::OpenPage(Page::Updates));
                effects.push(Effect::Dispatch(
                    self.active,
                    vec![Command::CheckForUpdates {}],
                ));
                Some(effects)
            }
            Message::Key(_) | Message::KeyCaptured(_) | Message::Press => {
                self.engine.note_input();
                None
            }
            _ => None,
        }
    }

    fn on_update(&mut self, message: UpdateMsg) -> Vec<Effect> {
        match message {
            UpdateMsg::Started | UpdateMsg::Looked => Vec::new(),
            UpdateMsg::Tick => {
                let engine = self.engine.clone();
                let dialog_open = self.dialog_open() || self.settings.is_some();
                vec![Effect::Task(Task::perform(
                    async move { engine.update_idle_tick(dialog_open).await },
                    |_| wrap(UpdateMsg::Looked),
                ))]
            }
            UpdateMsg::WhatsNew => {
                let mut effects = self.settings_message(SettingsMessage::OpenPage(Page::Updates));
                effects.push(status_task(&self.engine));
                effects
            }
            UpdateMsg::Later => {
                self.engine.dismiss_update_notice();
                Vec::new()
            }
            UpdateMsg::CheckNow => self.dispatch_update(Command::CheckForUpdates {}),
            UpdateMsg::Download => self.dispatch_update(Command::DownloadUpdate {}),
            UpdateMsg::Install => self.dispatch_update(Command::InstallUpdate {}),
            UpdateMsg::OpenFolder => {
                let folder = self.downloaded_file().and_then(|file| {
                    Path::new(&file)
                        .parent()
                        .map(|folder| folder.to_string_lossy().into_owned())
                });
                let Some(path) = folder else {
                    return Vec::new();
                };
                self.settings = None;
                let panel = self.active;
                vec![Effect::Dispatch(
                    panel,
                    vec![Command::Navigate {
                        panel: Some(panel),
                        path,
                    }],
                )]
            }
            UpdateMsg::CopyAddress => {
                let address = self.shown_updates().and_then(|status| {
                    status
                        .offer
                        .as_ref()?
                        .artifact
                        .as_ref()
                        .map(|artifact| artifact.url.clone())
                });
                match address {
                    Some(address) => vec![Effect::Task(iced::clipboard::write(address))],
                    None => Vec::new(),
                }
            }
            UpdateMsg::CopyCommand => {
                match self
                    .shown_updates()
                    .and_then(|status| status.install_command.clone())
                {
                    Some(command) => vec![Effect::Task(iced::clipboard::write(command))],
                    None => Vec::new(),
                }
            }
            UpdateMsg::Status(status) => {
                if let Some(dialog) = self.settings.as_mut() {
                    dialog.set_updates(status);
                }
                Vec::new()
            }
        }
    }

    /// A command of the Updates page; the engine's events bring the page up to date.
    fn dispatch_update(&self, command: Command) -> Vec<Effect> {
        vec![Effect::Dispatch(self.active, vec![command])]
    }

    /// The status the open settings dialog shows.
    fn shown_updates(&self) -> Option<&UpdateStatus> {
        self.settings.as_ref()?.updates()
    }

    fn downloaded_file(&self) -> Option<String> {
        match &self.shown_updates()?.download {
            DownloadState::Ready { file } => Some(file.clone()),
            _ => None,
        }
    }
}

/// The notice at the status bar's left, with its two actions; nothing without one.
pub(super) fn notice<'a>(app: &App, tokens: &'static Tokens) -> Element<'a, Message> {
    let Some(notice) = app.engine.update_notice() else {
        return Space::new().into();
    };
    let (line, action) = notice_line(&notice);
    let link = |caption: &'static str, message: UpdateMsg| {
        button(
            label_or_glyph(caption)
                .size(theme::STATUS_SIZE)
                .color(tokens.accent)
                .wrapping(text::Wrapping::None),
        )
        .padding(0)
        .style(button::text)
        .on_press(wrap(message))
    };
    row![
        label_or_glyph(line)
            .size(theme::STATUS_SIZE)
            .color(tokens.accent)
            .wrapping(text::Wrapping::None),
        link(action, UpdateMsg::WhatsNew),
        link(t!("update.notice.later"), UpdateMsg::Later),
    ]
    .spacing(8)
    .into()
}

/// The notice's text and the caption of its first action.
pub(super) fn notice_line(notice: &UpdateNotice) -> (String, &'static str) {
    match notice {
        UpdateNotice::Available {
            version,
            on_quit: false,
        } => (
            t!("update.notice.available", version = version),
            t!("update.notice.whats_new"),
        ),
        UpdateNotice::Available {
            version,
            on_quit: true,
        } => (
            t!("update.notice.on_quit", version = version),
            t!("update.notice.whats_new"),
        ),
        UpdateNotice::UpdatedTo { version, .. } => (
            t!("update.notice.updated", version = version),
            t!("update.notice.whats_new"),
        ),
        UpdateNotice::Failed { version, code } => (
            t!(
                "update.notice.failed",
                version = version,
                reason = error_text(code)
            ),
            t!("update.notice.details"),
        ),
    }
}

/// The Updates page above its switches: what runs, the last check, the offer with its notes and
/// the buttons.
pub(in crate::app) fn page<'a>(
    status: Option<&UpdateStatus>,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let Some(status) = status else {
        return label(t!("settings.loading")).into();
    };
    let hint = |text: String| prose(text).color(tokens.text_secondary);
    let mut page = Column::new().spacing(8);
    page = page.push(label(t!(
        "update.page.running",
        version = status.running_version,
        channel = status.channel
    )));
    page = page.push(hint(last_check(status)));
    if let Some(reason) = &status.install_reason {
        page = page.push(hint(error_text(reason)));
    }
    if let Some(line) = manager_line(status) {
        page = page.push(label(line));
    }
    let offer = status.offer.as_ref();
    match offer {
        Some(offer) => {
            let size = offer
                .artifact
                .as_ref()
                .map_or(0, |artifact| artifact.size / (1024 * 1024));
            page = page.push(
                label(t!(
                    "update.page.offer",
                    version = offer.version,
                    date = offer.released_at.format("%Y-%m-%d"),
                    size = size
                ))
                .font(theme::UI_FONT_STRONG),
            );
            if !offer.notes.is_empty() {
                page = page.push(prose(offer.notes.clone()));
            }
        }
        None => page = page.push(hint(t!("update.page.up_to_date").to_owned())),
    }
    page = page.push(hint(download_line(&status.download)));
    if status.offer.is_some() && status.install_kind == "portable" {
        page = page.push(hint(t!("update.page.install_ends").to_owned()));
    }
    page.push(buttons(status)).into()
}

/// "Update it with Scoop: scoop update rcommander" for a manager's installation.
pub(super) fn manager_line(status: &UpdateStatus) -> Option<String> {
    let command = status.install_command.as_deref()?;
    let manager = rc_update::InstallKind::parse(&status.install_kind)
        .and_then(rc_update::InstallKind::manager_name)
        .unwrap_or(status.install_kind.as_str());
    Some(t!(
        "update.page.manager_command",
        manager = manager,
        command = command
    ))
}

fn last_check(status: &UpdateStatus) -> String {
    match (&status.last_check, &status.last_code) {
        (None, _) => t!("update.page.never_checked").to_owned(),
        (Some(at), None) => t!(
            "update.page.checked",
            time = at.format("%Y-%m-%d %H:%M UTC")
        ),
        (Some(at), Some(code)) => t!(
            "update.page.check_failed",
            time = at.format("%Y-%m-%d %H:%M UTC"),
            reason = error_text(code)
        ),
    }
}

fn download_line(download: &DownloadState) -> String {
    match download {
        DownloadState::None => String::new(),
        DownloadState::Running { received, total } => t!(
            "update.page.downloading",
            received = received / 1024,
            total = total / 1024
        ),
        DownloadState::Ready { .. } => t!("update.page.downloaded").to_owned(),
        DownloadState::Failed { code } => {
            t!("update.page.download_failed", reason = error_text(code))
        }
    }
}

/// The page's buttons, each a stop of the focus ring (`layout`'s helpers); one that cannot act
/// now is drawn disabled.
fn buttons<'a>(status: &UpdateStatus) -> Element<'a, Message> {
    let artifact = status
        .offer
        .as_ref()
        .is_some_and(|offer| offer.artifact.is_some());
    let ready = matches!(status.download, DownloadState::Ready { .. });
    let running = matches!(status.download, DownloadState::Running { .. });
    let portable = status.install_kind == "portable";
    let when = |on: bool, message: UpdateMsg| on.then(|| wrap(message));
    let install = if status.install_command.is_some() {
        layout::primary_button(
            t!("update.page.copy_command"),
            Some(wrap(UpdateMsg::CopyCommand)),
        )
    } else if status.queue_busy {
        layout::primary_button(t!("update.page.install_when_empty"), None)
    } else {
        layout::primary_button(
            t!("update.page.install"),
            when(ready && portable && !status.installing, UpdateMsg::Install),
        )
    };
    let row = row![
        layout::secondary_button(t!("update.page.check_now"), Some(wrap(UpdateMsg::CheckNow))),
        layout::secondary_button(
            t!("update.page.download"),
            when(artifact && !ready && !running, UpdateMsg::Download)
        ),
        layout::secondary_button(
            t!("update.page.open_folder"),
            when(ready, UpdateMsg::OpenFolder)
        ),
        layout::secondary_button(
            t!("update.page.copy_address"),
            when(artifact, UpdateMsg::CopyAddress)
        ),
        install,
    ]
    .spacing(8);
    column![row].into()
}

/// Starts the engine's self-update once the window runs, then looks every half minute.
pub(super) fn subscription(engine: &Engine) -> Subscription<Message> {
    Subscription::run_with(Updates(engine.clone()), updates)
}

/// The engine whose self-update the window runs; one per window.
struct Updates(Engine);

impl Hash for Updates {
    fn hash<H: Hasher>(&self, state: &mut H) {
        "rcommander.updates".hash(state);
    }
}

fn updates(updates: &Updates) -> impl Stream<Item = Message> + use<> {
    stream::unfold((updates.0.clone(), false), |(engine, started)| async move {
        if !started {
            if let Ok(executable) = std::env::current_exe() {
                engine.start_updates(&executable).await;
            }
            return Some((wrap(UpdateMsg::Started), (engine, true)));
        }
        tokio::time::sleep(IDLE_LOOK).await;
        Some((wrap(UpdateMsg::Tick), (engine, true)))
    })
}

#[cfg(test)]
#[path = "update_ui_tests.rs"]
mod tests;
