use std::time::Duration;

use rc_ops::JobOptions;
use serde_json::json;

use crate::test_support;
use crate::{Command, CommandClass, Engine, EngineEvent, settings::CONFIRM_QUIT_KEY};

async fn engine() -> (tempfile::TempDir, Engine) {
    test_support::engine_with_db(&[("a.txt", "a"), ("docs/a.txt", "old")]).await
}

/// A copy that waits for the answer to its conflict about `docs/a.txt`: unfinished.
async fn waiting_job(engine: &Engine) {
    let outcome = engine
        .dispatch(Command::OpsCopy(crate::TransferRequest {
            panel: None,
            sources: vec!["a.txt".into()],
            target: Some("docs".into()),
            options: JobOptions::default(),
        }))
        .await
        .expect("queued");
    let crate::Outcome::Job(job) = outcome else {
        panic!("a job, got {outcome:?}");
    };
    for _ in 0..500 {
        if engine.ops().job(job.id).expect("known").question.is_some() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("job {} asked nothing", job.id);
}

/// What `app.quit` announced.
async fn quit(engine: &Engine) -> EngineEvent {
    let mut events = engine.subscribe();
    engine.dispatch(Command::Quit {}).await.expect("quit");
    loop {
        if let event @ EngineEvent::Quit { .. } = events.try_recv().expect("an event") {
            return event;
        }
    }
}

#[tokio::test]
async fn quit_without_jobs_asks_nothing() {
    let (_root, engine) = engine().await;
    assert_eq!(quit(&engine).await, EngineEvent::Quit { unfinished: 0 });
}

#[tokio::test]
async fn quit_with_a_waiting_transfer_names_it_unless_the_question_is_off() {
    let (_root, engine) = engine().await;
    waiting_job(&engine).await;
    assert_eq!(quit(&engine).await, EngineEvent::Quit { unfinished: 1 });

    engine
        .dispatch(Command::SetSetting {
            key: CONFIRM_QUIT_KEY.into(),
            value: json!(false),
        })
        .await
        .expect("an agent may switch it");
    assert_eq!(quit(&engine).await, EngineEvent::Quit { unfinished: 0 });
}

/// Ending the person's window is never an agent's call: MCP's dispatch refuses it.
#[tokio::test]
async fn quit_is_the_windows_only() {
    let (_root, engine) = engine().await;
    assert_eq!(Command::Quit {}.spec().class, CommandClass::GuiOnly);
    let refused = engine
        .dispatch_mcp(Command::Quit {})
        .await
        .expect_err("gui only");
    assert_eq!(refused.code(), "gui_only");
}
