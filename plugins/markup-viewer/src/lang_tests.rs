use super::*;

#[test]
fn the_extension_names_the_format_in_any_case() {
    for (name, lang) in [
        ("index.rst", Lang::Rst),
        ("README.RST", Lang::Rst),
        ("thesis.tex", Lang::Latex),
        ("mystyle.sty", Lang::Latex),
        ("refs.bib", Lang::Bibtex),
        ("REFS.BIB", Lang::Bibtex),
    ] {
        assert_eq!(Lang::from_name(name), Some(lang), "{name}");
    }
    for name in ["notes.txt", "rst", "", "x.md", "a.cls"] {
        assert_eq!(Lang::from_name(name), None, "{name}");
    }
}

/// Every extension the manifest claims names a format, and every format has one there.
#[test]
fn the_manifest_claims_every_extension_the_viewer_reads() {
    let manifest = include_str!("../manifest.toml");
    let line = manifest
        .lines()
        .find(|line| line.starts_with("extensions = ["))
        .expect("the manifest names its extensions");
    let extensions: Vec<&str> = line.split('"').skip(1).step_by(2).collect();
    assert_eq!(extensions.len(), 4, "{extensions:?}");
    let mut found = Vec::new();
    for extension in extensions {
        let lang = Lang::from_name(&format!("file.{extension}"));
        assert!(lang.is_some(), "{extension}");
        found.extend(lang);
    }
    assert!(
        [Lang::Rst, Lang::Latex, Lang::Bibtex]
            .iter()
            .all(|lang| found.contains(lang))
    );
}
