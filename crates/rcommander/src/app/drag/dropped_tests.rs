use super::*;

fn local(path: &str) -> VfsPath {
    VfsPath::local(path)
}

fn ftp(path: &str) -> VfsPath {
    VfsPath {
        backend: BackendId::Ftp { connection_id: 1 },
        path: PathBuf::from(path),
    }
}

fn paths(texts: &[&str]) -> Vec<PathBuf> {
    texts.iter().map(PathBuf::from).collect()
}

#[test]
fn plain_absolute_paths_are_taken_once_in_order() {
    let (taken, refused) = accept(paths(&["/data/b", "/data/a", "/data/b"]), false);
    assert_eq!(taken, paths(&["/data/b", "/data/a"]));
    assert_eq!(refused, 1);
}

#[test]
fn relative_dotted_and_empty_paths_are_refused() {
    let (taken, refused) = accept(
        paths(&["data/a", "/data/../etc/passwd", "/data/./a", "", "/ok"]),
        false,
    );
    assert_eq!(taken, paths(&["/ok"]));
    assert_eq!(refused, 4);
}

#[test]
fn a_nul_or_an_overlong_path_is_refused() {
    let long = format!("/{}", "x".repeat(MAX_PATH_CHARS));
    let (taken, refused) = accept(
        vec![
            PathBuf::from("/a\0b"),
            PathBuf::from(long),
            PathBuf::from("/ok"),
        ],
        false,
    );
    assert_eq!(taken, paths(&["/ok"]));
    assert_eq!(refused, 2);
}

#[test]
fn at_most_max_paths_are_taken() {
    let many: Vec<PathBuf> = (0..=MAX_PATHS)
        .map(|n| PathBuf::from(format!("/f{n}")))
        .collect();
    let (taken, refused) = accept(many, false);
    assert_eq!(taken.len(), MAX_PATHS);
    assert_eq!(refused, 1);
}

#[test]
fn windows_takes_drives_and_shares_only() {
    for path in [
        r"C:\data\a.txt",
        r"c:/data/a.txt",
        r"\\server\share\a.txt",
        r"\\?\C:\long\a.txt",
        r"\\?\UNC\server\share\a.txt",
    ] {
        assert!(plain_on_windows(path), "{path} is refused");
    }
    for path in [
        r"\\.\PhysicalDrive0",
        r"//./pipe/x",
        r"\\?\GLOBALROOT\Device\HarddiskVolume1\a",
        r"\\server",
        r"\\server\",
        r"C:\data\a.txt:hidden",
        r"\\server\share\a.txt:stream",
        r"C:relative",
        r"\rooted\no\drive",
    ] {
        assert!(!plain_on_windows(path), "{path} is taken");
    }
}

#[test]
fn a_local_other_panel_copies_into_the_target_whatever_its_backend() {
    let command = transfer(
        PanelId::Right,
        &ftp("/upload"),
        &local("/home"),
        paths(&["/data/a"]),
        false,
    )
    .expect("a copy");
    assert_eq!(
        command,
        Command::OpsCopy(TransferRequest {
            panel: Some(PanelId::Left),
            sources: vec!["/data/a".into()],
            target: None,
            options: JobOptions::default(),
        })
    );
}

#[test]
fn shift_moves() {
    let command = transfer(
        PanelId::Left,
        &local("/out"),
        &local("/home"),
        paths(&["/data/a"]),
        true,
    )
    .expect("a move");
    assert!(matches!(command, Command::OpsMove(request) if request.panel == Some(PanelId::Right)));
}

#[test]
fn a_remote_other_panel_copies_with_the_target_panels_own_command() {
    let command = transfer(
        PanelId::Left,
        &local("/out"),
        &ftp("/remote"),
        paths(&["/data/a", "/data/b"]),
        false,
    )
    .expect("a copy");
    assert_eq!(
        command,
        Command::OpsCopyTo {
            panel: Some(PanelId::Left),
            sources: vec!["/data/a".into(), "/data/b".into()],
            target: into_folder(Path::new("/out")),
            options: JobOptions::default(),
            queue: None,
        }
    );
}

#[test]
fn two_remote_panels_refuse() {
    let refused = transfer(
        PanelId::Left,
        &ftp("/a"),
        &ftp("/b"),
        paths(&["/data/a"]),
        false,
    );
    assert_eq!(refused, Err(Refusal::NoLocalPanel));
}

#[test]
fn files_dropped_where_they_lie_are_left_out() {
    let refused = transfer(
        PanelId::Left,
        &local("/data"),
        &local("/home"),
        paths(&["/data/a"]),
        false,
    );
    assert_eq!(refused, Err(Refusal::Nothing));
    let command = transfer(
        PanelId::Left,
        &local("/data"),
        &local("/home"),
        paths(&["/data/a", "/other/b"]),
        false,
    )
    .expect("a copy");
    assert!(matches!(command, Command::OpsCopy(request) if request.sources == ["/other/b"]));
}

#[test]
fn the_target_ends_in_a_separator() {
    assert!(into_folder(Path::new("/out")).ends_with(MAIN_SEPARATOR));
    assert_eq!(into_folder(Path::new("/")), "/");
}

/// RC-080-38: the divider, not the window's middle, splits a drop; a maximized panel takes it.
#[test]
fn a_drop_lands_on_the_side_of_the_divider_or_in_the_maximized_panel() {
    assert_eq!(target_panel(Some(0.45), 500, None), Some(PanelId::Left));
    assert_eq!(target_panel(Some(0.45), 300, None), Some(PanelId::Right));
    assert_eq!(target_panel(Some(0.65), 700, None), Some(PanelId::Left));
    assert_eq!(target_panel(Some(0.75), 700, None), Some(PanelId::Right));
    assert_eq!(
        target_panel(Some(0.9), 500, Some(PanelId::Left)),
        Some(PanelId::Left)
    );
    assert_eq!(
        target_panel(None, 500, Some(PanelId::Right)),
        Some(PanelId::Right)
    );
    assert_eq!(target_panel(None, 500, None), None);
}
