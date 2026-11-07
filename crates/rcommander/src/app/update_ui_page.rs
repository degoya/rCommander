//! The Updates page of the settings (RC-100-05 ... RC-100-07, RC-100-84): what runs, the last
//! check, the offer with its notes and the buttons. A portable installation has one button for
//! download and install, "Install and restart"; a package manager's copies its command; an
//! unknown one only downloads.

use iced::{
    Element,
    widget::{Column, Row, column},
};
use rc_engine::{DownloadState, UpdateStatus};

use super::{UpdateMsg, wrap};
use crate::app::Message;
use crate::{
    i18n::{error_text, t},
    theme::{
        self, Tokens,
        fit::{label, prose},
        layout,
    },
};

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
    if status.install_after_download {
        page = page.push(hint(t!("update.page.install_after_download").to_owned()));
    }
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

/// What a button of the Updates page does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PageAction {
    CheckNow,
    Download,
    OpenFolder,
    CopyAddress,
    CopyCommand,
    Install,
    /// "Install once the queue is empty": never pressable.
    InstallWhenEmpty,
}

/// One button of the Updates page: what it does, whether it can now, and whether it is the
/// page's main button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PageButton {
    pub(super) action: PageAction,
    pub(super) enabled: bool,
    pub(super) primary: bool,
}

/// The page's buttons in their order (RC-100-84): a portable installation has one button for
/// download and install, "Install and restart"; a package manager's keeps "Download" and copies
/// its command; an unknown one only downloads.
pub(super) fn page_buttons(status: &UpdateStatus) -> Vec<PageButton> {
    let artifact = status
        .offer
        .as_ref()
        .is_some_and(|offer| offer.artifact.is_some());
    let ready = matches!(status.download, DownloadState::Ready { .. });
    let running = matches!(status.download, DownloadState::Running { .. });
    let portable = status.install_kind == "portable";
    let button = |action, enabled, primary| PageButton {
        action,
        enabled,
        primary,
    };
    let download = artifact && !ready && !running;
    let mut buttons = vec![button(PageAction::CheckNow, true, false)];
    if status.install_command.is_some() {
        buttons.push(button(PageAction::Download, download, false));
    }
    buttons.push(button(PageAction::OpenFolder, ready, false));
    buttons.push(button(PageAction::CopyAddress, artifact, false));
    buttons.push(if status.install_command.is_some() {
        button(PageAction::CopyCommand, true, true)
    } else if !portable {
        button(PageAction::Download, download, true)
    } else if status.queue_busy {
        button(PageAction::InstallWhenEmpty, false, true)
    } else {
        let waiting = running || status.installing || status.install_after_download;
        button(PageAction::Install, artifact && !waiting, true)
    });
    buttons
}

/// The page's buttons, each a stop of the focus ring (`layout`'s helpers); one that cannot act
/// now is drawn disabled.
fn buttons<'a>(status: &UpdateStatus) -> Element<'a, Message> {
    let row = page_buttons(status)
        .into_iter()
        .fold(Row::new().spacing(8), |row, shown| {
            let (caption, message) = match shown.action {
                PageAction::CheckNow => (t!("update.page.check_now"), UpdateMsg::CheckNow),
                PageAction::Download => (t!("update.page.download"), UpdateMsg::Download),
                PageAction::OpenFolder => (t!("update.page.open_folder"), UpdateMsg::OpenFolder),
                PageAction::CopyAddress => (t!("update.page.copy_address"), UpdateMsg::CopyAddress),
                PageAction::CopyCommand => (t!("update.page.copy_command"), UpdateMsg::CopyCommand),
                PageAction::Install => (t!("update.page.install"), UpdateMsg::Install),
                PageAction::InstallWhenEmpty => {
                    (t!("update.page.install_when_empty"), UpdateMsg::Install)
                }
            };
            let message = shown.enabled.then(|| wrap(message));
            row.push(if shown.primary {
                layout::primary_button(caption, message)
            } else {
                layout::secondary_button(caption, message)
            })
        });
    column![row].into()
}
