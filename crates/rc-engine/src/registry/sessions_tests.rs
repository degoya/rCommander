use crate::{AuthKind, Command, CommandClass, ConnectionId, SessionPrefs};

#[test]
fn a_session_command_reads_with_defaults() {
    let connect: Command =
        serde_json::from_str(r#"{"command":"connections.connect","params":{"id":3}}"#)
            .expect("deserializes");
    assert_eq!(
        connect,
        Command::ConnectSession {
            id: ConnectionId(3)
        }
    );
    assert_eq!(connect.spec().class, CommandClass::Write);
    let prefs: Command = serde_json::from_str(
        r#"{"command":"connections.set_session_prefs","params":{"id":3,"prefs":{"auth_order":["password","agent"]}}}"#,
    )
    .expect("deserializes");
    assert_eq!(
        prefs,
        Command::SetSessionPrefs {
            id: ConnectionId(3),
            prefs: SessionPrefs {
                auth_order: vec![AuthKind::Password, AuthKind::Agent],
                ..SessionPrefs::default()
            },
        }
    );
}
