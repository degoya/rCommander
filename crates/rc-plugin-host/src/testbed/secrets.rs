//! The test bed of the secrets world (RC-100-33): secret sources that answer, crash, hang, flood
//! or read their value over `http`, written as WebAssembly text like the viewer's bed.
//!
//! Layout as the viewer's (`testbed.rs`), plus the return area of `http.request` at [`HTTP_RET`]:
//! a discriminant, then an `http-response` (status at +4, headers at +8, body at +16) or a
//! `failure` (24 bytes from +4). A `secret-entry` is 40 bytes: id (ptr, len) at 0, title at 8,
//! user (tag, ptr, len) at 16, url at 28.
//!
//! From contract 0.5.0 a source exports `checkout` and `checkin` as well (RC-100-38): a
//! `result<lease, failure>` holds its payload at `RET + 8`, the lease's alignment - id (ptr, len)
//! at +0, value at +8, expires (tag at +16, the `u64` at +24).

use std::fmt::Write as _;

use super::{DATA, HEAP, REALLOC, RET, component_of, ok_pair, ok_unit};

/// The return area of `http.request`.
pub const HTTP_RET: u32 = 12352;
/// The value [`SecretsBed::new`]'s `get` answers, the canary the tests look for everywhere else.
pub const CANARY: &str = "canary-pw-7731";
/// The WIT enum index of `source-state`'s cases.
pub const READY: u8 = 0;
pub const LOCKED: u8 = 1;
/// The value [`SecretsBed::new`]'s `checkout` lends, and the id of its lease.
pub const LENT: &str = "canary-lent-5521";
pub const LEASE_ID: &str = "lease-1";
/// The WIT enum index of `failure-kind`'s `access-denied`.
const ACCESS_DENIED: u8 = 2;

/// The exports of `interface secrets`; `checkout` and `checkin` from contract 0.5.0 on.
#[derive(Clone, Copy, Debug)]
pub enum Export {
    Status,
    Search,
    Get,
    Checkout,
    Checkin,
}

impl Export {
    const ALL: [Self; 5] = [
        Self::Status,
        Self::Search,
        Self::Get,
        Self::Checkout,
        Self::Checkin,
    ];

    fn name(self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::Search => "search",
            Self::Get => "get",
            Self::Checkout => "checkout",
            Self::Checkin => "checkin",
        }
    }

    /// The flattened parameters of the canonical ABI: `checkout` takes the id (ptr, len), the
    /// field, and the reason (tag, ptr, len).
    fn params(self) -> &'static str {
        match self {
            Self::Status => "",
            Self::Search | Self::Get => "(param i32 i32 i32)",
            Self::Checkout => "(param i32 i32 i32 i32 i32 i32)",
            Self::Checkin => "(param i32 i32)",
        }
    }

    /// Whether a component of contract `version` exports it.
    fn in_contract(self, version: &str) -> bool {
        !matches!(self, Self::Checkout | Self::Checkin) || !matches!(version, "0.3.0" | "0.4.0")
    }
}

/// One secret source component, built from a body per export.
pub struct SecretsBed {
    data: Vec<(u32, Vec<u8>)>,
    next: u32,
    bodies: Vec<String>,
    /// The `rcommander:plugin` version the component is built against.
    version: &'static str,
}

impl Default for SecretsBed {
    fn default() -> Self {
        Self::new()
    }
}

impl SecretsBed {
    /// A source that works: `status` is ready, `search` finds nothing, `get` answers
    /// [`CANARY`], `checkout` lends [`LENT`] as lease [`LEASE_ID`] without an end, `checkin`
    /// takes any lease back.
    pub fn new() -> Self {
        let mut bed = Self {
            data: Vec::new(),
            next: DATA,
            bodies: Vec::new(),
            version: rc_plugin_api::API_VERSION,
        };
        let canary = bed.data(CANARY.as_bytes());
        let lease = bed.data(LEASE_ID.as_bytes());
        let lent = bed.data(LENT.as_bytes());
        bed.bodies = vec![
            format!("(i32.const {READY})"),
            ok_pair((0, 0)),
            ok_pair(canary),
            ok_lease(lease, lent, None),
            ok_unit(),
        ];
        bed
    }

