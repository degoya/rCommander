//! What never reaches a viewer plugin (RC-100-13): a sensitive file - by its name, or by a
//! private key in its head - and anything of the vault. Acceptance tests *Sensitive files* and
//! *Secret canary*.

use std::{
    fmt::Write as _,
    fs,
    sync::{Arc, Mutex, PoisonError},
};

use rc_plugin_host::testbed::{self, Export, Testbed};
use rc_vfs::VfsPath;
use tracing_subscriber::layer::SubscriberExt as _;

use super::Opening;
use crate::{
    Command, ConnectionDraft, SaveConnectionRequest, SaveFields, SecretChange, SecretKind,
    SecretParam,
    plugins::test_support::{Spec, judge, one, request},
    test_support::engine_in,
};

const KEY_BLOCK: &str = "-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXktdjEAAAAA\n-----END OPENSSH PRIVATE KEY-----\n";
const MASTER: &str = "correct horse battery";
const PASSWORD: &str = "canary-viewer-password";
const PASSPHRASE: &str = "fixture-passphrase";
/// `rc-remote`'s throw-away test keys.
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../rc-remote/tests/fixtures");

/// A plugin for `.txt` and anything starting with `-----`.
fn dashes() -> Spec<'static> {
    Spec {
        extensions: &["txt", "csv"],
        magic: Some("2d2d2d2d2d"),
        ..Spec::default()
    }
}

/// *Sensitive files*: Quick View on `id_ed25519`, or on a file whose head holds a private key,
/// runs no plugin; the built-in viewer shows both.
#[tokio::test]
async fn quick_view_on_a_key_calls_no_plugin() {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = engine_in(dir.path()).await;
    fs::create_dir_all(dir.path().join(".ssh")).expect("mkdir");
    fs::write(dir.path().join(".ssh/id_ed25519"), KEY_BLOCK).expect("write");
    fs::write(dir.path().join("pasted.txt"), KEY_BLOCK).expect("write");
    fs::write(dir.path().join("notes.txt"), "----- a rule").expect("write");
    let set = one(&dashes(), &Testbed::new());
    let open = |name: &str| {
        let path = VfsPath::local(dir.path().join(name));
        let set = &set;
        let engine = engine.clone();
        async move { set.open(&engine, &path, judge, request()).await }
    };
    for name in [".ssh/id_ed25519", "pasted.txt"] {
        let opening = open(name).await;
        assert!(matches!(opening, Opening::BuiltIn), "{name}: {opening:?}");
    }
    let records = set
        .execution_log()
        .records("019d0000-0000-7000-8000-0000000000a1");
    assert!(records.is_empty(), "no call ran: {records:?}");
    assert!(
        matches!(open("notes.txt").await, Opening::Document(_)),
        "the same plugin claims a file that is none"
    );
}

/// Every log line, all levels, with its fields.
#[derive(Clone, Default)]
struct Lines(Arc<Mutex<Vec<String>>>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Lines {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        let mut line = String::new();
        event.record(
            &mut |field: &tracing::field::Field, value: &dyn std::fmt::Debug| {
                let _ = write!(line, "{}={value:?} ", field.name());
            },
        );
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(line);
    }
}

/// A vault holding a password, a key and its passphrase.
async fn vault_with_canaries(engine: &crate::Engine) {
    engine
        .dispatch(Command::CreateVault {
            password: SecretParam::from(MASTER),
        })
        .await
        .expect("vault created");
    let request = SaveConnectionRequest {
        fields: SaveFields::Create {
            folder_id: None,
            connection: ConnectionDraft::new("canary", "example.org"),
        },
        secrets: vec![
            SecretChange::Set {
                kind: SecretKind::Password,
                value: SecretParam::from(PASSWORD),
            },
            SecretChange::ImportKey {
                path: format!("{FIXTURES}/client_key_rsapem"),
                passphrase: Some(SecretParam::from(PASSPHRASE)),
            },
        ],
        login_script: None,
        favourite: None,
        credential: None,
    };
    engine
        .dispatch(Command::SaveConnection(request))
        .await
        .expect("saved");
}

/// *Secret canary*: the test bed hands back everything it reaches - its source into the output
/// and into the log - and no secret of the vault is in the output, the log or the execution log.
#[tokio::test]
async fn a_plugin_that_hands_back_all_it_reaches_reaches_no_secret() {
    let lines = Lines::default();
    let _logging =
        tracing::subscriber::set_default(tracing_subscriber::registry().with(lines.clone()));
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = engine_in(dir.path()).await;
    vault_with_canaries(&engine).await;
    let key_file = fs::read_to_string(format!("{FIXTURES}/client_key_rsapem")).expect("fixture");
    let key_line = key_file
        .lines()
        .find(|line| line.len() >= 60)
        .expect("a line of the key's body")
        .to_owned();
    fs::create_dir_all(dir.path().join(".ssh")).expect("mkdir");
    fs::write(dir.path().join(".ssh/id_rsa"), &key_file).expect("write");
    fs::write(dir.path().join("data.csv"), "name,size\nreport,12\n").expect("write");

    let mut bed = Testbed::new();
    let offered = bed.data(&[testbed::output("text")]);
    let (warn, utf8) = (bed.data(b"warn"), bed.data(b"utf-8"));
    let open = format!(
        "{}{}",
        testbed::remember_source(),
        testbed::ok_document(offered, None, None)
    );
    let bed = bed
        .body(Export::Open, open)
        .body(Export::Text, testbed::echo_source(warn, utf8));
    let set = one(&dashes(), &bed);

    let data = VfsPath::local(dir.path().join("data.csv"));
    let shown = match set.open(&engine, &data, judge, request()).await {
        Opening::Document(document) => *document,
        other => panic!("a document, got {other:?}"),
    };
    let output = fs::read_to_string(shown.text_path().expect("a file")).expect("written");
    assert_eq!(
        output, "name,size\nreport,12\n",
        "the plugin echoed its source"
    );
    let key = VfsPath::local(dir.path().join(".ssh/id_rsa"));
    let refused = set.open(&engine, &key, judge, request()).await;
    assert!(matches!(refused, Opening::BuiltIn), "{refused:?}");

    let mut seen = output;
    let _ = write!(seen, "{shown:?} {refused:?}");
    let records = set
        .execution_log()
        .records("019d0000-0000-7000-8000-0000000000a1");
    let _ = write!(seen, "{records:?}");
    seen.push_str(
        &lines
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .join("\n"),
    );
    assert!(
        seen.contains("report,12"),
        "the log holds what the plugin wrote: {seen}"
    );
    for canary in [MASTER, PASSWORD, PASSPHRASE, key_line.as_str()] {
        assert!(!seen.contains(canary), "{canary} reached {seen}");
    }
}
