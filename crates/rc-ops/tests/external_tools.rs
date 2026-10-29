//! Packing against the outside world (RC-030-04): what rCommander packs, 7-Zip and GNU tar list
//! and unpack byte for byte; what they pack, rCommander opens and unpacks. Opt-in, because the
//! tools are not part of the build:
//! `cargo nextest run -p rc-ops --test external_tools --run-ignored only`. 7-Zip is `$RC_7ZIP`,
//! else `7zz` or `7z` on the `PATH`; a missing tool skips its half with a note.

mod support;

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
};

use rc_ops::{ExtractSettings, JobKind, JobOptions, JobSpec, JobState, PackFormat, PackSettings};
use rc_vfs::{ArchiveFs, ArchiveSettings, LocalFs, Vfs, VfsPath};
use support::{Harness, payload};

/// `root/src`: files at the top and below, umlauts, a long path, an empty file and folder.
fn tree(root: &Path) -> Vec<(String, Vec<u8>)> {
    let files = vec![
        ("src/a.txt".to_owned(), b"alpha\n".to_vec()),
        ("src/deep/b.bin".to_owned(), payload(200_000, 7)),
        (
            "src/deep/Gr\u{f6}\u{df}e \u{e4}\u{f6}\u{fc}.txt".to_owned(),
            b"umlaut\n".to_vec(),
        ),
        (
            format!(
                "src/{}/{}/{}.txt",
                "x".repeat(60),
                "y".repeat(60),
                "l".repeat(150)
            ),
            b"long\n".to_vec(),
        ),
        ("src/empty.txt".to_owned(), Vec::new()),
    ];
    for (rel, bytes) in &files {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        fs::write(path, bytes).expect("write");
    }
    fs::create_dir_all(root.join("src/hollow")).expect("mkdir");
    files
}

/// Every file below `root`, relative, sorted.
fn files(root: &Path) -> Vec<String> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).expect("read_dir").flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let rel = path.strip_prefix(root).expect("below root");
                found.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    found.sort();
    found
}

/// Every byte of `files` under `out`, nothing else, and the empty folder.
fn assert_same(out: &Path, files_in: &[(String, Vec<u8>)], what: &str) {
    let mut expected: Vec<String> = files_in.iter().map(|(rel, _)| rel.clone()).collect();
    expected.sort();
    assert_eq!(files(out), expected, "{what}");
    for (rel, bytes) in files_in {
        assert_eq!(&fs::read(out.join(rel)).expect(rel), bytes, "{what}: {rel}");
    }
    assert!(out.join("src/hollow").is_dir(), "{what}: the empty folder");
}

/// The first of `names` that starts, or `None`.
fn find_tool(names: &[String], probe: &str) -> Option<String> {
    names
        .iter()
        .find(|name| Command::new(name).arg(probe).output().is_ok())
        .cloned()
}

fn seven_zip() -> Option<String> {
    let mut names: Vec<String> = std::env::var("RC_7ZIP").into_iter().collect();
    names.extend(["7zz".to_owned(), "7z".to_owned()]);
    let found = find_tool(&names, "i");
    if found.is_none() {
        eprintln!("no 7-Zip ($RC_7ZIP, 7zz, 7z): its half is skipped");
    }
    found
}

fn tool(name: &str) -> Option<String> {
    let found = find_tool(&[name.to_owned()], "--version");
    if found.is_none() {
        eprintln!("no {name} on this machine: its half is skipped");
    }
    found
}

