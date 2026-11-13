//! RC-110-19: the program's folder and the portable mode's marker are no target. The program's
//! folder is given to each test - none depends on where the test binary lies.

use std::{
    fs,
    path::{Path, PathBuf},
};

use rc_db::{DB_FILE, Db};
use serde_json::json;

#[cfg(unix)]
use super::WriteGuard;
use super::{judge, judge_all, program_dirs_of, protected_target};
use crate::test_support::{Fixture, call, connect};

const PROGRAM: &str = "rCommander's program folder";
const MARKER: &str = "the marker of rCommander's portable mode";

fn path(parts: &[&str]) -> PathBuf {
    parts.iter().collect()
}

/// The names in the folder `dir`, sorted.
fn names_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("read dir")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

/// The folder itself and everything below it, whatever the name, ignoring case and as Windows
/// opens it; a folder beside it is not.
#[test]
fn the_program_s_folder_is_protected_as_a_whole() {
    let program = [path(&["opt", "rCommander"])];
    for parts in [
        &["opt", "rCommander"][..],
        &["opt", "rCommander", "rcommander.exe"],
        &["opt", "rCommander", "version.dll"],
        &["opt", "rCommander", "plugins", "deep", "x.wasm"],
        &["OPT", "RCOMMANDER", "notes.txt"],
        // As Windows opens them: trailing dots and spaces, an alternate data stream.
        &["opt", "rCommander.", "version.dll"],
        &["opt", "rCommander ", "version.dll"],
        &["opt", "rCommander::$DATA", "version.dll"],
    ] {
        assert_eq!(
            judge_all(&path(parts), &[], &program),
            Some(PROGRAM),
            "{parts:?}"
        );
    }
    for parts in [
        &["opt"][..],
        &["opt", "rCommander-old", "rcommander.exe"],
        &["opt", "other", "rCommander"],
        &["home", "bob", "notes.txt"],
    ] {
        assert_eq!(judge_all(&path(parts), &[], &program), None, "{parts:?}");
    }
    // Without a program folder nothing is one, and a root is never one.
    let at = path(&["opt", "rCommander", "version.dll"]);
    assert_eq!(judge_all(&at, &[], &[]), None);
    assert_eq!(judge_all(&at, &[], &[PathBuf::from("/")]), None);
}

/// The marker's name is no target anywhere, with or without a program folder known.
#[test]
fn the_portable_marker_is_protected_anywhere() {
    for parts in [
        &["tmp", "rcommander.portable"][..],
        &["tmp", "RCommander.Portable"],
        &["home", "bob", "stick", "rcommander.portable"],
        &["rcommander.portable"],
        &["tmp", "rcommander.portable", "inside.txt"],
    ] {
        assert_eq!(judge(&path(parts), &[]), Some(MARKER), "{parts:?}");
    }
    // As Windows opens them; `protected_target` judges both spellings.
    for parts in [
        &["tmp", "rcommander.portable."][..],
        &["tmp", "rcommander.portable "],
        &["tmp", "rcommander.portable::$DATA"],
        &["tmp", "rcommander.portable:stream"],
    ] {
        assert_eq!(protected_target(&path(parts)), Some(MARKER), "{parts:?}");
    }
    for parts in [
        &["tmp", "rcommander.portable.txt"][..],
        &["tmp", "my-rcommander.portable"],
        &["tmp", "rcommander", "portable"],
    ] {
        assert_eq!(judge(&path(parts), &[]), None, "{parts:?}");
    }
}

/// The program's folder is known under the name it was started by and where it really is: a
/// started link to the folder, a link to the program itself, and a target written through
/// another link into the folder.
#[cfg(unix)]
#[tokio::test]
async fn the_program_s_folder_is_judged_under_every_name_it_has() {
    use std::os::unix::fs::symlink;

    let root = tempfile::tempdir().expect("temp dir");
    let real = fs::canonicalize(root.path()).expect("real root");
    let at = |rel: &str| real.join(rel);
    fs::create_dir_all(at("app")).expect("mkdir");
    fs::create_dir_all(at("bin")).expect("mkdir");
    fs::write(at("app/rcommander"), b"program").expect("write");
    symlink(at("app"), at("current")).expect("link to the folder");
    symlink(at("app/rcommander"), at("bin/rcommander")).expect("link to the program");
    symlink(at("app"), at("elsewhere")).expect("another link");

    // Started through a link to its folder: the folder is known by both names.
    let by_folder = program_dirs_of(&at("current/rcommander"));
    assert!(by_folder.contains(&at("current")), "{by_folder:?}");
    assert!(by_folder.contains(&at("app")), "{by_folder:?}");
    // Started through a link to the program: the link's folder and the program's.
    let by_program = program_dirs_of(&at("bin/rcommander"));
    assert!(by_program.contains(&at("bin")), "{by_program:?}");
    assert!(by_program.contains(&at("app")), "{by_program:?}");

    let guard = WriteGuard {
        data_dirs: Vec::new(),
        program_dirs: by_folder,
    };
    for rel in [
        "app/version.dll",
        "current/version.dll",
        "elsewhere/version.dll",
        "elsewhere/plugins/new/x.wasm",
        "app/rcommander.portable",
    ] {
        let target = rc_engine::VfsPath::local(at(rel));
        assert_eq!(guard.refuses(&target).await, Some(PROGRAM), "{rel}");
    }
    let harmless = rc_engine::VfsPath::local(at("out/version.dll"));
    assert_eq!(guard.refuses(&harmless).await, None);
}

