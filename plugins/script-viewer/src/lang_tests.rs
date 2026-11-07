use super::*;

const LANGS: [Lang; 4] = [Lang::Ruby, Lang::Perl, Lang::Lua, Lang::VbScript];

#[test]
fn every_table_is_sorted_without_duplicates() {
    for (name, table) in [
        ("RUBY_KEYWORDS", RUBY_KEYWORDS),
        ("RUBY_LITERALS", RUBY_LITERALS),
        ("RUBY_FUNCTIONS", RUBY_FUNCTIONS),
        ("PERL_KEYWORDS", PERL_KEYWORDS),
        ("PERL_LITERALS", PERL_LITERALS),
        ("PERL_FUNCTIONS", PERL_FUNCTIONS),
        ("LUA_KEYWORDS", LUA_KEYWORDS),
        ("LUA_LITERALS", LUA_LITERALS),
        ("LUA_FUNCTIONS", LUA_FUNCTIONS),
        ("VBS_KEYWORDS", VBS_KEYWORDS),
        ("VBS_LITERALS", VBS_LITERALS),
        ("VBS_FUNCTIONS", VBS_FUNCTIONS),
    ] {
        assert!(
            table.windows(2).all(|pair| pair[0] < pair[1]),
            "{name} is not sorted or holds a word twice"
        );
    }
    // VBScript's words are compared lowercased.
    for word in VBS_KEYWORDS.iter().chain(VBS_LITERALS).chain(VBS_FUNCTIONS) {
        assert_eq!(word.to_ascii_lowercase(), word.to_vec());
    }
}

#[test]
fn a_word_has_one_class_per_language() {
    for lang in LANGS {
        let (keywords, literals, functions) = lang.tables();
        for word in keywords.iter().chain(literals).chain(functions) {
            let found = [keywords, literals, functions]
                .iter()
                .filter(|table| table.contains(word))
                .count();
            assert_eq!(found, 1, "{lang:?}: {}", String::from_utf8_lossy(word));
        }
    }
}

#[test]
fn the_name_names_the_language_in_any_case() {
    for (name, lang) in [
        ("app.rb", Lang::Ruby),
        ("Gemfile", Lang::Ruby),
        ("gemfile", Lang::Ruby),
        ("Rakefile", Lang::Ruby),
        ("RAKEFILE", Lang::Ruby),
        ("build.PL", Lang::Perl),
        ("Module.pm", Lang::Perl),
        ("init.lua", Lang::Lua),
        ("logon.VBS", Lang::VbScript),
    ] {
        assert_eq!(Lang::from_name(name), Some(lang), "{name}");
    }
    for name in [
        "Gemfile.lock",
        "Rakefile.txt",
        "notes.txt",
        "rb",
        "",
        "x.py",
    ] {
        assert_eq!(Lang::from_name(name), None, "{name}");
    }
}

/// Every name and extension the manifest claims names a language, and every language has one.
#[test]
fn the_manifest_claims_every_name_the_viewer_reads() {
    let manifest = include_str!("../manifest.toml");
    let listed = |key: &str| -> Vec<&str> {
        let line = manifest
            .lines()
            .find(|line| line.starts_with(key))
            .expect("the manifest names it");
        line.split('"').skip(1).step_by(2).collect()
    };
    let extensions = listed("extensions = [");
    assert_eq!(extensions.len(), 5, "{extensions:?}");
    let mut found = Vec::new();
    for extension in extensions {
        let lang = Lang::from_name(&format!("file.{extension}"));
        assert!(lang.is_some(), "{extension}");
        found.extend(lang);
    }
    for name in listed("names = [") {
        assert_eq!(Lang::from_name(name), Some(Lang::Ruby), "{name}");
    }
    assert!(LANGS.iter().all(|lang| found.contains(lang)));
}

#[test]
fn words_are_classified_by_their_language() {
    use TokenClass::{Function, Keyword, Literal};
    assert_eq!(Lang::Ruby.classify(b"def"), Some(Keyword));
    assert_eq!(Lang::Ruby.classify(b"nil"), Some(Literal));
    assert_eq!(Lang::Ruby.classify(b"puts"), Some(Function));
    assert_eq!(Lang::Perl.classify(b"def"), None);
    assert_eq!(Lang::Perl.classify(b"my"), Some(Keyword));
    assert_eq!(Lang::Perl.classify(b"undef"), Some(Literal));
    assert_eq!(Lang::Lua.classify(b"local"), Some(Keyword));
    assert_eq!(Lang::Lua.classify(b"nil"), Some(Literal));
    assert_eq!(Lang::Lua.classify(b"ipairs"), Some(Function));
    // VBScript ignores case.
    assert_eq!(Lang::VbScript.classify(b"Dim"), Some(Keyword));
    assert_eq!(Lang::VbScript.classify(b"END"), Some(Keyword));
    assert_eq!(Lang::VbScript.classify(b"Nothing"), Some(Literal));
    assert_eq!(Lang::VbScript.classify(b"MsgBox"), Some(Function));
    assert_eq!(Lang::Ruby.classify(b"Def"), None);
}
