//! RC-080-05: `keymap/tc.json`, the standard keymap against Total Commander 11's keys, which the
//! keyboard reference prints. A row is a key rCommander binds where TC has no default (`own`), a
//! key bound to something TC does differently (`differs`, with TC's meaning), or a TC key left
//! free (`free`, with TC's meaning); each says why. The rows must match the keymap, so the
//! reference cannot call a key free that a later job binds.

use std::collections::HashSet;

use serde::Deserialize;

use super::*;

const TC: &str = include_str!("../keymap/tc.json");

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Kind {
    Own,
    Differs,
    Free,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Row {
    keys: String,
    kind: Kind,
    #[serde(default)]
    tc: Option<String>,
    why: String,
}

fn rows() -> Vec<Row> {
    serde_json::from_str(TC).expect("keymap/tc.json is a list of rows")
}

#[test]
fn every_tc_row_matches_the_keymap() {
    let keymap = Keymap::standard();
    let taken = |chord: &Chord| {
        keymap.bindings().iter().any(|row| row.chord == *chord)
            || keymap.window().iter().any(|row| row.chord == *chord)
    };
    let mut seen = HashSet::new();
    for row in rows() {
        let chord: Chord = row.keys.parse().expect("a chord");
        assert!(seen.insert(chord.clone()), "{} is listed twice", row.keys);
        assert!(row.why.len() >= 8, "{}: say why", row.keys);
        match row.kind {
            Kind::Own => {
                assert!(
                    taken(&chord),
                    "{}: rCommander's own key is not bound",
                    row.keys
                );
                assert!(row.tc.is_none(), "{}: TC has no default for it", row.keys);
            }
            Kind::Differs => {
                assert!(taken(&chord), "{}: not bound", row.keys);
                assert!(row.tc.is_some(), "{}: say what TC does", row.keys);
            }
            Kind::Free => {
                assert!(
                    !taken(&chord),
                    "{} is bound now: drop or change its row in keymap/tc.json",
                    row.keys
                );
                assert!(row.tc.is_some(), "{}: say what TC does", row.keys);
            }
        }
    }
}

/// The keys only rCommander has are in the table: the terminal toggle, the new terminal, the
/// terminal over the whole window and the command palette.
#[test]
fn rcommanders_own_keys_are_listed() {
    let own: Vec<Chord> = rows()
        .into_iter()
        .filter(|row| matches!(row.kind, Kind::Own))
        .map(|row| row.keys.parse().expect("a chord"))
        .collect();
    for keys in [
        "Ctrl+[Semicolon]",
        "Ctrl+[Backquote]",
        "Ctrl+Shift+[Semicolon]",
        "Ctrl+Shift+[Backquote]",
        "Ctrl+[Quote]",
        "Ctrl+Shift+P",
    ] {
        let chord: Chord = keys.parse().expect("a chord");
        assert!(own.contains(&chord), "{keys}");
    }
}
