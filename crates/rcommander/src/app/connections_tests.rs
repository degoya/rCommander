//! The connection manager through `apply` and a real engine on an in-memory database: the keys
//! build, change, move and delete the tree; the form's errors stand at their fields; a change
//! over MCP shows at once; a secret waits for the master password.

use rc_engine::{
    Command, Connection, ConnectionDraft, ConnectionId, DriveGroup, FolderId, Outcome, SecretKind,
    TreeItem, VaultStateKind,
};
use rc_vault::SecretString;
use tempfile::TempDir;

use super::{ConnMsg, ConnectionManager, Focus, Pending, form::Edit, vault::Ask};
use crate::app::{
    Message,
    harness::{self, Harness},
};

async fn harness() -> (TempDir, Harness) {
    Harness::temp_with_db().await
}

fn manager(harness: &Harness) -> &ConnectionManager {
    harness
        .app
        .connections
        .as_ref()
        .expect("the manager is open")
}

async fn send(harness: &mut Harness, message: ConnMsg) {
    harness.send(Message::Connections(message)).await;
}

async fn connection(harness: &Harness, id: ConnectionId) -> Connection {
    match harness
        .app
        .engine()
        .dispatch(Command::GetConnection { id })
        .await
        .expect("stored")
    {
        Outcome::Connection { connection } => connection,
        other => panic!("a connection, got {other:?}"),
    }
}

async fn folder(harness: &Harness, name: &str) -> FolderId {
    let command = Command::CreateFolder {
        parent_id: None,
        name: name.into(),
    };
    match harness
        .app
        .engine()
        .dispatch(command)
        .await
        .expect("folder")
    {
        Outcome::Folder { folder } => folder.id,
        other => panic!("a folder, got {other:?}"),
    }
}

fn cursor_connection(harness: &Harness) -> ConnectionId {
    match manager(harness).tree.cursor {
        Some(TreeItem::Connection(id)) => id,
        other => panic!("the cursor on a connection, got {other:?}"),
    }
}

#[tokio::test]
async fn ctrl_f_opens_the_manager_and_escape_closes_it() {
    let (_root, mut harness) = harness().await;
    harness.press("Ctrl+F").await;
    let open = manager(&harness);
    assert_eq!(open.vault, Some(VaultStateKind::Absent));
    assert!(open.tree.rows().is_empty());
    // The panel did not take the key.
    assert_eq!(harness.app.pane(rc_engine::PanelId::Left).cursor(), 0);
    harness.press("Escape").await;
    assert!(harness.app.connections.is_none());
    send(&mut harness, ConnMsg::Open).await;
    assert!(
        harness.app.connections.is_some(),
        "the status bar opens it too"
    );
}

/// Ctrl+F puts the keyboard into the tree (owner, 2026-10-26, RC-050-15): a letter typed there
/// goes into the filter, which keeps the keys - the arrows move the tree's cursor from there -
/// and Escape closes the manager, from the filter and from the form alike.
#[tokio::test]
async fn the_manager_opens_in_its_tree_typing_filters_and_escape_closes_it_from_anywhere() {
    let (_root, mut harness) = harness().await;
    let home = folder(&harness, "home").await;
    let work = folder(&harness, "work").await;
    harness.press("Ctrl+F").await;
    assert_eq!(manager(&harness).focus, Focus::Tree);

    harness.press("o").await;
    assert_eq!(manager(&harness).focus, Focus::Filter);
    let rows: Vec<_> = manager(&harness)
        .tree
        .rows()
        .iter()
        .map(|row| row.item)
        .collect();
    assert_eq!(rows.len(), 2, "both folders match");
    harness.press("ArrowDown").await;
    assert_eq!(
        manager(&harness).focus,
        Focus::Filter,
        "the filter keeps the keys"
    );
    assert_eq!(manager(&harness).tree.current(), Some(rows[1]));
    harness.press("ArrowUp").await;
    assert_eq!(manager(&harness).tree.current(), Some(rows[0]));
    send(&mut harness, ConnMsg::Filter("work".into())).await;
    assert_eq!(
        manager(&harness).tree.current(),
        Some(TreeItem::Folder(work)),
        "the first match is under the cursor, for Enter"
    );
    assert_ne!(
        Some(TreeItem::Folder(home)),
        manager(&harness).tree.current()
    );
    // A text field that has the keys captures Escape; it closes the manager all the same.
    harness
        .send(Message::KeyCaptured("Escape".parse().expect("chord")))
        .await;
    assert!(harness.app.connections.is_none());

    // From a field of the form too.
    harness.press("Ctrl+F").await;
    harness.press("Insert").await;
    assert!(matches!(manager(&harness).focus, Focus::Field(_)));
    harness.press("Escape").await;
    assert!(harness.app.connections.is_none());
}

