//! The queue's soak run (RC-080-04 stage 2, `scripts/soak.sh`): the engine without a window
//! copies, moves, deletes and creates folders on the local disk, round after round, for a given
//! time, and samples what the process holds - resident memory, open file descriptors, threads -
//! and the queue's job count. `scripts/lib/soak.py` reads the samples and judges the growth.
//!
//! `cargo run --release -p rc-engine --example soak -- <dir> <seconds> <interval> <csv>
//! [--batches]` makes `<dir>/source`, a small tree, and a database `<dir>/soak.db` (jobs are kept
//! in `transfer_jobs` from their submission, as in the window). Each round:
//!
//! 1. `ops.mkdir` `<dir>/round/copy` and `<dir>/round/moved`;
//! 2. `ops.copy` of the tree into `round/copy` - one job, or with `--batches` one job per entry
//!    of the tree submitted at once (a batch, `queue.one_job_per_item`);
//! 3. `ops.move` of `round/copy` into `round/moved`;
//! 4. `ops.delete` (permanent) of `round`.
//!
//! Every job must end `completed` without a failed file within [`JOB_LIMIT`]; the first one that
//! does not ends the run with exit 1, the samples so far kept. Every `<interval>` seconds, and
//! once at the end, a line `seconds,rss_kib,fds,threads,jobs,rounds` goes into `<csv>`; a value
//! the system does not tell (everywhere but Linux) stays empty. The last lines on stdout are
//! `rounds\t<n>` and `jobs\t<n>`.

use std::{
    error::Error,
    fs::File,
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use rc_db::Db;
use rc_engine::{Command, DeleteRequest, Engine, Outcome, TransferRequest};
use rc_ops::{DeleteMode, ErrorPolicy, JobOptions, JobSnapshot, JobState};
use rc_vfs::VfsPath;
use tokio::sync::oneshot;

/// The longest a job of a round may take; the tree is a few MiB on the local disk.
const JOB_LIMIT: Duration = Duration::from_secs(300);

/// Folders and files per folder of the source tree.
const FOLDERS: usize = 4;
const FILES: usize = 25;

type Failure = Box<dyn Error>;

/// The tree every round copies: [`FOLDERS`] folders of [`FILES`] files of 0 to 99 KiB, and one
/// file of 1 MiB.
fn make_source(root: &Path) -> std::io::Result<()> {
    for folder in 0..FOLDERS {
        let dir = root.join(format!("d{folder}"));
        std::fs::create_dir_all(&dir)?;
        for file in 0..FILES {
            let size = (folder * FILES + file) * 1024;
            std::fs::write(dir.join(format!("f{file:02}.bin")), vec![b'x'; size])?;
        }
    }
    std::fs::write(root.join("big.bin"), vec![b'y'; 1 << 20])
}

/// What `/proc/self/status` says of `field` (`VmRSS`, `Threads`), without its unit.
fn status_value(field: &str) -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status
        .lines()
        .find_map(|line| line.strip_prefix(field)?.strip_prefix(':'))
        .and_then(|value| value.trim().trim_end_matches("kB").trim().parse().ok())
}

/// The process's open file descriptors (`/proc/self/fd`), without the one reading them.
fn open_fds() -> Option<u64> {
    let count = std::fs::read_dir("/proc/self/fd").ok()?.count() as u64;
    Some(count.saturating_sub(1))
}

/// One CSV line: the seconds since `started`, then each value or nothing.
fn sample(engine: &Engine, started: Instant, rounds: &AtomicU64) -> String {
    let cell = |value: Option<u64>| value.map(|v| v.to_string()).unwrap_or_default();
    format!(
        "{:.1},{},{},{},{},{}",
        started.elapsed().as_secs_f64(),
        cell(status_value("VmRSS")),
        cell(open_fds()),
        cell(status_value("Threads")),
        engine.ops().jobs().len(),
        rounds.load(Ordering::Relaxed),
    )
}

/// Writes a sample every `interval` and a last one when `stop` fires.
async fn sampler(
    engine: Engine,
    csv: PathBuf,
    interval: Duration,
    rounds: Arc<AtomicU64>,
    mut stop: oneshot::Receiver<()>,
) -> std::io::Result<()> {
    let started = Instant::now();
    let mut out = BufWriter::new(File::create(csv)?);
    writeln!(out, "seconds,rss_kib,fds,threads,jobs,rounds")?;
    let mut ticker = tokio::time::interval(interval);
    loop {
        tokio::select! {
            _ = ticker.tick() => {
                writeln!(out, "{}", sample(&engine, started, &rounds))?;
                // Flushed at once: a run that dies keeps every sample up to then.
                out.flush()?;
            }
            _ = &mut stop => {
                writeln!(out, "{}", sample(&engine, started, &rounds))?;
                return out.flush();
            }
        }
    }
}

