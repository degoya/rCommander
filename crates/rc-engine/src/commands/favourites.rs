//! `connections.manager`, `connections.set_favourite` and `connections.set_folder_favourite`:
//! the window's connection manager, and which connections and folders the drive bar shows as
//! buttons of their own (RC-050-04, RC-100-63).
//!
//! By default the drive bar lists the connections of the tree's root level, and each folder of
//! the root level is a button for all its servers. The manager's mark changes that per
//! connection and per folder: `shown` adds one that sits in a folder, `hidden` takes one of the
//! root level away, so a connection or folder created at the root later still appears without a
//! mark. The lists live in the settings table under [`FAVOURITES_KEY`] (the folder lists with
//! `serde(default)`, so the JSON stored before RC-100-63 still reads); an id whose node is gone
//! is dropped at the next mark, so the lists never outgrow the tree.

use serde::{Deserialize, Serialize};

use crate::{
    Connection, ConnectionFolder, ConnectionId, ConnectionTree, Engine, EngineError, EngineEvent,
    FolderId, FolderNode, Outcome,
};

/// The settings key of the marks.
pub(crate) const FAVOURITES_KEY: &str = "connections.favourites";

/// The marks that differ from the default.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Favourites {
    /// Connections in a folder that the drive bar lists all the same.
    #[serde(default)]
    pub(crate) shown: Vec<ConnectionId>,
    /// Connections of the root level that the drive bar leaves out.
    #[serde(default)]
    pub(crate) hidden: Vec<ConnectionId>,
    /// Folders below the root level that are a button of the drive bar all the same.
    #[serde(default)]
    pub(crate) folders_shown: Vec<FolderId>,
    /// Folders of the root level that are no button of the drive bar.
    #[serde(default)]
    pub(crate) folders_hidden: Vec<FolderId>,
}

impl Favourites {
    /// Whether the drive bar lists `connection`.
    pub(crate) fn contains(&self, connection: &Connection) -> bool {
        if self.shown.contains(&connection.id) {
            return true;
        }
        connection.folder_id.is_none() && !self.hidden.contains(&connection.id)
    }

    /// Marks `connection` as listed or not; a mark equal to the default is no mark.
    pub(crate) fn set(&mut self, connection: &Connection, favourite: bool) {
        let id = connection.id;
        self.shown.retain(|shown| *shown != id);
        self.hidden.retain(|hidden| *hidden != id);
        let by_default = connection.folder_id.is_none();
        match (favourite, by_default) {
            (true, false) => self.shown.push(id),
            (false, true) => self.hidden.push(id),
            _ => {}
        }
    }

    /// Whether the drive bar shows `folder` as a button.
    pub(crate) fn contains_folder(&self, folder: &ConnectionFolder) -> bool {
        if self.folders_shown.contains(&folder.id) {
            return true;
        }
        folder.parent_id.is_none() && !self.folders_hidden.contains(&folder.id)
    }

    /// Marks `folder` as a button or not; a mark equal to the default is no mark.
    pub(crate) fn set_folder(&mut self, folder: &ConnectionFolder, favourite: bool) {
        let id = folder.id;
        self.folders_shown.retain(|shown| *shown != id);
        self.folders_hidden.retain(|hidden| *hidden != id);
        match (favourite, folder.parent_id.is_none()) {
            (true, false) => self.folders_shown.push(id),
            (false, true) => self.folders_hidden.push(id),
            _ => {}
        }
    }

    /// The folders of `tree` the drive bar shows as buttons, in the tree's order.
    pub(crate) fn bar_folders(&self, tree: &ConnectionTree) -> Vec<FolderId> {
        folders_in_order(tree)
            .into_iter()
            .filter(|node| self.contains_folder(&node.folder))
            .map(|node| node.folder.id)
            .collect()
    }

