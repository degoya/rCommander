//! Drawing the export and the import (RC-080-19): the export's parts, its encryption and its
//! secrets with their warning; the import's export password and its table of rows - a box, what
//! the row is, here and in the file, its status and warning - and the vault's question when one
//! is asked. Every box is a stop of the focus ring; a password field never shows what is typed.

use iced::{
    Element, Length,
    widget::{Column, checkbox, scrollable, text_input},
};
use rc_engine::exchange::{ExchangePart, ExchangeReport, ExchangeRow, RowStatus};
use rc_vault::{ExposeSecret as _, SecretString};

use super::{DatabaseDialog, DatabaseMsg, ExchangeMsg, Part, Phase};
use crate::{
    app::{Message, vault_ask::Ask},
    i18n::{error_text, t},
    theme::{
        self, Tokens,
        fit::{cell, line, prose, table_row},
        focus, layout,
        mnemonic::Label,
    },
};

fn msg(message: ExchangeMsg) -> Message {
    Message::Database(DatabaseMsg::Exchange(message))
}

/// A box of the dialog: a stop of the focus ring, Space and its access key toggle it.
fn tick<'a>(
    label: &'static str,
    on: bool,
    live: bool,
    toggle: fn(bool) -> ExchangeMsg,
) -> Element<'a, Message> {
    layout::check(label, on, live, move |on| msg(toggle(on)))
}

/// A field whose text is never shown.
fn secret<'a>(
    placeholder: &'static str,
    value: &'a str,
    live: bool,
    typed: fn(SecretString) -> ExchangeMsg,
) -> Element<'a, Message> {
    let mut field = text_input(placeholder, value)
        .secure(true)
        .font(theme::UI_FONT)
        .size(theme::TEXT_SIZE)
        .padding([6, 8]);
    if live {
        field = field.on_input(move |text| msg(typed(SecretString::from(text))));
    }
    field.into()
}

fn part_label(part: Part) -> &'static str {
    match part {
        Part::Settings => t!("exchange.part.settings"),
        Part::Connections => t!("exchange.part.connections"),
        Part::Trust => t!("exchange.part.trust"),
        Part::Hotlist => t!("exchange.part.hotlist"),
    }
}

/// The export's fields under the path.
pub fn export<'a>(
    mut body: Column<'a, Message>,
    dialog: &'a DatabaseDialog,
    tokens: &'static Tokens,
) -> Column<'a, Message> {
    let form = &dialog.exchange;
    let live = dialog.phase != Phase::Running;
    body = body.push(line(t!("exchange.parts")).color(tokens.text_secondary));
    for part in Part::ALL {
        let toggle: fn(bool) -> ExchangeMsg = match part {
            Part::Settings => |on| ExchangeMsg::Part(Part::Settings, on),
            Part::Connections => |on| ExchangeMsg::Part(Part::Connections, on),
            Part::Trust => |on| ExchangeMsg::Part(Part::Trust, on),
            Part::Hotlist => |on| ExchangeMsg::Part(Part::Hotlist, on),
        };
        body = body.push(tick(part_label(part), part.of(form.parts), live, toggle));
    }
    body = body.push(tick(
        t!("exchange.encrypt"),
        form.encrypted,
        live,
        ExchangeMsg::Encrypted,
    ));
    if form.encrypted {
        body = body
            .push(secret(
                t!("exchange.password"),
                form.password.expose_secret(),
                live,
                ExchangeMsg::Password,
            ))
            .push(secret(
                t!("exchange.repeat"),
                form.repeat.expose_secret(),
                live,
                ExchangeMsg::Repeat,
            ))
            .push(prose(t!("exchange.password_hint")).color(tokens.text_secondary));
    }
    let secrets_live = live && form.parts.connections;
    body = body.push(tick(
        t!("exchange.secrets"),
        form.secrets && form.parts.connections,
        secrets_live,
        ExchangeMsg::Secrets,
    ));
    if form.secrets && form.parts.connections && !form.encrypted {
        body = body
            .push(prose(t!("exchange.plain_warning")).color(theme::DANGER))
            .push(tick(
                t!("exchange.plain_confirm"),
                form.plain_confirmed,
                live,
                ExchangeMsg::PlainConfirmed,
            ));
    }
    body = body.push(prose(t!("exchange.export_note")).color(tokens.text_secondary));
    vault(body, dialog, tokens)
}

