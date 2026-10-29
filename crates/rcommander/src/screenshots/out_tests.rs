use std::{fs, path::Path};

use super::{MANIFEST, Manifest, OutDir, Taken, write_manifest, write_png};

#[test]
fn a_relative_folder_is_refused() {
    let error = OutDir::prepare(Path::new("shots")).expect_err("relative");
    assert!(error.to_string().contains("not absolute"), "{error}");
}

#[test]
fn a_missing_folder_is_made_and_a_shot_lies_directly_in_it() {
    let temp = tempfile::tempdir().expect("temp dir");
    let out = OutDir::prepare(&temp.path().join("new").join("out")).expect("out");
    assert!(out.root().is_dir());
    let shot = out.shot("panels-dark").expect("a shot");
    assert_eq!(shot.parent(), Some(out.root()));
    assert_eq!(
        shot.file_name().and_then(|name| name.to_str()),
        Some("panels-dark.png")
    );
    let manifest = out.manifest().expect("the manifest");
    assert_eq!(manifest, out.root().join(MANIFEST));
}

#[test]
fn no_name_leads_out_of_the_folder() {
    let temp = tempfile::tempdir().expect("temp dir");
    let out = OutDir::prepare(temp.path()).expect("out");
    for bad in [
        "../up", "..", "a/b", "a\\b", "/abs", "C:\\x", "", "con", "Shot",
    ] {
        assert!(out.shot(bad).is_err(), "{bad:?} passed");
    }
}

#[test]
fn a_folder_at_the_name_is_refused() {
    let temp = tempfile::tempdir().expect("temp dir");
    let out = OutDir::prepare(temp.path()).expect("out");
    fs::create_dir(out.root().join("taken.png")).expect("mkdir");
    let error = out.shot("taken").expect_err("a folder there");
    assert!(error.to_string().contains("no plain file"), "{error}");
}

#[cfg(unix)]
#[test]
fn a_link_at_the_name_is_refused() {
    let temp = tempfile::tempdir().expect("temp dir");
    let out = OutDir::prepare(&temp.path().join("out")).expect("out");
    let elsewhere = temp.path().join("elsewhere.png");
    fs::write(&elsewhere, b"keep").expect("write");
    std::os::unix::fs::symlink(&elsewhere, out.root().join("linked.png")).expect("link");
    assert!(out.shot("linked").is_err(), "a link was followed");
    assert_eq!(fs::read(&elsewhere).expect("read"), b"keep");
}

#[test]
fn a_picture_is_written_as_png_and_read_back() {
    let temp = tempfile::tempdir().expect("temp dir");
    let out = OutDir::prepare(temp.path()).expect("out");
    let path = out.shot("tiny").expect("path");
    let rgba = [255, 0, 0, 255, 0, 0, 255, 128];
    write_png(&path, &rgba, 2, 1).expect("written");
    let read = image::open(&path).expect("a png").to_rgba8();
    assert_eq!(read.dimensions(), (2, 1));
    assert_eq!(read.as_raw().as_slice(), rgba.as_slice());
    // A second run replaces it.
    write_png(&path, &[0, 0, 0, 255], 1, 1).expect("replaced");
    assert_eq!(
        image::open(&path).expect("a png").to_rgba8().dimensions(),
        (1, 1)
    );
}

#[test]
fn bytes_that_do_not_fit_the_size_are_refused() {
    let temp = tempfile::tempdir().expect("temp dir");
    let path = temp.path().join("bad.png");
    let error = write_png(&path, &[0; 7], 2, 1).expect_err("seven bytes");
    assert!(error.to_string().contains("not 8"), "{error}");
    assert!(write_png(&path, &[], 0, 0).is_err());
    assert!(!path.exists());
}

#[test]
fn the_manifest_says_what_was_taken() {
    let temp = tempfile::tempdir().expect("temp dir");
    let out = OutDir::prepare(temp.path()).expect("out");
    let path = out.manifest().expect("path");
    let manifest = Manifest {
        version: "0.0.0",
        ok: true,
        error: None,
        shots: vec![Taken {
            name: "a".to_owned(),
            file: "a.png".to_owned(),
            width: 1920,
            height: 1200,
        }],
    };
    write_manifest(&path, &manifest).expect("written");
    let read: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).expect("read")).expect("json");
    assert_eq!(read["ok"], true);
    assert!(read.get("error").is_none());
    assert_eq!(read["shots"][0]["file"], "a.png");
    assert_eq!(read["shots"][0]["width"], 1920);
}
