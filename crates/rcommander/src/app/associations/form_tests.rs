use rc_engine::{AssociationVerb, CommandKind, CommandRef, FileAssociation};

use super::{AssocWindow, RuleMsg};
use crate::app::user_commands::CmdMsg;

fn rule(mask: &str, description: &str) -> FileAssociation {
    FileAssociation {
        mask: mask.into(),
        description: description.into(),
        ..FileAssociation::default()
    }
}

fn masks(window: &AssocWindow) -> Vec<&str> {
    window.rules.iter().map(|rule| rule.mask.as_str()).collect()
}

fn window() -> AssocWindow {
    AssocWindow::new(vec![
        rule("*.log", "Logs"),
        rule("*.md", "Markdown"),
        rule("*.txt", "Text"),
    ])
}

#[test]
fn the_filter_reads_masks_and_descriptions_and_keeps_the_cursor_on_a_shown_rule() {
    let mut window = window();
    assert_eq!(window.cursor, Some(0));
    window.set_filter("MARK".into());
    assert_eq!(window.shown(), [1]);
    assert_eq!(window.cursor, Some(1));
    window.set_filter("nothing".into());
    assert!(window.shown().is_empty());
    assert_eq!(window.cursor, None);
    window.set_filter(String::new());
    assert_eq!(window.shown(), [0, 1, 2]);
    window.step(isize::MAX);
    assert_eq!(window.cursor, Some(2));
    window.step(-1);
    assert_eq!(window.cursor, Some(1));
}

/// Ctrl+Up and Ctrl+Down: the order the rules are tried in; the cursor goes along.
#[test]
fn moving_changes_the_order_the_rules_are_tried_in() {
    let mut window = window();
    window.move_rule(true);
    assert_eq!(
        masks(&window),
        ["*.log", "*.md", "*.txt"],
        "the first stays"
    );
    assert!(!window.dirty);
    window.move_rule(false);
    assert_eq!(masks(&window), ["*.md", "*.log", "*.txt"]);
    assert_eq!(window.cursor, Some(1));
    assert!(window.dirty);
    window.select(Some(2));
    window.move_rule(false);
    assert_eq!(masks(&window), ["*.md", "*.log", "*.txt"], "the last stays");
}

#[test]
fn a_new_rule_follows_the_cursor_and_a_deleted_one_leaves_it_on_the_next() {
    let mut window = window();
    window.set_filter("log".into());
    window.add();
    assert_eq!(masks(&window), ["*.log", "", "*.md", "*.txt"]);
    assert_eq!(window.cursor, Some(1));
    assert!(window.filter.is_empty(), "the new rule shows");
    window.delete();
    assert_eq!(masks(&window), ["*.log", "*.md", "*.txt"]);
    assert_eq!(window.cursor, Some(1));
    window.select(Some(2));
    window.delete();
    assert_eq!(window.cursor, Some(1), "the one before the last");
}

/// `open` first, named commands after it; each edit goes to the command shown.
#[test]
fn the_form_edits_open_and_the_named_commands() {
    let mut window = window();
    window.edit(RuleMsg::Cmd(CmdMsg::Command("notepad++.exe".into())));
    window.edit(RuleMsg::Cmd(CmdMsg::Params("\"%1\"".into())));
    let open = window.rules[0].open.clone().expect("open set");
    assert_eq!(
        (open.command.as_str(), open.params.as_str()),
        ("notepad++.exe", "\"%1\"")
    );
    window.edit(RuleMsg::AddCommand);
    assert_eq!(window.command, 1);
    window.edit(RuleMsg::Name("view".into()));
    window.edit(RuleMsg::Cmd(CmdMsg::Kind(CommandKind::Internal)));
    window.edit(RuleMsg::Cmd(CmdMsg::Command("view_file".into())));
    assert_eq!(
        window.rules[0].verbs,
        [AssociationVerb {
            name: "view".into(),
            action: CommandRef {
                kind: CommandKind::Internal,
                command: "view_file".into(),
                ..CommandRef::default()
            },
        }]
    );
    assert_eq!(window.command_names(), ["open", "view"]);
    assert_eq!(window.action().command, "view_file");
    window.edit(RuleMsg::Command(0));
    assert_eq!(window.action().command, "notepad++.exe");
    window.edit(RuleMsg::RemoveCommand);
    assert_eq!(window.rules[0].open, None, "open emptied");
    window.edit(RuleMsg::Command(1));
    window.edit(RuleMsg::RemoveCommand);
    assert!(window.rules[0].verbs.is_empty());
    assert_eq!(window.command, 0);
    window.edit(RuleMsg::Mask("*.log *.txt".into()));
    assert_eq!(window.rules[0].mask, "*.log *.txt");
}