/// The acceptance check "folders and connections are made, changed, moved and deleted by the
/// keyboard alone".
#[tokio::test]
async fn the_keyboard_alone_builds_changes_moves_and_deletes_the_tree() {
    let (_root, mut harness) = harness().await;
    harness.press("Ctrl+F").await;

    // F7: a folder, named in the tree.
    harness.press("F7").await;
    assert!(matches!(
        manager(&harness).pending,
        Some(Pending::Name { .. })
    ));
    send(&mut harness, ConnMsg::NameInput("work".into())).await;
    send(&mut harness, ConnMsg::NameSubmit).await;
    let work = match manager(&harness).tree.cursor {
        Some(TreeItem::Folder(id)) => id,
        other => panic!("the new folder, got {other:?}"),
    };

    // Insert: a connection in it, saved by Enter on a choice of the form.
    harness.press("Insert").await;
    assert_eq!(
        manager(&harness).focus,
        Focus::Field(super::form::Field::Name)
    );
    send(&mut harness, ConnMsg::Edit(Edit::Name("web".into()))).await;
    send(
        &mut harness,
        ConnMsg::Edit(Edit::Host("www.example.org".into())),
    )
    .await;
    harness.press("Tab").await;
    harness.press("Enter").await;
    let web = cursor_connection(&harness);
    assert_eq!(connection(&harness, web).await.folder_id, Some(work));
    assert_eq!(
        manager(&harness).form.as_ref().and_then(|form| form.id()),
        Some(web)
    );

    // Ctrl+X, then Ctrl+V on another folder: it moves there. A click on the connection gives
    // the tree the keys again - Escape would close the manager (RC-080-15).
    send(&mut harness, ConnMsg::Press(TreeItem::Connection(web))).await;
    send(&mut harness, ConnMsg::Release(TreeItem::Connection(web))).await;
    assert_eq!(manager(&harness).focus, Focus::Tree);
    let home = folder(&harness, "home").await;
    harness.settle().await;
    harness.press("Ctrl+X").await;
    harness.press("End").await;
    assert_eq!(manager(&harness).tree.cursor, Some(TreeItem::Folder(home)));
    harness.press("Ctrl+V").await;
    assert_eq!(connection(&harness, web).await.folder_id, Some(home));
    assert_eq!(cursor_connection(&harness), web);

    // F2 renames it.
    harness.press("F2").await;
    send(&mut harness, ConnMsg::NameInput("web-1".into())).await;
    send(&mut harness, ConnMsg::NameSubmit).await;
    assert_eq!(connection(&harness, web).await.name, "web-1");

    // F8 asks, Enter deletes; the folder goes with Delete and Enter too.
    harness.press("F8").await;
    assert!(matches!(
        manager(&harness).pending,
        Some(Pending::Delete { connections: 1, .. })
    ));
    harness.press("Enter").await;
    let gone = harness
        .app
        .engine()
        .dispatch(Command::GetConnection { id: web })
        .await;
    assert_eq!(gone.expect_err("deleted").code(), "connection_not_found");
    harness.press("Home").await;
    harness.press("Delete").await;
    harness.press("Enter").await;
    assert_eq!(manager(&harness).tree.rows().len(), 1, "one folder left");
}

/// The acceptance check "validation errors stand at the field; invalid data is not stored".
#[tokio::test]
async fn invalid_fields_stand_at_their_field_and_nothing_is_stored() {
    let (_root, mut harness) = harness().await;
    harness.press("Ctrl+F").await;
    harness.press("Insert").await;
    send(&mut harness, ConnMsg::Edit(Edit::Name("x".into()))).await;
    send(&mut harness, ConnMsg::Edit(Edit::Host("bad host".into()))).await;
    send(&mut harness, ConnMsg::Save).await;
    let form = manager(&harness).form.as_ref().expect("the form");
    assert!(form.error(super::form::Field::Host).is_some());
    assert!(manager(&harness).tree.tree.connections.is_empty());

    // The engine's refusal lands at its field too.
    harness
        .app
        .engine()
        .dispatch(Command::CreateConnection {
            folder_id: None,
            connection: ConnectionDraft::new("x", "a.example.org"),
        })
        .await
        .expect("created");
    send(
        &mut harness,
        ConnMsg::Edit(Edit::Host("b.example.org".into())),
    )
    .await;
    send(&mut harness, ConnMsg::Save).await;
    let form = manager(&harness).form.as_ref().expect("the form");
    assert_eq!(form.id(), None, "nothing stored");
    assert!(
        form.error(super::form::Field::Name).is_some(),
        "{:?}",
        form.errors
    );
}

