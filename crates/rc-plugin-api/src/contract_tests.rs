//! The package line of the contract names the version a manifest's `api_version` is checked
//! against. That the worlds are the design's, byte for byte, is `tests/design.rs`.

use super::{API_VERSION, PACKAGE, WIT};

#[test]
fn the_package_line_names_the_api_version() {
    assert!(WIT.contains(&format!("package {PACKAGE}@{API_VERSION};")));
    // The viewer and, since 0.3.0, the secret source; the others come with their stages.
    assert_eq!(WIT.matches("\nworld ").count(), 2);
}
