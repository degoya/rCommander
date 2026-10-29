use rc_plugin_api::viewer::Output;

use super::{
    MAX_MANIFEST_BYTES, ManifestRejection, PluginType, SettingKind, check_app_version,
    parse_manifest,
};
use crate::limits::{DEFAULT_FUEL, DEFAULT_VIEWER_TIMEOUT_MILLISECONDS, MAX_MEMORY_BYTES};
use crate::test_fixtures::{viewer_manifest, viewer_manifest_text};

fn parse(text: &str) -> anyhow::Result<super::PluginManifest> {
    parse_manifest(text.as_bytes())
}

fn refusal(text: &str) -> String {
    format!("{:#}", parse(text).expect_err("refused"))
}

#[test]
fn the_designs_viewer_manifest_is_read() {
    let manifest = viewer_manifest(&["markdown", "text"]);
    assert_eq!(manifest.plugin_type, PluginType::Viewer);
    assert_eq!(
        manifest.viewer.expect("[viewer]").outputs,
        [Output::Markdown, Output::Text]
    );
    assert_eq!(manifest.matching.extensions, ["csv", "tsv"]);
    assert_eq!(manifest.limits.fuel, DEFAULT_FUEL);
    assert!(manifest.capabilities.declared.is_empty());
}

#[test]
fn unknown_keys_are_refused_in_every_section() {
    for (needle, replacement) in [
        (
            "name = \"Test bed\"",
            "name = \"Test bed\"\nicon = \"x.png\"",
        ),
        ("max_size = 268435456", "max_size = 268435456\nglob = \"*\""),
        (
            "outputs = [\"text\"]",
            "outputs = [\"text\"]\nrenderer = \"html\"",
        ),
        ("license = \"MIT\"", "license = \"MIT\"\ntwitter = \"@x\""),
    ] {
        let text = viewer_manifest_text(&["text"], "").replace(needle, replacement);
        assert!(text.contains(replacement), "{needle}");
        let message = refusal(&text);
        assert!(message.contains("unknown field"), "{needle}: {message}");
    }
    let limits = viewer_manifest_text(&["text"], "[limits]\nthreads = 4\n");
    assert!(refusal(&limits).contains("unknown field"));
    let settings = viewer_manifest_text(
        &["text"],
        "[[settings]]\nkey = \"a\"\ntype = \"text\"\nlabel = \"A\"\n",
    );
    assert!(refusal(&settings).contains("unknown field"));
}

#[test]
fn an_unknown_capability_type_or_version_is_a_rejection() {
    let capability = viewer_manifest_text(&["text"], "")
        .replace("[capabilities]", "[capabilities]\nclipboard = true");
    let error = parse(&capability).expect_err("refused");
    let rejection = error
        .downcast_ref::<ManifestRejection>()
        .expect("a rejection");
    assert_eq!(rejection.code(), "plugin_capability_unknown");

    let kind = viewer_manifest_text(&["text"], "").replace("\"viewer\"", "\"packer\"");
    let kind = kind.replace("[viewer]\noutputs = [\"text\"]\n", "");
    assert!(
        parse(&kind)
            .expect_err("refused")
            .downcast_ref::<ManifestRejection>()
            .is_some()
    );

    let api = viewer_manifest_text(&["text"], "").replace("\"0.1.0\"\nid", "\"0.9.0\"\nid");
    assert!(refusal(&api).contains("api_version"));

    let outdated = viewer_manifest_text(&["text"], "")
        .replace("manifest_version = 1", "manifest_version = 3\nprovider = 1");
    let error = parse(&outdated).expect_err("refused");
    assert_eq!(
        error
            .downcast_ref::<ManifestRejection>()
            .map(ManifestRejection::code),
        Some("plugin_manifest_outdated"),
        "an older revision says so before naming a key it did not have"
    );
}

#[test]
fn a_viewer_holds_no_capability_in_stage_one() {
    let text = viewer_manifest_text(&["text"], "").replace(
        "[capabilities]",
        "[capabilities]\nnet_http = { domains = [\"example.net\"] }",
    );
    assert!(refusal(&text).contains("cannot be granted the capability `net_http`"));
}

#[test]
fn limits_are_held_to_their_ceilings() {
    let over = viewer_manifest_text(
        &["text"],
        &format!("[limits]\nmemory_bytes = {}\n", MAX_MEMORY_BYTES + 1),
    );
    assert!(refusal(&over).contains("limits.memory_bytes"));
    let zero = viewer_manifest_text(&["text"], "[limits]\nfuel = 0\n");
    assert!(refusal(&zero).contains("limits.fuel"));
    let concurrent = viewer_manifest_text(&["text"], "[limits]\nmax_concurrent = 17\n");
    assert!(refusal(&concurrent).contains("limits.max_concurrent"));
    let day = viewer_manifest_text(&["text"], "[limits]\ntimeout_milliseconds = 86400000\n");
    assert!(refusal(&day).contains("limits.timeout_milliseconds"));

    let own = parse(&viewer_manifest_text(
        &["text"],
        "[limits]\ntimeout_milliseconds = 2000\n",
    ))
    .expect("within");
    assert_eq!(own.timeout_milliseconds(), 2000);
    assert_eq!(
        viewer_manifest(&["text"]).timeout_milliseconds(),
        DEFAULT_VIEWER_TIMEOUT_MILLISECONDS,
        "a viewer's own default"
    );
}

