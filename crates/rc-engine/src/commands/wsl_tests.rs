use crate::test_support::engine;
use crate::{Command, EngineEvent, Outcome, VfsPath, WslDistro};

fn distros_of(outcome: Outcome) -> Vec<WslDistro> {
    match outcome {
        Outcome::WslDistros { distros } => distros,
        other => panic!("WSL distributions, got {other:?}"),
    }
}

#[cfg(not(windows))]
#[tokio::test]
async fn outside_windows_the_list_is_empty() {
    let (_root, engine) = engine(&[]).await;
    let listed = engine
        .dispatch(Command::ListWslDistros)
        .await
        .expect("lists");
    assert!(distros_of(listed).is_empty());
}

#[tokio::test]
async fn refresh_answers_the_list_and_announces_it() {
    let (_root, engine) = engine(&[]).await;
    let mut events = engine.subscribe();
    let refreshed = engine
        .dispatch(Command::RefreshWslDistros)
        .await
        .expect("refreshes");
    let listed = engine
        .dispatch(Command::ListWslDistros)
        .await
        .expect("lists");
    assert_eq!(distros_of(listed), distros_of(refreshed));
    assert_eq!(
        events.try_recv().expect("an event"),
        EngineEvent::WslDistros
    );
    assert!(events.try_recv().is_err(), "listing announces nothing");
}

#[test]
fn the_outcome_serializes_as_a_kind_with_the_list() {
    let outcome = Outcome::WslDistros {
        distros: vec![WslDistro {
            name: "Ubuntu".into(),
            is_default: true,
            version: Some(2),
            root: VfsPath::local(r"\\wsl.localhost\Ubuntu\"),
        }],
    };
    let value = serde_json::to_value(&outcome).expect("serializes");
    assert_eq!(value["kind"], "wsl_distros");
    assert_eq!(value["distros"][0]["name"], "Ubuntu");
    assert_eq!(value["distros"][0]["is_default"], true);
}
