use crate::{Command, CommandClass, spec, viewer::ReadTextRequest};

#[test]
fn a_file_read_takes_only_a_path_and_is_read_class() {
    let command: Command =
        serde_json::from_str(r#"{"command":"files.read_text","params":{"path":"log.txt"}}"#)
            .expect("deserializes");
    assert_eq!(
        command,
        Command::ReadText(ReadTextRequest {
            path: "log.txt".into(),
            panel: None,
            offset: 0,
            max_bytes: None,
            encoding: None,
        })
    );
    assert_eq!(command.spec().class, CommandClass::Read);
    assert_eq!(
        spec("files.read_hex").expect("a row").class,
        CommandClass::Read
    );
}
