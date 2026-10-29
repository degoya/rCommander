//! `rcommander plugin ...` end to end on files: the arguments, `new` -> `package` -> `verify` ->
//! `conformance` with the test bed's component, the shared manifest fields, `keygen`, and the
//! refusals an author meets.

use clap::Parser;

use super::{
    ConformanceArgs, KeygenArgs, PackageArgs, PluginCommand, VerifyArgs, conformance,
    describe_verify_error, group_fingerprint, keygen, package, read_locales, verify,
};

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: PluginCommand,
}

fn parse(arguments: &[&str]) -> PluginCommand {
    Cli::try_parse_from(std::iter::once("plugin").chain(arguments.iter().copied()))
        .unwrap_or_else(|error| panic!("{arguments:?}: {error}"))
        .command
}

#[test]
fn the_subcommands_take_rdownloaders_arguments() {
    assert!(matches!(
        parse(&["new", "--type", "viewer", "--out", "x"]),
        PluginCommand::New(_)
    ));
    assert!(Cli::try_parse_from(["plugin", "new", "--type", "packer", "--out", "x"]).is_err());
    let PluginCommand::Package(args) = parse(&[
        "package",
        "--manifest",
        "m.toml",
        "--component",
        "c.wasm",
        "--output",
        "p.rcplug",
        "--defaults",
        "plugins/metadata.toml",
        "--development",
    ]) else {
        panic!("package");
    };
    assert!(args.development && args.defaults.is_some() && args.key.is_none());
    let PluginCommand::Conformance(args) = parse(&[
        "conformance",
        "p.rcplug",
        "--json",
        "--trusted-key",
        "a=b",
        "--trusted-key",
        "c=d",
    ]) else {
        panic!("conformance");
    };
    assert!(args.json);
    assert_eq!(args.package.trusted_keys, ["a=b", "c=d"]);
    assert!(matches!(
        parse(&["keygen", "--out", "k.pem"]),
        PluginCommand::Keygen(_)
    ));
    assert!(matches!(
        parse(&["verify", "p.rcplug", "--development-mode"]),
        PluginCommand::Verify(_)
    ));
}

/// What `plugin new` wrote, with the test bed's component where `cargo build` would put one.
struct Scaffold {
    _temp: tempfile::TempDir,
    dir: std::path::PathBuf,
    component: std::path::PathBuf,
    trusted: String,
}

fn scaffold() -> Scaffold {
    let temp = tempfile::tempdir().expect("temp dir");
    let dir = temp.path().join("csv_view");
    super::scaffold::scaffold(super::scaffold::PluginKind::Viewer, &dir, None).expect("new");
    let component = dir.join("component.wasm");
    std::fs::write(
        &component,
        rc_plugin_host::test_support::current_viewer_component(),
    )
    .expect("component");
    let manifest = std::fs::read(dir.join("manifest.toml")).expect("manifest");
    let manifest = rc_plugin_host::parse_manifest(&manifest).expect("a valid manifest");
    let trusted = format!("{}={}", manifest.key_id, manifest.public_key);
    Scaffold {
        _temp: temp,
        dir,
        component,
        trusted,
    }
}

fn package_args(scaffold: &Scaffold, output: &str) -> PackageArgs {
    PackageArgs {
        manifest: scaffold.dir.join("manifest.toml"),
        defaults: None,
        component: scaffold.component.clone(),
        locales: Some(scaffold.dir.join("locales")),
        key: Some(scaffold.dir.join("plugin-signing.key")),
        output: scaffold.dir.join(output),
        development: false,
    }
}

fn verify_args(scaffold: &Scaffold, package: &str, trusted: bool) -> VerifyArgs {
    VerifyArgs {
        package: scaffold.dir.join(package),
        trusted_keys: if trusted {
            vec![scaffold.trusted.clone()]
        } else {
            Vec::new()
        },
        development_mode: false,
    }
}

#[test]
fn a_scaffold_packages_verifies_and_passes_conformance() {
    let scaffold = scaffold();
    package(&package_args(&scaffold, "out/csv_view.rcplug")).expect("package");
    verify(&verify_args(&scaffold, "out/csv_view.rcplug", true)).expect("verify");
    conformance(&ConformanceArgs {
        package: verify_args(&scaffold, "out/csv_view.rcplug", true),
        json: true,
    })
    .expect("conformance");

    let untrusted =
        verify(&verify_args(&scaffold, "out/csv_view.rcplug", false)).expect_err("an unknown key");
    let text = format!("{untrusted:#}");
    assert!(
        text.contains("untrusted key `csv_view-release-v1`"),
        "{text}"
    );
    assert!(
        text.contains(&format!("--trusted-key {}", scaffold.trusted)),
        "{text}"
    );
}

