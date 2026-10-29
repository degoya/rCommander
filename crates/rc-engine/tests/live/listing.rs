//! A folder of 100 000 entries on a real server (RC-060-02): the panel shows the first of them
//! before the listing ends, and the rest follows in the panel's order. The times are printed for
//! the job file with the resident memory on Linux (RC-080-04 stage 2, `docs/performance.md`);
//! the `many` server is not disturbed.

use std::time::{Duration, Instant};

use rc_engine::{Command, Outcome, PanelId};

use crate::live::{connect_trusting, connection, engine, sftp, workdir};
use crate::live_or_skip;

/// The entries of `/home/tester/many` (ctl.sh): `entry-1` to `entry-100000`.
const MANY: usize = 100_000;

/// What `/proc/self/status` says of `field` (`VmRSS`, `VmHWM`), in MiB, or `-` where it says
/// nothing. nextest runs each test in a process of its own, so the peak is this test's.
fn memory_mib(field: &str) -> String {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status
                .lines()
                .find_map(|line| line.strip_prefix(field)?.strip_prefix(':'))
                .and_then(|value| {
                    value
                        .trim()
                        .trim_end_matches("kB")
                        .trim()
                        .parse::<f64>()
                        .ok()
                })
        })
        .map_or_else(|| "-".to_owned(), |kib| format!("{:.1}", kib / 1024.0))
}

#[tokio::test]
#[ignore = "needs the servers of scripts/live/sftp/run.sh"]
async fn a_folder_of_100_000_entries_shows_its_first_batch_before_the_end() {
    let live = live_or_skip!();
    let dir = workdir();
    let (engine, _db) = engine(dir.path()).await;
    let id = connection(&engine, &live, "live-many", "many").await;
    connect_trusting(&engine, id).await;
    let path = sftp("live-many", "/home/tester/many");
    let connected_rss = memory_mib("VmRSS");

    let started = Instant::now();
    let shown = engine
        .dispatch(Command::Navigate {
            panel: Some(PanelId::Left),
            path: path.clone(),
        })
        .await;
    let first = started.elapsed();
    let Ok(Outcome::Panel(shown)) = shown else {
        panic!("a panel, got {shown:?}");
    };
    assert!(shown.loading, "shown before the end: {shown:?}");
    assert!(
        shown.entry_count > 0 && shown.entry_count < MANY,
        "{}",
        shown.entry_count
    );
    while engine.panel(PanelId::Left).listing.is_some() {
        assert!(
            started.elapsed() < Duration::from_secs(120),
            "never complete"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let all = started.elapsed();
    let state = engine.panel(PanelId::Left);
    assert_eq!(state.entries.len(), MANY);
    let names: Vec<&str> = state
        .entries
        .iter()
        .map(|entry| entry.name.as_str())
        .collect();
    assert_eq!(names[0], "entry-1", "in the panel's order");
    assert_eq!(names[MANY - 1], "entry-100000", "in the panel's order");
    eprintln!(
        "100 000 entries over SFTP: first {} shown after {first:?}, all after {all:?}",
        shown.entry_count
    );
    // The row of `docs/performance.md`, "Over SFTP".
    eprintln!(
        "resident memory: {connected_rss} MiB connected, {} MiB after the listing, peak {} MiB",
        memory_mib("VmRSS"),
        memory_mib("VmHWM")
    );

    // `list_dir` (the MCP tool) reads the folder at once.
    let started = Instant::now();
    let listed = engine
        .dispatch(Command::ListDir {
            panel: None,
            path: Some(path),
        })
        .await;
    let Ok(Outcome::Listing { entries, .. }) = listed else {
        panic!("a listing, got {listed:?}");
    };
    assert_eq!(entries.len(), MANY);
    eprintln!(
        "list_dir of the same folder: {:?}, peak resident memory {} MiB",
        started.elapsed(),
        memory_mib("VmHWM")
    );
}