    /// Drops the marks of connections and folders that are no longer in `tree`.
    pub(crate) fn keep_only(&mut self, tree: &ConnectionTree) {
        let ids: Vec<ConnectionId> = connections_in_order(tree)
            .map(|connection| connection.id)
            .collect();
        self.keep_ids(&ids);
        let folders: Vec<FolderId> = folders_in_order(tree)
            .into_iter()
            .map(|node| node.folder.id)
            .collect();
        self.folders_shown.retain(|id| folders.contains(id));
        self.folders_hidden.retain(|id| folders.contains(id));
    }

    /// Drops the marks of connections whose id is not among `ids`.
    pub(crate) fn keep_ids(&mut self, ids: &[ConnectionId]) {
        self.shown.retain(|id| ids.contains(id));
        self.hidden.retain(|id| ids.contains(id));
    }
}

/// Every folder of `tree` as the manager shows it: depth first, a folder before its folders.
pub(crate) fn folders_in_order(tree: &ConnectionTree) -> Vec<&FolderNode> {
    fn walk<'a>(nodes: &'a [FolderNode], into: &mut Vec<&'a FolderNode>) {
        for node in nodes {
            into.push(node);
            walk(&node.folders, into);
        }
    }
    let mut all = Vec::new();
    walk(&tree.folders, &mut all);
    all
}

/// Every connection of `tree` as the manager shows it: the root's own, then each folder's,
/// depth first.
pub(crate) fn connections_in_order(tree: &ConnectionTree) -> impl Iterator<Item = &Connection> {
    fn walk<'a>(nodes: &'a [FolderNode], into: &mut Vec<&'a Connection>) {
        for node in nodes {
            walk(&node.folders, into);
            into.extend(node.connections.iter());
        }
    }
    let mut all: Vec<&Connection> = tree.connections.iter().collect();
    walk(&tree.folders, &mut all);
    all.into_iter()
}

impl Engine {
    /// `connections.manager`: announces that the connection manager is wanted.
    pub(crate) fn connection_manager(&self) -> Outcome {
        self.publish(EngineEvent::ConnectionManager);
        Outcome::Panel(self.snapshot(None))
    }

    /// `connections.set_favourite`: the mark is stored and the drive bar told; answers the drive
    /// bar as it is now.
    pub(crate) async fn set_favourite(
        &self,
        id: ConnectionId,
        favourite: bool,
    ) -> Result<Outcome, EngineError> {
        let db = self.db()?;
        let connection = db.connection(id).await?;
        let tree = db.connection_tree().await?;
        let mut favourites = self.favourites().await;
        favourites.set(&connection, favourite);
        favourites.keep_only(&tree);
        db.set(FAVOURITES_KEY, &favourites).await?;
        self.publish(EngineEvent::Drives);
        Ok(self.list_drives(false).await)
    }

    /// `connections.set_folder_favourite`: the folder's mark is stored and the drive bar told;
    /// answers the drive bar as it is now (RC-100-63).
    pub(crate) async fn set_folder_favourite(
        &self,
        id: FolderId,
        favourite: bool,
    ) -> Result<Outcome, EngineError> {
        let db = self.db()?;
        let tree = db.connection_tree().await?;
        let folder = folders_in_order(&tree)
            .into_iter()
            .find(|node| node.folder.id == id)
            .map(|node| node.folder.clone())
            .ok_or(crate::ConnectionError::FolderNotFound { id })?;
        let mut favourites = self.favourites().await;
        favourites.set_folder(&folder, favourite);
        favourites.keep_only(&tree);
        db.set(FAVOURITES_KEY, &favourites).await?;
        self.publish(EngineEvent::Drives);
        Ok(self.list_drives(false).await)
    }

    /// The stored marks; none when there are none or they do not read.
    pub(crate) async fn favourites(&self) -> Favourites {
        let Ok(db) = self.db() else {
            return Favourites::default();
        };
        match db.get::<Favourites>(FAVOURITES_KEY).await {
            Ok(stored) => stored.unwrap_or_default(),
            Err(error) => {
                tracing::warn!(%error, "the drive bar's server marks cannot be read");
                Favourites::default()
            }
        }
    }
}

#[cfg(test)]
#[path = "favourites_tests.rs"]
mod tests;
