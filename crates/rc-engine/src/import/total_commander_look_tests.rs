use super::{TcLook, TcLookKind, colours, font, rules};
use crate::{
    import::{tc_ini, total_commander::TcSkip},
    settings::ColourRule,
};

/// The colour and font sections of the owner's `wincmd.ini` (2026-10-26).
pub(crate) const OWNERS: &str = "\
[Colors]
InverseCursor=0
InverseSelection=0
BackColor=-1
BackColor2=15132390
ForeColor=-1
MarkColor=-1
CursorColor=128
CursorText=-1
[AllResolutions]
FontSize=10
FontName=Consolas
FontSizeWindow=10
FontNameWindow=Roboto Mono for Powerline
FontWeight=400
[ColorsDark]
InverseCursor=0
ThemedCursor=1
InverseSelection=0
BackColor=-1
BackColor2=3158064
ForeColor=-1
MarkColor=-1
CursorColor=-1
CursorText=-1
";

fn colour(key: &str, source: &str, dark: bool, token: &'static str, value: &str) -> TcLook {
    TcLook {
        key: key.to_owned(),
        source: source.to_owned(),
        kind: Ok(TcLookKind::Colour {
            dark,
            token,
            value: value.to_owned(),
        }),
    }
}

fn skipped(key: &str, source: &str, skip: TcSkip) -> TcLook {
    TcLook {
        key: key.to_owned(),
        source: source.to_owned(),
        kind: Err(skip),
    }
}

#[test]
fn the_owners_file_gives_three_colours_and_the_list_font() {
    let ini = tc_ini::parse(OWNERS);
    assert_eq!(
        colours(&ini),
        [
            colour(
                "colour.light.stripe",
                "[Colors] BackColor2=15132390",
                false,
                "stripe",
                "#e6e6e6"
            ),
            colour(
                "colour.light.cursor",
                "[Colors] CursorColor=128",
                false,
                "cursor",
                "#800000"
            ),
            colour(
                "colour.dark.stripe",
                "[ColorsDark] BackColor2=3158064",
                true,
                "stripe",
                "#303030"
            ),
        ]
    );
    assert_eq!(rules(&ini), []);
    assert_eq!(
        font(&ini),
        Some(TcLook {
            key: "font.list".to_owned(),
            source: "[AllResolutions] FontName=Consolas, FontSize=10".to_owned(),
            kind: Ok(TcLookKind::ListFont {
                family: "Consolas".to_owned(),
                size: Some(13),
            }),
        })
    );
}

#[test]
fn a_themed_cursor_gives_no_row_and_inverse_colours_are_skipped() {
    let ini = tc_ini::parse(
        "[Colors]\nInverseCursor=1\nInverseSelection=1\nForeColor=16777216\nBackColor=blue\n\
         [ColorsDark]\nThemedCursor=1\nCursorColor=255\nCursorText=0\nMarkColor=65280\n",
    );
    assert_eq!(
        colours(&ini),
        [
            skipped(
                "colour.light.text",
                "[Colors] ForeColor=16777216",
                TcSkip::ColourInvalid
            ),
            skipped(
                "colour.light.background",
                "[Colors] BackColor=blue",
                TcSkip::ColourInvalid
            ),
            skipped(
                "colour.light.inverse_cursor",
                "[Colors] InverseCursor=1",
                TcSkip::Inverse
            ),
            skipped(
                "colour.light.inverse_selection",
                "[Colors] InverseSelection=1",
                TcSkip::Inverse
            ),
            colour(
                "colour.dark.marked",
                "[ColorsDark] MarkColor=65280",
                true,
                "marked",
                "#00ff00"
            ),
        ]
    );
}

#[test]
fn rules_run_to_the_first_gap_with_their_dark_colour_when_there_is_one() {
    let ini = tc_ini::parse(
        "[Colors]\nColorFilter1=*.exe;*.bat\nColorFilter1Color=255\n\
         ColorFilter2=>Big files\nColorFilter2Color=65280\n\
         ColorFilter3=*.txt\nColorFilter3Color=-1\n\
         ColorFilter4=*.md\nColorFilter4Color=16711680\n\
         ColorFilter6=*.log\nColorFilter6Color=255\n\
         [ColorsDark]\nColorFilter1Color=16711680\nColorFilter4Color=nonsense\n",
    );
    let rule = |mask: &str, light: &str, dark: Option<&str>| {
        Ok(TcLookKind::Rule(ColourRule {
            mask: mask.to_owned(),
            light: light.to_owned(),
            dark: dark.map(str::to_owned),
        }))
    };
    let found = rules(&ini);
    let kinds: Vec<_> = found.iter().map(|look| look.kind.clone()).collect();
    assert_eq!(
        kinds,
        [
            rule("*.exe;*.bat", "#ff0000", Some("#0000ff")),
            Err(TcSkip::SearchFilter),
            Err(TcSkip::ColourInvalid),
            rule("*.md", "#0000ff", None),
        ]
    );
    assert_eq!(found[0].key, "rule.1");
    assert_eq!(found[0].source, "[Colors] ColorFilter1=*.exe;*.bat");
}

#[test]
fn font_points_become_pixels_within_the_limits() {
    let size = |points: &str| {
        let ini = tc_ini::parse(&format!(
            "[AllResolutions]\nFontName=Consolas\nFontSize={points}\n"
        ));
        match font(&ini).map(|look| look.kind) {
            Some(Ok(TcLookKind::ListFont { size, .. })) => size,
            other => panic!("a font, got {other:?}"),
        }
    };
    assert_eq!(size("9"), Some(12));
    assert_eq!(size("10"), Some(13));
    assert_eq!(size("11"), Some(15));
    assert_eq!(size("4"), Some(8));
    assert_eq!(size("30"), Some(24));
    assert_eq!(size("-12"), None);
    let no_size = tc_ini::parse("[AllResolutions]\nFontName=Consolas\n");
    assert!(matches!(
        font(&no_size).map(|look| look.kind),
        Some(Ok(TcLookKind::ListFont { size: None, .. }))
    ));
    assert_eq!(
        font(&tc_ini::parse("[1920x1080 (8x16)]\nFontName=Arial\n")),
        None
    );
    assert_eq!(font(&tc_ini::parse("[AllResolutions]\nFontName=\n")), None);
}
