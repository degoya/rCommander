//! The component: `src/lib.rs` translated into the vocabulary of `wit/rcommander.wit`. Built
//! only for `wasm32`; it holds no logic of its own, so it needs no test of its own.
#![allow(
    unsafe_code,
    reason = "the canonical-ABI glue wit-bindgen generates is the only unsafe code here"
)]

wit_bindgen::generate!({
    path: "wit",
    world: "secrets-plugin",
});

use exports::rcommander::plugin::secrets::{self as wit, Guest};
use rcommander::plugin::{http, types as wit_types};

use crate::types::{Failure, FailureKind, Http, HttpResponse, SecretField, SourceState};

struct Component;

/// The `http` interface: requests to the port the manifest grants, nothing else.
struct Bridge;

impl Http for Bridge {
    fn get(&mut self, url: &str) -> Result<HttpResponse, Failure> {
        http::request("GET", url, &[], &[])
            .map(|answer| HttpResponse {
                status: answer.status,
                body: answer.body,
            })
            .map_err(|_| Failure::of(FailureKind::Io))
    }
}

impl Guest for Component {
    fn status() -> wit::SourceState {
        match crate::status(&mut Bridge) {
            SourceState::Ready => wit::SourceState::Ready,
            SourceState::Locked => wit::SourceState::Locked,
            SourceState::Unreachable => wit::SourceState::Unreachable,
            SourceState::Unlicensed => wit::SourceState::Unlicensed,
        }
    }

    fn search(query: String, limit: u32) -> Result<Vec<wit::SecretEntry>, wit_types::Failure> {
        let found = crate::search(&mut Bridge, &query, limit).map_err(to_wit)?;
        Ok(found
            .into_iter()
            .map(|entry| wit::SecretEntry {
                id: entry.id,
                title: entry.title,
                user: entry.user,
                url: entry.url,
            })
            .collect())
    }

    fn get(id: String, field: wit::SecretField) -> Result<String, wit_types::Failure> {
        let field = match field {
            wit::SecretField::Password => SecretField::Password,
            wit::SecretField::PrivateKey => SecretField::PrivateKey,
            wit::SecretField::Passphrase => SecretField::Passphrase,
            wit::SecretField::Totp => SecretField::Totp,
        };
        crate::get(&mut Bridge, &id, field).map_err(to_wit)
    }
}

fn to_wit(failure: Failure) -> wit_types::Failure {
    wit_types::Failure {
        kind: match failure.kind {
            FailureKind::NotFound => wit_types::FailureKind::NotFound,
            FailureKind::BadData => wit_types::FailureKind::BadData,
            FailureKind::Io => wit_types::FailureKind::Io,
        },
        code: failure.code,
        params: Vec::new(),
    }
}

export!(Component);
