//! The host-key gate end to end: questions, the TOFU answers, the separate replace action.

use rc_db::Db;
use tokio::sync::broadcast::Receiver;

use crate::test_support;
use crate::test_support::engine_with_db;
use crate::{
    Command, Engine, EngineError, EngineEvent, HostKeyAnswer, HostKeyQuestion, HostKeyQuestionKind,
    OfferedKey, Outcome,
};

const ED25519: &str = include_str!("../../rc-remote/tests/fixtures/known_hosts/ed25519.pub");
const OTHER: &str = include_str!("../../rc-remote/tests/fixtures/known_hosts/other.pub");

fn key(pub_line: &str) -> OfferedKey {
    let mut fields = pub_line.split_whitespace();
    OfferedKey::from_openssh(fields.next().expect("type"), fields.next().expect("key"))
        .expect("a valid key")
}

/// The question the last check filed, from the event stream.
fn asked(events: &mut Receiver<EngineEvent>) -> HostKeyQuestion {
    loop {
        match events.try_recv().expect("an event") {
            EngineEvent::HostKeyQuestion { question } => return *question,
            _ => continue,
        }
    }
}

async fn answer(
    engine: &Engine,
    question: u64,
    answer: HostKeyAnswer,
) -> Result<Outcome, EngineError> {
    engine
        .dispatch(Command::AnswerHostKey { question, answer })
        .await
}

#[tokio::test]
async fn without_a_database_nothing_is_trusted() {
    let (_root, engine) = test_support::engine(&[]).await;
    let error = engine
        .check_host_key("box.example", 22, key(ED25519))
        .await
        .expect_err("no store, no trust");
    assert_eq!(error.code(), "database_unavailable");
    assert!(engine.attach_db(Db::open_in_memory().await.expect("db")));
    assert!(!engine.attach_db(Db::open_in_memory().await.expect("db")));
}

#[tokio::test]
async fn an_unknown_key_fails_with_its_fingerprint_and_files_a_question() {
    let (_root, engine) = engine_with_db(&[]).await;
    let mut events = engine.subscribe();
    let error = engine
        .check_host_key("Box.Example", 2222, key(ED25519))
        .await
        .expect_err("unknown");
    assert_eq!(error.code(), "host_key_unknown");
    let message = error.to_string();
    assert!(message.contains(&key(ED25519).fingerprint), "{message}");
    assert!(message.contains("rCommander window"), "{message}");

    let question = asked(&mut events);
    assert_eq!(question.host, "box.example");
    assert_eq!(question.port, 2222);
    assert_eq!(question.kind, HostKeyQuestionKind::Unknown);
    assert_eq!(engine.host_key_questions(), vec![question.clone()]);

    // Asking again for the same key does not stack a second question.
    let _ = engine
        .check_host_key("box.example", 2222, key(ED25519))
        .await;
    assert_eq!(asked(&mut events).id, question.id);
    assert_eq!(engine.host_key_questions().len(), 1);
}

#[tokio::test]
async fn connect_once_allows_one_retry_and_stores_nothing() {
    let (_root, engine) = engine_with_db(&[]).await;
    let mut events = engine.subscribe();
    let _ = engine.check_host_key("box.example", 22, key(ED25519)).await;
    let question = asked(&mut events);
    let outcome = answer(&engine, question.id, HostKeyAnswer::ConnectOnce)
        .await
        .expect("answered");
    assert_eq!(
        outcome,
        Outcome::HostKeyAnswered {
            question: question.id,
            answer: HostKeyAnswer::ConnectOnce,
            stored: None,
        }
    );
    assert!(engine.host_key_questions().is_empty());
    // A grant is for this host only.
    assert!(
        engine
            .check_host_key("box.example", 2222, key(ED25519))
            .await
            .is_err()
    );
    engine
        .check_host_key("box.example", 22, key(ED25519))
        .await
        .expect("the retry");
    assert!(
        engine
            .check_host_key("box.example", 22, key(ED25519))
            .await
            .is_err(),
        "spent by the retry"
    );
    let db = engine.db().expect("db");
    assert!(db.known_hosts().await.expect("list").is_empty());
}

