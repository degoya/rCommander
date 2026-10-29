use super::{SecretField, SourceState};
use crate::WIT;

#[test]
fn the_enums_are_named_as_the_contract_names_them() {
    let states = [
        SourceState::Ready,
        SourceState::Locked,
        SourceState::Unreachable,
        SourceState::Unlicensed,
    ];
    let names: Vec<&str> = states.iter().map(|state| state.as_str()).collect();
    assert!(WIT.contains(&format!("enum source-state {{ {} }}", names.join(", "))));
    let fields = [
        SecretField::Password,
        SecretField::PrivateKey,
        SecretField::Passphrase,
        SecretField::Totp,
    ];
    let names: Vec<&str> = fields.iter().map(|field| field.as_str()).collect();
    assert!(WIT.contains(&format!("enum secret-field {{ {} }}", names.join(", "))));
}
