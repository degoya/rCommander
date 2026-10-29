//! Opening a file with its plugin against the host's test bed: the document and its file, the
//! token spans asked by line start, a crash falling back with its code, and Quick View running
//! through twenty files with at most one session left (RC-100-13); a declining `probe` handing
//! the file to the next plugin that claims it (RC-100-27).

use std::{
    collections::BTreeMap,
    fs,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use rc_plugin_host::{
    ExecutionLog, ExecutionRecord,
    testbed::{self, Export, Testbed},
};
use rc_vfs::VfsPath;

use super::Opening;
use crate::{
    Command, Outcome,
    plugins::{
        Output, PluginDocument, TokenClass, ViewerPlugins,
        test_support::{Spec, judge, one, plugin, request},
    },
    test_support::engine,
};

/// `open` offering `outputs` and `text` writing `text`, as one write or two out of order.
fn writing(outputs: &[&str], text: &[u8], in_order: bool) -> Testbed {
    let mut bed = Testbed::new();
    let offered: Vec<u8> = outputs.iter().map(|name| testbed::output(name)).collect();
    let offered = bed.data(&offered);
    let utf8 = bed.data(b"utf-8");
    let body = if in_order {
        testbed::write(0, 0, bed.data(text))
    } else {
        let cut = text.len() / 2;
        let tail = bed.data(&text[cut..]);
        let head = bed.data(&text[..cut]);
        let at = u64::try_from(cut).expect("small");
        format!(
            "{} {}",
            testbed::write(0, at, tail),
            testbed::write(0, 0, head)
        )
    };
    bed.body(Export::Open, testbed::ok_document(offered, None, None))
        .body(Export::Text, format!("{body} {}", testbed::ok_pair(utf8)))
}

/// `tokens` answering, for the first line asked, one `key` span from 0 as long as the line's
/// number plus one: what the plugin was asked is in its answer.
fn numbering(bed: Testbed) -> Testbed {
    let mut bed = bed;
    let span = bed.data(&testbed::token_bytes(&[(0, 1, 1)]));
    // `data` answers a length in bytes; a `list<token>` counts tokens of 12 bytes.
    let lists = bed.data(&testbed::list_bytes(&[(span.0, 1)]));
    let body = format!(
        "(i32.store (i32.const {}) (i32.add (local.get 0) (i32.const 1)))\n{}",
        span.0 + 4,
        testbed::ok_pair((lists.0, 1))
    );
    bed.body(Export::Tokens, body)
}

async fn opened(set: &ViewerPlugins, engine: &crate::Engine, path: VfsPath) -> Opening {
    set.open(engine, &path, judge, request()).await
}

fn document(opening: Opening) -> PluginDocument {
    match opening {
        Opening::Document(document) => *document,
        other => panic!("a document, got {other:?}"),
    }
}

#[tokio::test]
async fn the_text_lands_in_a_file_of_the_session_that_goes_with_the_document() {
    let (root, engine) = engine(&[("data.csv", "a,b\n1,2\n")]).await;
    let set = one(&Spec::default(), &Testbed::new());
    let path = VfsPath::local(root.path().join("data.csv"));
    let shown = document(opened(&set, &engine, path).await);
    assert_eq!(shown.outputs(), [Output::Text]);
    assert_eq!(shown.plugin(), "Test bed");
    let output = shown.text_path().expect("a file").to_path_buf();
    assert_eq!(fs::read(&output).expect("written"), b"hello");
    assert_eq!(set.live_sessions(), 1);
    drop(shown);
    assert_eq!(set.live_sessions(), 0);
    assert!(!output.exists(), "the file goes with the document");

    let other = VfsPath::local(root.path().join("notes.txt"));
    fs::write(&other.path, "plain").expect("write");
    assert!(
        matches!(opened(&set, &engine, other).await, Opening::BuiltIn),
        "a file no plugin claims"
    );
}

#[tokio::test]
async fn tokens_are_asked_by_line_number_with_the_lines_text() {
    let (root, engine) = engine(&[("data.csv", "a,b\n")]).await;
    let path = VfsPath::local(root.path().join("data.csv"));
    let outputs = ["highlighted", "tree"];
    for in_order in [true, false] {
        let bed = numbering(writing(&outputs, b"ab\ncd\nef", in_order));
        let set = one(
            &Spec {
                outputs: &outputs,
                ..Spec::default()
            },
            &bed,
        );
        let shown = document(opened(&set, &engine, path.clone()).await);
        assert_eq!(shown.outputs(), [Output::Highlighted, Output::Tree]);
        let first = shown.tokens(&[3]).await.expect("tokens");
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].len(), 1, "in order: {in_order}");
        assert_eq!(
            first[0][0].len, 2,
            "line 1 was asked (in order: {in_order})"
        );
        assert_eq!(first[0][0].class, TokenClass::Key);

        let two = shown.tokens(&[0, 3]).await.expect("tokens");
        assert_eq!(two[0][0].len, 1, "one call from line 0");
        assert!(two[1].is_empty(), "the plugin answered one line");
        assert!(shown.children(None).await.expect("children").is_empty());
    }
}

