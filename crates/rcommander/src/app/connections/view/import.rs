//! The import page: the source, the target folder, a table row per host with its box, its key
//! and its status - under it, once its key was tried, what became of the key, and for an
//! encrypted one a passphrase field - then the names that are no host and the warnings, never a
//! value of the file beyond what the engine's report names. Back and Import sit in the card's
//! row of buttons.

use std::fmt;

use iced::{
    Alignment, Element, Length,
    widget::{Column, Space, button, checkbox, column, container, pick_list, row, scrollable},
};
use rc_engine::{FolderId, FolderNode, ImportStatus, SshConfigReport};
use rc_vault::{ExposeSecret as _, SecretString};

use super::{Actions, cell, field, hint, label, path_cell, table_row, wrap};
use crate::{
    app::{
        Message,
        browse::{self, PathField},
        connections::{
            ConnMsg, ConnectionManager,
            import::{ImportDialog, ImportMsg, KeyRow, Phase, importable, status_code},
        },
    },
    i18n::{code_text, error_text, t},
    theme::{self, Tokens, fit, focus, icon::Icon, layout},
};

fn msg(message: ImportMsg) -> Message {
    wrap(ConnMsg::Import(message))
}

pub(super) fn page<'a>(
    manager: &'a ConnectionManager,
    dialog: &'a ImportDialog,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let mut source = row![
        field(t!("connections.import.own_config"), &dialog.path)
            .on_input(|path| msg(ImportMsg::Path(path)))
            .on_submit(msg(ImportMsg::Read)),
        browse::button(PathField::SshConfig, &dialog.path, true),
        layout::secondary_button(t!("connections.import.read"), Some(msg(ImportMsg::Read))),
    ]
    .spacing(6)
    .align_y(Alignment::Center);
    for (at, (name, _)) in dialog.wsl.iter().enumerate() {
        let face = button(label(name.clone()))
            .padding([5, 10])
            .style(button::secondary)
            .on_press(msg(ImportMsg::Wsl(at)));
        source = source.push(focus::button(face, name, Some(msg(ImportMsg::Wsl(at)))));
    }
    let folders = folder_choices(manager);
    let selected = folders
        .iter()
        .find(|choice| choice.0 == dialog.folder)
        .cloned();
    let choose = |choice: FolderChoice| msg(ImportMsg::Folder(choice.0));
    let moves = focus::moves(&folders, selected.as_ref(), &choose);
    let list = pick_list(folders, selected, choose)
        .text_size(theme::TEXT_SIZE)
        .width(320);
    let target_label = t!("connections.import.target");
    let target = row![
        container(layout::caption(target_label, None)).width(150),
        layout::keyed(focus::Stop::pick(list, target_label, moves), target_label),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    let mut body = column![
        label(t!("connections.import.title")).font(theme::UI_FONT_STRONG),
        hint(t!("connections.import.help"), tokens),
        source,
        target,
    ]
    .spacing(10);
    if let Some(error) = &dialog.page.error {
        body = body.push(fit::prose(error.as_str()).color(theme::DANGER));
    }
    let list: Element<'a, Message> = match (&dialog.report, dialog.page.phase) {
        (_, Phase::Reading) => hint(t!("connections.import.reading"), tokens).into(),
        (Some(report), _) => report_view(dialog, report, tokens),
        (None, _) => Space::new().into(),
    };
    body.push(scrollable(list).height(Length::Fill))
        .height(Length::Fill)
        .into()
}

/// Back and Import, beside Close in the card's row of buttons.
pub(super) fn actions<'a>(dialog: &ImportDialog) -> Actions<'a> {
    let can_run = dialog.page.phase == Phase::Ready && dialog.chosen.iter().any(|on| *on);
    Actions {
        left: Vec::new(),
        right: vec![
            layout::secondary_button(t!("connections.import.back"), Some(msg(ImportMsg::Back))),
            layout::primary_icon_button(
                Icon::Import,
                t!("connections.import.run"),
                can_run.then(|| msg(ImportMsg::Run)),
            ),
        ],
    }
}

