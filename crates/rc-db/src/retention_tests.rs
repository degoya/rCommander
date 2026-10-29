use super::*;

/// Every table the migrations create, sqlite's own (`sqlite_*`) aside.
async fn migrated_tables() -> Vec<String> {
    let db = Db::open_in_memory().await.expect("open");
    sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite\\_%' \
         ESCAPE '\\' ORDER BY name",
    )
    .fetch_all(db.readers())
    .await
    .expect("tables")
}

/// What is wrong with the two lists against `tables`, one message each.
fn problems(tables: &[String]) -> Vec<String> {
    let retained: Vec<&str> = RETAINED_TABLES.iter().map(|table| table.table).collect();
    let durable: Vec<&str> = DURABLE_TABLES.iter().map(|(table, _)| *table).collect();
    let mut problems = Vec::new();
    for table in tables {
        match (
            retained.contains(&table.as_str()),
            durable.contains(&table.as_str()),
        ) {
            (false, false) => problems.push(format!(
                "{table} grows without a limit as far as anyone knows: add it to \
                 rc_db::RETAINED_TABLES with a row and an age limit, or to DURABLE_TABLES with \
                 the reason it does not grow with use"
            )),
            (true, true) => problems.push(format!("{table} is both retained and durable")),
            _ => {}
        }
    }
    for listed in retained.iter().chain(&durable) {
        if !tables.iter().any(|table| table == listed) {
            problems.push(format!("{listed} is listed but no migration creates it"));
        }
    }
    for (table, reason) in DURABLE_TABLES {
        if reason.trim().len() < 10 {
            problems.push(format!("{table}: say why it is durable"));
        }
    }
    problems
}

#[tokio::test]
async fn every_table_is_retained_or_durable() {
    let problems = problems(&migrated_tables().await);
    assert!(problems.is_empty(), "\n{}\n", problems.join("\n"));
}

/// Synthetic on purpose, like `migration_checksums`: a probe migration could be left behind.
#[tokio::test]
async fn a_new_table_without_an_entry_fails() {
    let mut tables = migrated_tables().await;
    tables.push("transfer_history".to_owned());
    let problems = problems(&tables);
    assert_eq!(problems.len(), 1, "{problems:#?}");
    assert!(
        problems[0].starts_with("transfer_history grows without a limit"),
        "{}",
        problems[0]
    );
}

#[test]
fn every_retained_table_has_sane_bounds_and_its_own_keys() {
    for table in RETAINED_TABLES {
        let (floor, defaults, ceiling) = (table.floor, table.defaults, table.ceiling);
        assert!(
            floor.max_rows >= 1 && floor.max_age_days >= 1,
            "{}",
            table.table
        );
        assert!(
            floor.max_rows <= defaults.max_rows && defaults.max_rows <= ceiling.max_rows,
            "{}",
            table.table
        );
        assert!(
            floor.max_age_days <= defaults.max_age_days
                && defaults.max_age_days <= ceiling.max_age_days,
            "{}",
            table.table
        );
        assert_eq!(
            table.rows_key,
            format!("retention.{}.max_rows", table.table)
        );
        assert_eq!(
            table.age_key,
            format!("retention.{}.max_age_days", table.table)
        );
    }
}

#[test]
fn the_error_log_keeps_5000_lines_and_30_days_by_default() {
    let limits = RetentionSettings::defaults()
        .limits("error_log")
        .expect("error_log is retained");
    assert_eq!(
        limits,
        RetentionLimits {
            max_rows: 5_000,
            max_age_days: 30
        }
    );
}

#[test]
fn a_limit_outside_floor_and_ceiling_is_refused() {
    let table = retained_table("error_log").expect("retained");
    let unlimited = RetentionLimits {
        max_rows: u64::MAX,
        max_age_days: 30,
    };
    assert_eq!(
        table.check(unlimited),
        Err(RetentionError::OutOfRange {
            key: "retention.error_log.max_rows",
            value: u64::MAX,
            min: 100,
            max: 50_000,
        })
    );
    let zero_days = RetentionLimits {
        max_rows: 1_000,
        max_age_days: 0,
    };
    assert!(matches!(
        table.check(zero_days),
        Err(RetentionError::OutOfRange {
            key: "retention.error_log.max_age_days",
            ..
        })
    ));
    assert!(table.check(table.floor).is_ok());
    assert!(table.check(table.ceiling).is_ok());
}

#[test]
fn an_unknown_table_is_named() {
    assert_eq!(
        retained_table("settings"),
        Err(RetentionError::UnknownTable {
            table: "settings".to_owned()
        })
    );
}

#[tokio::test]
async fn stored_limits_are_read_and_hand_edited_ones_clamped() {
    let db = Db::open_in_memory().await.expect("open");
    let table = retained_table("error_log").expect("retained");
    assert_eq!(
        db.retention_settings().await.expect("read"),
        RetentionSettings::defaults()
    );

    let lowered = table
        .check(RetentionLimits {
            max_rows: 1_000,
            max_age_days: 7,
        })
        .expect("in range");
    db.set_retention(lowered).await.expect("store");
    assert_eq!(
        db.retention_settings()
            .await
            .expect("read")
            .limits("error_log"),
        Some(lowered.limits())
    );

    // Edited past the bounds, or to something that is no number: never unlimited.
    db.set(table.rows_key, &0_u64).await.expect("set");
    db.set(table.age_key, "forever").await.expect("set");
    assert_eq!(
        db.retention_settings()
            .await
            .expect("read")
            .limits("error_log"),
        Some(RetentionLimits {
            max_rows: table.floor.max_rows,
            max_age_days: table.defaults.max_age_days,
        })
    );
}

#[test]
fn the_status_names_limits_defaults_and_bounds() {
    let status = RetentionSettings::defaults().status();
    let error_log = status
        .iter()
        .find(|status| status.table == "error_log")
        .expect("error_log");
    assert_eq!(error_log.limits, error_log.defaults);
    assert_eq!(error_log.floor.max_rows, 100);
    assert_eq!(error_log.ceiling.max_age_days, 365);
}

/// `retention_settings` reads every limit in one query, `LIKE 'retention.%'` (RC-080-54): a key
/// outside that prefix would never be read and its table would keep its default unseen.
#[test]
fn every_limit_key_lies_under_the_prefix_read_at_once() {
    for table in crate::RETAINED_TABLES {
        for key in [table.rows_key, table.age_key] {
            assert!(key.starts_with("retention."), "{key}");
        }
    }
}
