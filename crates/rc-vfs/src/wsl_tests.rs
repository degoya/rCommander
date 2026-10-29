use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use super::*;

/// A source that counts its reads and lists one distribution per read so far.
struct Counting(Arc<AtomicUsize>);

#[async_trait]
impl DistroSource for Counting {
    async fn list(&self) -> Result<Vec<WslDistro>> {
        let reads = self.0.fetch_add(1, Ordering::SeqCst) + 1;
        Ok((0..reads)
            .map(|index| WslDistro {
                name: format!("Distro{index}"),
                is_default: index == 0,
                version: Some(2),
                root: VfsPath::local(format!(r"\\wsl.localhost\Distro{index}\")),
            })
            .collect())
    }
}

fn counting() -> (WslCatalog, Arc<AtomicUsize>) {
    let reads = Arc::new(AtomicUsize::new(0));
    let catalog = WslCatalog::with_source(Box::new(Counting(Arc::clone(&reads))));
    (catalog, reads)
}

#[tokio::test]
async fn the_list_is_read_once_and_kept() {
    let (catalog, reads) = counting();
    assert_eq!(catalog.distros().await.expect("lists").len(), 1);
    assert_eq!(catalog.distros().await.expect("lists").len(), 1);
    assert_eq!(reads.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn refresh_reads_again_and_keeps_the_new_list() {
    let (catalog, reads) = counting();
    catalog.distros().await.expect("lists");
    assert_eq!(catalog.refresh().await.expect("lists").len(), 2);
    assert_eq!(catalog.distros().await.expect("lists").len(), 2);
    assert_eq!(reads.load(Ordering::SeqCst), 2);
}

#[cfg(not(windows))]
#[tokio::test]
async fn outside_windows_there_is_no_wsl_to_list() {
    assert!(list_distros().await.expect("lists").is_empty());
    assert!(WslCatalog::new().distros().await.expect("lists").is_empty());
}

#[test]
fn a_distribution_serializes_with_its_root() {
    let distro = WslDistro {
        name: "Ubuntu".into(),
        is_default: true,
        version: Some(2),
        root: VfsPath::local(r"\\wsl.localhost\Ubuntu\"),
    };
    let value = serde_json::to_value(&distro).expect("serializes");
    assert_eq!(value["name"], "Ubuntu");
    assert_eq!(value["is_default"], true);
    assert_eq!(value["version"], 2);
    assert_eq!(value["root"]["backend"], "local");
    assert_eq!(value["root"]["path"], r"\\wsl.localhost\Ubuntu\");
}
