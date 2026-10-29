//! Keys of an ssh_config import taken into the vault (RC-050-06): the import leaves every key a
//! path and never touches the vault; `secrets.import_key` then takes the first `IdentityFile`
//! of an imported host - a plain key, an encrypted one with its passphrase - and refuses a
//! wrong passphrase, a missing file and a key path that is not on this computer, before it is
//! opened. Neither the passphrase nor the key reaches an answer or a log line.

use std::{
    collections::HashMap,
    fmt::Write as _,
    fs,
    path::Path,
    sync::{Arc, Mutex, PoisonError},
};

use rc_vault::ExposeSecret as _;
use tracing_subscriber::layer::SubscriberExt as _;

use super::tests::{FIXTURES, MASTER, engine_in, master};
use crate::{
    AuthMethod, Command, ConnectionId, Engine, EngineError, ImportStatus, Outcome, SecretKind,
    SecretParam, SecretRef, SshConfigReport, VaultStateKind, commands::ssh_import::LocalUser,
};

const PASSPHRASE: &str = "fixture-passphrase";

const CONFIG: &str = "\
Host plain
    HostName plain.example.org
    IdentityFile ~/.ssh/id_plain

Host locked
    HostName locked.example.org
    IdentityFile ~/.ssh/id_locked

Host gone
    HostName gone.example.org
    IdentityFile ~/.ssh/id_gone

Host share
    HostName share.example.org
    IdentityFile //nas/keys/id_share
";

/// Every log line written meanwhile, all levels, with its fields.
#[derive(Clone, Default)]
struct Lines(Arc<Mutex<Vec<String>>>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Lines {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        let mut line = String::new();
        event.record(
            &mut |field: &tracing::field::Field, value: &dyn std::fmt::Debug| {
                let _ = write!(line, "{}={value:?} ", field.name());
            },
        );
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(line);
    }
}

/// A home `<dir>/home/tester` whose `.ssh` holds `CONFIG` and two of `rc-remote`'s test keys;
/// its config imported for real. The connection and the key path of each host, by alias.
async fn imported(dir: &Path, engine: &Engine) -> HashMap<String, (ConnectionId, String)> {
    let home = dir.join("home/tester");
    let ssh = home.join(".ssh");
    fs::create_dir_all(&ssh).expect("ssh dir");
    fs::write(ssh.join("config"), CONFIG).expect("config");
    fs::copy(
        format!("{FIXTURES}/client_key_ed25519"),
        ssh.join("id_plain"),
    )
    .expect("plain key");
    fs::copy(
        format!("{FIXTURES}/client_key_rsapem"),
        ssh.join("id_locked"),
    )
    .expect("locked key");
    let user = LocalUser {
        home: Some(home),
        name: "tester".to_owned(),
    };
    let outcome = engine
        .import_ssh_config_for(user, None, false, None, None)
        .await
        .expect("import");
    let Outcome::SshConfigImported(SshConfigReport { hosts, .. }) = outcome else {
        panic!("an import report, got {outcome:?}");
    };
    hosts
        .into_iter()
        .map(|host| {
            let ImportStatus::Created { id } = host.status else {
                panic!("{} was not created: {:?}", host.alias, host.status);
            };
            let path = host.identity_files.first().cloned().expect("a key path");
            (host.alias, (id, path))
        })
        .collect()
}

fn take(
    hosts: &HashMap<String, (ConnectionId, String)>,
    alias: &str,
    passphrase: Option<&str>,
) -> Command {
    let (id, path) = hosts.get(alias).cloned().expect("an imported host");
    Command::ImportKey(crate::ImportKeyRequest {
        connection_id: id,
        path,
        passphrase: passphrase.map(SecretParam::from),
    })
}

async fn refused(engine: &Engine, command: Command) -> EngineError {
    engine.dispatch(command).await.expect_err("refused")
}

#[tokio::test]
async fn an_imported_hosts_key_file_goes_into_the_vault_only_when_taken() {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = engine_in(dir.path()).await;
    let lines = Lines::default();
    let _logs =
        tracing::subscriber::set_default(tracing_subscriber::registry().with(lines.clone()));
    let hosts = imported(dir.path(), &engine).await;

    // The import keeps the paths and asks for no master password.
    let Ok(Outcome::Vault(status)) = engine.dispatch(Command::GetVaultStatus).await else {
        panic!("the vault's status");
    };
    assert_eq!(status.state, VaultStateKind::Absent);
    let (plain, plain_path) = hosts.get("plain").cloned().expect("plain");
    assert!(plain_path.ends_with("id_plain"), "{plain_path}");

    // A path that is not on this computer is refused before the vault or the file is asked.
    let share = refused(&engine, take(&hosts, "share", None)).await;
    assert_eq!(share.code(), "key_path_refused");

    // Taking a key is the one step that needs the vault.
    assert_eq!(
        refused(&engine, take(&hosts, "plain", None)).await.code(),
        "vault_absent"
    );
    engine
        .dispatch(Command::CreateVault {
            password: master(MASTER),
        })
        .await
        .expect("create");
    let Outcome::Connection { connection } = engine
        .dispatch(take(&hosts, "plain", None))
        .await
        .expect("taken")
    else {
        panic!("the connection");
    };
    assert!(connection.secrets.has_key && !connection.secrets.has_passphrase);
    assert_eq!(connection.auth_method, AuthMethod::Key);
    // The path stays as the import wrote it: a second import finds the host unchanged, and the
    // stored key is the one a session uses.
    assert_eq!(connection.key_path.as_deref(), Some(plain_path.as_str()));
    let stored = engine
        .resolve_secret(SecretRef {
            connection: plain,
            kind: SecretKind::Key,
        })
        .await
        .expect("resolves")
        .expect("stored");
    assert_eq!(
        stored.expose_secret(),
        fs::read_to_string(&plain_path).expect("the key file")
    );

    // An encrypted key: its passphrase is asked for, a wrong one refused, the right one stored.
    for (passphrase, code) in [
        (None, "key_passphrase_needed"),
        (Some("not it"), "key_passphrase_wrong"),
    ] {
        let error = refused(&engine, take(&hosts, "locked", passphrase)).await;
        assert_eq!(error.code(), code);
    }
    let Outcome::Connection { connection } = engine
        .dispatch(take(&hosts, "locked", Some(PASSPHRASE)))
        .await
        .expect("taken")
    else {
        panic!("the connection");
    };
    assert!(connection.secrets.has_key && connection.secrets.has_passphrase);

    // A key file that is not there is refused, and the connection keeps no key.
    let gone = refused(&engine, take(&hosts, "gone", None)).await;
    assert_eq!(gone.code(), "key_file_unreadable");
    let (gone_id, _) = hosts.get("gone").cloned().expect("gone");
    let Ok(Outcome::Connection { connection }) = engine
        .dispatch(Command::GetConnection { id: gone_id })
        .await
    else {
        panic!("the connection");
    };
    assert!(!connection.secrets.has_key && connection.key.is_none());

    // No answer, error or log line carries the passphrase or the key.
    let key = fs::read_to_string(&plain_path).expect("the key file");
    let material: Vec<&str> = key
        .lines()
        .filter(|line| !line.starts_with("-----") && line.len() >= 16)
        .collect();
    let logged = lines
        .0
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .join("\n");
    let shown = format!("{connection:?} {share:?} {gone:?} {share} {gone} {logged}");
    assert!(!shown.contains(PASSPHRASE), "{shown}");
    for line in material {
        assert!(!shown.contains(line), "{shown}");
    }
}
