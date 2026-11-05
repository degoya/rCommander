-- The MCP audit log (RC-080-66): one row per tool call the gate decided - when, the MCP session,
-- the tool, its class, the gate's decision (allowed, asked, confirmed, declined, denied, refused,
-- unknown), the answer's code (`ok` or its error code) and the arguments, masked as every answer
-- is and cut to 2048 bytes; never an answer. Written only while `mcp.audit_log` is on, read only
-- by the settings dialog - no MCP tool reads it. Grows with use, so it is in
-- `rc_db::RETAINED_TABLES`, cut by the shared sweep by rows and age; `id` is the insertion order
-- the sweep deletes by.
CREATE TABLE mcp_audit_log (
    id          INTEGER PRIMARY KEY,
    recorded_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    session     TEXT NOT NULL CHECK (length(session) <= 128),
    tool        TEXT NOT NULL CHECK (length(tool) <= 128),
    class       TEXT CHECK (class IS NULL OR length(class) <= 32),
    decision    TEXT NOT NULL CHECK (length(decision) <= 32),
    code        TEXT NOT NULL CHECK (length(code) <= 128),
    arguments   TEXT NOT NULL CHECK (length(arguments) <= 2048)
) STRICT;

CREATE INDEX mcp_audit_log_recorded_at ON mcp_audit_log (recorded_at);
CREATE INDEX mcp_audit_log_session ON mcp_audit_log (session, id);