/// A crash: the notice's code and the built-in viewer; the engine's next command runs, and so
/// does the plugin's next call.
#[tokio::test]
async fn a_crashing_plugin_falls_back_with_its_code_and_nothing_waits() {
    let (root, engine) = engine(&[("data.csv", "a,b\n")]).await;
    let set = one(
        &Spec::default(),
        &Testbed::new().body(Export::Open, testbed::TRAP),
    );
    let path = VfsPath::local(root.path().join("data.csv"));
    match opened(&set, &engine, path.clone()).await {
        Opening::Failed { plugin, code } => {
            assert_eq!(plugin, "Test bed");
            assert_eq!(code, "plugin_crashed");
        }
        other => panic!("a failure, got {other:?}"),
    }
    assert_eq!(set.live_sessions(), 0);
    let listing = engine
        .dispatch(Command::ListDir {
            panel: None,
            path: None,
        })
        .await;
    assert!(
        matches!(listing, Ok(Outcome::Listing { .. })),
        "{listing:?}"
    );
    assert!(
        matches!(
            opened(&set, &engine, path).await,
            Opening::Failed {
                code: "plugin_crashed",
                ..
            }
        ),
        "the next open runs the plugin again"
    );
}

/// `open` that counts for a while, asking `should-stop` on every turn: a document that takes
/// its time, as a large file's would.
fn slow() -> Testbed {
    let mut bed = Testbed::new();
    let offered = bed.data(&[testbed::output("text")]);
    let count = "(local.set $i (i32.const 0))\n\
                 (block $done (loop $turn\n\
                   (br_if $done (call $should_stop))\n\
                   (local.set $i (i32.add (local.get $i) (i32.const 1)))\n\
                   (br_if $done (i32.ge_u (local.get $i) (i32.const 200000)))\n\
                   (br $turn)))\n";
    let open = format!("{count}{}", testbed::ok_document(offered, None, None));
    bed.body(Export::Open, open)
}

/// *Quick View through twenty files*: each move raises the flag of the file before, as the
/// window does when its lister goes; at most one session is left, the others ended.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn quick_view_through_twenty_files_leaves_one_session() {
    let names: Vec<String> = (0..20).map(|index| format!("f{index:02}.csv")).collect();
    let entries: Vec<(&str, &str)> = names.iter().map(|name| (name.as_str(), "a,b\n")).collect();
    let (root, engine) = engine(&entries).await;
    let set = Arc::new(one(
        &Spec {
            limits: "timeout_milliseconds = 60000",
            ..Spec::default()
        },
        &slow(),
    ));
    let mut flags: Vec<Arc<AtomicBool>> = Vec::new();
    let mut running = Vec::new();
    for name in &names {
        if let Some(before) = flags.last() {
            before.store(true, Ordering::SeqCst);
        }
        let asked = request();
        flags.push(Arc::clone(&asked.cancel));
        let (set, engine) = (Arc::clone(&set), engine.clone());
        let path = VfsPath::local(root.path().join(name));
        running.push(tokio::spawn(async move {
            set.open(&engine, &path, judge, asked).await
        }));
    }
    let mut results = Vec::new();
    for task in running {
        results.push(task.await.expect("the open ran"));
    }
    let last = results.pop().expect("twenty");
    assert!(
        matches!(last, Opening::Document(_)),
        "the file shown: {last:?}"
    );
    for (index, result) in results.into_iter().enumerate() {
        match result {
            Opening::Cancelled => {}
            // Done before the cursor moved on: the window drops it with its lister.
            Opening::Document(document) => assert!(document.is_cancelled(), "file {index}"),
            other => panic!("file {index}: {other:?}"),
        }
    }
    assert_eq!(
        set.live_sessions(),
        1,
        "only the file shown keeps a session"
    );
    let failures = set
        .execution_log()
        .records("019d0000-0000-7000-8000-0000000000a1")
        .iter()
        .filter(|record| record.outcome.counts_as_failure())
        .count();
    assert_eq!(failures, 0, "a cancelled call is no failure of the plugin");
    drop(last);
    assert_eq!(set.live_sessions(), 0);
}

