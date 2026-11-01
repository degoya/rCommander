//! A connection tree for the tests of arranging and dropping (RC-100-63), built through serde so
//! that fields later jobs add with defaults need no change here:
//!
//! ```text
//! work/ (1)            folders 0
//!   db/ (3)
//!     pg (2)
//!   ops/ (4)
//!   web (1)
//!   api (4)
//! home/ (2)            folders 1
//! nas (3)              connections 0
//! jump (5)             connections 1
//! vault-user (7)       a shared credential at the root
//! ```

use rc_engine::ConnectionTree;
use serde_json::{Value, json};

use super::tree::TreeState;

fn connection(id: i64, folder: Option<i64>, name: &str) -> Value {
    json!({
        "id": id, "folder_id": folder, "name": name, "kind": "ssh", "host": "example.org",
        "port": 22, "username": null, "auth_method": "password", "key_path": null,
        "login_script_id": null, "default_view": "files", "start_directory": null, "notes": null,
        "sort_order": 0, "created_at": "", "updated_at": "", "has_password": false,
        "has_key": false, "has_passphrase": false, "key": null
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
        "id": id, "parent_id": parent, "name": name, "sort_order": 0, "created_at": "",
        "updated_at": "", "folders": folders, "connections": connections
    })
}

/// The tree above, every folder open, the credentials offered; `work` and `home` on the bar.
pub fn state() -> TreeState {
    let tree: ConnectionTree = serde_json::from_value(json!({
        "folders": [
            folder(1, None, "work", vec![
                folder(3, Some(1), "db", vec![], vec![connection(2, Some(3), "pg")]),
                folder(4, Some(1), "ops", vec![], vec![]),
            ], vec![connection(1, Some(1), "web"), connection(4, Some(1), "api")]),
            folder(2, None, "home", vec![], vec![]),
        ],
        "connections": [connection(3, None, "nas"), connection(5, None, "jump")],
        "credentials": [{
            "id": 7, "folder_id": null, "name": "vault-user", "username": "deploy",
            "created_at": "", "updated_at": "", "has_password": true, "has_key": false,
            "has_passphrase": false, "key": null, "connections": 0
        }],
        "bar_folders": [1, 2]
    }))
    .expect("a tree");
    let mut state = TreeState::default();
    state.tree = tree;
    state.offer_credentials = true;
    state
}
