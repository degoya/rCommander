use rc_engine::exchange::{ExchangePart, ExchangeRow, RowStatus};

use super::*;

fn form() -> ExchangeForm {
    let mut form = ExchangeForm::default();
    form.update(ExchangeMsg::Password("export pw".into()));
    form.update(ExchangeMsg::Repeat("export pw".into()));
    form
}

fn exported(commands: &[Command]) -> &ConfigExportRequest {
    match commands.last() {
        Some(Command::ExportConfig(request)) => request,
        other => panic!("an export, got {other:?}"),
    }
}

fn row(key: &str, status: RowStatus, selected: bool) -> ExchangeRow {
    ExchangeRow {
        key: key.to_owned(),
        part: ExchangePart::Settings,
        name: key.to_owned(),
        old: None,
        new: None,
        warning: None,
        secrets: false,
        selected,
        status,
    }
}

#[test]
fn an_export_starts_with_every_part_encrypted() {
    let form = ExchangeForm::default();
    assert_eq!(form.parts, ExportParts::ALL);
    assert!(form.encrypted && !form.secrets);
}

#[test]
fn an_export_needs_a_part_and_a_password_typed_twice() {
    let mut form = form();
    let commands = form.export("x.rcexport".into(), false).expect("runs");
    let request = exported(&commands);
    assert!(request.password.is_some());
    assert_eq!(request.parts, ExportParts::ALL);

    form.update(ExchangeMsg::Repeat("other".into()));
    assert_eq!(
        form.export("x".into(), false).err(),
        Some(t!("exchange.problem.password_mismatch"))
    );
    form.update(ExchangeMsg::Password("".into()));
    assert_eq!(
        form.export("x".into(), false).err(),
        Some(t!("exchange.problem.password_empty"))
    );
    for part in Part::ALL {
        form.update(ExchangeMsg::Part(part, false));
    }
    assert_eq!(
        form.export("x".into(), false).err(),
        Some(t!("exchange.problem.no_part"))
    );
}

#[test]
fn secrets_in_clear_wait_for_the_confirmed_warning() {
    let mut form = form();
    form.update(ExchangeMsg::Encrypted(false));
    form.update(ExchangeMsg::Secrets(true));
    assert_eq!(
        form.export("x".into(), false).err(),
        Some(t!("exchange.problem.plain_unconfirmed"))
    );
    form.update(ExchangeMsg::PlainConfirmed(true));
    let commands = form.export("x".into(), false).expect("runs");
    let request = exported(&commands);
    assert!(request.password.is_none() && request.secrets && request.plain_secrets);
    // Another choice of encryption asks again.
    form.update(ExchangeMsg::Encrypted(true));
    form.update(ExchangeMsg::Encrypted(false));
    assert!(form.export("x".into(), false).is_err());
    // Without the connections no secret goes, and nothing is to confirm.
    form.update(ExchangeMsg::Part(Part::Connections, false));
    let commands = form.export("x".into(), false).expect("runs");
    assert!(!exported(&commands).secrets);
}

#[test]
fn the_vaults_answer_runs_first() {
    let mut form = form();
    form.vault = Some(Ask::Unlock);
    assert_eq!(
        form.export("x".into(), false).err(),
        Some(t!("exchange.problem.master_empty"))
    );
    form.update(ExchangeMsg::Master("master pw".into()));
    let commands = form.export("x".into(), false).expect("runs");
    assert!(matches!(commands[0], Command::UnlockVault { .. }));
    assert_eq!(commands.len(), 2);

    form.vault = Some(Ask::Create);
    assert!(form.import("x".into(), false).is_err(), "typed once");
    form.update(ExchangeMsg::MasterRepeat("master pw".into()));
    let commands = form.import("x".into(), false).expect("runs");
    assert!(matches!(commands[0], Command::CreateVault { .. }));
    // A preview asks nothing of the vault.
    assert_eq!(form.import("x".into(), true).expect("runs").len(), 1);
}

#[test]
fn an_import_names_the_rows_ticked() {
    let mut form = ExchangeForm::default();
    form.previewed(rc_engine::exchange::ExchangeReport {
        dry_run: true,
        encrypted: false,
        created_at: String::new(),
        secrets: false,
        parts: ExportParts::ALL,
        rows: vec![
            row("setting:a", RowStatus::Changed, true),
            row("setting:b", RowStatus::Changed, false),
            row("setting:c", RowStatus::Exists, false),
        ],
    });
    assert_eq!(form.chosen.iter().collect::<Vec<_>>(), ["setting:a"]);
    let Command::ImportConfig(preview) = &form.import("x".into(), true).expect("runs")[0] else {
        panic!("an import");
    };
    assert!(preview.dry_run && preview.only.is_none() && preview.password.is_none());
    form.update(ExchangeMsg::ChooseAll(true));
    assert_eq!(
        form.chosen.len(),
        2,
        "the row already there is not choosable"
    );
    form.update(ExchangeMsg::Choose("setting:a".into(), false));
    let Command::ImportConfig(run) = &form.import("x".into(), false).expect("runs")[0] else {
        panic!("an import");
    };
    assert_eq!(run.only.as_deref(), Some(&["setting:b".to_owned()][..]));
    form.update(ExchangeMsg::ChooseAll(false));
    assert!(form.chosen.is_empty());
}

#[test]
fn a_typed_password_never_shows() {
    let mut form = form();
    form.update(ExchangeMsg::Master("master-canary".into()));
    let shown = format!("{form:?}");
    assert!(!shown.contains("export pw") && !shown.contains("master-canary"));
    // Nor in the messages that carry it (RC-080-39).
    for message in [
        ExchangeMsg::Password("export-canary".into()),
        ExchangeMsg::Repeat("export-canary".into()),
        ExchangeMsg::Master("master-canary".into()),
        ExchangeMsg::MasterRepeat("master-canary".into()),
    ] {
        let shown = format!("{message:?}");
        assert!(!shown.contains("canary"), "{shown}");
    }
}
