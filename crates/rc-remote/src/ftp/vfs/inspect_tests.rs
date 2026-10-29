use std::path::Path;

use rc_vfs::Vfs;
use tokio::sync::mpsc;

use crate::ftp::{
    Encryption,
    test_server::{Options, ServerTls, TestServer},
    vfs::{FtpVfs, tests::config},
};

/// A directory of `count` files with long names: more than one chunk of listing.
async fn server_with_files(mlst: bool, count: usize) -> TestServer {
    let server = TestServer::start(Options {
        tls: ServerTls::None,
        mlst,
        ..Options::default()
    })
    .await;
    let dir = server.local("big");
    std::fs::create_dir(&dir).expect("dir");
    for index in 0..count {
        let name = format!("{index:05}-{}.txt", "n".repeat(80));
        std::fs::write(dir.join(name), b"x").expect("file");
    }
    server
}

/// Acceptance (RC-080-46): a long listing reaches the panel in batches as it arrives, and the
/// batches hold exactly what `list` returns.
#[tokio::test]
async fn a_long_listing_arrives_in_batches() {
    for mlst in [true, false] {
        let server = server_with_files(mlst, 2000).await;
        let vfs = FtpVfs::connect(config(&server, Encryption::None))
            .await
            .expect("connect");
        let (sender, mut receiver) = mpsc::channel(64);
        let collect = async {
            let mut batches = Vec::new();
            while let Some(batch) = receiver.recv().await {
                batches.push(batch);
            }
            batches
        };
        let (listed, batches) = tokio::join!(vfs.list_batches(Path::new("/big"), sender), collect);
        listed.expect("listed");
        assert!(
            batches.len() > 1,
            "mlst {mlst}: {} batch(es)",
            batches.len()
        );
        let mut streamed: Vec<String> = batches.into_iter().flatten().map(|e| e.name).collect();
        let mut whole: Vec<String> = Vfs::list(&vfs, Path::new("/big"))
            .await
            .expect("list")
            .into_iter()
            .map(|entry| entry.name)
            .collect();
        streamed.sort();
        whole.sort();
        assert_eq!(streamed.len(), 2000, "mlst {mlst}");
        assert_eq!(streamed, whole, "mlst {mlst}");
    }
}

/// A panel that stops listening ends the listing without an error, and the session lists on.
#[tokio::test]
async fn a_listing_nobody_waits_for_stops() {
    let server = server_with_files(true, 2000).await;
    let vfs = FtpVfs::connect(config(&server, Encryption::None))
        .await
        .expect("connect");
    let (sender, mut receiver) = mpsc::channel(1);
    let first = async {
        let batch = receiver.recv().await;
        drop(receiver);
        batch
    };
    let (listed, first) = tokio::join!(vfs.list_batches(Path::new("/big"), sender), first);
    listed.expect("stopped without an error");
    assert!(first.is_some_and(|batch| !batch.is_empty()));
    assert_eq!(
        Vfs::list(&vfs, Path::new("/big"))
            .await
            .expect("list")
            .len(),
        2000
    );
}

/// A folder with a folder, a file and a link to each.
#[cfg(unix)]
async fn server_with_links(mlst: bool) -> TestServer {
    let server = TestServer::start(Options {
        tls: ServerTls::None,
        mlst,
        ..Options::default()
    })
    .await;
    let dir = server.local("l");
    std::fs::create_dir_all(dir.join("real")).expect("dir");
    std::fs::write(dir.join("real").join("inside.txt"), b"in").expect("file");
    std::fs::write(dir.join("file.txt"), b"data").expect("file");
    std::os::unix::fs::symlink("real", dir.join("to_dir")).expect("link");
    std::os::unix::fs::symlink("file.txt", dir.join("to_file")).expect("link");
    server
}

#[cfg(unix)]
fn named<'a>(entries: &'a [rc_vfs::Entry], name: &str) -> &'a rc_vfs::Entry {
    entries
        .iter()
        .find(|entry| entry.name == name)
        .unwrap_or_else(|| panic!("{name} listed"))
}

#[cfg(unix)]
fn unresolved(entry: &rc_vfs::Entry) -> bool {
    matches!(
        entry.kind,
        rc_vfs::EntryKind::Symlink {
            target_kind: None,
            ..
        }
    )
}

/// Acceptance (RC-080-46), the default: a listing shows links unresolved, at once and with the
/// other entries; entering, stat-ing and opening one works, and a link learned to be a folder
/// shows as one in the next listing.
#[cfg(unix)]
#[tokio::test]
async fn links_are_looked_up_lazily_by_default() {
    for mlst in [true, false] {
        let server = server_with_links(mlst).await;
        let vfs = FtpVfs::connect(config(&server, Encryption::None))
            .await
            .expect("connect");
        let listed = Vfs::list(&vfs, Path::new("/l")).await.expect("list");
        assert!(unresolved(named(&listed, "to_dir")), "mlst {mlst}");
        assert!(unresolved(named(&listed, "to_file")), "mlst {mlst}");
        let (sender, mut receiver) = mpsc::channel(8);
        vfs.list_batches(Path::new("/l"), sender)
            .await
            .expect("batches");
        let first = receiver.recv().await.expect("a batch");
        assert_eq!(first.len(), 4, "mlst {mlst}: one batch, links in it");

        let inside = Vfs::list(&vfs, Path::new("/l/to_dir"))
            .await
            .expect("entered");
        assert_eq!(inside.len(), 1, "mlst {mlst}");
        assert!(
            vfs.stat(Path::new("/l/to_dir"))
                .await
                .expect("stat")
                .is_dir_like()
        );
        let mut stream = vfs.open_read(Path::new("/l/to_file")).await.expect("open");
        let mut bytes = Vec::new();
        tokio::io::AsyncReadExt::read_to_end(&mut stream, &mut bytes)
            .await
            .expect("read");
        assert_eq!(bytes, b"data");

        let again = Vfs::list(&vfs, Path::new("/l")).await.expect("list");
        assert!(
            named(&again, "to_dir").is_dir_like(),
            "mlst {mlst}: learned"
        );
    }
}

/// With `resolve_links` on, a listing looks its links up: a link to a folder is one at once.
#[cfg(unix)]
#[tokio::test]
async fn links_are_looked_up_in_the_listing_when_asked() {
    for mlst in [true, false] {
        let server = server_with_links(mlst).await;
        let mut resolving = config(&server, Encryption::None);
        resolving.resolve_links = true;
        let vfs = FtpVfs::connect(resolving).await.expect("connect");
        let listed = Vfs::list(&vfs, Path::new("/l")).await.expect("list");
        assert!(named(&listed, "to_dir").is_dir_like(), "mlst {mlst}");
        assert!(
            matches!(
                named(&listed, "to_file").kind,
                rc_vfs::EntryKind::Symlink {
                    target_kind: Some(rc_vfs::TargetKind::File),
                    ..
                }
            ),
            "mlst {mlst}"
        );
        let (sender, mut receiver) = mpsc::channel(8);
        vfs.list_batches(Path::new("/l"), sender)
            .await
            .expect("batches");
        let mut batches = Vec::new();
        while let Ok(batch) = receiver.try_recv() {
            batches.push(batch);
        }
        let last = batches.last().expect("batches");
        assert!(
            last.iter()
                .all(|entry| entry.is_dir_like() || !unresolved(entry))
        );
        assert!(
            last.iter().any(|entry| entry.name == "to_dir"),
            "links last"
        );
    }
}
