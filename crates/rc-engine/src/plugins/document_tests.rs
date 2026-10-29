//! A document offering a picture and text (RC-100-29), against the host's test bed: both offered,
//! the first written before the window shows it and the second only when asked, a person moving
//! on in between, and each output held to its own limits. The bed's `image` writes what the
//! SVG viewer's does - a raw raster named `rgba:<w>x<h>` - and its `text` writes `hello`.

use std::{
    fs,
    sync::{Arc, atomic::Ordering},
};

use rc_plugin_host::{
    codes,
    testbed::{self, Export, Testbed},
};
use rc_vfs::VfsPath;

use super::PictureFormat;
use crate::{
    plugins::{
        Opening, Output, PluginDocument, ViewerPlugins,
        test_support::{Spec, judge, one, request},
    },
    test_support::engine,
};

const ID: &str = "019d0000-0000-7000-8000-0000000000a1";

/// Two pixels, red and blue, as straight RGBA.
const RASTER: [u8; 8] = [255, 0, 0, 255, 0, 0, 255, 255];

/// `open` offering `outputs`; `image` writing `picture` and naming it `format`.
fn picture_and_text(outputs: &[&str], picture: &[u8], format: &str) -> Testbed {
    let mut bed = Testbed::new();
    let offered: Vec<u8> = outputs.iter().map(|name| testbed::output(name)).collect();
    let offered = bed.data(&offered);
    let pixels = bed.data(picture);
    let named = bed.data(format.as_bytes());
    let image = format!(
        "{} {}",
        testbed::write(0, 0, pixels),
        testbed::ok_pair(named)
    );
    bed.body(Export::Open, testbed::ok_document(offered, None, None))
        .body(Export::Image, image)
}

fn set(outputs: &[&str], bed: &Testbed, output_limit: Option<u64>) -> ViewerPlugins {
    let mut spec = Spec {
        extensions: &["svg", "csv"],
        outputs,
        ..Spec::default()
    };
    if let Some(limit) = output_limit {
        spec.output_limit = limit;
    }
    one(&spec, bed)
}

/// The exports the plugin was called with, in order.
fn calls(set: &ViewerPlugins) -> Vec<&'static str> {
    set.execution_log()
        .records(ID)
        .iter()
        .map(|record| record.operation)
        .collect()
}

fn document(opening: Opening) -> PluginDocument {
    match opening {
        Opening::Document(document) => *document,
        other => panic!("a document, got {other:?}"),
    }
}

#[tokio::test]
async fn the_picture_first_and_the_text_only_when_asked() {
    let (root, engine) = engine(&[("logo.svg", "<svg/>")]).await;
    let path = VfsPath::local(root.path().join("logo.svg"));
    let outputs = ["image", "highlighted"];
    let set = set(
        &outputs,
        &picture_and_text(&outputs, &RASTER, "rgba:2x1"),
        None,
    );
    let shown = document(set.open(&engine, &path, judge, request()).await);
    assert_eq!(shown.outputs(), [Output::Image, Output::Highlighted]);
    let (picture, format) = shown.picture().expect("written before the window shows it");
    assert_eq!(
        format,
        PictureFormat::Rgba {
            width: 2,
            height: 1
        }
    );
    let picture = picture.to_path_buf();
    assert_eq!(fs::read(&picture).expect("written"), RASTER);
    assert!(shown.text_path().is_none(), "the text waits");
    assert_eq!(calls(&set), ["probe", "open", "image"]);

    let (text, _) = shown.write_text(&engine).await.expect("the text");
    let text = text.to_path_buf();
    assert_eq!(fs::read(&text).expect("written"), b"hello");
    assert_ne!(text, picture, "a file of its own");
    assert!(shown.write_text(&engine).await.is_ok());
    assert!(shown.write_picture().await.is_ok());
    assert_eq!(
        calls(&set),
        ["probe", "open", "image", "text"],
        "each output is written once"
    );
    assert_eq!(set.live_sessions(), 1);
    drop(shown);
    assert_eq!(set.live_sessions(), 0);
    assert!(
        !picture.exists() && !text.exists(),
        "both go with the document"
    );
}

