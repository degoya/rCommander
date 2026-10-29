//! The Host keys page through `apply`, against an engine over a database: it lists the trusted
//! keys and certificates, searches them, removes one only after a yes, reports an import and
//! follows a change made elsewhere.

use rc_db::{Db, NewTrustedCertificate};
use rc_engine::{Command, Engine, ImportReport, Outcome};
use tempfile::TempDir;

use super::{HostKeysMsg, Item, Stored, TrustAnswer, TrustStoreCall, page_message};
use crate::app::{
    Effect, Message,
    harness::Harness,
    settings::{Page, SettingsMessage, host_keys_view::shown_time},
};

const PLAIN: &str = include_str!("../../../../rc-remote/tests/fixtures/known_hosts/known_hosts");
const HASHED: &str =
    include_str!("../../../../rc-remote/tests/fixtures/known_hosts/hashed_known_hosts");

/// The keys of both fixtures (four plain, two hashed) and one FTPS certificate.
const ALL: usize = 7;

async fn harness(root: &TempDir) -> Harness {
    let db = Db::open_in_memory().await.expect("db");
    db.add_trusted_certificate(&NewTrustedCertificate {
        host: "nas.example".to_owned(),
        port: 990,
        fingerprint_sha256: format!("AB:CD:EF{}", ":00".repeat(29)),
        subject: "CN=nas.example".to_owned(),
        issuer: "CN=nas.example".to_owned(),
        not_after: i64::MAX / 2,
    })
    .await
    .expect("certificate");
    let mut harness = Harness::on_db(root.path(), db).await;
    import(harness.app.engine(), root, "known_hosts", PLAIN).await;
    import(harness.app.engine(), root, "hashed", HASHED).await;
    // The window was not open during the imports: it saw none of them.
    harness.miss_events();
    harness
}

/// Imports `text` as the file `name` through the engine, as another part of the app would.
async fn import(engine: &Engine, root: &TempDir, name: &str, text: &str) -> ImportReport {
    let path = root.path().join(name);
    std::fs::write(&path, text).expect("write");
    match engine
        .dispatch(Command::ImportKnownHosts {
            path: Some(path.display().to_string()),
        })
        .await
        .expect("import")
    {
        Outcome::KnownHostsImported(report) => report,
        other => panic!("an import report, got {other:?}"),
    }
}

/// The settings, turned to the Host keys page, its table read.
async fn shown(root: &TempDir) -> Harness {
    let mut harness = harness(root).await;
    harness
        .send(Message::Settings(SettingsMessage::OpenPage(Page::HostKeys)))
        .await;
    harness
}

fn entries(harness: &Harness) -> &[Stored] {
    harness
        .app
        .settings
        .as_ref()
        .and_then(|dialog| dialog.host_keys.entries.as_deref())
        .expect("the table is read")
}

fn page(harness: &Harness) -> &super::HostKeysPage {
    &harness.app.settings.as_ref().expect("open").host_keys
}

async fn send(harness: &mut Harness, message: HostKeysMsg) {
    harness.send(page_message(message)).await;
}

#[tokio::test]
async fn the_page_lists_keys_and_certificates_by_server_hashed_names_last() {
    let root = TempDir::new().expect("temp dir");
    let harness = shown(&root).await;
    let entries = entries(&harness);
    assert_eq!(entries.len(), ALL);
    let certificates: Vec<&Stored> = entries
        .iter()
        .filter(|stored| matches!(stored, Stored::Certificate(_)))
        .collect();
    assert_eq!(certificates.len(), 1);
    assert_eq!(certificates[0].server(), "nas.example:990");
    let hashed = entries
        .iter()
        .position(Stored::hashed)
        .expect("a hashed name");
    assert!(entries[hashed..].iter().all(Stored::hashed), "hashed last");
    assert_eq!(entries[hashed].port(), None);
    assert!(
        entries
            .iter()
            .all(|stored| stored.fingerprint().len() > 4 && !stored.added().is_empty())
    );
}

#[tokio::test]
async fn turning_to_the_page_reads_it_and_another_page_reads_nothing() {
    let root = TempDir::new().expect("temp dir");
    let mut harness = harness(&root).await;
    harness.send(Message::Settings(SettingsMessage::Open)).await;
    assert!(page(&harness).entries.is_none(), "read only when shown");
    harness
        .send(Message::Settings(SettingsMessage::Page(Page::HostKeys)))
        .await;
    assert_eq!(entries(&harness).len(), ALL);
}

#[tokio::test]
async fn the_search_finds_types_hosts_and_hashed_names_and_drops_an_older_answer() {
    let root = TempDir::new().expect("temp dir");
    let mut harness = shown(&root).await;
    send(&mut harness, HostKeysMsg::Filter("nas.example".into())).await;
    assert_eq!(entries(&harness).len(), 1, "the certificate");
    // `box.example` names two plain rows and the hashed row of port 22.
    send(&mut harness, HostKeysMsg::Filter("box.example".into())).await;
    assert_eq!(entries(&harness).len(), 3);
    assert_eq!(entries(&harness).iter().filter(|s| s.hashed()).count(), 1);
    send(&mut harness, HostKeysMsg::Filter("no-such-host".into())).await;
    assert!(entries(&harness).is_empty());

    let stale = TrustAnswer::Listed {
        filter: "box.example".into(),
        result: Ok(Vec::new()),
    };
    harness
        .app
        .settings
        .as_mut()
        .expect("open")
        .host_keys
        .entries = None;
    send(&mut harness, HostKeysMsg::Answered(Box::new(stale))).await;
    assert!(
        page(&harness).entries.is_none(),
        "an answer for an older search"
    );
}

