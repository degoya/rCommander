//! The questions of MCP calls end to end: filed, announced, answered only through `dispatch`,
//! timed out, taken back by a call that ends.

use std::time::Duration;

use tempfile::TempDir;
use tokio::{sync::broadcast::Receiver, task::JoinHandle};

use crate::test_support;
use crate::{
    Command, CommandClass, Engine, EngineEvent, MAX_MCP_QUESTIONS, McpAnswer, McpArgument, McpAsk,
    McpQuestion, McpSessionOffer, McpVerdict, Outcome, PanelId, spec,
};

/// Long enough never to run out in a test that answers.
const PATIENT: Duration = Duration::from_secs(30);

async fn engine() -> (TempDir, Engine) {
    test_support::engine(&[("a.txt", "a")]).await
}

fn ask(tool: &str, class: CommandClass) -> McpAsk {
    McpAsk {
        tool: tool.to_owned(),
        class,
        arguments: vec![McpArgument {
            name: "shell".to_owned(),
            value: "cmd".to_owned(),
        }],
        panel: None,
        cursor: false,
        session: None,
    }
}

/// `ask` with what the gate asks "allow for this session" to grant.
fn offering(tool: &str, class: CommandClass, session: McpSessionOffer) -> McpAsk {
    McpAsk {
        session: Some(session),
        ..ask(tool, class)
    }
}

/// A call asking in the background, as the MCP gate does.
fn asking(engine: &Engine, ask: McpAsk, within: Duration) -> JoinHandle<McpVerdict> {
    let engine = engine.clone();
    tokio::spawn(async move { engine.ask_mcp(ask, within).await })
}

/// The next question announced.
async fn announced(events: &mut Receiver<EngineEvent>) -> McpQuestion {
    loop {
        if let EngineEvent::McpQuestion { question } = events.recv().await.expect("an event") {
            return *question;
        }
    }
}

/// The next question closed.
async fn closed(events: &mut Receiver<EngineEvent>) -> u64 {
    loop {
        if let EngineEvent::McpQuestionClosed { question } = events.recv().await.expect("event") {
            return question;
        }
    }
}

async fn answer(engine: &Engine, question: u64, answer: McpAnswer) -> Result<Outcome, String> {
    engine
        .dispatch(Command::AnswerMcpQuestion { question, answer })
        .await
        .map_err(|error| error.code().to_owned())
}

#[test]
fn only_the_window_answers() {
    assert_eq!(
        spec("mcp.answer").expect("a row").class,
        CommandClass::GuiOnly
    );
}

#[tokio::test]
async fn without_a_window_nobody_is_asked() {
    let (_root, engine) = engine().await;
    let mut events = engine.subscribe();
    let verdict = engine
        .ask_mcp(ask("terminal_open", CommandClass::Exec), PATIENT)
        .await;
    assert_eq!(verdict, McpVerdict::NoWindow);
    assert!(engine.mcp_questions().is_empty());
    assert!(events.try_recv().is_err(), "nothing announced");
}

#[tokio::test]
async fn the_call_waits_for_allow_or_deny() {
    let (_root, engine) = engine().await;
    engine.attach_window();
    let mut events = engine.subscribe();

    let call = asking(&engine, ask("terminal_open", CommandClass::Exec), PATIENT);
    let question = announced(&mut events).await;
    assert_eq!(question.tool, "terminal_open");
    assert_eq!(question.class, CommandClass::Exec);
    assert_eq!(question.arguments[0].value, "cmd");
    assert_eq!(question.panel.panel, PanelId::Left);
    assert_eq!(question.panel.entries, ["a.txt"], "the cursor's entry");
    assert!(!question.offers_session());
    assert_eq!(engine.mcp_questions(), std::slice::from_ref(&question));
    assert!(!call.is_finished(), "the call waits");

    let answered = answer(&engine, question.id, McpAnswer::Deny).await;
    assert_eq!(
        answered,
        Ok(Outcome::McpAnswered {
            question: question.id,
            answer: McpAnswer::Deny
        })
    );
    assert_eq!(call.await.expect("joins"), McpVerdict::Denied);
    assert_eq!(closed(&mut events).await, question.id);
    assert!(engine.mcp_questions().is_empty());

    let call = asking(&engine, ask("terminal_open", CommandClass::Exec), PATIENT);
    let question = announced(&mut events).await;
    answer(&engine, question.id, McpAnswer::Allow)
        .await
        .expect("answers");
    assert_eq!(call.await.expect("joins"), McpVerdict::Allowed);
    assert_eq!(
        answer(&engine, question.id, McpAnswer::Allow).await,
        Err("mcp_question_not_found".to_owned()),
        "an answered question is gone"
    );
}

#[tokio::test]
async fn no_answer_in_time_is_a_no_and_closes_the_dialog() {
    let (_root, engine) = engine().await;
    engine.attach_window();
    let mut events = engine.subscribe();
    let call = asking(
        &engine,
        ask("ops_delete", CommandClass::Destructive),
        Duration::from_millis(50),
    );
    let question = announced(&mut events).await;
    assert_eq!(call.await.expect("joins"), McpVerdict::TimedOut);
    assert_eq!(closed(&mut events).await, question.id);
    assert!(engine.mcp_questions().is_empty());
}

