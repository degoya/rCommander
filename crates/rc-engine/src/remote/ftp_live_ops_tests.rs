//! The operations queue against real FTP servers, once per server (`ftp_live_tests.rs` has the
//! set-up). The slow instance (`RC_LIVE_FTP_SLOW_PORT`, about 2 MB/s) keeps a transfer running
//! long enough to look at it, cancel it or drop its connection (`RC_LIVE_FTP_DROP`); the
//! instance without `REST` (`RC_LIVE_FTP_NOREST_PORT`) shows a resume that has to start over.

use std::{
    path::Path,
    time::{Duration, SystemTime},
};

use rc_ops::JobState;
use tokio::time::{Instant, sleep};

use super::live_tests::{Live, completed, content, digest, live_on, port_var, read_remote, var};
use crate::{Command, PanelId};

/// The slow instance, or the test is not run on this server.
async fn slow() -> Option<Live> {
    let Some(port) = port_var("RC_LIVE_FTP_SLOW_PORT") else {
        println!("no slow instance on this server: not testable here");
        return None;
    };
    let live = live_on(Some(port), None, true).await;
    live.open(PanelId::Right).await.expect("the panel opens");
    Some(live)
}

/// Waits until `check` holds, at most `limit`.
async fn until<F: AsyncFnMut() -> bool>(limit: Duration, mut check: F) -> bool {
    let end = Instant::now() + limit;
    while Instant::now() < end {
        if check().await {
            return true;
        }
        sleep(Duration::from_millis(100)).await;
    }
    false
}

/// Drops every FTP session of the server, as a broken link would.
fn drop_connections() {
    let drop = var("RC_LIVE_FTP_DROP");
    // console-window: allow (a test helper on Linux, in the test's container)
    let status = std::process::Command::new(&drop)
        .status()
        .expect("the drop helper runs");
    assert!(status.success(), "{drop} failed");
}

async fn exists(live: &Live, path: &str) -> bool {
    live.vfs().await.stat(Path::new(path)).await.is_ok()
}

#[tokio::test]
#[ignore = "live: scripts/live/ftp/run.sh"]
async fn live_ftp_ops_upload_writes_rcpart_and_cancel_leaves_nothing() {
    let Some(live) = slow().await else { return };
    let home = live.env.home.clone();
    let name = format!("cancel-{}.bin", std::process::id());
    std::fs::write(live.local().join(&name), content(16 * 1024 * 1024)).expect("local file");
    let job = live
        .submit(PanelId::Left, &[&name], &live.location(&home), false)
        .await;
    let part = format!("{home}/{name}.rcpart");
    let target = format!("{home}/{name}");
    assert!(
        until(Duration::from_secs(10), async || exists(&live, &part).await).await,
        "the upload writes {part}"
    );
    assert!(
        !exists(&live, &target).await,
        "no target before it is complete"
    );
    live.engine().ops().cancel(job).expect("cancelled");
    let done = live.engine().ops().wait(job).await.expect("known");
    assert_eq!(done.state, JobState::Cancelled, "{done:?}");
    assert!(!exists(&live, &part).await, "the part file is removed");
    assert!(!exists(&live, &target).await, "no half file");
}

#[tokio::test]
#[ignore = "live: scripts/live/ftp/run.sh"]
async fn live_ftp_ops_move_removes_the_source_only_when_complete() {
    let Some(live) = slow().await else { return };
    let home = live.env.home.clone();
    let name = format!("move-{}.bin", std::process::id());
    let source = live.local().join(&name);
    let data = content(6 * 1024 * 1024);
    std::fs::write(&source, &data).expect("local file");
    let job = live
        .submit(PanelId::Left, &[&name], &live.location(&home), true)
        .await;
    let part = format!("{home}/{name}.rcpart");
    assert!(
        until(Duration::from_secs(10), async || exists(&live, &part).await).await,
        "the move is under way"
    );
    assert!(source.exists(), "the source stays while the upload runs");
    let done = live.engine().ops().wait(job).await.expect("known");
    completed(&done);
    assert!(
        !source.exists(),
        "the source is gone once the target is complete"
    );
    let moved = read_remote(&live.vfs().await, &format!("{home}/{name}")).await;
    assert!(moved == data, "moved byte for byte");
}

#[tokio::test]
#[ignore = "live: scripts/live/ftp/run.sh"]
async fn live_ftp_ops_keeps_timestamps_where_the_server_can_set_them() {
    let live = live_on(None, None, true).await;
    live.open(PanelId::Right).await.expect("the panel opens");
    let home = live.env.home.clone();
    let name = format!("stamp-{}.txt", std::process::id());
    let local = live.local().join(&name);
    std::fs::write(&local, b"stamped").expect("local file");
    let stamp = SystemTime::UNIX_EPOCH + Duration::from_secs(1_577_934_245); // 2020-01-02 03:04:05
    std::fs::File::options()
        .write(true)
        .open(&local)
        .and_then(|file| file.set_modified(stamp))
        .expect("stamp set");
    completed(
        &live
            .transfer(PanelId::Left, &[&name], &live.location(&home), false)
            .await,
    );
    let vfs = live.vfs().await;
    let entry = vfs
        .stat(Path::new(&format!("{home}/{name}")))
        .await
        .expect("uploaded");
    if vfs.capabilities().set_mtime {
        assert_eq!(entry.modified, Some(stamp), "MFMT kept the time");
        println!("MFMT offered: the upload kept its time");
    } else {
        println!(
            "no MFMT on this server: upload time is the server's ({:?})",
            entry.modified
        );
    }
    let back = live.local().join("back");
    std::fs::create_dir_all(&back).expect("folder");
    completed(
        &live
            .transfer(PanelId::Right, &[&name], &back.display().to_string(), false)
            .await,
    );
    let downloaded = std::fs::metadata(back.join(&name)).expect("downloaded");
    let expected = entry.modified.expect("the server tells the time");
    let got = downloaded.modified().expect("local time");
    let skew = got
        .duration_since(expected)
        .unwrap_or_else(|error| error.duration());
    assert!(
        skew < Duration::from_secs(1),
        "download kept the server's time: {got:?} vs {expected:?}"
    );
}

