use serde_json::{Value, json};

use crate::{
    Command, Engine,
    import::{
        tc_ini,
        total_commander_look::tests::OWNERS,
        total_commander_look::{self as look, TcLook},
        total_commander_report::{TcLookRow, TcRowStatus},
    },
    settings::{COLOUR_RULES_KEY, LIST_FONT_KEY, LIST_FONT_SIZE_KEY, stored_setting},
    test_support,
};

fn looks(text: &str) -> Vec<TcLook> {
    let ini = tc_ini::parse(text);
    let mut found = look::colours(&ini);
    found.extend(look::rules(&ini));
    found.extend(look::font(&ini));
    found
}

async fn set(engine: &Engine, key: &str, value: Value) {
    engine
        .dispatch(Command::SetSetting {
            key: key.to_owned(),
            value,
        })
        .await
        .expect("set");
}

async fn stored(engine: &Engine, key: &str) -> Value {
    stored_setting(engine.db().expect("db"), key)
        .await
        .expect("known")
}

fn statuses(rows: &[TcLookRow]) -> Vec<(&str, &TcRowStatus, bool)> {
    rows.iter()
        .map(|row| (row.key.as_str(), &row.status, row.selected))
        .collect()
}

#[tokio::test]
async fn the_owners_look_is_new_then_imported_then_there_already() {
    let (_root, engine) = test_support::engine_with_db(&[]).await;
    let found = looks(OWNERS);
    let preview = engine
        .import_tc_look(&found, None, true)
        .await
        .expect("plans");
    assert_eq!(
        statuses(&preview),
        [
            ("colour.light.stripe", &TcRowStatus::New, true),
            ("colour.light.cursor", &TcRowStatus::New, true),
            ("colour.dark.stripe", &TcRowStatus::New, true),
            ("font.list", &TcRowStatus::New, true),
        ]
    );
    assert_eq!(preview[3].value, "Consolas 13");
    assert_eq!(
        stored(&engine, "appearance.colours.light.cursor").await,
        json!(""),
        "a preview writes nothing"
    );

    let done = engine
        .import_tc_look(&found, None, false)
        .await
        .expect("imports");
    assert!(done.iter().all(|row| row.status == TcRowStatus::Created));
    assert_eq!(
        stored(&engine, "appearance.colours.light.stripe").await,
        json!("#e6e6e6")
    );
    assert_eq!(
        stored(&engine, "appearance.colours.light.cursor").await,
        json!("#800000")
    );
    assert_eq!(
        stored(&engine, "appearance.colours.dark.stripe").await,
        json!("#303030")
    );
    assert_eq!(stored(&engine, LIST_FONT_KEY).await, json!("Consolas"));
    assert_eq!(stored(&engine, LIST_FONT_SIZE_KEY).await, json!(13));

    let again = engine
        .import_tc_look(&found, None, true)
        .await
        .expect("plans");
    assert!(
        again
            .iter()
            .all(|row| row.status == TcRowStatus::Exists && !row.selected),
        "{again:?}"
    );
}

#[tokio::test]
async fn a_changed_colour_is_written_only_when_chosen() {
    let (_root, engine) = test_support::engine_with_db(&[]).await;
    set(&engine, "appearance.colours.light.cursor", json!("#123456")).await;
    let found = looks(OWNERS);
    let only = ["font.list".to_owned()];
    let rows = engine
        .import_tc_look(&found, Some(&only), false)
        .await
        .expect("imports");
    let cursor = rows
        .iter()
        .find(|row| row.key == "colour.light.cursor")
        .expect("listed");
    assert_eq!(cursor.status, TcRowStatus::Update);
    assert!(!cursor.selected);
    assert_eq!(
        stored(&engine, "appearance.colours.light.cursor").await,
        json!("#123456")
    );
    assert_eq!(stored(&engine, LIST_FONT_KEY).await, json!("Consolas"));
}

#[tokio::test]
async fn rules_are_appended_and_a_known_mask_is_replaced_where_it_stands() {
    let (_root, engine) = test_support::engine_with_db(&[]).await;
    set(
        &engine,
        COLOUR_RULES_KEY,
        json!("*.exe\t#00ff00\n*.log\t#aaaaaa"),
    )
    .await;
    let found = looks(
        "[Colors]\nColorFilter1=*.EXE\nColorFilter1Color=255\n\
         ColorFilter2=*.log\nColorFilter2Color=11184810\n\
         ColorFilter3=*.txt\nColorFilter3Color=16711680\n",
    );
    let rows = engine
        .import_tc_look(&found, None, false)
        .await
        .expect("imports");
    assert_eq!(
        statuses(&rows),
        [
            ("rule.1", &TcRowStatus::Updated, true),
            ("rule.2", &TcRowStatus::Exists, false),
            ("rule.3", &TcRowStatus::Created, true),
        ]
    );
    assert_eq!(
        stored(&engine, COLOUR_RULES_KEY).await,
        json!("*.EXE\t#ff0000\n*.log\t#aaaaaa\n*.txt\t#0000ff")
    );
}

#[tokio::test]
async fn new_rules_beyond_the_limit_are_skipped() {
    let (_root, engine) = test_support::engine_with_db(&[]).await;
    let full: Vec<String> = (0..63).map(|n| format!("*.x{n}\t#111111")).collect();
    set(&engine, COLOUR_RULES_KEY, json!(full.join("\n"))).await;
    let found = looks(
        "[Colors]\nColorFilter1=*.a\nColorFilter1Color=255\n\
         ColorFilter2=*.b\nColorFilter2Color=255\n",
    );
    let rows = engine
        .import_tc_look(&found, None, true)
        .await
        .expect("plans");
    assert_eq!(rows[0].status, TcRowStatus::New);
    assert_eq!(
        rows[1].status,
        TcRowStatus::Skipped {
            reason: "too_many".to_owned()
        }
    );
    assert!(!rows[1].selected);
}
