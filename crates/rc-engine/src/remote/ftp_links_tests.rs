//! FTP links in a panel (RC-080-46), against rc-remote's in-process FTP server: by default a
//! listing shows links unresolved and Enter looks one up - a link to a folder is entered and
//! shows as a folder from then on, F5 copies a link to a file; with
//! `ftp.resolve_links_in_listing` on, the listing shows it as a folder at once.

use std::path::PathBuf;

use rc_remote::ftp::test_server::{Options, TestServer};
use rc_vfs::Entry;
use serde_json::json;

use super::panel_tests::{add_ftp, copy, open_in};
use super::*;
use crate::{
    Command, CursorTarget, PanelId, remote::sftp::tests::harness, settings::FTP_RESOLVE_LINKS_KEY,
};

/// A server whose login starts in a folder with a folder, a file and a link to each.
async fn server_with_links() -> TestServer {
    let server = TestServer::start(Options {
        start: "/home/tester",
        ..Options::default()
    })
    .await;
    let home = server.local("home/tester");
    std::fs::create_dir_all(home.join("real")).expect("folder");
    std::fs::write(home.join("real/inside.txt"), b"in").expect("file");
    std::fs::write(home.join("file.txt"), b"data").expect("file");
    std::os::unix::fs::symlink("real", home.join("to_dir")).expect("link");
    std::os::unix::fs::symlink("file.txt", home.join("to_file")).expect("link");
    server
}

/// The entry `name` of the right panel once its listing is complete: links a listing resolves
/// arrive in a batch of their own after the transfer, possibly after the panel showed the rest.
async fn entry(engine: &Engine, name: &str) -> Entry {
    for _ in 0..500 {
        let panel = engine.panel(PanelId::Right);
        if panel.listing.is_none()
            && let Some(found) = panel.entries.iter().find(|entry| entry.name == name)
        {
            return found.clone();
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("{name} listed")
}

#[tokio::test]
async fn by_default_enter_looks_a_link_up_and_the_panel_shows_it_after() {
    let base = harness().await;
    let engine = &base.engine;
    let server = server_with_links().await;
    let id = add_ftp(engine, &server, ConnectionKind::Ftp, "links").await;
    open_in(engine, PanelId::Right, id).await.expect("opens");
    assert!(
        !entry(engine, "to_dir").await.is_dir_like(),
        "unresolved at first"
    );

    engine
        .dispatch(Command::SetCursor {
            panel: Some(PanelId::Right),
            target: CursorTarget::Name("to_dir".to_owned()),
        })
        .await
        .expect("cursor");
    engine
        .dispatch(Command::OpenFile {
            panel: Some(PanelId::Right),
            path: None,
        })
        .await
        .expect("entered");
    assert_eq!(
        engine.panel(PanelId::Right).path.path,
        PathBuf::from("/home/tester/to_dir")
    );
    engine
        .dispatch(Command::Navigate {
            panel: Some(PanelId::Right),
            path: "..".to_owned(),
        })
        .await
        .expect("back");
    assert!(entry(engine, "to_dir").await.is_dir_like(), "learned");

    let done = copy(engine, PanelId::Right, "to_file").await;
    assert_eq!(done.state, rc_ops::JobState::Completed, "{done:?}");
    let local = engine.panel(PanelId::Left).path.path;
    assert_eq!(
        std::fs::read(local.join("to_file")).expect("copied"),
        b"data"
    );
}

#[tokio::test]
async fn with_the_setting_on_the_listing_shows_a_link_to_a_folder_at_once() {
    let base = harness().await;
    let engine = &base.engine;
    engine
        .dispatch(Command::SetSetting {
            key: FTP_RESOLVE_LINKS_KEY.into(),
            value: json!(true),
        })
        .await
        .expect("set");
    let server = server_with_links().await;
    let id = add_ftp(engine, &server, ConnectionKind::Ftp, "links").await;
    open_in(engine, PanelId::Right, id).await.expect("opens");
    let session = engine
        .inner
        .remotes
        .ftp()
        .get(&id.0)
        .cloned()
        .expect("kept");
    assert!(
        session.resolves_links(),
        "the setting reached the connection"
    );
    assert!(entry(engine, "to_dir").await.is_dir_like());
    assert!(!entry(engine, "to_file").await.is_dir_like());
}
