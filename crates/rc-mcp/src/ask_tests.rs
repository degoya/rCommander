//! The question in the window end to end (RC-010-13): an rmcp client over an in-process pipe,
//! a test tool that creates a file when it runs, and the person played by a task that answers
//! through the engine's `dispatch` as the window's dialog does.

use std::{path::Path, time::Duration};

use rc_db::Db;
use rc_engine::{
    Command, CommandClass, Engine, EngineEvent, McpAnswer, McpQuestion, McpSessionOffer,
    settings::MCP_HEADLESS_CODE_KEY,
};
use rmcp::{
    RoleClient, ServiceExt as _, handler::server::wrapper::Parameters, schemars,
    service::RunningService, tool, tool_router,
};
use serde::Deserialize;
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::task::JoinHandle;

use super::{Gate, allow_headless_codes, ask, readable};
use crate::test_support::{Built, Fixture, call};
use crate::{
    McpContext, Policy, RcMcpServer, Rule,
    answer::{ToolResult, json_result},
    coverage::{COVERAGE, Coverage},
    gate_settings::GateSettings,
};

#[derive(Deserialize, schemars::JsonSchema)]
struct RunParams {
    /// The file the tool creates when it runs.
    path: String,
    #[serde(default)]
    #[allow(dead_code, reason = "read by the gate, not by the tool")]
    confirmation: Option<String>,
}

#[tool_router(router = ask_test_router, vis = "pub(crate)")]
impl RcMcpServer {
    #[tool(description = "Test only: an executing tool that creates a file.")]
    async fn test_run(&self, Parameters(params): Parameters<RunParams>) -> ToolResult {
        std::fs::write(&params.path, b"ran").expect("the test tool writes");
        json_result(&json!({ "ran": params.path }))
    }

    #[tool(description = "Test only: a writing tool that creates a file.")]
    async fn test_write(&self, Parameters(params): Parameters<RunParams>) -> ToolResult {
        std::fs::write(&params.path, b"written").expect("the test tool writes");
        json_result(&json!({ "written": params.path }))
    }
}

struct Setup {
    root: TempDir,
    db: Db,
    context: McpContext,
}

impl Setup {
    fn engine(&self) -> &Engine {
        self.context.engine()
    }

    fn target(&self, name: &str) -> String {
        self.root.path().join(name).display().to_string()
    }
}

async fn setup(within: Duration) -> Setup {
    let Built {
        root,
        db,
        mut context,
        ..
    } = Fixture::new().build().await;
    context.classify("test_run", CommandClass::Exec);
    context.classify("test_write", CommandClass::Write);
    context.answer_within(within);
    Setup { root, db, context }
}

async fn connect(context: &McpContext) -> RunningService<RoleClient, ()> {
    let (server_side, client_side) = tokio::io::duplex(64 * 1024);
    let server = RcMcpServer::with_router(
        context.clone(),
        RcMcpServer::router() + RcMcpServer::ask_test_router(),
    );
    tokio::spawn(async move {
        if let Ok(running) = server.serve(server_side).await {
            let _ = running.waiting().await;
        }
    });
    ().serve(client_side).await.expect("client")
}

/// The person: answers the next question in the window with `answer`, and tells which it was.
fn person(engine: &Engine, answer: McpAnswer) -> JoinHandle<McpQuestion> {
    let mut events = engine.subscribe();
    let engine = engine.clone();
    tokio::spawn(async move {
        loop {
            if let EngineEvent::McpQuestion { question } = events.recv().await.expect("event") {
                engine
                    .dispatch(Command::AnswerMcpQuestion {
                        question: question.id,
                        answer,
                    })
                    .await
                    .expect("the window answers");
                return *question;
            }
        }
    })
}

