//! The test bed of the inventory world (RC-100-39): inventories that answer, crash, flood, send
//! their token marker over `http` or echo what the server sent, written as WebAssembly text like
//! the secret source's bed.
//!
//! Layout as the secret source's (`secrets.rs`): `http.request`'s return area at [`HTTP_RET`] (a
//! discriminant, then an `http-response`: status at +4, headers at +8, body at +16), and one
//! failure parameter built at [`PARAM`]. An `inventory-host` is 68 bytes: external-id (ptr, len)
//! at 0, name at 8, address at 16, port (u16) at 24, protocol (u8) at 26, user (tag, ptr, len) at
//! 28, folder at 40, tags at 48, note (tag, ptr, len) at 56.

use std::fmt::Write as _;

use super::{DATA, HEAP, REALLOC, RET, component_of, ok_pair};

/// The return area of `http.request`.
pub const HTTP_RET: u32 = 12352;
/// Where [`echo_answer`] builds its one failure parameter: (key ptr, len, value ptr, len).
const PARAM: u32 = 12400;
/// The size of one `inventory-host`.
pub const HOST_BYTES: u32 = 68;
/// The WIT enum index of `protocol`'s cases.
pub const SSH: u8 = 0;
pub const SFTP: u8 = 1;

/// One inventory component, built from the body of `list`.
pub struct InventoryBed {
    data: Vec<(u32, Vec<u8>)>,
    next: u32,
    body: String,
    version: &'static str,
}

impl Default for InventoryBed {
    fn default() -> Self {
        Self::new()
    }
}

