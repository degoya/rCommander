//! Tar, zip and 7z archives and single compressed files (`rc_vfs::ArchiveFs`): the format
//! detected from the first bytes as a panel does, then the whole listing walked. Every name
//! listed is one entry of its folder - never empty, `.`, `..` or holding a separator - however
//! the archive spells it (`archive::safety`).

#![no_main]

use std::{
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
};

use libfuzzer_sys::fuzz_target;
use rc_vfs::{ArchiveFs, ArchiveSettings, EntryKind, LocalFs, Vfs};
use tokio::runtime::Runtime;

/// Folders listed at most, so one input of a deep tree stays quick.
const MAX_FOLDERS: usize = 256;

fn runtime() -> &'static Runtime {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a current-thread runtime builds")
    })
}

/// The archive file: the backend reads zip and 7z seeking, which only a real file offers.
fn archive_file() -> &'static Path {
    static FILE: OnceLock<PathBuf> = OnceLock::new();
    FILE.get_or_init(|| {
        std::env::temp_dir().join(format!("rc-fuzz-archive-{}", std::process::id()))
    })
}

async fn walk(path: &Path) {
    let outer: Arc<dyn Vfs> = Arc::new(LocalFs::new());
    let Ok(Some(archive)) = ArchiveFs::open(outer, path, ArchiveSettings::default()).await else {
        return;
    };
    let mut folders = vec![PathBuf::from("/")];
    let mut listed = 0;
    while let Some(folder) = folders.pop() {
        listed += 1;
        if listed > MAX_FOLDERS {
            break;
        }
        let Ok(items) = archive.list_detailed(&folder).await else {
            continue;
        };
        for item in items {
            let name = item.entry.name.as_str();
            assert!(
                !matches!(name, "" | "." | "..") && !name.contains(['/', '\\']),
                "the archive lists {name:?} in {}",
                folder.display()
            );
            if item.entry.kind == EntryKind::Dir {
                folders.push(folder.join(name));
            }
        }
    }
}

fuzz_target!(|data: &[u8]| {
    let path = archive_file();
    std::fs::write(path, data).expect("the input is written to the temp folder");
    runtime().block_on(walk(path));
});
