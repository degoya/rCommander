use crate::{Command, CommandClass, ConnectionDraft, FolderId};

#[test]
fn a_connection_command_reads_with_defaults() {
    let command: Command = serde_json::from_str(
        r#"{"command":"connections.create","params":{"connection":{"name":"web","host":"example.org"}}}"#,
    )
    .expect("deserializes");
    assert_eq!(
        command,
        Command::CreateConnection {
            folder_id: None,
            connection: ConnectionDraft::new("web", "example.org"),
        }
    );
    let list: Command =
        serde_json::from_str(r#"{"command":"connections.list"}"#).expect("deserializes");
    assert_eq!(list, Command::ListConnections);
    let delete: Command = serde_json::from_str(r#"{"command":"folders.delete","params":{"id":4}}"#)
        .expect("deserializes");
    assert_eq!(
        delete,
        Command::DeleteFolder {
            id: FolderId(4),
            recursive: false
        }
    );
    assert_eq!(delete.spec().class, CommandClass::Destructive);
}
