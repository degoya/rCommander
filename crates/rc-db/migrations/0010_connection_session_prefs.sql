-- How the SSH sessions of a connection are set up and kept (RC-060-01): the order of the
-- authentication methods ("agent,key,password", the engine checks it), the one agent key to offer
-- (its SHA256 fingerprint, NULL for every key), the keepalive interval and the idle time after
-- which a session without channels is closed (0: never). One row per connection at most, and only
-- once a person changed something: a connection without a row takes the defaults. The row goes
-- with its connection.

CREATE TABLE connection_session_prefs (
    connection_id     INTEGER PRIMARY KEY REFERENCES connections (id) ON DELETE CASCADE,
    auth_order        TEXT NOT NULL CHECK (length(auth_order) > 0),
    agent_fingerprint TEXT,
    keepalive_secs    INTEGER NOT NULL CHECK (keepalive_secs > 0),
    idle_close_secs   INTEGER NOT NULL CHECK (idle_close_secs >= 0),
    updated_at        TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;
