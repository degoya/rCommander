//! The compare kind (RC-100-41): a compare plugin answers its comparison held to the host's
//! rules, says when it is unlicensed, claims files by its `[match]`, and is refused where its
//! manifest asks for what the kind is not.

use std::sync::{Arc, atomic::AtomicBool};

use rc_plugin_api::compare::{CompareState, MAX_CHANGES};

use super::{CompareOptions, ComparePlugin};
use crate::{
    BytesSource, SourceReader,
    failure::codes,
    manifest::parse_manifest,
    test_fixtures::{compare_manifest, compare_manifest_text},
    testbed::{
        TRAP,
        compare::{CompareBed, Export},
        inventory::InventoryBed,
    },
};

fn plugin(bed: &CompareBed) -> ComparePlugin {
    ComparePlugin::new(&compare_manifest(), &bed.build(), CompareOptions::default())
        .expect("a compare plugin")
}

fn source(name: &str, text: &str) -> Arc<dyn SourceReader> {
    Arc::new(BytesSource::new(name, text.as_bytes().to_vec()))
}

async fn compare(
    plugin: &ComparePlugin,
) -> Result<rc_plugin_api::compare::Comparison, rc_plugin_types::Failure> {
    plugin
        .compare(
            source("a.json", "{\"a\": 1}"),
            source("b.json", "{\"a\": 2}"),
            Arc::new(AtomicBool::new(false)),
        )
        .await
}

#[tokio::test]
async fn a_plugin_that_finds_nothing_answers_an_equal_comparison() {
    let plugin = plugin(&CompareBed::new());
    assert_eq!(plugin.status().await.expect("status"), CompareState::Ready);
    let comparison = compare(&plugin).await.expect("compared");
    assert!(comparison.is_equal());
    assert!(comparison.changes.is_empty());
    assert_eq!(plugin.identity().plugin_type, "compare");
}

#[tokio::test]
async fn an_unlicensed_plugin_says_so() {
    let plugin = plugin(&CompareBed::new().unlicensed());
    assert_eq!(
        plugin.status().await.expect("status"),
        CompareState::Unlicensed
    );
}

#[tokio::test]
async fn the_changes_a_plugin_lists_arrive_checked() {
    let plugin = plugin(&CompareBed::new().changes(3, "/items/0/name"));
    let comparison = compare(&plugin).await.expect("compared");
    assert_eq!(comparison.changes.len(), 3);
    assert_eq!(comparison.summary.changed, 3);
    assert!(
        comparison
            .changes
            .iter()
            .all(|change| change.path == "/items/0/name" && change.old_value.is_none())
    );
}

#[tokio::test]
async fn a_flood_of_changes_is_refused_as_a_whole() {
    let count = u32::try_from(MAX_CHANGES + 1).expect("fits");
    let plugin = plugin(&CompareBed::new().changes(count, "/x"));
    let refused = compare(&plugin).await.expect_err("too many");
    assert_eq!(refused.code(), Some(codes::PLUGIN_COMPARE_TOO_LARGE));
}

#[tokio::test]
async fn a_crash_fails_the_call() {
    let plugin = plugin(&CompareBed::new().body(Export::Compare, TRAP));
    let refused = compare(&plugin).await.expect_err("crashed");
    assert_eq!(refused.code(), Some(codes::PLUGIN_CRASHED));
}

#[test]
fn it_claims_files_by_its_match_and_size() {
    let plugin = plugin(&CompareBed::new());
    assert!(plugin.claims("Data.JSON", 10));
    assert!(plugin.claims("table.csv", 1000));
    assert!(!plugin.claims("table.csv", 1001));
    assert!(!plugin.claims("notes.txt", 10));
}

#[test]
fn the_manifest_holds_the_kind_to_its_rules() {
    for (extra, why) in [
        ("[viewer]\noutputs = [\"text\"]\n", "a viewer section"),
        (
            "[capabilities]\nnet_http = { loopback = 8087 }\n",
            "a grant",
        ),
    ] {
        assert!(
            parse_manifest(compare_manifest_text(extra).as_bytes()).is_err(),
            "{why} is refused"
        );
    }
    let magic = compare_manifest_text("").replace(
        "max_size = 1000",
        "max_size = 1000\nmagic = [{ offset = 0, hex = \"7b\" }]",
    );
    assert!(
        parse_manifest(magic.as_bytes()).is_err(),
        "a magic is refused"
    );
    let old = compare_manifest_text("").replace(rc_plugin_api::API_VERSION, "0.5.0");
    assert!(
        parse_manifest(old.as_bytes()).is_err(),
        "contract 0.5.0 has no compare world"
    );
    let nothing = compare_manifest_text("").replace("extensions = [\"json\", \"csv\"]", "");
    assert!(
        parse_manifest(nothing.as_bytes()).is_err(),
        "it must claim a file"
    );
}

#[test]
fn a_component_of_another_world_is_no_compare_plugin() {
    let refused = ComparePlugin::new(
        &compare_manifest(),
        &InventoryBed::new().build(),
        CompareOptions::default(),
    )
    .err()
    .expect("refused");
    assert!(matches!(
        refused.code(),
        Some(codes::PLUGIN_INVALID | codes::PLUGIN_DENIED)
    ));
}
