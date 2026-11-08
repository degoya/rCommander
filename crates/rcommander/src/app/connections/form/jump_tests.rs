use rc_engine::{Connection, ConnectionId, ConnectionKind, ConnectionTree};

use super::{chain_text, jump_choices};

fn connection(id: i64, name: &str, kind: ConnectionKind, jump: Option<i64>) -> Connection {
    let json = serde_json::json!({
        "id": id,
        "folder_id": null,
        "name": name,
        "kind": kind.as_str(),
        "host": format!("{name}.example.org"),
        "port": 22,
        "username": null,
        "auth_method": "password",
        "key_path": null,
        "login_script_id": null,
        "default_view": "files",
        "start_directory": null,
        "notes": null,
        "ftp_data_mode": null,
        "ftp_encoding": null,
        "sort_order": id,
        "created_at": "2026-11-07T00:00:00Z",
        "updated_at": "2026-11-07T00:00:00Z",
        "has_password": false,
        "has_key": false,
        "has_passphrase": false,
        "has_totp_seed": false,
        "key": null,
        "credential_id": null,
        "jump_id": jump,
    });
    serde_json::from_value(json).expect("a connection")
}

fn tree() -> ConnectionTree {
    ConnectionTree {
        connections: vec![
            connection(1, "bastion", ConnectionKind::Ssh, None),
            connection(2, "inner", ConnectionKind::Ssh, Some(1)),
            connection(3, "target", ConnectionKind::Ssh, Some(2)),
            connection(4, "files", ConnectionKind::Ftp, None),
        ],
        ..ConnectionTree::default()
    }
}

#[test]
fn the_choices_leave_out_itself_ftp_and_what_goes_through_it() {
    let names = |own: Option<i64>| -> Vec<String> {
        jump_choices(&tree(), own.map(ConnectionId))
            .into_iter()
            .map(|(_, name)| name)
            .collect()
    };
    assert_eq!(names(None), ["bastion", "inner", "target"]);
    // inner and target go through bastion: bastion cannot go through them.
    assert_eq!(names(Some(1)), Vec::<String>::new());
    assert_eq!(names(Some(3)), ["bastion", "inner"]);
}

#[test]
fn the_chain_reads_from_the_first_hop() {
    assert_eq!(
        chain_text(&tree(), Some(ConnectionId(2))).as_deref(),
        Some("bastion \u{2192} inner")
    );
    assert_eq!(chain_text(&tree(), None), None);
}

#[test]
fn the_form_sends_its_jump_host_and_ftp_has_none() {
    use rc_engine::SaveFields;

    use crate::app::connections::form::{Edit, Field, Form};

    let mut form = Form::new(None);
    form.edit(Edit::Name("target".into()));
    form.edit(Edit::Host("target.example.org".into()));
    assert!(form.fields().contains(&Field::Jump));
    form.set_jump(Some(ConnectionId(1)));
    let SaveFields::Create { connection, .. } = form.plan().expect("valid").fields else {
        panic!("a new connection");
    };
    assert_eq!(connection.jump_id, Some(ConnectionId(1)));
    form.set_kind(ConnectionKind::Ftp);
    assert_eq!(form.jump, None);
    assert!(!form.fields().contains(&Field::Jump));
    let stored = connection_with_jump();
    let mut form = Form::of(&stored, false);
    form.set_jump(None);
    let SaveFields::Update { patch, .. } = form.plan().expect("valid").fields else {
        panic!("a stored connection");
    };
    assert_eq!(patch.jump_id, Some(0), "0 clears it");
}

fn connection_with_jump() -> Connection {
    connection(3, "target", ConnectionKind::Ssh, Some(2))
}
