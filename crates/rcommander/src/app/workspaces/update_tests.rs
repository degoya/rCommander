use super::*;

/// A note of `workspaces.apply` names what was left out with its code's message; a note without
/// a code stays as it is.
#[test]
fn a_skipped_note_is_put_in_words() {
    assert_eq!(
        skipped_note("workspace_connection:build server"),
        format!("{} (build server)", error_text("workspace_connection"))
    );
    assert_eq!(skipped_note("left as it is"), "left as it is");
    assert_eq!(skipped_note("C:\\x"), "C:\\x", "a drive letter is no code");
}
