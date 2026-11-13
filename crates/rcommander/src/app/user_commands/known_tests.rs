//! The own commands read in place (RC-110-29): `with_commands` shows the list `set_commands`
//! stored, and its answer leaves the lock.

use rc_engine::{CommandRef, UserCommand};

use super::{commands, set_commands, with_commands};

fn command(name: &str, tooltip: &str) -> UserCommand {
    UserCommand {
        id: 1,
        name: name.into(),
        action: CommandRef::default(),
        tooltip: tooltip.into(),
        icon_path: String::new(),
        icon_index: 0,
    }
}

#[test]
fn with_commands_reads_the_stored_list_in_place() {
    set_commands(vec![
        command("em_np", "Notepad"),
        command("em_calc", "Calculator"),
    ]);
    let tooltip = with_commands(|list| {
        list.iter()
            .find(|command| command.name == "em_calc")
            .map(|command| command.tooltip.clone())
    });
    assert_eq!(tooltip.as_deref(), Some("Calculator"));
    assert_eq!(with_commands(|list| list.len()), 2);
    // The lock is free again: the list can be set and copied.
    set_commands(Vec::new());
    assert!(commands().is_empty());
}
