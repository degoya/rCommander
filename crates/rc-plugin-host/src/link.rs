//! The linker of the viewer world: `host`, `source`, `sink` and, when granted, `settings`, under
//! the contract version the manifest names.
//!
//! Copied and adapted from rDownloader's `crates/rd-plugin-host/src/component/link.rs` (one grant,
//! one interface) and `extension/source.rs` (`add_metered_read_to_linker`, RD-191-06) at commit
//! 26c8592a6; the same fuel credit on every `sink.write-at` (`docs/plugins.md`, "Bytes through
//! host-provided streams").
//!
//! Linked by hand rather than through the generated `add_to_linker`, which names the one version
//! the bindings are generated from (RC-100-14): Wasmtime matches a `0.x` version only within its
//! minor, so a component built against 0.1.0 would find no `rcommander:plugin/host@0.3.0` it
//! imports. The interfaces of 0.1.0 are those of 0.2.0 without `settings`, and 0.3.0 adds `http`
//! for a secret source only (RC-100-33), so the functions of `host_impls.rs` serve every version
//! in [`crate::SUPPORTED_API_VERSIONS`].

use wasmtime::{StoreContextMut, component::Linker};

use crate::{
    bindings::{
        rcommander::plugin::{host, settings, sink, source, types},
        secrets_world::rcommander::plugin::http,
    },
    runtime::{PluginStoreState, versioned},
};

type Context<'a> = StoreContextMut<'a, PluginStoreState>;

/// Links what a viewer of contract `api_version` is made of: `host`, `source` and `sink`
/// (`types` holds no function), and `settings` when `settings` is granted. Nothing else is
/// linked, so a component importing more fails to instantiate even if it got past the import
/// check.
pub(crate) fn link_viewer(
    linker: &mut Linker<PluginStoreState>,
    api_version: &str,
    settings: bool,
) -> wasmtime::Result<()> {
    link_host(linker, api_version)?;
    link_source(linker, api_version)?;
    link_sink(linker, api_version)?;
    if settings {
        link_settings(linker, api_version)?;
    }
    Ok(())
}

/// Links what a secret source is made of (RC-100-33): `host`, and `settings` and `http` when
/// granted. It reads no file and writes none, so neither `source` nor `sink` is linked.
pub(crate) fn link_secrets(
    linker: &mut Linker<PluginStoreState>,
    api_version: &str,
    settings: bool,
    http: bool,
) -> wasmtime::Result<()> {
    link_host(linker, api_version)?;
    if settings {
        link_settings(linker, api_version)?;
    }
    if http {
        link_http(linker, api_version)?;
    }
    Ok(())
}

fn link_host(linker: &mut Linker<PluginStoreState>, api_version: &str) -> wasmtime::Result<()> {
    let mut instance = linker.instance(&versioned("rcommander:plugin/host", api_version))?;
    instance.func_wrap_async::<(String, String), (), _>("log", |mut store, (level, message)| {
        Box::new(async move {
            host::Host::log(store.data_mut(), level, message).await;
            Ok(())
        })
    })?;
    instance.func_wrap_async::<(), (u64,), _>("now-unix-seconds", |mut store, ()| {
        Box::new(async move { Ok((host::Host::now_unix_seconds(store.data_mut()).await,)) })
    })?;
    instance.func_wrap_async::<(u32,), (Vec<u8>,), _>("random-bytes", |mut store, (count,)| {
        Box::new(async move { Ok((host::Host::random_bytes(store.data_mut(), count).await,)) })
    })?;
    Ok(())
}

fn link_source(linker: &mut Linker<PluginStoreState>, api_version: &str) -> wasmtime::Result<()> {
    let mut instance = linker.instance(&versioned("rcommander:plugin/source", api_version))?;
    instance.func_wrap_async::<(u32,), (types::SourceInfo,), _>("info", |mut store, (src,)| {
        Box::new(async move { Ok((source::Host::info(store.data_mut(), src).await,)) })
    })?;
    instance.func_wrap_async::<(u32, u64, u32), (Result<Vec<u8>, types::Failure>,), _>(
        "read-at",
        |mut store, (src, offset, length)| {
            Box::new(async move { Ok((metered_read(&mut store, src, offset, length).await?,)) })
        },
    )?;
    instance.func_wrap_async::<(u64, Option<u64>), (), _>(
        "progress",
        |mut store, (done, total)| {
            Box::new(async move {
                source::Host::progress(store.data_mut(), done, total).await;
                Ok(())
            })
        },
    )?;
    instance.func_wrap_async::<(), (bool,), _>("should-stop", |mut store, ()| {
        Box::new(async move { Ok((source::Host::should_stop(store.data_mut()).await,)) })
    })?;
    Ok(())
}

