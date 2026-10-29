use super::TermError;
use crate::TermId;

/// One error of every variant. The match stops compiling when a variant is added: give it a
/// sample here and its code in `ALL_CODES`.
fn one_of_each() -> Vec<TermError> {
    let samples = vec![
        TermError::NotFound(TermId(1)),
        TermError::Exited,
        TermError::UnknownShell("x".to_owned()),
        TermError::InvalidSize,
        TermError::Pty("x".to_owned()),
        TermError::WslUnavailable,
        TermError::UnknownDistro("x".to_owned()),
        TermError::Busy,
    ];
    for sample in &samples {
        match sample {
            TermError::NotFound(_)
            | TermError::Exited
            | TermError::UnknownShell(_)
            | TermError::InvalidSize
            | TermError::Pty(_)
            | TermError::WslUnavailable
            | TermError::UnknownDistro(_)
            | TermError::Busy => {}
        }
    }
    samples
}

#[test]
fn all_codes_lists_every_code_once() {
    let codes: Vec<&str> = one_of_each().iter().map(TermError::code).collect();
    assert_eq!(codes, TermError::ALL_CODES);
}
