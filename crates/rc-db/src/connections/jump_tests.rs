//! Jump hosts (RC-110-03): the chain is checked inside every write and read back in order.

use crate::{
    Db,
    connections::{
        Connection, ConnectionDraft, ConnectionId, ConnectionKind, ConnectionPatch, EndpointRule,
        MAX_JUMPS,
    },
};

async fn ssh(db: &Db, name: &str, jump: Option<ConnectionId>) -> Connection {
    db.create_connection(
        None,
        &ConnectionDraft {
            jump_id: jump,
            ..ConnectionDraft::new(name, format!("{name}.example.org"))
        },
    )
    .await
    .expect(name)
}

fn jump_to(id: Option<ConnectionId>) -> ConnectionPatch {
    ConnectionPatch {
        jump_id: Some(id.map_or(0, |id| id.0)),
        ..ConnectionPatch::default()
    }
}

#[tokio::test]
async fn a_chain_reads_back_outermost_first() {
    let db = Db::open_in_memory().await.expect("open");
    let bastion = ssh(&db, "bastion", None).await;
    let inner = ssh(&db, "inner", Some(bastion.id)).await;
    let target = ssh(&db, "target", Some(inner.id)).await;
    assert_eq!(target.jump_id, Some(inner.id));
    let chain = db.jump_chain(target.id).await.expect("chain");
    let names: Vec<&str> = chain.iter().map(|hop| hop.name.as_str()).collect();
    assert_eq!(names, ["bastion", "inner"]);
    assert!(db.jump_chain(bastion.id).await.expect("none").is_empty());
    assert_eq!(db.jump_users(bastion.id).await.expect("users"), ["inner"]);
}

#[tokio::test]
async fn a_connection_is_not_its_own_jump_host_nor_reaches_itself() {
    let db = Db::open_in_memory().await.expect("open");
    let a = ssh(&db, "a", None).await;
    let b = ssh(&db, "b", Some(a.id)).await;
    let error = db
        .update_connection(a.id, &jump_to(Some(a.id)))
        .await
        .expect_err("itself");
    assert_eq!(error.code(), "jump_cycle");
    let error = db
        .update_connection(a.id, &jump_to(Some(b.id)))
        .await
        .expect_err("a cycle");
    assert_eq!(error.code(), "jump_cycle");
    assert_eq!(db.connection(a.id).await.expect("a").jump_id, None);
}

#[tokio::test]
async fn a_chain_is_at_most_four_hops_for_every_connection_on_it() {
    let db = Db::open_in_memory().await.expect("open");
    let mut last = None;
    let mut hops = Vec::new();
    for index in 0..=MAX_JUMPS {
        let made = ssh(&db, &format!("hop{index}"), last).await;
        last = Some(made.id);
        hops.push(made);
    }
    // hop4 has four in front of it: one more is refused.
    let error = db
        .create_connection(
            None,
            &ConnectionDraft {
                jump_id: last,
                ..ConnectionDraft::new("five", "five.example.org")
            },
        )
        .await
        .expect_err("five hops");
    assert_eq!(error.code(), "jump_chain_too_long");
    // A jump host in front of hop0 would put five in front of hop4.
    let front = ssh(&db, "front", None).await;
    let error = db
        .update_connection(hops[0].id, &jump_to(Some(front.id)))
        .await
        .expect_err("hop4 would have five");
    assert_eq!(error.code(), "jump_chain_too_long");
}

#[tokio::test]
async fn only_ssh_connections_use_or_are_jump_hosts() {
    let db = Db::open_in_memory().await.expect("open");
    let mut ftp = ConnectionDraft::new("files", "files.example.org");
    ftp.kind = ConnectionKind::Ftp;
    ftp.username = Some("me".to_owned());
    let ftp = db.create_connection(None, &ftp).await.expect("ftp");
    let error = db
        .create_connection(
            None,
            &ConnectionDraft {
                jump_id: Some(ftp.id),
                ..ConnectionDraft::new("behind", "behind.example.org")
            },
        )
        .await
        .expect_err("an FTP jump host");
    assert_eq!(error.code(), "jump_needs_ssh");
    let bastion = ssh(&db, "bastion", None).await;
    let error = db
        .update_connection(ftp.id, &jump_to(Some(bastion.id)))
        .await
        .expect_err("FTP through a jump host");
    assert_eq!(error.code(), "jump_needs_ssh");
    ssh(&db, "target", Some(bastion.id)).await;
    let to_ftp = ConnectionPatch {
        kind: Some(ConnectionKind::Ftp),
        username: Some("me".to_owned()),
        ..ConnectionPatch::default()
    };
    let error = db
        .update_connection(bastion.id, &to_ftp)
        .await
        .expect_err("a jump host in use stays SSH");
    assert_eq!(error.code(), "jump_needs_ssh");
}

#[tokio::test]
async fn a_jump_host_in_use_is_not_deleted_alone() {
    let db = Db::open_in_memory().await.expect("open");
    let folder = db.create_folder(None, "edge").await.expect("folder");
    let bastion = db
        .create_connection(
            Some(folder.id),
            &ConnectionDraft::new("bastion", "b.example"),
        )
        .await
        .expect("bastion");
    let target = ssh(&db, "target", Some(bastion.id)).await;
    let error = db.delete_connection(bastion.id).await.expect_err("in use");
    assert_eq!(error.code(), "connection_is_jump_host");
    assert!(error.to_string().contains("target"), "{error}");
    let error = db
        .delete_folder(folder.id, true)
        .await
        .expect_err("a user outside the folder");
    assert_eq!(error.code(), "connection_is_jump_host");
    db.update_connection(target.id, &jump_to(None))
        .await
        .expect("direct again");
    db.delete_connection(bastion.id).await.expect("unused now");
}

#[tokio::test]
async fn a_folder_deletes_a_jump_host_together_with_its_users() {
    let db = Db::open_in_memory().await.expect("open");
    let folder = db.create_folder(None, "edge").await.expect("folder");
    let bastion = db
        .create_connection(
            Some(folder.id),
            &ConnectionDraft::new("bastion", "b.example"),
        )
        .await
        .expect("bastion");
    db.create_connection(
        Some(folder.id),
        &ConnectionDraft {
            jump_id: Some(bastion.id),
            ..ConnectionDraft::new("target", "t.example")
        },
    )
    .await
    .expect("target");
    let gone = db.delete_folder(folder.id, true).await.expect("together");
    assert_eq!(gone.connections, 2);
}

#[tokio::test]
async fn a_new_jump_host_moves_the_endpoint_for_mcp() {
    let db = Db::open_in_memory().await.expect("open");
    let bastion = ssh(&db, "bastion", None).await;
    let target = ssh(&db, "target", None).await;
    {
        let mut writer = db.writer().await;
        sqlx::query("UPDATE connections SET password_enc = x'00' WHERE id = ?")
            .bind(target.id.0)
            .execute(&mut *writer)
            .await
            .expect("password");
    }
    let held = db.connection(target.id).await.expect("target");
    let patch = jump_to(Some(bastion.id));
    assert!(super::super::moves_endpoint(&held, &patch));
    let error = db
        .update_connection_with(target.id, &patch, EndpointRule::KeepWhileSecretsHeld)
        .await
        .expect_err("secrets held");
    assert_eq!(error.code(), "endpoint_change_needs_gui");
    db.update_connection(target.id, &patch)
        .await
        .expect("the person in the window");
}
