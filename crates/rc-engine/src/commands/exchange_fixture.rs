//! What the export and import tests of RC-080-19 share: an engine on a database file with a
//! vault that derives in milliseconds, and one seeded with something of every part - a folder
//! with a connection that has a stored password and runs a login script with a secret of its
//! own, an FTP connection at the root, a trusted host key and certificate, a hotlist folder with
//! an entry, and a changed setting.

use std::path::Path;

use rc_db::{
    Db, HotlistItem, KnownHostSource, NewHotlistItem, NewKnownHost, NewTrustedCertificate,
};
use rc_remote::hostkey::OfferedKey;
use rc_vfs::VfsPath;
use serde_json::json;

use crate::test_support;
use crate::{
    AuthMethod, Command, ConfigExportRequest, ConfigImportRequest, ConnectionDraft, ConnectionId,
    ConnectionKind, Engine, FolderId, LoginScriptDraft, LoginScriptPatch, Outcome, Pattern,
    ScriptSecretRef, SecretKind, SecretParam, Step,
    exchange::{ExchangeReport, ExportParts},
    settings::THEME_KEY,
};

pub(crate) const MASTER: &str = "correct horse battery";
pub(crate) const EXPORT_PASSWORD: &str = "export password 1234";
/// The connection's stored password: must never be found in a sealed file.
pub(crate) const CANARY_PASSWORD: &str = "canary-web-pw-7781";
/// The login script's stored secret.
pub(crate) const CANARY_SCRIPT: &str = "canary-sudo-4410";
/// `rc-remote`'s ed25519 test key, as a host key.
pub(crate) const HOST_KEY: &str =
    "AAAAC3NzaC1lZDI1NTE5AAAAIH38B0mPWqPM0FQUvjlDMwWAKkVe2nmIfFKh/biuz4mA";
/// Another ed25519 key: the bytes 1 to 32.
pub(crate) const OTHER_HOST_KEY: &str =
    "AAAAC3NzaC1lZDI1NTE5AAAAIAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8g";

/// A certificate fingerprint of 32 equal bytes.
pub(crate) fn fingerprint(byte: &str) -> String {
    vec![byte; 32].join(":")
}

pub(crate) use crate::test_support::engine_in;

pub(crate) async fn create_vault(engine: &Engine) {
    engine
        .dispatch(Command::CreateVault {
            password: SecretParam::from(MASTER),
        })
        .await
        .expect("vault");
}

/// The host key `key` (base64) trusted for `host` on port 22.
pub(crate) async fn trust_key(db: &Db, host: &str, key: &str) {
    let offered = OfferedKey::from_openssh("ssh-ed25519", key).expect("a key");
    db.add_known_host(&NewKnownHost {
        host: host.to_owned(),
        port: 22,
        algorithm: offered.algorithm,
        public_key: offered.public_key,
        fingerprint_sha256: offered.fingerprint,
        source: KnownHostSource::User,
    })
    .await
    .expect("stored")
    .expect("new");
}

async fn run(engine: &Engine, command: Command) -> Outcome {
    engine.dispatch(command).await.expect("runs")
}

