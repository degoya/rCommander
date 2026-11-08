use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use super::{ImportPreview, PreviewResult, preview_with};
use crate::{
    ButtonBarError,
    button_bars::MAX_BUTTONS,
    import::{
        tc_ini::{self, Ini},
        total_commander::TcSkip,
    },
    user_commands::CommandKind,
};

fn folder() -> PathBuf {
    std::env::temp_dir().join("bars")
}

/// Reads the files of `table` (name in [`folder`] to text) through the INI reader.
fn read(table: &[(&str, &[u8])], name: &str) -> Result<ImportPreview, ButtonBarError> {
    let files: HashMap<PathBuf, Vec<u8>> = table
        .iter()
        .map(|(name, bytes)| (folder().join(name), bytes.to_vec()))
        .collect();
    let reader = move |path: &Path| -> Result<Ini, String> {
        files
            .get(path)
            .map(|bytes| tc_ini::parse(&tc_ini::decode(bytes)))
            .ok_or_else(|| "no such file".to_owned())
    };
    preview_with(&folder().join(name), &reader)
}

const MAIN: &[u8] = b"[Buttonbar]\r\n\
Buttoncount=6\r\n\
button1=%COMMANDER_PATH%\\wcmicons.dll,3\r\n\
cmd1=cm_SrcFull\r\n\
menu1=&Full\r\n\
button2=\r\n\
cmd2=\r\n\
button3=\r\n\
cmd3=cm_NoSuchThing\r\n\
button4=notepad.exe\r\n\
cmd4=notepad.exe\r\n\
param4=\"%P%N\"\r\n\
menu4=Notepad\r\n\
iconic4=1\r\n\
cmd5=em_np\r\n\
cmd6=tools.bar\r\n\
menu6=Tools\r\n";

const TOOLS: &[u8] = b"[Buttonbar]\r\nButtoncount=1\r\ncmd1=cm_Exit\r\n";

#[test]
fn buttons_separators_programs_and_own_commands_are_read() {
    let preview = read(&[("main.bar", MAIN), ("tools.bar", TOOLS)], "main.bar").expect("read");
    assert_eq!(preview.bars.len(), 2, "the sub-bar is read too");
    let main = &preview.bars[0];
    assert_eq!(main.name, "main");
    assert_eq!(main.file, "main.bar");
    assert_eq!(main.items.len(), 6);

    let full = &main.items[0];
    assert_eq!(full.tooltip, "Full", "the accelerator goes");
    let icon = full.icon.as_ref().expect("a symbol");
    assert_eq!(icon.index, 3);
    assert!(
        icon.path.starts_with(folder().to_string_lossy().as_ref()),
        "%COMMANDER_PATH% is the imported file's folder: {}",
        icon.path
    );
    match &full.result {
        PreviewResult::Command { action, unknown } => {
            assert_eq!(action.kind, CommandKind::Internal);
            assert_eq!(action.command, "view_full");
            assert!(unknown.is_empty());
        }
        other => panic!("cm_SrcFull is known: {other:?}"),
    }

    assert_eq!(main.items[1].result, PreviewResult::Separator);

    match &main.items[2].result {
        PreviewResult::Command { unknown, .. } => {
            assert_eq!(unknown, &["cm_NoSuchThing".to_owned()]);
        }
        other => panic!("an unknown cm_ is kept and named: {other:?}"),
    }

    match &main.items[3].result {
        PreviewResult::Command { action, .. } => {
            assert_eq!(action.kind, CommandKind::Program);
            assert_eq!(action.command, "notepad.exe");
            assert_eq!(action.params, "\"%P%N\"");
        }
        other => panic!("a program: {other:?}"),
    }

    match &main.items[4].result {
        PreviewResult::Command { action, .. } => {
            assert_eq!(action.kind, CommandKind::Own);
            assert_eq!(action.command, "em_np");
        }
        other => panic!("an own command is a reference: {other:?}"),
    }

    assert_eq!(main.items[5].result, PreviewResult::SubBar { bar: 1 });
    assert_eq!(preview.bars[1].name, "tools");
    assert_eq!(preview.bars[1].items.len(), 1);
}

