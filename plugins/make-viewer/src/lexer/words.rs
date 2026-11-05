//! The words the Makefile lexer knows: GNU make's directives, functions and special targets, and
//! the shell's keywords and builtins for the recipes - the shell viewer's colours, without its
//! grammar.

/// The directives that start a line.
#[rustfmt::skip]
const DIRECTIVES: &[&[u8]] = &[
    b"define", b"else", b"endef", b"endif", b"export", b"ifdef", b"ifeq", b"ifndef", b"ifneq",
    b"include", b"-include", b"sinclude", b"load", b"-load", b"override", b"private",
    b"undefine", b"unexport", b"vpath",
];

/// The directives that may stand before an assignment or another directive.
#[rustfmt::skip]
const PREFIXES: &[&[u8]] = &[b"export", b"override", b"private", b"unexport"];

/// GNU make's functions, `$(name args)`.
#[rustfmt::skip]
const FUNCTIONS: &[&[u8]] = &[
    b"abspath", b"addprefix", b"addsuffix", b"and", b"basename", b"call", b"dir", b"error",
    b"eval", b"file", b"filter", b"filter-out", b"findstring", b"firstword", b"flavor",
    b"foreach", b"guile", b"if", b"info", b"intcmp", b"join", b"lastword", b"let", b"notdir",
    b"or", b"origin", b"patsubst", b"realpath", b"shell", b"sort", b"strip", b"subst", b"suffix",
    b"value", b"warning", b"wildcard", b"word", b"wordlist", b"words",
];

/// The targets with a meaning of their own.
#[rustfmt::skip]
const SPECIAL_TARGETS: &[&[u8]] = &[
    b".DEFAULT", b".DELETE_ON_ERROR", b".EXPORT_ALL_VARIABLES", b".IGNORE", b".INTERMEDIATE",
    b".LOW_RESOLUTION_TIME", b".NOTINTERMEDIATE", b".NOTPARALLEL", b".ONESHELL", b".PHONY",
    b".POSIX", b".PRECIOUS", b".SECONDARY", b".SECONDEXPANSION", b".SILENT", b".SUFFIXES",
    b".WAIT",
];

/// The shell's keywords where a command starts.
#[rustfmt::skip]
const SHELL_KEYWORDS: &[&[u8]] = &[
    b"!", b"[[", b"]]", b"case", b"do", b"done", b"elif", b"else", b"esac", b"fi", b"for",
    b"function", b"if", b"in", b"select", b"then", b"time", b"until", b"while",
];

/// The shell's builtins where a command starts.
#[rustfmt::skip]
const SHELL_BUILTINS: &[&[u8]] = &[
    b".", b":", b"[", b"alias", b"cd", b"command", b"declare", b"echo", b"eval", b"exec",
    b"exit", b"export", b"false", b"local", b"printf", b"pwd", b"read", b"readonly", b"return",
    b"set", b"shift", b"source", b"test", b"trap", b"true", b"type", b"umask", b"unset", b"wait",
];

pub(crate) fn is_directive(word: &[u8]) -> bool {
    DIRECTIVES.contains(&word)
}

pub(crate) fn is_prefix(word: &[u8]) -> bool {
    PREFIXES.contains(&word)
}

pub(crate) fn is_function(word: &[u8]) -> bool {
    FUNCTIONS.contains(&word)
}

pub(crate) fn is_special_target(word: &[u8]) -> bool {
    SPECIAL_TARGETS.contains(&word)
}

pub(crate) fn is_shell_keyword(word: &[u8]) -> bool {
    SHELL_KEYWORDS.contains(&word)
}

pub(crate) fn is_shell_builtin(word: &[u8]) -> bool {
    SHELL_BUILTINS.contains(&word)
}
