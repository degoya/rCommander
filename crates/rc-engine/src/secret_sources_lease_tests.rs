//! A PAM system as a connection's source (RC-100-38), over an installed lending test source:
//! lent at connect and held for the session; given back at disconnect, when the session ends by
//! itself and when rCommander ends; an ended lease is lent anew; a wanted reason is asked for in
//! the window, never for MCP, and handed over at the next connect.

use rc_db::{Db, connections::ConnectionDraft};
use rc_plugin_host::{
    key_fingerprint,
    test_support::{secrets_package_with, signing_key},
    testbed::secrets::{LENT, SecretsBed},
};
use rc_vault::ExposeSecret as _;

use super::*;
use crate::{Command, ConnectionId, EngineEvent, Outcome, SecretRef, test_support};

const ID: &str = "019d0000-0000-7000-8000-0000000000b2";
const SOURCE: &str = "plugin:019d0000-0000-7000-8000-0000000000b2";
const LENDING: &str = "\n[secrets]\ncheckout = true\n";

struct Setup {
    _root: tempfile::TempDir,
    engine: Engine,
    connection: ConnectionId,
}

impl Setup {
    /// An engine with a database, the lending source of `bed` installed, and a connection whose
    /// password is mapped to it.
    async fn new(bed: &SecretsBed) -> Self {
        let root = tempfile::tempdir().expect("temp dir");
        let engine = test_support::headless(root.path()).await;
        let db = Db::open(root.path().join("rcommander.db"))
            .await
            .expect("db");
        assert!(engine.attach_db(db.clone()));
        engine
            .set_protected_setting(WINCRED_SOURCE_KEY, &false.into())
            .await
            .expect("off");
        assert!(engine.attach_plugins(root.path().join("data"), None));
        let package = root.path().join("pam.rcplug");
        let key = signing_key(9);
        let bytes = secrets_package_with(ID, "tests-v1", &key, &bed.build(), LENDING);
        std::fs::write(&package, &bytes).expect("write");
        engine
            .dispatch(Command::InstallPlugin {
                path: package.display().to_string(),
                fingerprint: Some(key_fingerprint(&key.verifying_key())),
                digest: rc_plugin_host::sha256_hex(&bytes),
            })
            .await
            .expect("the window installs");
        let connection = db
            .create_connection(None, &ConnectionDraft::new("db01", "db01.example.org"))
            .await
            .expect("create")
            .id;
        engine
            .dispatch(Command::MapSecretSource(SecretSourceRef {
                connection_id: connection,
                kind: SecretKind::Password,
                source: SOURCE.to_owned(),
                entry_id: "acct-17".to_owned(),
                entry_label: "db01 (root)".to_owned(),
            }))
            .await
            .expect("mapped");
        Self {
            _root: root,
            engine,
            connection,
        }
    }

    fn secret(&self) -> SecretRef {
        SecretRef {
            connection: self.connection,
            kind: SecretKind::Password,
        }
    }

    /// A connect's read of the password, the person's (`announce`) or an agent's.
    async fn connect(&self, announce: bool) -> Result<String, EngineError> {
        self.engine
            .secret_for(self.secret(), announce)
            .await
            .map(|value| value.expect("a value").expose_secret().to_owned())
    }

    /// How often the source was asked `operation`.
    fn calls(&self, operation: &str) -> usize {
        self.engine
            .plugin_execution_log()
            .expect("an execution log")
            .records(ID)
            .iter()
            .filter(|record| record.operation == operation)
            .count()
    }

    fn held(&self) -> usize {
        self.engine.inner.remotes.leases.held_by(self.connection)
    }
}

/// Lent at the first connect, held for the session: a second login of the connection takes the
/// held value; `get` is never asked; the value goes on the scrub list.
#[tokio::test]
async fn a_connect_borrows_once_for_the_session() {
    let setup = Setup::new(&SecretsBed::new()).await;
    assert_eq!(setup.connect(true).await.expect("lent"), LENT);
    assert_eq!(setup.connect(true).await.expect("held"), LENT);
    assert_eq!(setup.calls("checkout"), 1);
    assert_eq!(setup.calls("get"), 0);
    assert_eq!(setup.held(), 1);
    let scrubbed = setup
        .engine
        .scrub_secrets(&format!("echo {LENT}"))
        .expect("the lent value is known");
    assert!(!scrubbed.contains(LENT), "{scrubbed}");

    let listed = setup
        .engine
        .dispatch(Command::ListSecretSources {
            connection_id: None,
        })
        .await
        .expect("listed");
    let Outcome::SecretSources(outcome) = listed else {
        panic!("sources, got {listed:?}");
    };
    let SecretSourcesOutcome::Sources { sources, .. } = *outcome else {
        panic!("sources");
    };
    assert!(
        sources
            .iter()
            .any(|info| info.source == SOURCE && info.lends)
    );
}

