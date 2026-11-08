use std::{fs, path::PathBuf, sync::Mutex};

use crate::test_support;
use crate::{Command, CommandClass, CursorTarget, Engine, Outcome};

/// What `launch` was handed, by every test of the crate.
pub(super) static LAUNCHED: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

fn launched(file: &PathBuf) -> bool {
    LAUNCHED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .contains(file)
}

async fn open(engine: &Engine, path: Option<&str>) -> Result<Outcome, crate::EngineError> {
    engine
        .dispatch(Command::OpenFile {
            panel: None,
            path: path.map(str::to_owned),
        })
        .await
}

#[tokio::test]
async fn a_file_goes_to_the_system_and_a_directory_into_the_panel() {
    let root = tempfile::tempdir().expect("temp dir");
    fs::create_dir(root.path().join("sub")).expect("mkdir");
    fs::write(root.path().join("doc.txt"), b"x").expect("write");
    fs::write(root.path().join("sub").join("tool.exe"), b"x").expect("write");
    let engine = test_support::headless(root.path()).await;

    engine
        .dispatch(Command::SetCursor {
            panel: None,
            target: CursorTarget::Name("doc.txt".into()),
        })
        .await
        .expect("cursor");
    let Outcome::Panel(stays) = open(&engine, None).await.expect("opens") else {
        panic!("a panel");
    };
    assert_eq!(stays.path.path, root.path(), "a file moves no panel");
    assert!(launched(&root.path().join("doc.txt")));

    let Outcome::Panel(inside) = open(&engine, Some("sub")).await.expect("enters") else {
        panic!("a panel");
    };
    assert_eq!(inside.path.path, root.path().join("sub"));
    open(&engine, Some("tool.exe")).await.expect("starts");
    assert!(launched(&root.path().join("sub").join("tool.exe")));

    let missing = open(&engine, Some("../gone.txt")).await;
    assert_eq!(
        missing.map(|_| ()).map_err(|error| error.code()),
        Err("not_found")
    );
}

#[test]
fn opening_runs_something_so_it_is_an_exec_command() {
    let open = Command::OpenFile {
        panel: None,
        path: None,
    };
    assert_eq!(open.spec().class, CommandClass::Exec);
}

/// A folder opens by its path as it is, also below a folder whose name is no UTF-8 (Linux allows
/// any bytes but `/`; macOS refuses such a name): it never goes through text (RC-080-50).
#[cfg(target_os = "linux")]
#[tokio::test]
async fn a_folder_below_a_name_that_is_no_utf8_opens() {
    use std::{ffi::OsStr, os::unix::ffi::OsStrExt as _};

    let root = tempfile::tempdir().expect("temp dir");
    let odd = root.path().join(OsStr::from_bytes(b"caf\xe9"));
    fs::create_dir_all(odd.join("sub")).expect("mkdir");
    let engine = test_support::headless(root.path()).await;
    engine
        .show_path(crate::PanelId::Left, rc_vfs::VfsPath::local(&odd))
        .await
        .expect("shown");
    let Outcome::Panel(inside) = open(&engine, Some("sub")).await.expect("enters") else {
        panic!("a panel");
    };
    assert_eq!(inside.path.path, odd.join("sub"));
}

/// What `file.open` was given by the internal associations (RC-100-91): the rules stored, the
/// switch set, the cursor on `name`.
mod associations {
    use std::path::Path;

    use serde_json::json;

    use super::{launched, open};
    use crate::{
        Command, CursorTarget, Engine,
        associations::{FileAssociation, write_rules},
        recover::LockOrRecover as _,
        settings::{ASSOCIATIONS_ENABLED_KEY, FILE_ASSOCIATIONS_KEY},
        test_support,
        user_commands::{CommandRef, Launch, STARTED},
    };

    fn rule(mask: &str, program: &str) -> FileAssociation {
        FileAssociation {
            mask: mask.into(),
            open: Some(CommandRef {
                command: program.into(),
                params: "-o %1".into(),
                ..CommandRef::default()
            }),
            ..FileAssociation::default()
        }
    }

    async fn store(engine: &Engine, rules: &[FileAssociation], enabled: bool) {
        engine
            .set_protected_setting(FILE_ASSOCIATIONS_KEY, &json!(write_rules(rules)))
            .await
            .expect("rules stored");
        engine
            .set_protected_setting(ASSOCIATIONS_ENABLED_KEY, &json!(enabled))
            .await
            .expect("switch stored");
    }

    async fn enter_on(engine: &Engine, name: &str) {
        engine
            .dispatch(Command::SetCursor {
                panel: None,
                target: CursorTarget::Name(name.into()),
            })
            .await
            .expect("cursor");
        open(engine, None).await.expect("opened");
    }

    fn started(program: &str) -> Vec<Launch> {
        STARTED
            .lock_or_recover("test")
            .iter()
            .filter(|launch| launch.program == program)
            .cloned()
            .collect()
    }

    /// The file's full path as `%1` hands it to the program, after `-o`.
    fn args_for(root: &Path, name: &str) -> Vec<String> {
        let path = format!(
            "{}{}{name}",
            root.to_string_lossy(),
            std::path::MAIN_SEPARATOR
        );
        if cfg!(windows) {
            vec![format!("-o \"{path}\"")]
        } else {
            vec!["-o".to_owned(), path]
        }
    }

    #[tokio::test]
    async fn first_matching_rule_opens() {
        let (root, engine) = test_support::engine_with_db(&[("app.log", "x")]).await;
        let rules = [
            rule("*.log", "assoc-first-test"),
            rule("*.*", "assoc-second-test"),
        ];
        store(&engine, &rules, true).await;
        enter_on(&engine, "app.log").await;
        let launch = started("assoc-first-test")
            .pop()
            .expect("the first rule ran");
        assert_eq!(launch.args, args_for(root.path(), "app.log"));
        assert!(started("assoc-second-test").is_empty());
        assert!(!launched(&root.path().join("app.log")), "not the system");
    }

    #[tokio::test]
    async fn no_rule_falls_back_to_the_system() {
        let (root, engine) = test_support::engine_with_db(&[("readme.md", "x")]).await;
        store(&engine, &[rule("*.log", "assoc-none-test")], true).await;
        enter_on(&engine, "readme.md").await;
        assert!(started("assoc-none-test").is_empty());
        assert!(launched(&root.path().join("readme.md")));
    }

    /// `*.*` leaves an archive to open as a folder (or to the system); `*.zip` takes it.
    #[tokio::test]
    async fn an_archive_needs_its_extension_named() {
        let (root, engine) = test_support::engine_with_db(&[("pack.zip", "no zip")]).await;
        store(&engine, &[rule("*.*", "assoc-every-test")], true).await;
        enter_on(&engine, "pack.zip").await;
        assert!(started("assoc-every-test").is_empty());
        store(&engine, &[rule("*.zip", "assoc-zip-test")], true).await;
        enter_on(&engine, "pack.zip").await;
        let launch = started("assoc-zip-test").pop().expect("the named rule ran");
        assert_eq!(launch.args, args_for(root.path(), "pack.zip"));
    }

    #[tokio::test]
    async fn switched_off_uses_the_system() {
        let (root, engine) = test_support::engine_with_db(&[("off.log", "x")]).await;
        store(&engine, &[rule("*.log", "assoc-off-test")], false).await;
        enter_on(&engine, "off.log").await;
        assert!(started("assoc-off-test").is_empty());
        assert!(launched(&root.path().join("off.log")));
    }
}
