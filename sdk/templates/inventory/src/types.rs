//! The inventory world's shapes this plugin uses, mirrored from `wit/rcommander.wit`, so the
//! logic in `lib.rs` is plain Rust that `cargo test` runs without a WebAssembly toolchain.
//! `src/guest.rs` maps each to its WIT twin and nothing more.

/// `protocol`: how rCommander connects to a host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Protocol {
    Ssh,
    Sftp,
    Ftp,
    Ftps,
}

/// `inventory-host`: one host of the inventory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InventoryHost {
    /// The inventory's own id of the host: what rCommander matches its connection by, so it has
    /// to stay the same when the host is renamed.
    pub external_id: String,
    pub name: String,
    /// A host name or an IP address, nothing else.
    pub address: String,
    pub port: u16,
    pub protocol: Protocol,
    pub user: Option<String>,
    /// The folder below the source's folder, `/`-separated; empty for the source's folder.
    pub folder: String,
    pub tags: Vec<String>,
    pub note: Option<String>,
}

/// `failure-kind`, the cases this inventory answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureKind {
    AccessDenied,
    BadData,
    Io,
}

/// `failure`: a kind and an optional code from `locales/en.json`, which starts with the plugin's
/// id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub kind: FailureKind,
    pub code: Option<String>,
}

impl Failure {
    /// A failure of `kind` without a code of its own.
    #[must_use]
    pub fn of(kind: FailureKind) -> Self {
        Self { kind, code: None }
    }
}

/// `http-response`, the parts this inventory reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

/// The `http` interface: one `GET` to the address the person entered. A relative `path` is
/// resolved against it; a redirect is answered, never followed; a refused or failed request is
/// an `Err`.
pub trait Http {
    fn get(&mut self, path: &str, headers: &[(String, String)]) -> Result<HttpResponse, Failure>;
}
