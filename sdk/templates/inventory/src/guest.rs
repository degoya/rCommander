//! The component: `src/lib.rs` translated into the vocabulary of `wit/rcommander.wit`. Built
//! only for `wasm32`; it holds no logic of its own, so it needs no test of its own.
#![allow(
    unsafe_code,
    reason = "the canonical-ABI glue wit-bindgen generates is the only unsafe code here"
)]

wit_bindgen::generate!({
    path: "wit",
    world: "inventory-plugin",
});

use exports::rcommander::plugin::inventory::{self as wit, Guest};
use rcommander::plugin::{http, types as wit_types};

use crate::types::{Failure, FailureKind, Http, HttpResponse, Protocol};

struct Component;

/// The `http` interface: requests to the address the person entered, nothing else.
struct Server;

impl Http for Server {
    fn get(&mut self, path: &str, headers: &[(String, String)]) -> Result<HttpResponse, Failure> {
        http::request("GET", path, headers, &[])
            .map(|answer| HttpResponse {
                status: answer.status,
                body: answer.body,
            })
            .map_err(|_| Failure::of(FailureKind::Io))
    }
}

impl Guest for Component {
    fn list() -> Result<Vec<wit::InventoryHost>, wit_types::Failure> {
        let hosts = crate::list(&mut Server).map_err(to_wit)?;
        Ok(hosts
            .into_iter()
            .map(|host| wit::InventoryHost {
                external_id: host.external_id,
                name: host.name,
                address: host.address,
                port: host.port,
                protocol: match host.protocol {
                    Protocol::Ssh => wit::Protocol::Ssh,
                    Protocol::Sftp => wit::Protocol::Sftp,
                    Protocol::Ftp => wit::Protocol::Ftp,
                    Protocol::Ftps => wit::Protocol::Ftps,
                },
                user: host.user,
                folder: host.folder,
                tags: host.tags,
                note: host.note,
            })
            .collect())
    }
}

fn to_wit(failure: Failure) -> wit_types::Failure {
    wit_types::Failure {
        kind: match failure.kind {
            FailureKind::AccessDenied => wit_types::FailureKind::AccessDenied,
            FailureKind::BadData => wit_types::FailureKind::BadData,
            FailureKind::Io => wit_types::FailureKind::Io,
        },
        code: failure.code,
        params: Vec::new(),
    }
}

export!(Component);
