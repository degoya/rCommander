use std::path::{Path, PathBuf};

use rc_engine::{
    VfsPath,
    viewer::{Detected, TextEncoding},
};

use super::{ColoredLines, Format, MAX_BYTES, format_of, line_starts, read};
use crate::app::{
    harness::Harness,
    viewer::{
        document::{
            Token,
            TokenClass::{self, Comment, Key, Keyword, Punctuation, TypeName},
        },
        load,
    },
};

#[test]
fn the_name_gives_the_viewer_whose_colours_a_credential_file_gets() {
    for (name, format) in [
        (".env", Format::Config),
        (".ENV", Format::Config),
        (".env.local", Format::Config),
        (".env.Production", Format::Config),
        (".npmrc", Format::Config),
        ("winscp.ini", Format::Config),
        ("wcx_ftp.ini", Format::Config),
        ("rclone.conf", Format::Config),
        ("credentials.toml", Format::Config),
        ("credentials.tfrc.json", Format::Json),
        ("logins.json", Format::Json),
        ("config.json", Format::Json),
        (".env.json", Format::Json),
        ("hosts.yml", Format::Yaml),
        ("hosts.YAML", Format::Yaml),
        ("sitemanager.xml", Format::Xml),
        ("recentservers.xml", Format::Xml),
    ] {
        assert_eq!(format_of(name), Some(format), "{name}");
    }
    for name in [
        "id_ed25519",
        "key.pem",
        "store.p12",
        ".git-credentials",
        ".netrc",
        ".bash_history",
        "key4.db",
        "credentials",
        ".envrc",
        "",
    ] {
        assert_eq!(format_of(name), None, "{name}");
    }
}

#[test]
fn the_tokens_are_kept_by_line_start_past_a_mark_and_a_crlf() {
    let bytes = b"\xEF\xBB\xBFA=1\r\n# c\r\n\r\nB=x\n";
    let starts = line_starts(bytes, 3);
    assert_eq!(starts, [3, 8, 13, 15]);
    let token = |class| Token {
        start: 0,
        len: 1,
        class,
    };
    let lines = ColoredLines::new(
        &starts,
        vec![
            vec![token(Key)],
            vec![token(Comment)],
            Vec::new(),
            vec![token(Key)],
            vec![token(Key)],
        ],
    );
    assert_eq!(lines.line(3), [token(Key)]);
    assert_eq!(lines.line(8), [token(Comment)]);
    assert!(lines.line(13).is_empty(), "a blank line");
    assert_eq!(lines.line(15), [token(Key)]);
    assert!(lines.line(0).is_empty(), "no line starts inside the mark");
    assert!(
        lines.line(19).is_empty(),
        "a list past the last line is dropped"
    );
}

/// `content` written at `relative` under `root` (its folders made), opened as the Lister opens
/// it, and coloured: each line's tokens as their text and class.
async fn coloured(
    harness: &Harness,
    root: &Path,
    relative: &str,
    content: &str,
) -> Option<Vec<Vec<(String, TokenClass)>>> {
    let path: PathBuf = root.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("mkdir");
    }
    std::fs::write(&path, content).expect("write");
    let opened = load::open(
        harness.app.engine().clone(),
        VfsPath::local(path.clone()),
        None,
    )
    .await
    .expect("opens");
    let lines = read(&path, &opened).await?;
    Some(
        line_starts(content.as_bytes(), 0)
            .into_iter()
            .map(|start| {
                let start_at = usize::try_from(start).expect("small");
                let line = content[start_at..].lines().next().unwrap_or_default();
                lines
                    .line(start)
                    .iter()
                    .map(|token| {
                        let from = token.start as usize;
                        (
                            line[from..from + token.len as usize].to_owned(),
                            token.class,
                        )
                    })
                    .collect()
            })
            .collect(),
    )
}

/// Whether `line` has a token `text` of `class`.
fn has(line: &[(String, TokenClass)], text: &str, class: TokenClass) -> bool {
    line.iter().any(|(of, at)| of == text && *at == class)
}

