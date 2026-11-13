//! The thumbnails through `apply`, without a window (RC-100-89): Ctrl+Shift+F1, the grid's
//! measure and its steps, Ctrl and the wheel, which tiles are asked for and how a load ends. The
//! harness runs no tasks: the settings are handed over and a load is awaited by hand.

use std::{fs, io::Cursor, path::Path, sync::Arc};

use ::image::{ImageFormat, Rgba, RgbaImage};
use iced::{Size, keyboard::Modifiers, mouse::ScrollDelta};
use rc_engine::{
    Command, ListMode, Outcome, PanelId, PanelState, VfsPath, settings::THUMBNAIL_SIZE_KEY,
};
use rc_vfs::BackendId;
use tokio::sync::Semaphore;

use super::{
    ThumbSettings, ThumbsMsg,
    disk::DiskCache,
    key::{Source, ThumbKey},
    load::{self, Job, Loaded, disk_for},
};
use crate::app::{
    Message, file_list::GRID_CHROME, file_ops::FileOpsMsg, harness::Harness, panel::Pane,
};

fn png(width: u32, height: u32) -> Vec<u8> {
    let image = RgbaImage::from_pixel(width, height, Rgba([20, 120, 220, 255]));
    let mut bytes = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
        .expect("encode");
    bytes
}

/// A folder with `pictures` PNGs, `notes.txt` and the folder `sub`.
fn photos(pictures: usize) -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("temp dir");
    for index in 0..pictures {
        let name = format!("p{index:02}.png");
        fs::write(root.path().join(name), png(300, 150)).expect("written");
    }
    fs::write(root.path().join("notes.txt"), b"text").expect("written");
    fs::create_dir(root.path().join("sub")).expect("folder");
    root
}

/// The grid of the left panel with three tiles a line and two lines on screen.
async fn grid(harness: &mut Harness) {
    harness.press("Ctrl+Shift+F1").await;
    let cell = harness.app.thumbnails.cell();
    let size = Size::new(
        3.0 * cell.width + GRID_CHROME + 1.0,
        2.0 * cell.height + 1.0,
    );
    harness.send(Message::Viewport(PanelId::Left, size)).await;
}

fn settings(remote: bool) -> ThumbSettings {
    ThumbSettings {
        disk_cache: false,
        remote,
        ..ThumbSettings::default()
    }
}

#[tokio::test]
async fn ctrl_shift_f1_shows_the_grid_and_the_arrows_go_by_tiles_and_lines() {
    let root = photos(10);
    let mut harness = Harness::headless(root.path()).await;
    grid(&mut harness).await;
    let pane = harness.app.pane(PanelId::Left);
    assert!(pane.thumbnails());
    assert_eq!((pane.columns(), pane.page_rows()), (3, 2));
    let engine = harness.app.engine().panel(PanelId::Left);
    assert_eq!(engine.mode, ListMode::Thumbnails);
    // `..` is the first tile: a line down is row 3, the third entry.
    harness.press("ArrowDown").await;
    assert_eq!(harness.app.pane(PanelId::Left).cursor(), 2);
    harness.press("ArrowRight").await;
    assert_eq!(harness.app.pane(PanelId::Left).cursor(), 3);
    harness.press("PageDown").await;
    assert_eq!(
        harness.app.pane(PanelId::Left).cursor(),
        6,
        "a page is one line here"
    );
    harness.press("Ctrl+F2").await;
    assert!(!harness.app.pane(PanelId::Left).thumbnails());
}

#[tokio::test]
async fn ctrl_and_the_wheel_zoom_the_tiles_and_the_wheel_alone_scrolls() {
    let root = photos(30);
    let mut harness = Harness::with_db(root.path()).await;
    grid(&mut harness).await;
    let ctrl = |modifiers| Message::FileOps(FileOpsMsg::Modifiers(modifiers));
    harness.send(ctrl(Modifiers::CTRL)).await;
    let up = ScrollDelta::Lines { x: 0.0, y: 1.0 };
    harness.send(Message::Scroll(PanelId::Left, up)).await;
    let engine = harness.app.engine().clone();
    let Ok(Outcome::Settings { settings }) = engine
        .dispatch(Command::GetSettings {
            key: Some(THUMBNAIL_SIZE_KEY.into()),
        })
        .await
    else {
        panic!("the setting");
    };
    assert_eq!(settings[0].value, 128, "a step larger");
    assert_eq!(
        harness.app.pane(PanelId::Left).top(),
        0,
        "no scrolling with Ctrl"
    );

    harness.send(ctrl(Modifiers::empty())).await;
    let down = ScrollDelta::Lines { x: 0.0, y: -1.0 };
    harness.send(Message::Scroll(PanelId::Left, down)).await;
    // A notch is three lines of three tiles.
    assert_eq!(harness.app.pane(PanelId::Left).top(), 9);
}

