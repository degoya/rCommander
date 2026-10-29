//! The test bed: small viewer components that crash, hang, flood the log or answer hostile
//! output, written as WebAssembly text and made into components the way
//! `wasm-tools component new` does (`wit-component` over the contract in `rc_plugin_api::WIT`).
//!
//! rDownloader's host tests run components built from plugin crates
//! (`crates/rd-plugin-host/src/artifact.rs` at commit 26c8592a6), which needs a WebAssembly
//! toolchain and a build step before the tests. A test bed this small is clearer as text: each
//! export is one body, the canonical ABI layout is spelled out below, and `cargo test` builds
//! everything it runs.
//!
//! Layout of the core module's memory: data at [`DATA`] and up, the return area of every export
//! at [`RET`], a scratch area for `write-at`'s result at [`SCRATCH`], the source handle `open`
//! keeps at [`SOURCE_SLOT`] and `read-at`'s result behind it, the bump allocator's heap from
//! 32 KiB. Results (`result<T, failure>`) are a discriminant byte at `RET` and the payload at
//! `RET + 4`; a `document` is `outputs` (ptr, len) at +0, `title` (tag, ptr, len) at +8, `pages`
//! (tag, value) at +20 of the payload.
//!
//! A test bed is built against contract 0.1.0 unless [`Testbed::contract`] names another: the
//! frozen `testbed/rcommander-0.1.0.wit` is what a plugin built before RC-100-14 was built
//! against, so every test on the default bed shows that such a plugin still runs. A body calling
//! `$setting` ([`echo_setting`]) makes the core module import `settings`, which 0.2.0 has.

use std::fmt::Write as _;

use crate::MAX_WINDOW_BYTES;

/// The secrets world's bed (RC-100-33).
pub mod secrets;

/// Where every export writes its result.
pub const RET: u32 = 4096;
/// Where `write-at` writes its own result, which the test bed ignores.
pub const SCRATCH: u32 = 8192;
/// Where `open` keeps its source handle for the calls after it ([`remember_source`]).
pub const SOURCE_SLOT: u32 = 12288;
/// The return area of `source.read-at`: a discriminant, then the list's pointer and length.
const READ_RET: u32 = 12296;
/// The first byte of the data segments.
pub const DATA: u32 = 16384;
/// The first byte the allocator hands out.
const HEAP: u32 = 32768;
/// The return area of `settings.setting`: a tag, then the string's pointer and length.
const SETTING_RET: u32 = 12320;

/// The contract every plugin was built against before RC-100-14, exactly as it was.
const WIT_0_1_0: &str = include_str!("testbed/rcommander-0.1.0.wit");
/// The contract of RC-100-14, before `http` and the secrets world (RC-100-33), exactly as it was.
const WIT_0_2_0: &str = include_str!("testbed/rcommander-0.2.0.wit");

/// The exports of `interface viewer`, with their core signatures.
#[derive(Clone, Copy, Debug)]
pub enum Export {
    Probe,
    Open,
    Text,
    Tokens,
    Image,
    Page,
    Children,
    Search,
    Thumbnail,
}

impl Export {
    const ALL: [Self; 9] = [
        Self::Probe,
        Self::Open,
        Self::Text,
        Self::Tokens,
        Self::Image,
        Self::Page,
        Self::Children,
        Self::Search,
        Self::Thumbnail,
    ];

    fn name(self) -> &'static str {
        match self {
            Self::Probe => "probe",
            Self::Open => "open",
            Self::Text => "text",
            Self::Tokens => "tokens",
            Self::Image => "image",
            Self::Page => "page",
            Self::Children => "children",
            Self::Search => "search",
            Self::Thumbnail => "thumbnail",
        }
    }

    /// The flattened parameters of the canonical ABI.
    fn params(self) -> &'static str {
        match self {
            Self::Probe | Self::Text | Self::Image => "(param i32)",
            Self::Open => "(param i32 i32 i32 i32 i32 i32 i32 i32 i32)",
            Self::Tokens => "(param i32 i32)",
            Self::Page | Self::Search => "(param i32 i32 i32 i32)",
            Self::Children => "(param i32 i64)",
            Self::Thumbnail => "(param i32 i32 i32)",
        }
    }
}

/// The index of an output in the WIT enum.
pub fn output(name: &str) -> u8 {
    ["text", "highlighted", "markdown", "image", "tree", "pages"]
        .iter()
        .position(|known| *known == name)
        .and_then(|index| u8::try_from(index).ok())
        .expect("an output of the contract")
}