/// The jobs a command queued.
fn jobs_of(outcome: Outcome) -> Result<Vec<JobSnapshot>, Failure> {
    match outcome {
        Outcome::Job(job) => Ok(vec![*job]),
        Outcome::Jobs { jobs } => Ok(jobs),
        other => Err(format!("a job, got {other:?}").into()),
    }
}

/// Dispatches `command` and waits for each of its jobs to complete without a failed file;
/// answers how many jobs it ran.
async fn run(engine: &Engine, command: Command) -> Result<u64, Failure> {
    let id = command.id();
    let jobs = jobs_of(engine.dispatch(command).await?)?;
    for job in &jobs {
        let done = tokio::time::timeout(JOB_LIMIT, engine.ops().wait(job.id))
            .await
            .map_err(|_| format!("{id}: job {:?} did not end within {JOB_LIMIT:?}", job.id))??;
        if done.state != JobState::Completed || !done.failures.is_empty() {
            return Err(format!("{id}: job ended {:?}: {done:?}", done.state).into());
        }
    }
    Ok(jobs.len() as u64)
}

fn path_text(path: &Path) -> String {
    path.display().to_string()
}

/// One round over `dir`; answers how many jobs it ran.
async fn round(engine: &Engine, dir: &Path, batches: bool) -> Result<u64, Failure> {
    let source = dir.join("source");
    let round = dir.join("round");
    let copy = round.join("copy");
    let moved = round.join("moved");
    let options = JobOptions {
        on_error: ErrorPolicy::Cancel,
        ..JobOptions::default()
    };
    let mut jobs = 0;
    for folder in [&copy, &moved] {
        let mkdir = Command::OpsMkdir {
            panel: None,
            path: path_text(folder),
        };
        jobs += run(engine, mkdir).await?;
    }
    let sources = if batches {
        let mut entries = std::fs::read_dir(&source)?
            .map(|entry| entry.map(|entry| path_text(&entry.path())))
            .collect::<Result<Vec<_>, _>>()?;
        entries.sort();
        entries
    } else {
        vec![path_text(&source)]
    };
    let copy_all = Command::OpsCopy(TransferRequest {
        panel: None,
        sources,
        target: Some(path_text(&copy)),
        options,
    });
    jobs += run(engine, copy_all).await?;
    let move_copy = Command::OpsMove(TransferRequest {
        panel: None,
        sources: vec![path_text(&copy)],
        target: Some(path_text(&moved)),
        options,
    });
    jobs += run(engine, move_copy).await?;
    let delete = Command::OpsDelete(DeleteRequest {
        panel: None,
        sources: vec![path_text(&round)],
        mode: DeleteMode::Permanent,
        on_error: ErrorPolicy::Cancel,
        queue: None,
    });
    jobs += run(engine, delete).await?;
    Ok(jobs)
}

const USAGE: &str = "usage: soak <dir> <seconds> <interval> <csv> [--batches]";

#[tokio::main]
async fn main() -> Result<(), Failure> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (dir, seconds, interval, csv) = match args.as_slice() {
        [dir, seconds, interval, csv, ..] => (
            PathBuf::from(dir),
            seconds.parse::<u64>().map_err(|_| USAGE)?,
            interval.parse::<u64>().map_err(|_| USAGE)?,
            PathBuf::from(csv),
        ),
        _ => return Err(USAGE.into()),
    };
    let batches = match args.get(4).map(String::as_str) {
        None => false,
        Some("--batches") => true,
        Some(_) => return Err(USAGE.into()),
    };
    if interval == 0 {
        return Err(USAGE.into());
    }
    make_source(&dir.join("source"))?;
    let engine = Engine::open(VfsPath::local(&dir), VfsPath::local(&dir)).await?;
    engine.attach_db(Db::open(dir.join("soak.db")).await?);

    let rounds = Arc::new(AtomicU64::new(0));
    let (stop, stopped) = oneshot::channel();
    let samples = tokio::spawn(sampler(
        engine.clone(),
        csv,
        Duration::from_secs(interval),
        Arc::clone(&rounds),
        stopped,
    ));
    let deadline = Instant::now() + Duration::from_secs(seconds);
    let mut jobs = 0;
    let mut outcome = Ok(());
    while Instant::now() < deadline {
        match round(&engine, &dir, batches).await {
            Ok(ran) => {
                jobs += ran;
                rounds.fetch_add(1, Ordering::Relaxed);
            }
            Err(error) => {
                outcome = Err(error);
                break;
            }
        }
    }
    // The sampler has gone when the receiver is dropped; its error comes from the join.
    let _ = stop.send(());
    samples.await??;
    println!("rounds\t{}", rounds.load(Ordering::Relaxed));
    println!("jobs\t{jobs}");
    outcome
}