/// Acceptance (RC-110-19): no tool copies, moves, renames or creates anything in the program's
/// folder - written plainly or as Windows opens it - or under the marker's name anywhere, and
/// nothing is written.
#[tokio::test]
async fn no_tool_writes_into_the_program_s_folder_or_the_marker() {
    let fixture = Fixture::new();
    let at = |rel: &str| fixture.root().join(rel);
    for dir in ["data", "app", "out", "stage"] {
        fs::create_dir_all(at(dir)).expect("mkdir");
    }
    fs::write(at("app/rcommander.exe"), b"program").expect("write");
    fs::write(at("a.txt"), b"a").expect("write");
    fs::write(at("out/note.txt"), b"n").expect("write");
    fs::write(at("stage/rcommander.portable"), b"").expect("write");
    let db = Db::open(at("data").join(DB_FILE)).await.expect("db");
    let built = fixture.with_db(db).build().await;
    let root = built.root.path();
    let mut context = built.context;
    context.program_dirs = program_dirs_of(&root.join("app").join("rcommander.exe"));
    let client = connect(&context).await;

    let calls = [
        // Into the program's folder.
        ("ops_copy", json!({ "sources": ["a.txt"], "target": "app" })),
        ("ops_move", json!({ "sources": ["a.txt"], "target": "app" })),
        (
            "ops_copy",
            json!({ "sources": ["a.txt"], "target": "app/plugins" }),
        ),
        ("ops_mkdir", json!({ "path": "app/plugins" })),
        (
            "archives_pack",
            json!({ "sources": ["a.txt"], "target": "app/a.zip" }),
        ),
        (
            "files_create_link",
            json!({ "target": "a.txt", "link": "app/version.dll" }),
        ),
        // The program renamed away, so that another file can take its name.
        (
            "multi_rename_apply",
            json!({
                "dir": "app",
                "names": ["rcommander.exe"],
                "name_mask": "old",
                "extension_mask": "[E]",
            }),
        ),
        // As Windows opens the folder's name.
        (
            "ops_copy",
            json!({ "sources": ["a.txt"], "target": "app." }),
        ),
        ("ops_mkdir", json!({ "path": "app./plugins" })),
        // The marker, far from the program.
        (
            "ops_copy",
            json!({ "sources": ["stage/rcommander.portable"], "target": "out" }),
        ),
        (
            "ops_move",
            json!({ "sources": ["stage/rcommander.portable"], "target": "out" }),
        ),
        ("ops_mkdir", json!({ "path": "out/rcommander.portable" })),
        ("ops_mkdir", json!({ "path": "out/rcommander.portable." })),
        (
            "files_create_link",
            json!({ "target": "a.txt", "link": "out/rcommander.portable" }),
        ),
        (
            "multi_rename_apply",
            json!({
                "dir": "out",
                "names": ["note.txt"],
                "name_mask": "rcommander",
                "extension_mask": "portable",
            }),
        ),
    ];
    for (tool, arguments) in calls {
        let answer = call(&client, tool, arguments.clone()).await;
        assert_eq!(
            answer["code"], "protected_target",
            "{tool} {arguments}: {answer}"
        );
    }
    assert!(
        context.engine().ops().jobs().is_empty(),
        "no job was queued"
    );
    assert_eq!(names_in(&root.join("app")), ["rcommander.exe"]);
    assert_eq!(names_in(&root.join("out")), ["note.txt"]);
    assert!(root.join("a.txt").is_file(), "nothing was moved");
    assert!(root.join("stage/rcommander.portable").is_file());

    // The reason is the place's own.
    let copied = call(
        &client,
        "ops_copy",
        json!({ "sources": ["a.txt"], "target": "app" }),
    )
    .await;
    let said = copied.to_string();
    assert!(said.contains(PROGRAM), "{said}");
    let marked = call(
        &client,
        "ops_mkdir",
        json!({ "path": "out/rcommander.portable" }),
    )
    .await;
    let said = marked.to_string();
    assert!(said.contains(MARKER), "{said}");

    // A harmless target beside them is written as before.
    let harmless = call(
        &client,
        "ops_copy",
        json!({ "sources": ["a.txt"], "target": "out" }),
    )
    .await;
    assert!(harmless["id"].as_u64().is_some(), "{harmless}");
}