/// Runs `program args`, panicking with its output when it fails; returns stdout.
fn run(program: &str, args: &[&std::ffi::OsStr], dir: Option<&Path>) -> String {
    let mut command = Command::new(program);
    command.args(args);
    if let Some(dir) = dir {
        command.current_dir(dir);
    }
    let output = command.output().expect("start the tool");
    assert!(
        output.status.success(),
        "{program} {args:?}: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn os(text: &str) -> &std::ffi::OsStr {
    std::ffi::OsStr::new(text)
}

fn is_tar(format: PackFormat) -> bool {
    !matches!(format, PackFormat::Zip | PackFormat::SevenZ)
}

/// 7-Zip tests, lists and unpacks `archive` into `out`; a compressed tar in two steps, as 7-Zip
/// unwraps one layer at a time.
fn seven_zip_unpacks(seven: &str, archive: &Path, format: PackFormat, out: &Path) -> String {
    run(seven, &[os("t"), archive.as_os_str()], None);
    let mut inner = archive.to_path_buf();
    if is_tar(format) && format != PackFormat::Tar {
        let layer = PathBuf::from(format!("{}-layer", out.display()));
        let flag = format!("-o{}", layer.display());
        run(
            seven,
            &[os("x"), os("-y"), os(&flag), archive.as_os_str()],
            None,
        );
        inner = fs::read_dir(&layer)
            .expect("layer")
            .flatten()
            .next()
            .expect("the tar inside")
            .path();
    }
    let listing = run(seven, &[os("l"), os("-slt"), inner.as_os_str()], None);
    let flag = format!("-o{}", out.display());
    run(
        seven,
        &[os("x"), os("-y"), os(&flag), inner.as_os_str()],
        None,
    );
    listing
}

fn pack_spec(source: &Path, target: &Path, format: PackFormat, solid: bool) -> JobSpec {
    JobSpec {
        kind: JobKind::Pack {
            sources: vec![VfsPath::local(source)],
            target: VfsPath::local(target),
            settings: PackSettings {
                format,
                solid,
                ..PackSettings::default()
            },
        },
        options: JobOptions::default(),
    }
}

/// Acceptance: every format rCommander packs (7z solid and not) passes 7-Zip's test, lists every
/// name and unpacks byte for byte, umlauts, long paths and the empty folder included; GNU tar
/// does the same for the tar family (tar.zst only with `zstd` installed).
#[tokio::test]
#[ignore = "needs 7-Zip and GNU tar; run with --run-ignored only"]
async fn rcommander_archives_open_in_7zip_and_tar() {
    let root = tempfile::tempdir().expect("tempdir");
    let files_in = tree(root.path());
    let seven = seven_zip();
    let tar = tool("tar");
    let zstd = tool("zstd");
    let harness = Harness::start();
    harness.local();
    for format in PackFormat::ALL {
        for solid in [true, false] {
            if !solid && format != PackFormat::SevenZ {
                continue;
            }
            let name = format!("rc-{solid}.{}", format.extension());
            let archive = root.path().join(&name);
            let done = harness
                .run(pack_spec(&root.path().join("src"), &archive, format, solid))
                .await;
            assert_eq!(
                done.state,
                JobState::Completed,
                "{name}: {:?}",
                done.failures
            );
            assert!(done.failures.is_empty(), "{name}: {:?}", done.failures);
            if let Some(seven) = &seven {
                let out = root.path().join(format!("{name}-7z"));
                let listing = seven_zip_unpacks(seven, &archive, format, &out);
                for (rel, _) in &files_in {
                    assert!(
                        listing.contains(&format!("Path = {rel}\n")),
                        "{name}: {rel}"
                    );
                }
                assert_same(&out, &files_in, &format!("7-Zip {name}"));
            }
            let skip_zst = format == PackFormat::TarZst && zstd.is_none();
            if let (Some(tar), true, false) = (&tar, is_tar(format), skip_zst) {
                let listing = run(tar, &[os("-tf"), archive.as_os_str()], None);
                for (rel, _) in &files_in {
                    assert!(listing.lines().any(|line| line == rel), "{name}: {rel}");
                }
                let out = root.path().join(format!("{name}-tar"));
                fs::create_dir_all(&out).expect("mkdir");
                run(
                    tar,
                    &[os("-xf"), archive.as_os_str(), os("-C"), out.as_os_str()],
                    None,
                );
                assert_same(&out, &files_in, &format!("GNU tar {name}"));
            }
        }
    }
}

/// What 7-Zip and GNU tar pack: (archive name, program, arguments), run in the tree's root.
fn foreign_archives(seven: Option<&str>, tar: Option<&str>) -> Vec<(String, String, Vec<String>)> {
    let mut made = Vec::new();
    let strings = |args: &[&str]| args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
    if let Some(seven) = seven {
        for (name, args) in [
            ("7zip.zip", strings(&["a", "-tzip", "7zip.zip", "src"])),
            (
                "7zip-solid.7z",
                strings(&["a", "-t7z", "-ms=on", "7zip-solid.7z", "src"]),
            ),
            (
                "7zip-files.7z",
                strings(&["a", "-t7z", "-ms=off", "7zip-files.7z", "src"]),
            ),
            ("7zip.tar", strings(&["a", "-ttar", "7zip.tar", "src"])),
        ] {
            made.push((name.to_owned(), seven.to_owned(), args));
        }
    }
    if let Some(tar) = tar {
        for (name, flag) in [
            ("gnu.tar", "-cf"),
            ("gnu.tar.gz", "-czf"),
            ("gnu.tar.bz2", "-cjf"),
            ("gnu.tar.xz", "-cJf"),
        ] {
            made.push((
                name.to_owned(),
                tar.to_owned(),
                strings(&[flag, name, "src"]),
            ));
        }
    }
    made
}

/// Acceptance: zip, 7z (solid and not) and tar from 7-Zip, tar/gz/bz2/xz from GNU tar open in
/// the archive backend with every name and unpack through rCommander's extract job byte for
/// byte. (tar.zst: neither tool here writes zstd; covered once `zstd` is installed.)
#[tokio::test]
#[ignore = "needs 7-Zip and GNU tar; run with --run-ignored only"]
async fn foreign_archives_open_and_unpack_in_rcommander() {
    let root = tempfile::tempdir().expect("tempdir");
    let files_in = tree(root.path());
    let seven = seven_zip();
    let tar = tool("tar");
    let mut made = foreign_archives(seven.as_deref(), tar.as_deref());
    if let Some(tar) = tar.as_deref().filter(|_| tool("zstd").is_some()) {
        let args = ["--zstd", "-cf", "gnu.tar.zst", "src"]
            .map(str::to_owned)
            .to_vec();
        made.push(("gnu.tar.zst".to_owned(), tar.to_owned(), args));
    }
    let harness = Harness::start();
    harness.local();
    for (name, program, args) in made {
        let args: Vec<&std::ffi::OsStr> = args.iter().map(|arg| os(arg)).collect();
        run(&program, &args, Some(root.path()));
        let archive = root.path().join(&name);
        let outer: Arc<dyn Vfs> = Arc::new(LocalFs::new());
        let opened = ArchiveFs::open(outer, &archive, ArchiveSettings::default())
            .await
            .unwrap_or_else(|error| panic!("{name}: {error}"))
            .unwrap_or_else(|| panic!("{name}: not taken for an archive"));
        for (rel, bytes) in &files_in {
            let stat = opened
                .stat(Path::new(rel))
                .await
                .unwrap_or_else(|error| panic!("{name}: {rel}: {error}"));
            assert_eq!(stat.size, bytes.len() as u64, "{name}: {rel}");
        }
        let out: PathBuf = root.path().join(format!("{name}-rc"));
        let done = harness
            .run(JobSpec {
                kind: JobKind::Extract {
                    archives: vec![VfsPath::local(&archive)],
                    target: VfsPath::local(&out),
                    settings: ExtractSettings::default(),
                },
                options: JobOptions::default(),
            })
            .await;
        assert_eq!(
            done.state,
            JobState::Completed,
            "{name}: {:?}",
            done.failures
        );
        assert!(done.failures.is_empty(), "{name}: {:?}", done.failures);
        assert_same(&out, &files_in, &format!("rCommander {name}"));
    }
}