const ALPHA: &str = "019d0000-0000-7000-8000-0000000000a1";
const BETA: &str = "019d0000-0000-7000-8000-0000000000b2";

/// Alpha and Beta, both for `.csv`, running `alpha` and `beta`, their calls in `log`.
fn pair(alpha: &Testbed, beta: &Testbed, log: Arc<ExecutionLog>) -> ViewerPlugins {
    let first = plugin(
        &Spec {
            name: "Alpha",
            ..Spec::default()
        },
        alpha,
        &log,
    );
    let second = plugin(
        &Spec {
            id: "b2",
            name: "Beta",
            ..Spec::default()
        },
        beta,
        &log,
    );
    ViewerPlugins::from_plugins(vec![second, first], BTreeMap::new(), log)
}

/// A viewer whose `probe` says no.
fn declining() -> Testbed {
    Testbed::new().body(Export::Probe, "(i32.const 0)")
}

/// The exports `id` was called with, in order.
fn calls(set: &ViewerPlugins, id: &str) -> Vec<&'static str> {
    set.execution_log()
        .records(id)
        .iter()
        .map(|record| record.operation)
        .collect()
}

#[tokio::test]
async fn the_first_declines_and_the_second_opens() {
    let (root, engine) = engine(&[("data.csv", "a,b\n")]).await;
    let set = pair(&declining(), &Testbed::new(), Arc::new(ExecutionLog::new()));
    let path = VfsPath::local(root.path().join("data.csv"));
    let shown = document(opened(&set, &engine, path).await);
    assert_eq!(shown.plugin(), "Beta");
    assert_eq!(calls(&set, ALPHA), ["probe"], "Alpha was asked first");
    assert_eq!(calls(&set, BETA), ["probe", "open", "text"]);
    assert_eq!(set.live_sessions(), 1);
}

#[tokio::test]
async fn when_every_plugin_declines_the_built_in_viewer_shows_the_file() {
    let (root, engine) = engine(&[("data.csv", "a,b\n")]).await;
    let set = pair(&declining(), &declining(), Arc::new(ExecutionLog::new()));
    let path = VfsPath::local(root.path().join("data.csv"));
    assert!(matches!(
        opened(&set, &engine, path).await,
        Opening::BuiltIn
    ));
    assert_eq!(calls(&set, ALPHA), ["probe"]);
    assert_eq!(calls(&set, BETA), ["probe"]);
    assert_eq!(set.live_sessions(), 0);
}

/// A failure is no decline: its notice, and the plugin after it is not asked.
#[tokio::test]
async fn a_failing_probe_is_the_notice_and_the_next_plugin_is_not_asked() {
    let (root, engine) = engine(&[("data.csv", "a,b\n")]).await;
    let crashing = Testbed::new().body(Export::Probe, testbed::TRAP);
    let set = pair(&crashing, &Testbed::new(), Arc::new(ExecutionLog::new()));
    let path = VfsPath::local(root.path().join("data.csv"));
    match opened(&set, &engine, path).await {
        Opening::Failed { plugin, code } => {
            assert_eq!(plugin, "Alpha");
            assert_eq!(code, "plugin_crashed");
        }
        other => panic!("a failure, got {other:?}"),
    }
    assert!(calls(&set, BETA).is_empty(), "Beta was not asked");
}

/// Quick View moves on while the first `probe` runs: the flag, raised as its call ends, is seen
/// before the second `probe`.
#[tokio::test]
async fn the_flag_is_checked_between_two_probes() {
    let (root, engine) = engine(&[("data.csv", "a,b\n")]).await;
    let asked = request();
    let flag = Arc::clone(&asked.cancel);
    let log = Arc::new(ExecutionLog::new());
    let raised = log.set_sink(Arc::new(move |record: &ExecutionRecord| {
        if record.plugin_id == ALPHA && record.operation == "probe" {
            flag.store(true, Ordering::SeqCst);
        }
    }));
    assert!(raised, "the first sink");
    let set = pair(&declining(), &Testbed::new(), log);
    let path = VfsPath::local(root.path().join("data.csv"));
    assert!(matches!(
        set.open(&engine, &path, judge, asked).await,
        Opening::Cancelled
    ));
    assert_eq!(calls(&set, ALPHA), ["probe"]);
    assert!(calls(&set, BETA).is_empty(), "Beta was not asked");
    assert_eq!(set.live_sessions(), 0);
}
