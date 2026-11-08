-- Jump hosts and local port forwards of SSH connections (RC-110-03).
--
-- `connections.jump_id` names the saved SSH connection a session is set up through (OpenSSH's
-- ProxyJump): the next hop is reached over a `direct-tcpip` channel of that one, which may name
-- its own. NULL connects directly. The foreign key takes no action: a folder that deletes a jump
-- host together with its users goes through, a jump host alone does not. The repository refuses a
-- cycle, a chain of more than four hops, a jump host that is not SSH and a delete of one in use,
-- inside the write's own transaction.
--
-- `connection_forwards` are a connection's local forwards (`ssh -L`): a listener on `bind_address`
-- and `bind_port` of this computer whose every accepted connection is carried over a
-- `direct-tcpip` channel to `target_host`:`target_port` as the server sees it. `kind` is 'local'
-- only for now and carries no CHECK, like `connections.kind`; the code validates it. A forward
-- with `auto_start` starts when its connection's session comes up. The rows go with their
-- connection. No secret lives in either column or table.

ALTER TABLE connections ADD COLUMN jump_id INTEGER REFERENCES connections (id);

CREATE INDEX connections_jump ON connections (jump_id) WHERE jump_id IS NOT NULL;

CREATE TABLE connection_forwards (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    connection_id INTEGER NOT NULL REFERENCES connections (id) ON DELETE CASCADE,
    kind          TEXT NOT NULL DEFAULT 'local',
    bind_address  TEXT NOT NULL DEFAULT '127.0.0.1'
                  CHECK (length(bind_address) BETWEEN 1 AND 255),
    bind_port     INTEGER NOT NULL CHECK (bind_port BETWEEN 1 AND 65535),
    target_host   TEXT NOT NULL CHECK (length(trim(target_host)) BETWEEN 1 AND 255),
    target_port   INTEGER NOT NULL CHECK (target_port BETWEEN 1 AND 65535),
    auto_start    INTEGER NOT NULL DEFAULT 0 CHECK (auto_start IN (0, 1)),
    name          TEXT CHECK (name IS NULL OR length(name) <= 255),
    sort_order    INTEGER NOT NULL DEFAULT 0,
    created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;

CREATE INDEX connection_forwards_connection ON connection_forwards (connection_id, sort_order);
