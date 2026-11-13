use super::{split_extension, split_name};

/// The two rules side by side: they part only where the name ends in a dot.
#[test]
fn the_two_rules_differ_only_for_a_name_that_ends_in_a_dot() {
    // (name, split_name, split_extension)
    let cases = [
        ("notes.", ("notes.", ""), ("notes", "")),
        ("a..", ("a..", ""), ("a.", "")),
        (".bashrc", (".bashrc", ""), (".bashrc", "")),
        ("a.tar.gz", ("a.tar", "gz"), ("a.tar", "gz")),
        ("noext", ("noext", ""), ("noext", "")),
        ("notes.txt", ("notes", "txt"), ("notes", "txt")),
        (".config.bak", (".config", "bak"), (".config", "bak")),
        ("..x", (".", "x"), (".", "x")),
        (".", (".", ""), (".", "")),
        ("..", ("..", ""), (".", "")),
        ("", ("", ""), ("", "")),
    ];
    for (name, strict, loose) in cases {
        assert_eq!(split_name(name), strict, "split_name({name:?})");
        assert_eq!(split_extension(name), loose, "split_extension({name:?})");
    }
}

#[test]
fn a_name_split_by_split_name_joins_back() {
    for name in ["notes.", ".bashrc", "a.tar.gz", "noext", "a..", "."] {
        let (stem, extension) = split_name(name);
        let joined = if extension.is_empty() {
            stem.to_owned()
        } else {
            format!("{stem}.{extension}")
        };
        assert_eq!(joined, name);
    }
}
