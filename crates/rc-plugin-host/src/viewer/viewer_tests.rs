//! The viewer kind against the test bed: a working viewer, a crashing one, a hanging one, one
//! that is cancelled, one that imports what it was not granted (RC-100-08, acceptance tests
//! *Crash*, *Hang* and *Undeclared import*).

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

use rc_plugin_api::viewer::{Output, Purpose, Theme};
use rc_plugin_types::Failure;

use super::{ViewerOptions, ViewerPlugin, ViewerSession};
use crate::{
    BytesSource, ExecutionOutcome, MemorySink, SourceReader,
    failure::codes,
    manifest::PluginManifest,
    test_fixtures::{viewer_manifest, viewer_manifest_with_limits},
    testbed::{self, Export, Testbed},
};

pub(super) fn source() -> Arc<dyn SourceReader> {
    Arc::new(BytesSource::new("data.csv", b"a,b\n1,2\n".to_vec()))
}

pub(super) fn theme() -> Theme {
    Theme {
        dark: false,
        background: "#ffffff".to_owned(),
        foreground: "#000000".to_owned(),
        accent: "#0078d4".to_owned(),
    }
}

pub(super) fn plugin(manifest: &PluginManifest, testbed: &Testbed) -> ViewerPlugin {
    plugin_with(manifest, testbed, ViewerOptions::default())
}

pub(super) fn plugin_with(
    manifest: &PluginManifest,
    testbed: &Testbed,
    options: ViewerOptions,
) -> ViewerPlugin {
    ViewerPlugin::new(manifest, &testbed.build(), options).expect("the test bed is a viewer")
}

pub(super) async fn open(plugin: &ViewerPlugin) -> Result<ViewerSession, Failure> {
    plugin
        .open(
            source(),
            Purpose::QuickView,
            &theme(),
            Arc::new(AtomicBool::new(false)),
        )
        .await
}

pub(super) fn code<T>(result: Result<T, Failure>) -> String {
    match result {
        Ok(_) => "ok".to_owned(),
        Err(failure) => failure.code.unwrap_or_default(),
    }
}

#[tokio::test]
async fn a_working_viewer_opens_its_document_and_writes_its_text() {
    let plugin = plugin(&viewer_manifest(&["text"]), &Testbed::new());
    assert!(plugin.probe(source()).await.expect("probe"));
    let mut session = open(&plugin).await.expect("open");
    assert_eq!(session.document().outputs, [Output::Text]);
    assert_eq!(session.document().title, None);

    let sink = MemorySink::new();
    let text = session.text(Box::new(sink.clone())).await.expect("text");
    assert_eq!(text.encoding, "utf-8");
    assert_eq!(text.bytes, 5);
    assert_eq!(sink.contents(), b"hello");
    // The session stays: a second call runs in the same store with a fresh budget.
    assert!(session.text(Box::new(MemorySink::new())).await.is_ok());
    assert_eq!(
        code(session.tokens(0, &["a,b"]).await),
        codes::PLUGIN_OUTPUT_NOT_OFFERED,
        "an output the document did not name runs nothing"
    );
    let records = plugin.execution_log().records(&plugin.identity().id);
    let operations: Vec<_> = records.iter().map(|record| record.operation).collect();
    assert_eq!(operations, ["probe", "open", "text", "text"]);
    assert!(
        records
            .iter()
            .all(|record| record.outcome == ExecutionOutcome::Ok)
    );
}

/// *Crash*: a trap in `open` is `plugin_crashed`, the next call runs; three failures suspend.
#[tokio::test]
async fn a_crash_fails_its_call_and_three_suspend_the_plugin() {
    let plugin = plugin(
        &viewer_manifest(&["text"]),
        &Testbed::new().body(Export::Open, testbed::TRAP),
    );
    assert_eq!(code(open(&plugin).await), codes::PLUGIN_CRASHED);
    assert!(
        plugin.probe(source()).await.expect("the next call runs"),
        "a crash fails the one call"
    );
    assert_eq!(code(open(&plugin).await), codes::PLUGIN_CRASHED);
    assert!(!plugin.health().is_suspended());
    assert_eq!(code(open(&plugin).await), codes::PLUGIN_CRASHED);
    assert!(plugin.health().is_suspended(), "the third failure suspends");
    assert_eq!(code(plugin.probe(source()).await), codes::PLUGIN_SUSPENDED);
    assert_eq!(code(open(&plugin).await), codes::PLUGIN_SUSPENDED);

    plugin.health().resume();
    assert!(plugin.probe(source()).await.expect("resumed"));
    let crashes = plugin
        .execution_log()
        .records(&plugin.identity().id)
        .iter()
        .filter(|record| record.outcome == ExecutionOutcome::Crash)
        .count();
    assert_eq!(crashes, 3, "a suspended call is no record");
}

