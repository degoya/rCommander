//! `Engine::dispatch` for the `inventory` group (`registry/inventory.rs`, RC-100-39).

/// The `inventory` group's arms, in the order of its rows (`dispatch/arms.rs`).
macro_rules! arms {
    ($next:ident, $state:tt, $self:ident, { $($done:tt)* }) => {
        $next! { $state, $self, { $($done)*
            Command::RefreshInventory { source, preview } => {
                Box::pin($self.refresh_inventory(source, preview)).await
            }
            Command::DeleteInventorySource { source } => {
                $self.delete_inventory_source(source).await
            }
            Command::SaveInventorySource(request) => {
                Box::pin($self.save_inventory_source(request)).await
            }
            Command::ListInventorySources {} => $self.list_inventory_sources().await,
        } }
    };
}
pub(crate) use arms;
