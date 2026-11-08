use super::{action_of, commands, start_menu, start_menu_has_keys};
use crate::{
    import::{tc_ini, total_commander::TcSkip},
    user_commands::{CommandKind, CommandRef, Show, StartItemKind},
};

fn program(command: &str, params: &str) -> CommandRef {
    CommandRef {
        kind: CommandKind::Program,
        command: command.to_owned(),
        params: params.to_owned(),
        ..CommandRef::default()
    }
}

/// `usercmd.ini` as Total Commander writes it: a program with its placeholders, folder, tooltip,
/// symbol and window; `*` in front; `*` alone with the program in `param`; `?` kept; a list of
/// internal commands with one rCommander does not know; another own command.
const USERCMD: &str = "\
[em_np]
cmd=notepad.exe
param=\"%P%N\"
path=%COMMANDER_PATH%
menu=Edit in &Notepad
button=%windir%\\notepad.exe,1
iconic=-1
[EM_Reg]
cmd=*regedit.exe
[em_admin]
cmd=*
param=\"C:\\Program Files\\Tool\\tool.exe\" /s %P
[em_ask]
cmd=notepad.exe
param=?%P%N
iconic=1
[em_both]
cmd=cm_SelectAll,cm_NoSuchThing,em_np
[em_again]
cmd=em_both
[em_cd]
cmd=cd C:\\Work
[em_empty]
menu=nothing to run
[em_bad name]
cmd=notepad.exe
[Buttonbar]
cmd=not an own command
";

#[test]
fn usercmd_ini_reads_as_total_commander_writes_it() {
    let found = commands(&tc_ini::parse(USERCMD));
    let names: Vec<&str> = found.iter().map(|found| found.section.as_str()).collect();
    assert_eq!(
        names,
        [
            "em_np",
            "EM_Reg",
            "em_admin",
            "em_ask",
            "em_both",
            "em_again",
            "em_cd",
            "em_empty",
            "em_bad name"
        ],
        "only the em_ sections, in file order"
    );

    let np = found[0].draft.as_ref().expect("em_np");
    assert_eq!(np.name, "em_np");
    assert_eq!(
        np.action,
        CommandRef {
            folder: "%COMMANDER_PATH%".to_owned(),
            show: Show::Maximized,
            ..program("notepad.exe", "\"%P%N\"")
        },
        "a variable stays as text: the engine expands it when it runs"
    );
    assert_eq!(np.tooltip, "Edit in &Notepad");
    assert_eq!(
        (np.icon_path.as_str(), np.icon_index),
        ("%windir%\\notepad.exe", 1)
    );

    let reg = found[1].draft.as_ref().expect("em_reg");
    assert_eq!(reg.name, "em_reg", "kept in lower case");
    assert!(reg.action.elevated, "* in front of cmd");
    assert_eq!(reg.action.command, "regedit.exe");

    let admin = found[2].draft.as_ref().expect("em_admin");
    assert!(admin.action.elevated);
    assert_eq!(admin.action.command, "C:\\Program Files\\Tool\\tool.exe");
    assert_eq!(
        admin.action.params, "/s %P",
        "* alone: the program is param's first word"
    );

    let ask = found[3].draft.as_ref().expect("em_ask");
    assert_eq!(
        ask.action.params, "?%P%N",
        "? stays: the window asks before the start"
    );
    assert!(ask.action.asks());
    assert_eq!(ask.action.show, Show::Minimized);

    let both = found[4].draft.as_ref().expect("em_both");
    assert_eq!(both.action.kind, CommandKind::Internal);
    assert_eq!(
        both.action.command, "panel.select_all,cm_NoSuchThing,em_np",
        "a known cm_ becomes rCommander's row, an unknown one stays as written"
    );
    assert_eq!(found[4].unknown, ["cm_NoSuchThing"]);

    let again = found[5].draft.as_ref().expect("em_again");
    assert_eq!(again.action.kind, CommandKind::Own);
    assert_eq!(again.action.command, "em_both");

    assert_eq!(found[6].draft, Err(TcSkip::ChangeDir));
    assert_eq!(found[7].draft, Err(TcSkip::NoCommand));
    assert_eq!(found[8].draft, Err(TcSkip::NameInvalid));
}