/// An engine in `dir` with something of every part; the `web` connection's id.
pub(crate) async fn seeded(dir: &Path) -> (Engine, ConnectionId) {
    let engine = engine_in(dir).await;
    create_vault(&engine).await;
    let Outcome::Folder { folder } = run(
        &engine,
        Command::CreateFolder {
            parent_id: None,
            name: "prod".into(),
        },
    )
    .await
    else {
        panic!("a folder");
    };
    let web = connection(&engine, Some(folder.id), "web", ConnectionKind::Ssh).await;
    connection(&engine, None, "nas", ConnectionKind::Ftp).await;
    run(
        &engine,
        test_support::store_secret(
            web,
            SecretKind::Password,
            SecretParam::from(CANARY_PASSWORD),
        ),
    )
    .await;
    let draft = LoginScriptDraft {
        name: "sudo".into(),
        steps: sudo_steps(None),
        step_timeout_ms: None,
        total_timeout_ms: None,
    };
    let Outcome::LoginScript { script } =
        run(&engine, Command::CreateLoginScript { script: draft }).await
    else {
        panic!("a script");
    };
    let Outcome::LoginScript { script } = run(
        &engine,
        Command::SetLoginScriptSecret {
            script_id: script.id,
            label: "root".into(),
            value: SecretParam::from(CANARY_SCRIPT),
        },
    )
    .await
    else {
        panic!("a script");
    };
    let patch = LoginScriptPatch {
        steps: Some(sudo_steps(Some(script.secrets[0].id))),
        ..LoginScriptPatch::default()
    };
    run(
        &engine,
        Command::UpdateLoginScript {
            id: script.id,
            patch,
        },
    )
    .await;
    run(
        &engine,
        Command::AssignLoginScript {
            connection_id: web,
            script_id: Some(script.id),
        },
    )
    .await;
    let db = engine.db().expect("db");
    trust_key(db, "web.example.org", HOST_KEY).await;
    db.add_trusted_certificate(&NewTrustedCertificate {
        host: "nas.example.org".into(),
        port: 21,
        fingerprint_sha256: fingerprint("AB"),
        subject: "CN=nas".into(),
        issuer: "CN=nas".into(),
        not_after: 2_000_000_000,
    })
    .await
    .expect("stored");
    let projects: HotlistItem = db
        .add_hotlist(&NewHotlistItem {
            parent_id: None,
            name: "Projects",
            location: None,
            target: None,
        })
        .await
        .expect("folder");
    let place = serde_json::to_string(&VfsPath::local(dir.join("src"))).expect("json");
    db.add_hotlist(&NewHotlistItem {
        parent_id: Some(projects.id),
        name: "src",
        location: Some(&place),
        target: None,
    })
    .await
    .expect("entry");
    engine
        .set_protected_setting(THEME_KEY, &json!("dark"))
        .await
        .expect("setting");
    (engine, web)
}

async fn connection(
    engine: &Engine,
    folder_id: Option<FolderId>,
    name: &str,
    kind: ConnectionKind,
) -> ConnectionId {
    let draft = ConnectionDraft {
        kind,
        username: Some("deploy".into()),
        auth_method: AuthMethod::Password,
        ..ConnectionDraft::new(name, format!("{name}.example.org"))
    };
    match run(
        engine,
        Command::CreateConnection {
            folder_id,
            connection: draft,
        },
    )
    .await
    {
        Outcome::Connection { connection } => connection.id,
        other => panic!("a connection, got {other:?}"),
    }
}

/// `sudo -i`, then the password: the connection's, or the script's stored secret `secret`.
pub(crate) fn sudo_steps(secret: Option<i64>) -> Vec<Step> {
    let source = secret.map_or(ScriptSecretRef::ConnectionPassword, |id| {
        ScriptSecretRef::Stored { id }
    });
    vec![
        Step::SendText {
            text: "sudo -i".into(),
            newline: true,
        },
        Step::WaitFor {
            pattern: Pattern::new("(?i)password.*:").expect("pattern"),
            timeout_ms: Some(2_000),
        },
        Step::SendSecret {
            source,
            newline: true,
        },
    ]
}

/// `config.export` of `parts` to `path`.
pub(crate) fn export(
    path: &Path,
    parts: ExportParts,
    password: Option<&str>,
    secrets: bool,
) -> Command {
    Command::ExportConfig(ConfigExportRequest {
        path: path.display().to_string(),
        parts,
        password: password.map(SecretParam::from),
        secrets,
        plain_secrets: false,
        overwrite: true,
    })
}

/// `config.import` of `path`.
pub(crate) fn import(
    path: &Path,
    password: Option<&str>,
    dry_run: bool,
    only: Option<Vec<String>>,
) -> Command {
    Command::ImportConfig(ConfigImportRequest {
        path: path.display().to_string(),
        password: password.map(SecretParam::from),
        dry_run,
        only,
    })
}

/// The report of an import that succeeds.
pub(crate) async fn report(engine: &Engine, command: Command) -> ExchangeReport {
    match engine.dispatch(command).await {
        Ok(Outcome::ConfigImported(report)) => report,
        other => panic!("an import report, got {other:?}"),
    }
}
