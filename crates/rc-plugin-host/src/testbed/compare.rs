//! The test bed of the compare world (RC-100-41): compare plugins that answer, say they are
//! unlicensed, crash or flood the host with changes, written as WebAssembly text like the
//! inventory's bed.
//!
//! Layout as the viewer's (`testbed.rs`). A `result<comparison, failure>` holds its payload at
//! `RET + 4`: the `compare-summary` (added, removed, changed, moved as `u32` at +0..+16,
//! truncated at +16), the changes (ptr, len) at +20, the notices (ptr, len) at +28. A `change`
//! is 64 bytes: kind (u8) at 0, path (ptr, len) at 4, column (tag, ptr, len) at 12, old-value
//! at 24, new-value at 36, old-line (tag, value) at 48, new-line at 56.

use std::fmt::Write as _;

use super::{DATA, HEAP, REALLOC, RET, component_of};

/// The size of one `change`.
pub const CHANGE_BYTES: u32 = 64;
/// The WIT enum index of `compare-state`'s cases.
pub const READY: u8 = 0;
pub const UNLICENSED: u8 = 1;
/// The WIT enum index of `change-kind`'s `changed`.
pub const CHANGED: u8 = 2;

/// The exports of `interface compare`.
#[derive(Clone, Copy, Debug)]
pub enum Export {
    Status,
    Compare,
}

/// One compare plugin component, built from a body per export.
pub struct CompareBed {
    data: Vec<(u32, Vec<u8>)>,
    next: u32,
    status: String,
    compare: String,
    version: &'static str,
}

impl Default for CompareBed {
    fn default() -> Self {
        Self::new()
    }
}

impl CompareBed {
    /// A plugin that works and finds no change: `status` is ready, `compare` answers an empty
    /// comparison.
    pub fn new() -> Self {
        Self {
            data: Vec::new(),
            next: DATA,
            status: format!("(i32.const {READY})"),
            compare: ok_comparison(0, (0, 0)),
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

    /// Replaces the body of `export`; it leaves the result's pointer (or `status`'s case).
    pub fn body(mut self, export: Export, body: impl Into<String>) -> Self {
        match export {
            Export::Status => self.status = body.into(),
            Export::Compare => self.compare = body.into(),
        }
        self
    }

    /// `status` answering `unlicensed`, as a paid plugin without a valid key does.
    #[must_use]
    pub fn unlicensed(self) -> Self {
        self.body(Export::Status, format!("(i32.const {UNLICENSED})"))
    }

    /// `compare` answering `count` changes of kind `changed`, each at the path `path`, and a
    /// summary counting exactly them.
    #[must_use]
    pub fn changes(mut self, count: u32, path: &str) -> Self {
        let path = self.data(path.as_bytes());
        self.compare = many_changes(count, path);
        self
    }

    /// Builds the component against contract `version`.
    pub fn contract(mut self, version: &'static str) -> Self {
        self.version = version;
        self
    }

    /// The core module as text; it imports `log` only when a body calls it.
    pub fn core_text(&self) -> String {
        let version = self.version;
        let mut text = String::from("(module\n");
        if self.status.contains("$log") || self.compare.contains("$log") {
            let _ = writeln!(
                text,
                "(import \"rcommander:plugin/host@{version}\" \"log\" (func $log (param i32 i32 i32 i32)))"
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
            "(func (export \"rcommander:plugin/compare@{version}#status\") (result i32)\n{})",
            self.status
        );
        let _ = writeln!(
            text,
            "(func (export \"rcommander:plugin/compare@{version}#compare\") (param i32 i32) \
             (result i32) (local $i i32) (local $p i32)\n{})",
            self.compare
        );
        text.push_str(")\n");
        text
    }

    /// The component.
    pub fn build(&self) -> Vec<u8> {
        component_of(&self.core_text(), self.version, "compare-plugin")
    }
}

/// `ok(comparison)` counting `changed` changes of kind `changed`, the list (ptr, len) `changes`,
/// no notice.
pub fn ok_comparison(changed: u32, (at, length): (u32, u32)) -> String {
    let payload = RET + 4;
    format!(
        "(i32.store8 (i32.const {RET}) (i32.const 0))\n\
         (i32.store (i32.const {}) (i32.const 0))\n\
         (i32.store (i32.const {}) (i32.const 0))\n\
         (i32.store (i32.const {}) (i32.const {changed}))\n\
         (i32.store (i32.const {}) (i32.const 0))\n\
         (i32.store8 (i32.const {}) (i32.const 0))\n\
         (i32.store (i32.const {}) (i32.const {at}))\n(i32.store (i32.const {}) (i32.const {length}))\n\
         (i32.store (i32.const {}) (i32.const 0))\n(i32.store (i32.const {}) (i32.const 0))\n\
         (i32.const {RET})",
        payload,
        payload + 4,
        payload + 8,
        payload + 12,
        payload + 16,
        payload + 20,
        payload + 24,
        payload + 28,
        payload + 32
    )
}

/// `compare` answering `count` changes of kind `changed` at `path`, laid from 64 KiB on in fresh
/// (zeroed) memory, so every optional field is `none`.
pub fn many_changes(count: u32, path: (u32, u32)) -> String {
    let pages = (u64::from(count) * u64::from(CHANGE_BYTES)).div_ceil(65536) + 1;
    format!(
        "(drop (memory.grow (i32.const {pages})))\n\
         (local.set $p (i32.const 65536))\n\
         (local.set $i (i32.const 0))\n\
         (block $done (loop $next\n\
           (br_if $done (i32.ge_u (local.get $i) (i32.const {count})))\n\
           (i32.store8 (local.get $p) (i32.const {CHANGED}))\n\
           (i32.store (i32.add (local.get $p) (i32.const 4)) (i32.const {}))\n\
           (i32.store (i32.add (local.get $p) (i32.const 8)) (i32.const {}))\n\
           (local.set $p (i32.add (local.get $p) (i32.const {CHANGE_BYTES})))\n\
           (local.set $i (i32.add (local.get $i) (i32.const 1)))\n\
           (br $next)))\n{}",
        path.0,
        path.1,
        ok_comparison(count, (65536, count))
    )
}
