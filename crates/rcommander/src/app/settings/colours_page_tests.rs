use rc_engine::settings::COLOUR_RULES_KEY;

use super::{
    RuleOp, hex_of,
    rule_list::{Typed, edit_rule, typed_rules, with_rule_colour, written},
};
use crate::app::{
    Message,
    harness::Harness,
    settings::{self, SettingsMessage, draft::Field},
};

#[test]
fn rules_as_typed_go_back_as_they_came() {
    let text = "*.exe;*.bat\t#c42b1c\t#ff99a4\n*\\\t#107c10";
    let rules = typed_rules(text);
    assert_eq!(rules.len(), 2);
    assert_eq!(
        rules[1],
        ("*\\".to_owned(), "#107c10".to_owned(), String::new())
    );
    assert_eq!(written(&rules), text);
    // A half-typed rule keeps what was typed, for the check on Apply to judge.
    let typing: Vec<Typed> = vec![("*.t".to_owned(), "#12".to_owned(), String::new())];
    assert_eq!(typed_rules(&written(&typing)), typing);
}

/// The dialog's colour lands in the rule it was opened for, light or dark, the others untouched.
#[test]
fn a_chosen_colour_lands_in_its_rule() {
    let text = "*.exe\t#c42b1c\n*.txt\t#0078d4";
    assert_eq!(hex_of([0x10, 0x7c, 0x10]), "#107c10");
    assert_eq!(
        with_rule_colour(text, 1, true, "#4cc2ff".to_owned()),
        "*.exe\t#c42b1c\n*.txt\t#0078d4\t#4cc2ff"
    );
    assert_eq!(
        with_rule_colour(text, 0, false, "#ff0000".to_owned()),
        "*.exe\t#ff0000\n*.txt\t#0078d4"
    );
    // A rule removed while the dialog was open changes nothing.
    assert_eq!(with_rule_colour(text, 5, false, "#ff0000".to_owned()), text);
}

/// RC-080-53: a row's edit names the row and the change; Up and Down at the ends and a row that is
/// gone change nothing.
#[test]
fn an_edit_changes_its_rule_and_leaves_the_others() {
    let text = "*.exe\t#c42b1c\n*.txt\t#0078d4";
    let mask = edit_rule(text, 1, RuleOp::Mask("*.md".into()));
    assert_eq!(mask, "*.exe\t#c42b1c\n*.md\t#0078d4");
    assert_eq!(
        edit_rule(text, 1, RuleOp::Up),
        "*.txt\t#0078d4\n*.exe\t#c42b1c"
    );
    assert_eq!(
        edit_rule(text, 0, RuleOp::Down),
        edit_rule(text, 1, RuleOp::Up)
    );
    assert_eq!(edit_rule(text, 0, RuleOp::Up), text);
    assert_eq!(edit_rule(text, 1, RuleOp::Down), text);
    assert_eq!(edit_rule(text, 0, RuleOp::Remove), "*.txt\t#0078d4");
    assert_eq!(edit_rule(text, 7, RuleOp::Remove), text);
    assert_eq!(
        edit_rule(text, 2, RuleOp::Add),
        format!("{text}\n*.exe\t#c42b1c")
    );
}

/// RC-080-53: a line of blanks is no rule for the engine (`parse_colour_rules` trims it), so the
/// tab shows none for it either, and the rows' places are the engine's.
#[test]
fn a_blank_line_is_no_rule_on_the_tab_either() {
    let text = "*.exe\t#c42b1c\n   \n*.txt\t#0078d4";
    let rules = typed_rules(text);
    assert_eq!(rules.len(), 2);
    assert_eq!(rules[1].0, "*.txt");
    assert_eq!(
        with_rule_colour(text, 1, false, "#107c10".to_owned()),
        "*.exe\t#c42b1c\n*.txt\t#107c10"
    );
}

/// RC-080-53: through `apply`, a rule's edit reaches the draft's rule list.
#[tokio::test]
async fn a_rule_edit_reaches_the_draft() {
    let root = tempfile::tempdir().expect("temp dir");
    let db = rc_db::Db::open_in_memory().await.expect("db");
    let mut harness = Harness::on_db(root.path(), db.clone()).await;
    harness.app.db = Some(db.clone());
    harness.send(Message::Settings(SettingsMessage::Open)).await;
    let loaded = settings::load(harness.app.engine().clone(), db).await;
    harness
        .send(Message::Settings(SettingsMessage::Loaded(
            loaded.map(Box::new),
        )))
        .await;
    for op in [RuleOp::Add, RuleOp::Add, RuleOp::Mask("*.md".into())] {
        harness
            .send(Message::Settings(SettingsMessage::ColourRule(1, op)))
            .await;
    }
    let draft = harness
        .app
        .settings
        .as_ref()
        .and_then(|dialog| dialog.draft.as_ref())
        .expect("the draft");
    let Some(Field::Text(text)) = draft.field(COLOUR_RULES_KEY) else {
        panic!("the rule list");
    };
    assert_eq!(text, "*.exe\t#c42b1c\n*.md\t#c42b1c");
}
