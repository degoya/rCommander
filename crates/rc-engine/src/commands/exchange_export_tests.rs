//! `config.export` (RC-080-19): every combination of parts, unencrypted and sealed; no secret in
//! clear in a sealed file (the canary test), none at all in an unencrypted one unless the
//! person confirmed it; the vault asked only when secrets go with the file.

use serde_json::Value;

use crate::{
    Command, EngineError, Outcome,
    commands::exchange_fixture::{
        CANARY_PASSWORD, CANARY_SCRIPT, EXPORT_PASSWORD, MASTER, export, import, report, seeded,
    },
    exchange::{ExchangePart, ExportParts},
    settings::{EXPORT_CONNECTIONS_KEY, EXPORT_ENCRYPTED_KEY, stored_setting},
};

fn parts(bits: u8) -> ExportParts {
    ExportParts {
        settings: bits & 1 != 0,
        connections: bits & 2 != 0,
        trust: bits & 4 != 0,
        hotlist: bits & 8 != 0,
    }
}

fn written(outcome: Result<Outcome, EngineError>) -> crate::exchange::ExportReport {
    match outcome {
        Ok(Outcome::ConfigExported(report)) => report,
        other => panic!("an export report, got {other:?}"),
    }
}

#[tokio::test]
async fn a_sealed_export_holds_no_secret_in_clear() {
    let dir = tempfile::tempdir().expect("dir");
    let (engine, _) = seeded(dir.path()).await;
    let path = dir.path().join("all.rcexport");
    let command = export(&path, ExportParts::ALL, Some(EXPORT_PASSWORD), true);
    let report = written(engine.dispatch(command).await);
    assert!(report.encrypted && report.secrets);
    assert_eq!((report.connections, report.login_scripts), (2, 1));
    assert_eq!((report.host_keys, report.certificates), (1, 1));
    assert_eq!(report.hotlist, 2);
    let text = std::fs::read_to_string(&path).expect("written");
    for canary in [CANARY_PASSWORD, CANARY_SCRIPT, "web.example.org", "deploy"] {
        assert!(!text.contains(canary), "{canary} in the sealed file");
    }
    let file: Value = serde_json::from_str(&text).expect("json");
    assert_eq!(file["format"], "rcommander-export");
    assert!(file.get("content").is_none());
    assert_eq!(file["sealed"]["kdf"], "argon2id");
}

#[tokio::test]
async fn an_unencrypted_export_holds_secrets_only_after_the_warning() {
    let dir = tempfile::tempdir().expect("dir");
    let (engine, _) = seeded(dir.path()).await;
    let path = dir.path().join("plain.rcexport");
    written(
        engine
            .dispatch(export(&path, ExportParts::ALL, None, false))
            .await,
    );
    let text = std::fs::read_to_string(&path).expect("written");
    assert!(!text.contains(CANARY_PASSWORD) && !text.contains(CANARY_SCRIPT));
    let file: Value = serde_json::from_str(&text).expect("json");
    assert_eq!(file["content"]["secrets"], false);
    assert_eq!(
        file["content"]["connections"]["login_scripts"][0]["secrets"][0]["label"],
        "root"
    );
    let unconfirmed = engine
        .dispatch(export(&path, ExportParts::ALL, None, true))
        .await
        .expect_err("no warning confirmed");
    assert_eq!(unconfirmed.code(), "exchange_secrets_unencrypted");
    let Command::ExportConfig(mut request) = export(&path, ExportParts::ALL, None, true) else {
        unreachable!("an export");
    };
    request.plain_secrets = true;
    written(engine.dispatch(Command::ExportConfig(request)).await);
    let text = std::fs::read_to_string(&path).expect("written");
    assert!(text.contains(CANARY_PASSWORD) && text.contains(CANARY_SCRIPT));
}

