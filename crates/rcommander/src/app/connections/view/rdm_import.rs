//! The page of the import from Remote Desktop Manager: the export, then a table with a box per
//! entry - its folders and name, its kind (or RDM's type when rCommander has none for it), host,
//! user, port, whether RDM keeps a password, and its status. Never a password. A cell is one
//! line, cut at its column, its whole value in a tooltip.

use iced::{
    Alignment, Element, Length,
    widget::{Column, Space, checkbox, column, row, scrollable},
};
use rc_engine::{
    ImportStatus,
    import::{rdm::RdmPassword, rdm_report::RdmImportReport},
};

use super::{Actions, cell, field, form::kind_label, hint, label, path_cell, table_row, wrap};
use crate::{
    app::{
        Message,
        browse::{self, PathField},
        connections::{
            ConnMsg,
            import::{Phase, status_code},
            rdm_import::{RdmImportDialog, RdmImportMsg, row_importable},
        },
    },
    i18n::{code_text, error_text, t},
    theme::{self, Tokens, fit, focus, icon::Icon, layout},
};

fn msg(message: RdmImportMsg) -> Message {
    wrap(ConnMsg::RdmImport(message))
}

pub(super) fn page<'a>(
    dialog: &'a RdmImportDialog,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let source = row![
        field(t!("rdm_import.path"), &dialog.path)
            .on_input(|path| msg(RdmImportMsg::Path(path)))
            .on_submit(msg(RdmImportMsg::Read)),
        browse::button(PathField::RdmExport, &dialog.path, true),
        layout::secondary_button(t!("rdm_import.read"), Some(msg(RdmImportMsg::Read))),
    ]
    .spacing(6)
    .align_y(Alignment::Center);
    let mut body = column![
        label(t!("rdm_import.title")).font(theme::UI_FONT_STRONG),
        hint(t!("rdm_import.help"), tokens),
        source,
    ]
    .spacing(10);
    if let Some(error) = &dialog.page.error {
        body = body.push(fit::prose(error.as_str()).color(theme::DANGER));
    }
    let list: Element<'a, Message> = match (&dialog.report, dialog.page.phase) {
        (_, Phase::Reading) => hint(t!("rdm_import.reading"), tokens).into(),
        (Some(report), _) => report_view(dialog, report, tokens),
        (None, _) => Space::new().into(),
    };
    body.push(scrollable(list).height(Length::Fill))
        .push(hint(t!("rdm_import.passwords_hint"), tokens))
        .height(Length::Fill)
        .into()
}

/// Back and Import, beside Close in the card's row of buttons.
pub(super) fn actions<'a>(dialog: &RdmImportDialog) -> Actions<'a> {
    let can_run = dialog.page.phase == Phase::Ready && !dialog.chosen.is_empty();
    Actions {
        left: Vec::new(),
        right: vec![
            layout::secondary_button(t!("rdm_import.back"), Some(msg(RdmImportMsg::Back))),
            layout::primary_icon_button(
                Icon::Import,
                t!("rdm_import.run"),
                can_run.then(|| msg(RdmImportMsg::Run)),
            ),
        ],
    }
}

/// The entries with their boxes, then the warning.
fn report_view<'a>(
    dialog: &'a RdmImportDialog,
    report: &'a RdmImportReport,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let mut rows = Column::new().spacing(4);
    if report.entries.is_empty() {
        rows = rows.push(hint(t!("rdm_import.no_entries"), tokens));
    }
    for (at, entry) in report.entries.iter().enumerate() {
        let chosen = dialog.chosen.contains(&entry.key);
        let mut pick = checkbox(chosen).size(14);
        let can_choose = dialog.page.phase == Phase::Ready && row_importable(entry);
        if can_choose {
            let key = entry.key.clone();
            pick = pick.on_toggle(move |on| msg(RdmImportMsg::Choose(key.clone(), on)));
        }
        let pick = focus::toggle(
            pick,
            &entry.name,
            can_choose.then(|| msg(RdmImportMsg::Choose(entry.key.clone(), !chosen))),
        );
        let mut status = match &entry.status {
            ImportStatus::Refused { code } => format!(
                "{}: {}",
                code_text("ssh_import.status", status_code(&entry.status)),
                error_text(code)
            ),
            other => code_text("ssh_import.status", status_code(other)),
        };
        if entry.key_path_refused {
            status = format!("{status}; {}", t!("rdm_import.key_path_refused"));
        }
        let mut name = entry.folders.join("/");
        if !name.is_empty() {
            name.push('/');
        }
        name.push_str(&entry.name);
        let kind = entry.kind.map_or_else(
            || entry.rdm_type.clone(),
            |kind| kind_label(kind).to_owned(),
        );
        let password = match entry.password_state {
            RdmPassword::Obfuscated => t!("rdm_import.password_not_taken"),
            RdmPassword::None => "",
        };
        let port = entry.port.map(|port| port.to_string()).unwrap_or_default();
        rows = rows.push(table_row(
            theme::alternate(at),
            tokens,
            vec![
                pick,
                cell(name, 3, false, tokens),
                cell(kind, 2, false, tokens),
                path_cell(entry.host.as_str(), 3, false, tokens),
                cell(entry.user.as_deref().unwrap_or_default(), 2, false, tokens),
                cell(port, 1, false, tokens),
                cell(password, 2, true, tokens),
                cell(status, 3, true, tokens),
            ],
        ));
    }
    if report
        .warnings
        .iter()
        .any(|code| code == "too_many_entries")
    {
        rows = rows.push(hint(t!("rdm_import.too_many_entries"), tokens));
    }
    rows.into()
}
