//! The measurement of the viewer on a very large log file (RC-020-07, `docs/performance.md`):
//! what F3 does on its first page and while scrolling, through the engine and without the window.
//!
//! ```text
//! cargo run --release -p rc-engine --example viewer_big -- log <file> <GiB>
//! cargo run --release -p rc-engine --example viewer_big -- view <file>
//! ```
//!
//! `log` writes a log file of `<GiB>` GiB, one numbered line of about 80 bytes after the other.
//! `view` opens it as the viewer window does (`open_for_view`, the head read to tell the
//! encoding), reads the first page, jumps to the end (Ctrl+End), pages down and up 2000 times,
//! and jumps to 200 places spread over the file (the scroll bar). Each page is computed as the
//! window's `text_page` does it: the last page's top, the seek, the page's lines. It prints
//! `<step>\t<value>` lines: milliseconds, and on Linux the resident memory in KiB.

use std::{
    error::Error,
    io::Write as _,
    path::{Path, PathBuf},
    time::Instant,
};

use rc_engine::{
    Engine,
    viewer::{DETECT_BYTES, Detected, FileWindow, detect, text},
};
use rc_vfs::VfsPath;

/// Rows of a viewer window on a full-HD screen.
const ROWS: usize = 60;
const PAGES: usize = 2000;
const JUMPS: u64 = 200;

/// What `/proc/self/status` says of `field` (`VmRSS`, `VmHWM`), in KiB; `None` elsewhere.
fn memory_kib(field: &str) -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status
        .lines()
        .find_map(|line| line.strip_prefix(field)?.strip_prefix(':'))
        .and_then(|value| value.trim().trim_end_matches("kB").trim().parse().ok())
}

fn print_memory(step: &str) {
    if let Some(kib) = memory_kib("VmRSS") {
        println!("{step}_rss_kib\t{kib}");
    }
}

fn ms(since: Instant) -> f64 {
    since.elapsed().as_secs_f64() * 1000.0
}

/// A log file of `gib` GiB at `path`, which must not exist.
fn log(path: &Path, gib: u64) -> Result<(), Box<dyn Error>> {
    let file = std::fs::File::create_new(path)?;
    let mut out = std::io::BufWriter::with_capacity(8 << 20, file);
    let size = gib << 30;
    let mut written = 0u64;
    let mut line = 0u64;
    let mut text = String::new();
    while written < size {
        text.clear();
        let (hour, minute, second) = (line / 3_600_000 % 24, line / 60_000 % 60, line / 1000 % 60);
        use std::fmt::Write as _;
        writeln!(
            text,
            "2026-10-18 {hour:02}:{minute:02}:{second:02}.{:03} INFO  [worker-{:02}] request {line:010} handled in {} ms",
            line % 1000,
            line % 16,
            line % 97
        )?;
        let room = usize::try_from(size - written).unwrap_or(usize::MAX);
        let bytes = &text.as_bytes()[..text.len().min(room)];
        out.write_all(bytes)?;
        written += bytes.len() as u64;
        line += 1;
    }
    out.flush()?;
    println!("lines\t{line}");
    Ok(())
}

/// Where a page down or up from `top` leads.
enum Seek {
    Down,
    Up,
    End,
    At(u64),
}

/// The page `seek` leads to from `top`, as the window's `text_page` computes it; its top.
async fn page(
    file: &FileWindow,
    detected: Detected,
    top: u64,
    seek: Seek,
) -> Result<u64, Box<dyn Error>> {
    let last = text::last_page(file, detected, ROWS).await?;
    let top = match seek {
        Seek::Down if top >= last => top,
        Seek::Down => text::read_lines(file, detected, top, ROWS + 1)
            .await?
            .lines
            .get(ROWS)
            .map_or(last, |line| line.offset)
            .min(last),
        Seek::Up => text::lines_before(file, detected, top, ROWS).await?,
        Seek::End => last,
        Seek::At(offset) => text::line_start(file, detected, offset).await?.min(last),
    };
    let shown = text::read_lines(file, detected, top, ROWS).await?;
    if shown.lines.is_empty() {
        return Err("an empty page".into());
    }
    Ok(top)
}

/// Runs `count` seeks and prints their mean and longest time.
async fn timed_pages(
    name: &str,
    file: &FileWindow,
    detected: Detected,
    mut top: u64,
    count: u64,
    seek: impl Fn(u64) -> Seek,
) -> Result<u64, Box<dyn Error>> {
    let mut longest = 0f64;
    let started = Instant::now();
    for index in 0..count {
        let one = Instant::now();
        top = page(file, detected, top, seek(index)).await?;
        longest = longest.max(ms(one));
    }
    println!("{name}_mean_ms\t{:.3}", ms(started) / count as f64);
    println!("{name}_longest_ms\t{longest:.3}");
    print_memory(name);
    Ok(top)
}

async fn view(path: &Path) -> Result<(), Box<dyn Error>> {
    let parent = VfsPath::local(path.parent().ok_or("no parent")?);
    let engine = Engine::open(parent.clone(), parent).await?;
    print_memory("engine");

    let started = Instant::now();
    let file = engine.open_for_view(&VfsPath::local(path)).await?;
    let head = file.read_uncached(0, DETECT_BYTES).await?;
    let detected = detect(&head);
    println!("open_ms\t{:.3}", ms(started));
    let top = page(&file, detected, 0, Seek::At(0)).await?;
    println!("first_page_ms\t{:.3}", ms(started));
    println!("size_bytes\t{}", file.size());
    print_memory("first_page");

    let started = Instant::now();
    let end = page(&file, detected, top, Seek::End).await?;
    println!("end_ms\t{:.3}", ms(started));

    let pages = PAGES as u64;
    timed_pages("page_down", &file, detected, top, pages, |_| Seek::Down).await?;
    timed_pages("page_up", &file, detected, end, pages, |_| Seek::Up).await?;
    let size = file.size();
    timed_pages("jump", &file, detected, 0, JUMPS, |index| {
        // Spread over the file, in an order that is not monotonic.
        Seek::At((index * 7919 % JUMPS) * (size / JUMPS))
    })
    .await?;
    println!("cached_bytes\t{}", file.cached_bytes());
    if let Some(peak) = memory_kib("VmHWM") {
        println!("rss_peak_kib\t{peak}");
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let usage = "usage: viewer_big log <file> <GiB> | view <file>";
    let path = PathBuf::from(args.get(1).ok_or(usage)?);
    match args.first().map(String::as_str) {
        Some("log") => log(&path, args.get(2).ok_or(usage)?.parse()?),
        Some("view") => view(&path).await,
        _ => Err(usage.into()),
    }
}