#[tokio::test]
async fn only_the_pictures_on_screen_are_asked_for_once_the_settings_are_read() {
    let root = photos(10);
    let mut harness = Harness::headless(root.path()).await;
    grid(&mut harness).await;
    let _ = harness.app.request_thumbnails();
    assert_eq!(
        harness.app.thumbnails.cache.len(),
        0,
        "nothing before the settings"
    );
    let read = ThumbsMsg::Settings(settings(false));
    harness.send(Message::Thumbnails(read)).await;
    let _ = harness.app.request_thumbnails();
    // Rows 0 to 11 - two lines on screen, the partly shown third and one below: `..`, `sub`,
    // `notes.txt`, then p00 to p08.
    assert_eq!(harness.app.thumbnails.cache.len(), 9);
}

#[tokio::test]
async fn a_tile_loads_through_the_engine_and_shows_and_one_off_screen_is_skipped() {
    let root = photos(1);
    let mut harness = Harness::headless(root.path()).await;
    grid(&mut harness).await;
    let read = ThumbsMsg::Settings(settings(false));
    harness.send(Message::Thumbnails(read)).await;
    let _ = harness.app.request_thumbnails();
    let dir = harness.app.pane(PanelId::Left).state.path.clone();
    let entry = harness
        .app
        .pane(PanelId::Left)
        .entries()
        .iter()
        .find(|entry| entry.name == "p00.png")
        .cloned()
        .expect("the picture");
    let key = ThumbKey::of(&entry, &dir, 96);
    let job = |key: ThumbKey| Job {
        key,
        source: Source::Picture,
        engine: harness.app.engine().clone(),
        disk: None,
        disk_limit: 0,
        wanted: harness.app.thumbnails.wanted.clone(),
        gate: Arc::new(Semaphore::new(1)),
    };
    let loaded = load::load(job(key.clone())).await;
    assert!(matches!(loaded, Loaded::Tile(_, bytes) if bytes == 96 * 48 * 4));
    let elsewhere = ThumbKey {
        tile: 200,
        ..key.clone()
    };
    let skipped = load::load(job(elsewhere)).await;
    assert!(matches!(skipped, Loaded::Skipped), "not on screen");
    harness
        .send(Message::Thumbnails(ThumbsMsg::Loaded(key, loaded)))
        .await;
    assert!(harness.app.thumbnails.of(&dir, &entry).is_some());
}

/// The left panel as the thumbnails of a folder on an SFTP server.
fn on_server(harness: &mut Harness, local: &Path) {
    let mut state: PanelState = harness.app.engine().panel(PanelId::Left);
    state.path = VfsPath {
        backend: BackendId::Sftp { connection_id: 3 },
        path: local.to_path_buf(),
    };
    state.mode = ListMode::Thumbnails;
    let mut pane = Pane::new(state);
    pane.set_page_rows(2);
    pane.set_tiles(3);
    harness.app.panes[0] = pane;
}

#[tokio::test]
async fn a_server_is_asked_for_no_tile_unless_switched_on_and_never_kept_on_disk() {
    let root = photos(4);
    let mut harness = Harness::headless(root.path()).await;
    on_server(&mut harness, root.path());
    let read = ThumbsMsg::Settings(settings(false));
    harness.send(Message::Thumbnails(read)).await;
    let _ = harness.app.request_thumbnails();
    assert_eq!(
        harness.app.thumbnails.cache.len(),
        0,
        "no load: not a byte is read for a thumbnail"
    );
    let read = ThumbsMsg::Settings(settings(true));
    harness.send(Message::Thumbnails(read)).await;
    let _ = harness.app.request_thumbnails();
    assert_eq!(harness.app.thumbnails.cache.len(), 4, "the pictures");

    let cache = tempfile::tempdir().expect("temp dir");
    let disk = Some(DiskCache::at(cache.path()));
    let entry = harness.app.pane(PanelId::Left).entries()[1].clone();
    let remote = ThumbKey::of(&entry, &harness.app.pane(PanelId::Left).state.path, 96);
    assert_eq!(disk_for(disk.clone(), &remote), None);
    let local = ThumbKey::of(&entry, &VfsPath::local(root.path()), 96);
    assert_eq!(disk_for(disk.clone(), &local), disk);
}