/// The hosts with their boxes, then what was skipped and warned about.
fn report_view<'a>(
    dialog: &'a ImportDialog,
    report: &'a SshConfigReport,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let editable = dialog.page.phase == Phase::Ready;
    let mut rows = Column::new().spacing(4);
    if report.hosts.is_empty() {
        rows = rows.push(hint(t!("connections.import.no_hosts"), tokens));
    }
    for (at, host) in report.hosts.iter().enumerate() {
        let chosen = dialog.chosen.get(at).copied().unwrap_or(false);
        let mut pick = checkbox(chosen).size(14);
        let can_choose = editable && importable(&host.status);
        if can_choose {
            pick = pick.on_toggle(move |on| msg(ImportMsg::Choose(at, on)));
        }
        let pick = focus::toggle(
            pick,
            &host.alias,
            can_choose.then(|| msg(ImportMsg::Choose(at, !chosen))),
        );
        let key = host.identity_files.first().cloned().unwrap_or_default();
        let taken = dialog.take_key.get(at).copied().unwrap_or(false);
        let mut take = checkbox(taken)
            .label(t!("connections.import.take_key"))
            .size(14)
            .text_size(12)
            .text_wrapping(iced::widget::text::Wrapping::None);
        // Only a host with a connection, or one about to get it, has a key to take.
        let can_take = editable && !key.is_empty() && (chosen || dialog.has_connection(at));
        if can_take {
            take = take.on_toggle(move |on| msg(ImportMsg::TakeKey(at, on)));
        }
        let take = focus::toggle(
            take,
            t!("connections.import.take_key"),
            can_take.then(|| msg(ImportMsg::TakeKey(at, !taken))),
        );
        let status = match &host.status {
            ImportStatus::Refused { code } => format!(
                "{}: {}",
                code_text("ssh_import.status", status_code(&host.status)),
                error_text(code)
            ),
            other => code_text("ssh_import.status", status_code(other)),
        };
        let port = host.port.map(|port| port.to_string()).unwrap_or_default();
        rows = rows.push(table_row(
            theme::alternate(at),
            tokens,
            vec![
                pick,
                cell(host.alias.as_str(), 3, false, tokens),
                path_cell(host.host.as_str(), 4, false, tokens),
                cell(host.user.as_deref().unwrap_or_default(), 2, false, tokens),
                cell(port, 1, false, tokens),
                cell(key, 3, true, tokens),
                take,
                cell(status, 3, true, tokens),
            ],
        ));
        if let Some(row) = dialog.key_row(at) {
            rows = rows.push(key_line(at, row, tokens));
        }
    }
    for skipped in &report.skipped {
        rows = rows.push(hint(
            format!(
                "{} ({}:{}): {}",
                skipped.name,
                skipped.file,
                skipped.line,
                code_text("ssh_import.skip", &skipped.reason)
            ),
            tokens,
        ));
    }
    for warning in &report.warnings {
        rows = rows.push(hint(
            format!(
                "{}:{} {}: {}",
                warning.file,
                warning.line,
                warning.keyword.as_deref().unwrap_or_default(),
                code_text("ssh_import.warning", &warning.code)
            ),
            tokens,
        ));
    }
    rows.into()
}

/// Under a host's row, what became of its key: taken, or why not - and for an encrypted key
/// its passphrase, masked, and the button that takes the key again with it.
fn key_line<'a>(at: usize, row: &'a KeyRow, tokens: &'static Tokens) -> Element<'a, Message> {
    let said: Element<'a, Message> = match row.result {
        Ok(()) => hint(
            t!("connections.import.key_taken", host = row.alias.as_str()),
            tokens,
        )
        .into(),
        Err(code) => fit::prose(error_text(code)).color(theme::DANGER).into(),
    };
    let mut line = row![said].spacing(8).align_y(Alignment::Center);
    if row.wants_passphrase() {
        let typed = row.passphrase.expose_secret();
        let again = (!typed.is_empty()).then(|| msg(ImportMsg::TakeAgain(at)));
        line = line
            .push(
                field(t!("connections.import.passphrase"), typed)
                    .secure(true)
                    .width(220)
                    .on_input(move |value| {
                        msg(ImportMsg::Passphrase(at, SecretString::from(value)))
                    })
                    .on_submit_maybe(again.clone()),
            )
            .push(layout::secondary_button(
                t!("connections.import.take_again"),
                again,
            ));
    }
    container(line).padding([2, 28]).into()
}

/// A target folder in the list: the default first, then every folder of the tree.
#[derive(Debug, Clone, PartialEq)]
struct FolderChoice(Option<FolderId>, String);

impl fmt::Display for FolderChoice {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.1)
    }
}

fn folder_choices(manager: &ConnectionManager) -> Vec<FolderChoice> {
    fn walk(nodes: &[FolderNode], depth: usize, into: &mut Vec<FolderChoice>) {
        for node in nodes {
            let name = format!("{}{}", "  ".repeat(depth), node.folder.name);
            into.push(FolderChoice(Some(node.folder.id), name));
            walk(&node.folders, depth + 1, into);
        }
    }
    let mut choices = vec![FolderChoice(
        None,
        t!("connections.import.default_folder").to_owned(),
    )];
    walk(&manager.tree.tree.folders, 0, &mut choices);
    choices
}
