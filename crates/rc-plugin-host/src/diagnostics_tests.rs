use rc_plugin_types::{Failure, FailureKind};

use super::{ExecutionLog, ExecutionOutcome, MAX_EXECUTION_RECORDS, PluginIdentity};
use crate::failure::codes;

fn identity(id: &str) -> PluginIdentity {
    PluginIdentity {
        id: id.to_owned(),
        name: "Example".to_owned(),
        version: "1.0.0".to_owned(),
        plugin_type: "viewer",
    }
}

/// "It crashes" and "it is not allowed to" call for different answers, and a plugin's own
/// refusal is no malfunction at all.
#[test]
fn outcomes_are_classified_by_the_host_code() {
    for (code, expected, counts) in [
        (codes::PLUGIN_CRASHED, ExecutionOutcome::Crash, true),
        (codes::PLUGIN_TIMED_OUT, ExecutionOutcome::Timeout, true),
        (codes::PLUGIN_OUT_OF_FUEL, ExecutionOutcome::Fuel, true),
        (codes::PLUGIN_OUT_OF_MEMORY, ExecutionOutcome::Memory, true),
        (codes::PLUGIN_DENIED, ExecutionOutcome::Denied, true),
        (codes::PLUGIN_CANCELLED, ExecutionOutcome::Cancelled, false),
        (codes::PLUGIN_BAD_OUTPUT, ExecutionOutcome::Refused, false),
        (
            codes::PLUGIN_SINK_FAILED,
            ExecutionOutcome::HostError,
            false,
        ),
        ("csv.bad_quote", ExecutionOutcome::Failed, false),
    ] {
        let result: Result<(), Failure> = Err(Failure::coded(FailureKind::Other, code));
        let outcome = ExecutionOutcome::of(&result);
        assert_eq!(outcome, expected, "{code}");
        assert_eq!(outcome.counts_as_failure(), counts, "{code}");
    }
    assert_eq!(
        ExecutionOutcome::of(&Ok::<(), Failure>(())),
        ExecutionOutcome::Ok
    );
}

#[test]
fn records_are_kept_per_plugin_and_bounded() {
    let log = ExecutionLog::new();
    let crash: Result<(), Failure> = Err(Failure::coded(FailureKind::Other, codes::PLUGIN_CRASHED));
    log.finish(&identity("a"), log.begin("open"), &crash);
    log.finish(&identity("b"), log.begin("text"), &Ok::<(), Failure>(()));
    let records = log.records("a");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].operation, "open");
    assert_eq!(records[0].outcome, ExecutionOutcome::Crash);
    assert_eq!(records[0].code.as_deref(), Some(codes::PLUGIN_CRASHED));

    for _ in 0..MAX_EXECUTION_RECORDS + 5 {
        log.finish(&identity("b"), log.begin("text"), &Ok::<(), Failure>(()));
    }
    assert!(log.records("a").is_empty(), "the oldest went first");
    assert_eq!(log.records("b").len(), MAX_EXECUTION_RECORDS);
}

/// RC-100-09: each record reaches the sink the engine writes into `plugin_executions`.
#[test]
fn every_record_reaches_the_sink_once_set() {
    let log = ExecutionLog::new();
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let into = std::sync::Arc::clone(&seen);
    assert!(log.set_sink(std::sync::Arc::new(move |record| {
        into.lock().expect("lock").push(record.operation);
    })));
    assert!(
        !log.set_sink(std::sync::Arc::new(|_| {})),
        "the first stays"
    );
    let identity = PluginIdentity::default();
    let ok: Result<(), rc_plugin_types::Failure> = Ok(());
    log.finish(&identity, log.begin("open"), &ok);
    assert_eq!(*seen.lock().expect("lock"), ["open"]);
}
