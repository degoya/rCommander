//! `mcp.ask.outside_panels`: with it on, a reading or writing tool asks for a path outside the
//! panels' folders - also through `..` or a link - and runs unasked inside them; off, nothing
//! changes. Headless and without codes, "asks" answers `headless_ask_denied`.

use rc_engine::settings::MCP_ASK_OUTSIDE_PANELS_KEY;
use serde_json::{Value, json};

use crate::test_support::{Fixture, call};

#[tokio::test]
async fn outside_the_panels_a_read_asks_first() {
    let outside = tempfile::tempdir().expect("outside");
    let secret = outside.path().join("token.txt");
    std::fs::write(&secret, "hunter2").expect("secret");

    let fixture = Fixture::new();
    std::fs::write(fixture.root().join("notes.txt"), "inside").expect("notes");
    #[cfg(unix)]
    std::os::unix::fs::symlink(outside.path(), fixture.root().join("away")).expect("link");
    let built = fixture.build().await;
    let client = built.client().await;
    let read = |path: String| {
        let client = &client;
        async move { call(client, "files_read_text", json!({ "path": path })).await }
    };

    // Off by default: as before.
    let unasked = read(secret.display().to_string()).await;
    assert_eq!(unasked["text"], "hunter2", "{unasked}");

    built
        .db
        .set(MCP_ASK_OUTSIDE_PANELS_KEY, &true)
        .await
        .expect("switched on");
    let inside = read("notes.txt".to_owned()).await;
    assert_eq!(inside["text"], "inside", "{inside}");
    let mut asked: Vec<Value> = vec![
        read(secret.display().to_string()).await,
        read(format!("../{}/token.txt", outside.path().display())).await,
    ];
    if cfg!(unix) {
        asked.push(read("away/token.txt".to_owned()).await);
    }
    for answer in asked {
        assert_eq!(answer["code"], "headless_ask_denied", "{answer}");
    }
}
