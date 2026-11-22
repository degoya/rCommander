use super::{SharedEngine, configure_compile_cache, configured, shared};

/// The smallest valid component: the preamble of the component encoding and nothing else.
pub(crate) const EMPTY_COMPONENT: [u8; 8] = [0x00, 0x61, 0x73, 0x6d, 0x0d, 0x00, 0x01, 0x00];

/// The same component with a custom section appended, so its bytes - and nothing about
/// what it does - differ.
pub(crate) fn variant(tag: u8) -> Vec<u8> {
    let mut bytes = EMPTY_COMPONENT.to_vec();
    // Section id 0 (custom), 3 bytes: a one-byte name "t", then the payload.
    bytes.extend_from_slice(&[0x00, 0x03, 0x01, b't', tag]);
    bytes
}

#[test]
fn a_component_is_compiled_once_per_content() {
    let shared = SharedEngine::new(None).expect("engine");
    shared.compile(&EMPTY_COMPONENT).expect("first compile");
    shared.compile(&EMPTY_COMPONENT).expect("second compile");
    assert_eq!(shared.compilations(), 1, "the same bytes compile once");

    shared.compile(&variant(1)).expect("changed bytes");
    assert_eq!(shared.compilations(), 2, "different bytes compile anew");
}

#[test]
fn a_component_that_does_not_compile_is_not_remembered() {
    let shared = SharedEngine::new(None).expect("engine");
    assert!(shared.compile(b"not a component").is_err());
    assert!(shared.compile(b"not a component").is_err());
    assert_eq!(shared.compilations(), 0);
}

/// RC-110-48: the cache configured at the start is the one every compile of the process uses -
/// the entry lands under its folder, the next start (an engine of its own on the same folder)
/// reads it back instead of compiling, and a second configuration is refused. The engine is the
/// process's: nextest runs this test in a process of its own.
#[test]
fn the_configured_cache_serves_the_next_start() {
    let root = tempfile::tempdir().expect("tempdir");
    configure_compile_cache(root.path()).expect("configured before the first compile");
    assert!(configured().is_some_and(|engine| engine.disk().is_some()));
    shared()
        .expect("the configured engine")
        .compile(&variant(9))
        .expect("compile");
    assert!(
        configure_compile_cache(root.path()).is_err(),
        "the engine exists: a later configuration is refused, not ignored"
    );

    let next = SharedEngine::new(Some(root.path())).expect("the next start");
    next.compile(&variant(9)).expect("read back");
    let cache = next.disk().expect("a disk cache").cache();
    assert_eq!(
        (cache.cache_hits(), cache.cache_misses()),
        (1, 0),
        "the next start must not compile"
    );
}
