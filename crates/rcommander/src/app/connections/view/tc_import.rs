//! The page of the import from Total Commander: the two files, then a table with a box per
//! hotlist item, tab, connection, colour, rule by file type and font with its status, the warnings, and the choices for tabs and
//! passwords in the card's row of buttons - never a password, only whether Total Commander keeps
//! one. A cell is one line, cut at its column (a path or a host in the middle), its whole value
//! in a tooltip.

use iced::{
    Alignment, Element, Length,
    widget::{Column, Space, checkbox, column, row, scrollable},
};
use rc_engine::{ImportStatus, PanelId, import::total_commander_report::TcImportReport};

use super::{Actions, cell, field, form::kind_label, hint, label, path_cell, table_row, wrap};
use crate::{
    app::{
        Message,
        browse::{self, PathField},
        connections::{
            ConnMsg,
            import::{Phase, importable, status_code},
            tc_import::{
                TcImportDialog, TcImportMsg, password_code, row_importable, row_status_code,
            },
        },
    },
    i18n::{code_text, error_text, t},
    theme::{self, Tokens, fit, focus, icon::Icon, layout},
};

fn msg(message: TcImportMsg) -> Message {
    wrap(ConnMsg::TcImport(message))
}

pub(super) fn page<'a>(
    dialog: &'a TcImportDialog,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let source = row![
        field(t!("tc_import.wincmd_auto"), &dialog.wincmd)
            .on_input(|path| msg(TcImportMsg::Wincmd(path)))
            .on_submit(msg(TcImportMsg::Read)),
        browse::button(PathField::Wincmd, &dialog.wincmd, true),
        field(t!("tc_import.ftp_auto"), &dialog.ftp)
            .on_input(|path| msg(TcImportMsg::Ftp(path)))
            .on_submit(msg(TcImportMsg::Read)),
        browse::button(PathField::WcxFtp, &dialog.ftp, true),
        layout::secondary_button(t!("tc_import.read"), Some(msg(TcImportMsg::Read))),
    ]
    .spacing(6)
    .align_y(Alignment::Center);
    let mut body = column![hint(t!("tc_import.help"), tokens), source].spacing(10);
    if let Some(error) = &dialog.page.error {
        body = body.push(fit::prose(error.as_str()).color(theme::DANGER));
    }
    let list: Element<'a, Message> = match (&dialog.report, dialog.page.phase) {
        (_, Phase::Reading) => hint(t!("tc_import.reading"), tokens).into(),
        (Some(report), _) => report_view(dialog, report, tokens),
        (None, _) => Space::new().into(),
    };
    body.push(scrollable(list).height(Length::Fill))
        .push(hint(t!("tc_import.passwords_hint"), tokens))
        .height(Length::Fill)
        .into()
}

/// The choices for tabs and passwords on the left of the card's row of buttons, Back and Import
/// beside Close.
pub(super) fn actions<'a>(dialog: &TcImportDialog) -> Actions<'a> {
    let editable = dialog.page.phase == Phase::Ready;
    let replace = layout::check(
        t!("tc_import.replace_tabs"),
        dialog.replace_tabs,
        editable,
        |on| msg(TcImportMsg::ReplaceTabs(on)),
    );
    let passwords = layout::check(
        t!("tc_import.passwords"),
        dialog.passwords,
        editable,
        |on| msg(TcImportMsg::Passwords(on)),
    );
    let can_run = editable && !dialog.chosen.is_empty();
    Actions {
        left: vec![replace, passwords],
        // A wizard's Back stands directly left of its primary.
        right: vec![
            layout::secondary_button(t!("tc_import.back"), Some(msg(TcImportMsg::Back))),
            layout::primary_icon_button(
                Icon::Import,
                t!("tc_import.run"),
                can_run.then(|| msg(TcImportMsg::Run)),
            ),
        ],
    }
}

/// A box for the row `key`, live while the preview is ready and the row can be imported.
fn pick<'a>(dialog: &TcImportDialog, key: &str, live: bool) -> Element<'a, Message> {
    let chosen = dialog.chosen.contains(key);
    let mut pick = checkbox(chosen).size(14);
    let live = live && dialog.page.phase == Phase::Ready;
    if live {
        let key = key.to_owned();
        pick = pick.on_toggle(move |on| msg(TcImportMsg::Choose(key.clone(), on)));
    }
    let toggled = live.then(|| msg(TcImportMsg::Choose(key.to_owned(), !chosen)));
    focus::toggle(pick, key, toggled)
}

