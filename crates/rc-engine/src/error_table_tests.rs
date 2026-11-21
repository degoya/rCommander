//! Every own code and text against the table written from the source before the enum was split
//! into groups (RC-110-35, `error_table.txt`): the split moves variants, never a code or a word.

use super::{EngineError, one_of_each_own};

const TABLE: &str = include_str!("error_table.txt");

/// The table's rows: the code and the text as `{:?}` quotes it.
fn rows() -> Vec<(&'static str, &'static str)> {
    TABLE
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| line.split_once('\t').expect("a code, a tab and a text"))
        .collect()
}

#[test]
fn every_own_code_and_text_is_the_table_s() {
    let mut samples = one_of_each_own();
    samples.push(EngineError::Hop {
        hop: "bastion".to_owned(),
        index: 1,
        total: 2,
        source: Box::new(EngineError::Cancelled),
    });
    let table = rows();
    assert_eq!(samples.len(), table.len(), "one table row per sample");
    for (sample, (code, text)) in samples.iter().zip(table) {
        assert_eq!(sample.code(), code, "{sample:?}");
        assert_eq!(format!("{:?}", sample.to_string()), text, "{sample:?}");
    }
}
