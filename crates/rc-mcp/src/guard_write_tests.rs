use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use rc_db::{BACKUP_DIR, DB_FILE, Db};
use rc_engine::JobState;
use serde_json::{Value, json};
use tempfile::TempDir;

#[cfg(unix)]
use super::WriteGuard;
use super::{folder_names, judge, protected_target};
use crate::McpContext;
use crate::test_support::{Fixture, call, connect};

/// The Startup folder of a Windows user, below a test's root.
const STARTUP: &str = "AppData/Roaming/Microsoft/Windows/Start Menu/Programs/Startup";

fn path(parts: &[&str]) -> PathBuf {
    parts.iter().collect()
}

#[test]
fn rcommander_s_own_places_are_protected() {
    let data = path(&["home", "bob", ".local", "share", "rcommander"]);
    let dirs = [data.clone()];
    for at in [
        data.clone(),
        data.join("notes.txt"),
        data.join(BACKUP_DIR),
        data.join(BACKUP_DIR).join("2026.zip"),
        data.join("rcommander.db.restore"),
        data.join("rcommander.db.restore-mark"),
        data.join("anything.restore.sha256"),
        path(&["tmp", "rcommander.db.restore"]),
        path(&["tmp", "RCommander.DB.Restore"]),
        path(&["tmp", "rcommander.db.restore-mark"]),
        path(&["tmp", "rcommander.db.restore.digest"]),
        path(&["tmp", "rcommander.db.restore-rejected"]),
        path(&["tmp", "rcommander.db.restore", "inside.txt"]),
        path(&["tmp", "mcp-instance.json"]),
    ] {
        assert!(judge(&at, &dirs).is_some(), "{at:?}");
    }
    for at in [
        path(&["home", "bob", ".local", "share", "notes.txt"]),
        path(&["home", "bob", ".local", "share", "rcommander-old", "x"]),
    ] {
        assert_eq!(judge(&at, &dirs), None, "{at:?}");
    }
}

/// The folders an update keeps beside the program are no target (RC-100-06).
#[test]
fn the_update_folders_beside_the_program_are_protected() {
    for parts in [
        &["opt", "rcommander", ".previous"][..],
        &["opt", "rcommander", ".previous", "rcommander.exe"],
        &["opt", "rcommander", ".trash", "20261029T000000Z-0"],
        &["opt", "rcommander", ".update-1.2.0", "rcommander.exe"],
        &["opt", "rcommander", ".FAILED-1.2.0"],
    ] {
        assert_eq!(
            judge(&path(parts), &[]),
            Some("an update's own folder"),
            "{parts:?}"
        );
    }
    assert_eq!(judge(&path(&["opt", "rcommander", "previous"]), &[]), None);
}