#[test]
fn utf16_with_bom_and_ansi_read_alike() {
    let text = "[Buttonbar]\r\nButtoncount=1\r\ncmd1=cm_SrcFull\r\nmenu1=Voll\u{e4}ndig\r\n";
    let mut utf16 = vec![0xFF, 0xFE];
    for unit in text.encode_utf16() {
        utf16.extend_from_slice(&unit.to_le_bytes());
    }
    // Windows-1252: 0xE4 is the same letter.
    let ansi: Vec<u8> = text
        .chars()
        .map(|c| u8::try_from(u32::from(c)).expect("Latin-1"))
        .collect();
    for bytes in [utf16, ansi] {
        let preview = read(&[("full.bar", bytes.as_slice())], "full.bar").expect("read");
        let item = &preview.bars[0].items[0];
        assert_eq!(item.tooltip, "Voll\u{e4}ndig");
        assert!(matches!(item.result, PreviewResult::Command { .. }));
    }
}

#[test]
fn a_missing_or_circular_sub_bar_is_skipped() {
    let looping: &[u8] = b"[Buttonbar]\r\nButtoncount=2\r\ncmd1=loop.bar\r\ncmd2=gone.bar\r\n";
    let preview = read(&[("loop.bar", looping)], "loop.bar").expect("read");
    let items = &preview.bars[0].items;
    assert_eq!(
        items[0].result,
        PreviewResult::Skipped {
            skip: TcSkip::TooDeep
        },
        "a bar that opens itself"
    );
    assert_eq!(
        items[1].result,
        PreviewResult::Skipped {
            skip: TcSkip::InvalidPath
        }
    );
    assert_eq!(preview.bars.len(), 1);
}

#[test]
fn a_folder_change_and_a_symbol_without_command_are_skipped() {
    let bar: &[u8] = b"[Buttonbar]\r\nButtoncount=2\r\ncmd1=cd C:\\\r\nbutton2=x.ico\r\n";
    let preview = read(&[("cd.bar", bar)], "cd.bar").expect("read");
    let items = &preview.bars[0].items;
    assert_eq!(
        items[0].result,
        PreviewResult::Skipped {
            skip: TcSkip::ChangeDir
        }
    );
    assert_eq!(
        items[1].result,
        PreviewResult::Skipped {
            skip: TcSkip::NoCommand
        }
    );
}

#[test]
fn more_buttons_than_a_bar_holds_are_counted() {
    let mut text = format!("[Buttonbar]\r\nButtoncount={}\r\n", MAX_BUTTONS + 3);
    for index in 1..=MAX_BUTTONS + 3 {
        text.push_str(&format!("cmd{index}=cm_SrcFull\r\n"));
    }
    let preview = read(&[("big.bar", text.as_bytes())], "big.bar").expect("read");
    assert_eq!(preview.bars[0].items.len(), MAX_BUTTONS);
    assert_eq!(preview.bars[0].dropped, 3);
}

#[test]
fn a_file_without_buttonbar_or_that_does_not_read_is_refused() {
    let error = read(&[("x.bar", &b"[Other]\r\na=1\r\n"[..])], "x.bar").expect_err("no section");
    assert_eq!(error.code(), "button_bar_import");
    let error = read(&[], "none.bar").expect_err("no file");
    assert_eq!(error.code(), "button_bar_import");
}

#[test]
fn a_file_too_large_is_refused_from_disk() {
    let dir = std::env::temp_dir().join(format!("rc-bar-import-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("folder");
    let path = dir.join("large.bar");
    let size = usize::try_from(tc_ini::MAX_FILE_BYTES).expect("fits") + 1;
    std::fs::write(&path, vec![b';'; size]).expect("written");
    let error = super::preview(&path).expect_err("too large");
    assert_eq!(error.code(), "button_bar_import");
    std::fs::remove_dir_all(&dir).expect("removed");
}
