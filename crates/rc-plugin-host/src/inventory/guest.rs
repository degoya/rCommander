//! The `inventory` export of a component, found under the contract version its manifest names,
//! as `secrets/guest.rs` finds the secret source's.

use wasmtime::{
    Store,
    component::{Component, ComponentExportIndex, Instance, TypedFunc},
};

use crate::{
    bindings::{
        inventory_world::exports::rcommander::plugin::inventory as wit, rcommander::plugin::types,
    },
    runtime::{PluginStoreState, versioned},
};

/// What `list` returns.
type Answer = (Result<Vec<wit::InventoryHost>, types::Failure>,);

/// Where `list` is in one component, found before instantiating.
pub(crate) struct InventoryIndices {
    list: ComponentExportIndex,
}

impl InventoryIndices {
    /// The export `rcommander:plugin/inventory@<api_version>` and its `list`; `None` when the
    /// component is not an inventory of that version.
    pub(crate) fn new(component: &Component, api_version: &str) -> Option<Self> {
        let inventory = component
            .get_export_index(None, versioned("rcommander:plugin/inventory", api_version))?;
        Some(Self {
            list: component.get_export_index(Some(&inventory), "list")?,
        })
    }

    /// The typed function of `instance`; an error when it has other types than the contract's.
    pub(crate) fn load(
        &self,
        store: &mut Store<PluginStoreState>,
        instance: &Instance,
    ) -> wasmtime::Result<InventoryGuest> {
        Ok(InventoryGuest {
            list: instance.get_typed_func(&mut *store, self.list)?,
        })
    }
}

/// `list` in one instance.
pub(crate) struct InventoryGuest {
    list: TypedFunc<(), Answer>,
}

impl InventoryGuest {
    pub(crate) async fn call_list(
        &self,
        store: &mut Store<PluginStoreState>,
    ) -> wasmtime::Result<Result<Vec<wit::InventoryHost>, types::Failure>> {
        let (answer,) = self.list.call_async(store, ()).await?;
        Ok(answer)
    }
}
