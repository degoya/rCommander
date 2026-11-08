use rc_engine::{ConnectionId, Forward, ForwardView, ForwardsOutcome, Outcome};

use super::{ForwardEdit, ForwardMsg, ForwardPanel};
use crate::app::connections::{ConnectionManager, jobs::Then};

fn view(id: i64, connection: i64) -> ForwardView {
    let forward = Forward {
        id,
        connection_id: ConnectionId(connection),
        bind_address: "127.0.0.1".to_owned(),
        bind_port: 15432,
        target_host: "db".to_owned(),
        target_port: 5432,
        auto_start: true,
        name: None,
        sort_order: 0,
    };
    ForwardView {
        label: forward.label(),
        running: false,
        connections: 0,
        forward,
    }
}

#[test]
fn the_editor_offers_loopback_and_other_addresses_only_with_the_setting() {
    let mut panel = ForwardPanel::default();
    assert_eq!(panel.binds(), ["127.0.0.1", "::1"]);
    panel.lan = true;
    assert_eq!(panel.binds(), ["127.0.0.1", "::1", "0.0.0.0", "::"]);
    panel.lan = false;
    panel.edit = Some(ForwardEdit {
        bind: "192.168.1.5".to_owned(),
        ..ForwardEdit::default()
    });
    assert_eq!(panel.binds()[0], "192.168.1.5", "a stored one stays shown");
}

#[test]
fn a_port_that_is_no_number_stays_in_the_editor_with_its_error() {
    let mut manager = ConnectionManager::default();
    manager.forwards.connection = Some(ConnectionId(7));
    manager.on_forward(ForwardMsg::Add);
    manager.on_forward(ForwardMsg::Port("eighty".to_owned()));
    manager.on_forward(ForwardMsg::Host("db".to_owned()));
    manager.on_forward(ForwardMsg::TargetPort("5432".to_owned()));
    let effects = manager.on_forward(ForwardMsg::Save);
    assert!(effects.is_empty(), "nothing is sent");
    let edit = manager.forwards.edit.as_ref().expect("still open");
    assert!(edit.error.is_some());
    manager.on_forward(ForwardMsg::Port("15432".to_owned()));
    assert_eq!(
        manager
            .forwards
            .edit
            .as_ref()
            .and_then(|edit| edit.error.clone()),
        None,
        "typing clears it"
    );
    assert_eq!(manager.on_forward(ForwardMsg::Save).len(), 1);
}

#[test]
fn a_list_for_another_connection_is_left_out() {
    let mut manager = ConnectionManager::default();
    manager.forwards.connection = Some(ConnectionId(7));
    let listed = ForwardsOutcome::Listed {
        forwards: vec![view(1, 7), view(2, 8)],
    };
    manager.forward_answer(Then::Forwards, Ok(Outcome::Forwards(Box::new(listed))));
    let ids: Vec<i64> = manager
        .forwards
        .forwards
        .iter()
        .map(|view| view.forward.id)
        .collect();
    assert_eq!(ids, [1]);
    manager.on_forward(ForwardMsg::Edit(1));
    let edit = manager.forwards.edit.as_ref().expect("open");
    assert_eq!((edit.port.as_str(), edit.host.as_str()), ("15432", "db"));
    manager.forward_answer(Then::ForwardDone, Err("forward_port_in_use"));
    assert!(
        manager
            .forwards
            .edit
            .as_ref()
            .is_some_and(|edit| edit.error.is_some())
    );
}
