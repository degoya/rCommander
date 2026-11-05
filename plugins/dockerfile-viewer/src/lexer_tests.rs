//! The lexer on Dockerfiles: instructions, the exec and the shell form, pairs, flags,
//! directives, continuation lines, heredocs and a multi-stage build, each line as the classes
//! and texts of its tokens.

use crate::test_support::{highlighted, line};
use crate::types::TokenClass::String as Str;
use crate::types::TokenClass::{
    self, Comment, Function, Invalid, Key, Keyword, Number, Operator, Punctuation, TypeName,
};

fn t(class: TokenClass, text: &str) -> (TokenClass, String) {
    (class, text.to_owned())
}

#[test]
fn from_names_the_image_the_stage_and_the_platform() {
    assert_eq!(
        line("FROM alpine:3.20 AS build"),
        [
            t(Keyword, "FROM"),
            t(TypeName, "alpine:3.20"),
            t(Keyword, "AS"),
            t(Function, "build")
        ]
    );
    assert_eq!(
        line("from --platform=$BUILDPLATFORM golang:1.22 as builder"),
        [
            t(Keyword, "from"),
            t(Key, "--platform"),
            t(Operator, "="),
            t(Key, "$BUILDPLATFORM"),
            t(TypeName, "golang:1.22"),
            t(Keyword, "as"),
            t(Function, "builder")
        ]
    );
}

#[test]
fn the_shell_form_has_the_shell_viewers_colours() {
    assert_eq!(
        line("RUN cd /src && export GOOS=linux && echo \"$HOME/x\" > out.txt 2>&1"),
        [
            t(Keyword, "RUN"),
            t(Function, "cd"),
            t(Operator, "&&"),
            t(Function, "export"),
            t(Operator, "&&"),
            t(Function, "echo"),
            t(Str, "\""),
            t(Key, "$HOME"),
            t(Str, "/x\""),
            t(Operator, ">"),
            t(Operator, "2"),
            t(Operator, ">&"),
            t(Number, "1")
        ]
    );
    assert_eq!(
        line("RUN apt-get update && apt-get install -y curl # tools"),
        [t(Keyword, "RUN"), t(Operator, "&&"), t(Comment, "# tools")]
    );
}

#[test]
fn the_exec_form_is_a_json_array() {
    assert_eq!(
        line("CMD [\"nginx\", \"-g\", \"daemon off;\"]"),
        [
            t(Keyword, "CMD"),
            t(Punctuation, "["),
            t(Str, "\"nginx\""),
            t(Punctuation, ","),
            t(Str, "\"-g\""),
            t(Punctuation, ","),
            t(Str, "\"daemon off;\""),
            t(Punctuation, "]")
        ]
    );
}

#[test]
fn env_arg_and_label_are_keys_and_values() {
    assert_eq!(
        line("ENV PATH=/usr/local/bin:$PATH LANG=\"C.UTF-8\""),
        [
            t(Keyword, "ENV"),
            t(Key, "PATH"),
            t(Operator, "="),
            t(Str, "/usr/local/bin:"),
            t(Key, "$PATH"),
            t(Key, "LANG"),
            t(Operator, "="),
            t(Str, "\"C.UTF-8\"")
        ]
    );
    assert_eq!(
        line("ENV APP_HOME /srv/app"),
        [t(Keyword, "ENV"), t(Key, "APP_HOME"), t(Str, "/srv/app")]
    );
    assert_eq!(line("ARG VERSION"), [t(Keyword, "ARG"), t(Key, "VERSION")]);
    assert_eq!(
        line("ARG VERSION=1.0"),
        [
            t(Keyword, "ARG"),
            t(Key, "VERSION"),
            t(Operator, "="),
            t(Str, "1.0")
        ]
    );
}

#[test]
fn flags_ports_and_the_rest() {
    assert_eq!(
        line("COPY --from=build --chown=app:app /out/bin /usr/local/bin/"),
        [
            t(Keyword, "COPY"),
            t(Key, "--from"),
            t(Operator, "="),
            t(Str, "build"),
            t(Key, "--chown"),
            t(Operator, "="),
            t(Str, "app:app")
        ]
    );
    assert_eq!(
        line("EXPOSE 8080/tcp 443"),
        [t(Keyword, "EXPOSE"), t(Number, "8080"), t(Number, "443")]
    );
    assert_eq!(
        line("ONBUILD RUN echo hi"),
        [
            t(Keyword, "ONBUILD"),
            t(Keyword, "RUN"),
            t(Function, "echo")
        ]
    );
    assert_eq!(
        line("HEALTHCHECK --interval=30s CMD curl -f http://localhost/ || exit 1"),
        [
            t(Keyword, "HEALTHCHECK"),
            t(Key, "--interval"),
            t(Operator, "="),
            t(Str, "30s"),
            t(Keyword, "CMD"),
            t(Operator, "||"),
            t(Function, "exit"),
            t(Number, "1")
        ]
    );
    assert_eq!(line("FORM alpine"), [t(Invalid, "FORM")]);
}

