//! The `compare` export of a component, found under the contract version its manifest names, as
//! `inventory/guest.rs` finds the inventory's (RC-100-41).

use wasmtime::{
    Store,
    component::{Component, ComponentExportIndex, Instance, TypedFunc},
};

use crate::{
    bindings::{
        compare_world::exports::rcommander::plugin::compare as wit, rcommander::plugin::types,
    },
    runtime::{PluginStoreState, versioned},
};

/// What `compare` returns.
type Answer = (Result<wit::Comparison, types::Failure>,);

/// Where `status` and `compare` are in one component, found before instantiating.
pub(crate) struct CompareIndices {
    status: ComponentExportIndex,
    compare: ComponentExportIndex,
}

impl CompareIndices {
    /// The export `rcommander:plugin/compare@<api_version>` and its functions; `None` when the
    /// component is not a compare plugin of that version.
    pub(crate) fn new(component: &Component, api_version: &str) -> Option<Self> {
        let compare = component
            .get_export_index(None, versioned("rcommander:plugin/compare", api_version))?;
        let function = |name: &str| component.get_export_index(Some(&compare), name);
        Some(Self {
            status: function("status")?,
            compare: function("compare")?,
        })
    }

    /// The typed functions of `instance`; an error when one has other types than the contract's.
    pub(crate) fn load(
        &self,
        store: &mut Store<PluginStoreState>,
        instance: &Instance,
    ) -> wasmtime::Result<CompareGuest> {
        Ok(CompareGuest {
            status: instance.get_typed_func(&mut *store, self.status)?,
            compare: instance.get_typed_func(&mut *store, self.compare)?,
        })
    }
}

/// `status` and `compare` in one instance.
pub(crate) struct CompareGuest {
    status: TypedFunc<(), (wit::CompareState,)>,
    compare: TypedFunc<(u32, u32), Answer>,
}

impl CompareGuest {
    pub(crate) async fn call_status(
        &self,
        store: &mut Store<PluginStoreState>,
    ) -> wasmtime::Result<wit::CompareState> {
        let (state,) = self.status.call_async(store, ()).await?;
        Ok(state)
    }

    pub(crate) async fn call_compare(
        &self,
        store: &mut Store<PluginStoreState>,
        a: u32,
        b: u32,
    ) -> wasmtime::Result<Result<wit::Comparison, types::Failure>> {
        let (answer,) = self.compare.call_async(store, (a, b)).await?;
        Ok(answer)
    }
}
