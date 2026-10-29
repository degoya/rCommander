//! Generated bindings for `rcommander:plugin@0.3.0`: the records and the host traits.
//!
//! As rDownloader's `crates/rd-plugin-host/src/component.rs` and `extension/bindings.rs` at commit
//! 26c8592a6 generate them: async imports and exports. One `bindgen!` per world; the secrets world
//! (RC-100-33) maps `types`, `host` and `settings` onto the viewer's modules (`with: { ... }`), so
//! a `failure` is one type across every world and the host functions in `host_impls.rs` are
//! written once. The generated linker and export lookup name the current version alone, so
//! `link.rs`, `viewer/guest.rs` and `secrets/guest.rs` do both under the manifest's version
//! instead (RC-100-14).

wasmtime::component::bindgen!({
    path: "../rc-plugin-api/wit",
    world: "viewer-plugin",
    imports: { default: async },
    exports: { default: async },
});

/// The secrets world's own interfaces: `http` and the `secrets` export.
pub(crate) mod secrets_world {
    wasmtime::component::bindgen!({
        path: "../rc-plugin-api/wit",
        world: "secrets-plugin",
        imports: { default: async },
        exports: { default: async },
        with: {
            "rcommander:plugin/types": crate::bindings::rcommander::plugin::types,
            "rcommander:plugin/host": crate::bindings::rcommander::plugin::host,
            "rcommander:plugin/settings": crate::bindings::rcommander::plugin::settings,
        },
    });
}