#[test]
fn a_command_is_checked_as_rcommander_checks_it() {
    // A program among internal commands, and more rows than rCommander runs.
    assert_eq!(
        action_of("cm_Copy,notepad.exe", "", "", None),
        Err(TcSkip::CommandInvalid)
    );
    let many = vec!["cm_Copy"; 17].join(",");
    assert_eq!(action_of(&many, "", "", None), Err(TcSkip::CommandInvalid));
    assert_eq!(action_of("*", "", "", None), Err(TcSkip::NoCommand));
    // An internal command never starts as administrator: Total Commander ignores the `*` there.
    let (rows, unknown) = action_of("*cm_Copy", "", "", None).expect("rows");
    assert!(!rows.elevated);
    assert_eq!(rows.command, "copy");
    assert!(unknown.is_empty());
    // A quoted program loses its quotes.
    let (quoted, _) = action_of("\"C:\\My Tools\\x.exe\"", "%S", "\"D:\\\"", None).expect("ok");
    assert_eq!(quoted.command, "C:\\My Tools\\x.exe");
    assert_eq!(quoted.folder, "D:\\");
}

/// `[user]` with a submenu (`-Name` ... `--`), a separator (`-`), `*`, `?`, a `cm_` list and a key.
const USER: &str = "\
[user]
menu1=&Notepad
cmd1=notepad.exe
param1=?%P%N
key1=CA+N
menu2=-
menu3=-&Admin
menu4=Registry
cmd4=*regedit.exe
menu5=Select all
cmd5=cm_SelectAll
menu6=Go to work
cmd6=cd C:\\Work
menu7=--
menu8=Console
cmd8=cmd.exe
path8=C:\\
iconic8=1
";

#[test]
fn the_start_menu_reads_with_submenus_and_separators() {
    let ini = tc_ini::parse(USER);
    let found = start_menu(&ini);
    let shape: Vec<(usize, StartItemKind, &str, usize, Option<TcSkip>)> = found
        .iter()
        .map(|entry| {
            (
                entry.index,
                entry.kind,
                entry.title.as_str(),
                entry.folders.len(),
                entry.skip,
            )
        })
        .collect();
    assert_eq!(
        shape,
        [
            (1, StartItemKind::Item, "Notepad", 0, None),
            (2, StartItemKind::Separator, "", 0, None),
            (3, StartItemKind::Submenu, "Admin", 0, None),
            (4, StartItemKind::Item, "Registry", 1, None),
            (5, StartItemKind::Item, "Select all", 1, None),
            (
                6,
                StartItemKind::Item,
                "Go to work",
                1,
                Some(TcSkip::ChangeDir)
            ),
            (8, StartItemKind::Item, "Console", 0, None),
        ]
    );
    assert_eq!(found[3].folders, ["Admin"]);
    let notepad = found[0].action.as_ref().expect("an entry's command");
    assert!(notepad.asks());
    assert!(
        found[3]
            .action
            .as_ref()
            .is_some_and(|action| action.elevated)
    );
    assert_eq!(
        found[4]
            .action
            .as_ref()
            .map(|action| action.command.as_str()),
        Some("panel.select_all")
    );
    let console = found[6].action.as_ref().expect("console");
    assert_eq!(
        (console.folder.as_str(), console.show),
        ("C:\\", Show::Minimized)
    );
    assert!(found[1].action.is_none() && found[2].action.is_none());
    assert!(start_menu_has_keys(&ini), "key1 is left out and reported");
    assert!(!start_menu_has_keys(&tc_ini::parse(
        "[user]\nmenu1=x\ncmd1=y\n"
    )));
}

#[test]
fn a_skipped_submenu_skips_what_it_holds() {
    let mut text = String::from("[user]\n");
    for level in 1..=8 {
        text.push_str(&format!("menu{level}=-Level {level}\n"));
    }
    text.push_str("menu9=Deep\ncmd9=notepad.exe\n");
    let found = start_menu(&tc_ini::parse(&text));
    // Below the import's own submenu the Start menu takes 7 levels of Total Commander's.
    assert_eq!(found[6].skip, None, "{:?}", found[6]);
    assert_eq!(found[7].skip, Some(TcSkip::TooDeep));
    assert_eq!(found[8].skip, Some(TcSkip::TooDeep), "inherited");
    assert!(start_menu(&tc_ini::parse("[DirMenu]\nmenu1=x\n")).is_empty());
}
