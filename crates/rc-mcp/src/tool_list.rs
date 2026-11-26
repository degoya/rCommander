//! The tool list as `tools/list` gives it (RC-110-60): each tool's class, the hints its class
//! sets, and the tools the policy leaves out.
//!
//! A tool's class is the strictest class of the commands it covers (the coverage rows and the
//! registry, [`tool_classes`]); the gate holds every call to it, never lower. The MCP hints
//! (`readOnlyHint`, `destructiveHint`, `openWorldHint`) are set here for every tool at once
//! ([`annotate`]), from the strictest class a call of the tool can reach - its own, or the one an
//! argument escalates it to (`rc_engine::ESCALATIONS`): an overwriting `ops_copy` loses data, so
//! the tool is not hinted as harmless. They are hints for the client; the gate decides.
//!
//! `tools/list` leaves out the tools whose class the policy in force denies ([`listed`]): an
//! agent is not shown what it cannot call. A call of one is still refused with `policy_denied`,
//! and the list is read anew with each request, so a changed policy shows in the next one.

use std::collections::HashMap;

use rc_engine::{CommandClass, escalation, spec};
use rmcp::{
    handler::server::router::tool::ToolRouter,
    model::{Tool, ToolAnnotations},
};

use crate::{
    RcMcpServer,
    coverage::{COVERAGE, Coverage},
    policy::{Policy, Rule},
};

/// Each served tool with a class, from `class_of(command id, the command's class)` for every
/// command it covers; a tool that covers two commands is held to the stricter class.
fn classes_by(
    class_of: impl Fn(&str, CommandClass) -> CommandClass,
) -> HashMap<String, CommandClass> {
    COVERAGE
        .iter()
        .filter_map(|(id, coverage)| match coverage {
            Coverage::Covered(tool) => {
                spec(id).map(|spec| ((*tool).to_owned(), class_of(spec.id, spec.class)))
            }
            Coverage::Omitted(_) => None,
        })
        .fold(HashMap::new(), |mut classes, (tool, class)| {
            classes
                .entry(tool)
                .and_modify(|held: &mut CommandClass| *held = held.stricter(class))
                .or_insert(class);
            classes
        })
}

/// Each tool's class: the strictest class of the commands it covers, as the gate holds it.
#[must_use]
pub fn tool_classes() -> HashMap<String, CommandClass> {
    classes_by(|_, class| class)
}

/// Each tool's strictest reach: its class, or the class an argument escalates a call to.
fn hint_classes() -> HashMap<String, CommandClass> {
    classes_by(|id, class| escalation(id).map_or(class, |row| class.stricter(row.class)))
}

/// The MCP hints of a tool that can reach `class`. Only an executing tool is hinted to reach
/// outside: a reading or writing one reaches a server only by the location it is given, and
/// leaving `openWorldHint` out reads as the specification's default (true), never as a promise.
#[must_use]
pub(crate) fn annotations(class: CommandClass) -> ToolAnnotations {
    let hints = ToolAnnotations::new();
    match class {
        CommandClass::Read => hints.read_only(true),
        CommandClass::Write => hints.read_only(false).destructive(false),
        CommandClass::Destructive | CommandClass::GuiOnly => {
            hints.read_only(false).destructive(true)
        }
        CommandClass::Exec => hints.read_only(false).destructive(true).open_world(true),
    }
}

/// `router` with every tool's hints set from its class: one place, no entry per tool.
pub(crate) fn annotate(mut router: ToolRouter<RcMcpServer>) -> ToolRouter<RcMcpServer> {
    let hints = hint_classes();
    for route in router.map.values_mut() {
        if let Some(&class) = hints.get(route.attr.name.as_ref()) {
            route.attr.annotations = Some(annotations(class));
        }
    }
    router
}

/// `tools` without those whose class `policy` denies (a `gui_only` one always); a tool without a
/// class stays, the gate refuses its calls (`tool_unclassified`).
pub(crate) fn listed(
    tools: Vec<Tool>,
    classes: &HashMap<String, CommandClass>,
    policy: &Policy,
) -> Vec<Tool> {
    tools
        .into_iter()
        .filter(|tool| {
            classes
                .get(tool.name.as_ref())
                .is_none_or(|class| policy.decide(*class) != Rule::Deny)
        })
        .collect()
}

#[cfg(test)]
#[path = "tool_list_tests.rs"]
mod tests;
