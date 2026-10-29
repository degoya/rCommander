//! The measurements of RC-020-05 on real files (`docs/performance.md`): hashing a very large
//! file in constant memory, splitting it into parts and combining them, and the checksum files
//! `create` writes, for `sha256sum -c`, `md5sum -c` and an SFV tool to read.
//!
//! ```text
//! cargo run --release -p rc-checksum --example bigfile -- make <file> <GiB>
//! cargo run --release -p rc-checksum --example bigfile -- hash <file> <ext>...
//! cargo run --release -p rc-checksum --example bigfile -- split <file> <parts-dir> <out-dir> <MiB>
//! cargo run --release -p rc-checksum --example bigfile -- create <dir> <ext> per-file|combined <name>...
//! ```
//!
//! `make` writes `<GiB>` GiB of pseudo-random bytes. `hash` hashes the file once per algorithm,
//! named by its checksum extension (`sfv`, `md5`, `sha`, `sha256`, `sha512`, `blake3`), while a
//! thread reads the resident memory every 100 ms. `split` splits into parts of `<MiB>` MiB with
//! the `.crc` file and combines them again into `<out-dir>`. `create` writes the checksum files.
//! Each prints `<step>\t<value>` lines: milliseconds, MiB/s, and on Linux memory in KiB.

use std::{
    error::Error,
    io::Write as _,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use rc_checksum::{Algorithm, Control, Layout, combine, create, hash_file, split};
use rc_vfs::{LocalFs, Vfs};

/// What `/proc/self/status` says of `field` (`VmRSS`, `VmHWM`), in KiB; `None` elsewhere.
fn memory_kib(field: &str) -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status
        .lines()
        .find_map(|line| line.strip_prefix(field)?.strip_prefix(':'))
        .and_then(|value| value.trim().trim_end_matches("kB").trim().parse().ok())
}

/// Reads the resident memory every 100 ms until dropped, then prints its first, lowest and
/// highest sample.
struct Sampler {
    name: String,
    stop: Arc<AtomicBool>,
    samples: Arc<Mutex<Vec<u64>>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Sampler {
    fn start(name: &str) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let samples = Arc::new(Mutex::new(Vec::new()));
        let thread = {
            let (stop, samples) = (stop.clone(), samples.clone());
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    if let (Some(kib), Ok(mut all)) = (memory_kib("VmRSS"), samples.lock()) {
                        all.push(kib);
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
            })
        };
        Self {
            name: name.to_owned(),
            stop,
            samples,
            thread: Some(thread),
        }
    }
}

impl Drop for Sampler {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let samples = self
            .samples
            .lock()
            .map(|all| all.clone())
            .unwrap_or_default();
        let name = &self.name;
        println!("{name}_rss_samples\t{}", samples.len());
        if let (Some(first), Some(low), Some(high)) =
            (samples.first(), samples.iter().min(), samples.iter().max())
        {
            println!("{name}_rss_first_kib\t{first}");
            println!("{name}_rss_min_kib\t{low}");
            println!("{name}_rss_max_kib\t{high}");
        }
    }
}

fn algorithm(extension: &str) -> Result<Algorithm, Box<dyn Error>> {
    Algorithm::from_extension(extension).ok_or_else(|| format!("no algorithm: {extension}").into())
}

/// `gib` GiB of pseudo-random bytes (xorshift64) at `path`, which must not exist.
fn make(path: &Path, gib: u64) -> Result<(), Box<dyn Error>> {
    let mut out = std::fs::File::create_new(path)?;
    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    let mut block = vec![0u8; 8 << 20];
    for _ in 0..(gib << 30) / block.len() as u64 {
        for chunk in block.as_chunks_mut::<8>().0 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            *chunk = state.to_le_bytes();
        }
        out.write_all(&block)?;
    }
    out.sync_all()?;
    Ok(())
}

async fn hash(
    vfs: &Arc<dyn Vfs>,
    path: &Path,
    extensions: &[String],
) -> Result<(), Box<dyn Error>> {
    let size = std::fs::metadata(path)?.len();
    for extension in extensions {
        let algorithm = algorithm(extension)?;
        let sampler = Sampler::start(&format!("hash_{extension}"));
        let started = Instant::now();
        let hex = hash_file(vfs, path, algorithm, &Control::new()).await?;
        let took = started.elapsed().as_secs_f64();
        drop(sampler);
        println!("hash_{extension}_ms\t{:.0}", took * 1000.0);
        println!(
            "hash_{extension}_mib_s\t{:.0}",
            size as f64 / took / f64::from(1 << 20)
        );
        println!("hash_{extension}_hex\t{hex}");
    }
    Ok(())
}

async fn split_combine(
    vfs: &Arc<dyn Vfs>,
    path: &Path,
    parts: &Path,
    out: &Path,
    mib: u64,
) -> Result<(), Box<dyn Error>> {
    let sampler = Sampler::start("split");
    let started = Instant::now();
    let report = split(vfs, path, vfs, parts, mib << 20, &Control::new()).await?;
    println!("split_ms\t{:.0}", started.elapsed().as_secs_f64() * 1000.0);
    drop(sampler);
    println!("split_parts\t{}", report.parts.len());
    println!("split_crc_file\t{}", report.crc_file);
    println!("split_crc32\t{}", report.crc32);
    let first = parts.join(report.parts.first().ok_or("no parts")?);
    let sampler = Sampler::start("combine");
    let started = Instant::now();
    let report = combine(vfs, &first, vfs, out, None, &Control::new()).await?;
    println!(
        "combine_ms\t{:.0}",
        started.elapsed().as_secs_f64() * 1000.0
    );
    drop(sampler);
    println!("combine_target\t{}", report.target);
    println!("combine_size\t{}", report.size);
    println!("combine_crc32\t{}", report.crc32);
    println!("combine_crc_checked\t{}", report.crc_checked);
    Ok(())
}

async fn create_files(
    vfs: &Arc<dyn Vfs>,
    dir: &Path,
    extension: &str,
    layout: &str,
    names: &[String],
) -> Result<(), Box<dyn Error>> {
    let layout = match layout {
        "per-file" => Layout::PerFile,
        "combined" => Layout::Combined { file_name: None },
        _ => return Err("layout: per-file or combined".into()),
    };
    let report = create(
        vfs,
        dir,
        names,
        algorithm(extension)?,
        &layout,
        &Control::new(),
    )
    .await?;
    for written in report.written {
        println!("written\t{written}");
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let usage = "usage: bigfile make <file> <GiB> | hash <file> <ext>... \
                 | split <file> <parts-dir> <out-dir> <MiB> | create <dir> <ext> <layout> <name>...";
    let path = PathBuf::from(args.get(1).ok_or(usage)?);
    let vfs: Arc<dyn Vfs> = Arc::new(LocalFs::new());
    let arg = |index: usize| args.get(index).ok_or(usage);
    match args.first().map(String::as_str) {
        Some("make") => make(&path, arg(2)?.parse()?),
        Some("hash") => hash(&vfs, &path, &args[2..]).await,
        Some("split") => {
            let (parts, out) = (PathBuf::from(arg(2)?), PathBuf::from(arg(3)?));
            split_combine(&vfs, &path, &parts, &out, arg(4)?.parse()?).await
        }
        Some("create") => create_files(&vfs, &path, arg(2)?, arg(3)?, &args[4..]).await,
        _ => Err(usage.into()),
    }?;
    if let Some(peak) = memory_kib("VmHWM") {
        println!("rss_peak_kib\t{peak}");
    }
    Ok(())
}
