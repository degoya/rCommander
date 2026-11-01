use super::*;

#[test]
fn the_mnemonic_is_the_character_after_the_ampersand() {
    assert_eq!(mnemonic("&File"), Some('f'));
    assert_eq!(mnemonic("C&onfiguration"), Some('o'));
    assert_eq!(mnemonic("Divider to &50 %"), Some('5'));
    assert_eq!(mnemonic("Copy && paste"), None, "&& is an ampersand");
    assert_eq!(mnemonic("Copy && &paste"), Some('p'));
    assert_eq!(mnemonic("Plain"), None);
    assert_eq!(mnemonic("Trailing &"), None);
}

fn keys() -> AltKeys {
    AltKeys {
        active: true,
        letters_free: true,
        letters: vec!['f', 'm', 'o'],
        ..AltKeys::default()
    }
}

/// Alt pressed and let go alone highlights the bar, as in every Windows program; not after
/// another key, a click, with Ctrl or Shift (AltGr is Ctrl+Alt), or while the bar is inactive.
#[test]
fn alt_alone_highlights_the_bar() {
    let mut alt = keys();
    alt.down(true, false, false);
    alt.down(true, false, true);
    assert!(alt.alt_up(false), "Alt alone, held a while");
    assert!(!alt.alt_up(false), "once");

    alt.down(true, false, false);
    alt.down(false, false, false);
    assert!(!alt.alt_up(false), "Alt+F4, Alt+Tab, Alt+Enter");

    alt.down(true, false, false);
    alt.disarm();
    assert!(!alt.alt_up(false), "a click while Alt is held");

    alt.down(true, true, false);
    assert!(!alt.alt_up(true), "AltGr, Shift+Alt");

    alt.down(true, false, false);
    assert!(!alt.alt_up(true), "Shift pressed while Alt was held");

    alt.active = false;
    alt.down(true, false, false);
    assert!(!alt.alt_up(false), "a dialog is open or the bar hidden");
}

/// Alt with a menu's letter opens that menu - in either case - unless the letter belongs to the
/// quick search or a terminal, Ctrl is held, or no menu has it.
#[test]
fn alt_with_a_letter_opens_its_menu() {
    let mut alt = keys();
    assert_eq!(alt.letter('f', false), Some('f'));
    assert_eq!(alt.letter('F', false), Some('f'));
    assert_eq!(alt.letter('x', false), None, "no menu has it");
    assert_eq!(
        alt.letter('f', true),
        None,
        "Ctrl+Alt is AltGr or the quick search"
    );
    alt.letters_free = false;
    assert_eq!(
        alt.letter('f', false),
        None,
        "the quick search's or the terminal's"
    );
    alt.letters_free = true;
    alt.active = false;
    assert_eq!(
        alt.letter('f', false),
        None,
        "a dialog is open or the bar hidden"
    );
}

/// `SC_KEYMENU` goes on to Windows for Alt alone and a letter the bar may take; while the app
/// draws the bar only for Alt+Space, so Alt alone opens no system menu (RC-100-66).
#[test]
fn the_key_menu_passes_by_the_bars_kind() {
    let mut alt = keys();
    assert!(
        alt.key_menu_passes(None, false),
        "Alt alone highlights the bar"
    );
    assert!(alt.key_menu_passes(Some('F'), false));
    assert!(!alt.key_menu_passes(Some('x'), false), "no menu has it");
    assert!(!alt.key_menu_passes(Some('f'), true), "Ctrl+Alt");

    alt = AltKeys {
        active: false,
        drawn: true,
        ..keys()
    };
    assert!(!alt.key_menu_passes(None, false), "no system menu on Alt");
    assert!(
        !alt.key_menu_passes(Some('f'), false),
        "the drawn bar's letter"
    );
    assert!(!alt.key_menu_passes(Some('x'), false));
    assert!(
        alt.key_menu_passes(Some(' '), false),
        "Alt+Space, the system menu"
    );
}

/// The system's menu shows while the setting is on and the app does not draw the bar.
#[test]
fn the_native_menu_shows_only_when_not_drawn() {
    let look = Look {
        shown: true,
        dark: false,
        enabled: true,
        alt_letters: true,
        drawn: false,
    };
    assert!(look.native());
    assert!(
        !Look {
            drawn: true,
            ..look
        }
        .native()
    );
    assert!(
        !Look {
            shown: false,
            ..look
        }
        .native()
    );
}

/// `RC_MENU_BAR_THEME`: unset, empty or unknown is the fix; the other values turn rCommander's
/// parts off one at a time.
#[test]
fn the_diagnostic_switch_reads_its_values() {
    assert_eq!(BarPaint::parse(None), BarPaint::Opaque);
    assert_eq!(BarPaint::parse(Some("")), BarPaint::Opaque);
    assert_eq!(BarPaint::parse(Some("dark")), BarPaint::Opaque, "unknown");
    assert_eq!(BarPaint::parse(Some("opaque")), BarPaint::Opaque);
    assert_eq!(BarPaint::parse(Some(" Muda ")), BarPaint::Muda);
    assert_eq!(BarPaint::parse(Some("system")), BarPaint::System);
    assert_eq!(BarPaint::parse(Some("LIGHT")), BarPaint::Light);
    assert_eq!(BarPaint::parse(Some("nosubclass")), BarPaint::NoSubclass);
}

#[test]
fn each_switch_turns_off_its_part() {
    use BarPaint::{Light, Muda, NoSubclass, Opaque, System};
    // (switch, muda dark, opaque buffer, Alt subclass, popups follow)
    let table = [
        (Opaque, true, true, true, true),
        (Muda, true, false, true, true),
        (System, false, false, true, true),
        (Light, false, false, true, false),
        (NoSubclass, true, false, false, true),
    ];
    for (paint, muda, opaque, subclass, popups) in table {
        assert_eq!(paint.muda_dark(true), muda, "{paint:?}");
        assert_eq!(paint.opaque(true), opaque, "{paint:?}");
        assert_eq!(paint.subclass(), subclass, "{paint:?}");
        assert_eq!(paint.popups(), popups, "{paint:?}");
        assert!(
            !paint.muda_dark(false),
            "{paint:?}: the light look is Windows'"
        );
        assert!(
            !paint.opaque(false),
            "{paint:?}: the light look is Windows'"
        );
    }
}
