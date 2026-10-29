//! RC-060-02: a listing that runs long shows its first entries, the rest arrives in the panel's
//! order, and a panel that moves on ends it.

use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::SystemTime,
};

use async_trait::async_trait;
use rc_vfs::{
    Attributes, BackendId, Capabilities, EntryKind, ReadStream, SetAttributes, WriteMode,
    WriteStream,
};
use tokio::sync::{Semaphore, mpsc::Sender};

use super::*;
use crate::Outcome;
use crate::test_support;

/// Batches of [`Trickle::BATCH`] files, the later ones sorting first; the last one also holds
/// the folder `sub`.
const BATCHES: usize = 3;

/// How a [`Trickle`] listing goes on once the gate opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ending {
    /// The other batches, then the end.
    Complete,
    /// `permission_denied` instead of the second batch.
    Fails,
    /// The other batches, then one more after another until nobody listens.
    Endless,
}

/// A server folder `/big` that sends its first batch at once and the rest when the test opens
/// the gate.
struct Trickle {
    gate: Arc<Semaphore>,
    /// The listing found its receiver gone.
    stopped: Arc<AtomicBool>,
    ending: Ending,
}

impl Trickle {
    const BATCH: usize = 50;

    fn batch(index: usize) -> Vec<Entry> {
        let first = (BATCHES - 1 - index) * Self::BATCH;
        let mut batch: Vec<Entry> = (first..first + Self::BATCH)
            .map(|number| entry(&format!("f{number:03}"), EntryKind::File))
            .collect();
        if index == BATCHES - 1 {
            batch.push(entry("sub", EntryKind::Dir));
        }
        batch
    }
}

fn entry(name: &str, kind: EntryKind) -> Entry {
    Entry {
        name: name.to_owned(),
        kind,
        size: 1,
        modified: None,
        created: None,
        attributes: Attributes::default(),
        hidden: false,
        owner: None,
    }
}

fn unsupported<T>(path: &Path) -> rc_vfs::Result<T> {
    Err(VfsError::Unsupported {
        path: path.to_path_buf(),
        operation: "anything but listing",
    })
}

#[async_trait]
impl Vfs for Trickle {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            rename_across_dirs: false,
            symlinks: false,
            case_sensitive: true,
            unix_mode: false,
            set_mtime: false,
            free_space: false,
        }
    }
    async fn list(&self, _: &Path) -> rc_vfs::Result<Vec<Entry>> {
        Ok((0..BATCHES).flat_map(Trickle::batch).collect())
    }
    async fn list_batches(&self, dir: &Path, batches: Sender<Vec<Entry>>) -> rc_vfs::Result<()> {
        for index in 0.. {
            if index == 1 {
                if let Ok(permit) = self.gate.acquire().await {
                    permit.forget();
                }
                if self.ending == Ending::Fails {
                    return Err(VfsError::PermissionDenied {
                        path: dir.to_path_buf(),
                    });
                }
            }
            let batch = if index < BATCHES {
                Trickle::batch(index)
            } else if self.ending == Ending::Endless {
                vec![entry(&format!("g{index:09}"), EntryKind::File)]
            } else {
                break;
            };
            if batches.send(batch).await.is_err() {
                self.stopped.store(true, Ordering::SeqCst);
                break;
            }
        }
        Ok(())
    }
    async fn stat(&self, path: &Path) -> rc_vfs::Result<Entry> {
        unsupported(path)
    }
    async fn open_read(&self, path: &Path) -> rc_vfs::Result<ReadStream> {
        unsupported(path)
    }
    async fn open_write(&self, path: &Path, _: WriteMode) -> rc_vfs::Result<WriteStream> {
        unsupported(path)
    }
    async fn mkdir(&self, path: &Path, _: bool) -> rc_vfs::Result<()> {
        unsupported(path)
    }
    async fn rename(&self, from: &Path, _: &Path) -> rc_vfs::Result<()> {
        unsupported(from)
    }
    async fn remove_file(&self, path: &Path) -> rc_vfs::Result<()> {
        unsupported(path)
    }
    async fn remove_dir(&self, path: &Path, _: bool) -> rc_vfs::Result<()> {
        unsupported(path)
    }
    async fn set_mtime(&self, path: &Path, _: SystemTime) -> rc_vfs::Result<()> {
        unsupported(path)
    }
    async fn set_attributes(&self, path: &Path, _: SetAttributes) -> rc_vfs::Result<()> {
        unsupported(path)
    }
}

const SERVER: BackendId = BackendId::Ftp { connection_id: 7 };

fn on_server(path: &str) -> VfsPath {
    VfsPath {
        backend: SERVER,
        path: path.into(),
    }
}

/// The engine, the gate, the stop flag and the folder both panels start in.
type Setup = (Engine, Arc<Semaphore>, Arc<AtomicBool>, tempfile::TempDir);