#[tokio::test]
async fn every_combination_of_parts_reads_back_both_ways() {
    let dir = tempfile::tempdir().expect("dir");
    let (engine, _) = seeded(dir.path()).await;
    for bits in 1..16 {
        let chosen = parts(bits);
        for password in [None, Some(EXPORT_PASSWORD)] {
            let path = dir.path().join(format!("p{bits}.rcexport"));
            written(
                engine
                    .dispatch(export(&path, chosen, password, false))
                    .await,
            );
            let read = report(&engine, import(&path, password, true, None)).await;
            assert_eq!(read.parts, chosen, "parts {bits}");
            assert_eq!(read.encrypted, password.is_some());
            let has = |part| read.rows.iter().any(|row| row.part == part);
            assert_eq!(has(ExchangePart::Settings), chosen.settings);
            assert_eq!(has(ExchangePart::Connections), chosen.connections);
            assert_eq!(has(ExchangePart::Trust), chosen.trust);
            assert_eq!(has(ExchangePart::Hotlist), chosen.hotlist);
            // Exported here, everything is here already - but the export's own choice, which
            // each export keeps after it was written.
            assert!(
                read.rows
                    .iter()
                    .filter(|row| !row.key.starts_with("setting:exchange.export."))
                    .all(|row| !row.selected),
                "parts {bits}: {:?}",
                read.rows
            );
        }
    }
}

#[tokio::test]
async fn a_refused_export_writes_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    let (engine, _) = seeded(dir.path()).await;
    let path = dir.path().join("x.rcexport");
    let nothing = engine
        .dispatch(export(&path, ExportParts::default(), None, false))
        .await
        .expect_err("no part");
    assert_eq!(nothing.code(), "exchange_nothing_chosen");
    let empty = engine
        .dispatch(export(&path, ExportParts::ALL, Some(""), false))
        .await
        .expect_err("empty password");
    assert_eq!(empty.code(), "exchange_password_empty");
    assert!(!path.exists());
    std::fs::write(&path, "keep").expect("there");
    let Command::ExportConfig(mut request) = export(&path, ExportParts::ALL, None, false) else {
        unreachable!("an export");
    };
    request.overwrite = false;
    let exists = engine
        .dispatch(Command::ExportConfig(request))
        .await
        .expect_err("exists");
    assert_eq!(exists.code(), "exchange_target_exists");
    assert_eq!(std::fs::read_to_string(&path).expect("kept"), "keep");
}

#[tokio::test]
async fn the_vault_is_asked_only_for_secrets_and_the_choice_is_kept() {
    let dir = tempfile::tempdir().expect("dir");
    let (engine, _) = seeded(dir.path()).await;
    engine.dispatch(Command::LockVault).await.expect("locked");
    let path = dir.path().join("locked.rcexport");
    let settings_only = ExportParts {
        settings: true,
        ..ExportParts::default()
    };
    written(
        engine
            .dispatch(export(&path, settings_only, Some(EXPORT_PASSWORD), false))
            .await,
    );
    let db = engine.db().expect("db");
    assert_eq!(
        stored_setting(db, EXPORT_CONNECTIONS_KEY).await,
        Some(Value::Bool(false))
    );
    assert_eq!(
        stored_setting(db, EXPORT_ENCRYPTED_KEY).await,
        Some(Value::Bool(true))
    );
    written(
        engine
            .dispatch(export(&path, ExportParts::ALL, None, false))
            .await,
    );
    let locked = engine
        .dispatch(export(&path, ExportParts::ALL, Some(EXPORT_PASSWORD), true))
        .await
        .expect_err("locked");
    assert_eq!(locked.code(), "vault_locked");
    engine
        .dispatch(Command::UnlockVault {
            password: crate::SecretParam::from(MASTER),
        })
        .await
        .expect("unlocked");
    written(
        engine
            .dispatch(export(&path, ExportParts::ALL, Some(EXPORT_PASSWORD), true))
            .await,
    );
}

/// A link planted at the old fixed staging name is not written through, the file is the
/// person's alone, and no temporary name stays.
#[cfg(unix)]
#[tokio::test]
async fn an_export_never_writes_through_a_planted_link() {
    use std::os::unix::fs::PermissionsExt as _;

    let dir = tempfile::tempdir().expect("dir");
    let (engine, _) = seeded(dir.path()).await;
    let out = dir.path().join("out");
    std::fs::create_dir(&out).expect("folder");
    let victim = dir.path().join("victim");
    std::fs::write(&victim, "precious").expect("victim");
    let path = out.join("all.rcexport");
    std::os::unix::fs::symlink(&victim, out.join("all.rcexport.part")).expect("link");
    written(
        engine
            .dispatch(export(&path, ExportParts::ALL, None, false))
            .await,
    );
    assert_eq!(std::fs::read_to_string(&victim).expect("read"), "precious");
    let mode = std::fs::metadata(&path).expect("meta").permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
    let names: Vec<String> = std::fs::read_dir(&out)
        .expect("listed")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    assert_eq!(names.len(), 2, "the export and the planted link: {names:?}");
}
