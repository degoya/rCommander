use super::*;
use crate::{
    associations::{AssociationVerb, Associations},
    user_commands::CommandRef,
};

fn program(command: &str) -> CommandRef {
    CommandRef {
        command: command.into(),
        params: "%1".into(),
        ..CommandRef::default()
    }
}

fn rule(mask: &str, open: Option<&str>, verbs: &[&str]) -> FileAssociation {
    FileAssociation {
        mask: mask.into(),
        open: open.map(program),
        verbs: verbs
            .iter()
            .map(|name| AssociationVerb {
                name: (*name).into(),
                action: program(&format!("{mask}-{name}")),
            })
            .collect(),
        ..FileAssociation::default()
    }
}

fn opened<'a>(rules: &'a [FileAssociation], name: &str) -> Option<&'a str> {
    open_for(rules, name, is_archive_name(name)).map(|action| action.command.as_str())
}

#[test]
fn the_first_rule_with_open_wins() {
    let rules = [
        rule("*.log|debug*", Some("first"), &[]),
        rule("*.log *.txt", None, &["view"]),
        rule("*.*", Some("any"), &[]),
    ];
    assert_eq!(
        opened(&rules, "app.LOG"),
        Some("first"),
        "case does not count"
    );
    assert_eq!(
        opened(&rules, "debug.log"),
        Some("any"),
        "excluded by the first"
    );
    assert_eq!(
        opened(&rules, "notes.txt"),
        Some("any"),
        "the second has no open"
    );
    assert_eq!(opened(&[], "notes.txt"), None);
}

#[test]
fn an_archive_needs_its_extension_named() {
    assert!(is_archive_name("pack.zip"));
    assert!(!is_archive_name("notes.txt"));
    assert!(names_extension("*.zip", "pack.ZIP"));
    assert!(names_extension("*.txt;pack.zip", "pack.zip"));
    assert!(!names_extension("*", "pack.zip"));
    assert!(!names_extension("*.*", "pack.zip"));
    assert!(!names_extension("*.z?p", "pack.zip"));
    assert!(
        !names_extension("|*.zip", "pack.zip"),
        "an exclusion names nothing"
    );
    let every = [rule("*.*", Some("any"), &[])];
    assert_eq!(opened(&every, "pack.zip"), None);
    assert_eq!(opened(&every, "notes.txt"), Some("any"));
    let named = [rule("*.zip", Some("zipper"), &[])];
    assert_eq!(opened(&named, "pack.zip"), Some("zipper"));
}

#[test]
fn the_menu_offers_the_first_rule_or_every_matching_one() {
    let rules = [
        rule("*.log", Some("tail"), &["view", "edit"]),
        rule("*.*", None, &["EDIT", "print"]),
    ];
    assert_eq!(
        commands_for(&rules, "a.log", false, false),
        ["open", "view", "edit"]
    );
    assert_eq!(
        commands_for(&rules, "a.log", false, true),
        ["open", "view", "edit", "print"]
    );
    assert_eq!(
        commands_for(&rules, "a.txt", false, false),
        ["EDIT", "print"]
    );
    let print = command_for(&rules, "a.log", false, true, "Print").map(|a| a.command.as_str());
    assert_eq!(print, Some("*.*-print"));
    assert_eq!(command_for(&rules, "a.log", false, false, "print"), None);
    let open = command_for(&rules, "a.log", false, false, "OPEN").map(|a| a.command.as_str());
    assert_eq!(open, Some("tail"));
    let associations = Associations {
        rules: rules.to_vec(),
        enabled: false,
        all_matches: true,
    };
    assert!(
        associations.commands_for("a.log").is_empty(),
        "switched off"
    );
    assert!(associations.command_for("a.log", "view").is_none());
}

#[test]
fn percent_one_and_two_are_the_file_and_the_marked_files() {
    assert_eq!(file_params("%1"), "\"%P%N\"");
    assert_eq!(file_params("\"%1\""), "\"%P%N\"");
    assert_eq!(file_params("-n \"%2\" x"), "-n %P%S x");
    assert_eq!(file_params("%2"), "%P%S");
    assert_eq!(file_params("100%% %1"), "100%% \"%P%N\"");
    assert_eq!(file_params("\"a b\" %N"), "\"a b\" %N");
    assert_eq!(file_params("%%1"), "%%1");
}
