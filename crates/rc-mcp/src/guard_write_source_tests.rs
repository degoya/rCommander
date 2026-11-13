//! RC-110-19: a protected place is no source of a move or a rename, and neither is a folder that
//! holds one. The program's and the data folder are given to each test - none depends on where
//! the test binary lies.

use std::{fs, path::PathBuf};

use rc_db::{DB_FILE, Db};
use rc_engine::{Command, JobOptions, TransferRequest, VfsPath};
use serde_json::json;

use super::{WriteGuard, judge_holding, program_dirs_of};
use crate::test_support::{Fixture, call, connect};

const PROGRAM: &str = "rCommander's program folder";
const DATA: &str = "rCommander's data folder";

fn path(parts: &[&str]) -> PathBuf {
    parts.iter().collect()
}

/// A folder above the program's or the data folder holds it, whatever the case and as Windows
/// opens its name; the folders themselves, what lies in them and their neighbours do not.
#[test]
fn a_folder_above_the_program_or_the_data_folder_holds_it() {
    let program = [path(&["opt", "tools", "rCommander"])];
    let data = [path(&["home", "bob", ".local", "share", "rcommander"])];
    for (parts, reason) in [
        (&["opt"][..], PROGRAM),
        (&["opt", "tools"][..], PROGRAM),
        (&["OPT", "Tools"][..], PROGRAM),
        (&["opt", "tools."][..], PROGRAM),
        (&["opt", "tools "][..], PROGRAM),
        (&["opt", "tools::$DATA"][..], PROGRAM),
        (&["home"][..], DATA),
        (&["home", "bob", ".local", "share"][..], DATA),
        (&["home", "BOB", ".Local"][..], DATA),
        (&["home", "bob."][..], DATA),
    ] {
        assert_eq!(
            judge_holding(&path(parts), &data, &program),
            Some(reason),
            "{parts:?}"
        );
    }
    // A root holds everything.
    assert_eq!(
        judge_holding(&PathBuf::from("/"), &data, &program),
        Some(PROGRAM)
    );
    for parts in [
        &["opt", "tools", "rCommander", "plugins"][..],
        &["opt", "tools-old"],
        &["opt", "other"],
        &["home", "alice"],
        &["home", "bob", ".local", "share", "rcommander", "backups"],
        &["srv"],
    ] {
        assert_eq!(
            judge_holding(&path(parts), &data, &program),
            None,
            "{parts:?}"
        );
    }
    // Without a folder known nothing holds one, and a root is never one.
    assert_eq!(judge_holding(&path(&["opt"]), &[], &[]), None);
    assert_eq!(
        judge_holding(&path(&["opt"]), &[PathBuf::from("/")], &[]),
        None
    );
}

