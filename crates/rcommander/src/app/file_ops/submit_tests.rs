use rc_engine::{EngineError, RenameError};

use super::named;

/// A rename mask's collision and chain name their entries in the status line (RC-110-04); other
/// refusals keep the text of their code.
#[test]
fn a_mask_refusal_names_its_entries() {
    let collision = EngineError::Rename(RenameError::MaskCollision {
        first: "a.txt".into(),
        second: "b.txt".into(),
        name: "x.txt".into(),
    });
    let text = named(&collision).expect("a message");
    assert!(
        text.contains("a.txt") && text.contains("b.txt") && text.contains("x.txt"),
        "{text}"
    );
    assert!(!text.contains('{'), "every placeholder filled: {text}");
    let chain = EngineError::Rename(RenameError::MaskChain {
        entry: "a1.txt".into(),
        name: "b1.txt".into(),
    });
    let text = named(&chain).expect("a message");
    assert!(text.contains("a1.txt") && text.contains("b1.txt"), "{text}");
    assert_eq!(
        named(&EngineError::Rename(RenameError::MaskIntoArchive)),
        None
    );
    assert_eq!(named(&EngineError::Cancelled), None);
}
