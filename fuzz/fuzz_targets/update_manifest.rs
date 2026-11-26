//! The update manifest as the update check reads it from a release (`rc_update::manifest`): the
//! signed envelope - payload, signatures, base64 - and, behind a signature that holds, the
//! manifest itself with its schema, channel, freshness and content rules.
//!
//! A fuzzer cannot forge a signature, so each input is read twice on either channel: as it is,
//! an envelope whose signatures a store of one throw-away key judges, and - when it is JSON - as
//! a payload that key signed, so the checks behind the signature see fuzzed manifests too. It is
//! an error or the manifest, never a panic; a manifest accepted is one of the channel asked,
//! still valid, within its bounds, and signed anew it is accepted again with the same content.

#![no_main]

use std::sync::OnceLock;

use jiff::Timestamp;
use libfuzzer_sys::fuzz_target;
use rc_update::{
    Channel, UpdateManifest,
    manifest::{MAX_ARTIFACTS, MAX_NOTES_CHARS, UPDATE_MANIFEST_DOMAIN, sign, verify_with},
    sign::{SigningKey, TrustStore, sign_document},
};
use serde_json::value::RawValue;

/// The key id the store trusts, as the release's own is named.
const KEY_ID: &str = "rcommander-update-v1";

/// The throw-away key every signed payload carries.
fn key() -> &'static SigningKey {
    static KEY: OnceLock<SigningKey> = OnceLock::new();
    KEY.get_or_init(|| SigningKey::from_bytes(&[7; 32]))
}

fn trust() -> &'static TrustStore {
    static TRUST: OnceLock<TrustStore> = OnceLock::new();
    TRUST.get_or_init(|| {
        let trust = TrustStore::new();
        trust
            .trust(KEY_ID.to_owned(), key().verifying_key())
            .expect("one key is trusted");
        trust
    })
}

/// The moment the check runs: 2026-10-01, a month after the seeds were issued.
fn now() -> Timestamp {
    Timestamp::from_second(1_790_812_800).expect("a timestamp in range")
}

/// `bytes` verified for `channel` as the update check verifies a fetched manifest.
fn verify(bytes: &[u8], channel: Channel) -> Option<UpdateManifest> {
    let manifest = verify_with(bytes, trust(), channel, None, now()).ok()?;
    assert_eq!(manifest.channel, channel, "a manifest of another channel");
    assert!(
        manifest.validate().is_ok(),
        "an accepted manifest is invalid"
    );
    assert!(now() <= manifest.not_after, "an expired manifest");
    assert!(manifest.artifacts.len() <= MAX_ARTIFACTS);
    assert!(manifest.notes.chars().count() <= MAX_NOTES_CHARS);
    Some(manifest)
}

/// An accepted manifest signed anew is accepted again, unchanged.
fn round_trip(manifest: &UpdateManifest) {
    let Ok(bytes) = sign(KEY_ID, key(), manifest) else {
        return;
    };
    let again = verify(&bytes, manifest.channel).expect("a re-signed manifest is accepted");
    assert_eq!(again.sequence, manifest.sequence);
    assert_eq!(again.version, manifest.version);
    assert_eq!(again.issued_at, manifest.issued_at);
    assert_eq!(again.not_after, manifest.not_after);
    assert_eq!(again.released_at, manifest.released_at);
    assert_eq!(again.notes, manifest.notes);
    assert_eq!(again.artifacts, manifest.artifacts);
    assert_eq!(again.schema_change, manifest.schema_change);
}

fuzz_target!(|data: &[u8]| {
    let signed = serde_json::from_slice::<Box<RawValue>>(data)
        .ok()
        .and_then(|payload| sign_document(UPDATE_MANIFEST_DOMAIN, KEY_ID, key(), &payload).ok())
        .and_then(|document| serde_json::to_vec(&document).ok());
    for channel in [Channel::Stable, Channel::Beta] {
        if let Some(manifest) = verify(data, channel) {
            round_trip(&manifest);
        }
        if let Some(manifest) = signed.as_deref().and_then(|bytes| verify(bytes, channel)) {
            round_trip(&manifest);
        }
    }
});