#[test]
fn viewer_outputs_are_listed_once_each_and_known() {
    assert!(refusal(&viewer_manifest_text(&[], "")).contains("at least one output"));
    assert!(refusal(&viewer_manifest_text(&["text", "text"], "")).contains("twice"));
    assert!(refusal(&viewer_manifest_text(&["html"], "")).contains("unknown variant"));
    let missing =
        viewer_manifest_text(&["text"], "").replace("[viewer]\noutputs = [\"text\"]\n", "");
    assert!(refusal(&missing).contains("needs a [viewer] section"));
    let all = viewer_manifest(&["text", "highlighted", "markdown", "image", "tree", "pages"]);
    assert_eq!(all.viewer.expect("[viewer]").outputs, Output::ALL);
}

#[test]
fn match_names_plain_extensions_and_hex_magic() {
    let none = viewer_manifest_text(&["text"], "")
        .replace("extensions = [\"csv\", \"tsv\"]", "extensions = []");
    assert!(refusal(&none).contains("no file is ever offered"));
    for bad in ["\"CSV\"", "\".csv\"", "\"c/sv\"", "\"csv\", \"csv\""] {
        let text =
            viewer_manifest_text(&["text"], "").replace("[\"csv\", \"tsv\"]", &format!("[{bad}]"));
        assert!(refusal(&text).contains("[match] extension"), "{bad}");
    }
    let magic = viewer_manifest_text(&["text"], "")
        .replace("magic = []", "magic = [{ offset = 0, hex = \"25PDF\" }]");
    assert!(refusal(&magic).contains("not hex bytes"));

    let pdf = viewer_manifest_text(&["text"], "")
        .replace("magic = []", "magic = [{ offset = 1, hex = \"504446\" }]")
        .replace("max_size = 268435456", "max_size = 100");
    let matching = parse(&pdf).expect("valid").matching;
    assert_eq!(matching.head_length(), 4);
    assert!(matching.matches("data.CSV", b"", 10));
    assert!(matching.matches("report", b"%PDF-1.7", 10));
    assert!(!matching.matches("report", b"%PDX", 10));
    assert!(!matching.matches("big.csv", b"", 101), "over max_size");
}

#[test]
fn settings_are_typed_and_bounded() {
    let separator = "[[settings]]\nkey = \"separator\"\ntype = \"choice\"\nvalues = [\"auto\", \"comma\"]\ndefault = \"auto\"\n";
    let manifest = parse(&viewer_manifest_text(&["text"], separator)).expect("valid");
    assert_eq!(manifest.settings[0].kind, SettingKind::Choice);
    assert!(manifest.settings[0].accepts("comma"));
    assert!(!manifest.settings[0].accepts("tab"));

    for (bad, why) in [
        (
            separator.replace("default = \"auto\"", "default = \"tab\""),
            "defaults",
        ),
        (
            separator.replace("values = [\"auto\", \"comma\"]", "values = []"),
            "values",
        ),
        (
            separator.replace("key = \"separator\"", "key = \"Sep\""),
            "key",
        ),
        (format!("{separator}{separator}"), "twice"),
        (
            "[[settings]]\nkey = \"note\"\ntype = \"text\"\nvalues = [\"a\"]\n".to_owned(),
            "no values",
        ),
    ] {
        let message = refusal(&viewer_manifest_text(&["text"], &bad));
        assert!(message.contains(why), "{why}: {message}");
    }
    let text = parse(&viewer_manifest_text(
        &["text"],
        "[[settings]]\nkey = \"note\"\ntype = \"text\"\nmultiline = true\n",
    ))
    .expect("valid");
    assert!(text.settings[0].accepts("two\nlines"));
    assert!(!text.settings[0].accepts("bell\u{7}"));
}

#[test]
fn identity_fields_are_checked() {
    let base = viewer_manifest_text(&["text"], "");
    for (from, to, why) in [
        (
            "019d0000-0000-7000-8000-0000000000a1",
            "019d0000-0000-4000-8000-0000000000a1",
            "UUIDv7",
        ),
        ("version = \"1.0.0\"", "version = \"1.0\"", "semantic"),
        ("version = \"1.0.0\"", "version = \"../1.0.0\"", "semantic"),
        (
            "key_id = \"example-release-v1\"",
            "key_id = \"Example Key\"",
            "key_id",
        ),
        (
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
            "AAAA",
            "32-byte",
        ),
        ("name = \"Test bed\"", "name = \"   \"", "name is required"),
        (
            "homepage = \"https://example.net/test-bed\"",
            "homepage = \"http://example.net\"",
            "https",
        ),
    ] {
        let message = refusal(&base.replace(from, to));
        assert!(message.contains(why), "{why}: {message}");
    }
}

#[test]
fn a_manifest_over_a_mebibyte_is_not_read() {
    let mut text = viewer_manifest_text(&["text"], "");
    text.push_str(&format!("# {}\n", "x".repeat(MAX_MANIFEST_BYTES)));
    assert!(refusal(&text).contains("exceeds"));
}

#[test]
fn a_plugin_for_a_newer_rcommander_is_refused() {
    let manifest = viewer_manifest(&["text"]);
    assert!(check_app_version(&manifest, "0.1.0").is_ok());
    assert!(check_app_version(&manifest, "0.0.9").is_err());
}