/// One viewer component, built from a body per export.
pub struct Testbed {
    data: Vec<(u32, Vec<u8>)>,
    next: u32,
    bodies: Vec<String>,
    /// The `rcommander:plugin` version the component is built against.
    version: &'static str,
}

impl Default for Testbed {
    fn default() -> Self {
        Self::new()
    }
}

impl Testbed {
    /// A viewer that works: `open` offers `text`, `text` writes `hello` and names `utf-8`,
    /// `probe` says yes, every other output answers an empty success.
    pub fn new() -> Self {
        let mut testbed = Self {
            data: Vec::new(),
            next: DATA,
            bodies: Vec::new(),
            version: "0.1.0",
        };
        let outputs = testbed.data(&[output("text")]);
        let hello = testbed.data(b"hello");
        let utf8 = testbed.data(b"utf-8");
        let png = testbed.data(b"png");
        testbed.bodies = Export::ALL
            .iter()
            .map(|export| match export {
                Export::Probe => "(i32.const 1)".to_owned(),
                Export::Open => ok_document(outputs, None, None),
                Export::Text => format!("{} {}", write(0, 0, hello), ok_pair(utf8)),
                Export::Tokens | Export::Children => ok_pair((0, 0)),
                Export::Image | Export::Thumbnail => ok_pair(png),
                Export::Page => ok_unit(),
                Export::Search => ok_none(),
            })
            .collect();
        testbed
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

    /// Replaces the body of `export`; it leaves the result's pointer (or `probe`'s answer).
    pub fn body(mut self, export: Export, body: impl Into<String>) -> Self {
        self.bodies[export as usize] = body.into();
        self
    }

    /// Builds the component against contract `version`: `0.1.0`, `0.2.0` or the current one.
    pub fn contract(mut self, version: &'static str) -> Self {
        self.version = version;
        self
    }

    /// The core module as text.
    pub fn core_text(&self) -> String {
        let version = self.version;
        let mut text = format!(
            "(module\n\
             (import \"rcommander:plugin/host@{version}\" \"log\" (func $log (param i32 i32 i32 i32)))\n\
             (import \"rcommander:plugin/sink@{version}\" \"write-at\" (func $write_at (param i32 i64 i32 i32 i32)))\n\
             (import \"rcommander:plugin/source@{version}\" \"read-at\" (func $read_at (param i32 i64 i32 i32)))\n\
             (import \"rcommander:plugin/source@{version}\" \"should-stop\" (func $should_stop (result i32)))\n"
        );
        // Only a body that reads a setting imports `settings`, as only a guest that calls it does.
        if self.bodies.iter().any(|body| body.contains("$setting")) {
            let _ = writeln!(
                text,
                "(import \"rcommander:plugin/settings@{version}\" \"setting\" (func $setting (param i32 i32 i32)))"
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
                "(func (export \"rcommander:plugin/viewer@{version}#{}\") {} (result i32) \
                 (local $i i32) (local $p i32)\n{body})",
                export.name(),
                export.params()
            );
        }
        text.push_str(")\n");
        text
    }

    /// The component.
    pub fn build(&self) -> Vec<u8> {
        component(&self.core_text(), self.version)
    }
}

/// A bump allocator: `cabi_realloc(old, old_size, align, new_size)` for what the host passes in.
const REALLOC: &str = "(func (export \"cabi_realloc\") (param i32 i32 i32 i32) (result i32)
  (local $ptr i32)
  (local.set $ptr
    (i32.and
      (i32.add (global.get $heap) (i32.sub (local.get 2) (i32.const 1)))
      (i32.sub (i32.const 0) (local.get 2))))
  (global.set $heap (i32.add (local.get $ptr) (local.get 3)))
  (block $fits
    (br_if $fits (i32.le_u (global.get $heap) (i32.mul (memory.size) (i32.const 65536))))
    (drop (memory.grow
      (i32.add
        (i32.div_u (i32.sub (global.get $heap) (i32.mul (memory.size) (i32.const 65536)))
          (i32.const 65536))
        (i32.const 1)))))
  (local.get $ptr))
";

/// Makes a component of a core module implementing the viewer world of contract `version`.
pub fn component(core_text: &str, version: &str) -> Vec<u8> {
    component_of(core_text, version, "viewer-plugin")
}

