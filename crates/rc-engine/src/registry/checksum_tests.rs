use crate::{CommandClass, spec};

/// Hashing and checking read; writing checksum files, parts or a combined file creates new
/// files only (never overwrites), so it is `write`.
#[test]
fn checksum_commands_have_the_classes_their_effects_need() {
    let class = |id: &str| spec(id).expect("a row").class;
    assert_eq!(class("checksum.compute"), CommandClass::Read);
    assert_eq!(class("checksum.verify"), CommandClass::Read);
    assert_eq!(class("checksum.create"), CommandClass::Write);
    assert_eq!(class("file.split"), CommandClass::Write);
    assert_eq!(class("file.combine"), CommandClass::Write);
    // Opening a dialog changes nothing; its job writes as `checksum.create` does.
    assert_eq!(class("checksum.dialog"), CommandClass::Read);
    assert_eq!(class("checksum.queue"), CommandClass::Write);
}

#[test]
fn a_keymap_entry_names_the_dialog_and_a_job_its_operation() {
    let dialog: crate::Command = serde_json::from_value(serde_json::json!({
        "command": "checksum.dialog",
        "params": { "dialog": "split" },
    }))
    .expect("a keymap entry");
    assert_eq!(
        dialog,
        crate::Command::ChecksumDialog {
            panel: None,
            dialog: super::ChecksumDialog::Split,
        }
    );
    let job: crate::Command = serde_json::from_value(serde_json::json!({
        "command": "checksum.queue",
        "params": { "op": "verify", "path": "a.sfv" },
    }))
    .expect("a job");
    assert_eq!(job.id(), "checksum.queue");
}
