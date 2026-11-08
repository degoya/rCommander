//! rCommander itself: which build runs (RC-010-19), making this copy portable (RC-100-95) and
//! quitting it (F10, RC-080-17). `app.quit` is `GuiOnly`: ending the person's window is the
//! person's decision, never an agent's. So is `app.make_portable`: it copies the database, the
//! encrypted secrets among it, onto what may be a stranger's drive.

use super::{CommandClass, CommandSpec};

/// This group's rows, sorted by id.
pub(super) const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: "app.about",
        class: CommandClass::Read,
        summary: "Which build runs: version, commit, build time, platform, licence, authors and addresses",
    },
    CommandSpec {
        id: "app.make_portable",
        class: CommandClass::GuiOnly,
        summary: "Make this copy portable: a checked copy of the database and the installed plugins into data beside the program, then the marker rcommander.portable; takes effect at the next start",
    },
    CommandSpec {
        id: "app.quit",
        class: CommandClass::GuiOnly,
        summary: "Quit rCommander; with transfers running or waiting the window asks first",
    },
];

/// This group's [`super::Command`] variants, each after its id: `command.rs` gathers the
/// groups' lists into the enum, its serde names and [`super::Command::id`].
macro_rules! commands {
    ($next:ident, $rest:tt, { $($done:tt)* }) => {
        $next! { $rest, { $($done)*
            "app.about" => About {},
            "app.make_portable" => MakePortable {},
            "app.quit" => Quit {},
        } }
    };
}
pub(super) use commands;
