//! Jump hosts (RC-110-03): a connection names at most one saved SSH connection to set its
//! sessions up through, which may name its own, up to [`MAX_JUMPS`] hops.
//!
//! Every write that sets a jump host, changes a connection's kind or deletes one is checked here,
//! inside its own `BEGIN IMMEDIATE`: the jump host exists and is SSH, the chain leads back
//! neither to the connection itself (`jump_cycle`) nor past [`MAX_JUMPS`] for any connection that
//! goes through it (`jump_chain_too_long`), a jump host in use stays SSH (`jump_needs_ssh`) and is
//! not deleted (`connection_is_jump_host`). The engine walks the chain again when it connects
//! ([`crate::Db::jump_chain`]).

use sqlx::SqliteConnection;

use crate::{
    Db,
    connections::{
        Connection, ConnectionError, ConnectionId, ConnectionKind, FolderId, TreeResult, fetch,
    },
};

/// The most jump hosts in front of one connection: a bound of security, not a setting.
pub const MAX_JUMPS: usize = 4;

/// How far the recursive reads look: past any chain the writes allow, so a damaged table ends.
const WALK_LIMIT: i64 = 16;

impl Db {
    /// The jump hosts of connection `id`, the one connected to first at the front; empty when it
    /// connects directly.
    ///
    /// # Errors
    ///
    /// The connection or a jump host missing, a chain that is circular, longer than
    /// [`MAX_JUMPS`] or holds a connection that is not SSH (a table changed behind the
    /// repository), or the read fails.
    pub async fn jump_chain(&self, id: ConnectionId) -> TreeResult<Vec<Connection>> {
        let mut reader = self.readers().acquire().await?;
        let start = fetch(&mut reader, id).await?;
        let mut chain: Vec<Connection> = Vec::new();
        let mut next = start.jump_id;
        while let Some(hop) = next {
            if hop == id || chain.iter().any(|seen| seen.id == hop) {
                return Err(ConnectionError::JumpCycle);
            }
            if chain.len() == MAX_JUMPS {
                return Err(ConnectionError::JumpChainTooLong { max: MAX_JUMPS });
            }
            let connection = fetch(&mut reader, hop).await?;
            if connection.kind != ConnectionKind::Ssh {
                return Err(ConnectionError::JumpNeedsSsh);
            }
            next = connection.jump_id;
            chain.push(connection);
        }
        chain.reverse();
        Ok(chain)
    }

    /// The names of the connections that name `id` as their jump host, by name.
    ///
    /// # Errors
    ///
    /// The read fails.
    pub async fn jump_users(&self, id: ConnectionId) -> TreeResult<Vec<String>> {
        let mut reader = self.readers().acquire().await?;
        users(&mut reader, id).await
    }
}

/// Refuses the jump host `jump` for the connection `id` (`None` for a new one) of `kind`, and a
/// change of a jump host in use to another kind.
pub(crate) async fn check(
    connection: &mut SqliteConnection,
    id: Option<ConnectionId>,
    kind: ConnectionKind,
    jump: Option<ConnectionId>,
) -> TreeResult<()> {
    let below = match id {
        Some(id) => depth_below(connection, id).await?,
        None => 0,
    };
    if kind != ConnectionKind::Ssh && (below > 0 || jump.is_some()) {
        return Err(ConnectionError::JumpNeedsSsh);
    }
    let Some(jump) = jump else {
        return Ok(());
    };
    if Some(jump) == id {
        return Err(ConnectionError::JumpCycle);
    }
    // The jump host and the hosts in front of it.
    let mut above = 0usize;
    let mut next = Some(jump);
    while let Some(hop) = next {
        if Some(hop) == id {
            return Err(ConnectionError::JumpCycle);
        }
        above += 1;
        if above > MAX_JUMPS {
            return Err(ConnectionError::JumpChainTooLong { max: MAX_JUMPS });
        }
        let host = fetch(connection, hop).await?;
        if host.kind != ConnectionKind::Ssh {
            return Err(ConnectionError::JumpNeedsSsh);
        }
        next = host.jump_id;
    }
    // A connection `below` levels under this one has `below` more hops in front of it.
    if above + below > MAX_JUMPS {
        return Err(ConnectionError::JumpChainTooLong { max: MAX_JUMPS });
    }
    Ok(())
}

/// Refuses deleting the connection `id` while another names it as its jump host.
pub(crate) async fn guard_delete(
    connection: &mut SqliteConnection,
    id: ConnectionId,
) -> TreeResult<()> {
    let users = users(connection, id).await?;
    if users.is_empty() {
        return Ok(());
    }
    Err(ConnectionError::ConnectionIsJumpHost { id, users })
}

/// Refuses deleting the folder `folder` while a connection outside it names one inside it as its
/// jump host; jump hosts deleted together with their users go.
pub(crate) async fn guard_folder_delete(
    connection: &mut SqliteConnection,
    folder: FolderId,
) -> TreeResult<()> {
    let found: Option<(i64, String)> = sqlx::query_as(
        "WITH RECURSIVE inside (id, level) AS ( \
             SELECT id, 1 FROM connection_folders WHERE id = ? \
             UNION ALL \
             SELECT folder.id, inside.level + 1 \
             FROM connection_folders AS folder JOIN inside ON folder.parent_id = inside.id \
             WHERE inside.level <= 64 \
         ), doomed (id) AS ( \
             SELECT id FROM connections WHERE folder_id IN (SELECT id FROM inside) \
         ) \
         SELECT host.id, user.name FROM connections AS user \
         JOIN doomed AS host ON user.jump_id = host.id \
         WHERE user.id NOT IN (SELECT id FROM doomed) ORDER BY user.name LIMIT 1",
    )
    .bind(folder.0)
    .fetch_optional(&mut *connection)
    .await?;
    let Some((host, _)) = found else {
        return Ok(());
    };
    let host = ConnectionId(host);
    Err(ConnectionError::ConnectionIsJumpHost {
        id: host,
        users: users(connection, host).await?,
    })
}

/// The names of the connections that name `id` as their jump host.
async fn users(connection: &mut SqliteConnection, id: ConnectionId) -> TreeResult<Vec<String>> {
    Ok(
        sqlx::query_scalar("SELECT name FROM connections WHERE jump_id = ? ORDER BY name, id")
            .bind(id.0)
            .fetch_all(&mut *connection)
            .await?,
    )
}

/// How many levels of connections reach the connection `id` as a jump host: 0 when none names
/// it, 1 when some do directly, and so on.
async fn depth_below(connection: &mut SqliteConnection, id: ConnectionId) -> TreeResult<usize> {
    let depth: i64 = sqlx::query_scalar(
        "WITH RECURSIVE below (id, depth) AS ( \
             SELECT id, 1 FROM connections WHERE jump_id = ? \
             UNION ALL \
             SELECT user.id, below.depth + 1 \
             FROM connections AS user JOIN below ON user.jump_id = below.id \
             WHERE below.depth < ? \
         ) SELECT ifnull(max(depth), 0) FROM below",
    )
    .bind(id.0)
    .bind(WALK_LIMIT)
    .fetch_one(&mut *connection)
    .await?;
    Ok(usize::try_from(depth).unwrap_or(usize::MAX))
}

#[cfg(test)]
#[path = "jump_tests.rs"]
mod tests;
