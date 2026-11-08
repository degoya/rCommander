use rc_engine::{
    EngineEvent, FileAssociation,
    associations::{RuleError, RuleErrorKind},
    settings::{FILE_ASSOCIATIONS_KEY, VIEWER_KEY},
};

use super::{AssocMsg, concerns, form::RuleMsg, refusal};
use crate::app::{Message, harness::Harness};

#[test]
fn only_the_three_settings_concern_the_menu() {
    assert!(concerns(FILE_ASSOCIATIONS_KEY));
    assert!(concerns("files.associations_enabled"));
    assert!(concerns("files.associations_all_matches"));
    assert!(!concerns(VIEWER_KEY));
}

#[test]
fn a_refusal_names_the_rule_and_the_command() {
    let error = RuleError {
        rule: Some(1),
        verb: Some("open".into()),
        kind: RuleErrorKind::NeedsFileParameter,
    };
    let text = refusal(&error);
    assert!(text.contains('2'), "{text}");
    assert!(text.contains("open"), "{text}");
}

/// `file.associations` opens the window; a program without a file parameter is not saved, the
/// rule and its command are shown and the note says why.
#[tokio::test]
async fn the_window_opens_and_refuses_a_rule_without_a_file_parameter() {
    let (_root, mut harness) = Harness::temp().await;
    harness
        .send(Message::Engine(EngineEvent::AssociationsWindow))
        .await;
    let window = harness.app.associations.as_ref().expect("open");
    assert!(window.rules.is_empty());
    let edit = |message| Message::Associations(AssocMsg::Edit(message));
    harness.send(Message::Associations(AssocMsg::New)).await;
    harness.send(edit(RuleMsg::Mask("*.log".into()))).await;
    harness
        .send(edit(RuleMsg::Cmd(
            crate::app::user_commands::CmdMsg::Command("notepad++.exe".into()),
        )))
        .await;
    harness.send(Message::Associations(AssocMsg::Save)).await;
    let window = harness.app.associations.as_ref().expect("still open");
    let (note, error) = window.note.clone().expect("a note");
    assert!(error, "{note}");
    assert!(window.dirty && !window.busy);
    assert_eq!(
        window.rules,
        [FileAssociation {
            mask: "*.log".into(),
            open: window.rules[0].open.clone(),
            ..FileAssociation::default()
        }]
    );
    harness.send(Message::Associations(AssocMsg::Close)).await;
    assert!(
        harness.app.associations.is_none(),
        "Close drops what was not saved"
    );
}
