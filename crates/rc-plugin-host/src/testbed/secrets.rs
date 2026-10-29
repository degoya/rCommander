//! The test bed of the secrets world (RC-100-33): secret sources that answer, crash, hang, flood
//! or read their value over `http`, written as WebAssembly text like the viewer's bed.
//!
//! Layout as the viewer's (`testbed.rs`), plus the return area of `http.request` at [`HTTP_RET`]:
//! a discriminant, then an `http-response` (status at +4, headers at +8, body at +16) or a
//! `failure` (24 bytes from +4). A `secret-entry` is 40 bytes: id (ptr, len) at 0, title at 8,
//! user (tag, ptr, len) at 16, url at 28.

use std::fmt::Write as _;

use super::{DATA, HEAP, REALLOC, RET, component_of, ok_pair};

/// The return area of `http.request`.
pub const HTTP_RET: u32 = 12352;
/// The value [`SecretsBed::new`]'s `get` answers, the canary the tests look for everywhere else.
pub const CANARY: &str = "canary-pw-7731";
/// The WIT enum index of `source-state`'s cases.
pub const READY: u8 = 0;
pub const LOCKED: u8 = 1;

/// The exports of `interface secrets`.
#[derive(Clone, Copy, Debug)]
pub enum Export {
    Status,
    Search,
    Get,
}

impl Export {
    const ALL: [Self; 3] = [Self::Status, Self::Search, Self::Get];

    fn name(self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::Search => "search",
            Self::Get => "get",
        }
    }

    /// The flattened parameters of the canonical ABI.
    fn params(self) -> &'static str {
        match self {
            Self::Status => "",
            Self::Search | Self::Get => "(param i32 i32 i32)",
        }
    }
}

/// One secret source component, built from a body per export.
pub struct SecretsBed {
    data: Vec<(u32, Vec<u8>)>,
    next: u32,
    bodies: Vec<String>,
}

impl Default for SecretsBed {
    fn default() -> Self {
        Self::new()
    }
}

impl SecretsBed {
    /// A source that works: `status` is ready, `search` finds nothing, `get` answers
    /// [`CANARY`].
    pub fn new() -> Self {
        let mut bed = Self {
            data: Vec::new(),
            next: DATA,
            bodies: Vec::new(),
        };
        let canary = bed.data(CANARY.as_bytes());
        bed.bodies = vec![
            format!("(i32.const {READY})"),
            ok_pair((0, 0)),
            ok_pair(canary),
        ];
        bed
    }

    /// Places `bytes` in a data segment; answers its pointer and length.
    pub fn data(&mut self, bytes: &[u8]) -> (u32, u32) {
        let at = self.next;
        let length = u32::try_from(bytes.len()).expect("small data");
        self.next = (at + length + 7) & !7;
        assert!(self.next <= HEAP, "test bed data overlaps the heap");
        self.data.push((at, bytes.to_vec()));
        (at, length)
    }

    /// Replaces the body of `export`; it leaves the result's pointer (or `status`'s case).
    pub fn body(mut self, export: Export, body: impl Into<String>) -> Self {
        self.bodies[export as usize] = body.into();
        self
    }

    /// The core module as text; it imports `http`, `settings` and `log` only when a body calls
    /// them, as a guest does.
    pub fn core_text(&self) -> String {
        let version = rc_plugin_api::API_VERSION;
        let mut text = String::from("(module\n");
        let uses = |name: &str| self.bodies.iter().any(|body| body.contains(name));
        if uses("$log") {
            let _ = writeln!(
                text,
                "(import \"rcommander:plugin/host@{version}\" \"log\" (func $log (param i32 i32 i32 i32)))"
            );
        }
        if uses("$request") {
            let _ = writeln!(
                text,
                "(import \"rcommander:plugin/http@{version}\" \"request\" \
                 (func $request (param i32 i32 i32 i32 i32 i32 i32 i32 i32)))"
            );
        }
        text.push_str("(memory (export \"memory\") 1)\n");
        let _ = writeln!(text, "(global $heap (mut i32) (i32.const {HEAP}))");
        for (at, bytes) in &self.data {
            let escaped: String = bytes.iter().map(|byte| format!("\\{byte:02x}")).collect();
            let _ = writeln!(text, "(data (i32.const {at}) \"{escaped}\")");
        }
        text.push_str(REALLOC);
        for (export, body) in Export::ALL.iter().zip(&self.bodies) {
            let _ = writeln!(
                text,
                "(func (export \"rcommander:plugin/secrets@{version}#{}\") {} (result i32) \
                 (local $i i32)\n{body})",
                export.name(),
                export.params()
            );
        }
        text.push_str(")\n");
        text
    }

    /// The component, built against the current contract.
    pub fn build(&self) -> Vec<u8> {
        component_of(
            &self.core_text(),
            rc_plugin_api::API_VERSION,
            "secrets-plugin",
        )
    }
}

/// The bytes of a `list<secret-entry>`: (id, title, user, url) per entry, each a (ptr, len) the
/// bed placed, `None` for an absent user or url.
pub fn entry_bytes(entries: &[EntryData]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(entries.len() * 40);
    let pair = |bytes: &mut Vec<u8>, (at, length): (u32, u32)| {
        bytes.extend_from_slice(&at.to_le_bytes());
        bytes.extend_from_slice(&length.to_le_bytes());
    };
    for entry in entries {
        pair(&mut bytes, entry.id);
        pair(&mut bytes, entry.title);
        for optional in [entry.user, entry.url] {
            match optional {
                Some(text) => {
                    bytes.extend_from_slice(&[1, 0, 0, 0]);
                    pair(&mut bytes, text);
                }
                None => bytes.extend_from_slice(&[0; 12]),
            }
        }
    }
    bytes
}

/// One entry of [`entry_bytes`], each text a (ptr, len) placed with [`SecretsBed::data`].
#[derive(Clone, Copy, Debug)]
pub struct EntryData {
    pub id: (u32, u32),
    pub title: (u32, u32),
    pub user: Option<(u32, u32)>,
    pub url: Option<(u32, u32)>,
}

/// `get` answering the body of `GET <url>` as the secret, or the host's failure of the request
/// as its own.
pub fn get_over_http(method: (u32, u32), url: (u32, u32)) -> String {
    format!(
        "(call $request (i32.const {}) (i32.const {}) (i32.const {}) (i32.const {}) \
         (i32.const 0) (i32.const 0) (i32.const 0) (i32.const 0) (i32.const {HTTP_RET}))\n\
         (if (i32.eqz (i32.load8_u (i32.const {HTTP_RET})))\n\
           (then\n\
             (i32.store8 (i32.const {RET}) (i32.const 0))\n\
             (i32.store (i32.const {}) (i32.load (i32.const {})))\n\
             (i32.store (i32.const {}) (i32.load (i32.const {}))))\n\
           (else\n\
             (i32.store8 (i32.const {RET}) (i32.const 1))\n\
             (memory.copy (i32.const {}) (i32.const {}) (i32.const 24))))\n\
         (i32.const {RET})",
        method.0,
        method.1,
        url.0,
        url.1,
        RET + 4,
        HTTP_RET + 16,
        RET + 8,
        HTTP_RET + 20,
        RET + 4,
        HTTP_RET + 4,
    )
}
