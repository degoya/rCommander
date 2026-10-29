use rc_db::{Db, LoggedStep};
use rc_engine::VfsPath;

use super::{done_pairs, log_text, write_log};

fn pair(old: &str, new: &str) -> (String, String) {
    (old.to_owned(), new.to_owned())
}

fn step(from: &str, to: &str, done: bool) -> LoggedStep {
    LoggedStep {
        from: from.to_owned(),
        to: to.to_owned(),
        done,
    }
}

#[test]
fn only_pairs_whose_last_step_is_done_count() {
    // A swap through a temporary name, stopped after its second step.
    let pairs = [pair("a", "b"), pair("b", "a"), pair("c", "sub/c")];
    let steps = [
        step("a", ".rc-rename-t-1", true),
        step("b", "a", true),
        step(".rc-rename-t-1", "b", false),
        step("c", "sub/c", false),
    ];
    assert_eq!(done_pairs(&pairs, &steps), [pair("b", "a")]);
}

#[test]
fn the_log_names_full_paths() {
    let dir = VfsPath::local("/data");
    let text = log_text(&dir, &[pair("a.txt", "b.txt"), pair("c", "sub/c")]);
    let sub = std::path::Path::new("/data").join("sub").join("c");
    assert_eq!(
        text,
        format!(
            "{} -> {}\n{} -> {}\n",
            std::path::Path::new("/data").join("a.txt").display(),
            std::path::Path::new("/data").join("b.txt").display(),
            std::path::Path::new("/data").join("c").display(),
            sub.display()
        )
    );
}

#[tokio::test]
async fn the_log_holds_exactly_the_pairs_carried_out() {
    let db = Db::open_in_memory().await.expect("db");
    db.log_rename_run("run1", "{}", &[("a", "x"), ("b", "y")])
        .await
        .expect("logged");
    db.mark_rename_step("run1", 0, true).await.expect("marked");
    let folder = tempfile::tempdir().expect("temp dir");
    let path = folder.path().join("rename.log");
    let dir = VfsPath::local("/d");
    let pairs = [pair("a", "x"), pair("b", "y")];
    let path_text = path.display().to_string();
    let written = write_log(Some(db.clone()), "run1", &dir, &pairs, &path_text).await;
    assert_eq!(written, Ok(1));
    let text = std::fs::read_to_string(&path).expect("read");
    assert_eq!(text, log_text(&dir, &[pair("a", "x")]));

    let other = write_log(Some(db), "run2", &dir, &pairs, &path_text).await;
    assert!(other.is_err(), "another run is not logged");
}
