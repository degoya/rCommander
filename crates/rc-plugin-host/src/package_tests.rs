use std::io::Cursor;

use super::*;
use crate::{
    test_fixtures::{PLUGIN_ID, zip_package},
    test_support::{signing_key, viewer_package},
};

#[test]
fn the_members_are_read_and_the_digest_covers_them() {
    let package = viewer_package(PLUGIN_ID, "1.0.0", "release", &signing_key(7), true);
    let (manifest, component, signature, locales) =
        read_archive(Cursor::new(&package)).expect("read");
    assert!(signature.is_some() && locales.is_empty());
    let digest = package_digest(&manifest, &component, &locales);
    let mut other = component.clone();
    other.push(0);
    assert_ne!(digest, package_digest(&manifest, &other, &locales));
}

#[test]
fn a_missing_or_oversize_member_refuses_the_package() {
    let no_component = zip_package(&[("manifest.toml", b"x")]);
    let error = read_archive(Cursor::new(&no_component)).expect_err("missing");
    assert!(error.to_string().contains("component.wasm"));
    let big = vec![b'a'; MAX_SIGNATURE_BYTES as usize + 1];
    let oversize = zip_package(&[
        ("manifest.toml", b"x"),
        ("component.wasm", b"y"),
        ("signature.ed25519", &big),
    ]);
    let error = read_archive(Cursor::new(&oversize)).expect_err("oversize");
    assert!(error.to_string().contains("size limit"));
    let path = zip_package(&[("../manifest.toml", b"x")]);
    assert!(read_archive(Cursor::new(&path)).is_err());
}
