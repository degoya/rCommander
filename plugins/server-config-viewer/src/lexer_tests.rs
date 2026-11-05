use super::*;
use crate::types::TokenClass::{
    Comment, Function, Key, Keyword, Literal, Number, Operator, Punctuation as P,
};

const STR: TokenClass = TokenClass::String;

/// The tokens of every line of `text` in `format`, each as its text and class, the state
/// carried on.
fn lex(text: &str, format: Format) -> Vec<Vec<(&str, TokenClass)>> {
    let mut state = LineState::default();
    text.split('\n')
        .map(|line| {
            let mut tokens: Vec<Token> = Vec::new();
            state = lex_line(line.as_bytes(), state, format, &mut tokens);
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

#[test]
fn nginx_blocks_directives_variables_and_statements_over_lines() {
    let lines = lex(
        "http {\n  server {\n    listen 80 default_server; # main\n    server_name example.com;\n    location ~* \\.(png)$ {\n      expires 30d;\n      proxy_set_header Host $host;\n    }\n    log_format main '$remote_addr '\n                    \"${request}\";\n    gzip on;\n  }\n}",
        Format::Nginx,
    );
    assert_eq!(lines[0], vec![("http", Keyword), ("{", P)]);
    assert_eq!(lines[1], vec![("server", Keyword), ("{", P)]);
    assert_eq!(
        lines[2],
        vec![
            ("listen", Key),
            ("80", Number),
            (";", P),
            ("# main", Comment)
        ]
    );
    assert_eq!(lines[3], vec![("server_name", Key), (";", P)]);
    assert_eq!(
        lines[4],
        vec![
            ("location", Keyword),
            ("~*", Operator),
            ("(", P),
            (")", P),
            ("{", P),
        ]
    );
    assert_eq!(lines[5], vec![("expires", Key), ("30d", Number), (";", P)]);
    assert_eq!(
        lines[6],
        vec![("proxy_set_header", Key), ("$host", Function), (";", P)]
    );
    assert_eq!(lines[7], vec![("}", P)]);
    assert_eq!(
        lines[8],
        vec![
            ("log_format", Key),
            ("'", STR),
            ("$remote_addr", Function),
            (" '", STR),
        ]
    );
    // The statement goes on: the line's first word is no directive.
    assert_eq!(
        lines[9],
        vec![("\"", STR), ("${request}", Function), ("\"", STR), (";", P)]
    );
    assert_eq!(lines[10], vec![("gzip", Key), ("on", Literal), (";", P)]);
}

#[test]
fn an_nginx_string_may_run_over_lines() {
    let lines = lex("return 200 'one\ntwo';\nlisten 81;", Format::Nginx);
    assert_eq!(
        lines[0],
        vec![("return", Key), ("200", Number), ("'one", STR)]
    );
    assert_eq!(lines[1], vec![("two'", STR), (";", P)]);
    assert_eq!(lines[2], vec![("listen", Key), ("81", Number), (";", P)]);
}

#[test]
fn apache_sections_directives_and_its_variables() {
    let lines = lex(
        "<VirtualHost *:443>\n    DocumentRoot \"/var/www/html\"\n    RewriteRule ^/(.*)$ https://%{HTTP_HOST}/$1 [R=301,L]\n    Require all granted\n# comment\n    Options -Indexes \\\n      +FollowSymLinks\n</VirtualHost>",
        Format::Apache,
    );
    assert_eq!(lines[0], vec![("<VirtualHost", Keyword), (">", Keyword)]);
    assert_eq!(
        lines[1],
        vec![("DocumentRoot", Key), ("\"/var/www/html\"", STR)]
    );
    assert_eq!(
        lines[2],
        vec![
            ("RewriteRule", Key),
            ("%{HTTP_HOST}", Function),
            ("$1", Function),
        ]
    );
    assert_eq!(lines[3], vec![("Require", Key), ("all", Literal)]);
    assert_eq!(lines[4], vec![("# comment", Comment)]);
    assert_eq!(lines[5], vec![("Options", Key)]);
    // Continued by the backslash: no directive on the next line.
    assert!(lines[6].is_empty());
    assert_eq!(lines[7], vec![("</VirtualHost", Keyword), (">", Keyword)]);
}

#[test]
fn an_htaccess_file_reads_as_apache() {
    let lines = lex(
        "RewriteEngine On\nRewriteCond %{REQUEST_FILENAME} !-f\nRewriteCond %1 x",
        Format::Apache,
    );
    assert_eq!(lines[0], vec![("RewriteEngine", Key), ("On", Literal)]);
    assert_eq!(
        lines[1],
        vec![("RewriteCond", Key), ("%{REQUEST_FILENAME}", Function)]
    );
    assert_eq!(lines[2], vec![("RewriteCond", Key), ("%1", Function)]);
}

#[test]
fn sshd_config_keywords_blocks_and_tokens() {
    let lines = lex(
        "# server\nPort 22\nPermitRootLogin no\nListenAddress 0.0.0.0\nMatch User alice Address 10.0.0.0/8\n    X11Forwarding=yes\nHost *.example.com\n    IdentityFile ~/.ssh/%h_key\n    ProxyCommand \"nc %h %p\"",
        Format::Ssh,
    );
    assert_eq!(lines[0], vec![("# server", Comment)]);
    assert_eq!(lines[1], vec![("Port", Key), ("22", Number)]);
    assert_eq!(lines[2], vec![("PermitRootLogin", Key), ("no", Literal)]);
    assert_eq!(lines[3], vec![("ListenAddress", Key), ("0.0.0.0", Number)]);
    assert_eq!(lines[4], vec![("Match", Keyword), ("10.0.0.0/8", Number)]);
    assert_eq!(
        lines[5],
        vec![("X11Forwarding", Key), ("=", P), ("yes", Literal)]
    );
    assert_eq!(lines[6], vec![("Host", Keyword)]);
    assert_eq!(lines[7], vec![("IdentityFile", Key), ("%h", Function)]);
    assert_eq!(
        lines[8],
        vec![
            ("ProxyCommand", Key),
            ("\"nc ", STR),
            ("%h", Function),
            (" ", STR),
            ("%p", Function),
            ("\"", STR),
        ]
    );
}

#[test]
fn a_crontab_has_five_time_fields_specials_and_variables() {
    let lines = lex(
        "# m h dom mon dow command\nSHELL=/bin/bash\nMAILTO = \"root\"\n*/5 0-23 * jan,feb mon-fri /usr/bin/backup --all\n@reboot /usr/local/bin/start\n17 *",
        Format::Crontab,
    );
    assert_eq!(lines[0], vec![("# m h dom mon dow command", Comment)]);
    assert_eq!(lines[1], vec![("SHELL", Key), ("=", P)]);
    assert_eq!(lines[2], vec![("MAILTO", Key), ("=", P), ("\"root\"", STR)]);
    assert_eq!(
        lines[3],
        vec![
            ("*/5", Number),
            ("0-23", Number),
            ("*", Number),
            ("jan,feb", Number),
            ("mon-fri", Number),
        ]
    );
    assert_eq!(lines[4], vec![("@reboot", Keyword)]);
    assert_eq!(lines[5], vec![("17", Number), ("*", Number)]);
}

#[test]
fn hosts_lines_are_an_address_and_names() {
    let lines = lex(
        "127.0.0.1 localhost # loopback\n::1\tip6-localhost ip6-loopback\n# all\n10.0.0.2 db#x",
        Format::Hosts,
    );
    assert_eq!(
        lines[0],
        vec![
            ("127.0.0.1", Number),
            ("localhost", Key),
            ("# loopback", Comment)
        ]
    );
    assert_eq!(
        lines[1],
        vec![
            ("::1", Number),
            ("ip6-localhost", Key),
            ("ip6-loopback", Key)
        ]
    );
    assert_eq!(lines[2], vec![("# all", Comment)]);
    assert_eq!(
        lines[3],
        vec![("10.0.0.2", Number), ("db", Key), ("#x", Comment)]
    );
}

#[test]
fn every_token_sits_on_character_boundaries() {
    let line = "add_header X-Gr\u{fc}\u{df}e \"\u{e4} $\u{e9}\" \u{4e00}; # \u{fc}";
    for format in [
        Format::Nginx,
        Format::Apache,
        Format::Ssh,
        Format::Crontab,
        Format::Hosts,
    ] {
        let mut tokens: Vec<Token> = Vec::new();
        lex_line(line.as_bytes(), LineState::default(), format, &mut tokens);
        for token in &tokens {
            let start = token.start as usize;
            assert!(
                line.get(start..start + token.len as usize).is_some(),
                "{format:?}"
            );
        }
        assert!(!tokens.is_empty(), "{format:?}");
    }
}
