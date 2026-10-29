use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::Mutex,
};

use rc_vfs::Result;

use super::{TreeOps, mkdir_all, remove_tree};

/// A tree in memory: directories and files by path, and the commands it was given.
#[derive(Default)]
struct Fake {
    dirs: Mutex<BTreeSet<PathBuf>>,
    files: BTreeSet<PathBuf>,
    /// Every directory holds one more directory, without end.
    endless: bool,
    log: Mutex<Vec<(&'static str, PathBuf)>>,
}

impl Fake {
    fn with(dirs: &[&str], files: &[&str]) -> Self {
        Self {
            dirs: Mutex::new(dirs.iter().map(PathBuf::from).collect()),
            files: files.iter().map(PathBuf::from).collect(),
            ..Self::default()
        }
    }

    fn note(&self, what: &'static str, path: &Path) {
        self.log
            .lock()
            .expect("log")
            .push((what, path.to_path_buf()));
    }

    fn log(&self) -> Vec<(&'static str, PathBuf)> {
        self.log.lock().expect("log").clone()
    }
}

impl TreeOps for Fake {
    async fn kind_of(&self, path: &Path) -> Result<Option<bool>> {
        if self.dirs.lock().expect("dirs").contains(path) {
            return Ok(Some(true));
        }
        Ok(self.files.contains(path).then_some(false))
    }

    async fn make_one_dir(&self, path: &Path) -> Result<()> {
        self.note("mkdir", path);
        self.dirs.lock().expect("dirs").insert(path.to_path_buf());
        Ok(())
    }

    async fn entries_of(&self, dir: &Path) -> Result<Vec<(String, bool)>> {
        if self.endless {
            return Ok(vec![("again".to_owned(), true)]);
        }
        let dirs = self.dirs.lock().expect("dirs");
        let below = |path: &PathBuf| path.parent() == Some(dir);
        let name = |path: &PathBuf| -> Option<String> {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        };
        let mut entries: Vec<(String, bool)> = dirs
            .iter()
            .filter(|path| below(path))
            .filter_map(|path| Some((name(path)?, true)))
            .collect();
        entries.extend(
            self.files
                .iter()
                .filter(|path| below(path))
                .filter_map(|path| Some((name(path)?, false))),
        );
        Ok(entries)
    }

    async fn remove_one(&self, path: &Path) -> Result<()> {
        self.note("rm", path);
        Ok(())
    }

    async fn remove_empty(&self, path: &Path) -> Result<()> {
        self.note("rmdir", path);
        Ok(())
    }
}

/// One command of the log; paths compare by component, so `/` and a Windows `\` are alike.
fn step(what: &'static str, path: &str) -> (&'static str, PathBuf) {
    (what, PathBuf::from(path))
}

#[tokio::test]
async fn mkdir_all_creates_the_missing_directories_from_the_top() {
    let fake = Fake::with(&["/", "/srv"], &[]);
    mkdir_all(&fake, Path::new("/srv/a/b")).await.expect("made");
    assert_eq!(
        fake.log(),
        [step("mkdir", "/srv/a"), step("mkdir", "/srv/a/b")]
    );
    mkdir_all(&fake, Path::new("/srv/a")).await.expect("exists");
    assert_eq!(fake.log().len(), 2, "an existing directory is fine");
}

#[tokio::test]
async fn mkdir_all_refuses_a_file_in_the_way() {
    let fake = Fake::with(&["/"], &["/f"]);
    let code = |result: Result<()>| result.expect_err("refused").code();
    assert_eq!(
        code(mkdir_all(&fake, Path::new("/f")).await),
        "already_exists"
    );
    assert_eq!(
        code(mkdir_all(&fake, Path::new("/f/x")).await),
        "not_a_directory"
    );
}

#[tokio::test]
async fn remove_tree_removes_the_contents_before_the_directory() {
    let fake = Fake::with(&["/", "/t", "/t/d"], &["/t/a", "/t/d/b"]);
    remove_tree(&fake, Path::new("/t")).await.expect("removed");
    assert_eq!(
        fake.log(),
        [
            step("rm", "/t/d/b"),
            step("rmdir", "/t/d"),
            step("rm", "/t/a"),
            step("rmdir", "/t"),
        ]
    );
}

#[tokio::test]
async fn remove_tree_stops_at_the_depth_limit() {
    let fake = Fake {
        endless: true,
        ..Fake::default()
    };
    let error = remove_tree(&fake, Path::new("/t"))
        .await
        .expect_err("too deep");
    assert_eq!(error.code(), "unsupported");
    assert!(fake.log().is_empty(), "nothing removed on the way down");
}
