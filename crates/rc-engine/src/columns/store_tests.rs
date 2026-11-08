//! The stored column sets through the commands: saved, read, changed, ordered and deleted, every
//! boundary checked, every change announced; a deleted set leaves the full view.

use tokio::sync::broadcast::error::TryRecvError;

use crate::{
    ColumnSet, ColumnSetId, ColumnSetSpec, ColumnSetsOutcome, Command, Engine, EngineError,
    EngineEvent, Field, ListMode, Outcome, PanelId, test_support,
};

fn outcome(result: Result<Outcome, EngineError>) -> ColumnSetsOutcome {
    match result.expect("answered") {
        Outcome::ColumnSets(outcome) => *outcome,
        other => panic!("a column sets outcome, got {other:?}"),
    }
}

fn code(result: Result<Outcome, EngineError>) -> &'static str {
    result.expect_err("refused").code()
}

fn spec() -> ColumnSetSpec {
    ColumnSetSpec::of(&[Field::Name, Field::Size, Field::Created])
}

async fn created(engine: &Engine, name: &str) -> ColumnSetId {
    let command = Command::CreateColumnSet {
        name: name.into(),
        spec: spec(),
    };
    match outcome(engine.dispatch(command).await) {
        ColumnSetsOutcome::Saved { id } => id,
        other => panic!("saved, got {other:?}"),
    }
}

async fn listed(engine: &Engine) -> Vec<ColumnSet> {
    match outcome(engine.dispatch(Command::ListColumnSets {}).await) {
        ColumnSetsOutcome::List { sets } => sets,
        other => panic!("a list, got {other:?}"),
    }
}

async fn names(engine: &Engine) -> Vec<String> {
    listed(engine)
        .await
        .into_iter()
        .map(|set| set.name)
        .collect()
}

#[tokio::test]
async fn a_set_is_saved_read_changed_moved_and_deleted() {
    let (_root, engine) = test_support::engine_with_db(&[("a.txt", "a")]).await;
    let mut events = engine.subscribe();
    let photos = created(&engine, " Photos ").await;
    assert!(matches!(
        events.try_recv(),
        Ok(EngineEvent::ColumnSetsChanged)
    ));
    let code_set = created(&engine, "Code").await;
    assert_eq!(names(&engine).await, ["Photos", "Code"]);

    let mut changed = spec();
    changed.masks = vec![r"D:\Fotos\*".into()];
    outcome(
        engine
            .dispatch(Command::UpdateColumnSet {
                id: photos,
                spec: changed.clone(),
            })
            .await,
    );
    match outcome(engine.dispatch(Command::GetColumnSet { id: photos }).await) {
        ColumnSetsOutcome::Set(set) => assert_eq!(set.spec, changed),
        other => panic!("a set, got {other:?}"),
    }
    outcome(
        engine
            .dispatch(Command::RenameColumnSet {
                id: photos,
                name: "Pictures".into(),
            })
            .await,
    );
    outcome(
        engine
            .dispatch(Command::MoveColumnSet {
                id: code_set,
                position: 0,
            })
            .await,
    );
    assert_eq!(names(&engine).await, ["Code", "Pictures"]);

    outcome(
        engine
            .dispatch(Command::DeleteColumnSet { id: photos })
            .await,
    );
    assert_eq!(names(&engine).await, ["Code"]);
    assert_eq!(
        code(
            engine
                .dispatch(Command::DeleteColumnSet { id: photos })
                .await
        ),
        "column_set_unknown"
    );
    assert_eq!(
        code(engine.dispatch(Command::GetColumnSet { id: photos }).await),
        "column_set_unknown"
    );
    while let Ok(event) = events.try_recv() {
        assert!(
            !matches!(event, EngineEvent::Panel { .. }),
            "no panel showed it"
        );
    }
    assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));
}

#[tokio::test]
async fn names_are_checked_and_unique_without_regard_to_case() {
    let (_root, engine) = test_support::engine_with_db(&[("a.txt", "a")]).await;
    created(&engine, "Photos").await;
    let again = Command::CreateColumnSet {
        name: "PHOTOS".into(),
        spec: spec(),
    };
    assert_eq!(code(engine.dispatch(again).await), "column_set_name_taken");
    let blank = Command::CreateColumnSet {
        name: " ".into(),
        spec: spec(),
    };
    assert_eq!(code(engine.dispatch(blank).await), "column_set_name");
    let mut broken = spec();
    broken.columns.clear();
    let invalid = Command::CreateColumnSet {
        name: "Empty".into(),
        spec: broken,
    };
    assert_eq!(code(engine.dispatch(invalid).await), "column_set_invalid");
}

#[tokio::test]
async fn at_most_64_sets_are_kept() {
    let (_root, engine) = test_support::engine_with_db(&[("a.txt", "a")]).await;
    for n in 0..crate::columns::MAX_SETS {
        created(&engine, &format!("set {n}")).await;
    }
    let one_more = Command::CreateColumnSet {
        name: "one more".into(),
        spec: spec(),
    };
    assert_eq!(code(engine.dispatch(one_more).await), "column_set_too_many");
}

#[tokio::test]
async fn a_deleted_set_leaves_the_full_view_in_its_panel() {
    let (_root, engine) = test_support::engine_with_db(&[("a.txt", "a")]).await;
    let id = created(&engine, "Photos").await;
    let shown = test_support::run_panel(
        &engine,
        Command::SetListMode {
            panel: Some(PanelId::Left),
            mode: ListMode::Custom { set: id },
        },
    )
    .await;
    assert_eq!(shown.mode, ListMode::Custom { set: id });
    engine
        .dispatch(Command::DeleteColumnSet { id })
        .await
        .expect("deleted");
    assert_eq!(engine.panel(PanelId::Left).mode, ListMode::Full);
    let unknown = Command::SetListMode {
        panel: Some(PanelId::Left),
        mode: ListMode::Custom { set: id },
    };
    assert_eq!(code(engine.dispatch(unknown).await), "column_set_unknown");
}
