use rc_db::Db;
use rc_rename::{CaseMode, CounterSettings, RenameRule, SearchReplace};

use super::{MAX_PRESETS, PRESETS_KEY, RenamePreset, sanitized};
use crate::test_support;
use crate::{Command, Engine, EngineEvent, Outcome, PanelId};

async fn engine() -> (tempfile::TempDir, Engine, Db) {
    let (root, engine) = test_support::engine(&[]).await;
    let db = Db::open_in_memory().await.expect("db");
    assert!(engine.attach_db(db.clone()));
    (root, engine, db)
}

fn full_rule() -> RenameRule {
    RenameRule {
        name_mask: "[Y]-[M]-[D]_[C:3]".to_owned(),
        extension_mask: "[L][E]".to_owned(),
        case: CaseMode::WordsUpper,
        search: Some(SearchReplace {
            find: "ae|oe".to_owned(),
            replace: "a|o".to_owned(),
            regex: false,
            case_sensitive: true,
            first_only: true,
            include_extension: true,
            substitute: false,
        }),
        counter: CounterSettings {
            start: 10,
            step: 5,
            width: 3,
        },
    }
}

async fn presets(engine: &Engine, command: Command) -> (Vec<RenamePreset>, Option<RenameRule>) {
    match engine.dispatch(command).await.expect("the presets") {
        Outcome::RenamePresets { presets, last } => (presets, last),
        other => panic!("the presets, got {other:?}"),
    }
}

fn names(presets: &[RenamePreset]) -> Vec<&str> {
    presets.iter().map(|preset| preset.name.as_str()).collect()
}

#[tokio::test]
async fn a_saved_preset_keeps_every_field_and_a_new_engine_reads_it() {
    let (root, engine, db) = engine().await;
    let save = Command::MultiRenamePresetSave {
        name: "  Photos  ".to_owned(),
        rule: full_rule(),
    };
    let (saved, last) = presets(&engine, save).await;
    assert_eq!(names(&saved), ["Photos"]);
    assert_eq!(saved[0].rule, full_rule());
    assert_eq!(last, None);

    // A restart: another engine on the same database.
    let again = test_support::headless(root.path()).await;
    assert!(again.attach_db(db));
    let (listed, _) = presets(&again, Command::MultiRenamePresetList).await;
    assert_eq!(listed, saved);
}

#[tokio::test]
async fn save_replaces_by_name_rename_and_delete_find_it_without_case() {
    let (_root, engine, _db) = engine().await;
    for name in ["b", "a"] {
        presets(
            &engine,
            Command::MultiRenamePresetSave {
                name: name.to_owned(),
                rule: RenameRule::default(),
            },
        )
        .await;
    }
    let (list, _) = presets(
        &engine,
        Command::MultiRenamePresetSave {
            name: "A".to_owned(),
            rule: full_rule(),
        },
    )
    .await;
    assert_eq!(names(&list), ["A", "b"], "replaced in place, sorted");
    assert_eq!(list[0].rule, full_rule());

    let (list, _) = presets(
        &engine,
        Command::MultiRenamePresetRename {
            name: "B".to_owned(),
            new_name: "c".to_owned(),
        },
    )
    .await;
    assert_eq!(names(&list), ["A", "c"]);
    let clash = engine
        .dispatch(Command::MultiRenamePresetRename {
            name: "c".to_owned(),
            new_name: "a".to_owned(),
        })
        .await
        .expect_err("taken");
    assert_eq!(clash.code(), "rename_preset_invalid");

    let (list, _) = presets(
        &engine,
        Command::MultiRenamePresetDelete {
            name: "a".to_owned(),
        },
    )
    .await;
    assert_eq!(names(&list), ["c"]);
    let gone = engine
        .dispatch(Command::MultiRenamePresetDelete {
            name: "a".to_owned(),
        })
        .await
        .expect_err("gone");
    assert_eq!(gone.code(), "rename_preset_not_found");
}

#[tokio::test]
async fn bad_names_and_overlong_rules_are_refused() {
    let (_root, engine, _db) = engine().await;
    for name in ["", "   ", "a\nb", &"x".repeat(65)] {
        let refused = engine
            .dispatch(Command::MultiRenamePresetSave {
                name: name.to_owned(),
                rule: RenameRule::default(),
            })
            .await
            .expect_err("a bad name");
        assert_eq!(refused.code(), "rename_preset_invalid", "{name:?}");
    }
    let long = RenameRule {
        name_mask: "x".repeat(rc_rename::MAX_RULE_TEXT + 1),
        ..RenameRule::default()
    };
    let refused = engine
        .dispatch(Command::MultiRenamePresetSave {
            name: "long".to_owned(),
            rule: long,
        })
        .await
        .expect_err("too long");
    assert_eq!(refused.code(), "rename_input_too_long");
}

#[tokio::test]
async fn stored_presets_are_read_as_untrusted() {
    let (_root, engine, db) = engine().await;
    let stored = vec![
        RenamePreset {
            name: "ok".to_owned(),
            rule: RenameRule::default(),
        },
        RenamePreset {
            name: "OK".to_owned(),
            rule: full_rule(),
        },
        RenamePreset {
            name: String::new(),
            rule: RenameRule::default(),
        },
        RenamePreset {
            name: "long".to_owned(),
            rule: RenameRule {
                name_mask: "x".repeat(rc_rename::MAX_RULE_TEXT + 1),
                ..RenameRule::default()
            },
        },
    ];
    db.set(PRESETS_KEY, &stored).await.expect("stored");
    let (list, _) = presets(&engine, Command::MultiRenamePresetList).await;
    assert_eq!(names(&list), ["ok"]);

    let many: Vec<RenamePreset> = (0..MAX_PRESETS + 5)
        .map(|n| RenamePreset {
            name: format!("p{n}"),
            rule: RenameRule::default(),
        })
        .collect();
    assert_eq!(sanitized(many).len(), MAX_PRESETS);
}

#[tokio::test]
async fn ctrl_m_announces_the_dialog_for_the_panel() {
    let (_root, engine, _db) = engine().await;
    let mut events = engine.subscribe();
    engine
        .dispatch(Command::MultiRenameOpenDialog {
            panel: Some(PanelId::Right),
        })
        .await
        .expect("announced");
    let mut seen = false;
    while let Ok(event) = events.try_recv() {
        seen |= event
            == EngineEvent::MultiRenameDialog {
                panel: PanelId::Right,
            };
    }
    assert!(seen);
}
