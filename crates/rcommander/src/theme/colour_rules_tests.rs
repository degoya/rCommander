use iced::Color;

use super::{compile, first_match};

#[test]
fn the_first_matching_rule_colours_a_name() {
    let rules = compile("*.exe;*.bat\t#c42b1c\t#ff99a4\n*.*\t#0078d4\n*\\\t#107c10");
    let red = Color::from_rgb8(0xc4, 0x2b, 0x1c);
    let pink = Color::from_rgb8(0xff, 0x99, 0xa4);
    let blue = Color::from_rgb8(0x00, 0x78, 0xd4);
    assert_eq!(first_match(&rules, "SETUP.EXE", false, false), Some(red));
    assert_eq!(first_match(&rules, "setup.exe", false, true), Some(pink));
    // The second rule takes what the first leaves, with the light colour in the dark look too.
    assert_eq!(first_match(&rules, "notes.txt", false, true), Some(blue));
    // A folder only by a rule that names folders.
    let green = Color::from_rgb8(0x10, 0x7c, 0x10);
    assert_eq!(first_match(&rules, "tools.exe", true, false), Some(green));
}

#[test]
fn no_rule_and_a_broken_list_colour_nothing() {
    assert_eq!(first_match(&compile(""), "a.exe", false, false), None);
    assert!(compile("*.exe\tred").is_empty());
    let rules = compile("*.exe\t#c42b1c");
    assert_eq!(first_match(&rules, "a.txt", false, false), None);
}