impl InventoryBed {
    /// An inventory with no host.
    pub fn new() -> Self {
        Self {
            data: Vec::new(),
            next: DATA,
            body: ok_pair((0, 0)),
            version: rc_plugin_api::API_VERSION,
        }
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

    /// Places `text` and answers its pointer and length.
    pub fn text(&mut self, text: &str) -> (u32, u32) {
        self.data(text.as_bytes())
    }

    /// Replaces the body of `list`; it leaves the result's pointer.
    pub fn body(mut self, body: impl Into<String>) -> Self {
        self.body = body.into();
        self
    }

    /// Answers `hosts`: `ok(list)` of the records placed with [`host_bytes`].
    pub fn hosts(mut self, hosts: &[HostData]) -> Self {
        let count = u32::try_from(hosts.len()).expect("a few hosts");
        let bytes = host_bytes(hosts);
        let (at, _) = self.data(&bytes);
        self.body = ok_pair((at, count));
        self
    }

    /// Builds the component against contract `version` (the bed knows the current one only for
    /// this world; an older one shows the world is refused there).
    pub fn contract(mut self, version: &'static str) -> Self {
        self.version = version;
        self
    }

    /// The core module as text; it imports `http` and `log` only when the body calls them.
    pub fn core_text(&self) -> String {
        let version = self.version;
        let mut text = String::from("(module\n");
        if self.body.contains("$log") {
            let _ = writeln!(
                text,
                "(import \"rcommander:plugin/host@{version}\" \"log\" (func $log (param i32 i32 i32 i32)))"
            );
        }
        if self.body.contains("$request") {
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
        let _ = writeln!(
            text,
            "(func (export \"rcommander:plugin/inventory@{version}#list\") (result i32) \
             (local $i i32) (local $p i32)\n{})",
            self.body
        );
        text.push_str(")\n");
        text
    }

    /// The component.
    pub fn build(&self) -> Vec<u8> {
        component_of(&self.core_text(), self.version, "inventory-plugin")
    }
}

/// One host of [`host_bytes`], each text a (ptr, len) placed with [`InventoryBed::data`].
#[derive(Clone, Copy, Debug)]
pub struct HostData {
    pub external_id: (u32, u32),
    pub name: (u32, u32),
    pub address: (u32, u32),
    pub port: u16,
    pub protocol: u8,
    pub user: Option<(u32, u32)>,
    pub folder: (u32, u32),
    /// A `list<string>`: the pointer of [`super::list_bytes`] placed and the number of strings
    /// (not of bytes).
    pub tags: (u32, u32),
    pub note: Option<(u32, u32)>,
}

/// The bytes of a `list<inventory-host>`.
pub fn host_bytes(hosts: &[HostData]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(hosts.len() * HOST_BYTES as usize);
    let pair = |bytes: &mut Vec<u8>, (at, length): (u32, u32)| {
        bytes.extend_from_slice(&at.to_le_bytes());
        bytes.extend_from_slice(&length.to_le_bytes());
    };
    let optional = |bytes: &mut Vec<u8>, text: Option<(u32, u32)>| match text {
        Some(text) => {
            bytes.extend_from_slice(&[1, 0, 0, 0]);
            pair(bytes, text);
        }
        None => bytes.extend_from_slice(&[0; 12]),
    };
    for host in hosts {
        pair(&mut bytes, host.external_id);
        pair(&mut bytes, host.name);
        pair(&mut bytes, host.address);
        bytes.extend_from_slice(&host.port.to_le_bytes());
        bytes.extend_from_slice(&[host.protocol, 0]);
        optional(&mut bytes, host.user);
        pair(&mut bytes, host.folder);
        pair(&mut bytes, host.tags);
        optional(&mut bytes, host.note);
    }
    bytes
}

/// `list` answering `count` hosts, every field zero (empty ids, port 0), laid from 64 KiB on: a
/// list that long, cheaply.
pub fn many_hosts(count: u32) -> String {
    let pages = (u64::from(count) * u64::from(HOST_BYTES)).div_ceil(65536) + 1;
    format!(
        "(drop (memory.grow (i32.const {pages})))\n{}",
        ok_pair((65536, count))
    )
}

/// `http.request(method, url, headers, [])`; its answer left at [`HTTP_RET`].
pub fn request(method: (u32, u32), url: (u32, u32), headers: (u32, u32)) -> String {
    format!(
        "(call $request (i32.const {}) (i32.const {}) (i32.const {}) (i32.const {}) \
         (i32.const {}) (i32.const {}) (i32.const 0) (i32.const 0) (i32.const {HTTP_RET}))\n",
        method.0, method.1, url.0, url.1, headers.0, headers.1
    )
}

/// After [`request`]: the answer's body logged at `level` and handed back as the failure
/// parameter `key` of `err(failure { other, none, [(key, body)] })`; a refused request's own
/// failure handed back as it is.
pub fn echo_answer(level: (u32, u32), key: (u32, u32)) -> String {
    let payload = RET + 4;
    format!(
        "(if (i32.eqz (i32.load8_u (i32.const {HTTP_RET})))\n\
           (then\n\
             (call $log (i32.const {}) (i32.const {}) (i32.load (i32.const {})) \
              (i32.load (i32.const {})))\n\
             (i32.store (i32.const {PARAM}) (i32.const {}))\n\
             (i32.store (i32.const {}) (i32.const {}))\n\
             (i32.store (i32.const {}) (i32.load (i32.const {})))\n\
             (i32.store (i32.const {}) (i32.load (i32.const {})))\n\
             (i32.store8 (i32.const {RET}) (i32.const 1))\n\
             (i32.store8 (i32.const {payload}) (i32.const 8))\n\
             (i32.store8 (i32.const {}) (i32.const 0))\n\
             (i32.store (i32.const {}) (i32.const {PARAM}))\n\
             (i32.store (i32.const {}) (i32.const 1)))\n\
           (else\n\
             (i32.store8 (i32.const {RET}) (i32.const 1))\n\
             (memory.copy (i32.const {payload}) (i32.const {}) (i32.const 24))))\n\
         (i32.const {RET})",
        level.0,
        level.1,
        HTTP_RET + 16,
        HTTP_RET + 20,
        key.0,
        PARAM + 4,
        key.1,
        PARAM + 8,
        HTTP_RET + 16,
        PARAM + 12,
        HTTP_RET + 20,
        payload + 4,
        payload + 16,
        payload + 20,
        HTTP_RET + 4,
    )
}
