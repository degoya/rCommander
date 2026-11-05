use super::*;
use crate::types::TokenClass::{Comment, Function, Key, Keyword, Operator, Punctuation as P};

const STR: TokenClass = TokenClass::String;

/// The tokens of every line of `text`, each as its text and class, the state carried on.
fn lex(text: &str) -> Vec<Vec<(&str, TokenClass)>> {
    let mut state = LineState::default();
    text.split('\n')
        .map(|line| {
            let mut tokens: Vec<Token> = Vec::new();
            state = lex_line(line.as_bytes(), state, &mut tokens);
            tokens
                .iter()
                .map(|token| {
                    let start = token.start as usize;
                    (&line[start..start + token.len as usize], token.class)
                })
                .collect()
        })
        .collect()
}

/// A GNU Makefile: assignments of every flavour, functions, special targets, rules with
/// order-only prerequisites, pattern rules, recipes with a tab, a `define`, conditionals.
const MAKEFILE: &str = "CC ?= gcc
CFLAGS := -O2 $(EXTRA)
SRC = $(wildcard src/*.c)
OBJ = $(patsubst %.c,%.o,$(SRC))
.PHONY: all clean
all: app
app: $(OBJ) | build
\t@echo \"Linking $@\"
\t$(CC) -o $@ $^ && echo done # built
%.o: %.c
\t-$(CC) $(CFLAGS) -c $< -o $@
define HELP =
Usage: make $(TARGET)
endef
ifeq ($(OS),Windows_NT)
  EXE := .exe
else
  EXE :=
endif
clean:
\trm -f $(OBJ) \\
\t  app
include deps.mk
export PATH
debug: CFLAGS += -g";

#[test]
fn a_gnu_makefile_is_coloured_line_by_line() {
    let lines = lex(MAKEFILE);
    let expected: Vec<Vec<(&str, TokenClass)>> = vec![
        vec![("CC", Key), ("?=", Operator)],
        vec![("CFLAGS", Key), (":=", Operator), ("$(EXTRA)", Key)],
        vec![
            ("SRC", Key),
            ("=", Operator),
            ("$(", P),
            ("wildcard", Function),
            (")", P),
        ],
        vec![
            ("OBJ", Key),
            ("=", Operator),
            ("$(", P),
            ("patsubst", Function),
            (",", P),
            (",", P),
            ("$(SRC)", Key),
            (")", P),
        ],
        vec![(".PHONY", Keyword), (":", Operator)],
        vec![("all", Function), (":", Operator)],
        vec![
            ("app", Function),
            (":", Operator),
            ("$(OBJ)", Key),
            ("|", Operator),
        ],
        vec![
            ("@", Operator),
            ("echo", Function),
            ("\"Linking ", STR),
            ("$@", Key),
            ("\"", STR),
        ],
        vec![
            ("$(CC)", Key),
            ("$@", Key),
            ("$^", Key),
            ("&&", Operator),
            ("echo", Function),
            ("# built", Comment),
        ],
        vec![("%.o", Function), (":", Operator)],
        vec![
            ("-", Operator),
            ("$(CC)", Key),
            ("$(CFLAGS)", Key),
            ("$<", Key),
            ("$@", Key),
        ],
        vec![("define", Keyword), ("HELP", Key), ("=", Operator)],
        vec![("$(TARGET)", Key)],
        vec![("endef", Keyword)],
        vec![
            ("ifeq", Keyword),
            ("(", P),
            ("$(OS)", Key),
            (",", P),
            (")", P),
        ],
        vec![("EXE", Key), (":=", Operator)],
        vec![("else", Keyword)],
        vec![("EXE", Key), (":=", Operator)],
        vec![("endif", Keyword)],
        vec![("clean", Function), (":", Operator)],
        vec![("$(OBJ)", Key)],
        vec![],
        vec![("include", Keyword)],
        vec![("export", Keyword), ("PATH", Key)],
        vec![
            ("debug", Function),
            (":", Operator),
            ("CFLAGS", Key),
            ("+=", Operator),
        ],
    ];
    assert_eq!(lines.len(), expected.len());
    for (index, (got, wanted)) in lines.iter().zip(&expected).enumerate() {
        assert_eq!(got, wanted, "line {index}");
    }
}

#[test]
fn a_recipe_gets_the_shell_colours_and_dollars_for_the_shell() {
    assert_eq!(
        lex("\tfor f in $$(ls); do echo $$f; done")[0],
        vec![
            ("for", Keyword),
            ("$$", Key),
            ("(", P),
            (")", P),
            (";", Operator),
            ("do", Keyword),
            ("echo", Function),
            ("$$f", Key),
            (";", Operator),
            ("done", Keyword),
        ]
    );
    // References inside single quotes too: make expands them first.
    assert_eq!(
        lex("\t+cd '$(DIR)' || exit 1")[0],
        vec![
            ("+", Operator),
            ("cd", Function),
            ("'", STR),
            ("$(DIR)", Key),
            ("'", STR),
            ("||", Operator),
            ("exit", Function),
        ]
    );
    // A recipe after `;`, and a `#` inside a word is no comment.
    assert_eq!(
        lex("x: y ; @echo a#b")[0],
        vec![
            ("x", Function),
            (":", Operator),
            (";", P),
            ("@", Operator),
            ("echo", Function),
        ]
    );
}

#[test]
fn a_backslash_continues_the_line_as_what_it_was() {
    let lines =
        lex("SRCS = a.c \\\n  $(B) # c\n# one \\\nstill\nall: a \\\n  b | c\n\tcc \\\n\t  -o x");
    assert_eq!(lines[0], vec![("SRCS", Key), ("=", Operator)]);
    assert_eq!(lines[1], vec![("$(B)", Key), ("# c", Comment)]);
    assert_eq!(lines[2], vec![("# one \\", Comment)]);
    assert_eq!(lines[3], vec![("still", Comment)]);
    assert_eq!(lines[5], vec![("|", Operator)]);
    assert!(lines[7].is_empty());
    // An escaped backslash ends the line.
    let lines = lex("A = x \\\\\nb: c");
    assert_eq!(lines[1], vec![("b", Function), (":", Operator)]);
}

#[test]
fn defines_nest_and_their_body_is_text() {
    let lines =
        lex("override define TWO\ndefine INNER\nendef\n\tx: $(Y) # no comment\nendef\ny = 1");
    assert_eq!(
        lines[0],
        vec![("override", Keyword), ("define", Keyword), ("TWO", Key)]
    );
    assert_eq!(lines[1], vec![("define", Keyword), ("INNER", Key)]);
    assert_eq!(lines[2], vec![("endef", Keyword)]);
    assert_eq!(lines[3], vec![("$(Y)", Key)]);
    assert_eq!(lines[4], vec![("endef", Keyword)]);
    assert_eq!(lines[5], vec![("y", Key), ("=", Operator)]);
}

#[test]
fn rules_of_every_shape() {
    assert_eq!(
        lex("$(OBJS): %.o: %.c")[0],
        vec![("$(OBJS)", Key), (":", Operator), (":", Operator)]
    );
    assert_eq!(lex("a:: b")[0], vec![("a", Function), ("::", Operator)]);
    assert_eq!(
        lex("X ::= $(Y:.c=.o)")[0],
        vec![("X", Key), ("::=", Operator), ("$(Y:.c=.o)", Key)]
    );
    assert_eq!(lex("V != date")[0], vec![("V", Key), ("!=", Operator)]);
    assert_eq!(
        lex("else ifneq \"$(A)\" \"\"")[0],
        vec![
            ("else", Keyword),
            ("ifneq", Keyword),
            ("\"$(A)\"", STR),
            ("\"\"", STR)
        ]
    );
    assert_eq!(
        lex("ifdef DEBUG")[0],
        vec![("ifdef", Keyword), ("DEBUG", Key)]
    );
}

#[test]
fn function_calls_nest_with_their_own_brackets() {
    assert_eq!(
        lex("X = $(subst a,b,$(if $(x),(y),z)) ${Y}")[0],
        vec![
            ("X", Key),
            ("=", Operator),
            ("$(", P),
            ("subst", Function),
            (",", P),
            (",", P),
            ("$(", P),
            ("if", Function),
            ("$(x)", Key),
            (",", P),
            (",", P),
            (")", P),
            (")", P),
            ("${Y}", Key),
        ]
    );
}

#[test]
fn calls_nested_beyond_the_limit_stay_linear() {
    let depth = 100_000;
    let line = format!("X = {}{}", "$(if a,".repeat(depth), ")".repeat(depth));
    let mut tokens: Vec<Token> = Vec::new();
    lex_line(line.as_bytes(), LineState::default(), &mut tokens);
    // Past the nesting limit the rest of the line is one span: few tokens, never more than a
    // line may hold.
    assert!(!tokens.is_empty() && tokens.len() <= MAX_TOKENS_PER_LINE);
    lex_line(line.as_bytes(), LineState::default(), &mut ());
}

#[test]
fn every_token_sits_on_character_boundaries() {
    let line = "\u{fc}ber: $(\u{e4}) $\u{e9} ; echo \"Gr\u{fc}\u{df}e\" # \u{4e00}";
    let mut tokens: Vec<Token> = Vec::new();
    lex_line(line.as_bytes(), LineState::default(), &mut tokens);
    for token in &tokens {
        let start = token.start as usize;
        assert!(line.get(start..start + token.len as usize).is_some());
    }
    assert!(tokens.len() >= 6);
}
