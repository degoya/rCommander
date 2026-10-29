use crate::{Command, CommandClass, spec};

/// A backup into the fixed folder adds a file and loses nothing; a path a person chose and a
/// restore, which replaces every setting and secret, are a person's decision.
#[test]
fn database_commands_have_the_classes_their_effects_need() {
    let class = |id: &str| spec(id).expect("a row").class;
    assert_eq!(class("db.backup"), CommandClass::Write);
    assert_eq!(class("db.backup_to"), CommandClass::GuiOnly);
    assert_eq!(class("db.restore"), CommandClass::GuiOnly);
    let backup: Command =
        serde_json::from_str(r#"{"command":"db.backup_to","params":{"path":"x"}}"#)
            .expect("deserializes");
    assert_eq!(
        backup,
        Command::BackupDatabaseTo(crate::BackupToRequest {
            path: "x".into(),
            overwrite: false
        })
    );
}