/// Makes a component of a core module implementing `world` of contract `version`.
fn component_of(core_text: &str, version: &str, world: &str) -> Vec<u8> {
    let contract = match version {
        "0.1.0" => WIT_0_1_0,
        "0.2.0" => WIT_0_2_0,
        current if current == rc_plugin_api::API_VERSION => rc_plugin_api::WIT,
        other => panic!("the test bed knows no contract {other}"),
    };
    let mut module = wat::parse_str(core_text).expect("the core module parses");
    let mut resolve = wit_parser::Resolve::default();
    let package = resolve
        .push_str("rcommander.wit", contract)
        .expect("the contract parses");
    let world = resolve.packages[package].worlds[world];
    wit_component::embed_component_metadata(
        &mut module,
        &resolve,
        world,
        wit_component::StringEncoding::UTF8,
    )
    .expect("the contract embeds");
    let mut encoder = wit_component::ComponentEncoder::default();
    encoder
        .module(&module)
        .expect("the core module fits the world");
    encoder.validate(true);
    encoder.encode().expect("a valid component")
}

/// A component that imports `http`, which no viewer is granted.
pub fn importing_http() -> Vec<u8> {
    wat::parse_str(
        "(component (import \"rcommander:plugin/http@0.1.0\" (instance (export \"get\" (func)))))",
    )
    .expect("the component parses")
}

/// `ok(document)` with `outputs` (ptr, len of enum bytes), a title and a page count.
pub fn ok_document(outputs: (u32, u32), title: Option<(u32, u32)>, pages: Option<u32>) -> String {
    let payload = RET + 4;
    let mut body = format!(
        "(i32.store8 (i32.const {RET}) (i32.const 0))\n\
         (i32.store (i32.const {}) (i32.const {}))\n(i32.store (i32.const {}) (i32.const {}))\n",
        payload,
        outputs.0,
        payload + 4,
        outputs.1
    );
    match title {
        Some((at, length)) => {
            let _ = writeln!(
                body,
                "(i32.store8 (i32.const {}) (i32.const 1))\n\
                 (i32.store (i32.const {}) (i32.const {at}))\n(i32.store (i32.const {}) (i32.const {length}))",
                payload + 8,
                payload + 12,
                payload + 16
            );
        }
        None => {
            let _ = writeln!(
                body,
                "(i32.store8 (i32.const {}) (i32.const 0))",
                payload + 8
            );
        }
    }
    match pages {
        Some(count) => {
            let _ = writeln!(
                body,
                "(i32.store8 (i32.const {}) (i32.const 1))\n(i32.store (i32.const {}) (i32.const {count}))",
                payload + 20,
                payload + 24
            );
        }
        None => {
            let _ = writeln!(
                body,
                "(i32.store8 (i32.const {}) (i32.const 0))",
                payload + 20
            );
        }
    }
    let _ = write!(body, "(i32.const {RET})");
    body
}

/// `ok(x)` where `x` is a string or a list: (ptr, len).
pub fn ok_pair((at, length): (u32, u32)) -> String {
    format!(
        "(i32.store8 (i32.const {RET}) (i32.const 0))\n\
         (i32.store (i32.const {}) (i32.const {at}))\n(i32.store (i32.const {}) (i32.const {length}))\n\
         (i32.const {RET})",
        RET + 4,
        RET + 8
    )
}

/// `ok(_)`.
pub fn ok_unit() -> String {
    format!("(i32.store8 (i32.const {RET}) (i32.const 0)) (i32.const {RET})")
}

/// `ok(none)` of `search`.
pub fn ok_none() -> String {
    format!(
        "(i32.store8 (i32.const {RET}) (i32.const 0)) (i32.store8 (i32.const {}) (i32.const 0)) \
         (i32.const {RET})",
        RET + 4
    )
}

