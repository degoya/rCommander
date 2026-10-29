use std::sync::{Arc, atomic::AtomicBool};
use std::time::{Duration, Instant};

use super::{MAX_TABLE_ELEMENTS, SandboxEngine, allowed_imports, settings_granted};
use crate::{
    PluginLimits,
    test_fixtures::{SETTINGS, viewer_manifest, viewer_manifest_0_2, viewer_manifest_text},
};

fn sandbox(limits: PluginLimits) -> SandboxEngine {
    SandboxEngine::new(limits, 5_000).expect("sandbox")
}

fn store(sandbox: &SandboxEngine) -> wasmtime::Store<super::PluginStoreState> {
    sandbox
        .create_store("plugin", "1.0.0", Arc::new(AtomicBool::new(false)))
        .expect("store")
}

/// A viewer reaches the base, its source and its sink - nothing a capability would add - under
/// its manifest's contract version.
#[test]
fn a_viewer_imports_its_streams_and_nothing_else() {
    let allowed = allowed_imports(&viewer_manifest(&["text"]));
    assert_eq!(
        allowed,
        [
            "rcommander:plugin/host@0.1.0",
            "rcommander:plugin/types@0.1.0",
            "rcommander:plugin/source@0.1.0",
            "rcommander:plugin/sink@0.1.0",
        ]
    );
    assert!(!settings_granted(&viewer_manifest(&["text"])));
}

/// `settings` only for a 0.2.0 manifest that declares `[[settings]]` (RC-100-14).
#[test]
fn settings_are_granted_by_declaring_them_under_contract_0_2() {
    let declared = viewer_manifest_0_2(&["text"], SETTINGS);
    assert!(settings_granted(&declared));
    assert_eq!(
        allowed_imports(&declared).last().map(String::as_str),
        Some("rcommander:plugin/settings@0.2.0")
    );
    let undeclared = viewer_manifest_0_2(&["text"], "");
    assert!(!settings_granted(&undeclared));
    assert!(
        !allowed_imports(&undeclared)
            .iter()
            .any(|import| import.contains("settings"))
    );
    // 0.1.0 has no `settings`, declared rows or not.
    let old = viewer_manifest_text(&["text"], SETTINGS);
    let old = crate::manifest::parse_manifest(old.as_bytes()).expect("a valid manifest");
    assert!(!settings_granted(&old));
}

/// One engine and one epoch ticker for the process, however many sandboxes there are.
#[test]
fn every_sandbox_shares_one_engine() {
    let first = sandbox(PluginLimits::default());
    let second = sandbox(PluginLimits {
        fuel: 1,
        ..PluginLimits::default()
    });
    assert!(wasmtime::Engine::same(first.engine(), second.engine()));
}

#[test]
fn limits_beyond_their_ceilings_build_no_sandbox() {
    for limits in [
        PluginLimits {
            fuel: 0,
            ..PluginLimits::default()
        },
        PluginLimits {
            memory_bytes: u64::MAX,
            ..PluginLimits::default()
        },
    ] {
        assert!(SandboxEngine::new(limits, 5_000).is_err(), "{limits:?}");
    }
    assert!(SandboxEngine::new(PluginLimits::default(), 0).is_err());
}

/// Every call of a long-lived store starts with the whole budget again.
#[test]
fn each_call_gets_its_own_fuel_and_deadline() {
    let sandbox = sandbox(PluginLimits {
        fuel: 1_000,
        ..PluginLimits::default()
    });
    let mut store = store(&sandbox);
    sandbox.begin_call(&mut store, "open").expect("begin");
    store.set_fuel(3).expect("spend");
    store.data_mut().credit_host_time(Duration::from_secs(60));
    sandbox.begin_call(&mut store, "text").expect("begin");
    assert_eq!(store.get_fuel().expect("fuel"), 1_000);
    let left = store
        .data()
        .execution_deadline
        .saturating_duration_since(Instant::now());
    assert!(left <= Duration::from_secs(5), "{left:?}");
    assert_eq!(store.data().log_budget.origin().operation, "text");
}

#[test]
fn host_time_is_credited_to_the_deadline() {
    let sandbox = sandbox(PluginLimits::default());
    let mut store = store(&sandbox);
    let before = store.data().execution_deadline;
    store.data_mut().credit_host_time(Duration::from_secs(12));
    assert_eq!(
        store.data().execution_deadline - before,
        Duration::from_secs(12)
    );
}

/// A core module with a one-element funcref table and `grow(n)`, which runs
/// `table.grow 0 (ref.null func) n` and returns the old size (-1 on a refused grow).
const TABLE_GROWER: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, // magic, version
    0x01, 0x06, 0x01, 0x60, 0x01, 0x7f, 0x01, 0x7f, // type: (i32) -> i32
    0x03, 0x02, 0x01, 0x00, // function 0 has type 0
    0x04, 0x04, 0x01, 0x70, 0x00, 0x01, // table 0: funcref, min 1, no max
    0x07, 0x08, 0x01, 0x04, b'g', b'r', b'o', b'w', 0x00, 0x00, // export "grow"
    0x0a, 0x0b, 0x01, 0x09, 0x00, // code: one body, no locals
    0xd0, 0x70, 0x20, 0x00, 0xfc, 0x0f, 0x00,
    0x0b, // ref.null func, local.get 0, table.grow 0
];

/// rDownloader's PL-01: up to the cap a grow succeeds; one element past it traps instead of
/// taking host memory.
#[tokio::test]
async fn a_table_cannot_grow_past_its_element_cap() {
    let sandbox = sandbox(PluginLimits::default());
    let module = wasmtime::Module::new(sandbox.engine(), TABLE_GROWER).expect("module");
    let mut store = store(&sandbox);
    sandbox.begin_call(&mut store, "grow").expect("begin");
    let instance = wasmtime::Instance::new_async(&mut store, &module, &[])
        .await
        .expect("instance");
    let grow = instance
        .get_typed_func::<i32, i32>(&mut store, "grow")
        .expect("grow export");
    let to_cap = i32::try_from(MAX_TABLE_ELEMENTS - 1).expect("cap fits i32");
    assert_eq!(
        grow.call_async(&mut store, to_cap)
            .await
            .expect("grow to the cap"),
        1
    );
    assert!(
        grow.call_async(&mut store, 1).await.is_err(),
        "growing past the cap must trap"
    );
}
