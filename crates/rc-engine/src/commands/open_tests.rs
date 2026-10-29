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