#[tokio::test]
async fn trust_and_save_stores_the_key() {
    let (_root, engine) = engine_with_db(&[]).await;
    let mut events = engine.subscribe();
    let _ = engine.check_host_key("box.example", 22, key(ED25519)).await;
    let question = asked(&mut events);
    let Outcome::HostKeyAnswered {
        stored: Some(stored),
        ..
    } = answer(&engine, question.id, HostKeyAnswer::TrustAndSave)
        .await
        .expect("answered")
    else {
        panic!("a stored key");
    };
    assert_eq!(stored.fingerprint_sha256, key(ED25519).fingerprint);
    for _ in 0..2 {
        engine
            .check_host_key("box.example", 22, key(ED25519))
            .await
            .expect("trusted");
    }
    let error = answer(&engine, question.id, HostKeyAnswer::TrustAndSave)
        .await
        .expect_err("answered already");
    assert_eq!(error.code(), "host_key_question_not_found");
}

#[tokio::test]
async fn a_changed_key_takes_no_yes_in_the_dialog_only_the_replace_action() {
    let (_root, engine) = engine_with_db(&[]).await;
    let mut events = engine.subscribe();
    let _ = engine.check_host_key("box.example", 22, key(OTHER)).await;
    let first = asked(&mut events);
    answer(&engine, first.id, HostKeyAnswer::TrustAndSave)
        .await
        .expect("trusted");

    let error = engine
        .check_host_key("box.example", 22, key(ED25519))
        .await
        .expect_err("changed");
    assert_eq!(error.code(), "host_key_changed");
    assert!(error.to_string().contains(&key(OTHER).fingerprint));
    let question = asked(&mut events);
    assert!(matches!(question.kind, HostKeyQuestionKind::Changed { .. }));

    for yes in [HostKeyAnswer::ConnectOnce, HostKeyAnswer::TrustAndSave] {
        let refused = answer(&engine, question.id, yes)
            .await
            .expect_err("no yes for a changed key");
        assert_eq!(refused.code(), "host_key_changed");
    }
    assert!(
        engine
            .check_host_key("box.example", 22, key(ED25519))
            .await
            .is_err(),
        "still stopped"
    );
    assert_eq!(engine.host_key_questions(), vec![question.clone()]);

    let Outcome::HostKeyReplaced { key: replaced } = engine
        .dispatch(Command::ReplaceHostKey {
            question: question.id,
        })
        .await
        .expect("replaced")
    else {
        panic!("the replacing key");
    };
    assert_eq!(replaced.fingerprint_sha256, key(ED25519).fingerprint);
    assert!(engine.host_key_questions().is_empty());
    engine
        .check_host_key("box.example", 22, key(ED25519))
        .await
        .expect("trusted now");
}

#[tokio::test]
async fn replace_is_for_changed_keys_only() {
    let (_root, engine) = engine_with_db(&[]).await;
    let mut events = engine.subscribe();
    let _ = engine.check_host_key("box.example", 22, key(ED25519)).await;
    let question = asked(&mut events);
    let error = engine
        .dispatch(Command::ReplaceHostKey {
            question: question.id,
        })
        .await
        .expect_err("nothing to replace");
    assert_eq!(error.code(), "host_key_not_changed");
}

#[tokio::test]
async fn cancel_closes_the_question() {
    let (_root, engine) = engine_with_db(&[]).await;
    let mut events = engine.subscribe();
    let _ = engine.check_host_key("box.example", 22, key(OTHER)).await;
    let question = asked(&mut events);
    answer(&engine, question.id, HostKeyAnswer::Cancel)
        .await
        .expect("cancelled");
    assert!(engine.host_key_questions().is_empty());
    loop {
        if let EngineEvent::HostKeyQuestionClosed { question: closed } =
            events.try_recv().expect("an event")
        {
            assert_eq!(closed, question.id);
            break;
        }
    }
}

