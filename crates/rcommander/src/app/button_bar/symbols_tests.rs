use std::io::Cursor;

use ::image::{ImageFormat, RgbaImage};
use rc_engine::{BarButton, ButtonAction, CommandKind, CommandRef, IconRef, UserCommand};

use super::{SymbolKey, Symbols, decode, key_among, key_of};

fn png(side: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    RgbaImage::new(side, side)
        .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
        .expect("encoded");
    bytes
}

#[test]
fn a_picture_is_decoded_and_scaled_to_the_bars_size() {
    let rgba = decode(&png(64), ImageFormat::Png, 24).expect("decoded");
    assert_eq!((rgba.width, rgba.height), (24, 24));
    assert_eq!(rgba.pixels.len(), 24 * 24 * 4);
    let small = decode(&png(16), ImageFormat::Png, 24).expect("decoded");
    assert_eq!(small.width, 16, "a smaller picture is not blown up");
    assert!(decode(b"no picture", ImageFormat::Png, 24).is_none());
    assert!(
        decode(&png(2048), ImageFormat::Png, 24).is_none(),
        "larger than the limit"
    );
}

#[test]
fn a_button_names_its_symbol_or_shows_its_text() {
    let mut button = BarButton {
        action: ButtonAction::Command(CommandRef {
            command: "notepad".into(),
            ..CommandRef::default()
        }),
        icon: None,
        tooltip: String::new(),
    };
    assert_eq!(key_of(&button, 24), None, "a program by name: its text");
    button.icon = Some(IconRef {
        path: "/icons/tool.png".into(),
        index: 2,
    });
    assert_eq!(
        key_of(&button, 16),
        Some(SymbolKey {
            path: "/icons/tool.png".into(),
            index: 2,
            size: 16,
        })
    );
}

#[test]
fn a_symbol_is_asked_for_once() {
    let mut symbols = Symbols::default();
    let key = SymbolKey {
        path: "/icons/tool.png".into(),
        index: 0,
        size: 24,
    };
    assert!(symbols.ask(vec![key.clone(), key.clone()]).is_some());
    assert!(symbols.ask(vec![key.clone()]).is_none(), "asked already");
    assert!(symbols.get(&key).is_none(), "not read yet");
    symbols.loaded(vec![(key.clone(), None)]);
    assert!(symbols.get(&key).is_none(), "did not read: the text shows");
}

#[test]
fn an_own_commands_button_shows_the_own_commands_symbol() {
    let mut button = BarButton {
        action: ButtonAction::Command(CommandRef {
            kind: CommandKind::Own,
            command: "em_np".into(),
            ..CommandRef::default()
        }),
        icon: None,
        tooltip: String::new(),
    };
    let own = |icon_path: &str| UserCommand {
        id: 1,
        name: "em_np".into(),
        action: CommandRef::default(),
        tooltip: String::new(),
        icon_path: icon_path.into(),
        icon_index: 3,
    };
    assert_eq!(
        key_among(&button, 24, &[own("/icons/np.ico")]),
        Some(SymbolKey {
            path: "/icons/np.ico".into(),
            index: 3,
            size: 24,
        })
    );
    assert_eq!(key_among(&button, 24, &[own("")]), None, "none of its own");
    assert_eq!(key_among(&button, 24, &[]), None, "not read yet");
    button.icon = Some(IconRef {
        path: "/icons/button.png".into(),
        index: 0,
    });
    assert_eq!(
        key_among(&button, 24, &[own("/icons/np.ico")]).map(|key| key.path),
        Some("/icons/button.png".into()),
        "the button's own symbol goes first"
    );
}

/// RC-110-10: a symbol of the collection is no file to read - its own or an own command's.
#[test]
fn a_symbol_of_the_collection_is_no_file_to_read() {
    let mut button = BarButton {
        action: ButtonAction::Command(CommandRef {
            kind: CommandKind::Own,
            command: "em_np".into(),
            ..CommandRef::default()
        }),
        icon: Some(IconRef {
            path: "rc-symbol:folder".into(),
            index: 0,
        }),
        tooltip: String::new(),
    };
    assert_eq!(key_among(&button, 24, &[]), None);
    button.icon = None;
    let own = UserCommand {
        id: 1,
        name: "em_np".into(),
        action: CommandRef::default(),
        tooltip: String::new(),
        icon_path: "rc-symbol:no_such_symbol".into(),
        icon_index: 0,
    };
    assert_eq!(key_among(&button, 24, &[own]), None);
}