    /// `checkout` lending [`LENT`] until `expires` (Unix seconds); a past time makes a lease
    /// that has ended by the next connect.
    #[must_use]
    pub fn lending_until(mut self, expires: u64) -> Self {
        let lease = self.data(LEASE_ID.as_bytes());
        let lent = self.data(LENT.as_bytes());
        self.body(Export::Checkout, ok_lease(lease, lent, Some(expires)))
    }

    /// `checkout` refusing with [`rc_plugin_api::secrets::REASON_REQUIRED`] unless a reason is
    /// handed over, then lending [`LENT`].
    #[must_use]
    pub fn wanting_a_reason(mut self) -> Self {
        let lease = self.data(LEASE_ID.as_bytes());
        let lent = self.data(LENT.as_bytes());
        let code = self.data(rc_plugin_api::secrets::REASON_REQUIRED.as_bytes());
        let body = format!(
            "(if (result i32) (i32.eqz (local.get 3))\n\
             (then {})\n(else {}))",
            lease_failure(ACCESS_DENIED, code),
            ok_lease(lease, lent, None)
        );
        self.body(Export::Checkout, body)
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

    /// Builds the component against contract `version`: `0.3.0`, the first with the secrets
    /// world (RC-100-33), or the current one (RC-100-39).
    pub fn contract(mut self, version: &'static str) -> Self {
        self.version = version;
        self
    }

    /// The core module as text; it imports `http`, `settings` and `log` only when a body calls
    /// them, as a guest does.
    pub fn core_text(&self) -> String {
        let version = self.version;
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
        for (export, body) in Export::ALL
            .iter()
            .zip(&self.bodies)
            .filter(|(export, _)| export.in_contract(version))
        {
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

    /// The component, built against the contract [`Self::contract`] names.
    pub fn build(&self) -> Vec<u8> {
        component_of(&self.core_text(), self.version, "secrets-plugin")
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

/// `ok(lease)`: the lease's id and value (ptr, len each) and its end, the payload at `RET + 8`.
pub fn ok_lease(id: (u32, u32), value: (u32, u32), expires: Option<u64>) -> String {
    let payload = RET + 8;
    let (tag, seconds) = expires.map_or((0, 0), |seconds| (1, seconds));
    format!(
        "(i32.store8 (i32.const {RET}) (i32.const 0))\n\
         (i32.store (i32.const {}) (i32.const {}))\n(i32.store (i32.const {}) (i32.const {}))\n\
         (i32.store (i32.const {}) (i32.const {}))\n(i32.store (i32.const {}) (i32.const {}))\n\
         (i32.store8 (i32.const {}) (i32.const {tag}))\n\
         (i64.store (i32.const {}) (i64.const {seconds}))\n\
         (i32.const {RET})",
        payload,
        id.0,
        payload + 4,
        id.1,
        payload + 8,
        value.0,
        payload + 12,
        value.1,
        payload + 16,
        payload + 24,
    )
}

/// `err(failure { kind, code: some(code), params: [] })` of `checkout`, the payload at
/// `RET + 8`; `kind` is the WIT enum's index.
pub fn lease_failure(kind: u8, (at, length): (u32, u32)) -> String {
    let payload = RET + 8;
    format!(
        "(i32.store8 (i32.const {RET}) (i32.const 1))\n\
         (i32.store8 (i32.const {payload}) (i32.const {kind}))\n\
         (i32.store8 (i32.const {}) (i32.const 1))\n\
         (i32.store (i32.const {}) (i32.const {at}))\n(i32.store (i32.const {}) (i32.const {length}))\n\
         (i32.store (i32.const {}) (i32.const 0))\n(i32.store (i32.const {}) (i32.const 0))\n\
         (i32.const {RET})",
        payload + 4,
        payload + 8,
        payload + 12,
        payload + 16,
        payload + 20
    )
}