/// An engine whose connection 7 is a [`Trickle`] that goes on as `ending` says.
async fn engine_with(ending: Ending) -> Setup {
    let home = tempfile::tempdir().expect("temp dir");
    let engine = test_support::headless(home.path()).await;
    let (gate, stopped) = (
        Arc::new(Semaphore::new(0)),
        Arc::new(AtomicBool::new(false)),
    );
    let trickle: Arc<dyn Vfs> = Arc::new(Trickle {
        gate: Arc::clone(&gate),
        stopped: Arc::clone(&stopped),
        ending,
    });
    engine.inner.remotes.open().insert(SERVER, trickle);
    (engine, gate, stopped, home)
}

fn names(state: &PanelState) -> Vec<String> {
    state
        .entries
        .iter()
        .map(|entry| entry.name.clone())
        .collect()
}

/// Waits until `done` holds, or fails the test after five seconds.
async fn until(what: &str, done: impl Fn() -> bool) {
    for _ in 0..500 {
        if done() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("{what} did not happen");
}

#[tokio::test]
async fn a_long_listing_shows_its_first_entries_and_the_rest_in_order() {
    let (engine, gate, _, _home) = engine_with(Ending::Complete).await;
    let Outcome::Panel(shown) = engine
        .show_path(PanelId::Left, on_server("/big"))
        .await
        .expect("shown")
    else {
        panic!("a panel");
    };
    assert!(shown.loading, "{shown:?}");
    assert_eq!(shown.entry_count, Trickle::BATCH);
    let first = engine.panel(PanelId::Left);
    assert_eq!(names(&first)[0], "f100", "the first batch, sorted");
    engine
        .toggle_selection(Some(PanelId::Left), &["f120".to_owned()])
        .expect("selected while arriving");

    gate.add_permits(BATCHES);
    until("the listing's end", || {
        engine.panel(PanelId::Left).listing.is_none()
    })
    .await;
    let done = engine.panel(PanelId::Left);
    let mut expected: Vec<String> = (0..BATCHES * Trickle::BATCH)
        .map(|number| format!("f{number:03}"))
        .collect();
    expected.insert(0, "sub".to_owned());
    assert_eq!(names(&done), expected, "folders first, then by name");
    assert!(done.selection.contains("f120"), "the selection stays");
    assert_eq!(
        done.cursor, 0,
        "nobody moved the cursor: it stays at the top"
    );
    assert!(!engine.panel_state(Some(PanelId::Left)).loading);
}

/// The entry just left arrives in the last batch, after the cursor was put at the top.
#[tokio::test]
async fn the_cursor_finds_the_folder_just_left_when_it_arrives() {
    let (engine, gate, _, _home) = engine_with(Ending::Complete).await;
    engine.state().panel_mut(PanelId::Left).path = on_server("/big/f010");
    engine
        .show_path(PanelId::Left, on_server("/big"))
        .await
        .expect("shown");
    gate.add_permits(BATCHES);
    until("the folder just left under the cursor", || {
        let state = engine.panel(PanelId::Left);
        state.listing.is_none()
            && state
                .cursor_entry()
                .is_some_and(|entry| entry.name == "f010")
    })
    .await;
}

#[tokio::test]
async fn a_second_navigation_ends_the_running_listing() {
    let (engine, gate, stopped, home) = engine_with(Ending::Endless).await;
    engine
        .show_path(PanelId::Left, on_server("/big"))
        .await
        .expect("shown");
    let Outcome::Panel(local) = engine
        .show_path(PanelId::Left, VfsPath::local(home.path()))
        .await
        .expect("navigated on")
    else {
        panic!("a panel");
    };
    assert!(!local.loading);
    gate.add_permits(BATCHES);
    until("the listing to stop", || stopped.load(Ordering::SeqCst)).await;
    let state = engine.panel(PanelId::Left);
    assert_eq!(state.path, VfsPath::local(home.path()));
    assert!(state.listing.is_none());
}

#[tokio::test]
async fn a_listing_that_breaks_off_keeps_what_arrived_and_says_why() {
    let (engine, gate, _, _home) = engine_with(Ending::Fails).await;
    let mut events = engine.subscribe();
    engine
        .show_path(PanelId::Left, on_server("/big"))
        .await
        .expect("shown before the failure");
    gate.add_permits(1);
    let code = loop {
        match tokio::time::timeout(Duration::from_secs(5), events.recv()).await {
            Ok(Ok(EngineEvent::ListingFailed { panel, code })) => {
                assert_eq!(panel, PanelId::Left);
                break code;
            }
            Ok(Ok(_)) => {}
            other => panic!("no failure announced: {other:?}"),
        }
    };
    assert_eq!(code, "permission_denied");
    let state = engine.panel(PanelId::Left);
    assert_eq!(state.entries.len(), Trickle::BATCH, "what arrived stays");
    assert!(state.listing.is_none());
}

#[tokio::test]
async fn a_quick_listing_is_shown_complete_at_once() {
    let home = tempfile::tempdir().expect("temp dir");
    std::fs::write(home.path().join("a.txt"), b"x").expect("write");
    let engine = test_support::headless(home.path()).await;
    let Outcome::Panel(shown) = engine
        .show_path(PanelId::Left, VfsPath::local(home.path()))
        .await
        .expect("shown")
    else {
        panic!("a panel");
    };
    assert!(!shown.loading);
    assert_eq!(shown.entry_count, 1);
}
