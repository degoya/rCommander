use rc_engine::{CommandClass, ESCALATIONS};
use serde_json::{Value, json};

use super::{READ_TOOLS, class_of, command_of};
use crate::coverage::{COVERAGE, Coverage};

/// The fewest arguments each read tool takes.
fn least(tool: &str) -> Value {
    match tool {
        "login_scripts_create" => json!({ "name": "x" }),
        "login_scripts_update" => json!({ "id": 1 }),
        "diagnostics_set_retention" => json!({ "table": "error_log" }),
        "queue_load" => json!({ "saved": 1 }),
        _ => json!({}),
    }
}

/// Every tool that covers an escalating command is read into its command, and no other: a
/// tool left out would be gated by its command's row alone.
#[test]
fn every_tool_of_an_escalating_command_is_read() {
    let mut covering: Vec<&str> = ESCALATIONS
        .iter()
        .filter_map(|row| {
            COVERAGE.iter().find_map(|(id, coverage)| match coverage {
                Coverage::Covered(tool) if *id == row.id => Some(*tool),
                _ => None,
            })
        })
        .collect();
    covering.sort_unstable();
    assert_eq!(covering, READ_TOOLS);
    for tool in READ_TOOLS {
        let arguments = least(tool);
        let command = command_of(tool, arguments.as_object())
            .unwrap_or_else(|| panic!("{tool} reads its least arguments"));
        let covered = COVERAGE
            .iter()
            .find(|(_, coverage)| *coverage == Coverage::Covered(tool))
            .map(|(id, _)| *id);
        assert_eq!(Some(command.id()), covered, "{tool}");
    }
    assert!(command_of("ops_delete", None).is_none());
}

/// Arguments that do not read leave the tool's own class; the tool then refuses the call.
#[test]
fn arguments_that_do_not_read_keep_the_tools_class() {
    let wrong = json!({ "on_conflict": "overwrite_everything" });
    assert_eq!(
        class_of("ops_copy", wrong.as_object(), CommandClass::Write),
        CommandClass::Write
    );
    let right = json!({ "on_conflict": "overwrite" });
    assert_eq!(
        class_of("ops_copy", right.as_object(), CommandClass::Write),
        CommandClass::Destructive
    );
    // Never lower than the tool's own class.
    assert_eq!(
        class_of("ops_copy", None, CommandClass::GuiOnly),
        CommandClass::GuiOnly
    );
}