/// The import's fields under the path, and its table once the file was read.
pub fn import<'a>(
    mut body: Column<'a, Message>,
    dialog: &'a DatabaseDialog,
    tokens: &'static Tokens,
) -> Column<'a, Message> {
    let form = &dialog.exchange;
    let live = dialog.phase != Phase::Running;
    body = body.push(secret(
        t!("exchange.import_password"),
        form.password.expose_secret(),
        live,
        ExchangeMsg::Password,
    ));
    body = vault(body, dialog, tokens);
    match &form.report {
        Some(report) => {
            body = body.push(prose(summary(report)).color(tokens.text_secondary));
            body.push(scrollable(table(dialog, report, tokens)).height(Length::Fill))
        }
        None => body.push(prose(t!("exchange.import_note")).color(tokens.text_secondary)),
    }
}

/// What the file is: when it was written, whether encrypted, whether it holds secrets.
fn summary(report: &ExchangeReport) -> String {
    let sealed = if report.encrypted {
        t!("exchange.file_encrypted")
    } else {
        t!("exchange.file_plain")
    };
    let secrets = if report.secrets {
        t!("exchange.file_secrets")
    } else {
        t!("exchange.file_no_secrets")
    };
    format!("{} | {sealed} | {secrets}", report.created_at)
}

/// The rows by part, each with its box where it can be chosen.
fn table<'a>(
    dialog: &'a DatabaseDialog,
    report: &'a ExchangeReport,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let live = dialog.phase == Phase::Editing && !report.rows.is_empty() && report.dry_run;
    let mut rows = Column::new().spacing(4);
    let mut part = None;
    for (at, row) in report.rows.iter().enumerate() {
        if part != Some(row.part) {
            part = Some(row.part);
            // The export's checkbox labels, without their access keys.
            let title = Label::parse(part_title(row.part)).text;
            rows = rows.push(line(title).font(theme::UI_FONT_STRONG));
        }
        rows = rows.push(table_row(
            theme::alternate(at),
            tokens,
            vec![
                pick(dialog, row, live),
                cell(row.name.clone(), 4, false, tokens),
                cell(change(row), 5, true, tokens),
                cell(status(row), 4, row.warning.is_none(), tokens),
            ],
        ));
    }
    if report.rows.is_empty() {
        rows = rows.push(line(t!("exchange.nothing")));
    }
    rows.into()
}

fn part_title(part: ExchangePart) -> &'static str {
    match part {
        ExchangePart::Settings => t!("exchange.part.settings"),
        ExchangePart::Connections => t!("exchange.part.connections"),
        ExchangePart::Trust => t!("exchange.part.trust"),
        ExchangePart::Hotlist => t!("exchange.part.hotlist"),
    }
}

/// The box of `row`, live while the preview is shown and the row can be chosen.
fn pick<'a>(dialog: &DatabaseDialog, row: &ExchangeRow, live: bool) -> Element<'a, Message> {
    let chosen = dialog.exchange.chosen.contains(&row.key);
    let live = live && row.status.choosable(row.secrets);
    let mut pick = checkbox(chosen).size(14);
    if live {
        let key = row.key.clone();
        pick = pick.on_toggle(move |on| msg(ExchangeMsg::Choose(key.clone(), on)));
    }
    let toggled = live.then(|| msg(ExchangeMsg::Choose(row.key.clone(), !chosen)));
    focus::toggle(pick, &row.key, toggled)
}