/// `err(failure { kind, code: some(code), params: [] })`; `kind` is the WIT enum's index.
pub fn err_failure(kind: u8, (at, length): (u32, u32)) -> String {
    let payload = RET + 4;
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

/// `sink.write-at(<local>, offset, bytes)`, its result ignored.
pub fn write(sink_local: u32, offset: u64, (at, length): (u32, u32)) -> String {
    format!(
        "(call $write_at (local.get {sink_local}) (i64.const {offset}) (i32.const {at}) \
         (i32.const {length}) (i32.const {SCRATCH}))"
    )
}

/// Put before `open`'s result: keeps the source handle for [`echo_source`].
pub fn remember_source() -> String {
    format!("(i32.store (i32.const {SOURCE_SLOT}) (local.get 0))\n")
}

/// `text` handing back everything a viewer reaches: the first MiB of the source `open` kept,
/// written into the sink and into the log at `level`, the encoding named `encoding`.
pub fn echo_source(level: (u32, u32), encoding: (u32, u32)) -> String {
    let (bytes, length) = (READ_RET + 4, READ_RET + 8);
    format!(
        "(call $read_at (i32.load (i32.const {SOURCE_SLOT})) (i64.const 0) \
         (i32.const {MAX_WINDOW_BYTES}) (i32.const {READ_RET}))\n\
         (call $write_at (local.get 0) (i64.const 0) (i32.load (i32.const {bytes})) \
         (i32.load (i32.const {length})) (i32.const {SCRATCH}))\n\
         (call $log (i32.const {}) (i32.const {}) (i32.load (i32.const {bytes})) \
         (i32.load (i32.const {length})))\n{}",
        level.0,
        level.1,
        ok_pair(encoding)
    )
}

/// `text` writing what `settings.setting(name)` answers into the sink, or `none` (ptr, len of
/// the bytes `none`) when it answers nothing, the encoding named `encoding` (RC-100-14).
pub fn echo_setting(name: (u32, u32), none: (u32, u32), encoding: (u32, u32)) -> String {
    format!(
        "(call $setting (i32.const {}) (i32.const {}) (i32.const {SETTING_RET}))\n\
         (if (i32.load8_u (i32.const {SETTING_RET}))\n\
           (then (call $write_at (local.get 0) (i64.const 0) (i32.load (i32.const {})) \
             (i32.load (i32.const {})) (i32.const {SCRATCH})))\n\
           (else {}))\n{}",
        name.0,
        name.1,
        SETTING_RET + 4,
        SETTING_RET + 8,
        write(0, 0, none),
        ok_pair(encoding)
    )
}

/// A trap.
pub const TRAP: &str = "unreachable";

/// A loop that never ends.
pub const SPIN: &str = "(loop $spin (br $spin)) unreachable";

/// A loop that ends when `should-stop` says so, as a well-behaved viewer's would.
pub const SPIN_UNTIL_STOPPED: &str =
    "(block $stop (loop $spin (br_if $stop (call $should_stop)) (br $spin))) unreachable";

/// `host.log` called `lines` times with `level` and `message` (ptr, len each).
pub fn log_flood(lines: u32, level: (u32, u32), message: (u32, u32)) -> String {
    format!(
        "(local.set $i (i32.const 0))\n\
         (block $done (loop $next\n\
           (br_if $done (i32.ge_u (local.get $i) (i32.const {lines})))\n\
           (call $log (i32.const {}) (i32.const {}) (i32.const {}) (i32.const {}))\n\
           (local.set $i (i32.add (local.get $i) (i32.const 1)))\n\
           (br $next)))\n",
        level.0, level.1, message.0, message.1
    )
}

/// `children` answering `count` nodes with ids 0.., empty labels, no values: a `tree-node` is
/// 32 bytes (id at 0, label at 8, value at 16, has-children at 28), laid from 64 KiB on.
pub fn many_nodes(count: u32) -> String {
    let pages = (u64::from(count) * 32).div_ceil(65536) + 1;
    format!(
        "(drop (memory.grow (i32.const {pages})))\n\
         (local.set $p (i32.const 65536))\n\
         (local.set $i (i32.const 0))\n\
         (block $done (loop $next\n\
           (br_if $done (i32.ge_u (local.get $i) (i32.const {count})))\n\
           (i64.store (local.get $p) (i64.extend_i32_u (local.get $i)))\n\
           (local.set $p (i32.add (local.get $p) (i32.const 32)))\n\
           (local.set $i (i32.add (local.get $i) (i32.const 1)))\n\
           (br $next)))\n{}",
        ok_pair((65536, count))
    )
}

/// The bytes of a `list<token>`: (start, len, class) per span, 12 bytes each.
pub fn token_bytes(spans: &[(u32, u32, u8)]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(spans.len() * 12);
    for (start, length, class) in spans {
        bytes.extend_from_slice(&start.to_le_bytes());
        bytes.extend_from_slice(&length.to_le_bytes());
        bytes.extend_from_slice(&[*class, 0, 0, 0]);
    }
    bytes
}

/// The bytes of a list of lists: (ptr, len) per element, 8 bytes each.
pub fn list_bytes(elements: &[(u32, u32)]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(elements.len() * 8);
    for (at, length) in elements {
        bytes.extend_from_slice(&at.to_le_bytes());
        bytes.extend_from_slice(&length.to_le_bytes());
    }
    bytes
}
