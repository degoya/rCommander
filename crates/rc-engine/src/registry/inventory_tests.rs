use crate::{CommandClass, spec};

/// Setting a source up, changing its address or token and removing it are the person's; MCP
/// lists and syncs (RC-100-39).
#[test]
fn a_source_is_set_up_by_the_person_only() {
    for (id, class) in [
        ("inventory.sources", CommandClass::Read),
        ("inventory.refresh", CommandClass::Write),
        ("inventory.source_save", CommandClass::GuiOnly),
        ("inventory.source_delete", CommandClass::GuiOnly),
    ] {
        assert_eq!(spec(id).expect("a row").class, class, "{id}");
    }
}
