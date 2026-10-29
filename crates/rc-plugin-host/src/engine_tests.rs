use super::SharedEngine;

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
