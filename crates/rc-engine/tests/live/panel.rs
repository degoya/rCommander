//! The SFTP panel: the drive bar opens the home folder with its rights, the panel navigates, F7
//! makes a folder, F8 deletes it, F3 reads a file.

use std::path::Path;

use rc_engine::{
    Command, DeleteMode, DeleteRequest, ErrorPolicy, Outcome, PanelId, viewer::ReadTextRequest,
};

use crate::live::{completed, connect_trusting, connection, engine, finished, workdir};
use crate::live_or_skip;

fn panel(outcome: Result<Outcome, rc_engine::EngineError>) -> rc_engine::PanelSnapshot {
    match outcome {
        Ok(Outcome::Panel(panel)) => panel,
        other => panic!("a panel, got {other:?}"),
    }
}

fn job(outcome: Result<Outcome, rc_engine::EngineError>) -> rc_engine::JobId {
    match outcome {
        Ok(Outcome::Job(job)) => job.id,
        other => panic!("a job, got {other:?}"),
    }
}

#[tokio::test]
#[ignore = "needs the servers of scripts/live/sftp/run.sh"]
async fn the_panel_shows_the_home_navigates_makes_deletes_and_reads() {
    let live = live_or_skip!();
    let dir = workdir();
    let (engine, _db) = engine(dir.path()).await;
    let id = connection(&engine, &live, "live-a", "a").await;
    connect_trusting(&engine, id).await;
    let mine = live.remote_dir("a", "t-panel");

    let shown = panel(
        engine
            .dispatch(Command::ChangeDrive {
                panel: Some(PanelId::Right),
                drive: format!("server:{id}"),
            })
            .await,
    );
    assert_eq!(
        shown.path.path,
        Path::new("/home/tester"),
        "the home folder"
    );
    let listing = engine
        .dispatch(Command::ListDir {
            panel: Some(PanelId::Right),
            path: None,
        })
        .await;
    let Ok(Outcome::Listing { entries, .. }) = listing else {
        panic!("a listing, got {listing:?}");
    };
    let big = entries
        .iter()
        .find(|entry| entry.name == "big.bin")
        .expect("big.bin");
    assert_eq!(big.size, live.big);
    assert_eq!(big.attributes.mode, Some(0o644), "the rights column");
    let docs = entries
        .iter()
        .find(|entry| entry.name == "docs")
        .expect("docs");
    assert!(docs.is_dir_like());
    assert!(docs.attributes.mode.is_some());

    let inside = panel(
        engine
            .dispatch(Command::Navigate {
                panel: Some(PanelId::Right),
                path: "docs".to_owned(),
            })
            .await,
    );
    assert_eq!(inside.path.path, Path::new("/home/tester/docs"));
    let typed = panel(
        engine
            .dispatch(Command::Navigate {
                panel: Some(PanelId::Right),
                path: "sftp://live-a/srv".to_owned(),
            })
            .await,
    );
    assert_eq!(typed.path.path, Path::new("/srv"), "a typed sftp:// path");

    // F7 and F8 on the server's panel: paths on the panel's backend, as the window sends them
    // (`ops.mkdir`, `ops.delete`, `files.read_text` and `view.open` read no `sftp://` location).
    let mkdir = job(engine
        .dispatch(Command::OpsMkdir {
            panel: Some(PanelId::Right),
            path: format!("{mine}/made/deeper"),
        })
        .await);
    completed(&finished(&engine, mkdir, std::time::Duration::from_secs(30)).await);
    assert!(live.remote_exists("a", &format!("{mine}/made/deeper")));

    let delete = job(engine
        .dispatch(Command::OpsDelete(DeleteRequest {
            panel: Some(PanelId::Right),
            sources: vec![format!("{mine}/made")],
            mode: DeleteMode::Permanent,
            on_error: ErrorPolicy::Cancel,
            queue: None,
        }))
        .await);
    completed(&finished(&engine, delete, std::time::Duration::from_secs(30)).await);
    assert!(!live.remote_exists("a", &format!("{mine}/made")));

    let read = engine
        .dispatch(Command::ReadText(ReadTextRequest {
            path: "/home/tester/docs/readme.txt".to_owned(),
            panel: Some(PanelId::Right),
            offset: 0,
            max_bytes: None,
            encoding: None,
        }))
        .await;
    let Ok(Outcome::FileText(window)) = read else {
        panic!("text, got {read:?}");
    };
    let shown = format!("{window:?}");
    assert!(shown.contains("hello"), "{shown}");
    engine
        .dispatch(Command::ViewFile {
            panel: Some(PanelId::Right),
            path: Some("/home/tester/docs/readme.txt".to_owned()),
        })
        .await
        .expect("F3 opens the viewer");
}
