use crate::{Command, CommandClass, compare::FileCompareRequest};

#[test]
fn a_compare_command_reads_with_defaults_and_only_reads() {
    let files: Command =
        serde_json::from_str(r#"{"command":"compare.files","params":{}}"#).expect("deserializes");
    assert_eq!(files, Command::CompareFiles(FileCompareRequest::default()));
    let dirs: Command =
        serde_json::from_str(r#"{"command":"compare.dirs","params":{"options":{"masks":"*.rs"}}}"#)
            .expect("deserializes");
    let Command::CompareDirs(request) = &dirs else {
        panic!("compare.dirs, got {dirs:?}");
    };
    assert_eq!(request.options.masks, "*.rs");
    assert_eq!(files.spec().class, CommandClass::Read);
    assert_eq!(dirs.spec().class, CommandClass::Read);
}

#[test]
fn sync_apply_starts_from_the_proposal_and_confirms_no_delete() {
    let apply: Command =
        serde_json::from_str(r#"{"command":"sync.apply","params":{}}"#).expect("deserializes");
    let Command::SyncApply(request) = &apply else {
        panic!("sync.apply, got {apply:?}");
    };
    assert!(request.proposed);
    assert!(request.actions.is_empty());
    assert!(!request.run.deletes_confirmed);
    assert_eq!(request.run.on_conflict, None);
    assert_eq!(apply.spec().class, CommandClass::Write);
    for (id, wanted) in [
        ("compare.open_diff", Command::OpenDiff {}),
        ("sync.open_dialog", Command::OpenSyncDialog {}),
        (
            "compare.select_differences",
            Command::SelectDifferences { by_content: false },
        ),
    ] {
        let json = format!(r#"{{"command":"{id}","params":{{}}}}"#);
        let opened: Command = serde_json::from_str(&json).expect("deserializes");
        assert_eq!(opened, wanted);
        assert_eq!(opened.spec().class, CommandClass::Read);
    }
}

/// RC-100-94: Ctrl+Shift+F3's command runs a program, so it is Exec; copying blocks writes.
#[test]
fn compare_by_content_runs_a_program_and_apply_blocks_writes() {
    let by_content: Command =
        serde_json::from_str(r#"{"command":"compare.by_content","params":{}}"#)
            .expect("deserializes");
    assert_eq!(
        by_content,
        Command::CompareByContent(crate::compare::ByContentRequest::default())
    );
    assert_eq!(by_content.spec().class, CommandClass::Exec);
    let apply: Command = serde_json::from_str(
        r#"{"command":"compare.apply_blocks","params":{"left":"a","right":"b","blocks":[1],"direction":"right_to_left"}}"#,
    )
    .expect("deserializes");
    let Command::ApplyBlocks(request) = &apply else {
        panic!("compare.apply_blocks, got {apply:?}");
    };
    assert_eq!(request.blocks, [1]);
    assert_eq!(request.direction, crate::compare::Direction::RightToLeft);
    assert_eq!(request.expect_left, crate::compare::FileStamp::default());
    assert_eq!(apply.spec().class, CommandClass::Write);
}