/// Here and in the file: `old -> new`, or the file's value alone.
fn change(row: &ExchangeRow) -> String {
    let new = row.new.clone().unwrap_or_default();
    let mut text = match &row.old {
        Some(old) => format!("{old} -> {new}"),
        None => new,
    };
    if row.secrets {
        text = format!("{text} ({})", t!("exchange.with_secrets"));
    }
    text
}

/// The row's status, its warning after it.
fn status(row: &ExchangeRow) -> String {
    let status = match &row.status {
        RowStatus::New => t!("exchange.status.new").to_owned(),
        RowStatus::Exists => t!("exchange.status.exists").to_owned(),
        RowStatus::Changed => t!("exchange.status.changed").to_owned(),
        RowStatus::Created => t!("exchange.status.created").to_owned(),
        RowStatus::Updated => t!("exchange.status.updated").to_owned(),
        RowStatus::Refused { code } => {
            format!("{}: {}", t!("exchange.status.refused"), error_text(code))
        }
        RowStatus::Failed { code } => {
            format!("{}: {}", t!("exchange.status.failed"), error_text(code))
        }
    };
    match row.warning.as_deref().map(warning) {
        Some(warning) => format!("{status} - {warning}"),
        None => status,
    }
}

fn warning(code: &str) -> &'static str {
    match code {
        "protected_setting" => t!("exchange.warning.protected_setting"),
        "host_key_changed" => t!("exchange.warning.host_key_changed"),
        "certificate_changed" => t!("exchange.warning.certificate_changed"),
        "endpoint_with_secrets" => t!("exchange.warning.endpoint_with_secrets"),
        "network_location" => t!("exchange.warning.network_location"),
        "secret_sources" => t!("exchange.warning.secret_sources"),
        _ => t!("exchange.warning.other"),
    }
}

/// The vault's question, while it is asked: its master password, a new one twice.
fn vault<'a>(
    mut body: Column<'a, Message>,
    dialog: &'a DatabaseDialog,
    tokens: &'static Tokens,
) -> Column<'a, Message> {
    let form = &dialog.exchange;
    let live = dialog.phase != Phase::Running;
    let Some(ask) = form.vault else {
        return body;
    };
    let (question, field) = match ask {
        Ask::Unlock => (t!("exchange.vault_unlock"), t!("exchange.master")),
        Ask::Create => (t!("exchange.vault_create"), t!("exchange.master_new")),
    };
    body = body.push(prose(question).color(tokens.text)).push(secret(
        field,
        form.master.expose_secret(),
        live,
        ExchangeMsg::Master,
    ));
    if ask == Ask::Create {
        body = body.push(secret(
            t!("exchange.master_repeat"),
            form.master_repeat.expose_secret(),
            live,
            ExchangeMsg::MasterRepeat,
        ));
    }
    body
}

/// The problem that keeps the form from running, in the danger colour.
pub fn problem<'a>(dialog: &DatabaseDialog) -> Option<Element<'a, Message>> {
    dialog
        .exchange
        .problem
        .map(|problem| prose(problem).color(theme::DANGER).into())
}

/// The import's extra buttons: every choosable row ticked, or none.
pub fn choose_all<'a>(dialog: &DatabaseDialog) -> Vec<Element<'a, Message>> {
    let live = dialog.phase == Phase::Editing
        && dialog
            .exchange
            .report
            .as_ref()
            .is_some_and(|report| report.dry_run);
    if !live {
        return Vec::new();
    }
    vec![
        layout::secondary_button(
            t!("exchange.choose_all"),
            Some(msg(ExchangeMsg::ChooseAll(true))),
        ),
        layout::secondary_button(
            t!("exchange.choose_none"),
            Some(msg(ExchangeMsg::ChooseAll(false))),
        ),
    ]
}

/// Whether the import's Import runs: a preview is shown and a row is ticked.
pub fn can_import(dialog: &DatabaseDialog) -> bool {
    dialog
        .exchange
        .report
        .as_ref()
        .is_some_and(|report| report.dry_run)
        && !dialog.exchange.chosen.is_empty()
}
