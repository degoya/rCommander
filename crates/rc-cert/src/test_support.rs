//! Fixtures for the tests: the throw-away certificates and keys of `tests/fixtures/` (made by
//! OpenSSL and ssh-keygen for these tests only; `.gitleaks.toml` names the folder), and the clock
//! most tests read them at.

use crate::checks::Clock;
use crate::{Report, read};

/// 2026-06-01 00:00:00 UTC: inside the leaf's and the root's validity.
pub(crate) const MID_2026: i64 = 1_780_272_000;

/// The clock of most tests: mid-2026, warning 30 days ahead.
pub(crate) fn clock() -> Clock {
    Clock {
        now: MID_2026,
        warn_days: 30,
    }
}

/// The report of `bytes` mid-2026.
pub(crate) fn report(bytes: &[u8]) -> Report {
    read(bytes, clock()).expect("a report")
}

/// A fixture by name.
pub(crate) fn fixture(name: &str) -> &'static [u8] {
    FIXTURES
        .iter()
        .find(|(fixture, _)| *fixture == name)
        .map(|(_, bytes)| *bytes)
        .unwrap_or_else(|| panic!("no fixture {name}"))
}

macro_rules! fixtures {
    ($($name:literal),* $(,)?) => {
        &[$(($name, include_bytes!(concat!("../tests/fixtures/", $name)))),*]
    };
}

/// Every fixture, by its file name.
pub(crate) const FIXTURES: &[(&str, &[u8])] = fixtures![
    "authorized_keys",
    "chain-and-key.pem",
    "dsa.key",
    "ed25519-public.pem",
    "ed25519.key",
    "fullchain.pem",
    "id_ecdsa.pub",
    "id_ed25519",
    "id_ed25519.pub",
    "id_rsa_enc",
    "id_rsa_enc.pub",
    "key-mismatch.pem",
    "leaf-rsa-enc.key",
    "leaf-rsa-legacy-enc.key",
    "leaf-rsa-pkcs1.key",
    "leaf-rsa-public-pkcs1.pem",
    "leaf-rsa-public.pem",
    "leaf-rsa.key",
    "leaf.crt",
    "leaf.csr",
    "leaf.der",
    "leaf.p12",
    "root-ec-pkcs8.key",
    "root-ec.key",
    "root.crt",
    "weak.crt",
    "wrong-order.pem",
];

/// The fixtures that hold a private key.
pub(crate) const PRIVATE_FIXTURES: &[&str] = &[
    "chain-and-key.pem",
    "dsa.key",
    "ed25519.key",
    "id_ed25519",
    "id_rsa_enc",
    "key-mismatch.pem",
    "leaf-rsa-enc.key",
    "leaf-rsa-legacy-enc.key",
    "leaf-rsa-pkcs1.key",
    "leaf-rsa.key",
    "root-ec-pkcs8.key",
    "root-ec.key",
];
