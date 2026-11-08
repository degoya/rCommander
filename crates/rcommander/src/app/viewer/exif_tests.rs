//! A photo's EXIF beside the built-in image view (RC-100-99): read when the photo opens unless
//! `viewer.exif` is off, a document of the picture and the EXIF tree, the tree's groups and
//! fields.

use std::{fs, path::Path};

use rc_db::Db;
use rc_engine::{
    PanelId, VfsPath,
    viewer::exif::{ExifField, ExifGroup, ExifSummary, PhotoExif},
};

use super::{ExifSource, FIELD_IDS, document, read};
use crate::{
    app::{
        harness::Harness,
        viewer::{ViewerMessage, document::DocView, lister::Lister, load, plugin::PluginOpened},
    },
    i18n::t,
};

/// A JPEG whose EXIF block names its camera: IFD0 with `Make` = "Canon".
fn photo() -> Vec<u8> {
    let mut tiff = b"II*\0\x08\0\0\0".to_vec();
    tiff.extend(1u16.to_le_bytes());
    tiff.extend(0x010Fu16.to_le_bytes());
    tiff.extend(2u16.to_le_bytes());
    tiff.extend(6u32.to_le_bytes());
    // The value after the header (8), the count (2), one entry (12) and the next IFD (4).
    tiff.extend(26u32.to_le_bytes());
    tiff.extend(0u32.to_le_bytes());
    tiff.extend(b"Canon\0");
    let mut jpeg = vec![0xFF, 0xD8, 0xFF, 0xE1];
    jpeg.extend(u16::try_from(tiff.len() + 8).expect("small").to_be_bytes());
    jpeg.extend(b"Exif\0\0");
    jpeg.extend(&tiff);
    jpeg.extend([0xFF, 0xD9]);
    jpeg
}

fn field(group: ExifGroup, tag: &str, value: &str) -> ExifField {
    ExifField {
        group,
        tag: tag.to_owned(),
        value: value.to_owned(),
    }
}

fn sample() -> PhotoExif {
    PhotoExif {
        fields: vec![
            field(ExifGroup::Image, "Make", "Canon"),
            field(ExifGroup::Image, "Model", "EOS R5"),
            field(ExifGroup::Photo, "ExposureTime", "1/125 s"),
            field(ExifGroup::Gps, "GPSLatitude", "48 deg 51 min 24 sec N"),
        ],
        summary: ExifSummary::default(),
    }
}

async fn opened(root: &Path, harness: &Harness) -> load::Opened {
    let path = root.join("photo.jpg");
    fs::write(&path, photo()).expect("write");
    load::open(harness.app.engine().clone(), VfsPath::local(path), None)
        .await
        .expect("open")
}

#[test]
fn the_tree_holds_the_groups_and_below_them_the_fields() {
    let source = ExifSource::new(&sample());
    let groups = source.nodes(None);
    let labels: Vec<&str> = groups.iter().map(|node| node.label.as_str()).collect();
    assert_eq!(
        labels,
        [
            t!("viewer.exif_group.image"),
            t!("viewer.exif_group.photo"),
            t!("viewer.exif_group.gps")
        ]
    );
    assert!(groups.iter().all(|node| node.has_children));
    assert_eq!(groups[0].value.as_deref(), Some("2"), "the count of fields");
    let image = source.nodes(Some(groups[0].id));
    assert_eq!(image.len(), 2);
    assert_eq!(
        (image[1].label.as_str(), image[1].value.as_deref()),
        ("Model", Some("EOS R5"))
    );
    assert!(
        image
            .iter()
            .all(|node| !node.has_children && node.id >= FIELD_IDS)
    );
    assert!(source.nodes(Some(99)).is_empty(), "no such node");
    assert!(
        source.nodes(Some(image[0].id)).is_empty(),
        "a field has no children"
    );
}

#[test]
fn the_document_shows_the_picture_then_the_exif_and_the_picture_is_the_file() {
    let shown = document(&sample()).expect("a document");
    assert_eq!(shown.views(), [DocView::Image, DocView::Exif]);
    assert!(shown.file_picture());
    assert!(DocView::Exif.is_tree() && !DocView::Exif.is_text());
}

#[tokio::test]
async fn a_photo_brings_its_exif_unless_the_setting_is_off() {
    let root = tempfile::tempdir().expect("temp dir");
    let harness = Harness::headless(root.path()).await;
    let opened = opened(root.path(), &harness).await;
    let exif = read(&opened, None).await.expect("EXIF without a database");
    assert_eq!(exif.summary.camera.as_deref(), Some("Canon"));

    let db = Db::open_in_memory().await.expect("db");
    assert!(read(&opened, Some(&db)).await.is_some(), "on by default");
    db.set(rc_engine::settings::VIEWER_EXIF_KEY, &false)
        .await
        .expect("set");
    assert!(read(&opened, Some(&db)).await.is_none(), "viewer.exif off");
}

#[tokio::test]
async fn a_text_file_has_no_exif() {
    let root = tempfile::tempdir().expect("temp dir");
    let harness = Harness::headless(root.path()).await;
    let path = root.path().join("notes.txt");
    fs::write(&path, b"Exif\0\0 is no photo").expect("write");
    let opened = load::open(harness.app.engine().clone(), VfsPath::local(path), None)
        .await
        .expect("open");
    assert!(read(&opened, None).await.is_none());
}

/// The lister takes the photo with its EXIF document: the image view shows the file's own
/// picture, `7` steps to the EXIF tree, which asks for its top level.
#[tokio::test]
async fn the_lister_shows_the_picture_and_seven_steps_to_the_tree() {
    let root = tempfile::tempdir().expect("temp dir");
    let mut harness = Harness::headless(root.path()).await;
    let opened = opened(root.path(), &harness).await;
    let path = VfsPath::local(root.path().join("photo.jpg"));
    harness
        .app
        .viewers
        .open
        .push(Lister::new(7, PanelId::Left, path));
    let exif = read(&opened, None).await.expect("EXIF");
    let effects = harness.app.on_viewer(ViewerMessage::Plugin(
        7,
        PluginOpened::Exif { opened, exif },
    ));
    assert!(!effects.is_empty(), "the picture is read");
    let lister = harness.app.viewers.lister_mut(7).expect("open");
    assert!(!lister.document_picture(), "the picture is the file's");
    let document = lister.document.as_ref().expect("a document");
    assert_eq!(document.view(), DocView::Image);
    let requests = lister.document_key(super::super::keys::ListerKey::NextView);
    assert!(lister.tree_shown());
    assert!(requests.is_some_and(|requests| !requests.is_empty()));
}
