use std::path::{MAIN_SEPARATOR, Path};

use super::*;

#[test]
fn a_name_below_the_root_is_relative_and_one_elsewhere_whole() {
    let root = Path::new("/data/root");
    assert_eq!(
        flat_name(root, &root.join("sub").join("a.txt")),
        format!("sub{MAIN_SEPARATOR}a.txt")
    );
    assert_eq!(flat_name(root, &root.join("b.txt")), "b.txt");
    assert_eq!(
        flat_name(root, Path::new("/elsewhere/c.txt")),
        Path::new("/elsewhere/c.txt").to_string_lossy()
    );
    // The root itself keeps its whole path rather than an empty name.
    assert_eq!(flat_name(root, root), root.to_string_lossy());
}

#[test]
fn runs_are_numbered_apart() {
    assert_ne!(next_run(), next_run());
}

#[test]
fn the_view_serializes_without_its_plan() {
    let view = FlatView {
        source: FlatSource::Branch,
        loading: true,
        truncated: false,
        skipped: 2,
        plan: Arc::new(FlatPlan {
            walk: vec!["/secret/place".into()],
            stat: Vec::new(),
        }),
        run: 7,
    };
    let json = serde_json::to_value(&view).expect("json");
    assert_eq!(
        json,
        serde_json::json!({"source": "branch", "loading": true, "truncated": false, "skipped": 2})
    );
}
