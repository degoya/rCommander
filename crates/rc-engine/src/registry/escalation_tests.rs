use rc_ops::ConflictChoice;

use super::{ESCALATIONS, escalation, overwrites};
use crate::{
    Command, CommandClass, LoginScriptId, Step,
    registry::{COMMANDS, samples::one_of_each, spec},
};

/// The sample of `id` from the registry's samples.
fn sample(id: &str) -> Command {
    one_of_each()
        .into_iter()
        .find(|command| command.id() == id)
        .expect("every command has a sample")
}

/// The sample of `id`, changed by `change`.
fn changed(id: &str, change: impl FnOnce(&mut Command)) -> Command {
    let mut command = sample(id);
    change(&mut command);
    command
}

/// One call of each command whose parameters escalate it (the stored cases are
/// `dispatch/mcp_tests.rs`'s).
fn escalated() -> Vec<Command> {
    let overwrite = Some(ConflictChoice::OverwriteOlder);
    vec![
        changed("ops.copy", |command| {
            if let Command::OpsCopy(request) = command {
                request.options.on_conflict = overwrite;
            }
        }),
        changed("ops.move", |command| {
            if let Command::OpsMove(request) = command {
                request.options.on_conflict = Some(ConflictChoice::Overwrite);
            }
        }),
        changed("archive.extract", |command| {
            if let Command::ArchiveExtract { options, .. } = command {
                options.on_conflict = Some(ConflictChoice::OverwriteSmaller);
            }
        }),
        changed("archive.pack", |command| {
            if let Command::ArchivePack { settings, .. } = command {
                settings.delete_sources = true;
            }
        }),
        changed("archive.pack", |command| {
            if let Command::ArchivePack { settings, .. } = command {
                settings.overwrite = true;
            }
        }),
        changed("sync.apply", |command| {
            if let Command::SyncApply(request) = command {
                request.run.deletes_confirmed = true;
            }
        }),
        changed("sync.apply", |command| {
            if let Command::SyncApply(request) = command {
                request.run.on_conflict = Some(ConflictChoice::Overwrite);
            }
        }),
        changed("connections.save", |command| {
            if let Command::SaveConnection(request) = command {
                request.login_script = Some(Some(LoginScriptId(2)));
            }
        }),
        changed("connections.save", |command| {
            if let Command::SaveConnection(request) = command {
                request.login_script = Some(None);
            }
        }),
        changed("login_scripts.create", |command| {
            if let Command::CreateLoginScript { script } = command {
                script.steps = vec![Step::SendLine];
            }
        }),
        changed("login_scripts.update", |command| {
            if let Command::UpdateLoginScript { patch, .. } = command {
                patch.steps = Some(Vec::new());
            }
        }),
    ]
}

/// The same commands as they run without escalating: no overwrite, no deletes, no script.
fn plain() -> Vec<Command> {
    vec![
        changed("ops.copy", |command| {
            if let Command::OpsCopy(request) = command {
                request.options.on_conflict = Some(ConflictChoice::RenameExisting);
            }
        }),
        changed("ops.move", |command| {
            if let Command::OpsMove(request) = command {
                request.options.on_conflict = Some(ConflictChoice::Resume);
            }
        }),
        changed("archive.extract", |command| {
            if let Command::ArchiveExtract { options, .. } = command {
                options.on_conflict = None;
            }
        }),
        sample("archive.pack"),
        sample("sync.apply"),
        changed("connections.save", |command| {
            if let Command::SaveConnection(request) = command {
                request.login_script = None;
            }
        }),
        changed("login_scripts.create", |command| {
            if let Command::CreateLoginScript { script } = command {
                script.steps.clear();
            }
        }),
        changed("login_scripts.update", |command| {
            if let Command::UpdateLoginScript { patch, .. } = command {
                patch.steps = None;
            }
        }),
    ]
}

#[test]
fn an_escalated_call_takes_its_rows_class() {
    for command in escalated() {
        let row = escalation(command.id()).expect("an escalation row");
        assert_eq!(command.effective_class(), row.class, "{command:?}");
        assert_ne!(
            command.effective_class(),
            command.spec().class,
            "{command:?}"
        );
    }
}

#[test]
fn a_plain_call_keeps_its_commands_class() {
    for command in plain() {
        assert_eq!(
            command.effective_class(),
            command.spec().class,
            "{command:?}"
        );
    }
    for command in one_of_each() {
        if escalation(command.id()).is_none() {
            assert_eq!(
                command.effective_class(),
                command.spec().class,
                "{command:?}"
            );
        }
    }
}

/// Every row names a command, sorted and once, and is stricter than the command's own class;
/// every row is reached by a call here or by a stored case.
#[test]
fn every_row_is_a_stricter_class_of_a_command() {
    let ids: Vec<&str> = ESCALATIONS.iter().map(|row| row.id).collect();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(ids, sorted, "keep the rows sorted by id, one per command");
    let reached: Vec<&str> = escalated().iter().map(Command::id).collect();
    for row in ESCALATIONS {
        let command = spec(row.id).expect("the row names a command");
        assert_eq!(command.class.stricter(row.class), row.class, "{}", row.id);
        assert_ne!(command.class, row.class, "{}", row.id);
        let stored = matches!(row.id, "diagnostics.set_retention" | "queue.load");
        assert!(
            stored || reached.contains(&row.id),
            "{} is never reached",
            row.id
        );
        assert!(!row.when.contains('"'), "{}: no quotes in when", row.id);
    }
    assert!(COMMANDS.len() > ESCALATIONS.len());
}

#[test]
fn stricter_is_written_out_not_the_order_of_the_variants() {
    use CommandClass::{Destructive, Exec, GuiOnly, Read, Write};
    // An executing call that also loses data stays asked every time.
    assert_eq!(Destructive.stricter(Exec), Destructive);
    assert_eq!(Exec.stricter(Destructive), Destructive);
    assert_eq!(Write.stricter(Exec), Exec);
    assert_eq!(Read.stricter(Write), Write);
    for class in CommandClass::ALL {
        assert_eq!(class.stricter(class), class);
        assert_eq!(class.stricter(Read), class);
        assert_eq!(Read.stricter(class), class);
        assert_eq!(class.stricter(GuiOnly), GuiOnly);
    }
}

#[test]
fn only_the_four_overwrite_answers_overwrite() {
    for (choice, expected) in [
        (Some(ConflictChoice::Overwrite), true),
        (Some(ConflictChoice::OverwriteOlder), true),
        (Some(ConflictChoice::OverwriteLarger), true),
        (Some(ConflictChoice::OverwriteSmaller), true),
        (Some(ConflictChoice::Skip), false),
        (Some(ConflictChoice::Rename), false),
        (Some(ConflictChoice::RenameExisting), false),
        (Some(ConflictChoice::Cancel), false),
        (Some(ConflictChoice::Resume), false),
        (None, false),
    ] {
        assert_eq!(overwrites(choice), expected, "{choice:?}");
    }
}
