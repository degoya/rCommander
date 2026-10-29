//! The `viewer` export of a component, found under the contract version its manifest names.
//!
//! The generated `ViewerPlugin` looks the export up as `rcommander:plugin/viewer@0.2.0`, the one
//! version the bindings come from, and Wasmtime matches a `0.x` version only within its minor, so
//! a component built against 0.1.0 would not be a viewer to it. `interface viewer` is the same in
//! every supported version, so its functions are looked up by name under the manifest's version
//! and typed with the generated records (RC-100-14). A component whose functions have other types
//! fails [`ViewerIndices::load`], where it failed the generated instantiation before.

use wasmtime::{
    Store,
    component::{
        Component, ComponentExportIndex, ComponentNamedList, ComponentType, Instance, Lift, Lower,
        TypedFunc,
    },
};

use crate::{
    bindings::{exports::rcommander::plugin::viewer as wit, rcommander::plugin::types},
    runtime::{PluginStoreState, versioned},
};

/// What an export with a WIT `result<T, failure>` returns.
type Answer<T> = (Result<T, types::Failure>,);

/// Where the functions of `interface viewer` are in one component, found before instantiating.
pub(crate) struct ViewerIndices {
    probe: ComponentExportIndex,
    open: ComponentExportIndex,
    text: ComponentExportIndex,
    tokens: ComponentExportIndex,
    image: ComponentExportIndex,
    page: ComponentExportIndex,
    children: ComponentExportIndex,
    search: ComponentExportIndex,
    thumbnail: ComponentExportIndex,
}

impl ViewerIndices {
    /// The export `rcommander:plugin/viewer@<api_version>` and each of its functions; `None` when
    /// the component is not a viewer of that version.
    pub(crate) fn new(component: &Component, api_version: &str) -> Option<Self> {
        let viewer =
            component.get_export_index(None, versioned("rcommander:plugin/viewer", api_version))?;
        let function = |name: &str| component.get_export_index(Some(&viewer), name);
        Some(Self {
            probe: function("probe")?,
            open: function("open")?,
            text: function("text")?,
            tokens: function("tokens")?,
            image: function("image")?,
            page: function("page")?,
            children: function("children")?,
            search: function("search")?,
            thumbnail: function("thumbnail")?,
        })
    }

    /// The typed functions of `instance`; an error when one has other types than the contract's.
    pub(crate) fn load(
        &self,
        store: &mut Store<PluginStoreState>,
        instance: &Instance,
    ) -> wasmtime::Result<ViewerGuest> {
        // Exported by every viewer, called by none of rCommander's windows yet; its type is
        // still the contract's.
        instance.get_typed_func::<(u32, u32, u32), Answer<String>>(&mut *store, &self.thumbnail)?;
        Ok(ViewerGuest {
            probe: instance.get_typed_func(&mut *store, self.probe)?,
            open: instance.get_typed_func(&mut *store, self.open)?,
            text: instance.get_typed_func(&mut *store, self.text)?,
            tokens: instance.get_typed_func(&mut *store, self.tokens)?,
            image: instance.get_typed_func(&mut *store, self.image)?,
            page: instance.get_typed_func(&mut *store, self.page)?,
            children: instance.get_typed_func(&mut *store, self.children)?,
            search: instance.get_typed_func(&mut *store, self.search)?,
        })
    }
}

/// The functions of `interface viewer` in one instance, each call one call into the guest.
pub(crate) struct ViewerGuest {
    probe: TypedFunc<(u32,), (bool,)>,
    open: TypedFunc<(u32, wit::Purpose, wit::Theme), Answer<wit::Document>>,
    text: TypedFunc<(u32,), Answer<String>>,
    tokens: TypedFunc<(u32, u32), Answer<Vec<Vec<wit::Token>>>>,
    image: TypedFunc<(u32,), Answer<String>>,
    page: TypedFunc<(u32, u32, u32, u32), Answer<()>>,
    children: TypedFunc<(Option<u64>,), Answer<Vec<wit::TreeNode>>>,
    search: TypedFunc<(String, u32, bool), Answer<Option<u32>>>,
}

impl ViewerGuest {
    pub(crate) async fn call_probe(
        &self,
        store: &mut Store<PluginStoreState>,
        src: u32,
    ) -> wasmtime::Result<bool> {
        call(self.probe, store, (src,)).await
    }

    pub(crate) async fn call_open(
        &self,
        store: &mut Store<PluginStoreState>,
        src: u32,
        purpose: wit::Purpose,
        theme: wit::Theme,
    ) -> wasmtime::Result<Result<wit::Document, types::Failure>> {
        call(self.open, store, (src, purpose, theme)).await
    }

    pub(crate) async fn call_text(
        &self,
        store: &mut Store<PluginStoreState>,
        out: u32,
    ) -> wasmtime::Result<Result<String, types::Failure>> {
        call(self.text, store, (out,)).await
    }

    pub(crate) async fn call_tokens(
        &self,
        store: &mut Store<PluginStoreState>,
        first_line: u32,
        count: u32,
    ) -> wasmtime::Result<Result<Vec<Vec<wit::Token>>, types::Failure>> {
        call(self.tokens, store, (first_line, count)).await
    }

    pub(crate) async fn call_image(
        &self,
        store: &mut Store<PluginStoreState>,
        out: u32,
    ) -> wasmtime::Result<Result<String, types::Failure>> {
        call(self.image, store, (out,)).await
    }

    pub(crate) async fn call_page(
        &self,
        store: &mut Store<PluginStoreState>,
        index: u32,
        width: u32,
        height: u32,
        out: u32,
    ) -> wasmtime::Result<Result<(), types::Failure>> {
        call(self.page, store, (index, width, height, out)).await
    }

    pub(crate) async fn call_children(
        &self,
        store: &mut Store<PluginStoreState>,
        node: Option<u64>,
    ) -> wasmtime::Result<Result<Vec<wit::TreeNode>, types::Failure>> {
        call(self.children, store, (node,)).await
    }

    pub(crate) async fn call_search(
        &self,
        store: &mut Store<PluginStoreState>,
        needle: &str,
        from_page: u32,
        backwards: bool,
    ) -> wasmtime::Result<Result<Option<u32>, types::Failure>> {
        call(
            self.search,
            store,
            (needle.to_owned(), from_page, backwards),
        )
        .await
    }
}

/// One call of `func`, its single result unwrapped.
async fn call<P, R>(
    func: TypedFunc<P, (R,)>,
    store: &mut Store<PluginStoreState>,
    params: P,
) -> wasmtime::Result<R>
where
    P: ComponentNamedList + Lower + Send + Sync,
    R: ComponentType + Lift + Send + Sync + 'static,
{
    let (answer,) = func.call_async(store, params).await?;
    Ok(answer)
}
