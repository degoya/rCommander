use crate::lang::Lang;
use crate::test_support::{class_of, lexed, texts_of};
use crate::types::TokenClass::{
    self, Comment, Function, Key, Keyword, Literal, Number, Operator, Punctuation, String, TypeName,
};

/// A Ruby file with what sets the language apart: modules and classes, `def self.name`, keyword
/// arguments, instance, class and global variables, symbols, interpolation in both forms, `%w`,
/// a regular expression, a heredoc, `=begin` and `__END__`.
const SAMPLE: &str = r#"# frozen_string_literal: true
require "json"

module Shop
  class Cart < Base
    TAX = 0.19
    attr_reader :items

    def initialize(items = [])
      @items = items
      @@count ||= 0
    end

    def self.build(name:, total: 1_000)
      new(name: name, total: total)
    end

    def total?
      @items.sum { |i| i[:price] } > 0 && $DEBUG
    end

    def to_s = "Cart(#{@items.size} items, #@items)"
  end
end

words = %w[a b c]
pattern = /\d+x/i
text = <<~EOS
  Total: #{total}
  EOS
=begin
a block comment
=end
__END__
data here"#;

fn line(
    lines: &[Vec<(&'static str, TokenClass)>],
    index: usize,
) -> Vec<(&'static str, TokenClass)> {
    lines[index].clone()
}

#[test]
fn a_ruby_sample_is_coloured_like_an_editor() {
    let lines = lexed(Lang::Ruby, SAMPLE);
    assert_eq!(
        line(&lines, 0),
        vec![("# frozen_string_literal: true", Comment)]
    );
    assert_eq!(
        line(&lines, 1),
        vec![("require", Function), ("\"json\"", String)]
    );
    assert_eq!(
        line(&lines, 3),
        vec![("module", Keyword), ("Shop", TypeName)]
    );
    assert_eq!(
        line(&lines, 4),
        vec![
            ("class", Keyword),
            ("Cart", TypeName),
            ("<", Operator),
            ("Base", TypeName)
        ]
    );
    assert_eq!(
        line(&lines, 5),
        vec![("TAX", TypeName), ("=", Operator), ("0.19", Number)]
    );
    assert_eq!(
        line(&lines, 6),
        vec![("attr_reader", Function), (":items", Literal)]
    );
    assert_eq!(class_of(&lines, "initialize"), Some(Function));
    assert_eq!(
        line(&lines, 10),
        vec![("@@count", Key), ("||=", Operator), ("0", Number)]
    );
    assert_eq!(
        line(&lines, 13),
        vec![
            ("def", Keyword),
            ("self", Keyword),
            (".", Punctuation),
            ("build", Function),
            ("(", Punctuation),
            ("name:", Key),
            (",", Punctuation),
            ("total:", Key),
            ("1_000", Number),
            (")", Punctuation)
        ]
    );
    assert_eq!(class_of(&lines, "new"), Some(Function));
    assert_eq!(class_of(&lines, "total?"), Some(Function));
    assert_eq!(class_of(&lines, ":price"), Some(Literal));
    assert_eq!(class_of(&lines, "$DEBUG"), Some(Key));
    // A method after a dot is no keyword and, without parentheses, plain.
    assert_eq!(class_of(&lines, "sum"), None);
    assert_eq!(
        line(&lines, 21),
        vec![
            ("def", Keyword),
            ("to_s", Function),
            ("=", Operator),
            ("\"Cart(", String),
            ("#{", Punctuation),
            ("@items", Key),
            (".", Punctuation),
            ("}", Punctuation),
            (" items, ", String),
            ("#@items", Key),
            (")\"", String)
        ]
    );
    assert_eq!(
        line(&lines, 25),
        vec![("=", Operator), ("%w[a b c]", String)]
    );
    assert_eq!(
        line(&lines, 26),
        vec![("=", Operator), ("/\\d+x/i", String)]
    );
    // The heredoc: its opening, a body line with its interpolation, the indented tag.
    assert_eq!(line(&lines, 27), vec![("=", Operator), ("<<~EOS", String)]);
    assert_eq!(
        line(&lines, 28),
        vec![
            ("  Total: ", String),
            ("#{", Punctuation),
            ("}", Punctuation)
        ]
    );
    assert_eq!(line(&lines, 29), vec![("  EOS", String)]);
    for (index, tokens) in lines.iter().enumerate().skip(30).take(3) {
        assert_eq!(tokens.len(), 1, "line {index}");
        assert_eq!(tokens[0].1, Comment, "line {index}");
    }
    assert_eq!(line(&lines, 33), vec![("__END__", Keyword)]);
    assert!(lines[34].is_empty(), "data after __END__");
}

#[test]
fn a_heredoc_with_a_quoted_tag_interpolates_nothing() {
    let lines = lexed(Lang::Ruby, "sql = <<-'SQL'\n  #{x}\n    SQL\nx = 1");
    assert_eq!(lines[1], vec![("  #{x}", String)]);
    assert_eq!(lines[2], vec![("    SQL", String)]);
    assert_eq!(texts_of(&lines[3..], Number), vec!["1"]);
    // A bare tag must stand at the start of its line, and `a <<b` pushes.
    let lines = lexed(Lang::Ruby, "s = <<EOS\n  EOS\nEOS\nlist <<item");
    assert_eq!(lines[1], vec![("  EOS", String)]);
    assert_eq!(lines[2], vec![("EOS", String)]);
    assert_eq!(lines[3], vec![("<<", Operator)]);
}

#[test]
fn strings_and_percent_literals_go_on_over_lines_and_nest() {
    let lines = lexed(
        Lang::Ruby,
        "s = \"one\n#{two} three\"\nq = %q(a (b)\nc) + 1",
    );
    assert_eq!(lines[0], vec![("=", Operator), ("\"one", String)]);
    assert_eq!(
        lines[1],
        vec![
            ("#{", Punctuation),
            ("}", Punctuation),
            (" three\"", String)
        ]
    );
    assert_eq!(lines[2], vec![("=", Operator), ("%q(a (b)", String)]);
    assert_eq!(
        lines[3],
        vec![("c)", String), ("+", Operator), ("1", Number)]
    );
}

#[test]
fn a_slash_after_a_value_divides() {
    let lines = lexed(Lang::Ruby, "a = b / 2 / c\nx = 10 % 3\ny = %(text)");
    assert_eq!(texts_of(&lines[..1], Operator), vec!["=", "/", "/"]);
    assert_eq!(texts_of(&lines[1..2], Operator), vec!["=", "%"]);
    assert_eq!(texts_of(&lines[2..], String), vec!["%(text)"]);
}

#[test]
fn symbols_constants_and_scope() {
    let lines = lexed(
        Lang::Ruby,
        "Foo::Bar.new(:a, :\"b c\", x ? 1 : 2) unless obj.class.empty?",
    );
    assert_eq!(class_of(&lines, "Foo"), Some(TypeName));
    assert_eq!(class_of(&lines, "::"), Some(Operator));
    assert_eq!(class_of(&lines, "Bar"), Some(TypeName));
    assert_eq!(class_of(&lines, ":a"), Some(Literal));
    assert_eq!(class_of(&lines, ":\"b c\""), Some(String));
    assert_eq!(class_of(&lines, "unless"), Some(Keyword));
    // `.class` is a method, not the keyword.
    assert_eq!(class_of(&lines, "class"), None);
}