/// One file per credential format the read guard keeps from the plugins, each coloured by the
/// bundled viewer of its format, linked in process.
#[tokio::test]
async fn every_credential_format_is_coloured_by_its_bundled_viewer() {
    let root = tempfile::tempdir().expect("temp dir");
    let harness = Harness::headless(root.path()).await;
    let at = root.path();

    let env = coloured(&harness, at, ".env", "# local\nexport TOKEN=abc\n")
        .await
        .expect(".env");
    assert!(has(&env[0], "# local", Comment), "{env:?}");
    assert!(has(&env[1], "export", Keyword), "{env:?}");
    assert!(has(&env[1], "TOKEN", Key), "{env:?}");

    let local = coloured(&harness, at, ".env.local", "A=1\n").await;
    assert!(has(&local.expect(".env.local")[0], "A", Key));

    let npmrc = coloured(&harness, at, ".npmrc", "registry=https://r/\n; note\n")
        .await
        .expect(".npmrc");
    assert!(has(&npmrc[0], "registry", Key), "{npmrc:?}");
    assert!(has(&npmrc[1], "; note", Comment), "{npmrc:?}");

    for name in ["winscp.ini", "rclone.conf"] {
        let ini = coloured(&harness, at, name, "[remote]\ntype = sftp\n")
            .await
            .expect(name);
        assert!(has(&ini[0], "[remote]", Keyword), "{name}: {ini:?}");
        assert!(has(&ini[1], "type", Key), "{name}: {ini:?}");
    }

    let toml = coloured(
        &harness,
        at,
        ".cargo/credentials.toml",
        "[registry]\ntoken = \"abc\"\n",
    )
    .await
    .expect("credentials.toml");
    assert!(has(&toml[0], "[registry]", Keyword), "{toml:?}");
    assert!(has(&toml[1], "token", Key), "{toml:?}");

    for name in [
        "logins.json",
        ".docker/config.json",
        "credentials.tfrc.json",
    ] {
        let json = coloured(&harness, at, name, "{\n  \"auths\": {}\n}\n")
            .await
            .expect(name);
        assert!(has(&json[1], "\"auths\"", Key), "{name}: {json:?}");
    }

    let yaml = coloured(
        &harness,
        at,
        "gh/hosts.yml",
        "github.com:\n  oauth_token: abc\n",
    )
    .await
    .expect("hosts.yml");
    assert!(has(&yaml[1], "oauth_token", Key), "{yaml:?}");

    for name in ["filezilla/sitemanager.xml", "filezilla/recentservers.xml"] {
        let xml = coloured(
            &harness,
            at,
            name,
            "<FileZilla3>\n<Host>h</Host>\n</FileZilla3>\n",
        )
        .await
        .expect(name);
        assert!(has(&xml[1], "Host", TypeName), "{name}: {xml:?}");
        assert!(has(&xml[1], "<", Punctuation), "{name}: {xml:?}");
    }
}

/// A file the guard does not name - an ordinary JSON or INI file - is left to the plugins: the
/// built-in viewer adds no colours of its own.
#[tokio::test]
async fn a_file_the_guard_does_not_name_is_not_coloured_here() {
    let root = tempfile::tempdir().expect("temp dir");
    let harness = Harness::headless(root.path()).await;
    for name in ["data.json", "app.ini", "prod.env", "hosts.yml"] {
        assert!(
            coloured(&harness, root.path(), name, "a = 1\n")
                .await
                .is_none(),
            "{name}"
        );
    }
}

/// A file the Lister does not show as UTF-8 text, or one larger than the limit, opens without
/// colours.
#[tokio::test]
async fn only_utf8_text_up_to_the_limit_is_coloured() {
    let root = tempfile::tempdir().expect("temp dir");
    let harness = Harness::headless(root.path()).await;
    let engine = harness.app.engine().clone();
    let small = root.path().join(".env");
    std::fs::write(&small, b"A=1\n").expect("write");
    let mut opened = load::open(engine.clone(), VfsPath::local(small.clone()), None)
        .await
        .expect("opens");
    assert!(read(&small, &opened).await.is_some());
    opened.detected = Detected {
        encoding: TextEncoding::Windows1252,
        bom_len: 0,
    };
    assert!(read(&small, &opened).await.is_none(), "not UTF-8");

    let large = root.path().join(".env.local");
    let mut content = Vec::new();
    while (content.len() as u64) <= MAX_BYTES {
        content.extend_from_slice(b"KEY=value\n");
    }
    std::fs::write(&large, &content).expect("write");
    let opened = load::open(engine, VfsPath::local(large.clone()), None)
        .await
        .expect("opens");
    assert!(read(&large, &opened).await.is_none(), "too large");
}
