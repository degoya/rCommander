//! The FTP kinds in the `connections_*` tools and `files_stat`, end to end over an in-process
//! pipe like `connections_tests.rs`. The FTP login itself is tested in `rc-engine`
//! (`commands/remote_tests.rs`) and `rc-remote`.

use serde_json::{Value, json};
use tempfile::TempDir;

use crate::McpContext;
use crate::test_support::{Fixture, call, connect};

async fn setup() -> (TempDir, McpContext) {
    let fixture = Fixture::new();
    std::fs::write(fixture.root().join("notes.txt"), b"twelve bytes").expect("write");
    let built = fixture.build().await;
    (built.root, built.context)
}

/// A port nothing listens on.
async fn closed_port() -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    listener.local_addr().expect("address").port()
}

#[tokio::test]
async fn an_agent_adds_ftp_connections_and_reaches_them_by_name() {
    let (_root, context) = setup().await;
    let client = connect(&context).await;

    let implicit = call(
        &client,
        "connections_create",
        json!({ "name": "nas", "host": "127.0.0.1", "kind": "ftps_implicit", "auth_method": "anonymous",
                "ftp_data_mode": "active", "ftp_encoding": "utf8" }),
    )
    .await;
    assert_eq!(implicit["kind"], "ftps_implicit", "{implicit}");
    assert_eq!(implicit["port"], 990, "{implicit}");
    assert_eq!(implicit["ftp_data_mode"], "active", "{implicit}");
    assert_eq!(implicit["ftp_encoding"], "utf8", "{implicit}");

    let port = closed_port().await;
    let plain = call(
        &client,
        "connections_create",
        json!({ "name": "gone", "host": "127.0.0.1", "port": port, "kind": "ftp", "auth_method": "anonymous" }),
    )
    .await;
    assert_eq!(
        plain["ftp_data_mode"],
        Value::Null,
        "unset is passive: {plain}"
    );
    let unreachable = call(&client, "files_stat", json!({ "path": "ftp://gone/pub" })).await;
    assert_eq!(unreachable["code"], "ftp_connect_failed", "{unreachable}");

    // FTP with a password needs a user; SSH cannot log in anonymously.
    let no_user = call(
        &client,
        "connections_create",
        json!({ "name": "x", "host": "h", "kind": "ftp" }),
    )
    .await;
    assert_eq!(no_user["code"], "username_empty", "{no_user}");
    let anonymous_ssh = call(
        &client,
        "connections_create",
        json!({ "name": "y", "host": "h", "auth_method": "anonymous" }),
    )
    .await;
    assert_eq!(anonymous_ssh["code"], "auth_unsupported", "{anonymous_ssh}");
}

#[tokio::test]
async fn stat_answers_one_entry_and_names_unknown_connections() {
    let (_root, context) = setup().await;
    let client = connect(&context).await;
    let stat = call(&client, "files_stat", json!({ "path": "notes.txt" })).await;
    assert_eq!(stat["entry"]["name"], "notes.txt", "{stat}");
    assert_eq!(stat["entry"]["kind"], "file", "{stat}");
    assert_eq!(stat["entry"]["size"], 12, "{stat}");

    let unknown = call(
        &client,
        "files_stat",
        json!({ "path": "ftp://nowhere/pub" }),
    )
    .await;
    assert_eq!(unknown["code"], "ftp_connection_not_found", "{unknown}");
    let listing = call(&client, "list_dir", json!({ "path": "ftp://nowhere/" })).await;
    assert_eq!(listing["code"], "ftp_connection_not_found", "{listing}");
    let sftp = call(&client, "list_dir", json!({ "path": "sftp://nowhere/" })).await;
    assert_eq!(sftp["code"], "sftp_connection_not_found", "{sftp}");
    let sftp_stat = call(&client, "files_stat", json!({ "path": "sftp://nowhere/x" })).await;
    assert_eq!(
        sftp_stat["code"], "sftp_connection_not_found",
        "{sftp_stat}"
    );
}