fn link_sink(linker: &mut Linker<PluginStoreState>, api_version: &str) -> wasmtime::Result<()> {
    let mut instance = linker.instance(&versioned("rcommander:plugin/sink", api_version))?;
    instance.func_wrap_async::<(u32, u64, Vec<u8>), (Result<(), types::Failure>,), _>(
        "write-at",
        |mut store, (out, offset, bytes)| {
            Box::new(async move { Ok((metered_write(&mut store, out, offset, bytes).await?,)) })
        },
    )?;
    instance.func_wrap_async::<(u32,), (u64,), _>("committed", |mut store, (out,)| {
        Box::new(async move { Ok((sink::Host::committed(store.data_mut(), out).await,)) })
    })?;
    instance.func_wrap_async::<(u32,), (Result<(), types::Failure>,), _>(
        "sync",
        |mut store, (out,)| {
            Box::new(async move { Ok((sink::Host::sync(store.data_mut(), out).await,)) })
        },
    )?;
    instance.func_wrap_async::<(u64, Option<u64>), (), _>(
        "progress",
        |mut store, (done, total)| {
            Box::new(async move {
                sink::Host::progress(store.data_mut(), done, total).await;
                Ok(())
            })
        },
    )?;
    instance.func_wrap_async::<(), (bool,), _>("should-stop", |mut store, ()| {
        Box::new(async move { Ok((sink::Host::should_stop(store.data_mut()).await,)) })
    })?;
    Ok(())
}

fn link_settings(linker: &mut Linker<PluginStoreState>, api_version: &str) -> wasmtime::Result<()> {
    let mut instance = linker.instance(&versioned("rcommander:plugin/settings", api_version))?;
    instance.func_wrap_async::<(String,), (Option<String>,), _>(
        "setting",
        |mut store, (name,)| {
            Box::new(async move { Ok((settings::Host::setting(store.data_mut(), name).await,)) })
        },
    )?;
    Ok(())
}

fn link_http(linker: &mut Linker<PluginStoreState>, api_version: &str) -> wasmtime::Result<()> {
    type Request = (String, String, Vec<(String, String)>, Vec<u8>);
    let mut instance = linker.instance(&versioned("rcommander:plugin/http", api_version))?;
    instance.func_wrap_async::<Request, (Result<http::HttpResponse, types::Failure>,), _>(
        "request",
        |mut store, (method, url, headers, body)| {
            Box::new(async move {
                Ok((http::Host::request(store.data_mut(), method, url, headers, body).await,))
            })
        },
    )?;
    Ok(())
}

/// `read-at` with the store in hand: a host function sees the store's data, and fuel lives on
/// the store itself. It credits [`crate::streams::FUEL_PER_BYTE`] for every byte it moved,
/// within the call's ceiling, so a viewer over a 4 GiB file does not run out of fuel by reading
/// it; `write-at` likewise.
async fn metered_read(
    store: &mut Context<'_>,
    src: u32,
    offset: u64,
    length: u32,
) -> wasmtime::Result<Result<Vec<u8>, types::Failure>> {
    let read = source::Host::read_at(store.data_mut(), src, offset, length).await;
    if let Ok(bytes) = &read {
        let credit = store.data_mut().streams.take_read_credit(bytes.len());
        credit_fuel(store, credit)?;
    }
    Ok(read)
}

async fn metered_write(
    store: &mut Context<'_>,
    out: u32,
    offset: u64,
    bytes: Vec<u8>,
) -> wasmtime::Result<Result<(), types::Failure>> {
    let length = bytes.len();
    let written = sink::Host::write_at(store.data_mut(), out, offset, bytes).await;
    if written.is_ok() {
        let credit = store.data_mut().streams.take_write_credit(length);
        credit_fuel(store, credit)?;
    }
    Ok(written)
}

fn credit_fuel(store: &mut Context<'_>, credit: u64) -> wasmtime::Result<()> {
    if credit > 0 {
        let fuel = store.get_fuel()?;
        store.set_fuel(fuel.saturating_add(credit))?;
    }
    Ok(())
}
