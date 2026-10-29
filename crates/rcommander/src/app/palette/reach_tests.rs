use rc_engine::{COMMANDS, Command, Keymap};

use super::*;

fn ids() -> Vec<&'static str> {
    COMMANDS.iter().map(|spec| spec.id).collect()
}

/// The check of RC-080-05: every command of the registry has a key, or a row that says how the
/// keyboard reaches it - or why it does not.
#[test]
fn every_command_is_reached_or_says_why_not() {
    let problems = problems(&ids(), &Keymap::standard(), &standard());
    assert!(problems.is_empty(), "{problems:#?}");
}

/// A new command registered without a key and without a row fails the check, and so does a row
/// left behind when a command gets its key.
#[test]
fn a_command_without_key_and_row_fails() {
    let mut ids = ids();
    ids.push("panel.zz_new");
    let problems = problems(&ids, &Keymap::standard(), &standard());
    assert_eq!(
        problems,
        ["panel.zz_new has no key and no row in reach.json"]
    );

    let mut rows = standard();
    rows.push(ReachRow {
        command: "panel.refresh".into(),
        reach: Reach::Palette,
    });
    let problems = problems_of(&rows);
    assert_eq!(
        problems,
        ["panel.refresh has a key: drop its row from reach.json"]
    );
}

fn problems_of(rows: &[ReachRow]) -> Vec<String> {
    problems(&ids(), &Keymap::standard(), rows)
}

#[test]
fn a_via_must_lead_to_a_key() {
    let mut rows = standard();
    let row = rows
        .iter_mut()
        .find(|row| row.command == "hotlist.open")
        .expect("a row");
    row.reach = Reach::Via {
        target: "vault.change_password".into(),
        how: "the comparison dialog".into(),
    };
    assert_eq!(
        problems_of(&rows),
        ["hotlist.open: vault.change_password is not reached by a key"]
    );
    let row = rows
        .iter_mut()
        .find(|row| row.command == "hotlist.open")
        .expect("a row");
    row.reach = Reach::Via {
        target: "window:menu_bar".into(),
        how: "a menu bar that is not there".into(),
    };
    assert_eq!(
        problems_of(&rows),
        ["hotlist.open: window:menu_bar is not reached by a key"]
    );
    // Two rows that lead only to each other are a loop, not a way in.
    let mut rows = standard();
    for (command, target) in [
        ("hotlist.open", "hotlist.add"),
        ("hotlist.add", "hotlist.open"),
    ] {
        let row = rows
            .iter_mut()
            .find(|row| row.command == command)
            .expect("a row");
        row.reach = Reach::Via {
            target: target.into(),
            how: "each through the other".into(),
        };
    }
    assert_eq!(problems_of(&rows).len(), 2);
}

#[test]
fn every_palette_row_runs_without_parameters() {
    let palette: Vec<ReachRow> = standard()
        .into_iter()
        .filter(|row| row.reach == Reach::Palette)
        .collect();
    assert!(!palette.is_empty());
    for row in &palette {
        let command = palette_command(&row.command).expect("builds");
        assert_eq!(command.id(), row.command);
    }
    assert_eq!(palette_command("vault.lock"), Some(Command::LockVault));
    assert_eq!(palette_command("panel.navigate"), None, "needs a path");
}

#[test]
fn a_row_says_one_way() {
    for text in [
        r#"[{ "command": "vault.lock" }]"#,
        r#"[{ "command": "vault.lock", "palette": true, "omitted": "and also not" }]"#,
        r#"[{ "command": "vault.lock", "via": "connections.manager" }]"#,
        r#"[{ "command": "vault.lock", "how": "somehow" }]"#,
        r#"[{ "command": "vault.lock", "palette": true, "colour": "red" }]"#,
    ] {
        assert!(parse(text).is_err(), "{text}");
    }
    assert_eq!(
        parse(r#"[{ "command": "vault.lock", "shown": "the vault line" }]"#),
        Ok(vec![ReachRow {
            command: "vault.lock".into(),
            reach: Reach::Shown("the vault line".into()),
        }])
    );
}

/// The commands no key, palette row, dialog or question reaches, each for the reason its row in
/// `reach.json` gives (RC-080-16): read-outs and settings MCP has for diagnosis, what the window
/// does by itself, and the dialogs still to come.
const WITHOUT_A_WAY_IN: &[&str] = &[
    "agent.list_identities",
    "connections.session_prefs",
    "connections.set_session_prefs",
    "diagnostics.clear_error_log",
    "diagnostics.error_log",
    "queue.assign",
    "queue.move",
    "queue.remove",
    "terminal.send_input",
    "vault.change_password",
];

/// RC-080-16: no command stays without a way in but the reasoned exceptions above - a new
/// `omitted` row fails until the exception is written here too, and an exception that got a way
/// in is dropped from the list.
#[test]
fn only_the_reasoned_exceptions_have_no_way_in() {
    let rows = standard();
    let mut omitted: Vec<&str> = rows
        .iter()
        .filter(|row| matches!(row.reach, Reach::Omitted(_)))
        .map(|row| row.command.as_str())
        .collect();
    omitted.sort_unstable();
    assert_eq!(omitted, WITHOUT_A_WAY_IN);
}
