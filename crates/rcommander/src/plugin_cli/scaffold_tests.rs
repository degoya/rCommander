//! `plugin new`: the embedded template is the folder, a scaffold is complete and its own, and
//! what it writes packages, verifies and passes conformance unchanged (with the test bed's
//! component standing in for the one `cargo build` makes, which needs a WebAssembly toolchain).

use std::path::{Path, PathBuf};

use super::{
    IGNORED, PluginKind, SECRETS, SIGNING_KEY_FILE, VIEWER, scaffold, slug, write_gitignore,
};

fn template_dir(kind: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../sdk/templates/{kind}"))
}

/// A template's files as `scaffold.rs` embeds them.
type Files = &'static [(&'static str, &'static str)];

/// Every template folder with its embedded files.
const TEMPLATES: [(&str, Files); 2] = [("viewer", VIEWER), ("secrets", SECRETS)];

fn files_under(root: &Path, directory: &Path, found: &mut Vec<String>) {
    for entry in std::fs::read_dir(directory).expect("read the template folder") {
        let path = entry.expect("an entry").path();
        if path.is_dir() {
            files_under(root, &path, found);
        } else {
            let relative = path.strip_prefix(root).expect("under the root");
            found.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }
}

#[test]
fn every_embedded_template_is_its_folder_file_for_file() {
    for (name, files) in TEMPLATES {
        let root = template_dir(name);
        let mut on_disk = Vec::new();
        files_under(&root, &root, &mut on_disk);
        on_disk.sort();
        let embedded: Vec<&str> = files.iter().map(|(path, _)| *path).collect();
        assert_eq!(
            on_disk, embedded,
            "list every file of sdk/templates/{name} in its constant, sorted"
        );
        for (path, text) in files {
            let disk = std::fs::read_to_string(root.join(path)).expect("a template file");
            assert_eq!(&disk, text, "{name}/{path}");
        }
    }
}

#[test]
fn a_scaffold_is_complete_and_its_own() {
    let temp = tempfile::tempdir().expect("temp dir");
    let out = temp.path().join("my-viewer");
    let made = scaffold(PluginKind::Viewer, &out, None).expect("scaffold");
    assert_eq!(made.name, "my-viewer");
    assert_eq!(made.slug, "my_viewer");
    assert_eq!(made.key_path, out.join(SIGNING_KEY_FILE));
    for (path, _) in VIEWER {
        let text = std::fs::read_to_string(out.join(path)).expect("a scaffolded file");
        assert!(!text.contains("{{"), "{path} keeps a placeholder");
        assert!(
            !text.contains("REPLACE_WITH_YOUR"),
            "{path} keeps a placeholder"
        );
    }
    let manifest = std::fs::read_to_string(out.join("manifest.toml")).expect("manifest");
    assert!(manifest.contains(&format!("id = \"{}\"", made.id)));
    assert!(manifest.contains("key_id = \"my_viewer-release-v1\""));
    let cargo = std::fs::read_to_string(out.join("Cargo.toml")).expect("Cargo.toml");
    assert!(cargo.contains("name = \"my_viewer\""));

    let ignore = std::fs::read_to_string(out.join(".gitignore")).expect(".gitignore");
    for entry in IGNORED {
        assert!(ignore.lines().any(|line| line == entry), "{entry}");
    }
    let pem = std::fs::read_to_string(&made.key_path).expect("the key");
    assert!(pem.starts_with("-----BEGIN PRIVATE KEY-----"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(&made.key_path)
            .expect("meta")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    let other = scaffold(
        PluginKind::Viewer,
        &temp.path().join("second"),
        Some("Second viewer"),
    )
    .expect("a second scaffold");
    assert_ne!(other.id, made.id, "every scaffold has its own id");
    assert_eq!(other.slug, "second_viewer");
    assert_ne!(
        std::fs::read_to_string(&other.key_path).expect("key"),
        pem,
        "and its own key"
    );

    let refused = scaffold(PluginKind::Viewer, &out, None).expect_err("the folder exists");
    assert!(refused.to_string().contains("already exists"), "{refused}");
}

#[test]
fn names_that_break_a_manifest_or_a_crate_are_refused() {
    assert_eq!(slug("CSV Table 2").expect("slug"), "csv_table_2");
    assert!(slug("2fast").is_err());
    assert!(slug("x").is_err());
    let temp = tempfile::tempdir().expect("temp dir");
    for name in ["say \"hi\"", "back\\slash", "line\nbreak", ""] {
        assert!(
            scaffold(PluginKind::Viewer, &temp.path().join("n"), Some(name)).is_err(),
            "{name:?}"
        );
    }
}

#[test]
fn a_gitignore_keeps_its_lines_and_gains_the_key() {
    let temp = tempfile::tempdir().expect("temp dir");
    std::fs::write(temp.path().join(".gitignore"), "notes.txt\ntarget/").expect("write");
    write_gitignore(temp.path()).expect("gitignore");
    let text = std::fs::read_to_string(temp.path().join(".gitignore")).expect("read");
    assert_eq!(text, "notes.txt\ntarget/\nplugin-signing.key\n*.rcplug\n");
}

/// The acceptance's *SDK* line in the parts that run without a WebAssembly toolchain: the
/// scaffold's manifest, locales and key package as they are, the package verifies, and
/// conformance passes.
#[test]
fn a_scaffold_packages_verifies_and_passes_conformance_unchanged() {
    passes_conformance(
        PluginKind::Viewer,
        "table_view",
        &rc_plugin_host::test_support::current_viewer_component(),
    );
    passes_conformance(
        PluginKind::Secrets,
        "bridge_source",
        &rc_plugin_host::test_support::current_secrets_component(),
    );
}

fn passes_conformance(kind: PluginKind, name: &str, component: &[u8]) {
    let temp = tempfile::tempdir().expect("temp dir");
    let out = temp.path().join(name);
    scaffold(kind, &out, None).expect("scaffold");
    let source = std::fs::read_to_string(out.join("manifest.toml")).expect("manifest");
    let pem = std::fs::read_to_string(out.join(SIGNING_KEY_FILE)).expect("key");
    let key = rc_plugin_host::load_signing_key_pem(&pem).expect("the key reads");
    let public = rc_plugin_host::public_key_base64(&key);
    let manifest = rc_plugin_host::complete_manifest(&source, None, Some(&public))
        .expect("the manifest completes");
    assert_eq!(manifest, source, "a scaffold needs no completion");
    let english = std::fs::read(out.join("locales/en.json")).expect("en.json");
    let locales = vec![("en".to_owned(), english)];

    let package =
        rc_plugin_host::package_plugin(manifest.as_bytes(), component, &locales, Some(&key))
            .expect("the scaffold packages");
    let parsed = rc_plugin_host::parse_manifest(manifest.as_bytes()).expect("manifest");
    let verified = rc_plugin_host::verify_package(
        &package,
        &[(parsed.key_id.clone(), parsed.public_key.clone())],
        false,
    )
    .expect("the package verifies");
    assert_eq!(verified.manifest.name, name);

    let verifier = rc_plugin_host::PluginVerifier::new(false);
    verifier
        .trust_key_base64(parsed.key_id, &parsed.public_key)
        .expect("trust");
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let report = runtime.block_on(rc_plugin_host::check_package(&package, &verifier));
    assert!(report.passed, "{report:?}");
}

#[test]
fn a_secrets_scaffold_keeps_no_placeholder() {
    let temp = tempfile::tempdir().expect("temp dir");
    let out = temp.path().join("bridge-source");
    scaffold(PluginKind::Secrets, &out, None).expect("scaffold");
    for (path, _) in SECRETS {
        let text = std::fs::read_to_string(out.join(path)).expect("a scaffolded file");
        assert!(!text.contains("{{"), "{path} keeps a placeholder");
        assert!(
            !text.contains("REPLACE_WITH_YOUR"),
            "{path} keeps a placeholder"
        );
    }
    let manifest = std::fs::read_to_string(out.join("manifest.toml")).expect("manifest");
    assert!(manifest.contains("plugin_type = \"secrets\""));
}
