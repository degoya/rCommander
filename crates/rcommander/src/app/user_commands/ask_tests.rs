use rc_engine::{AskRequest, AskSource, Command, PanelId};

use super::*;

fn request(source: AskSource) -> AskRequest {
    AskRequest {
        source,
        panel: PanelId::Right,
        params: "\"%P%N\"".into(),
        title: "Notepad".into(),
    }
}

#[test]
fn an_own_command_runs_again_with_the_parameters_typed() {
    let asked = request(AskSource::Own {
        name: "em_np".into(),
    });
    assert_eq!(
        command_of(&asked, "readme.txt".into()),
        Command::RunUserCommand {
            name: "em_np".into(),
            panel: Some(PanelId::Right),
            params: Some("readme.txt".into()),
        }
    );
}

#[test]
fn a_start_menu_entry_runs_again_with_the_parameters_typed() {
    let asked = request(AskSource::Start { id: 7 });
    assert_eq!(
        command_of(&asked, String::new()),
        Command::RunStartItem {
            id: 7,
            panel: Some(PanelId::Right),
            params: Some(String::new()),
        }
    );
}

/// RC-100-91: a file's internal association runs again with the parameters typed.
#[test]
fn an_association_runs_again_with_the_parameters_typed() {
    let asked = request(AskSource::Association {
        path: "/home/me/app.log".into(),
        verb: "open".into(),
    });
    assert_eq!(
        command_of(&asked, "-v %1".into()),
        Command::RunAssociation {
            panel: Some(PanelId::Right),
            path: Some("/home/me/app.log".into()),
            verb: "open".into(),
            params: Some("-v %1".into()),
        }
    );
}
