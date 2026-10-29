//! The measurements of the find-files search (RC-020-03) and the folder sizes (RC-020-06) over a
//! large tree, through the engine and without the window (`docs/performance.md`).
//!
//! ```text
//! cargo run --release -p rc-engine --example find_size -- tree <root> <folders> <files-per-folder>
//! cargo run --release -p rc-engine --example find_size -- search <root>
//! cargo run --release -p rc-engine --example find_size -- size <root>
//! ```
//!
//! `tree` makes `<root>/d0000/f0000000.dat`, ...: the five extensions of `scripts/perf.sh` in
//! turn, 0 to 6 bytes each. `search` runs the dialog's search for `*.log` (a fifth of the files),
//! keeping at most 100 000 hits as the dialog's list does, then a search for every entry that is
//! cancelled after half a second. `size` runs Ctrl+L on the root, Alt+Shift+Enter in a panel on
//! the root, and Alt+Shift+Enter cancelled after a quarter of the full count's time. Each prints
//! `<step>\t<value>` lines: milliseconds, counts, and on Linux the resident memory in KiB
//! (`/proc/self/status`).

use std::{
    error::Error,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use rc_engine::{Command, DirSize, Engine, Outcome, PanelId, SearchEvent, SearchQuery};
use rc_vfs::VfsPath;

const LEFT: Option<PanelId> = Some(PanelId::Left);
const EXTENSIONS: [&str; 5] = ["dat", "txt", "log", "bin", "jpg"];
/// The dialog's hit list holds at most this many.
const HIT_LIST: usize = 100_000;

/// What `/proc/self/status` says of `field` (`VmRSS`, `VmHWM`), in KiB; `None` elsewhere.
fn memory_kib(field: &str) -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status
        .lines()
        .find_map(|line| line.strip_prefix(field)?.strip_prefix(':'))
        .and_then(|value| value.trim().trim_end_matches("kB").trim().parse().ok())
}

fn print_memory() {
    for (name, field) in [("rss_kib", "VmRSS"), ("rss_peak_kib", "VmHWM")] {
        if let Some(kib) = memory_kib(field) {
            println!("{name}\t{kib}");
        }
    }
}

fn ms(since: Instant) -> String {
    format!("{:.1}", since.elapsed().as_secs_f64() * 1000.0)
}

/// `folders` folders of `files` files each below `root`, which must not exist.
fn tree(root: &Path, folders: usize, files: usize) -> Result<(), Box<dyn Error>> {
    std::fs::create_dir_all(root.parent().ok_or("root has no parent")?)?;
    std::fs::create_dir(root)?;
    let mut index = 0usize;
    for folder in 0..folders {
        let dir = root.join(format!("d{folder:04}"));
        std::fs::create_dir(&dir)?;
        for _ in 0..files {
            let name = format!("f{index:07}.{}", EXTENSIONS[index % EXTENSIONS.len()]);
            std::fs::write(dir.join(name), &b"xxxxxx"[..index % 7])?;
            index += 1;
        }
    }
    println!("files\t{index}");
    Ok(())
}

async fn engine_on(root: &Path) -> Result<Engine, Box<dyn Error>> {
    let start = VfsPath::local(root);
    Ok(Engine::open(start.clone(), start).await?)
}