/// A trap in a later call closes the session; the document is opened again.
#[tokio::test]
async fn a_trap_closes_the_session() {
    let mut bed = Testbed::new();
    let outputs = bed.data(&[testbed::output("highlighted"), testbed::output("text")]);
    let bed = bed
        .body(Export::Open, testbed::ok_document(outputs, None, None))
        .body(Export::Tokens, testbed::TRAP);
    let plugin = plugin(&viewer_manifest(&["text", "highlighted"]), &bed);
    let mut session = open(&plugin).await.expect("open");
    assert_eq!(code(session.tokens(0, &["x"]).await), codes::PLUGIN_CRASHED);
    assert!(session.is_closed());
    assert_eq!(
        code(session.text(Box::new(MemorySink::new())).await),
        codes::PLUGIN_SESSION_CLOSED
    );
    assert!(open(&plugin).await.is_ok(), "a new session runs");
}

/// *Hang*: an endless loop ends at the epoch deadline, and the caller waits no longer.
#[tokio::test]
async fn an_endless_loop_ends_at_the_deadline() {
    let manifest = viewer_manifest_with_limits(&["text"], "timeout_milliseconds = 200");
    let plugin = plugin(&manifest, &Testbed::new().body(Export::Open, testbed::SPIN));
    let started = Instant::now();
    assert_eq!(code(open(&plugin).await), codes::PLUGIN_TIMED_OUT);
    let waited = started.elapsed();
    assert!(waited >= Duration::from_millis(200), "{waited:?}");
    assert!(
        waited < Duration::from_secs(2),
        "the caller waited {waited:?}"
    );
}

/// *Hang*: a computing loop with a small budget ends at its fuel, long before its deadline.
#[tokio::test]
async fn a_computing_loop_ends_at_its_fuel() {
    let manifest =
        viewer_manifest_with_limits(&["text"], "fuel = 1000000\ntimeout_milliseconds = 60000");
    let plugin = plugin(&manifest, &Testbed::new().body(Export::Open, testbed::SPIN));
    let started = Instant::now();
    assert_eq!(code(open(&plugin).await), codes::PLUGIN_OUT_OF_FUEL);
    assert!(started.elapsed() < Duration::from_secs(10));
    let last = plugin.execution_log().records(&plugin.identity().id);
    assert_eq!(last[0].outcome, ExecutionOutcome::Fuel);
}

/// Quick View moving on: the flag ends the call - asked through `should-stop` or ended at the
/// next tick after the grace - as cancelled, which is no failure of the plugin.
#[tokio::test]
async fn a_cancelled_call_ends_and_does_not_count() {
    let manifest = viewer_manifest_with_limits(&["text"], "timeout_milliseconds = 60000");
    for body in [testbed::SPIN, testbed::SPIN_UNTIL_STOPPED] {
        let plugin = plugin(&manifest, &Testbed::new().body(Export::Open, body));
        // As often as a crash would need to suspend it.
        for _ in 0..3 {
            let cancel = Arc::new(AtomicBool::new(false));
            let raise = Arc::clone(&cancel);
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(50)).await;
                raise.store(true, Ordering::Relaxed);
            });
            let started = Instant::now();
            let opened = plugin
                .open(source(), Purpose::QuickView, &theme(), cancel)
                .await;
            assert_eq!(code(opened), codes::PLUGIN_CANCELLED, "{body}");
            assert!(started.elapsed() < Duration::from_secs(5), "{body}");
        }
        let records = plugin.execution_log().records(&plugin.identity().id);
        assert!(
            records
                .iter()
                .all(|record| record.outcome == ExecutionOutcome::Cancelled)
        );
        assert!(!plugin.health().is_suspended(), "{body}");
    }
}

/// *Undeclared import*: a component importing `http` without the grant is refused - by the
/// import check, and by the linker if it ever got past it.
#[test]
fn a_component_importing_http_is_refused() {
    let manifest = viewer_manifest(&["text"]);
    let refused = ViewerPlugin::new(
        &manifest,
        &testbed::importing_http(),
        ViewerOptions::default(),
    );
    let failure = refused.err().expect("refused");
    assert_eq!(failure.code(), Some(codes::PLUGIN_DENIED));
    assert_eq!(
        failure.params.get("import").map(String::as_str),
        Some("rcommander:plugin/http@0.1.0")
    );

    let sandbox = crate::runtime::SandboxEngine::new(manifest.limits, 5_000).expect("sandbox");
    let component =
        wasmtime::component::Component::from_binary(sandbox.engine(), &testbed::importing_http())
            .expect("it compiles");
    let mut linker = wasmtime::component::Linker::new(sandbox.engine());
    crate::link::link_viewer(&mut linker, "0.1.0", false).expect("the viewer's interfaces");
    assert!(
        linker.instantiate_pre(&component).is_err(),
        "nothing provides http"
    );
}
