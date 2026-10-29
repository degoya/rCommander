//! The prompt's words, and a connection a locked vault refused: the unlock runs it again once,
//! Escape and a wrong password run nothing (the owner's Windows test, 2026-10-18: after the
//! master password nothing happened).

use rc_engine::{
    AuthMethod, Command, ConnectionDraft, ConnectionId, ConnectionKind, JobOptions, Outcome,
    TransferRequest, VaultStateKind,
};
use rc_vault::SecretString;
use rc_vfs::BackendId;
use tempfile::TempDir;

use super::{AfterVault, Ask, Retry, VaultPrompt};
use crate::{
    app::{
        Effect, Job, Message,
        connections::ConnMsg,
        file_ops::{FileOpsMsg, FollowUp, Submit},
        harness::{self, Harness},
        run_job,
    },
    i18n::error_text,
};

#[test]
fn the_prompt_is_asked_only_for_a_vault_that_is_not_open() {
    assert_eq!(Ask::for_state(VaultStateKind::Absent), Some(Ask::Create));
    assert_eq!(Ask::for_state(VaultStateKind::Locked), Some(Ask::Unlock));
    assert_eq!(Ask::for_state(VaultStateKind::Unlocked), None);
    assert_eq!(Ask::for_code("vault_locked"), Some(Ask::Unlock));
    assert_eq!(Ask::for_code("name_taken"), None);
}

#[test]
fn a_new_master_password_is_typed_twice_alike() {
    let mut prompt = VaultPrompt::new(Ask::Create, AfterVault::Save);
    assert!(prompt.command().is_err(), "nothing typed");
    prompt.password = SecretString::from("correct horse");
    prompt.repeat = SecretString::from("correct house");
    assert!(prompt.command().is_err(), "the two differ");
    prompt.repeat = SecretString::from("correct horse");
    assert!(matches!(prompt.command(), Ok(Command::CreateVault { .. })));

    let mut unlock = VaultPrompt::new(Ask::Unlock, AfterVault::Save);
    unlock.password = SecretString::from("correct horse");
    let command = unlock.command().expect("a command");
    assert!(matches!(command, Command::UnlockVault { .. }));
    assert!(!format!("{command:?}").contains("horse"));
}

const MASTER: &str = "master-password-1";

async fn harness() -> (TempDir, Harness) {
    Harness::temp_with_db().await
}

/// An FTP server nobody listens on, with its password stored behind a locked vault: the
/// password is read before the connect, so the vault refuses first, and once it is open the
/// connect fails at once, without a network.
async fn locked_ftp(harness: &mut Harness, name: &str) -> ConnectionId {
    let engine = harness.app.engine().clone();
    let status = engine.dispatch(Command::GetVaultStatus).await;
    let password = MASTER.into();
    let open = match status {
        Ok(Outcome::Vault(status)) if status.state == VaultStateKind::Absent => {
            Command::CreateVault { password }
        }
        _ => Command::UnlockVault { password },
    };
    engine.dispatch(open).await.expect("the vault open");
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .expect("a free port")
        .port();
    let connection = ConnectionDraft {
        kind: ConnectionKind::Ftp,
        port: Some(i64::from(port)),
        username: Some("backup".into()),
        auth_method: AuthMethod::Password,
        ..ConnectionDraft::new(name, "127.0.0.1")
    };
    let created = engine
        .dispatch(Command::CreateConnection {
            folder_id: None,
            connection,
        })
        .await
        .expect("created");
    let Outcome::Connection { connection } = created else {
        panic!("a connection, got {created:?}");
    };
    let id = connection.id;
    engine
        .dispatch(harness::password_change(id, Some("canary-password-4")))
        .await
        .expect("stored");
    engine.dispatch(Command::LockVault).await.expect("locked");
    harness.settle().await;
    id
}