/// The acceptance check "changes over MCP (`connections_create`) show in the open manager".
#[tokio::test]
async fn a_connection_made_elsewhere_shows_in_the_open_manager() {
    let (_root, mut harness) = harness().await;
    harness.press("Ctrl+F").await;
    harness
        .app
        .engine()
        .dispatch_mcp(Command::CreateConnection {
            folder_id: None,
            connection: ConnectionDraft::new("from-mcp", "mcp.example.org"),
        })
        .await
        .expect("created");
    harness.settle().await;
    let rows = manager(&harness).tree.rows();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].name, "from-mcp");
    // A root connection is a button of the drive bar; Space takes it away.
    let id = match rows[0].item {
        TreeItem::Connection(id) => id,
        other => panic!("a connection, got {other:?}"),
    };
    assert!(manager(&harness).favourites.contains(&id));
    harness.press("Space").await;
    assert!(manager(&harness).favourites.is_empty());
    let listed = harness
        .app
        .engine()
        .dispatch(Command::ListDrives { refresh: false })
        .await
        .expect("drives");
    let Outcome::Drives { drives } = listed else {
        panic!("drives");
    };
    assert!(
        drives
            .iter()
            .all(|drive| drive.group != DriveGroup::Servers || !drive.favourite),
        "still listed for the drive menu, but no button of the drive bar"
    );
}

/// The acceptance check "a password field never shows the stored value; storing the first
/// secret asks for the master password first".
#[tokio::test]
async fn the_first_secret_waits_for_a_master_password_and_is_never_shown_again() {
    let (_root, mut harness) = harness().await;
    harness.press("Ctrl+F").await;
    harness.press("Insert").await;
    send(&mut harness, ConnMsg::Edit(Edit::Name("db".into()))).await;
    send(
        &mut harness,
        ConnMsg::Edit(Edit::Host("db.example.org".into())),
    )
    .await;
    let typed = SecretString::from("canary-password-1");
    send(&mut harness, ConnMsg::Secret(SecretKind::Password, typed)).await;
    send(&mut harness, ConnMsg::Save).await;
    let prompt = manager(&harness).prompt.as_ref().expect("the prompt");
    assert_eq!(prompt.ask, Ask::Create);
    assert!(
        manager(&harness).tree.tree.connections.is_empty(),
        "nothing stored yet"
    );

    let master = || SecretString::from("master-password-1");
    send(&mut harness, ConnMsg::VaultInput(false, master())).await;
    send(&mut harness, ConnMsg::VaultInput(true, master())).await;
    send(&mut harness, ConnMsg::VaultSubmit).await;
    assert!(manager(&harness).prompt.is_none());
    assert_eq!(manager(&harness).vault, Some(VaultStateKind::Unlocked));
    let id = cursor_connection(&harness);
    assert!(connection(&harness, id).await.secrets.has_password);
    let form = manager(&harness).form.as_ref().expect("the form");
    assert!(form.stored.has_password);
    assert_eq!(rc_vault::ExposeSecret::expose_secret(&form.password), "");
    assert!(!format!("{:?}", manager(&harness)).contains("canary-password-1"));

    // Locked, the field shows nothing in clear and a new secret asks to unlock.
    harness
        .app
        .engine()
        .dispatch(Command::LockVault)
        .await
        .expect("locked");
    harness.settle().await;
    assert_eq!(manager(&harness).vault, Some(VaultStateKind::Locked));
    send(&mut harness, ConnMsg::Reveal(true)).await;
    assert!(!manager(&harness).form.as_ref().expect("form").reveal);
    let typed = SecretString::from("canary-password-2");
    send(&mut harness, ConnMsg::Secret(SecretKind::Password, typed)).await;
    send(&mut harness, ConnMsg::Save).await;
    let prompt = manager(&harness).prompt.as_ref().expect("the prompt");
    assert_eq!(prompt.ask, Ask::Unlock);
    harness.press("Escape").await;
    assert!(manager(&harness).prompt.is_none());
}

/// A stored password behind a locked vault: what a connection from the window meets.
async fn locked_secret(harness: &Harness) -> rc_engine::SecretRef {
    let engine = harness.app.engine();
    engine
        .dispatch(Command::CreateVault {
            password: "master-password-1".into(),
        })
        .await
        .expect("vault");
    let id = match engine
        .dispatch(Command::CreateConnection {
            folder_id: None,
            connection: ConnectionDraft::new("db", "db.example.org"),
        })
        .await
        .expect("created")
    {
        Outcome::Connection { connection } => connection.id,
        other => panic!("a connection, got {other:?}"),
    };
    engine
        .dispatch(harness::password_change(id, Some("canary-password-3")))
        .await
        .expect("stored");
    engine.dispatch(Command::LockVault).await.expect("locked");
    rc_engine::SecretRef {
        connection: id,
        kind: SecretKind::Password,
    }
}