#[tokio::test]
async fn with_the_window_open_the_person_decides() {
    let setup = setup(Duration::from_secs(30)).await;
    setup.engine().attach_window();
    let client = connect(&setup.context).await;

    let declined = setup.target("declined");
    let person_says_no = person(setup.engine(), McpAnswer::Deny);
    let denied = call(&client, "test_run", json!({ "path": declined })).await;
    assert_eq!(denied["code"], "denied_by_user", "{denied}");
    assert!(!Path::new(&declined).exists(), "nothing ran");
    let question = person_says_no.await.expect("joins");
    assert_eq!(question.tool, "test_run");
    assert_eq!(question.class, CommandClass::Exec);
    assert_eq!(question.arguments[0].name, "path");
    assert_eq!(question.arguments[0].value, declined);

    let allowed = setup.target("allowed");
    let person_says_yes = person(setup.engine(), McpAnswer::Allow);
    let ran = call(&client, "test_run", json!({ "path": allowed })).await;
    assert_eq!(ran["ran"], allowed, "{ran}");
    assert!(Path::new(&allowed).exists());
    person_says_yes.await.expect("joins");
}

#[tokio::test]
async fn no_answer_in_time_is_timed_out_and_nothing_runs() {
    let setup = setup(Duration::from_millis(100)).await;
    setup.engine().attach_window();
    let client = connect(&setup.context).await;
    let target = setup.target("late");
    let answer = call(&client, "test_run", json!({ "path": target })).await;
    assert_eq!(answer["code"], "timed_out", "{answer}");
    assert!(!Path::new(&target).exists());
    assert!(setup.engine().mcp_questions().is_empty());
}

#[tokio::test]
async fn a_code_the_agent_submits_is_no_answer_while_the_window_is_open() {
    let setup = setup(Duration::from_millis(100)).await;
    allow_headless_codes(&setup.db).await;
    let client = connect(&setup.context).await;
    let target = setup.target("forged");
    let asked = call(&client, "test_run", json!({ "path": target })).await;
    let code = asked["confirmation"].as_str().expect("a code headless");

    setup.engine().attach_window();
    let answer = call(
        &client,
        "test_run",
        json!({ "path": target, "confirmation": code }),
    )
    .await;
    assert_eq!(answer["code"], "timed_out", "the window asked: {answer}");
    assert!(!Path::new(&target).exists());
}

#[tokio::test]
async fn headless_ask_is_deny_by_default() {
    let setup = setup(Duration::from_secs(30)).await;
    let client = connect(&setup.context).await;
    let target = setup.target("headless");
    let answer = call(&client, "test_run", json!({ "path": target })).await;
    assert_eq!(answer["code"], "headless_ask_denied", "{answer}");
    assert!(answer.get("confirmation").is_none(), "no code: {answer}");
    assert!(!Path::new(&target).exists());
}

#[tokio::test]
async fn no_tool_answers_the_question_or_changes_the_setting() {
    let setup = setup(Duration::from_secs(30)).await;
    let client = connect(&setup.context).await;
    let refused = call(
        &client,
        "settings_set",
        json!({ "key": MCP_HEADLESS_CODE_KEY, "value": true }),
    )
    .await;
    assert_eq!(refused["code"], "setting_protected", "{refused}");
    assert_eq!(
        setup
            .db
            .get::<Value>(MCP_HEADLESS_CODE_KEY)
            .await
            .expect("read"),
        None
    );
    let row = COVERAGE.iter().find(|(id, _)| *id == "mcp.answer");
    assert!(
        matches!(row, Some((_, Coverage::Omitted(_)))),
        "mcp.answer has no tool: {row:?}"
    );
    let tools = RcMcpServer::tool_names();
    assert!(
        !tools.iter().any(|tool| tool.contains("answer")),
        "{tools:?}"
    );
}

