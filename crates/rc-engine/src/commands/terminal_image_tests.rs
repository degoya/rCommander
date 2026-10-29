//! A clipboard image pasted as a path (RC-040-08): where it goes for each kind of terminal, how
//! the path is pasted, the sweep, and a local image written and removed with its terminal.

use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use rc_vfs::{BackendId, VfsPath};

use super::{Place, file_name, placement, quoted, sweep};
use crate::ConnectionId;

#[test]
fn a_local_shell_reads_the_temp_folder_and_a_server_its_tmp() {
    let temp = Path::new("/var/tmp");
    let name = file_name(0x0123_4567_89ab_cdef_0123_4567_89ab_cdef);
    assert_eq!(name, "rcommander-clip-0123456789abcdef0123456789abcdef.png");
    let name = "rcommander-clip-7-3.png";

    let local = placement(Place::Local, temp, name);
    let file = PathBuf::from("/var/tmp/rcommander/rcommander-clip-7-3.png");
    assert_eq!(local.file, VfsPath::local(file.clone()));
    assert_eq!(local.shown, file.display().to_string());

    let server = placement(Place::Server(ConnectionId(4)), temp, name);
    assert_eq!(server.file.backend, BackendId::Sftp { connection_id: 4 });
    assert_eq!(
        server.file.path,
        PathBuf::from("/tmp/rcommander-clip-7-3.png")
    );
    assert_eq!(server.shown, "/tmp/rcommander-clip-7-3.png");
}

#[test]
fn wsl_reads_the_windows_file_under_mnt() {
    let temp = Path::new(r"C:\Users\Ann Lee\AppData\Local\Temp");
    let wsl = placement(Place::Wsl, temp, "rcommander-clip-1-1.png");
    assert_eq!(wsl.file.backend, BackendId::Local);
    if cfg!(windows) {
        assert_eq!(
            wsl.shown,
            "/mnt/c/Users/Ann Lee/AppData/Local/Temp/rcommander/rcommander-clip-1-1.png"
        );
    }
}

#[test]
fn a_path_with_a_space_is_quoted() {
    assert_eq!(quoted("/tmp/a.png"), "/tmp/a.png");
    assert_eq!(
        quoted(r"C:\Users\Ann Lee\a.png"),
        r#""C:\Users\Ann Lee\a.png""#
    );
}

#[tokio::test]
async fn the_sweep_removes_only_old_images() {
    let folder = tempfile::tempdir().expect("temp dir");
    for name in ["rcommander-clip-1-1.png", "other.png"] {
        fs::write(folder.path().join(name), b"png").expect("write");
    }
    sweep(folder.path(), SystemTime::now()).await;
    assert!(
        folder.path().join("rcommander-clip-1-1.png").exists(),
        "new"
    );

    let later = SystemTime::now() + Duration::from_secs(2 * 24 * 60 * 60);
    sweep(folder.path(), later).await;
    assert!(
        !folder.path().join("rcommander-clip-1-1.png").exists(),
        "old"
    );
    assert!(folder.path().join("other.png").exists(), "not ours");
}

#[cfg(unix)]
mod with_a_shell {
    use std::time::Duration;

    use crate::test_support::{self, expect_terminal};
    use crate::{Command, EngineEvent};

    /// The image lands in the temp folder under the path answered, `terminal.paste_image`
    /// announces the paste for the panel's terminal, and closing the terminal removes the file.
    #[tokio::test]
    async fn an_image_is_written_announced_and_removed_with_its_terminal() {
        let (_root, engine) = test_support::engine(&[("notes.txt", "n")]).await;
        let open = Command::OpenTerminal {
            panel: None,
            shell: Some("sh".into()),
            distro: None,
            cwd: None,
            size: None,
        };
        let terminal = expect_terminal(engine.dispatch(open).await.expect("opened")).terminal;

        let mut events = engine.subscribe();
        engine
            .dispatch(Command::PasteImage { terminal: None })
            .await
            .expect("announced");
        let mut announced = false;
        while let Ok(event) = events.try_recv() {
            announced |= event == EngineEvent::PasteImage { terminal };
        }
        assert!(announced);

        let path = engine
            .place_terminal_image(terminal, b"\x89PNG".to_vec())
            .await
            .expect("placed");
        let file = std::path::PathBuf::from(path.trim_matches('"'));
        assert_eq!(std::fs::read(&file).expect("written"), b"\x89PNG");

        engine
            .dispatch(Command::CloseTerminal { terminal })
            .await
            .expect("closed");
        for _ in 0..200 {
            if !file.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(!file.exists(), "removed with its terminal");
        let gone = engine.place_terminal_image(terminal, Vec::new()).await;
        assert_eq!(
            gone.map_err(|error| error.code()),
            Err("terminal_not_found")
        );
    }
}