/// The guard's judgement of a source on a real tree: in a protected place, a folder above one,
/// a folder holding a place known by its name - and everything else free.
#[tokio::test]
async fn an_entry_of_a_protected_place_and_a_folder_holding_one_are_not_taken() {
    let root = tempfile::tempdir().expect("temp dir");
    let real = fs::canonicalize(root.path()).expect("real root");
    let at = |rel: &str| real.join(rel);
    for dir in [
        "inst/app/plugins",
        "prof/data",
        "home/bob/.ssh",
        "home/bob/docs",
        "home/eve/.config/autostart",
        "home/kim",
        "plain/sub",
    ] {
        fs::create_dir_all(at(dir)).expect("mkdir");
    }
    for file in [
        "inst/app/rcommander.exe",
        "prof/data/notes.txt",
        "home/bob/.ssh/authorized_keys",
        "home/kim/.bashrc",
        "home/bob/docs/a.txt",
        "home/eve/.config/autostart/run.desktop",
        "plain/sub/a.txt",
    ] {
        fs::write(at(file), b"x").expect("write");
    }
    let guard = WriteGuard {
        data_dirs: vec![at("prof/data")],
        program_dirs: vec![at("inst/app")],
    };
    let said = |refusal: rmcp::model::CallToolResult| {
        refusal
            .content
            .first()
            .and_then(|content| content.as_text())
            .map(|text| text.text.clone())
            .unwrap_or_default()
    };

    for (rel, wanted) in [
        // In the place itself.
        ("inst/app", "is or lies in rCommander's program folder"),
        (
            "inst/app/rcommander.exe",
            "is or lies in rCommander's program folder",
        ),
        (
            "inst/app/plugins",
            "is or lies in rCommander's program folder",
        ),
        (
            "prof/data/notes.txt",
            "is or lies in rCommander's data folder",
        ),
        (
            "home/bob/.ssh/authorized_keys",
            "is or lies in an SSH folder",
        ),
        ("home/kim/.bashrc", "is or lies in a shell's start-up file"),
        (
            "home/eve/.config/autostart/run.desktop",
            "is or lies in an autostart folder",
        ),
        // As Windows opens the names; nothing of that name need exist.
        (
            "inst/app./rcommander.exe",
            "is or lies in rCommander's program folder",
        ),
        (
            "prof/data::$DATA/notes.txt",
            "is or lies in rCommander's data folder",
        ),
        // A folder above the place.
        ("inst", "holds rCommander's program folder"),
        ("prof", "holds rCommander's data folder"),
        ("inst.", "holds rCommander's program folder"),
        // A folder holding a place known by its name, found by its entries.
        ("home/bob", "holds an SSH folder"),
        ("home/kim", "holds a shell's start-up file"),
        // Whichever of its places is found first.
        ("home", "holds "),
        ("home/eve", "holds an autostart folder"),
        ("home/eve/.config", "holds an autostart folder"),
    ] {
        let source = VfsPath::local(at(rel));
        let refusal = guard.refuse_taking(&source).await;
        let text = refusal.map(said).unwrap_or_default();
        assert!(text.contains("protected_target"), "{rel}: {text}");
        assert!(text.contains(wanted), "{rel}: {text}");
    }
    // A rename is judged by names alone: the place itself and a folder above the program's or
    // the data folder, never what else lies below the folder.
    for rel in [
        "inst/app",
        "inst/app/rcommander.exe",
        "prof/data",
        "inst",
        "prof",
        "inst.",
    ] {
        let source = VfsPath::local(at(rel));
        assert!(guard.refuse_renaming(&source).await.is_some(), "{rel}");
    }
    for rel in ["home", "home/bob", "home/eve/.config", "plain"] {
        let source = VfsPath::local(at(rel));
        assert!(guard.refuse_renaming(&source).await.is_none(), "{rel}");
    }
    for rel in [
        "plain",
        "plain/sub",
        "plain/sub/a.txt",
        "home/bob/docs",
        "home/bob/docs/a.txt",
        "missing/x.txt",
    ] {
        let source = VfsPath::local(at(rel));
        assert!(guard.refuse_taking(&source).await.is_none(), "{rel}");
    }

    // A move is judged by both ends; a copy by its target alone, as before.
    let out = VfsPath::local(at("plain/out"));
    let program = VfsPath::local(at("inst/app/rcommander.exe"));
    let above = VfsPath::local(at("inst"));
    assert!(guard.refuse_move(&program, &out).await.is_some());
    assert!(guard.refuse_move(&above, &out).await.is_some());
    assert!(guard.refuse_copy(&program, &out).await.is_none());
    assert!(guard.refuse_copy(&above, &out).await.is_none());
    let plain = VfsPath::local(at("plain/sub"));
    assert!(guard.refuse_move(&plain, &out).await.is_none());
}

/// A source reached through a link is judged where it really is.
#[cfg(unix)]
#[tokio::test]
async fn a_source_named_through_a_link_is_judged_where_it_is() {
    use std::os::unix::fs::symlink;

    let root = tempfile::tempdir().expect("temp dir");
    let real = fs::canonicalize(root.path()).expect("real root");
    let at = |rel: &str| real.join(rel);
    fs::create_dir_all(at("inst/app")).expect("mkdir");
    fs::write(at("inst/app/rcommander"), b"program").expect("write");
    symlink(at("inst/app"), at("current")).expect("link to the folder");
    symlink(at("inst"), at("elsewhere")).expect("link above the folder");
    let guard = WriteGuard {
        data_dirs: Vec::new(),
        program_dirs: vec![at("inst/app")],
    };
    for rel in [
        "current/rcommander",
        "elsewhere/app/rcommander",
        "elsewhere",
    ] {
        let source = VfsPath::local(at(rel));
        assert!(guard.refuse_taking(&source).await.is_some(), "{rel}");
    }
}

