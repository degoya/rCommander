//! Server inventories in the engine (RC-100-39), over an installed test inventory: a preview
//! plans, a sync creates the folder and its connections, a second one changes nothing, the
//! schedule syncs only what is due, and MCP lists and syncs but never sets a source up.

use chrono::{Duration, Utc};
use rc_db::Db;
use rc_plugin_api::inventory::{InventoryHost, Protocol};
use rc_plugin_host::{
    key_fingerprint,
    test_support::{inventory_package, signing_key},
    testbed::{
        self,
        inventory::{HostData, InventoryBed, SFTP, SSH},
    },
};

use super::{
    InventoryOutcome, InventoryReport, InventorySource, InventorySourceId, InventorySourceRequest,
    commands::entry, schedule::due,
};
use crate::{
    Command, ConnectionKind, DefaultView, Engine, EngineError, Outcome, SecretParam, test_support,
};

const ID: &str = "019d0000-0000-7000-8000-0000000000c1";

/// Two hosts: an SSH server in `dc1/web` and an SFTP server at the top.
fn two_hosts() -> InventoryBed {
    let mut bed = InventoryBed::new();
    let tags = [bed.text("prod")];
    let tag_list = bed.data(&testbed::list_bytes(&tags));
    let first = HostData {
        external_id: bed.text("17"),
        name: bed.text("web-01"),
        address: bed.text("web-01.example.net"),
        port: 22,
        protocol: SSH,
        user: Some(bed.text("deploy")),
        folder: bed.text("dc1/web"),
        // A list's length counts its strings, not its bytes.
        tags: (tag_list.0, 1),
        note: None,
    };
    let second = HostData {
        external_id: bed.text("18"),
        name: bed.text("files"),
        address: bed.text("10.0.0.8"),
        port: 2222,
        protocol: SFTP,
        user: None,
        folder: bed.text(""),
        tags: (0, 0),
        note: None,
    };
    bed.hosts(&[first, second])
}

struct Setup {
    _root: tempfile::TempDir,
    engine: Engine,
    db: Db,
    source: InventorySourceId,
}

impl Setup {
    /// An engine with a database file, the inventory of `bed` installed and one source of it.
    async fn new(bed: &InventoryBed) -> Self {
        let root = tempfile::tempdir().expect("temp dir");
        let engine = test_support::headless(root.path()).await;
        let db = Db::open(root.path().join("rcommander.db"))
            .await
            .expect("db");
        assert!(engine.attach_db(db.clone()));
        assert!(engine.attach_plugins(root.path().join("data"), None));
        let package = root.path().join("inventory.rcplug");
        let key = signing_key(9);
        let bytes = inventory_package(ID, "tests-v1", &key, &bed.build());
        std::fs::write(&package, &bytes).expect("write");
        engine
            .dispatch(Command::InstallPlugin {
                path: package.display().to_string(),
                fingerprint: Some(key_fingerprint(&key.verifying_key())),
                digest: rc_plugin_host::sha256_hex(&bytes),
            })
            .await
            .expect("the window installs");
        let saved = engine
            .dispatch(Command::SaveInventorySource(InventorySourceRequest {
                id: None,
                name: "NetBox".into(),
                plugin_id: ID.into(),
                address: "https://netbox.example.net/".into(),
                folder_id: None,
                secrets: Vec::new(),
            }))
            .await
            .expect("the window saves a source");
        let Outcome::Inventory(outcome) = saved else {
            panic!("an inventory outcome");
        };
        let InventoryOutcome::Saved { source } = *outcome else {
            panic!("a saved source");
        };
        Self {
            _root: root,
            engine,
            db,
            source: source.id,
        }
    }

    async fn refresh(&self, preview: bool) -> InventoryReport {
        let outcome = self
            .engine
            .dispatch(Command::RefreshInventory {
                source: self.source,
                preview,
            })
            .await
            .expect("a sync");
        let Outcome::Inventory(outcome) = outcome else {
            panic!("an inventory outcome");
        };
        let InventoryOutcome::Synced(report) = *outcome else {
            panic!("a report");
        };
        report
    }
}

#[tokio::test]
async fn a_preview_plans_and_a_sync_keeps_the_folder_in_step() {
    let setup = Setup::new(&two_hosts()).await;
    let preview = setup.refresh(true).await;
    assert!(preview.preview);
    assert_eq!(preview.created, 2);
    assert!(setup.db.inventory_links().await.expect("links").is_empty());

    let first = setup.refresh(false).await;
    assert_eq!((first.created, first.updated, first.unchanged), (2, 0, 0));
    let links = setup.db.inventory_links().await.expect("links");
    assert_eq!(links.len(), 2);
    let web = setup
        .db
        .connection(links[0].connection)
        .await
        .expect("a connection");
    assert_eq!(web.name, "web-01");
    assert_eq!(web.default_view, DefaultView::Terminal);
    assert_eq!(web.notes.as_deref(), Some("Tags: prod"));

    let second = setup.refresh(false).await;
    assert_eq!(
        (second.created, second.updated, second.unchanged),
        (0, 0, 2)
    );
    assert_eq!(second.missing, 0);
}

