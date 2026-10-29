//! The diagnostics tools over an in-process pipe, against a database file seeded directly.

use rc_db::{DB_FILE, Db};
use rmcp::{RoleClient, model::CallToolRequestParams, service::RunningService};
use serde_json::{Value, json};
use sqlx::{Connection as _, SqliteConnection};
use tempfile::TempDir;

use crate::test_support::Fixture;

const PLANTED: &str = "planted-secret-0042";

struct Setup {
    dir: TempDir,
    client: RunningService<RoleClient, ()>,
}

async fn setup() -> Setup {
    let fixture = Fixture::new();
    let db = Db::open(fixture.root().join(DB_FILE)).await.expect("db");
    // Headless: these tests drive the confirmation-code path (RC-010-13).
    let built = fixture.with_db(db).with_codes().build().await;
    let client = built.client().await;
    Setup {
        dir: built.root,
        client,
    }
}

/// Writes lines the way an older build might have: straight into the table, unmasked.
async fn seed(setup: &Setup, lines: &[(&str, &str)]) {
    let url = format!("sqlite://{}", setup.dir.path().join(DB_FILE).display());
    let mut connection = SqliteConnection::connect(&url).await.expect("connect");
    for (level, message) in lines {
        sqlx::query("INSERT INTO error_log (level, target, message) VALUES (?, 'rc_test', ?)")
            .bind(*level)
            .bind(*message)
            .execute(&mut connection)
            .await
            .expect("insert");
    }
    connection.close().await.expect("close");
}

async fn call(setup: &Setup, tool: &str, arguments: Value) -> Value {
    let arguments = arguments.as_object().cloned().unwrap_or_default();
    let result = setup
        .client
        .call_tool(CallToolRequestParams::new(tool.to_owned()).with_arguments(arguments))
        .await
        .expect("call");
    let text = result
        .content
        .first()
        .and_then(|content| content.as_text())
        .map(|text| text.text.clone())
        .expect("text");
    serde_json::from_str(&text).expect("json")
}

#[tokio::test]
async fn the_error_log_reads_filtered_and_masked() {
    let setup = setup().await;
    let leaked = format!("login failed: password={PLANTED}");
    seed(
        &setup,
        &[("warn", "slow listing"), ("error", leaked.as_str())],
    )
    .await;

    let all = call(&setup, "diagnostics_error_log", json!({})).await;
    let rendered = all.to_string();
    assert!(!rendered.contains(PLANTED), "{rendered}");
    assert_eq!(all["entries"].as_array().map(Vec::len), Some(2), "{all}");
    assert_eq!(all["entries"][0]["level"], "error");

    let warnings = call(&setup, "diagnostics_error_log", json!({ "level": "warn" })).await;
    assert_eq!(
        warnings["entries"][0]["message"], "slow listing",
        "{warnings}"
    );
    assert_eq!(warnings["entries"].as_array().map(Vec::len), Some(1));
}

#[tokio::test]
async fn clearing_the_error_log_wants_a_confirmation() {
    let setup = setup().await;
    seed(&setup, &[("warn", "one"), ("error", "two")]).await;

    let asked = call(&setup, "diagnostics_clear_error_log", json!({})).await;
    assert_eq!(asked["confirmation_required"], true, "{asked}");
    assert_eq!(asked["class"], "destructive");
    let code = asked["confirmation"].as_str().expect("code").to_owned();

    let cleared = call(
        &setup,
        "diagnostics_clear_error_log",
        json!({ "confirmation": code }),
    )
    .await;
    assert_eq!(cleared["removed"], 2, "{cleared}");
    let empty = call(&setup, "diagnostics_error_log", json!({})).await;
    assert_eq!(empty["entries"], json!([]));
}

#[tokio::test]
async fn retention_reads_changes_and_refuses_unlimited() {
    let setup = setup().await;
    let read = call(&setup, "diagnostics_get_retention", json!({})).await;
    assert_eq!(read["tables"][0]["table"], "error_log", "{read}");
    assert_eq!(read["tables"][0]["limits"]["max_rows"], 5_000);

    // Raising runs at once; lowering below the limit in force is destructive (RC-080-12).
    let raised = call(
        &setup,
        "diagnostics_set_retention",
        json!({ "table": "error_log", "max_age_days": 60 }),
    )
    .await;
    assert_eq!(
        raised["tables"][0]["limits"]["max_age_days"], 60,
        "{raised}"
    );
    let lowering = json!({ "table": "error_log", "max_age_days": 7 });
    let asked = call(&setup, "diagnostics_set_retention", lowering.clone()).await;
    assert_eq!(asked["confirmation_required"], true, "{asked}");
    assert_eq!(asked["class"], "destructive");
    let mut confirmed = lowering;
    confirmed["confirmation"] = asked["confirmation"].clone();
    let changed = call(&setup, "diagnostics_set_retention", confirmed).await;
    assert_eq!(
        changed["tables"][0]["limits"]["max_age_days"], 7,
        "{changed}"
    );
    assert_eq!(changed["tables"][0]["limits"]["max_rows"], 5_000);

    let refused = call(
        &setup,
        "diagnostics_set_retention",
        json!({ "table": "error_log", "max_rows": 10_000_000 }),
    )
    .await;
    assert_eq!(refused["code"], "retention_out_of_range", "{refused}");
}
