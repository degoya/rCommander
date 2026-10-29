use super::*;
use crate::{RetentionLimits, RetentionSettings, retained_table};

async fn db() -> Db {
    Db::open_in_memory().await.expect("open")
}

fn tab(panel: &str, position: i64, location: &str, history: &[(i64, &str)]) -> StoredTab {
    StoredTab {
        panel: panel.to_owned(),
        position,
        active: position == 0,
        locked: false,
        location: location.to_owned(),
        sort: r#"{"key":"name","descending":false}"#.to_owned(),
        mode: r#""full""#.to_owned(),
        history: history
            .iter()
            .map(|(step, location)| StoredVisit {
                step: *step,
                location: (*location).to_owned(),
            })
            .collect(),
    }
}

#[tokio::test]
async fn nothing_saved_is_no_tab() {
    assert_eq!(db().await.load_tabs().await.expect("load"), Vec::new());
}

#[tokio::test]
async fn saved_tabs_come_back_by_panel_and_position_with_their_history() {
    let db = db().await;
    let mut locked = tab("right", 1, "r1", &[]);
    locked.locked = true;
    let tabs = vec![
        tab("left", 0, "l0", &[(-2, "a"), (-1, "b"), (1, "c")]),
        locked,
        tab("right", 0, "r0", &[(-1, "x")]),
    ];
    db.save_tabs(&tabs).await.expect("save");
    let loaded = db.load_tabs().await.expect("load");
    assert_eq!(
        loaded,
        vec![tabs[0].clone(), tabs[2].clone(), tabs[1].clone()]
    );
}

#[tokio::test]
async fn a_save_replaces_every_tab_and_its_history() {
    let db = db().await;
    db.save_tabs(&[tab("left", 0, "old", &[(-1, "older")])])
        .await
        .expect("first");
    let now = vec![tab("right", 0, "new", &[])];
    db.save_tabs(&now).await.expect("second");
    assert_eq!(db.load_tabs().await.expect("load"), now);
    let visits: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM dir_history")
        .fetch_one(db.readers())
        .await
        .expect("count");
    assert_eq!(visits, 0);
}

#[tokio::test]
async fn step_zero_is_the_tab_itself_and_not_stored() {
    let db = db().await;
    db.save_tabs(&[tab("left", 0, "here", &[(0, "here"), (-1, "back")])])
        .await
        .expect("save");
    let loaded = db.load_tabs().await.expect("load");
    assert_eq!(loaded[0].history.len(), 1);
    assert_eq!(loaded[0].history[0].step, -1);
}

/// The sweep keeps the history within its row limit by taking the farthest folders of every tab
/// first, so the folders next to each tab stay.
#[tokio::test]
async fn the_sweep_takes_the_oldest_folders_of_every_tab_first() {
    let db = db().await;
    let back: Vec<(i64, String)> = (1..=60).map(|step| (-step, format!("dir{step}"))).collect();
    let history: Vec<(i64, &str)> = back
        .iter()
        .map(|(step, location)| (*step, location.as_str()))
        .collect();
    db.save_tabs(&[
        tab("left", 0, "l", &history),
        tab("right", 0, "r", &history),
    ])
    .await
    .expect("save");
    let table = retained_table("dir_history").expect("retained");
    let checked = table
        .check(RetentionLimits {
            max_rows: table.floor.max_rows,
            max_age_days: table.defaults.max_age_days,
        })
        .expect("in range");
    let settings = RetentionSettings::defaults().with(checked);
    let report = db.prune(&settings).await.expect("prune");
    assert_eq!(report.deleted, 20);
    for loaded in db.load_tabs().await.expect("load") {
        assert_eq!(loaded.history.len(), 50, "{}", loaded.panel);
        assert_eq!(loaded.history.first().map(|visit| visit.step), Some(-50));
        assert_eq!(loaded.history.last().map(|visit| visit.step), Some(-1));
    }
}