#[tokio::test]
async fn a_failing_inventory_is_recorded_with_its_source() {
    let setup = Setup::new(&InventoryBed::new().body(testbed::TRAP)).await;
    let error = setup
        .engine
        .dispatch(Command::RefreshInventory {
            source: setup.source,
            preview: false,
        })
        .await
        .expect_err("crashed");
    assert_eq!(error.code(), "plugin_crashed");
    let source = setup
        .db
        .inventory_source(setup.source)
        .await
        .expect("the source");
    assert_eq!(source.last_outcome.as_deref(), Some("plugin_crashed"));
}

/// Listing and syncing reach MCP; setting a source up or removing it does not.
#[tokio::test]
async fn mcp_lists_and_syncs_but_never_sets_a_source_up() {
    let setup = Setup::new(&two_hosts()).await;
    let listed = setup
        .engine
        .dispatch_mcp(Command::ListInventorySources {})
        .await
        .expect("a read");
    let Outcome::Inventory(outcome) = listed else {
        panic!("an inventory outcome");
    };
    let InventoryOutcome::Sources { sources, plugins } = *outcome else {
        panic!("the sources");
    };
    assert_eq!(sources.len(), 1);
    assert_eq!(plugins.len(), 1);
    let refused = setup
        .engine
        .dispatch_mcp(Command::SaveInventorySource(InventorySourceRequest {
            id: Some(setup.source),
            name: "NetBox".into(),
            plugin_id: ID.into(),
            address: "https://evil.example.net/".into(),
            folder_id: None,
            secrets: vec![super::InventorySecret {
                slot: "api_token".into(),
                value: SecretParam::from("canary-token-5521"),
            }],
        }))
        .await
        .expect_err("gui only");
    assert!(matches!(refused, EngineError::GuiOnly { .. }), "{refused}");
    let refused = setup
        .engine
        .dispatch_mcp(Command::DeleteInventorySource {
            source: setup.source,
        })
        .await
        .expect_err("gui only");
    assert!(matches!(refused, EngineError::GuiOnly { .. }), "{refused}");
}

#[test]
fn a_host_becomes_an_entry_by_its_protocol() {
    let host = |protocol| InventoryHost {
        external_id: "1".into(),
        name: "a".into(),
        address: "a.lan".into(),
        port: 21,
        protocol,
        user: None,
        folder: "dc1/web".into(),
        tags: vec!["prod".into(), "web".into()],
        note: Some("rack 4".into()),
    };
    let ssh = entry(&host(Protocol::Ssh));
    assert_eq!(
        (ssh.kind, ssh.default_view),
        (ConnectionKind::Ssh, DefaultView::Terminal)
    );
    assert_eq!(ssh.folder, ["dc1", "web"]);
    assert_eq!(ssh.notes.as_deref(), Some("rack 4\nTags: prod, web"));
    let sftp = entry(&host(Protocol::Sftp));
    assert_eq!(
        (sftp.kind, sftp.default_view),
        (ConnectionKind::Ssh, DefaultView::Files)
    );
    assert_eq!(entry(&host(Protocol::Ftps)).kind, ConnectionKind::Ftps);
}

/// The schedule syncs a source synced before once it is due, never one waiting for its preview.
#[test]
fn a_source_is_due_after_its_minutes() {
    let source = |last: Option<String>| InventorySource {
        id: InventorySourceId(1),
        name: "NetBox".into(),
        plugin_id: ID.into(),
        address: "https://netbox.example.net/".into(),
        folder_id: None,
        last_sync_at: last,
        last_outcome: None,
        slots: Vec::new(),
    };
    let now = Utc::now();
    let stamp = |minutes_ago: i64| {
        Some(
            (now - Duration::minutes(minutes_ago))
                .format("%Y-%m-%dT%H:%M:%S%.3fZ")
                .to_string(),
        )
    };
    assert!(
        !due(&source(None), 60, now),
        "never synced: the preview first"
    );
    assert!(!due(&source(stamp(10)), 60, now));
    assert!(due(&source(stamp(61)), 60, now));
    assert!(!due(&source(Some("not a time".into())), 60, now));
}
