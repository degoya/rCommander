//! The all-folder-sizes case of the measurements (RC-110-63, `scripts/perf.sh --cases`):
//! Alt+Shift+Enter (`panel.count_dir_content`) in a panel on a folder of many small folders,
//! while a reader takes every event the way the window does - each panel change reads the
//! panel's state again, which holds a copy of the counted sizes as the window's does.
//!
//! `perf dir-sizes <work folder> <folders>` makes `<work folder>` (it must not exist) with
//! `<folders>` folders of one file of 0 to 6 bytes each, then prints `dir_sizes_answer` (the
//! command answers) and `dir_sizes_all` (every folder counted) in milliseconds,
//! `dir_sizes_events` (the panel's listing changes until then), `dir_sizes_lagged` (the events
//! the reader missed - each a full reload in the window), `dir_sizes_folders` and the resident
//! memory peak. A count whose sum is not what the folders hold is an error.

use std::{error::Error, path::Path, time::Duration};

use rc_engine::{Command, DirSize, Engine, EngineEvent, PanelChange, PanelId};
use rc_vfs::VfsPath;
use tokio::sync::broadcast::error::RecvError;

use super::{LEFT, memory_kib, timed};

/// How long the reader waits for an event before it looks at the panel anyway.
const LOOK_EVERY: Duration = Duration::from_millis(500);
/// How long the count may take at most.
const GIVE_UP: Duration = Duration::from_secs(600);

pub(super) async fn run(dir: &Path, folders: usize) -> Result<(), Box<dyn Error>> {
    let expected = make(dir, folders)?;
    let engine = Engine::open(VfsPath::local(dir), VfsPath::local(dir)).await?;
    let mut events = engine.subscribe();
    let started = std::time::Instant::now();
    timed(
        "dir_sizes_answer",
        engine.dispatch(Command::CountDirContent { panel: LEFT }),
    )
    .await?;
    let (mut shown, mut lagged) = (0_u64, 0_u64);
    let total = loop {
        if let Some(total) = counted(&engine, folders) {
            break total;
        }
        if started.elapsed() > GIVE_UP {
            return Err("the count did not end".into());
        }
        match tokio::time::timeout(LOOK_EVERY, events.recv()).await {
            Ok(Ok(EngineEvent::Panel {
                panel: PanelId::Left,
                change: PanelChange::Listing,
            })) => shown += 1,
            Ok(Ok(_)) | Err(_) => {}
            Ok(Err(RecvError::Lagged(missed))) => lagged += missed,
            Ok(Err(RecvError::Closed)) => return Err("the engine's events ended".into()),
        }
    };
    println!(
        "dir_sizes_all\t{:.1}",
        started.elapsed().as_secs_f64() * 1000.0
    );
    if total != expected {
        return Err(format!("counted {total} bytes, the folders hold {expected}").into());
    }
    println!("dir_sizes_events\t{shown}");
    println!("dir_sizes_lagged\t{lagged}");
    println!("dir_sizes_folders\t{folders}");
    if let Some(peak) = memory_kib("VmHWM") {
        println!("rss_peak_kib\t{peak}");
    }
    Ok(())
}

/// The bytes of every counted folder once all `folders` are counted; `None` before.
fn counted(engine: &Engine, folders: usize) -> Option<u64> {
    let sizes = engine.panel(PanelId::Left).dir_sizes;
    if sizes.len() < folders {
        return None;
    }
    sizes.values().try_fold(0_u64, |sum, size| match size {
        DirSize::Bytes(bytes) => Some(sum + bytes),
        DirSize::Counting => None,
    })
}

/// `folders` folders below `dir`, which must not exist, each with one file of 0 to 6 bytes;
/// the bytes of all of them.
fn make(dir: &Path, folders: usize) -> std::io::Result<u64> {
    std::fs::create_dir(dir)?;
    let mut total = 0_u64;
    for index in 0..folders {
        let folder = dir.join(format!("d{index:07}"));
        std::fs::create_dir(&folder)?;
        let bytes = index % 7;
        std::fs::write(folder.join("f.dat"), vec![b'x'; bytes])?;
        total += bytes as u64;
    }
    Ok(total)
}
