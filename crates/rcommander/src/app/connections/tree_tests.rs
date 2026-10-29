use rc_engine::{ConnectionId, ConnectionTree, FolderId, TreeItem};
use serde_json::{Value, json};

use super::{TreeState, contents};

/// A connection as the engine answers it; built through serde so that fields later jobs add
/// with defaults need no change here.
fn connection(id: i64, folder: Option<i64>, name: &str, host: &str) -> Value {
    json!({
        "id": id, "folder_id": folder, "name": name, "kind": "ssh", "host": host, "port": 22,
        "username": null, "auth_method": "password", "key_path": null, "login_script_id": null,
        "default_view": "files", "start_directory": null, "notes": null, "sort_order": id,
        "created_at": "", "updated_at": "", "has_password": false, "has_key": false,
        "has_passphrase": false, "key": null
    })
}

fn folder(
    id: i64,
    parent: Option<i64>,
    name: &str,
    folders: Vec<Value>,
    connections: Vec<Value>,
) -> Value {
    json!({
        "id": id, "parent_id": parent, "name": name, "sort_order": id, "created_at": "",
        "updated_at": "", "folders": folders, "connections": connections
    })
}

/// `work/` (with `db/` holding `pg`, and `web`), `home/` (empty), and `nas` at the root.
fn state() -> TreeState {
    let tree: ConnectionTree = serde_json::from_value(json!({
        "folders": [
            folder(1, None, "work", vec![
                folder(3, Some(1), "db", vec![], vec![connection(2, Some(3), "pg", "db.example.org")]),
            ], vec![connection(1, Some(1), "web", "www.example.org")]),
            folder(2, None, "home", vec![], vec![]),
        ],
        "connections": [connection(3, None, "nas", "192.168.1.5")]
    }))
    .expect("a tree");
    TreeState {
        tree,
        ..TreeState::default()
    }
}

fn names(state: &TreeState) -> Vec<String> {
    state
        .rows()
        .iter()
        .map(|row| format!("{}{}", "-".repeat(row.depth), row.name))
        .collect()
}

#[test]
fn rows_show_folders_then_connections_per_level() {
    let state = state();
    assert_eq!(
        names(&state),
        ["work", "-db", "--pg", "-web", "home", "nas"]
    );
    let rows = state.rows();
    assert_eq!(rows[2].parent, Some(FolderId(3)));
    assert_eq!(rows[2].detail, "db.example.org");
}

#[test]
fn a_closed_folder_hides_what_it_holds_and_the_cursor_walks_the_rows() {
    let mut state = state();
    state.fold(FolderId(1), Some(false));
    assert_eq!(names(&state), ["work", "home", "nas"]);
    assert_eq!(state.step(1), Some(TreeItem::Folder(FolderId(2))));
    assert_eq!(state.step(5), Some(TreeItem::Connection(ConnectionId(3))));
    assert_eq!(state.jump(false), Some(TreeItem::Folder(FolderId(1))));
    state.fold(FolderId(1), None);
    assert!(state.is_open(FolderId(1)));
}

#[test]
fn the_filter_matches_name_or_host_and_keeps_the_folders_on_the_way() {
    let mut state = state();
    state.filter = "DB.EXAMPLE".into();
    assert_eq!(names(&state), ["work", "-db", "--pg"]);
    state.filter = "nas".into();
    assert_eq!(names(&state), ["nas"]);
    // A folder whose name matches shows all it holds.
    state.filter = "work".into();
    assert_eq!(names(&state), ["work", "-db", "--pg", "-web"]);
}

#[test]
fn new_things_go_into_the_folder_at_the_cursor_and_a_delete_moves_to_the_neighbour() {
    let state = state();
    let web = TreeItem::Connection(ConnectionId(1));
    assert_eq!(state.target_folder(Some(web)), Some(FolderId(1)));
    assert_eq!(
        state.target_folder(Some(TreeItem::Folder(FolderId(2)))),
        Some(FolderId(2))
    );
    assert_eq!(state.target_folder(None), None);
    assert_eq!(
        state.parent_of(TreeItem::Folder(FolderId(3))),
        Some(FolderId(1))
    );
    // `work` goes with everything inside it: the cursor lands on `home`.
    assert_eq!(
        state.neighbour(TreeItem::Folder(FolderId(1))),
        Some(TreeItem::Folder(FolderId(2)))
    );
    // The last row falls back to the one before it.
    assert_eq!(
        state.neighbour(TreeItem::Connection(ConnectionId(3))),
        Some(TreeItem::Folder(FolderId(2)))
    );
    let work = state.folder(FolderId(1)).expect("work");
    assert_eq!(contents(work), (2, 2));
    assert_eq!(state.name_of(web), "web");
}