/// The four parts, then the warnings and what the vault took.
fn report_view<'a>(
    dialog: &'a TcImportDialog,
    report: &'a TcImportReport,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let mut rows = Column::new().spacing(4);
    let files = format!(
        "{} | {}",
        report.wincmd_ini.as_deref().unwrap_or("-"),
        report.ftp_ini.as_deref().unwrap_or("-")
    );
    rows = rows.push(hint(files, tokens));
    rows = rows.push(label(t!("tc_import.hotlist")).font(theme::UI_FONT_STRONG));
    for (at, item) in report.hotlist.iter().enumerate() {
        let status = match &item.status {
            rc_engine::import::total_commander_report::TcRowStatus::Skipped { reason } => {
                code_text("tc_import.skip", reason)
            }
            other => code_text("tc_import.status", row_status_code(other)),
        };
        let indent = "    ".repeat(item.folders.len());
        let name = if item.folder {
            format!("{indent}{}/", item.name)
        } else {
            format!("{indent}{}", item.name)
        };
        rows = rows.push(table_row(
            theme::alternate(at),
            tokens,
            vec![
                pick(dialog, &item.key, row_importable(&item.status)),
                cell(name, 3, false, tokens),
                path_cell(item.path.as_deref().unwrap_or_default(), 5, false, tokens),
                cell(status, 3, true, tokens),
            ],
        ));
    }
    rows = rows.push(label(t!("tc_import.tabs")).font(theme::UI_FONT_STRONG));
    for (at, tab) in report.tabs.iter().enumerate() {
        let status = match &tab.status {
            rc_engine::import::total_commander_report::TcRowStatus::Skipped { reason } => {
                code_text("tc_import.skip", reason)
            }
            other => code_text("tc_import.status", row_status_code(other)),
        };
        let side = match tab.panel {
            PanelId::Left => t!("tc_import.left"),
            PanelId::Right => t!("tc_import.right"),
        };
        let marks = match (tab.active, tab.locked) {
            (true, true) => format!("{} {}", t!("tc_import.active"), t!("tc_import.locked")),
            (true, false) => t!("tc_import.active").to_owned(),
            (false, true) => t!("tc_import.locked").to_owned(),
            (false, false) => String::new(),
        };
        rows = rows.push(table_row(
            theme::alternate(at),
            tokens,
            vec![
                pick(dialog, &tab.key, row_importable(&tab.status)),
                cell(side, 1, false, tokens),
                path_cell(tab.path.as_deref().unwrap_or_default(), 7, false, tokens),
                cell(marks, 2, true, tokens),
                cell(status, 3, true, tokens),
            ],
        ));
    }
    rows = rows.push(label(t!("tc_import.connections")).font(theme::UI_FONT_STRONG));
    for (at, connection) in report.connections.iter().enumerate() {
        let status = match &connection.status {
            ImportStatus::Refused { code } => format!(
                "{}: {}",
                code_text("ssh_import.status", status_code(&connection.status)),
                error_text(code)
            ),
            other => code_text("ssh_import.status", status_code(other)),
        };
        let mut name = connection.folders.join("/");
        if !name.is_empty() {
            name.push('/');
        }
        name.push_str(&connection.name);
        let kind = connection.kind.map(kind_label).unwrap_or_default();
        let password = code_text(
            "tc_import.password",
            password_code(connection.password_state),
        );
        rows = rows.push(table_row(
            theme::alternate(at),
            tokens,
            vec![
                pick(dialog, &connection.key, importable(&connection.status)),
                cell(name, 3, false, tokens),
                cell(kind, 1, false, tokens),
                path_cell(connection.host.as_str(), 3, false, tokens),
                cell(
                    connection.user.as_deref().unwrap_or_default(),
                    2,
                    false,
                    tokens,
                ),
                cell(password, 2, true, tokens),
                cell(status, 2, true, tokens),
            ],
        ));
    }
    rows = rows.push(label(t!("tc_import.look")).font(theme::UI_FONT_STRONG));
    for (at, look) in report.look.iter().enumerate() {
        let status = match &look.status {
            rc_engine::import::total_commander_report::TcRowStatus::Skipped { reason } => {
                code_text("tc_import.skip", reason)
            }
            other => code_text("tc_import.status", row_status_code(other)),
        };
        rows = rows.push(table_row(
            theme::alternate(at),
            tokens,
            vec![
                pick(dialog, &look.key, row_importable(&look.status)),
                cell(look.source.as_str(), 6, false, tokens),
                cell(look.value.as_str(), 3, false, tokens),
                cell(status, 3, true, tokens),
            ],
        ));
    }
    for warning in &report.warnings {
        rows = rows.push(hint(code_text("tc_import.warning", warning), tokens));
    }
    if let Some(stored) = &dialog.stored {
        for skip in &stored.skipped {
            rows = rows.push(hint(
                format!(
                    "{}: {}",
                    skip.key,
                    code_text("tc_import.password_skip", &skip.reason)
                ),
                tokens,
            ));
        }
    }
    rows.into()
}