#[tokio::test]
async fn delete_asks_first_escape_keeps_the_key_and_enter_removes_it() {
    let root = TempDir::new().expect("temp dir");
    let mut harness = shown(&root).await;
    harness.press("ArrowDown").await;
    let first = entries(&harness)[0].item();
    assert_eq!(page(&harness).selected, Some(first));

    harness.press("Delete").await;
    assert_eq!(page(&harness).asking, Some(first));
    harness.press("Escape").await;
    assert!(
        harness.app.settings.is_some(),
        "Escape took the question only"
    );
    assert_eq!(page(&harness).asking, None);
    assert_eq!(entries(&harness).len(), ALL);

    harness.press("Delete").await;
    harness.press("Enter").await;
    assert!(
        harness.app.settings.is_some(),
        "Enter took the question only"
    );
    // The engine's event read the table again.
    assert_eq!(entries(&harness).len(), ALL - 1);
    assert!(
        entries(&harness)
            .iter()
            .all(|stored| stored.item() != first)
    );
    assert_eq!(page(&harness).selected, None);
    let dialog = harness.app.settings.as_ref().expect("open");
    assert!(dialog.note.as_ref().is_some_and(|(_, error)| !error));
}

#[tokio::test]
async fn a_certificate_is_removed_after_a_yes() {
    let root = TempDir::new().expect("temp dir");
    let mut harness = shown(&root).await;
    let certificate = entries(&harness)
        .iter()
        .find(|stored| matches!(stored, Stored::Certificate(_)))
        .map(Stored::item)
        .expect("the certificate");
    assert!(matches!(certificate, Item::Certificate(_)));
    send(&mut harness, HostKeysMsg::Select(certificate)).await;
    send(&mut harness, HostKeysMsg::Remove).await;
    assert_eq!(entries(&harness).len(), ALL, "nothing goes without a yes");
    send(&mut harness, HostKeysMsg::ConfirmRemove).await;
    assert_eq!(entries(&harness).len(), ALL - 1);
    assert!(
        entries(&harness)
            .iter()
            .all(|stored| matches!(stored, Stored::Key(_)))
    );
}

#[tokio::test]
async fn the_arrows_walk_the_rows_and_stop_at_the_ends() {
    let root = TempDir::new().expect("temp dir");
    let mut harness = shown(&root).await;
    harness.press("ArrowUp").await;
    assert_eq!(page(&harness).selected, Some(entries(&harness)[0].item()));
    for _ in 0..ALL + 2 {
        harness.press("ArrowDown").await;
    }
    assert_eq!(
        page(&harness).selected,
        Some(entries(&harness)[ALL - 1].item())
    );
}

#[tokio::test]
async fn import_reads_the_users_file_and_its_report_names_the_conflicts() {
    let root = TempDir::new().expect("temp dir");
    let mut harness = shown(&root).await;
    // The button imports the user's own file: the engine's default path, no path of the page's.
    let effects = harness.app.apply(page_message(HostKeysMsg::Import));
    assert!(matches!(
        effects.as_slice(),
        [Effect::TrustStore(TrustStoreCall::Import)]
    ));
    assert!(page(&harness).busy);

    // A file with a key that differs from a stored one: reported, the stored key stays.
    let other = include_str!("../../../../rc-remote/tests/fixtures/known_hosts/other.pub");
    let mut fields = other.split_whitespace();
    let line = format!(
        "box.example {} {}\n",
        fields.next().expect("type"),
        fields.next().expect("key")
    );
    let report = import(harness.app.engine(), &root, "changed", &line).await;
    assert_eq!(report.conflicts.len(), 1);
    let answer = TrustAnswer::Imported(Ok(report));
    send(&mut harness, HostKeysMsg::Answered(Box::new(answer))).await;
    assert!(!page(&harness).busy);
    let (note, error) = harness
        .app
        .settings
        .as_ref()
        .and_then(|dialog| dialog.note.clone())
        .expect("a note");
    assert!(error, "a conflict is worth a look");
    assert!(note.contains("1 changed"), "{note}");
    assert_eq!(entries(&harness).len(), ALL);
}

#[tokio::test]
async fn a_key_removed_elsewhere_leaves_the_table_at_once() {
    let root = TempDir::new().expect("temp dir");
    let mut harness = shown(&root).await;
    send(&mut harness, HostKeysMsg::Filter("box.example".into())).await;
    let before = entries(&harness).len();
    let keys = match harness
        .app
        .engine()
        .dispatch(Command::ListHostKeys {
            filter: Some("box.example".into()),
        })
        .await
        .expect("list")
    {
        Outcome::KnownHosts { keys } => keys,
        other => panic!("keys, got {other:?}"),
    };
    harness
        .app
        .engine()
        .dispatch(Command::RemoveHostKey { id: keys[0].id })
        .await
        .expect("removed");
    harness.settle().await;
    assert_eq!(entries(&harness).len(), before - 1, "the search is kept");
}

#[test]
fn the_added_time_is_shown_to_the_minute() {
    assert_eq!(shown_time("2026-10-18T09:15:42.123Z"), "2026-10-18 09:15");
    assert_eq!(shown_time("yesterday"), "yesterday");
}