#[tokio::test]
async fn the_text_first_and_the_picture_only_when_asked() {
    let (root, engine) = engine(&[("data.csv", "a,b\n")]).await;
    let path = VfsPath::local(root.path().join("data.csv"));
    let outputs = ["highlighted", "image"];
    let png = b"\x89PNG\r\n\x1a\n";
    let set = set(&outputs, &picture_and_text(&outputs, png, "png"), None);
    let shown = document(set.open(&engine, &path, judge, request()).await);
    assert_eq!(shown.outputs(), [Output::Highlighted, Output::Image]);
    assert!(shown.text_path().is_some());
    assert!(shown.picture().is_none(), "the picture waits");
    assert_eq!(calls(&set), ["probe", "open", "text"]);

    let (picture, format) = shown.write_picture().await.expect("the picture");
    assert_eq!(format, PictureFormat::Encoded);
    assert_eq!(fs::read(picture).expect("written"), png);
    assert_eq!(calls(&set), ["probe", "open", "text", "image"]);
}

/// The person moves on between the two: nothing more runs, and the first output stays readable
/// until the window drops the document.
#[tokio::test]
async fn moving_on_before_the_second_output_writes_nothing() {
    let (root, engine) = engine(&[("logo.svg", "<svg/>")]).await;
    let path = VfsPath::local(root.path().join("logo.svg"));
    let outputs = ["image", "highlighted"];
    let set = set(
        &outputs,
        &picture_and_text(&outputs, &RASTER, "rgba:2x1"),
        None,
    );
    let asked = request();
    let flag = Arc::clone(&asked.cancel);
    let shown = document(set.open(&engine, &path, judge, asked).await);
    flag.store(true, Ordering::SeqCst);
    assert_eq!(
        shown.write_text(&engine).await.err(),
        Some(codes::PLUGIN_CANCELLED)
    );
    assert_eq!(calls(&set), ["probe", "open", "image"]);
    let (picture, _) = shown.picture().expect("the first output");
    assert_eq!(fs::read(picture).expect("still there"), RASTER);
}

/// A raster whose length is not `width * height * 4` is refused: as the first output the
/// document fails, as the second only the picture does.
#[tokio::test]
async fn a_raster_of_the_wrong_length_is_refused() {
    let (root, engine) = engine(&[("logo.svg", "<svg/>")]).await;
    let path = VfsPath::local(root.path().join("logo.svg"));
    let first = ["image", "highlighted"];
    let set_first = set(&first, &picture_and_text(&first, &RASTER, "rgba:2x2"), None);
    match set_first.open(&engine, &path, judge, request()).await {
        Opening::Failed { code, .. } => assert_eq!(code, codes::PLUGIN_BAD_OUTPUT),
        other => panic!("a failure, got {other:?}"),
    }
    assert_eq!(set_first.live_sessions(), 0);

    let second = ["highlighted", "image"];
    let set_second = set(
        &second,
        &picture_and_text(&second, &RASTER, "rgba:2x2"),
        None,
    );
    let shown = document(set_second.open(&engine, &path, judge, request()).await);
    assert_eq!(
        shown.write_picture().await.err(),
        Some(codes::PLUGIN_BAD_OUTPUT)
    );
    assert!(shown.picture().is_none());
    let text = shown.text_path().expect("the first output");
    assert_eq!(fs::read(text).expect("still there"), b"hello");
}

/// Each output is held to `plugins.viewer_output_limit_mib` on its own: a one-pixel picture fits
/// four bytes, the five bytes of text do not.
#[tokio::test]
async fn each_output_is_held_to_the_output_limit() {
    let (root, engine) = engine(&[("logo.svg", "<svg/>")]).await;
    let path = VfsPath::local(root.path().join("logo.svg"));
    let outputs = ["image", "highlighted"];
    let bed = picture_and_text(&outputs, &RASTER[..4], "rgba:1x1");
    let set = set(&outputs, &bed, Some(4));
    let shown = document(set.open(&engine, &path, judge, request()).await);
    assert!(shown.picture().is_some());
    assert_eq!(
        shown.write_text(&engine).await.err(),
        Some(codes::PLUGIN_OUTPUT_TOO_LARGE)
    );
    assert!(shown.text_path().is_none());
    assert!(
        shown.tokens(&[0]).await.expect("no text, no question")[0].is_empty(),
        "nothing to colour"
    );
}

#[tokio::test]
async fn an_output_the_document_does_not_offer_is_never_asked() {
    let (root, engine) = engine(&[("data.csv", "a,b\n")]).await;
    let path = VfsPath::local(root.path().join("data.csv"));
    let set = one(&Spec::default(), &Testbed::new());
    let shown = document(set.open(&engine, &path, judge, request()).await);
    assert_eq!(
        shown.write_picture().await.err(),
        Some(codes::PLUGIN_OUTPUT_NOT_OFFERED)
    );
    assert_eq!(calls(&set), ["probe", "open", "text"]);
}
