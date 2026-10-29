//! The unpacking case of the measurements (RC-080-46, `scripts/perf.sh --cases`): a folder of
//! many small files packed into a 7z (one solid block) and a `tar.xz` by the queue's own pack
//! job, then each unpacked by the queue's unpack job, as Alt+F9 does - the time and the peak
//! resident memory of each unpacking (Linux only). Each unpacking runs in a process of its own,
//! so its peak holds nothing the packing or the other format left (RC-080-57).
//!
//! `perf archive-pack <work folder> <files>` makes the tree there, packs `tree.7z` and
//! `tree.tar.xz` beside it and prints `pack_<format>` in milliseconds and `files`.
//! `perf archive-unpack <work folder> <format> <files>` (`7z`, `tar_xz`) unpacks one into
//! `out-<format>` and prints `unpack_<format>` in milliseconds, `rss_peak_unpack_<format>_kib` (the
//! peak taken anew after the engine opened) and `unpacked_<format>` (the files found after). A
//! file missing after the unpacking, or a job that did not complete, is an error.

use std::{error::Error, path::Path};

use rc_engine::Engine;
use rc_ops::{
    ExtractSettings, JobKind, JobOptions, JobSnapshot, JobSpec, JobState, PackFormat, PackSettings,
};
use rc_vfs::VfsPath;

use super::{memory_kib, timed};

/// Files per folder of the tree.
const PER_FOLDER: usize = 100;

/// The formats measured: the name in the output lines, the format, the archive's file name.
const FORMATS: [(&str, PackFormat, &str); 2] = [
    ("7z", PackFormat::SevenZ, "tree.7z"),
    ("tar_xz", PackFormat::TarXz, "tree.tar.xz"),
];

/// Makes the tree of `files` files in `dir` and packs it in every format of [`FORMATS`].
pub(super) async fn pack(dir: &Path, files: usize) -> Result<(), Box<dyn Error>> {
    let source = dir.join("tree");
    make_tree(&source, files)?;
    let engine = Engine::open(VfsPath::local(dir), VfsPath::local(dir)).await?;
    for (name, format, file) in FORMATS {
        let pack = JobKind::Pack {
            sources: vec![VfsPath::local(&source)],
            target: VfsPath::local(dir.join(file)),
            settings: PackSettings {
                format,
                solid: true,
                overwrite: true,
                ..PackSettings::default()
            },
        };
        timed(&format!("pack_{name}"), run_job(&engine, pack)).await?;
    }
    println!("files\t{files}");
    Ok(())
}

/// Unpacks the archive [`pack`] made in `dir` in the format `name` and checks that all `files`
/// files came out.
pub(super) async fn unpack(dir: &Path, name: &str, files: usize) -> Result<(), Box<dyn Error>> {
    let (_, _, file) = FORMATS
        .into_iter()
        .find(|(known, _, _)| *known == name)
        .ok_or_else(|| format!("{name} is no format of the case (7z, tar_xz)"))?;
    let engine = Engine::open(VfsPath::local(dir), VfsPath::local(dir)).await?;
    let out = dir.join(format!("out-{name}"));
    reset_peak();
    let unpack = JobKind::Extract {
        archives: vec![VfsPath::local(dir.join(file))],
        target: VfsPath::local(&out),
        settings: ExtractSettings::default(),
    };
    timed(&format!("unpack_{name}"), run_job(&engine, unpack)).await?;
    if let Some(peak) = memory_kib("VmHWM") {
        println!("rss_peak_unpack_{name}_kib\t{peak}");
    }
    let count = count_files(&out)?;
    println!("unpacked_{name}\t{count}");
    if count != files {
        return Err(format!("{name}: {count} of {files} files unpacked").into());
    }
    Ok(())
}

/// `files` small text files below `root`, [`PER_FOLDER`] to a folder, a few hundred bytes each.
fn make_tree(root: &Path, files: usize) -> std::io::Result<()> {
    for index in 0..files {
        let folder = root.join(format!("d{:04}", index / PER_FOLDER));
        if index % PER_FOLDER == 0 {
            std::fs::create_dir_all(&folder)?;
        }
        let line = format!("line of file {index}, as a log or a source file has them\n");
        std::fs::write(
            folder.join(format!("f{index:06}.txt")),
            line.repeat(1 + index % 12),
        )?;
    }
    Ok(())
}

/// Runs `kind` on the engine's queue to its end; anything but a clean completion is an error.
async fn run_job(engine: &Engine, kind: JobKind) -> Result<JobSnapshot, Box<dyn Error>> {
    let queued = engine.ops().submit(JobSpec {
        kind,
        options: JobOptions::default(),
    })?;
    let done = engine.ops().wait(queued.id).await?;
    if done.state != JobState::Completed || !done.failures.is_empty() {
        return Err(format!("the job ended {:?}: {:?}", done.state, done.failures).into());
    }
    Ok(done)
}

/// Starts the peak resident memory over from now (`/proc/self/clear_refs`, Linux 4.0 on).
fn reset_peak() {
    // Without it the peak counts from the start; the line says so by being higher.
    let _ = std::fs::write("/proc/self/clear_refs", "5");
}

/// The files below `root`.
fn count_files(root: &Path) -> std::io::Result<usize> {
    let mut count = 0;
    let mut folders = vec![root.to_path_buf()];
    while let Some(folder) = folders.pop() {
        for entry in std::fs::read_dir(folder)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                folders.push(entry.path());
            } else {
                count += 1;
            }
        }
    }
    Ok(count)
}