#[test]
fn an_unsigned_package_is_for_development_mode_only() {
    let scaffold = scaffold();
    let mut args = package_args(&scaffold, "dev.rcplug");
    args.development = true;
    args.key = None;
    package(&args).expect("an unsigned package");
    assert!(verify(&verify_args(&scaffold, "dev.rcplug", true)).is_err());
    let mut development = verify_args(&scaffold, "dev.rcplug", false);
    development.development_mode = true;
    verify(&development).expect("development mode accepts it");
}

#[test]
fn the_shared_fields_complete_a_bundled_manifest() {
    let scaffold = scaffold();
    let source = std::fs::read_to_string(scaffold.dir.join("manifest.toml")).expect("manifest");
    let stripped: String = source
        .lines()
        .filter(|line| {
            !line.starts_with("manifest_version")
                && !line.starts_with("key_id")
                && !line.starts_with("public_key")
                && !line.starts_with("author")
        })
        .map(|line| format!("{line}\n"))
        .collect();
    std::fs::write(scaffold.dir.join("bundled.toml"), stripped).expect("write");
    std::fs::write(
        scaffold.dir.join("metadata.toml"),
        "manifest_version = 1\nkey_id = \"csv_view-release-v1\"\n\n[metadata]\nauthor = \"Someone\"\n",
    )
    .expect("defaults");
    let mut args = package_args(&scaffold, "bundled.rcplug");
    args.manifest = scaffold.dir.join("bundled.toml");
    assert!(package(&args).is_err(), "incomplete without the defaults");
    args.defaults = Some(scaffold.dir.join("metadata.toml"));
    package(&args).expect("completed from the defaults and the key");
    verify(&verify_args(&scaffold, "bundled.rcplug", true)).expect("verify");
}

#[test]
fn a_conformance_failure_exits_with_an_error() {
    let scaffold = scaffold();
    std::fs::write(scaffold.dir.join("broken.rcplug"), b"not a package").expect("write");
    let failed = conformance(&ConformanceArgs {
        package: verify_args(&scaffold, "broken.rcplug", true),
        json: false,
    })
    .expect_err("a broken package");
    assert!(failed.to_string().contains("does not pass conformance"));
}

#[test]
fn keygen_writes_a_private_key_once() {
    let temp = tempfile::tempdir().expect("temp dir");
    let out = temp.path().join("release.pem");
    keygen(&KeygenArgs { out: out.clone() }).expect("keygen");
    let pem = std::fs::read_to_string(&out).expect("the key");
    rc_plugin_host::load_signing_key_pem(&pem).expect("a signing key");
    assert!(
        keygen(&KeygenArgs { out: out.clone() }).is_err(),
        "never replaced"
    );
    assert_eq!(std::fs::read_to_string(&out).expect("the key"), pem);
}

#[test]
fn locales_are_lang_json_files_only() {
    let temp = tempfile::tempdir().expect("temp dir");
    std::fs::write(temp.path().join("en.json"), b"{}").expect("en");
    std::fs::write(temp.path().join("de.json"), b"{}").expect("de");
    let locales = read_locales(temp.path()).expect("locales");
    let languages: Vec<&str> = locales.iter().map(|(tag, _)| tag.as_str()).collect();
    assert_eq!(languages, ["de", "en"]);
    std::fs::write(temp.path().join("notes.txt"), b"x").expect("notes");
    assert!(read_locales(temp.path()).is_err());
}

#[test]
fn a_fingerprint_reads_in_blocks_of_eight() {
    assert_eq!(
        group_fingerprint("0123456789abcdef01"),
        "01234567 89abcdef 01"
    );
    let error = describe_verify_error(rc_plugin_host::VerifyError::UntrustedKey {
        key_id: "k".to_owned(),
        public_key: "PUB".to_owned(),
        fingerprint: "aaaaaaaabbbbbbbb".to_owned(),
        name: "N".to_owned(),
        version: "1.0.0".to_owned(),
    });
    let text = error.to_string();
    assert!(text.contains("aaaaaaaa bbbbbbbb"), "{text}");
    assert!(text.contains("--trusted-key k=PUB"), "{text}");
}
