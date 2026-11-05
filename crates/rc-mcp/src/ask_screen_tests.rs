//! RC-080-66: `terminal_read_screen` asks as a destructive call does - the screen shows what was
//! typed there - and "allow for this session" holds on the terminal it was given for.

use std::time::Duration;

use rc_engine::{
    Command, CommandClass, Engine, EngineEvent, McpAnswer, McpQuestion, McpSessionOffer, spec,
};
use serde_json::{Map, Value, json};
use tokio::task::JoinHandle;

use super::{Gate, ask, session_offer};
use crate::{
    Policy, Rule,
    gate_settings::GateSettings,
    test_support::{Built, Fixture},
};

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

fn on(terminal: u64) -> Option<Map<String, Value>> {
    json!({ "terminal": terminal, "scrollback": 10 })
        .as_object()
        .cloned()
}

#[test]
fn the_screen_is_read_as_a_destructive_call_and_never_allowed_outright() {
    let class = spec("terminal.read_screen").expect("a row").class;
    assert_eq!(class, CommandClass::Destructive);
    let loosest = Policy {
        destructive: Rule::Allowed,
        ..Policy::default()
    };
    assert_eq!(
        loosest.decide(class),
        Rule::Ask,
        "the setting cannot drop it"
    );
}

#[tokio::test]
async fn reading_a_screen_offers_the_session_on_that_terminal_only() {
    let built = Fixture::new().build().await;
    let settings = GateSettings::load(&built.db).await.expect("settings");
    let class = CommandClass::Destructive;
    assert_eq!(
        session_offer(&settings, class, "terminal_read_screen", on(3).as_ref()),
        Some((McpSessionOffer::Session, Some("terminal=3".to_owned())))
    );
    let without = json!({ "scrollback": 10 }).as_object().cloned();
    assert_eq!(
        session_offer(&settings, class, "terminal_read_screen", without.as_ref()),
        None,
        "no terminal, nothing to bind a grant to"
    );
    assert_eq!(
        session_offer(&settings, class, "terminal_close", on(3).as_ref()),
        None,
        "any other destructive tool offers none"
    );
}

#[tokio::test]
async fn a_screen_allowed_for_the_session_is_read_without_asking_again() {
    let Built {
        root: _root,
        db,
        mut context,
        ..
    } = Fixture::new().build().await;
    context.answer_within(Duration::from_secs(30));
    let engine = context.engine().clone();
    engine.attach_window();
    let settings = &GateSettings::load(&db).await.expect("settings");
    let class = CommandClass::Destructive;
    let tool = "terminal_read_screen";

    let person_allows = person(&engine, McpAnswer::AllowForSession);
    let first = ask(&context, settings, "s1", tool, class, on(3).as_ref()).await;
    assert!(matches!(first.expect("asks"), Gate::Run(None)), "allowed");
    let question = person_allows.await.expect("joins");
    assert_eq!(question.tool, tool);
    assert_eq!(question.session, Some(McpSessionOffer::Session));

    // Nobody answers now, and the same terminal is read at once.
    let again = ask(&context, settings, "s1", tool, class, on(3).as_ref()).await;
    assert!(
        matches!(again.expect("asks"), Gate::Run(None)),
        "no question"
    );
    assert!(engine.mcp_questions().is_empty());

    // Another terminal, another session: asked again.
    for (session, terminal) in [("s1", 4), ("s2", 3)] {
        let person_denies = person(&engine, McpAnswer::Deny);
        let target = on(terminal);
        let other = ask(&context, settings, session, tool, class, target.as_ref());
        let Gate::Answer(refused) = other.await.expect("asks") else {
            panic!("refused: {session} on {terminal}");
        };
        assert_eq!(refused.is_error, Some(true), "{session} on {terminal}");
        person_denies.await.expect("joins");
    }
}