/// Acceptance (RC-110-19): no tool moves or renames an entry away from the program's folder, the
/// data folder or a place protected by its name, nor a folder that holds one (a rename: one
/// above the program's or the data folder) - written plainly or as Windows opens it; nothing is
/// queued and nothing moves. The same move elsewhere runs,
/// and a copy from these places is queued as before.
#[tokio::test]
async fn no_tool_moves_or_renames_anything_away_from_a_protected_place() {
    let fixture = Fixture::new();
    let at = |rel: &str| fixture.root().join(rel);
    for dir in [
        "prof/data",
        "inst/app",
        "out",
        "home/.ssh",
        "cfg/.config/autostart",
        "repo/.git/hooks",
        "plain",
    ] {
        fs::create_dir_all(at(dir)).expect("mkdir");
    }
    fs::write(at("inst/app/rcommander.exe"), b"program").expect("write");
    fs::write(at("inst/app/readme.txt"), b"r").expect("write");
    fs::write(at("prof/data/notes.txt"), b"n").expect("write");
    fs::write(at("home/.ssh/config"), b"k").expect("write");
    fs::write(at("cfg/.config/autostart/run.desktop"), b"d").expect("write");
    fs::write(at("repo/.git/hooks/pre-commit"), b"h").expect("write");
    fs::write(at("home/.bashrc"), b"b").expect("write");
    fs::write(at("plain/a.txt"), b"a").expect("write");
    fs::write(at("a.txt"), b"a").expect("write");
    // A conflict for the move a person starts below.
    fs::write(at("out/rcommander.exe"), b"other").expect("write");
    let db = Db::open(at("prof/data").join(DB_FILE)).await.expect("db");
    let built = fixture.with_db(db).build().await;
    let root = built.root.path();
    let mut context = built.context;
    context.program_dirs = program_dirs_of(&root.join("inst/app").join("rcommander.exe"));
    let client = connect(&context).await;

    // A rename in `dir`, the panel's own folder when empty.
    let rename = |dir: &str, name: &str, mask: &str| {
        let mut call = json!({ "names": [name], "name_mask": mask, "extension_mask": "[E]" });
        if !dir.is_empty() {
            call["dir"] = json!(dir);
        }
        call
    };
    let calls = [
        // Out of the program's folder, and the folder itself.
        (
            "ops_move",
            json!({ "sources": ["inst/app/readme.txt"], "target": "out" }),
        ),
        (
            "ops_move",
            json!({ "sources": ["inst/app/rcommander.exe"], "target": "out", "rename_mask": "old.bin" }),
        ),
        (
            "ops_move",
            json!({ "sources": ["inst/app"], "target": "out" }),
        ),
        // As Windows opens the folder's name.
        (
            "ops_move",
            json!({ "sources": ["inst/app./readme.txt"], "target": "out" }),
        ),
        // Out of the data folder.
        (
            "ops_move",
            json!({ "sources": ["prof/data/notes.txt"], "target": "out" }),
        ),
        // The folders above them, moved and renamed.
        ("ops_move", json!({ "sources": ["inst"], "target": "out" })),
        ("ops_move", json!({ "sources": ["prof"], "target": "out" })),
        ("ops_move", json!({ "sources": ["inst."], "target": "out" })),
        ("multi_rename_apply", rename("", "inst", "inst-old")),
        ("multi_rename_apply", rename("prof/data", "notes.txt", "n")),
        ("multi_rename_apply", rename("inst", "app", "app-old")),
        (
            "multi_rename_apply",
            rename("inst/app", "readme.txt", "liesmich"),
        ),
        // A place known by its name, moved under another name, and the folder holding it.
        (
            "ops_move",
            json!({ "sources": ["home/.bashrc"], "target": "out", "rename_mask": "x.txt" }),
        ),
        (
            "ops_move",
            json!({ "sources": ["home/.ssh/config"], "target": "out", "rename_mask": "x.txt" }),
        ),
        ("ops_move", json!({ "sources": ["cfg"], "target": "out" })),
        // A folder with a Git repository: its hooks would land below the target.
        ("ops_move", json!({ "sources": ["repo"], "target": "out" })),
    ];
    for (tool, arguments) in calls {
        let answer = call(&client, tool, arguments.clone()).await;
        assert_eq!(
            answer["code"], "protected_target",
            "{tool} {arguments}: {answer}"
        );
    }
    // The data folder and the folder above it hold the database: refused, by whichever guard
    // comes first (the read guard's `sensitive_path` or this one).
    for (dir, name) in [("", "prof"), ("prof", "data")] {
        let answer = call(&client, "multi_rename_apply", rename(dir, name, "old")).await;
        let code = answer["code"].as_str().unwrap_or_default();
        assert!(
            ["protected_target", "sensitive_path"].contains(&code),
            "{name}: {answer}"
        );
    }
    assert!(
        context.engine().ops().jobs().is_empty(),
        "no job was queued"
    );
    for kept in [
        "inst/app/rcommander.exe",
        "inst/app/readme.txt",
        "prof/data/notes.txt",
        "home/.ssh/config",
        "home/.bashrc",
        "cfg/.config/autostart/run.desktop",
    ] {
        assert!(root.join(kept).is_file(), "{kept} stays");
    }

    // The refusal names the place and says what is refused.
    let moved = call(
        &client,
        "ops_move",
        json!({ "sources": ["inst/app/readme.txt"], "target": "out" }),
    )
    .await;
    let said = moved.to_string();
    assert!(said.contains(PROGRAM), "{said}");
    assert!(said.contains("never moves or renames"), "{said}");
    let above = call(
        &client,
        "ops_move",
        json!({ "sources": ["prof"], "target": "out" }),
    )
    .await;
    let said = above.to_string();
    assert!(said.contains("holds rCommander's data folder"), "{said}");

    // A saved transfer list with such a move is not loaded: the move a person started in the
    // window waits on its conflict, is saved, and an agent loads the list.
    let created = call(&client, "queue_create", json!({ "name": "Later" })).await;
    let list = created["lists"][1]["id"].as_u64().expect("the new list");
    let started = context
        .engine()
        .dispatch(Command::OpsMove(TransferRequest {
            panel: None,
            sources: vec!["inst/app/rcommander.exe".into()],
            target: Some("out".into()),
            options: JobOptions::default(),
            rename_mask: None,
        }))
        .await
        .expect("queued");
    let job = match started {
        rc_engine::Outcome::Job(job) => job,
        other => panic!("a job, got {other:?}"),
    };
    call(
        &client,
        "queue_assign",
        json!({ "job": job.id, "list": list }),
    )
    .await;
    let saved = call(&client, "queue_save", json!({ "list": list })).await;
    let id = saved["saved"]["id"].as_i64().expect("the saved id");
    let loaded = call(&client, "queue_load", json!({ "saved": id })).await;
    assert_eq!(loaded["code"], "protected_target", "{loaded}");
    assert!(loaded.to_string().contains(PROGRAM), "{loaded}");
    let _ = context.engine().ops().cancel(job.id);
    assert!(root.join("inst/app/rcommander.exe").is_file());

    // The same move outside every protected place is queued as before, and so is a rename.
    let plain = call(
        &client,
        "ops_move",
        json!({ "sources": ["a.txt"], "target": "out" }),
    )
    .await;
    assert!(plain["id"].as_u64().is_some(), "{plain}");
    let renamed = call(&client, "multi_rename_apply", rename("plain", "a.txt", "b")).await;
    assert!(renamed.get("code").is_none(), "{renamed}");
    // A rename reads no tree: a folder with a Git repository or an autostart folder below it
    // keeps its content under the new name and is renamed like any other.
    for folder in ["repo", "cfg"] {
        let renamed = call(
            &client,
            "multi_rename_apply",
            rename("", folder, &format!("{folder}-new")),
        )
        .await;
        assert!(renamed.get("code").is_none(), "{folder}: {renamed}");
    }
    // A copy from the protected places is not this guard's: queued as before.
    for source in ["inst/app/readme.txt", "prof/data/notes.txt"] {
        let copied = call(
            &client,
            "ops_copy",
            json!({ "sources": [source], "target": "out" }),
        )
        .await;
        assert!(copied["id"].as_u64().is_some(), "{source}: {copied}");
    }
}
