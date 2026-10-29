use std::path::PathBuf;

use super::{IconProvider, Loaded, Loader, answer};
use crate::app::icons::{key::IconKey, pixels::Rgba};

/// Icons for every key but `.none`, one transparent pixel each.
struct Fake;

impl IconProvider for Fake {
    fn load(&self, key: &IconKey) -> Option<Rgba> {
        (*key != IconKey::Ext("none".into())).then(|| Rgba {
            width: 1,
            height: 1,
            pixels: vec![0; 4],
        })
    }
}

fn keys() -> Vec<IconKey> {
    vec![
        IconKey::Ext("txt".into()),
        IconKey::Ext("none".into()),
        IconKey::File(PathBuf::from("/bin/tool.exe")),
    ]
}

fn kinds(answers: &[(IconKey, Loaded)]) -> Vec<&'static str> {
    answers
        .iter()
        .map(|(_, loaded)| match loaded {
            Loaded::Icon(_) => "icon",
            Loaded::None => "none",
            Loaded::Skipped => "skipped",
        })
        .collect()
}

#[test]
fn the_newest_batch_is_answered_in_full() {
    let answers = answer(&Fake, keys(), true);
    assert_eq!(kinds(&answers), vec!["icon", "none", "icon"]);
    assert_eq!(answers[2].0, IconKey::File(PathBuf::from("/bin/tool.exe")));
}

#[test]
fn an_older_batch_skips_its_per_file_keys() {
    let answers = answer(&Fake, keys(), false);
    assert_eq!(kinds(&answers), vec!["icon", "none", "skipped"]);
}

#[test]
fn the_thread_answers_a_request() {
    let loader =
        Loader::spawn(|| Box::new(Fake) as Box<dyn IconProvider>).expect("a thread starts");
    let answers = loader
        .request(vec![IconKey::Folder])
        .expect("the thread runs")
        .blocking_recv()
        .expect("an answer comes");
    assert_eq!(kinds(&answers), vec!["icon"]);
}