/// The search as the dialog runs it, and one cancelled half way.
async fn search(root: &Path) -> Result<(), Box<dyn Error>> {
    let engine = engine_on(root).await?;
    let query = SearchQuery {
        name: "*.log".to_owned(),
        ..SearchQuery::default()
    };
    let started = Instant::now();
    let mut handle = engine.start_search(LEFT, &[], &query).await?;
    let mut hits = Vec::new();
    let mut first_hit = None;
    let mut last_event = Instant::now();
    let mut longest_gap = Duration::ZERO;
    while let Some(event) = handle.next().await {
        longest_gap = longest_gap.max(last_event.elapsed());
        last_event = Instant::now();
        match event {
            SearchEvent::Hit(hit) => {
                first_hit.get_or_insert_with(|| ms(started));
                if hits.len() < HIT_LIST {
                    hits.push(hit);
                }
            }
            SearchEvent::Finished { stats, cancelled } => {
                println!("search_ms\t{}", ms(started));
                println!(
                    "search_first_hit_ms\t{}",
                    first_hit.as_deref().unwrap_or("-")
                );
                println!("search_longest_gap_ms\t{}", longest_gap.as_millis());
                println!("search_dirs\t{}", stats.dirs);
                println!("search_entries\t{}", stats.entries);
                println!("search_hits\t{}", stats.hits);
                println!("search_kept\t{}", hits.len());
                println!("search_cancelled\t{cancelled}");
            }
            SearchEvent::Progress { .. } | SearchEvent::Skipped { .. } => {}
        }
    }
    print_memory();
    drop(hits);

    let mut handle = engine
        .start_search(LEFT, &[], &SearchQuery::default())
        .await?;
    let started = Instant::now();
    let mut cancelled_at = None;
    while let Some(event) = handle.next().await {
        if cancelled_at.is_none() && started.elapsed() >= Duration::from_millis(500) {
            handle.cancel();
            cancelled_at = Some(Instant::now());
        }
        if let SearchEvent::Finished { stats, cancelled } = event {
            let after = cancelled_at.map_or_else(|| "-".to_owned(), ms);
            println!("cancel_finished_after_ms\t{after}");
            println!("cancel_entries_seen\t{}", stats.entries);
            println!("cancel_cancelled\t{cancelled}");
        }
    }
    Ok(())
}

/// How many of the panel's folders are counted, and how many are being counted.
fn counted(engine: &Engine) -> (usize, usize) {
    let sizes = engine.panel(PanelId::Left).dir_sizes;
    let done = sizes
        .values()
        .filter(|size| matches!(size, DirSize::Bytes(_)))
        .count();
    (done, sizes.len() - done)
}

/// Ctrl+L, Alt+Shift+Enter, and Alt+Shift+Enter cancelled.
async fn size(root: &Path) -> Result<(), Box<dyn Error>> {
    let engine = engine_on(root).await?;
    let paths = vec![root.display().to_string()];
    let started = Instant::now();
    let outcome = engine
        .dispatch(Command::DirSize { panel: LEFT, paths })
        .await?;
    println!("dir_size_ms\t{}", ms(started));
    if let Outcome::DirSizes { total, .. } = outcome {
        println!("dir_size_files\t{}", total.files);
        println!("dir_size_dirs\t{}", total.dirs);
        println!("dir_size_bytes\t{}", total.bytes);
    }

    let folders = engine
        .panel(PanelId::Left)
        .entries
        .iter()
        .filter(|entry| entry.kind == rc_vfs::EntryKind::Dir)
        .count();
    let started = Instant::now();
    engine
        .dispatch(Command::CountDirContent { panel: LEFT })
        .await?;
    println!("count_all_answer_ms\t{}", ms(started));
    while counted(&engine).0 < folders {
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    let full = started.elapsed();
    println!("count_all_ms\t{}", ms(started));
    println!("count_all_folders\t{folders}");

    engine.dispatch(Command::Refresh { panel: LEFT }).await?;
    engine
        .dispatch(Command::CountDirContent { panel: LEFT })
        .await?;
    tokio::time::sleep(full / 4).await;
    let started = Instant::now();
    engine
        .dispatch(Command::CancelDirSizes { panel: LEFT })
        .await?;
    println!("count_cancel_answer_ms\t{}", ms(started));
    let at_cancel = counted(&engine);
    tokio::time::sleep(Duration::from_millis(500)).await;
    let later = counted(&engine);
    println!("count_cancel_done_at_cancel\t{}", at_cancel.0);
    println!("count_cancel_counting_at_cancel\t{}", at_cancel.1);
    println!("count_cancel_done_500ms_later\t{}", later.0);
    println!("count_cancel_counting_500ms_later\t{}", later.1);
    print_memory();
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let usage = "usage: find_size tree <root> <folders> <files> | search <root> | size <root>";
    let root = PathBuf::from(args.get(1).ok_or(usage)?);
    match args.first().map(String::as_str) {
        Some("tree") => {
            let folders = args.get(2).ok_or(usage)?.parse()?;
            let files = args.get(3).ok_or(usage)?.parse()?;
            tree(&root, folders, files)
        }
        Some("search") => search(&root).await,
        Some("size") => size(&root).await,
        _ => Err(usage.into()),
    }
}
