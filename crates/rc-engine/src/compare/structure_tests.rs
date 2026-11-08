//! Compare by structure in the engine (RC-100-41), over an installed test compare plugin: the
//! plugin claiming both files compares them, an unlicensed one leaves the line diff alone, files
//! it does not claim, larger than the limit or holding a key never reach it, and
//! `compare.files` carries the outcome when asked.

use rc_db::Db;
use rc_plugin_host::{
    key_fingerprint,
    test_support::{compare_package, signing_key},
    testbed::compare::CompareBed,
};
use rc_vfs::VfsPath;
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

use super::StructureOutcome;
use crate::{
    Command, Engine, Outcome, compare::FileCompareRequest, settings::COMPARE_LIMIT_KEY,
    test_support,
};

const ID: &str = "019d0000-0000-7000-8000-0000000000d1";

struct Setup {
    root: TempDir,
    engine: Engine,
    db: Db,
}

impl Setup {
    /// An engine with a database file, the compare plugin of `bed` installed when there is one,
    /// and `files` in its folder.
    async fn new(bed: Option<&CompareBed>, files: &[(&str, &str)]) -> Self {
        let root = test_support::tree(files);
        let engine = test_support::headless(root.path()).await;
        let db = Db::open(root.path().join("rcommander.db"))
            .await
            .expect("db");
        assert!(engine.attach_db(db.clone()));
        assert!(engine.attach_plugins(root.path().join("data"), None));
        if let Some(bed) = bed {
            let package = root.path().join("compare.rcplug");
            let key = signing_key(11);
            let bytes = compare_package(ID, "tests-v1", &key, &bed.build());
            std::fs::write(&package, &bytes).expect("write");
            engine
                .dispatch(Command::InstallPlugin {
                    path: package.display().to_string(),
                    fingerprint: Some(key_fingerprint(&key.verifying_key())),
                    digest: rc_plugin_host::sha256_hex(&bytes),
                })
                .await
                .expect("the window installs");
        }
        Self { root, engine, db }
    }

    fn path(&self, name: &str) -> VfsPath {
        VfsPath::local(self.root.path().join(name))
    }

    async fn compare(&self, left: &str, right: &str) -> StructureOutcome {
        self.engine
            .structure_paths(
                &self.path(left),
                &self.path(right),
                &CancellationToken::new(),
            )
            .await
            .expect("an outcome")
    }
}

const JSON: [(&str, &str); 3] = [
    ("a.json", "{\"id\": 1, \"name\": \"a\"}"),
    ("b.json", "{\"name\": \"b\", \"id\": 1}"),
    ("notes.txt", "text"),
];

#[tokio::test]
async fn the_plugin_that_claims_both_files_compares_them() {
    let setup = Setup::new(Some(&CompareBed::new().changes(2, "/name")), &JSON).await;
    let outcome = setup.compare("a.json", "b.json").await;
    let StructureOutcome::Compared {
        plugin,
        plugin_id,
        comparison,
        notices,
    } = outcome
    else {
        panic!("compared, got {outcome:?}");
    };
    assert_eq!(plugin, "Test compare");
    assert_eq!(plugin_id, ID);
    assert_eq!(comparison.changes.len(), 2);
    assert_eq!(comparison.changes[0].path, "/name");
    assert!(notices.is_empty());
}

#[test]
fn a_notice_reads_in_the_plugins_words() {
    let codes = std::collections::BTreeMap::from([(
        "x.duplicate_key".to_owned(),
        "Key {key} is there {count} times".to_owned(),
    )]);
    let notice = super::Notice {
        code: "x.duplicate_key".to_owned(),
        params: vec![
            ("key".to_owned(), "7".to_owned()),
            ("count".to_owned(), "2".to_owned()),
        ],
    };
    assert_eq!(
        super::notice_text(&codes, &notice),
        "Key 7 is there 2 times"
    );
    let unknown = super::Notice {
        code: "x.other".to_owned(),
        params: Vec::new(),
    };
    assert_eq!(super::notice_text(&codes, &unknown), "x.other");
}

#[tokio::test]
async fn without_a_plugin_or_a_claim_there_is_nothing_to_compare() {
    let none = Setup::new(None, &JSON).await;
    assert_eq!(
        none.compare("a.json", "b.json").await,
        StructureOutcome::NoPlugin
    );
    let installed = Setup::new(Some(&CompareBed::new()), &JSON).await;
    assert_eq!(
        installed.compare("a.json", "notes.txt").await,
        StructureOutcome::NoPlugin
    );
}

#[tokio::test]
async fn an_unlicensed_plugin_leaves_the_line_diff_alone() {
    let setup = Setup::new(Some(&CompareBed::new().unlicensed()), &JSON).await;
    assert_eq!(
        setup.compare("a.json", "b.json").await,
        StructureOutcome::Unlicensed {
            plugin: "Test compare".to_owned()
        }
    );
}

#[tokio::test]
async fn a_file_over_the_limit_is_not_handed_over() {
    let big = format!("[{}]", "1,".repeat(600_000) + "1");
    let setup = Setup::new(
        Some(&CompareBed::new()),
        &[("a.json", "[]"), ("b.json", big.as_str())],
    )
    .await;
    setup.db.set(COMPARE_LIMIT_KEY, &1_u64).await.expect("set");
    assert_eq!(
        setup.compare("a.json", "b.json").await,
        StructureOutcome::TooLarge { limit_mib: 1 }
    );
}

#[tokio::test]
async fn a_file_holding_a_private_key_never_reaches_a_plugin() {
    let key = "{\"key\": \"-----BEGIN OPENSSH PRIVATE KEY-----\\nabc\"}";
    let setup = Setup::new(
        Some(&CompareBed::new()),
        &[("a.json", "{}"), ("b.json", key)],
    )
    .await;
    assert_eq!(
        setup.compare("a.json", "b.json").await,
        StructureOutcome::Sensitive
    );
}

#[tokio::test]
async fn a_crashing_plugin_is_a_failed_outcome() {
    let crashing = CompareBed::new().body(
        rc_plugin_host::testbed::compare::Export::Compare,
        rc_plugin_host::testbed::TRAP,
    );
    let setup = Setup::new(Some(&crashing), &JSON).await;
    let outcome = setup.compare("a.json", "b.json").await;
    assert_eq!(
        outcome,
        StructureOutcome::Failed {
            plugin: "Test compare".to_owned(),
            code: rc_plugin_host::codes::PLUGIN_CRASHED.to_owned()
        }
    );
}

#[tokio::test]
async fn compare_files_carries_the_structure_when_asked() {
    let setup = Setup::new(Some(&CompareBed::new()), &JSON).await;
    let request = |structure| FileCompareRequest {
        left: Some(setup.path("a.json").path.display().to_string()),
        right: Some(setup.path("b.json").path.display().to_string()),
        structure,
        ..FileCompareRequest::default()
    };
    let asked = setup
        .engine
        .dispatch(Command::CompareFiles(request(true)))
        .await
        .expect("compared");
    let Outcome::FileComparison(asked) = asked else {
        panic!("a file comparison");
    };
    assert!(
        asked
            .structure
            .as_ref()
            .and_then(StructureOutcome::comparison)
            .is_some_and(|comparison| comparison.is_equal())
    );
    let plain = setup
        .engine
        .dispatch(Command::CompareFiles(request(false)))
        .await
        .expect("compared");
    let Outcome::FileComparison(plain) = plain else {
        panic!("a file comparison");
    };
    assert!(plain.structure.is_none());
}