#[test]
fn ssh_autostart_and_shell_start_up_files_are_protected() {
    for parts in [
        &["home", "bob", ".ssh"][..],
        &["home", "bob", ".ssh", "authorized_keys"],
        &["home", "bob", ".SSH", "config.d", "work"],
        &[
            "Users",
            "Bob",
            "AppData",
            "Roaming",
            "Microsoft",
            "Windows",
            "Start Menu",
            "Programs",
            "Startup",
        ],
        &[
            "ProgramData",
            "Microsoft",
            "Windows",
            "Start Menu",
            "Programs",
            "StartUp",
            "run.bat",
        ],
        &["home", "bob", ".config", "autostart", "run.desktop"],
        &["home", "bob", ".bashrc"],
        &["home", "bob", ".ZSHRC"],
        &["home", "bob", ".profile"],
        &[
            "Users",
            "Bob",
            "Documents",
            "WindowsPowerShell",
            "Microsoft.PowerShell_profile.ps1",
        ],
        &["Users", "Bob", "Documents", "PowerShell", "profile.ps1"],
    ] {
        assert!(protected_target(&path(parts)).is_some(), "{parts:?}");
    }
    // The owner's list of 2026-10-28 (RC-080-42): what a tool runs or obeys on its own.
    for (parts, reason) in [
        (
            &["home", "bob", "src", "app", ".git", "hooks", "pre-commit"][..],
            "Git's hooks",
        ),
        (
            &["home", "bob", "src", "app", ".git", "hooks"][..],
            "Git's hooks",
        ),
        (&["home", "bob", ".gitconfig"][..], "Git's configuration"),
        (&["Users", "Bob", ".GitConfig"][..], "Git's configuration"),
        (
            &["home", "bob", ".config", "fish", "config.fish"][..],
            "a shell's start-up file",
        ),
        (
            &["home", "bob", ".bash_logout"][..],
            "a shell's start-up file",
        ),
        (
            &["home", "bob", ".config", "systemd", "user", "x.service"][..],
            "an autostart folder",
        ),
        (
            &["home", "bob", "src", "app", ".vscode", "tasks.json"][..],
            "an editor's tasks",
        ),
    ] {
        assert_eq!(protected_target(&path(parts)), Some(reason), "{parts:?}");
    }
    for parts in [
        &["home", "bob", "notes.txt"][..],
        &["home", "bob", "Start Menu", "notes.txt"],
        &["Users", "Bob", "Start Menu", "Programs", "Games", "x.lnk"],
        &["home", "bob", "startup", "run.bat"],
        &["home", "bob", ".config", "other", "x"],
        &["home", "bob", "bashrc.txt"],
        &["home", "bob", "ssh", "x"],
        &["home", "bob", "src", "app", ".git", "config.txt"],
        &["home", "bob", "src", "app", "hooks", "pre-commit"],
        &["home", "bob", ".config", "fish", "functions", "x.fish"],
        &["home", "bob", ".config", "systemd", "system.conf"],
        &["home", "bob", "src", "app", ".vscode", "settings.json"],
    ] {
        assert_eq!(protected_target(&path(parts)), None, "{parts:?}");
    }
}

/// Windows opens `.ssh.` as `.ssh` and `.bashrc ` as `.bashrc`.
#[test]
fn names_are_judged_as_windows_opens_them() {
    for parts in [
        &["home", "bob", ".ssh.", "authorized_keys"][..],
        &["home", "bob", ".bashrc "],
        &["x", "Start Menu", "Programs", "Startup.", "run.bat"],
        &["x", ".config", "autostart:stream"],
    ] {
        assert!(protected_target(&path(parts)).is_some(), "{parts:?}");
    }
}

#[test]
fn an_archive_s_own_folder_is_judged_under_every_name() {
    assert_eq!(
        folder_names("site.tar.gz"),
        ["site.tar.gz", "site.tar", "site"]
    );
    assert_eq!(folder_names(".ssh.zip"), [".ssh.zip", ".ssh"]);
    assert_eq!(folder_names("plain"), ["plain"]);
}

/// A harmless name that leads into a protected folder is judged where it leads, also for a
/// file that does not exist yet.
#[cfg(unix)]
#[tokio::test]
async fn a_link_into_a_protected_place_is_judged_where_it_leads() {
    let root = tempfile::tempdir().expect("temp dir");
    fs::create_dir_all(root.path().join("home").join(".ssh")).expect("mkdir");
    let link = root.path().join("keys");
    std::os::unix::fs::symlink(root.path().join("home").join(".ssh"), &link).expect("link");
    let guard = WriteGuard {
        data_dirs: Vec::new(),
    };
    let planted = rc_engine::VfsPath::local(link.join("authorized_keys"));
    assert_eq!(guard.refuses(&planted).await, Some("an SSH folder"));
    let plain = rc_engine::VfsPath::local(root.path().join("plain").join("x.txt"));
    assert_eq!(guard.refuses(&plain).await, None);
}