#[test]
fn directives_come_first_and_the_escape_directive_changes_the_escape() {
    let lines = highlighted(
        b"# syntax=docker/dockerfile:1\n# escape=`\nFROM servercore\nRUN dir C:\\ `\n    && echo done\n# syntax=late\n",
    );
    assert_eq!(
        lines[0],
        [
            t(Comment, "#"),
            t(Key, "syntax"),
            t(Operator, "="),
            t(Str, "docker/dockerfile:1")
        ]
    );
    assert_eq!(
        lines[1],
        [
            t(Comment, "#"),
            t(Key, "escape"),
            t(Operator, "="),
            t(Str, "`")
        ]
    );
    assert_eq!(lines[3], [t(Keyword, "RUN"), t(Punctuation, "`")]);
    assert_eq!(lines[4], [t(Operator, "&&"), t(Function, "echo")]);
    assert_eq!(
        lines[5],
        [t(Comment, "# syntax=late")],
        "after an instruction"
    );
}

#[test]
fn continuation_lines_go_on_with_their_instruction_comments_between() {
    let lines = highlighted(
        b"RUN apt-get update \\\n    && apt-get install -y \\\n       curl \\\n    # a comment inside\n    && rm -rf /var/lib/apt/lists/*\nCMD [\"x\"]\n",
    );
    assert_eq!(lines[0], [t(Keyword, "RUN"), t(Punctuation, "\\")]);
    assert_eq!(lines[1], [t(Operator, "&&"), t(Punctuation, "\\")]);
    assert_eq!(lines[2], [t(Punctuation, "\\")]);
    assert_eq!(lines[3], [t(Comment, "# a comment inside")]);
    assert_eq!(lines[4], [t(Operator, "&&")]);
    assert_eq!(
        lines[5],
        [
            t(Keyword, "CMD"),
            t(Punctuation, "["),
            t(Str, "\"x\""),
            t(Punctuation, "]")
        ],
        "the instruction ended"
    );
}

#[test]
fn a_quote_goes_on_into_the_continued_line() {
    let lines = highlighted(
        b"LABEL org.opencontainers.image.title=\"App\" \\\n      description=\"multi \\\nline\"\nUSER app\n",
    );
    assert_eq!(
        lines[0],
        [
            t(Keyword, "LABEL"),
            t(Key, "org.opencontainers.image.title"),
            t(Operator, "="),
            t(Str, "\"App\""),
            t(Punctuation, "\\")
        ]
    );
    assert_eq!(
        lines[1],
        [
            t(Key, "description"),
            t(Operator, "="),
            t(Str, "\"multi "),
            t(Punctuation, "\\")
        ]
    );
    assert_eq!(lines[2], [t(Str, "line\"")]);
    assert_eq!(lines[3], [t(Keyword, "USER")]);
}

#[test]
fn heredocs_a_shell_script_and_a_file() {
    let lines = highlighted(
        b"RUN <<EOF\nset -e\nif [ -f /x ]; then echo \"yes\"; fi\nEOF\nCOPY <<-CONF /etc/app.conf\n\tkey = value\n\tCONF\nWORKDIR /srv\n",
    );
    assert_eq!(
        lines[0],
        [t(Keyword, "RUN"), t(Operator, "<<"), t(Keyword, "EOF")]
    );
    assert_eq!(lines[1], [t(Function, "set")]);
    assert_eq!(
        lines[2],
        [
            t(Keyword, "if"),
            t(Function, "["),
            t(Operator, ";"),
            t(Keyword, "then"),
            t(Function, "echo"),
            t(Str, "\"yes\""),
            t(Operator, ";"),
            t(Keyword, "fi")
        ]
    );
    assert_eq!(lines[3], [t(Keyword, "EOF")]);
    assert_eq!(
        lines[4],
        [t(Keyword, "COPY"), t(Operator, "<<-"), t(Keyword, "CONF")]
    );
    assert_eq!(lines[5], [t(Str, "key = value")]);
    assert_eq!(lines[6], [t(Keyword, "CONF")]);
    assert_eq!(lines[7], [t(Keyword, "WORKDIR")], "the heredoc ended");
}

#[test]
fn a_heredoc_after_a_command_is_its_input_not_a_script() {
    let lines = highlighted(b"RUN python3 <<'PY'\nimport os\nPY\n");
    assert_eq!(
        lines[0],
        [t(Keyword, "RUN"), t(Operator, "<<"), t(Keyword, "'PY'")]
    );
    assert_eq!(lines[1], [t(Str, "import os")]);
    assert_eq!(lines[2], [t(Keyword, "PY")]);
}

#[test]
fn a_multi_stage_build() {
    let lines = highlighted(
        b"FROM golang:1.22 AS build\nWORKDIR /src\nCOPY . .\nRUN go build -o /out/app ./cmd/app\n\nFROM gcr.io/distroless/static AS final\nCOPY --from=build /out/app /app\nUSER 65532:65532\nENTRYPOINT [\"/app\"]\n",
    );
    assert_eq!(lines.len(), 9);
    assert_eq!(lines[1], [t(Keyword, "WORKDIR")]);
    assert_eq!(lines[3], [t(Keyword, "RUN")]);
    assert!(lines[4].is_empty());
    assert_eq!(
        lines[5],
        [
            t(Keyword, "FROM"),
            t(TypeName, "gcr.io/distroless/static"),
            t(Keyword, "AS"),
            t(Function, "final")
        ]
    );
    assert_eq!(
        lines[6],
        [
            t(Keyword, "COPY"),
            t(Key, "--from"),
            t(Operator, "="),
            t(Str, "build")
        ]
    );
    assert_eq!(lines[7], [t(Keyword, "USER"), t(Number, "65532")]);
    assert_eq!(
        lines[8],
        [
            t(Keyword, "ENTRYPOINT"),
            t(Punctuation, "["),
            t(Str, "\"/app\""),
            t(Punctuation, "]")
        ]
    );
}