/// `SecretNeeded` shows the unlock prompt alone; the master password unlocks the vault and the
/// prompt goes, so the connection can try again.
#[tokio::test]
async fn a_connection_that_needs_a_secret_asks_for_the_master_password_alone() {
    let (_root, mut harness) = harness().await;
    let secret = locked_secret(&harness).await;
    harness.settle().await;
    assert!(
        harness.app.connections.is_none(),
        "nothing asks before a connection needs it"
    );

    let refused = harness.app.engine().resolve_secret(secret).await;
    assert_eq!(refused.expect_err("locked").code(), "vault_locked");
    harness.settle().await;
    let open = manager(&harness);
    assert!(open.prompt_only);
    assert_eq!(
        open.prompt.as_ref().map(|prompt| prompt.ask),
        Some(Ask::Unlock)
    );
    assert!(open.tree.rows().is_empty(), "the tree is not shown or read");

    // Keys go to the prompt, not to the panels.
    harness.press("ArrowDown").await;
    assert_eq!(harness.app.pane(rc_engine::PanelId::Left).cursor(), 0);
    send(
        &mut harness,
        ConnMsg::VaultInput(false, SecretString::from("wrong-password")),
    )
    .await;
    send(&mut harness, ConnMsg::VaultSubmit).await;
    assert!(
        manager(&harness)
            .prompt
            .as_ref()
            .and_then(|p| p.error.as_ref())
            .is_some()
    );

    send(
        &mut harness,
        ConnMsg::VaultInput(false, SecretString::from("master-password-1")),
    )
    .await;
    send(&mut harness, ConnMsg::VaultSubmit).await;
    assert!(
        harness.app.connections.is_none(),
        "the prompt closed with the unlock"
    );
    let resolved = harness.app.engine().resolve_secret(secret).await;
    assert!(resolved.expect("unlocked").is_some());
}

/// Escape drops the prompt (only that connection fails); in an open manager the prompt stands
/// in its form and the manager stays.
#[tokio::test]
async fn the_unlock_prompt_can_be_dropped_and_stays_inside_an_open_manager() {
    let (_root, mut harness) = harness().await;
    let secret = locked_secret(&harness).await;
    let _ = harness.app.engine().resolve_secret(secret).await;
    harness.settle().await;
    harness.press("Escape").await;
    assert!(harness.app.connections.is_none());

    harness.press("Ctrl+F").await;
    let _ = harness.app.engine().resolve_secret(secret).await;
    harness.settle().await;
    assert!(!manager(&harness).prompt_only);
    assert!(manager(&harness).prompt.is_some());
    harness.press("Escape").await;
    assert!(manager(&harness).prompt.is_none(), "the prompt went");
    assert!(harness.app.connections.is_some(), "the manager stays");
}

/// Tab and Shift+Tab move between the master password and its repeat (Windows test
/// 2026-10-18: Tab did nothing in the prompt); the form behind keeps its field, the panels
/// their side.
#[tokio::test]
async fn tab_walks_the_fields_of_the_master_password_prompt() {
    let (_root, mut harness) = harness().await;
    harness.press("Ctrl+F").await;
    harness.press("Insert").await;
    send(&mut harness, ConnMsg::Edit(Edit::Name("db".into()))).await;
    send(
        &mut harness,
        ConnMsg::Edit(Edit::Host("db.example.org".into())),
    )
    .await;
    let typed = SecretString::from("canary-password-1");
    send(&mut harness, ConnMsg::Secret(SecretKind::Password, typed)).await;
    send(&mut harness, ConnMsg::Save).await;
    let prompt = manager(&harness).prompt.as_ref().expect("the prompt");
    assert_eq!(prompt.ask, Ask::Create);
    let focus = manager(&harness).focus;

    harness.press("Tab").await;
    harness.press("Shift+Tab").await;
    assert_eq!(harness.fields_walked, [false, true]);
    assert_eq!(manager(&harness).focus, focus, "the form keeps its field");
    assert!(manager(&harness).prompt.is_some(), "the prompt stays");
    assert_eq!(harness.app.active, rc_engine::PanelId::Left);

    // Without the prompt, Tab walks the manager's ring (RC-080-16).
    harness.press("Escape").await;
    harness.press("Tab").await;
    assert_eq!(harness.fields_walked, [false, true, false]);
}
