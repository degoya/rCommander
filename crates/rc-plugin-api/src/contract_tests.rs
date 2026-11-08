//! The package line of the contract names the version a manifest's `api_version` is checked
//! against. That the worlds are the design's, byte for byte, is `tests/design.rs`.

use super::{API_VERSION, PACKAGE, WIT};

#[test]
fn the_package_line_names_the_api_version() {
    assert!(WIT.contains(&format!("package {PACKAGE}@{API_VERSION};")));
    // The viewer, since 0.3.0 the secret source, since 0.4.0 the inventory and since 0.6.0 the
    // compare plugin (RC-100-41); the others come with their stages. 0.5.0 adds functions to the
    // secret source, no world (RC-100-38).
    assert_eq!(WIT.matches("\nworld ").count(), 4);
}