/// `root/` with the server's database in `data/`, `a.txt`, the split part `p.001`, an empty SSH
/// folder `home/.ssh`, an empty Startup folder and `out/`; both panels on `root`.
async fn setup() -> (TempDir, McpContext) {
    let fixture = Fixture::new();
    let at = |rel: &str| fixture.root().join(rel);
    for dir in ["data", "home/.ssh", STARTUP, "out"] {
        fs::create_dir_all(at(dir)).expect("mkdir");
    }
    fs::write(at("a.txt"), b"a").expect("write");
    fs::write(at("p.001"), b"p").expect("write");
    let db = Db::open(at("data").join(DB_FILE)).await.expect("db");
    let built = fixture.with_db(db).build().await;
    (built.root, built.context)
}

/// Waits for the job an answer names.
async fn finished(context: &McpContext, answer: &Value) -> JobState {
    let id = answer["id"]
        .as_u64()
        .unwrap_or_else(|| panic!("a job: {answer}"));
    let wait = context.engine().ops().wait(id);
    tokio::time::timeout(Duration::from_secs(20), wait)
        .await
        .expect("in time")
        .expect("known")
        .state
}

/// The names in the folder `dir`.
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

/// Acceptance (RC-080-09): every write tool refuses `rcommander.db.restore`, the data folder,
/// the autostart folder and `.ssh` as its target, and writes nothing there. A name the read
/// guard knows (`.ssh`, rCommander's file names) may be refused by it first.
#[tokio::test]
async fn every_write_tool_refuses_every_protected_place() {
    let (root, context) = setup().await;
    let client = connect(&context).await;
    // What is protected, a folder there and a file there.
    let places = [
        (
            "restore",
            "out/rcommander.db.restore".to_owned(),
            "out/rcommander.db.restore".to_owned(),
        ),
        ("data", "data".to_owned(), "data/x.zip".to_owned()),
        (
            "autostart",
            STARTUP.to_owned(),
            format!("{STARTUP}/run.lnk"),
        ),
        (
            "ssh",
            "home/.ssh".to_owned(),
            "home/.ssh/authorized_keys".to_owned(),
        ),
    ];
    for (what, folder, file) in &places {
        let (parent, name) = file.rsplit_once('/').expect("a folder and a name");
        let calls = [
            (
                "ops_copy",
                json!({ "sources": ["a.txt"], "target": folder }),
            ),
            (
                "ops_move",
                json!({ "sources": ["a.txt"], "target": folder }),
            ),
            ("ops_mkdir", json!({ "path": folder })),
            (
                "archives_pack",
                json!({ "sources": ["a.txt"], "target": file }),
            ),
            (
                "archives_extract",
                json!({ "archives": ["a.zip"], "target": folder }),
            ),
            (
                "files_combine",
                json!({ "path": "p.001", "target_dir": parent, "target_name": name }),
            ),
            (
                "files_checksum_create",
                json!({ "names": ["a.txt"], "algorithm": "md5", "dir": parent, "file_name": name }),
            ),
            (
                "files_split",
                json!({ "path": "a.txt", "part_size": 1, "target_dir": folder }),
            ),
            (
                "files_set_attributes",
                json!({ "paths": [folder], "readonly": true, "recursive": true }),
            ),
            ("files_set_comment", json!({ "path": file, "comment": "x" })),
            (
                "files_create_link",
                json!({ "target": "a.txt", "link": file }),
            ),
        ];
        for (tool, arguments) in calls {
            let answer = call(&client, tool, arguments).await;
            let code = answer["code"].as_str().unwrap_or_default();
            // The read guard does not know the data folder or the autostart folder.
            let codes: &[&str] = if matches!(*what, "data" | "autostart") {
                &["protected_target"]
            } else {
                &["protected_target", "sensitive_path"]
            };
            assert!(codes.contains(&code), "{tool} into {what}: {answer}");
        }
    }
    assert!(
        context.engine().ops().jobs().is_empty(),
        "no job was queued"
    );
    assert!(!root.path().join("out/rcommander.db.restore").exists());
    assert_eq!(names_in(&root.path().join(STARTUP)), Vec::<String>::new());
    assert_eq!(
        names_in(&root.path().join("home/.ssh")),
        Vec::<String>::new()
    );
    let data = names_in(&root.path().join("data"));
    assert!(
        data.iter().all(|name| name.starts_with(DB_FILE)),
        "only the database is in its folder: {data:?}"
    );
    assert!(root.path().join("a.txt").is_file(), "nothing was moved");
}

