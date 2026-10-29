//! The `secrets` export of a component, found under the contract version its manifest names, as
//! `viewer/guest.rs` finds the viewer's (RC-100-14): the generated lookup would name the current
//! version alone.

use wasmtime::{
    Store,
    component::{Component, ComponentExportIndex, Instance, TypedFunc},
};

use crate::{
    bindings::{
        rcommander::plugin::types, secrets_world::exports::rcommander::plugin::secrets as wit,
    },
    runtime::{PluginStoreState, versioned},
};

/// What an export with a WIT `result<T, failure>` returns.
type Answer<T> = (Result<T, types::Failure>,);

/// Where the functions of `interface secrets` are in one component, found before instantiating.
pub(crate) struct SecretsIndices {
    status: ComponentExportIndex,
    search: ComponentExportIndex,
    get: ComponentExportIndex,
}

impl SecretsIndices {
    /// The export `rcommander:plugin/secrets@<api_version>` and each of its functions; `None`
    /// when the component is not a secret source of that version.
    pub(crate) fn new(component: &Component, api_version: &str) -> Option<Self> {
        let secrets = component
            .get_export_index(None, versioned("rcommander:plugin/secrets", api_version))?;
        let function = |name: &str| component.get_export_index(Some(&secrets), name);
        Some(Self {
            status: function("status")?,
            search: function("search")?,
            get: function("get")?,
        })
    }

    /// The typed functions of `instance`; an error when one has other types than the contract's.
    pub(crate) fn load(
        &self,
        store: &mut Store<PluginStoreState>,
        instance: &Instance,
    ) -> wasmtime::Result<SecretsGuest> {
        Ok(SecretsGuest {
            status: instance.get_typed_func(&mut *store, self.status)?,
            search: instance.get_typed_func(&mut *store, self.search)?,
            get: instance.get_typed_func(&mut *store, self.get)?,
        })
    }
}

/// The functions of `interface secrets` in one instance, each call one call into the guest.
pub(crate) struct SecretsGuest {
    status: TypedFunc<(), (wit::SourceState,)>,
    search: TypedFunc<(String, u32), Answer<Vec<wit::SecretEntry>>>,
    get: TypedFunc<(String, wit::SecretField), Answer<String>>,
}

impl SecretsGuest {
    pub(crate) async fn call_status(
        &self,
        store: &mut Store<PluginStoreState>,
    ) -> wasmtime::Result<wit::SourceState> {
        let (state,) = self.status.call_async(store, ()).await?;
        Ok(state)
    }

    pub(crate) async fn call_search(
        &self,
        store: &mut Store<PluginStoreState>,
        query: &str,
        limit: u32,
    ) -> wasmtime::Result<Result<Vec<wit::SecretEntry>, types::Failure>> {
        let (answer,) = self
            .search
            .call_async(store, (query.to_owned(), limit))
            .await?;
        Ok(answer)
    }

    pub(crate) async fn call_get(
        &self,
        store: &mut Store<PluginStoreState>,
        id: &str,
        field: wit::SecretField,
    ) -> wasmtime::Result<Result<String, types::Failure>> {
        let (answer,) = self.get.call_async(store, (id.to_owned(), field)).await?;
        Ok(answer)
    }
}