#[tokio::test]
async fn a_call_given_up_takes_its_question_back() {
    let (_root, engine) = engine().await;
    engine.attach_window();
    let mut events = engine.subscribe();
    let call = asking(
        &engine,
        ask("ops_delete", CommandClass::Destructive),
        PATIENT,
    );
    let question = announced(&mut events).await;
    call.abort();
    assert_eq!(closed(&mut events).await, question.id);
    assert!(engine.mcp_questions().is_empty());
}

#[tokio::test]
async fn the_session_is_offered_for_write_and_exec_tools_only() {
    let (_root, engine) = engine().await;
    engine.attach_window();
    let mut events = engine.subscribe();

    // Whatever the gate asks for, a destructive tool is offered no session.
    let destructive = offering(
        "ops_delete",
        CommandClass::Destructive,
        McpSessionOffer::Minutes(30),
    );
    let call = asking(&engine, destructive, PATIENT);
    let question = announced(&mut events).await;
    assert_eq!(
        answer(&engine, question.id, McpAnswer::AllowForSession).await,
        Err("mcp_answer_not_offered".to_owned())
    );
    assert_eq!(engine.mcp_questions().len(), 1, "still open");
    answer(&engine, question.id, McpAnswer::Deny)
        .await
        .expect("answers");
    assert_eq!(call.await.expect("joins"), McpVerdict::Denied);

    let write = offering("ops_mkdir", CommandClass::Write, McpSessionOffer::Session);
    let call = asking(&engine, write, PATIENT);
    let question = announced(&mut events).await;
    assert_eq!(question.session, Some(McpSessionOffer::Session));
    answer(&engine, question.id, McpAnswer::AllowForSession)
        .await
        .expect("answers");
    assert_eq!(call.await.expect("joins"), McpVerdict::AllowedForSession);

    // An executing tool gets its minutes, never the whole session, and none for zero minutes.
    for (asked, offered) in [
        (
            McpSessionOffer::Minutes(30),
            Some(McpSessionOffer::Minutes(30)),
        ),
        (McpSessionOffer::Session, None),
        (McpSessionOffer::Minutes(0), None),
    ] {
        let exec = offering("terminal_send_input", CommandClass::Exec, asked);
        let call = asking(&engine, exec, PATIENT);
        let question = announced(&mut events).await;
        assert_eq!(question.session, offered, "{asked:?}");
        answer(&engine, question.id, McpAnswer::Deny)
            .await
            .expect("answers");
        assert_eq!(call.await.expect("joins"), McpVerdict::Denied);
    }
}

#[tokio::test]
async fn mcp_cannot_answer_its_own_question() {
    let (_root, engine) = engine().await;
    engine.attach_window();
    let mut events = engine.subscribe();
    let call = asking(&engine, ask("terminal_open", CommandClass::Exec), PATIENT);
    let question = announced(&mut events).await;
    let refused = engine
        .dispatch_mcp(Command::AnswerMcpQuestion {
            question: question.id,
            answer: McpAnswer::Allow,
        })
        .await
        .expect_err("GUI only");
    assert_eq!(refused.code(), "gui_only");
    assert_eq!(engine.mcp_questions().len(), 1, "still waiting");
    call.abort();
}

#[tokio::test]
async fn the_number_of_waiting_questions_is_bounded() {
    let (_root, engine) = engine().await;
    engine.attach_window();
    let mut events = engine.subscribe();
    let calls: Vec<_> = (0..MAX_MCP_QUESTIONS)
        .map(|_| {
            asking(
                &engine,
                ask("ops_delete", CommandClass::Destructive),
                PATIENT,
            )
        })
        .collect();
    for _ in 0..MAX_MCP_QUESTIONS {
        announced(&mut events).await;
    }
    let verdict = engine
        .ask_mcp(ask("ops_delete", CommandClass::Destructive), PATIENT)
        .await;
    assert_eq!(verdict, McpVerdict::Busy);
    for call in calls {
        call.abort();
    }
}

/// RC-080-12 (M6): a call that takes the cursor's entry is asked about that entry, not the
/// selection; the folder and every name are shown with direction characters spelled out.
#[tokio::test]
async fn the_question_names_what_the_call_takes_and_shows_names_readably() {
    let root = tempfile::tempdir().expect("temp dir");
    let folder = root.path().join("left\u{202e}txt");
    std::fs::create_dir(&folder).expect("mkdir");
    std::fs::write(folder.join("a.txt"), b"a").expect("write");
    std::fs::write(folder.join("b\u{202e}fdp.exe"), b"b").expect("write");
    let engine = test_support::headless(&folder).await;
    engine
        .dispatch(Command::ToggleSelection {
            panel: None,
            names: vec!["b\u{202e}fdp.exe".to_owned()],
        })
        .await
        .expect("selects");
    engine.attach_window();
    let mut events = engine.subscribe();

    let call = asking(
        &engine,
        ask("ops_delete", CommandClass::Destructive),
        PATIENT,
    );
    let question = announced(&mut events).await;
    assert!(question.panel.selection);
    assert_eq!(question.panel.entries, ["b\\u{202e}fdp.exe"]);
    assert!(
        question.panel.path.ends_with("left\\u{202e}txt"),
        "{}",
        question.panel.path
    );
    call.abort();
    closed(&mut events).await;

    let opening = McpAsk {
        cursor: true,
        ..ask("file_open", CommandClass::Exec)
    };
    let call = asking(&engine, opening, PATIENT);
    let question = announced(&mut events).await;
    assert!(!question.panel.selection, "the cursor, not the selection");
    assert_eq!(question.panel.entries, ["a.txt"]);
    call.abort();
}
