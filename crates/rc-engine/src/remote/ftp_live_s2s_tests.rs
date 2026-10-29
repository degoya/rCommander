//! Server to server with FTP against real servers (RC-080-14), once per server on plain FTP
//! (`ftp_live_tests.rs` has the set-up): F5 between two FTP sessions, and between FTP and SFTP
//! both ways, each file streaming through rCommander with no local copy. The second FTP session
//! is the server's slow instance (`RC_LIVE_FTP_SLOW_PORT`); the SFTP server is an OpenSSH
//! container in the FTP server's network namespace (`RC_LIVE_FTP_SFTP_PORT`, `_SFTP_USER`,
//! `_SFTP_PASSWORD`), which `scripts/live/ftp/run.sh` starts beside it.

use std::path::Path;

use super::live_tests::{Live, completed, content, digest, live_on, port_var, read_remote, var};
use crate::{
    AuthMethod, Command, ConnectionDraft, ConnectionId, ConnectionKind, HostKeyAnswer, Outcome,
    PanelId, SecretKind, SecretParam,
};

/// The second FTP connection's name, to the slow instance.
const OTHER: &str = "live-other";
/// The SFTP connection's name.
const SFTP: &str = "live-sftp";

/// A saved connection of the live engine with its password stored.
async fn saved(live: &Live, draft: ConnectionDraft, password: &str) -> ConnectionId {
    let engine = live.engine();
    let Ok(Outcome::Connection { connection }) = engine
        .dispatch(Command::CreateConnection {
            folder_id: None,
            connection: draft,
        })
        .await
    else {
        panic!("a connection");
    };
    engine
        .dispatch(Command::SaveConnection(crate::SaveConnectionRequest {
            fields: crate::SaveFields::Update {
                id: connection.id,
                patch: crate::ConnectionPatch::default(),
            },
            secrets: vec![crate::SecretChange::Set {
                kind: SecretKind::Password,
                value: SecretParam::from(password),
            }],
            login_script: None,
            favourite: None,
            credential: None,
        }))
        .await
        .expect("password stored");
    connection.id
}

/// The SFTP connection, its unknown host key trusted as the window's question does it.
async fn sftp(live: &Live, port: u16) -> String {
    let user = var("RC_LIVE_FTP_SFTP_USER");
    let draft = ConnectionDraft {
        port: Some(i64::from(port)),
        username: Some(user.clone()),
        auth_method: AuthMethod::Password,
        ..ConnectionDraft::new(SFTP, "127.0.0.1")
    };
    let id = saved(live, draft, &var("RC_LIVE_FTP_SFTP_PASSWORD")).await;
    let engine = live.engine();
    let first = engine
        .dispatch(Command::ConnectSession { id })
        .await
        .expect_err("an unknown key first");
    assert_eq!(first.code(), "host_key_unknown", "{first}");
    let question = engine.host_key_questions().pop().expect("a question");
    engine
        .dispatch(Command::AnswerHostKey {
            question: question.id,
            answer: HostKeyAnswer::TrustAndSave,
        })
        .await
        .expect("trusted");
    engine
        .dispatch(Command::ConnectSession { id })
        .await
        .expect("connected");
    format!("/home/{user}")
}

#[tokio::test]
#[ignore = "live: scripts/live/ftp/run.sh"]
async fn live_ftp_s2s_between_two_sessions_and_through_sftp_byte_for_byte() {
    let live = live_on(None, None, true).await;
    live.open(PanelId::Right).await.expect("the panel opens");
    let home = live.env.home.clone();
    let vfs = live.vfs().await;
    let data = content(4 * 1024 * 1024);
    let name = format!("s2s-{}.bin", std::process::id());
    let upload = live.local().join("s2s-up");
    std::fs::create_dir_all(&upload).expect("folder");
    std::fs::write(upload.join(&name), &data).expect("local file");
    let up = live.local().join("s2s-up").join(&name);
    completed(
        &live
            .transfer(
                PanelId::Left,
                &[&up.to_string_lossy()],
                &live.location(&home),
                false,
            )
            .await,
    );

    // FTP to FTP: the server's slow instance as a second session, into a folder of its own.
    let slow = port_var("RC_LIVE_FTP_SLOW_PORT").expect("a slow instance on every server");
    let other = ConnectionDraft {
        kind: ConnectionKind::Ftp,
        port: Some(i64::from(slow)),
        username: Some(live.env.user.clone()),
        auth_method: AuthMethod::Password,
        ftp_data_mode: Some(live.env.mode),
        ..ConnectionDraft::new(OTHER, "127.0.0.1")
    };
    saved(&live, other, &live.env.password).await;
    let across = format!("{home}/s2s-other");
    vfs.mkdir(Path::new(&across), true).await.expect("a folder");
    completed(
        &live
            .transfer(
                PanelId::Left,
                &[&live.location(&format!("{home}/{name}"))],
                &format!("ftp://{OTHER}{across}"),
                false,
            )
            .await,
    );
    let copied = read_remote(&vfs, &format!("{across}/{name}")).await;
    assert_eq!(digest(&copied), digest(&data), "FTP to FTP");

    let Some(port) = port_var("RC_LIVE_FTP_SFTP_PORT") else {
        println!("no SFTP server beside this one: FTP and SFTP not testable here");
        return;
    };
    // FTP to SFTP, then SFTP back to FTP into a third folder.
    let sftp_home = sftp(&live, port).await;
    completed(
        &live
            .transfer(
                PanelId::Left,
                &[&format!("ftp://{OTHER}{across}/{name}")],
                &format!("sftp://{SFTP}{sftp_home}"),
                false,
            )
            .await,
    );
    let back = format!("{home}/s2s-back");
    vfs.mkdir(Path::new(&back), true).await.expect("a folder");
    completed(
        &live
            .transfer(
                PanelId::Left,
                &[&format!("sftp://{SFTP}{sftp_home}/{name}")],
                &live.location(&back),
                false,
            )
            .await,
    );
    let returned = read_remote(&vfs, &format!("{back}/{name}")).await;
    assert_eq!(returned.len(), data.len(), "FTP to SFTP and back");
    assert_eq!(digest(&returned), digest(&data), "FTP to SFTP and back");
}
