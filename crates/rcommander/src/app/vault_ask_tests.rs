use rc_engine::{Command, VaultStateKind};
use rc_vault::SecretString;

use super::{Ask, Unsent, vault_command};

#[test]
fn a_question_and_its_state_belong_together() {
    for state in [VaultStateKind::Absent, VaultStateKind::Locked] {
        assert_eq!(Ask::for_state(state).map(Ask::state), Some(state));
    }
    assert_eq!(Ask::for_state(VaultStateKind::Unlocked), None);
    assert_eq!(Ask::for_code("vault_absent"), Some(Ask::Create));
}

#[test]
fn a_new_master_password_goes_only_when_typed_twice_alike() {
    let typed = SecretString::from("master pw");
    let empty = SecretString::from("");
    assert_eq!(
        vault_command(Ask::Unlock, &empty, &empty).err(),
        Some(Unsent::Empty)
    );
    assert!(matches!(
        vault_command(Ask::Unlock, &typed, &empty),
        Ok(Command::UnlockVault { .. })
    ));
    assert_eq!(
        vault_command(Ask::Create, &typed, &empty).err(),
        Some(Unsent::Mismatch)
    );
    assert!(matches!(
        vault_command(Ask::Create, &typed, &typed),
        Ok(Command::CreateVault { .. })
    ));
}
