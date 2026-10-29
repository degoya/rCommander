//! The engine's part of the repeatable measurements (RC-080-04, `scripts/perf.sh`): a folder of
//! many entries listed, sorted, quick-searched, filtered and selected through the engine's
//! commands - what the window does, without the window.
//!
//! `cargo run --release -p rc-engine --example perf -- <folder> [<database>]` prints one line per
//! step, `<step>\t<milliseconds>`, then the number of entries and, on Linux, the resident memory
//! (`/proc/self/status`, in KiB): idle, after the listing, and its peak. The script makes the
//! folder and reads the lines.
//!
//! `start` is the window's start without the window (`app/startup.rs`): from `main` until the
//! first panel's listing is there - the database opened (created and migrated when `<database>`
//! is new) and attached, the engine opened on the folder's parent, the stored settings and the
//! saved transfers read back. The idle memory is taken right after it, before the large folder
//! is listed.
//!
//! The cases of RC-080-46 (`scripts/perf.sh --cases`): `perf archive-pack <work folder> <files>`
//! packs a solid 7z and a `tar.xz` of many small files and `perf archive-unpack <work folder>
//! <format> <files>` unpacks one of them, each in a process of its own (`perf_cases/archive.rs`);
//! `perf ftp-viewer <MiB>` reads a large file on the in-process FTP server as the viewer does
//! (`perf_cases/ftp_viewer.rs`).

use std::{
    error::Error,
    path::{Path, PathBuf},
    time::Instant,
};

use rc_db::Db;
use rc_engine::{
    Command, CursorStep, Engine, MaskRequest, PanelId, SortKey,
    quick_search::{MatchOptions, NameMatch, Seek, find},
};
use rc_vfs::VfsPath;

#[path = "perf_cases/archive.rs"]
mod archive;
#[path = "perf_cases/ftp_viewer.rs"]
mod ftp_viewer;

const LEFT: Option<PanelId> = Some(PanelId::Left);

/// Runs `step` and prints how long it took.
async fn timed<T, F>(name: &str, step: F) -> T
where
    F: std::future::Future<Output = T>,
{
    let started = Instant::now();
    let answer = step.await;
    println!("{name}\t{:.1}", started.elapsed().as_secs_f64() * 1000.0);
    answer
}

/// A quick search for `text` over the left panel's entries, from the top, as the window runs it.
fn quick_search(engine: &Engine, name: &str, text: &str) {
    let entries = engine.panel(PanelId::Left).entries;
    let started = Instant::now();
    let found = NameMatch::quick_search(text, MatchOptions::default())
        .and_then(|matcher| find(&entries, &matcher, 0, Seek::Here));
    let took = started.elapsed().as_secs_f64() * 1000.0;
    println!("{name}\t{took:.1}\t{}", found.map_or(-1, |at| at as i64));
}

/// What `/proc/self/status` says of `field` (`VmRSS`, `VmHWM`), in KiB; `None` elsewhere.
fn memory_kib(field: &str) -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status
        .lines()
        .find_map(|line| line.strip_prefix(field)?.strip_prefix(':'))
        .and_then(|value| value.trim().trim_end_matches("kB").trim().parse().ok())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let started = Instant::now();
    let mut args = std::env::args_os().skip(1).map(PathBuf::from);
    let folder = args.next().ok_or("usage: perf <folder> [<database>]")?;
    let number = |arg: Option<PathBuf>| -> Result<u64, Box<dyn Error>> {
        Ok(arg
            .ok_or("a number is missing")?
            .to_string_lossy()
            .parse()?)
    };
    if folder.as_os_str() == "archive-pack" {
        let dir = args
            .next()
            .ok_or("usage: perf archive-pack <work folder> <files>")?;
        let files = usize::try_from(number(args.next())?)?;
        return archive::pack(&dir, files).await;
    }
    if folder.as_os_str() == "archive-unpack" {
        let usage = "usage: perf archive-unpack <work folder> <format> <files>";
        let dir = args.next().ok_or(usage)?;
        let format = args.next().ok_or(usage)?;
        let files = usize::try_from(number(args.next())?)?;
        return archive::unpack(&dir, &format.to_string_lossy(), files).await;
    }
    if folder.as_os_str() == "ftp-viewer" {
        return ftp_viewer::run(number(args.next())?).await;
    }
    let database = args.next();
    let db = match &database {
        Some(path) => Some(Db::open(path).await?),
        None => None,
    };
    let start = folder.parent().unwrap_or(Path::new("/")).to_path_buf();
    let engine = timed(
        "open",
        Engine::open(VfsPath::local(&start), VfsPath::local(&start)),
    )
    .await?;
    if let Some(db) = db {
        engine.attach_db(db);
        engine.restore_settings().await;
        engine.restore_transfers().await?;
    }
    println!("start\t{:.1}", started.elapsed().as_secs_f64() * 1000.0);
    let idle_rss = memory_kib("VmRSS");
    let path = folder.display().to_string();
    timed(
        "list",
        engine.dispatch(Command::Navigate { panel: LEFT, path }),
    )
    .await?;
    let entries = engine.panel(PanelId::Left).entries.len();
    let listed_rss = memory_kib("VmRSS");
    for (name, key) in [
        ("sort_size", SortKey::Size),
        ("sort_date", SortKey::Date),
        ("sort_ext", SortKey::Ext),
        ("sort_name", SortKey::Name),
    ] {
        let sort = Command::Sort {
            panel: LEFT,
            key,
            descending: None,
        };
        timed(name, engine.dispatch(sort)).await?;
    }
    // A name near the end, and one that is not there: the whole list is read.
    let last = format!("f{:07}", entries.saturating_sub(1));
    quick_search(&engine, "quick_search_last", &last);
    quick_search(&engine, "quick_search_missing", "no-such-name");
    let filter = Command::Filter {
        panel: LEFT,
        pattern: Some("*5.dat".to_owned()),
        last: false,
    };
    timed("filter", engine.dispatch(filter)).await?;
    let all = Command::Filter {
        panel: LEFT,
        pattern: None,
        last: false,
    };
    timed("filter_off", engine.dispatch(all)).await?;
    let select = Command::SelectByMask(MaskRequest {
        panel: LEFT,
        mask: Some("*.*".to_owned()),
    });
    timed("select_all_mask", engine.dispatch(select)).await?;
    // With every entry selected (RC-080-45): a cursor step answers the selection without a
    // copy, a refresh keeps it through one index, a toggle of one name copies nothing shared.
    let step = Command::MoveCursor {
        panel: LEFT,
        step: CursorStep::Down,
    };
    timed("cursor_step_selected", engine.dispatch(step)).await?;
    let refresh = Command::Refresh { panel: LEFT };
    timed("refresh_selected", engine.dispatch(refresh)).await?;
    // The first entry `scripts/lib/perf.py` makes.
    let toggle = Command::ToggleSelection {
        panel: LEFT,
        names: vec!["f0000000.dat".to_owned()],
    };
    timed("toggle_selected", engine.dispatch(toggle)).await?;
    timed(
        "deselect_all",
        engine.dispatch(Command::DeselectAll { panel: LEFT }),
    )
    .await?;
    timed("refresh", engine.dispatch(Command::Refresh { panel: LEFT })).await?;
    println!("entries\t{entries}");
    if let Some(rss) = idle_rss {
        println!("rss_idle_kib\t{rss}");
    }
    if let Some(rss) = listed_rss {
        println!("rss_after_list_kib\t{rss}");
    }
    if let Some(peak) = memory_kib("VmHWM") {
        println!("rss_peak_kib\t{peak}");
    }
    Ok(())
}
