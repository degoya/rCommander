//! RC-110-60: the hints each tool's class sets, the tools a policy leaves out of `tools/list`,
//! and the length of a description.

use rc_engine::CommandClass;
use serde_json::json;

use super::{annotations, listed, tool_classes};
use crate::test_support::{Fixture, call};
use crate::{Policy, RcMcpServer, Rule};

/// The longest description a tool may have; details go into the instructions.
const DESCRIPTION_MAX: usize = 1500;

#[test]
fn each_class_sets_its_hints() {
    let read = annotations(CommandClass::Read);
    assert_eq!(read.read_only_hint, Some(true));
    let write = annotations(CommandClass::Write);
    assert_eq!(write.read_only_hint, Some(false));
    assert_eq!(write.destructive_hint, Some(false));
    assert_eq!(write.open_world_hint, None);
    let destructive = annotations(CommandClass::Destructive);
    assert_eq!(destructive.destructive_hint, Some(true));
    let exec = annotations(CommandClass::Exec);
    assert_eq!(exec.destructive_hint, Some(true));
    assert_eq!(exec.open_world_hint, Some(true));
}

#[test]
fn every_tool_carries_the_hints_of_its_strictest_reach() {
    let tools = RcMcpServer::tools();
    assert!(tools.len() > 100, "{}", tools.len());
    let hints = |name: &str| {
        tools
            .iter()
            .find(|tool| tool.name == name)
            .and_then(|tool| tool.annotations.clone())
            .unwrap_or_else(|| panic!("{name} has hints"))
    };
    for tool in &tools {
        assert!(tool.annotations.is_some(), "{} has no hints", tool.name);
    }
    assert_eq!(hints("list_dir").read_only_hint, Some(true));
    assert_eq!(hints("ops_mkdir").destructive_hint, Some(false));
    // A copy may overwrite (`rc_engine::ESCALATIONS`): not hinted as harmless.
    assert_eq!(hints("ops_copy").destructive_hint, Some(true));
    assert_eq!(hints("ops_delete").destructive_hint, Some(true));
    assert_eq!(hints("ssh_exec").open_world_hint, Some(true));
}

#[test]
fn the_saved_filters_are_written_and_deleted_not_read() {
    let classes = tool_classes();
    assert_eq!(classes.get("filters_save"), Some(&CommandClass::Write));
    assert_eq!(classes.get("filters_rename"), Some(&CommandClass::Write));
    assert_eq!(
        classes.get("filters_delete"),
        Some(&CommandClass::Destructive)
    );
    assert_eq!(classes.get("filters_list"), Some(&CommandClass::Read));
}

#[test]
fn a_denied_class_is_left_out_of_the_list() {
    let classes = tool_classes();
    let names = |policy: &Policy| -> Vec<String> {
        listed(RcMcpServer::tools(), &classes, policy)
            .into_iter()
            .map(|tool| tool.name.into_owned())
            .collect()
    };
    let all = RcMcpServer::tool_names();
    let default = names(&Policy::default());
    let gui_only: Vec<&String> = all
        .iter()
        .filter(|tool| classes.get(tool.as_str()) == Some(&CommandClass::GuiOnly))
        .collect();
    assert_eq!(default.len() + gui_only.len(), all.len(), "{gui_only:?}");

    let no_exec = names(&Policy {
        exec: Rule::Deny,
        ..Policy::default()
    });
    assert!(!no_exec.iter().any(|tool| tool == "ssh_exec"));
    assert!(no_exec.iter().any(|tool| tool == "ops_copy"));
    let read_only = names(&Policy {
        write: Rule::Deny,
        destructive: Rule::Deny,
        exec: Rule::Deny,
        ..Policy::default()
    });
    assert!(read_only.iter().any(|tool| tool == "list_dir"));
    assert!(!read_only.iter().any(|tool| tool == "ops_copy"));
    assert!(!read_only.iter().any(|tool| tool == "filters_save"));
}

#[tokio::test]
async fn tools_list_follows_the_policy_and_a_hidden_tool_stays_refused() {
    let built = Fixture::new().build().await;
    let agent = built.client().await;
    let listed_names = |tools: Vec<rmcp::model::Tool>| -> Vec<String> {
        tools
            .into_iter()
            .map(|tool| tool.name.into_owned())
            .collect()
    };
    let before = listed_names(agent.list_all_tools().await.expect("tools"));
    assert!(before.iter().any(|tool| tool == "ops_copy"), "{before:?}");

    Policy {
        write: Rule::Deny,
        ..Policy::default()
    }
    .store(&built.db)
    .await
    .expect("the person denies writing");
    let after = listed_names(agent.list_all_tools().await.expect("tools"));
    assert!(!after.iter().any(|tool| tool == "ops_copy"), "{after:?}");
    assert!(after.iter().any(|tool| tool == "list_dir"), "{after:?}");

    let refused = call(&agent, "ops_mkdir", json!({ "path": "new" })).await;
    assert_eq!(refused["code"], "policy_denied", "{refused}");
    assert!(!built.root.path().join("new").exists());
}

#[test]
fn no_description_is_longer_than_the_limit() {
    for tool in RcMcpServer::tools() {
        let length = tool
            .description
            .as_deref()
            .map_or(0, |text| text.chars().count());
        assert!(
            length <= DESCRIPTION_MAX,
            "{} has a description of {length} characters; move details into instructions.md",
            tool.name
        );
    }
}
