//! The order of a folder's folders and of its connections (RC-100-63): `sort_order` is a node's
//! position among its siblings of the same kind. A move takes the node out of its old level,
//! puts it at the position asked for (clamped to the level; `None` is the end) and numbers both
//! levels densely again, 0, 1, 2, ..., inside the move's transaction. Shared credentials keep
//! their order by name and are never placed.

use sqlx::SqliteConnection;

use crate::connections::{FolderId, TreeResult};

/// Which siblings a node is placed among.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    /// A folder among the folders of its parent.
    Folders,
    /// A connection among the connections of its folder.
    Connections,
}

impl Kind {
    /// The ids of the level `parent` in display order, `id` left out.
    const fn siblings(self) -> &'static str {
        match self {
            Self::Folders => {
                "SELECT id FROM connection_folders WHERE ifnull(parent_id, 0) = ifnull(?, 0) \
                 AND id <> ? ORDER BY sort_order, name COLLATE NOCASE, id"
            }
            Self::Connections => {
                "SELECT id FROM connections WHERE ifnull(folder_id, 0) = ifnull(?, 0) \
                 AND id <> ? ORDER BY sort_order, name COLLATE NOCASE, id"
            }
        }
    }

    /// Sets one node's `sort_order`.
    const fn renumber(self) -> &'static str {
        match self {
            Self::Folders => "UPDATE connection_folders SET sort_order = ? WHERE id = ?",
            Self::Connections => "UPDATE connections SET sort_order = ? WHERE id = ?",
        }
    }

    /// The node's own level (its parent or folder).
    const fn level(self) -> &'static str {
        match self {
            Self::Folders => "SELECT parent_id FROM connection_folders WHERE id = ?",
            Self::Connections => "SELECT folder_id FROM connections WHERE id = ?",
        }
    }
}

/// Where `position` puts a node among `count` siblings: clamped to `0..=count`, the end for
/// `None`.
pub(super) fn slot(position: Option<i64>, count: usize) -> usize {
    match position {
        None => count,
        Some(position) => usize::try_from(position.max(0)).map_or(count, |at| at.min(count)),
    }
}

/// The position of the node `id` among its siblings of `kind`, as the tree lists them.
///
/// # Errors
///
/// The read fails; a node that is gone counts as the first.
pub(super) async fn position(
    connection: &mut SqliteConnection,
    kind: Kind,
    id: i64,
) -> TreeResult<i64> {
    let parent = sqlx::query_scalar::<_, Option<i64>>(kind.level())
        .bind(id)
        .fetch_optional(&mut *connection)
        .await?
        .flatten();
    let all: Vec<i64> = sqlx::query_scalar(kind.siblings())
        .bind(parent)
        .bind(0_i64)
        .fetch_all(&mut *connection)
        .await?;
    let at = all.iter().position(|sibling| *sibling == id).unwrap_or(0);
    Ok(i64::try_from(at).unwrap_or(i64::MAX))
}

/// Puts the node `id`, already moved into `parent`, at `position` among its siblings of `kind`
/// and numbers that level densely; the level it came from, `old` when it differs, is numbered
/// again too. Runs inside the caller's transaction.
///
/// # Errors
///
/// A write fails.
pub(super) async fn place(
    connection: &mut SqliteConnection,
    kind: Kind,
    id: i64,
    (old, parent): (Option<FolderId>, Option<FolderId>),
    position: Option<i64>,
) -> TreeResult<()> {
    let mut level = siblings(connection, kind, parent, id).await?;
    let at = slot(position, level.len());
    level.insert(at, id);
    number(connection, kind, &level).await?;
    if old != parent {
        let left = siblings(connection, kind, old, id).await?;
        number(connection, kind, &left).await?;
    }
    Ok(())
}

async fn siblings(
    connection: &mut SqliteConnection,
    kind: Kind,
    parent: Option<FolderId>,
    id: i64,
) -> TreeResult<Vec<i64>> {
    Ok(sqlx::query_scalar(kind.siblings())
        .bind(parent.map(|parent| parent.0))
        .bind(id)
        .fetch_all(&mut *connection)
        .await?)
}

/// `sort_order` 0, 1, 2, ... for `ids` in this order.
async fn number(connection: &mut SqliteConnection, kind: Kind, ids: &[i64]) -> TreeResult<()> {
    for (order, id) in (0_i64..).zip(ids) {
        sqlx::query(kind.renumber())
            .bind(order)
            .bind(id)
            .execute(&mut *connection)
            .await?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "order_tests.rs"]
mod tests;