#[tokio::test]
async fn a_write_tool_can_be_allowed_for_the_session() {
    let setup = setup(Duration::from_secs(30)).await;
    Policy {
        write: Rule::Ask,
        ..Policy::default()
    }
    .store(&setup.db)
    .await
    .expect("stores");
    setup.engine().attach_window();
    let client = connect(&setup.context).await;

    let person_allows = person(setup.engine(), McpAnswer::AllowForSession);
    let first = setup.target("first");
    let written = call(&client, "test_write", json!({ "path": first })).await;
    assert_eq!(written["written"], first, "{written}");
    assert!(person_allows.await.expect("joins").offers_session());

    // No question this time: nobody answers, and it still runs at once.
    let second = setup.target("second");
    let written = call(&client, "test_write", json!({ "path": second })).await;
    assert_eq!(written["written"], second, "{written}");
    assert!(setup.engine().mcp_questions().is_empty());
}

/// RC-010-27: an executing tool is allowed for the session on the terminal its call names,
/// for the setting's minutes; another terminal asks again, and the window can take it back.
#[tokio::test]
async fn an_exec_tool_can_be_allowed_on_its_target() {
    let setup = setup(Duration::from_secs(30)).await;
    setup.engine().attach_window();
    let context = &setup.context;
    let on = |terminal: u64| {
        json!({ "terminal": terminal, "text": "ls" })
            .as_object()
            .cloned()
    };
    let class = CommandClass::Exec;
    let settings = &GateSettings::load(&setup.db).await.expect("settings");

    let person_allows = person(setup.engine(), McpAnswer::AllowForSession);
    let first = ask(
        context,
        settings,
        "s1",
        "terminal_send_input",
        class,
        on(3).as_ref(),
    )
    .await;
    assert!(matches!(first.expect("asks"), Gate::Run(None)), "allowed");
    let question = person_allows.await.expect("joins");
    assert_eq!(question.session, Some(McpSessionOffer::Minutes(30)));

    // Nobody answers now, and the same terminal still goes ahead.
    let again = ask(
        context,
        settings,
        "s1",
        "terminal_send_input",
        class,
        on(3).as_ref(),
    )
    .await;
    assert!(
        matches!(again.expect("asks"), Gate::Run(None)),
        "no question"
    );
    assert!(setup.engine().mcp_questions().is_empty());

    for (session, terminal) in [("s1", 4), ("s2", 3)] {
        let person_denies = person(setup.engine(), McpAnswer::Deny);
        let arguments = on(terminal);
        let other = ask(
            context,
            settings,
            session,
            "terminal_send_input",
            class,
            arguments.as_ref(),
        );
        let Gate::Answer(refused) = other.await.expect("asks") else {
            panic!("refused: {session} on {terminal}");
        };
        assert_eq!(refused.is_error, Some(true), "{session} on {terminal}");
        person_denies.await.expect("joins");
    }

    setup
        .engine()
        .dispatch(Command::RevokeMcpGrants {})
        .await
        .expect("the window revokes");
    let person_denies = person(setup.engine(), McpAnswer::Deny);
    let revoked = ask(
        context,
        settings,
        "s1",
        "terminal_send_input",
        class,
        on(3).as_ref(),
    )
    .await;
    assert!(
        matches!(revoked.expect("asks"), Gate::Answer(_)),
        "asked again, and denied"
    );
    person_denies.await.expect("joins");
}

/// RC-080-03: filler arguments cannot push the one that matters out of the question - a call
/// with more arguments than the window shows is refused before anyone is asked.
#[tokio::test]
async fn a_call_with_more_arguments_than_the_question_shows_is_refused() {
    let setup = setup(Duration::from_secs(30)).await;
    setup.engine().attach_window();
    let client = connect(&setup.context).await;
    let target = setup.target("hidden");
    let mut arguments = json!({ "path": target });
    for at in 0..super::MAX_ARGUMENTS {
        arguments[format!("a{at:02}")] = json!("filler");
    }
    let refused = call(&client, "test_run", arguments).await;
    assert_eq!(refused["code"], "invalid_params", "{refused}");
    assert!(!Path::new(&target).exists(), "nothing ran");
    assert!(
        setup.engine().mcp_questions().is_empty(),
        "nobody was asked"
    );
}

