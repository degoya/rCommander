use serde_json::Value;

use super::{
    MAX_WSL_THEMES, WSL_THEMES_KEY, WslTheme, parse_wsl_themes, write_wsl_themes, wsl_theme,
};
use crate::settings::schema::setting;

fn row(distro: &str, light: Option<&str>, dark: Option<&str>) -> WslTheme {
    WslTheme {
        distro: distro.to_owned(),
        light: light.map(str::to_owned),
        dark: dark.map(str::to_owned),
    }
}

#[test]
fn a_line_names_a_distribution_and_a_theme_per_mode() {
    let rows = parse_wsl_themes("Ubuntu\tnord-light\tsolarized-dark\nDebian\t\tnord-dark\n")
        .expect("valid");
    assert_eq!(
        rows,
        vec![
            row("Ubuntu", Some("nord-light"), Some("solarized-dark")),
            row("Debian", None, Some("nord-dark")),
        ]
    );
    assert_eq!(
        parse_wsl_themes("Arch\tnord-light").expect("no dark field"),
        vec![row("Arch", Some("nord-light"), None)]
    );
    assert_eq!(parse_wsl_themes("").expect("empty"), Vec::new());
}

#[test]
fn a_name_compares_without_case_and_a_later_line_replaces_an_earlier() {
    let rows =
        parse_wsl_themes("Ubuntu\tnord-light\t\nubuntu\t\tnord-dark\nKali\t\t").expect("valid");
    assert_eq!(
        rows,
        vec![row("ubuntu", None, Some("nord-dark"))],
        "the later line wins, one without themes is dropped"
    );
    let found = wsl_theme(&rows, "UBUNTU").expect("found");
    assert_eq!(found.theme(true), Some("nord-dark"));
    assert_eq!(found.theme(false), None);
    assert!(wsl_theme(&rows, "Debian").is_none());
}

#[test]
fn a_line_that_is_no_distribution_and_ids_is_refused() {
    for wrong in [
        "Ubuntu",
        "\tnord-light\t",
        "Ubu\\ntu\tnord-light\t",
        "Ubuntu\tNord Light\t",
        "Ubuntu\tnord-light\tnord-dark\textra",
        "..\tnord-light",
    ] {
        assert_eq!(parse_wsl_themes(wrong), None, "{wrong:?}");
    }
    let long = "x".repeat(65);
    assert_eq!(parse_wsl_themes(&format!("{long}\tnord-light")), None);
    assert_eq!(parse_wsl_themes(&format!("Ubuntu\t{long}")), None);
}

#[test]
fn the_list_is_capped() {
    let lines = |count: usize| {
        (0..count)
            .map(|at| format!("D{at}\tnord-light\t"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    assert_eq!(
        parse_wsl_themes(&lines(MAX_WSL_THEMES)).map(|rows| rows.len()),
        Some(MAX_WSL_THEMES)
    );
    assert_eq!(parse_wsl_themes(&lines(MAX_WSL_THEMES + 1)), None);
}

#[test]
fn written_and_read_again_is_the_same_and_the_row_keeps_it() {
    let rows = vec![
        row("Ubuntu", Some("nord-light"), None),
        row("Debian", None, Some("nord-dark")),
        row("Gone", None, None),
    ];
    let text = write_wsl_themes(&rows);
    assert_eq!(text, "Ubuntu\tnord-light\t\nDebian\t\tnord-dark");
    assert_eq!(parse_wsl_themes(&text).expect("valid"), rows[..2].to_vec());

    let spec = setting(WSL_THEMES_KEY).expect("a schema row");
    assert_eq!(spec.type_name(), "wsl_themes");
    assert_eq!(spec.default_value(), Value::from(""));
    assert!(!spec.protected, "a theme changes only the look");
    assert_eq!(
        spec.check(&Value::from("ubuntu\tnord-light\n\nDebian\t\tnord-dark\n")),
        Ok(Value::from("ubuntu\tnord-light\t\nDebian\t\tnord-dark"))
    );
    assert!(spec.check(&Value::from("Ubuntu")).is_err());
    assert!(spec.check(&Value::from(1)).is_err());
    let huge = format!("Ubuntu\tnord-light\t{}", " ".repeat(5000));
    assert!(spec.check(&Value::from(huge)).is_err(), "too long");
}
