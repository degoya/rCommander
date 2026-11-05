use super::{NAMES_MIN_APP_VERSION, is_pattern, name_matches, validate_names};

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|name| (*name).to_owned()).collect()
}

#[test]
fn an_exact_name_ignores_case_and_nothing_else() {
    assert!(name_matches("Dockerfile", "Dockerfile"));
    assert!(name_matches("Dockerfile", "dockerfile"));
    assert!(name_matches("PKGBUILD", "pkgbuild"));
    assert!(!name_matches("Dockerfile", "Dockerfile.dev"));
    assert!(!name_matches("Dockerfile", "my-Dockerfile"));
    assert!(!is_pattern("Dockerfile"));
}

#[test]
fn a_star_is_any_run_of_characters_the_empty_one_included() {
    assert!(is_pattern("Dockerfile.*"));
    assert!(name_matches("Dockerfile.*", "Dockerfile.dev"));
    assert!(name_matches("Dockerfile.*", "DOCKERFILE.prod.arm64"));
    assert!(name_matches("Dockerfile.*", "Dockerfile."));
    assert!(!name_matches("Dockerfile.*", "Dockerfile"));
    assert!(name_matches("*.dockerfile", "app.dockerfile"));
    assert!(name_matches("*.dockerfile", "App.Dockerfile"));
    assert!(!name_matches("*.dockerfile", "app.dockerfile.bak"));
    assert!(name_matches(".env.*", ".env.local"));
    assert!(name_matches(".env.*", ".env.production"));
    assert!(!name_matches(".env.*", ".env"));
    assert!(!name_matches(".env.*", "x.env.local"));
    assert!(name_matches("*.mk", "rules.mk"));
    assert!(name_matches("a*b*c", "aXXbYYc"));
    assert!(name_matches("a*b*c", "abbbc"));
    assert!(!name_matches("a*b*c", "abcx"));
}

#[test]
fn only_the_file_name_is_compared_never_the_folder() {
    assert!(name_matches("Dockerfile", "/srv/app/Dockerfile"));
    assert!(name_matches("Dockerfile", "C:\\src\\Dockerfile"));
    assert!(!name_matches("Dockerfile", "/srv/Dockerfile/readme.txt"));
    assert!(!name_matches("*.mk", "/src/build.mk/notes.txt"));
}

#[test]
fn names_are_plain_file_names_none_twice() {
    let new = Some(NAMES_MIN_APP_VERSION);
    validate_names(&names(&["Dockerfile", "Dockerfile.*", "*.dockerfile"]), new).expect("valid");
    validate_names(&[], None).expect("no names, no version needed");
    for bad in [
        "",
        "*",
        "*.*",
        ".*",
        " Dockerfile",
        "Dockerfile ",
        "src/Dockerfile",
        "src\\Dockerfile",
        "Docker?file",
        "a:b",
        "bell\u{7}",
    ] {
        let error = validate_names(&names(&[bad]), new).expect_err(bad);
        assert!(format!("{error:#}").contains("[match] name"), "{bad}");
    }
    let twice = validate_names(&names(&["Dockerfile", "DOCKERFILE"]), new).expect_err("twice");
    assert!(format!("{twice:#}").contains("twice"));
    let long = "x".repeat(65);
    assert!(validate_names(&names(&[&long]), new).is_err());
    let many: Vec<String> = (0..33).map(|index| format!("name{index}")).collect();
    assert!(validate_names(&many, new).is_err());
}

#[test]
fn names_need_the_release_that_reads_them() {
    let list = names(&["Dockerfile"]);
    for old in [None, Some("0.1.0"), Some("0.18.9"), Some("not a version")] {
        let error = validate_names(&list, old).expect_err("too old");
        assert!(
            format!("{error:#}").contains("min_app_version 0.19.0"),
            "{old:?}"
        );
    }
    validate_names(&list, Some("0.19.0")).expect("the first release");
    validate_names(&list, Some("1.2.0")).expect("a later one");
}
