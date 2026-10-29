use std::path::Path;

use super::{MaskField, PatternError, PatternErrorKind, RenameError};
use crate::names::InvalidName;
use crate::plan::Conflict;

fn mask_error(kind: PatternErrorKind) -> RenameError {
    RenameError::Mask {
        field: MaskField::Name,
        error: PatternError { position: 0, kind },
    }
}

/// One error of every variant and pattern kind. The match stops compiling when a variant is
/// added: give it a sample here and its code in `ALL_CODES`.
#[test]
fn every_error_has_a_listed_code() {
    let samples = [
        RenameError::TooLong {
            field: "name mask",
            max: 1,
        },
        mask_error(PatternErrorKind::Unclosed),
        mask_error(PatternErrorKind::StrayBracket),
        mask_error(PatternErrorKind::Empty),
        mask_error(PatternErrorKind::Unknown {
            text: "X".to_owned(),
        }),
        mask_error(PatternErrorKind::Arguments {
            text: "N0".to_owned(),
        }),
        RenameError::Regex {
            message: "x".to_owned(),
        },
        RenameError::Conflicts { count: 1 },
        RenameError::Unschedulable,
    ];
    for sample in &samples {
        match sample {
            RenameError::TooLong { .. }
            | RenameError::Mask { .. }
            | RenameError::Regex { .. }
            | RenameError::Conflicts { .. }
            | RenameError::Unschedulable => {}
        }
    }
    let codes: Vec<&str> = samples.iter().map(RenameError::code).collect();
    assert_eq!(codes, RenameError::ALL_CODES);
}

#[test]
fn a_mask_error_names_the_mask_and_the_position() {
    let error = RenameError::Mask {
        field: MaskField::Extension,
        error: PatternError {
            position: 4,
            kind: PatternErrorKind::Unknown {
                text: "Q".to_owned(),
            },
        },
    };
    assert_eq!(
        error.to_string(),
        "extension mask, position 4: unknown placeholder [Q]"
    );
}

/// Every code of the engine - errors, conflicts and invalid names - has a message under
/// `error.<code>` in the GUI's catalogue. (The GUI's own test covers the crates it depends on;
/// this one covers rc-rename until it does.)
#[test]
fn every_code_has_a_message_in_the_catalogue() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../rcommander/locales/en.json");
    let text = std::fs::read_to_string(&path).expect("the catalogue is readable");
    let catalogue: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(&text).expect("the catalogue parses");
    let missing: Vec<String> = RenameError::ALL_CODES
        .iter()
        .chain(Conflict::ALL_CODES)
        .chain(InvalidName::ALL_CODES)
        .map(|code| format!("error.{code}"))
        .filter(|key| !catalogue.contains_key(key))
        .collect();
    assert!(missing.is_empty(), "keys missing in en.json: {missing:?}");
}