/// RC-080-12 (M12): a value the question would clip - text for a terminal, script steps, a list
/// of sources - is refused before anyone is asked, in the window and headless alike.
#[tokio::test]
async fn a_value_longer_than_the_question_shows_is_refused_before_asking() {
    let setup = setup(Duration::from_secs(30)).await;
    allow_headless_codes(&setup.db).await;
    let client = connect(&setup.context).await;
    let target = setup.target("long");
    let sources: Vec<String> = (0..200).map(|at| format!("file-{at:04}.txt")).collect();
    for filler in [
        json!("x".repeat(super::MAX_VALUE_CHARS + 1)),
        json!(sources),
    ] {
        let headless = call(
            &client,
            "test_run",
            json!({ "path": target, "text": filler }),
        )
        .await;
        assert_eq!(headless["code"], "invalid_params", "{headless}");
        assert!(
            headless.get("confirmation").is_none(),
            "no code: {headless}"
        );
    }
    setup.engine().attach_window();
    let in_window = call(
        &client,
        "test_run",
        json!({ "path": target, "text": "y".repeat(super::MAX_VALUE_CHARS + 1) }),
    )
    .await;
    assert_eq!(in_window["code"], "invalid_params", "{in_window}");
    assert!(!Path::new(&target).exists(), "nothing ran");
    assert!(setup.engine().mcp_questions().is_empty(), "nobody asked");

    // Exactly what a question shows still is asked.
    let person_says_no = person(setup.engine(), McpAnswer::Deny);
    let fits = "z".repeat(super::MAX_VALUE_CHARS);
    let denied = call(&client, "test_run", json!({ "path": target, "text": fits })).await;
    assert_eq!(denied["code"], "denied_by_user", "{denied}");
    let question = person_says_no.await.expect("joins");
    assert!(
        question
            .arguments
            .iter()
            .any(|argument| argument.value == fits)
    );
}

/// RC-080-12 (M6): `file_open` without a path opens the cursor's entry, so its question names
/// that entry even while others are selected; a path given takes neither.
#[test]
fn file_open_without_a_path_is_asked_about_the_cursor() {
    let path = json!({ "path": "a.txt" });
    let null = json!({ "path": null });
    assert!(super::takes_the_cursor("file_open", None));
    assert!(super::takes_the_cursor("file_open", null.as_object()));
    assert!(!super::takes_the_cursor("file_open", path.as_object()));
    assert!(!super::takes_the_cursor("ops_delete", None));
}

#[test]
fn the_question_shows_hidden_characters_and_clips_long_values() {
    assert_eq!(readable("a\rb"), "a\\rb");
    assert_eq!(readable("evil\u{202e}txt.exe"), "evil\\u{202e}txt.exe");
    assert_eq!(readable("\u{1b}[2J"), "\\u{1b}[2J");
    let long = "x".repeat(1005);
    assert!(readable(&long).ends_with(" ... (+5 characters)"));
}

#[tokio::test]
async fn the_question_shows_arguments_masked_and_without_the_code() {
    let setup = setup(Duration::from_secs(1)).await;
    let arguments = json!({
        "target": "sftp://bob:hunter2@host.example/home",
        "confirmation": "0123",
        "count": 3,
    });
    let shown = super::shown(arguments.as_object(), setup.engine());
    let value = |name: &str| {
        shown
            .iter()
            .find(|argument| argument.name == name)
            .map(|argument| argument.value.clone())
    };
    assert_eq!(shown.len(), 2, "{shown:?}");
    assert_eq!(value("confirmation"), None);
    assert_eq!(value("count").as_deref(), Some("3"));
    let target = value("target").expect("shown");
    assert!(!target.contains("hunter2"), "{target}");
    assert!(target.contains("host.example"), "{target}");
}