/// `connections.disconnect` gives the lease back, and the next connect borrows anew.
#[tokio::test]
async fn disconnecting_gives_the_lease_back() {
    let setup = Setup::new(&SecretsBed::new()).await;
    setup.connect(true).await.expect("lent");
    setup
        .engine
        .dispatch(Command::DisconnectSession {
            id: setup.connection,
        })
        .await
        .expect("disconnected");
    assert_eq!(setup.calls("checkin"), 1);
    assert_eq!(setup.held(), 0);
    setup.connect(true).await.expect("lent again");
    assert_eq!(setup.calls("checkout"), 2);
}

/// A session that ends by itself - closed for being idle, lost, or a login that failed - gives
/// the lease back as its watch does, unless another session of the connection is up; so does
/// the end of rCommander, for every connection.
#[tokio::test]
async fn an_ended_session_and_the_end_of_rcommander_give_leases_back() {
    let setup = Setup::new(&SecretsBed::new()).await;
    setup.connect(true).await.expect("lent");
    // No session of the connection is up: what the watch and a failed login call.
    setup.engine.return_unused_leases(setup.connection).await;
    assert_eq!(setup.calls("checkin"), 1);
    assert_eq!(setup.held(), 0);

    setup.connect(true).await.expect("lent again");
    setup.engine.return_all_leases().await;
    assert_eq!(setup.calls("checkin"), 2);
    assert_eq!(setup.held(), 0);
}

/// A lease the system has ended is not used again: the next connect gives it back and borrows
/// anew.
#[tokio::test]
async fn an_ended_lease_is_lent_anew() {
    let setup = Setup::new(&SecretsBed::new().lending_until(1)).await;
    assert_eq!(setup.connect(true).await.expect("lent"), LENT);
    assert_eq!(setup.connect(true).await.expect("lent anew"), LENT);
    assert_eq!(setup.calls("checkout"), 2);
    assert_eq!(setup.calls("checkin"), 1);
    assert_eq!(setup.held(), 1);
}

/// A system that wants a reason: the person's connect fails with its code and the window is
/// asked; the reason given lends at the next connect, and is kept or used up by the setting. An
/// agent's connect asks no window and cannot give one.
#[tokio::test]
async fn a_wanted_reason_is_asked_in_the_window_only() {
    let setup = Setup::new(&SecretsBed::new().wanting_a_reason()).await;
    let mut events = setup.engine.subscribe();
    let refused = setup.connect(true).await.expect_err("a reason is wanted");
    assert_eq!(refused.code(), "secret_source_reason_needed");
    let mut asked = false;
    while let Ok(event) = events.try_recv() {
        asked |= event
            == EngineEvent::CheckoutReasonNeeded {
                connection: setup.connection,
            };
    }
    assert!(asked, "the window is asked for a reason");

    let agent = setup.connect(false).await.expect_err("no reason from MCP");
    assert_eq!(agent.code(), "secret_source_reason_needed");
    while let Ok(event) = events.try_recv() {
        assert!(
            !matches!(event, EngineEvent::CheckoutReasonNeeded { .. }),
            "an agent's connect asks no window"
        );
    }
    let reason = Command::SetCheckoutReason {
        connection_id: setup.connection,
        reason: "ticket 4711".to_owned(),
    };
    let refused = setup
        .engine
        .dispatch_mcp(reason.clone())
        .await
        .expect_err("GUI only");
    assert_eq!(refused.code(), "gui_only");

    setup.engine.dispatch(reason.clone()).await.expect("given");
    assert_eq!(setup.connect(true).await.expect("lent"), LENT);
    setup.engine.return_all_leases().await;
    // Kept by default: the next checkout passes it on again.
    assert_eq!(setup.connect(true).await.expect("lent again"), LENT);
    setup.engine.return_all_leases().await;

    setup
        .engine
        .set_protected_setting(CHECKOUT_KEEP_REASON_KEY, &false.into())
        .await
        .expect("off");
    assert_eq!(
        setup.connect(true).await.expect("lent, reason used up"),
        LENT
    );
    setup.engine.return_all_leases().await;
    let refused = setup.connect(true).await.expect_err("asked again");
    assert_eq!(refused.code(), "secret_source_reason_needed");

    let bad = setup
        .engine
        .dispatch(Command::SetCheckoutReason {
            connection_id: setup.connection,
            reason: "two\nlines".to_owned(),
        })
        .await
        .expect_err("refused");
    assert_eq!(bad.code(), "secret_source_invalid");
}