#[tokio::test]
async fn dispatch_mcp_refuses_the_human_decisions() {
    let (root, engine) = engine_with_db(&[]).await;
    let mut events = engine.subscribe();
    let _ = engine.check_host_key("box.example", 22, key(ED25519)).await;
    let question = asked(&mut events);
    let path = root.path().join("known_hosts");
    std::fs::write(&path, ED25519).expect("write");
    let decisions = [
        Command::AnswerHostKey {
            question: question.id,
            answer: HostKeyAnswer::TrustAndSave,
        },
        Command::ReplaceHostKey {
            question: question.id,
        },
        Command::ImportKnownHosts {
            path: Some(path.display().to_string()),
        },
        // RC-080-12 (X7): forgetting a key, as removing a certificate.
        Command::RemoveHostKey { id: 1 },
    ];
    for command in decisions {
        assert_eq!(command.spec().class, crate::CommandClass::GuiOnly);
        let refused = engine
            .dispatch_mcp(command.clone())
            .await
            .expect_err("a person decides");
        assert_eq!(refused.code(), "gui_only", "{command:?}");
    }
    assert_eq!(engine.host_key_questions(), vec![question]);
    assert!(
        engine
            .db()
            .expect("db")
            .known_hosts()
            .await
            .expect("list")
            .is_empty(),
        "nothing trusted"
    );
    // The same commands run for the GUI, and other commands run for MCP.
    assert!(matches!(
        engine
            .dispatch_mcp(Command::ListHostKeys { filter: None })
            .await,
        Ok(Outcome::KnownHosts { .. })
    ));
}

/// RC-080-12 (X3): an unknown key of another type for a host already trusted is a first sighting
/// that says so - the dialog names the stored types - and the key exchange asks for them first.
#[tokio::test]
async fn a_new_key_type_for_a_known_host_names_the_stored_ones() {
    const ECDSA: &str = include_str!("../../rc-remote/tests/fixtures/known_hosts/ecdsa.pub");
    let (_root, engine) = engine_with_db(&[]).await;
    let mut events = engine.subscribe();
    let _ = engine.check_host_key("box.example", 22, key(ED25519)).await;
    let first = asked(&mut events);
    assert!(first.stored_types.is_empty(), "{first:?}");
    answer(&engine, first.id, HostKeyAnswer::TrustAndSave)
        .await
        .expect("trusted");
    assert_eq!(
        engine.stored_host_key_types("Box.Example", 22).await,
        ["ssh-ed25519"]
    );

    let error = engine
        .check_host_key("box.example", 22, key(ECDSA))
        .await
        .expect_err("another type is a first sighting");
    assert_eq!(error.code(), "host_key_unknown");
    let question = asked(&mut events);
    assert_eq!(question.kind, HostKeyQuestionKind::Unknown);
    assert_eq!(question.stored_types, ["ssh-ed25519"]);
    assert!(
        engine
            .stored_host_key_types("other.example", 22)
            .await
            .is_empty()
    );
}

/// RC-080-22: host-key questions keep the shared desk's rules - one question per host, port, key
/// and kind whatever the stored types, "connect once" grants the offered key, and the engine
/// keeps at most `MAX_QUESTIONS`, dropping the oldest.
#[tokio::test]
async fn host_key_questions_go_through_the_shared_desk() {
    use crate::question_desk::{MAX_QUESTIONS, Question};

    let question = HostKeyQuestion {
        id: 7,
        host: "box.example".to_owned(),
        port: 22,
        offered: key(ED25519),
        kind: HostKeyQuestionKind::Unknown,
        stored_types: Vec::new(),
    };
    let later = HostKeyQuestion {
        id: 0,
        stored_types: vec!["ssh-rsa".to_owned()],
        ..question.clone()
    };
    assert!(question.same_as(&later));
    for other in [
        HostKeyQuestion {
            offered: key(OTHER),
            ..question.clone()
        },
        HostKeyQuestion {
            port: 2222,
            ..question.clone()
        },
    ] {
        assert!(!question.same_as(&other), "{other:?}");
    }
    let fingerprint = key(ED25519).fingerprint;
    assert_eq!(question.grant(), ("box.example", 22, fingerprint.as_str()));

    let (_root, engine) = engine_with_db(&[]).await;
    for n in 0..=MAX_QUESTIONS {
        let _ = engine
            .check_host_key(&format!("host{n}.example"), 22, key(ED25519))
            .await;
    }
    let open = engine.host_key_questions();
    assert_eq!(open.len(), MAX_QUESTIONS);
    assert_eq!(open.first().map(|question| question.id), Some(2));
    assert_eq!(
        open.first().map(|question| question.host.as_str()),
        Some("host1.example")
    );
}