/// Enter on the connection in the manager (Ctrl+F) while the vault is locked: the prompt
/// alone, holding the refused drive change for the active panel.
async fn connect_while_locked(harness: &mut Harness, id: ConnectionId) -> Command {
    harness.press("Ctrl+F").await;
    harness.send(Message::Connections(ConnMsg::Files(id))).await;
    let active = harness.app.active;
    let change = Command::ChangeDrive {
        panel: Some(active),
        drive: format!("server:{id}"),
    };
    let manager = harness.app.connections.as_ref().expect("the prompt");
    assert!(manager.prompt_only, "the prompt alone, the manager closed");
    let prompt = manager.prompt.as_ref().expect("asked");
    assert_eq!(prompt.ask, Ask::Unlock);
    assert_eq!(
        prompt.after,
        AfterVault::Retry(Retry::Job(active, vec![change.clone()]))
    );
    assert_eq!(harness.app.pane(active).error, None, "the prompt says it");
    change
}

async fn type_master(harness: &mut Harness, password: &str) {
    let typed = SecretString::from(password);
    let input = ConnMsg::VaultInput(false, typed);
    harness.send(Message::Connections(input)).await;
}

#[tokio::test]
async fn the_unlock_runs_the_refused_connect_again_once() {
    let (_root, mut harness) = harness().await;
    let id = locked_ftp(&mut harness, "nas").await;
    let change = connect_while_locked(&mut harness, id).await;
    let active = harness.app.active;

    // A wrong password keeps the prompt and the command it waits to run.
    type_master(&mut harness, "wrong-password").await;
    harness
        .send(Message::Connections(ConnMsg::VaultSubmit))
        .await;
    let prompt = harness
        .app
        .connections
        .as_ref()
        .and_then(|manager| manager.prompt.as_ref())
        .expect("the prompt stays");
    assert!(prompt.error.is_some());
    assert_eq!(
        prompt.after,
        AfterVault::Retry(Retry::Job(active, vec![change.clone()]))
    );

    // The right one: the unlock's answer dispatches the drive change, once.
    type_master(&mut harness, MASTER).await;
    let submit = harness
        .app
        .apply(Message::Connections(ConnMsg::VaultSubmit));
    let Ok([Effect::Connections(unlock)]) = <[Effect; 1]>::try_from(submit) else {
        panic!("one job: the unlock command");
    };
    let answer = unlock.run(harness.app.engine().clone()).await;
    let effects = harness.app.apply(Message::Connections(answer));
    assert!(harness.app.connections.is_none(), "the prompt closed");
    let Ok([Effect::Dispatch(panel, commands)]) = <[Effect; 1]>::try_from(effects) else {
        panic!("exactly one effect: the connect again");
    };
    assert_eq!((panel, commands.as_slice()), (active, [change].as_slice()));

    // It reaches the server now: nobody listens there.
    let done = run_job(harness.app.engine(), Job { panel, commands }).await;
    harness.send(done).await;
    assert_eq!(
        harness.app.pane(active).error,
        Some(error_text("ftp_connect_failed"))
    );
}

#[tokio::test]
async fn escape_drops_the_refused_connect() {
    let (_root, mut harness) = harness().await;
    let id = locked_ftp(&mut harness, "nas").await;
    connect_while_locked(&mut harness, id).await;
    let active = harness.app.active;

    let cancelled = harness
        .app
        .apply(Message::Connections(ConnMsg::VaultCancel));
    assert!(cancelled.is_empty(), "nothing runs");
    assert!(harness.app.connections.is_none());

    // Unlocked elsewhere later: the dropped connect stays dropped.
    harness
        .app
        .engine()
        .dispatch(Command::UnlockVault {
            password: MASTER.into(),
        })
        .await
        .expect("unlocked");
    harness.settle().await;
    let pane = harness.app.pane(active);
    assert_eq!(pane.state.path.backend, BackendId::Local);
    assert_eq!(pane.error, None);
}

