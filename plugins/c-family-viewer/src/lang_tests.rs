use super::tables::*;
use super::*;

const LANGS: [Lang; 7] = [
    Lang::C,
    Lang::Cpp,
    Lang::CSharp,
    Lang::Java,
    Lang::Kotlin,
    Lang::Go,
    Lang::Rust,
];

#[test]
fn every_table_is_sorted_without_duplicates() {
    for (name, table) in [
        ("C_KEYWORDS", C_KEYWORDS),
        ("C_TYPES", C_TYPES),
        ("C_LITERALS", C_LITERALS),
        ("CPP_KEYWORDS", CPP_KEYWORDS),
        ("CSHARP_KEYWORDS", CSHARP_KEYWORDS),
        ("CSHARP_TYPES", CSHARP_TYPES),
        ("JAVA_KEYWORDS", JAVA_KEYWORDS),
        ("JAVA_TYPES", JAVA_TYPES),
        ("KOTLIN_KEYWORDS", KOTLIN_KEYWORDS),
        ("KOTLIN_TYPES", KOTLIN_TYPES),
        ("NULL_LITERALS", NULL_LITERALS),
        ("GO_KEYWORDS", GO_KEYWORDS),
        ("GO_TYPES", GO_TYPES),
        ("GO_LITERALS", GO_LITERALS),
        ("RUST_KEYWORDS", RUST_KEYWORDS),
        ("RUST_TYPES", RUST_TYPES),
        ("RUST_LITERALS", RUST_LITERALS),
    ] {
        assert!(
            table.windows(2).all(|pair| pair[0] < pair[1]),
            "{name} is not sorted or holds a word twice"
        );
    }
}

#[test]
fn a_word_has_one_class_per_language() {
    for lang in LANGS {
        let (keywords, types, literals) = lang.tables();
        for word in keywords.iter().chain(types).chain(literals) {
            let found = [keywords, types, literals]
                .iter()
                .filter(|table| table.contains(word))
                .count();
            assert_eq!(found, 1, "{lang:?}: {}", String::from_utf8_lossy(word));
        }
    }
}

#[test]
fn the_extension_names_the_language_in_any_case() {
    for (name, lang) in [
        ("main.c", Lang::C),
        ("stdio.h", Lang::Cpp),
        ("app.cpp", Lang::Cpp),
        ("APP.CC", Lang::Cpp),
        ("x.cxx", Lang::Cpp),
        ("x.hpp", Lang::Cpp),
        ("x.hh", Lang::Cpp),
        ("x.hxx", Lang::Cpp),
        ("Blink.ino", Lang::Cpp),
        ("Program.cs", Lang::CSharp),
        ("script.csx", Lang::CSharp),
        ("Main.java", Lang::Java),
        ("Main.kt", Lang::Kotlin),
        ("build.gradle.kts", Lang::Kotlin),
        ("main.go", Lang::Go),
        ("lib.rs", Lang::Rust),
    ] {
        assert_eq!(Lang::from_name(name), Some(lang), "{name}");
    }
    for name in ["README", "main.py", "a.c.txt", ".", "", "c"] {
        assert_eq!(Lang::from_name(name), None, "{name}");
    }
}

/// Every extension the manifest claims names a language, and every language has one there.
#[test]
fn the_manifest_claims_every_extension_the_viewer_reads() {
    let manifest = include_str!("../manifest.toml");
    let line = manifest
        .lines()
        .find(|line| line.starts_with("extensions = ["))
        .expect("the manifest names its extensions");
    let extensions: Vec<&str> = line.split('"').skip(1).step_by(2).collect();
    assert_eq!(extensions.len(), 16, "{extensions:?}");
    let mut found = Vec::new();
    for extension in extensions {
        let lang = Lang::from_name(&format!("file.{extension}"));
        assert!(lang.is_some(), "{extension}");
        found.extend(lang);
    }
    assert!(LANGS.iter().all(|lang| found.contains(lang)));
}

#[test]
fn words_are_classified_by_their_language() {
    use TokenClass::{Keyword, Literal, TypeName};
    assert_eq!(Lang::Rust.classify(b"fn"), Some(Keyword));
    assert_eq!(Lang::Go.classify(b"fn"), None);
    assert_eq!(Lang::Go.classify(b"func"), Some(Keyword));
    assert_eq!(Lang::Kotlin.classify(b"fun"), Some(Keyword));
    assert_eq!(Lang::Cpp.classify(b"class"), Some(Keyword));
    assert_eq!(Lang::C.classify(b"class"), None);
    assert_eq!(Lang::C.classify(b"NULL"), Some(Literal));
    assert_eq!(Lang::Go.classify(b"nil"), Some(Literal));
    assert_eq!(Lang::Java.classify(b"null"), Some(Literal));
    assert_eq!(Lang::Rust.classify(b"null"), None);
    assert_eq!(Lang::CSharp.classify(b"string"), Some(TypeName));
    assert_eq!(Lang::Rust.classify(b"usize"), Some(TypeName));
    assert_eq!(Lang::Kotlin.classify(b"Int"), Some(TypeName));
    assert_eq!(Lang::Rust.declares(b"fn"), Some(Declares::Function));
    assert_eq!(Lang::Go.declares(b"type"), Some(Declares::Type));
    assert_eq!(Lang::Kotlin.declares(b"type"), None);
}