/// What a copy or an unpack writes below a harmless target is judged too: a folder `Startup`
/// copied, unpacked or synchronized into `Programs`, an archive unpacked into a folder of its
/// own named `.ssh`, a combined file named by its `.crc` file, a folder renamed to `autostart`.
#[tokio::test]
async fn what_lands_below_a_harmless_target_is_judged() {
    let (root, context) = setup().await;
    let client = connect(&context).await;
    let at = |rel: &str| root.path().join(rel);
    let programs = STARTUP.rsplit_once('/').expect("parent").0;
    fs::create_dir_all(at("tree/Startup")).expect("mkdir");
    fs::write(at("tree/Startup/run.bat"), b"run").expect("write");

    let copied = call(
        &client,
        "ops_copy",
        json!({ "sources": ["tree/Startup"], "target": programs }),
    )
    .await;
    assert_eq!(copied["code"], "protected_target", "{copied}");

    let packed = call(
        &client,
        "archives_pack",
        json!({ "sources": ["tree/Startup"], "target": "startup.zip" }),
    )
    .await;
    assert_eq!(finished(&context, &packed).await, JobState::Completed);
    let unpacked = call(
        &client,
        "archives_extract",
        json!({ "archives": ["startup.zip"], "target": programs }),
    )
    .await;
    assert_eq!(unpacked["code"], "protected_target", "{unpacked}");

    let packed = call(
        &client,
        "archives_pack",
        json!({ "sources": ["a.txt"], "target": ".ssh.zip" }),
    )
    .await;
    assert_eq!(finished(&context, &packed).await, JobState::Completed);
    let own = call(
        &client,
        "archives_extract",
        json!({ "archives": [".ssh.zip"], "target": "out", "own_folder": true }),
    )
    .await;
    assert_eq!(own["code"], "protected_target", "{own}");

    fs::write(at("q.001"), b"q").expect("write");
    fs::write(
        at("q.crc"),
        "filename=.bashrc\r\nsize=1\r\ncrc32=00000000\r\n",
    )
    .expect("crc");
    let combined = call(
        &client,
        "files_combine",
        json!({ "path": "q.001", "target_dir": "out" }),
    )
    .await;
    assert_eq!(combined["code"], "protected_target", "{combined}");

    fs::create_dir_all(at(".config/autostart-off")).expect("mkdir");
    let renamed = call(
        &client,
        "multi_rename_apply",
        json!({
            "dir": ".config",
            "names": ["autostart-off"],
            "name_mask": "autostart",
            "extension_mask": "[E]",
        }),
    )
    .await;
    assert_eq!(renamed["code"], "protected_target", "{renamed}");

    // A synchronization copies either tree into the other.
    let synced = call(
        &client,
        "dirs_sync_apply",
        json!({ "left": "tree", "right": programs }),
    )
    .await;
    assert_eq!(synced["code"], "protected_target", "{synced}");

    assert_eq!(names_in(&at(STARTUP)), Vec::<String>::new());
    assert!(!at("out/.ssh").exists() && !at("out/.bashrc").exists());
    assert!(!at(".config/autostart").exists());
}

/// The guard refuses, it does not ask: a harmless target is still written without a question.
#[tokio::test]
async fn a_harmless_target_is_written_as_before() {
    let (root, context) = setup().await;
    let client = connect(&context).await;
    let copied = call(
        &client,
        "ops_copy",
        json!({ "sources": ["a.txt"], "target": "out" }),
    )
    .await;
    assert_eq!(finished(&context, &copied).await, JobState::Completed);
    assert!(root.path().join("out/a.txt").is_file());
}