/// Test in the manager, refused twice: the prompt keeps the first connection's command, the
/// second says the vault is locked; the unlock tests the first again.
#[tokio::test]
async fn the_manager_keeps_the_first_refused_test_and_runs_it_after_the_unlock() {
    let (_root, mut harness) = harness().await;
    let first = locked_ftp(&mut harness, "nas").await;
    let second = locked_ftp(&mut harness, "backup").await;
    harness.press("Ctrl+F").await;
    for id in [first, second] {
        harness.send(Message::Connections(ConnMsg::Test(id))).await;
    }
    let manager = harness.app.connections.as_ref().expect("open");
    assert!(!manager.prompt_only, "the prompt stands in the manager");
    let prompt = manager.prompt.as_ref().expect("asked once");
    let AfterVault::Retry(Retry::Manager(_, command)) = &prompt.after else {
        panic!("a retry of the manager, got {:?}", prompt.after);
    };
    assert_eq!(**command, Command::TestConnection { id: first });
    assert_eq!(manager.note, Some((error_text("vault_locked"), true)));

    type_master(&mut harness, MASTER).await;
    harness
        .send(Message::Connections(ConnMsg::VaultSubmit))
        .await;
    let manager = harness.app.connections.as_ref().expect("still open");
    assert!(manager.prompt.is_none());
    assert_eq!(
        manager.note,
        Some((error_text("ftp_connect_failed"), true)),
        "the first connection was tested again"
    );
}

/// F5 from the local panel into the FTP server behind the locked vault: the engine refuses the
/// copy before it becomes a job, and the prompt alone holds it.
async fn copy_while_locked(harness: &mut Harness) -> Submit {
    let active = harness.app.active;
    let submit = Submit {
        panel: active,
        command: Command::OpsCopy(TransferRequest {
            panel: Some(active),
            sources: vec!["a.txt".into()],
            target: Some("ftp://nas/upload".into()),
            options: JobOptions::default(),
        }),
        follow: FollowUp::Transfer { deselect: true },
        background: false,
    };
    let done = submit.clone().run(harness.app.engine().clone()).await;
    assert_eq!(done.result.as_ref().err(), Some(&"vault_locked"));
    harness
        .send(Message::FileOps(FileOpsMsg::Submitted(Box::new(done))))
        .await;
    let manager = harness.app.connections.as_ref().expect("the prompt");
    assert!(manager.prompt_only, "the prompt alone");
    let prompt = manager.prompt.as_ref().expect("asked");
    assert_eq!(prompt.ask, Ask::Unlock);
    assert_eq!(
        prompt.after,
        AfterVault::Retry(Retry::Ops(Box::new(submit.clone())))
    );
    assert_eq!(harness.app.pane(active).error, None, "the prompt says it");
    submit
}

#[tokio::test]
async fn the_unlock_submits_the_refused_transfer_again_once() {
    let (_root, mut harness) = harness().await;
    locked_ftp(&mut harness, "nas").await;
    let submit = copy_while_locked(&mut harness).await;
    let active = harness.app.active;

    type_master(&mut harness, MASTER).await;
    let unlocking = harness
        .app
        .apply(Message::Connections(ConnMsg::VaultSubmit));
    let Ok([Effect::Connections(unlock)]) = <[Effect; 1]>::try_from(unlocking) else {
        panic!("one job: the unlock command");
    };
    let answer = unlock.run(harness.app.engine().clone()).await;
    let effects = harness.app.apply(Message::Connections(answer));
    assert!(harness.app.connections.is_none(), "the prompt closed");
    let Ok([Effect::Ops(again)]) = <[Effect; 1]>::try_from(effects) else {
        panic!("exactly one effect: the transfer submitted again");
    };
    assert_eq!(again, submit);

    // It reaches the server now: nobody listens there, and the refusal is no longer the vault's.
    let done = again.run(harness.app.engine().clone()).await;
    assert_eq!(done.refused, None);
    harness
        .send(Message::FileOps(FileOpsMsg::Submitted(Box::new(done))))
        .await;
    assert!(harness.app.connections.is_none(), "no second prompt");
    assert_eq!(
        harness.app.pane(active).error,
        Some(error_text("ftp_connect_failed"))
    );
}

#[tokio::test]
async fn escape_drops_the_refused_transfer() {
    let (_root, mut harness) = harness().await;
    locked_ftp(&mut harness, "nas").await;
    copy_while_locked(&mut harness).await;

    let cancelled = harness
        .app
        .apply(Message::Connections(ConnMsg::VaultCancel));
    assert!(cancelled.is_empty(), "nothing is submitted");
    assert!(harness.app.connections.is_none());
    assert!(harness.app.engine().ops().jobs().is_empty(), "no job");
}
