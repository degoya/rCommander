use crate::lang::Lang;
use crate::test_support::{class_of, lexed, texts_of};
use crate::types::TokenClass::{
    Comment, Function, Key, Keyword, Number, Operator, Punctuation, String, TypeName,
};

/// A Perl file with what sets the language apart: sigils, `$#last`, `qw`, `=>`, interpolation,
/// `s///` with flags, a pattern after `=~`, `sub`, a method call, `//`, a heredoc, POD, and POD
/// after `__END__`.
const SAMPLE: &str = r#"#!/usr/bin/perl
use strict;
package My::Shop;

my $total = 0;
my @items = qw(apple pear);
my %prices = (apple => 1.5, pear => 2);
$total += $prices{$_} for @items;
print "Total: $total for @items\n";
(my $copy = $text) =~ s/\s+$//g;
if ($line =~ /^#(\d+)/) { say $1 }
sub price_of {
    my ($self, $name) = @_;
    return $self->{prices}{$name} // 0;
}
my $sql = <<"SQL";
SELECT * FROM t WHERE id = $id
SQL
print STDERR $#items, "\n";

=pod

Documentation.

=cut

__END__
data
=head1 NAME
after the end"#;

#[test]
fn a_perl_sample_is_coloured_like_an_editor() {
    let lines = lexed(Lang::Perl, SAMPLE);
    assert_eq!(lines[0], vec![("#!/usr/bin/perl", Comment)]);
    assert_eq!(
        lines[1],
        vec![("use", Keyword), ("strict", TypeName), (";", Punctuation)]
    );
    assert_eq!(class_of(&lines, "My::Shop"), Some(TypeName));
    assert_eq!(
        lines[5],
        vec![
            ("my", Keyword),
            ("@items", Key),
            ("=", Operator),
            ("qw(apple pear)", String),
            (";", Punctuation)
        ]
    );
    assert_eq!(
        texts_of(&lines[6..7], Key),
        vec!["%prices", "apple", "pear"]
    );
    assert_eq!(texts_of(&lines[6..7], Number), vec!["1.5", "2"]);
    assert_eq!(
        texts_of(&lines[7..8], Key),
        vec!["$total", "$prices", "$_", "@items"]
    );
    assert_eq!(
        lines[8],
        vec![
            ("print", Function),
            ("\"Total: ", String),
            ("$total", Key),
            (" for ", String),
            ("@items", Key),
            ("\\n\"", String),
            (";", Punctuation)
        ]
    );
    assert_eq!(class_of(&lines[9..10], "s/\\s+$//g"), Some(String));
    assert_eq!(class_of(&lines[9..10], "=~"), Some(Operator));
    // `#` inside a pattern starts no comment.
    assert_eq!(class_of(&lines[10..11], "/^#(\\d+)/"), Some(String));
    assert_eq!(class_of(&lines[10..11], "say"), Some(Function));
    assert_eq!(class_of(&lines[10..11], "$1"), Some(Key));
    assert_eq!(class_of(&lines, "price_of"), Some(Function));
    assert_eq!(class_of(&lines, "@_"), Some(Key));
    assert_eq!(
        lines[13],
        vec![
            ("return", Keyword),
            ("$self", Key),
            ("->", Operator),
            ("{", Punctuation),
            ("prices", Key),
            ("}", Punctuation),
            ("{", Punctuation),
            ("$name", Key),
            ("}", Punctuation),
            ("//", Operator),
            ("0", Number),
            (";", Punctuation)
        ]
    );
    assert_eq!(texts_of(&lines[15..16], String), vec!["<<\"SQL\""]);
    assert_eq!(
        lines[16],
        vec![("SELECT * FROM t WHERE id = ", String), ("$id", Key)]
    );
    assert_eq!(lines[17], vec![("SQL", String)]);
    assert_eq!(class_of(&lines[18..19], "$#items"), Some(Key));
    assert_eq!(class_of(&lines[18..19], "STDERR"), None);
    for index in [20, 22, 24, 28, 29] {
        assert_eq!(lines[index].len(), 1, "line {index}");
        assert_eq!(lines[index][0].1, Comment, "line {index}");
    }
    assert!(lines[25].is_empty());
    assert_eq!(lines[26], vec![("__END__", Keyword)]);
    assert!(lines[27].is_empty(), "data after __END__");
}

#[test]
fn quote_like_operators_take_any_delimiter_and_nest_brackets() {
    let lines = lexed(
        Lang::Perl,
        "$s =~ tr{a-z}{A-Z};\nmy $q = qq{a {b} $c};\nmy $r = q(x\ny) . 1;",
    );
    assert_eq!(class_of(&lines, "tr{a-z}{A-Z}"), Some(String));
    assert_eq!(texts_of(&lines[1..2], String), vec!["qq{a {b} ", "}"]);
    assert_eq!(class_of(&lines[1..2], "$c"), Some(Key));
    assert_eq!(texts_of(&lines[2..3], String), vec!["q(x"]);
    assert_eq!(lines[3][0], ("y)", String));
    assert_eq!(texts_of(&lines[3..], Number), vec!["1"]);
}

#[test]
fn barewords_file_tests_handles_and_methods_are_no_strings() {
    let lines = lexed(
        Lang::Perl,
        "my %h = (y => 1, s => 2);\nprint $h{q} if -s $file;\nmy $in = <STDIN>;\nmy $o = Foo->new; &helper(1);",
    );
    assert_eq!(texts_of(&lines[..1], Key), vec!["%h", "y", "s"]);
    assert_eq!(texts_of(&lines[1..2], Key), vec!["$h", "q", "$file"]);
    assert!(texts_of(&lines[..2], String).is_empty());
    assert_eq!(class_of(&lines, "<STDIN>"), Some(Key));
    assert_eq!(class_of(&lines, "Foo"), Some(TypeName));
    assert_eq!(class_of(&lines, "new"), Some(Function));
    assert_eq!(class_of(&lines, "&helper"), Some(Function));
}

#[test]
fn a_division_is_no_pattern_and_split_takes_one() {
    let lines = lexed(
        Lang::Perl,
        "my $x = $a / $b / 2;\nmy @f = split /,/, $line;",
    );
    assert_eq!(texts_of(&lines[..1], Operator), vec!["=", "/", "/"]);
    assert_eq!(texts_of(&lines[1..], String), vec!["/,/"]);
}
