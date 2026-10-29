use super::buttons;
use crate::app::connections::{ConnMsg, RdmImportMsg, import::ImportMsg};

/// The ssh_config and Remote Desktop Manager imports are under the tree (RC-050-11); Total
/// Commander's moved to the settings (RC-050-10).
#[test]
fn the_tree_offers_the_ssh_config_import_and_not_total_commanders() {
    let buttons = buttons(false);
    assert!(
        buttons
            .iter()
            .any(|(_, _, message)| matches!(message, ConnMsg::Import(ImportMsg::Open)))
    );
    assert!(
        buttons
            .iter()
            .any(|(_, _, message)| matches!(message, ConnMsg::RdmImport(RdmImportMsg::Open)))
    );
    assert!(
        !buttons
            .iter()
            .any(|(_, _, message)| matches!(message, ConnMsg::TcImport(_)))
    );
}

/// A new shared credential is a button only while the setting offers them (RC-050-12).
#[test]
fn a_new_credential_is_offered_with_the_setting() {
    let offered = |on: bool| {
        buttons(on)
            .iter()
            .any(|(_, _, message)| matches!(message, ConnMsg::NewCredential))
    };
    assert!(offered(true));
    assert!(!offered(false));
}