#[tokio::test]
#[ignore = "live: scripts/live/ftp/run.sh"]
async fn live_ftp_ops_disconnect_then_the_next_step_logs_in_again() {
    let live = live_on(None, None, true).await;
    live.open(PanelId::Right).await.expect("the panel opens");
    live.engine()
        .dispatch(Command::DisconnectSession { id: live.id })
        .await
        .expect("disconnected");
    assert!(
        live.engine().inner.remotes.ftp().is_empty(),
        "the session is gone"
    );
    // Ctrl+Shift+F leaves the server (RC-070-03, wave 11): the panel shows a local folder.
    assert_eq!(
        live.engine().panel(PanelId::Right).path.backend,
        rc_vfs::BackendId::Local,
        "the panel left the server"
    );
    live.open(PanelId::Right)
        .await
        .expect("opened again after a new login");
    assert!(
        !live.engine().inner.remotes.ftp().is_empty(),
        "a new session"
    );
}

#[tokio::test]
#[ignore = "live: scripts/live/ftp/run.sh"]
async fn live_ftp_ops_a_third_download_waits_for_the_transfer_limit() {
    let live = live_on(None, None, true).await;
    live.open(PanelId::Right).await.expect("the panel opens");
    let home = live.env.home.clone();
    let vfs = live.vfs().await;
    let mut held = Vec::new();
    for index in 0..3 {
        let path = format!("{home}/limit-{index}.bin");
        let mut writer = vfs
            .open_write(Path::new(&path), rc_vfs::WriteMode::Truncate)
            .await
            .expect("upload");
        tokio::io::AsyncWriteExt::write_all(&mut writer, &content(64 * 1024))
            .await
            .expect("written");
        tokio::io::AsyncWriteExt::shutdown(&mut writer)
            .await
            .expect("complete");
    }
    for index in 0..2 {
        let path = format!("{home}/limit-{index}.bin");
        held.push(vfs.open_read(Path::new(&path)).await.expect("a download"));
    }
    let third = format!("{home}/limit-2.bin");
    let waiting =
        tokio::time::timeout(Duration::from_secs(2), vfs.open_read(Path::new(&third))).await;
    assert!(waiting.is_err(), "a third download waits while two run");
    drop(held.pop());
    let started = tokio::time::timeout(Duration::from_secs(10), vfs.open_read(Path::new(&third)))
        .await
        .expect("starts once one ended");
    started.expect("the third download");
}

/// A download over the slow instance on `port` whose connections drop part-way; it must end
/// complete and byte for byte, `resumed` or started over.
async fn download_through_a_drop(port: u16) {
    let live = live_on(Some(port), None, true).await;
    live.open(PanelId::Right).await.expect("the panel opens");
    let home = live.env.home.clone();
    let data = content(10 * 1024 * 1024);
    let name = format!("drop-{}.bin", std::process::id());
    let upload = live.local().join("up");
    std::fs::create_dir_all(&upload).expect("folder");
    std::fs::write(upload.join(&name), &data).expect("local file");
    // Uploaded through the same slow instance, dropped half-way too: APPE continues it.
    let job = live
        .submit(
            PanelId::Left,
            &[&format!("up/{name}")],
            &live.location(&home),
            false,
        )
        .await;
    sleep(Duration::from_secs(2)).await;
    drop_connections();
    completed(&live.engine().ops().wait(job).await.expect("known"));
    let uploaded = read_remote(&live.vfs().await, &format!("{home}/{name}")).await;
    assert_eq!(digest(&uploaded), digest(&data), "upload through a drop");

    let back = live.local().join("back");
    std::fs::create_dir_all(&back).expect("folder");
    let job = live
        .submit(PanelId::Right, &[&name], &back.display().to_string(), false)
        .await;
    sleep(Duration::from_secs(2)).await;
    drop_connections();
    completed(&live.engine().ops().wait(job).await.expect("known"));
    let downloaded = std::fs::read(back.join(&name)).expect("downloaded");
    assert!(downloaded == data, "download through a drop, byte for byte");
}

#[tokio::test]
#[ignore = "live: scripts/live/ftp/run.sh"]
async fn live_ftp_ops_resume_after_a_dropped_connection() {
    let Some(port) = port_var("RC_LIVE_FTP_SLOW_PORT") else {
        println!("no slow instance on this server: not testable here");
        return;
    };
    download_through_a_drop(port).await;
}

#[tokio::test]
#[ignore = "live: scripts/live/ftp/run.sh"]
async fn live_ftp_ops_a_server_without_rest_starts_over() {
    let Some(port) = port_var("RC_LIVE_FTP_NOREST_PORT") else {
        println!("no instance without REST on this server: not testable here");
        return;
    };
    download_through_a_drop(port).await;
}

/// The server drops the panel's idle connection (a restart, a firewall's idle cut): the next
/// step on the panel works, on a new login.
#[tokio::test]
#[ignore = "live: scripts/live/ftp/run.sh"]
async fn live_ftp_ops_a_dropped_idle_connection_is_replaced() {
    let live = live_on(None, None, true).await;
    live.open(PanelId::Right).await.expect("the panel opens");
    drop_connections();
    sleep(Duration::from_millis(500)).await;
    live.engine()
        .dispatch(Command::Refresh {
            panel: Some(PanelId::Right),
        })
        .await
        .expect("the refresh after a dropped idle connection");
}
