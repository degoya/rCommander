//! rCommander's plugin host (RC-100-08, stage 1 of `docs/plugins.md`): the Wasmtime engine,
//! the limits of a store and a call, the compile cache, the manifest, the locales, the in-memory
//! execution log and suspension, and the viewer kind with its [`ViewerSession`].
//!
//! Copied and adapted from rDownloader's `crates/rd-plugin-host` at commit 26c8592a6; every
//! module names its source. Plugins are signed WebAssembly components (ADR 0003): no WASI, no
//! file system, no clock or random source of their own, only the interfaces their kind is made
//! of and their manifest grants. A trap fails the one call; memory is capped per store, fuel
//! and an epoch deadline per call.
//!
//! RC-100-09 adds the packages and their trust beside them: the `.rcplug` package (`package`),
//! signing and the compiled-in roots (`sign`), the verifier with withdrawals (`verifier`,
//! `revocation`), packaging and key pairs for `rcommander plugin ...` (`packager`), the install
//! preview (`preview`), the atomic install (`install`), the installed versions with their roles
//! (`installed`, `versions`) - [`installed::list`] is what the viewer adapter runs. The
//! connection to Lister and Quick View is RC-100-13's.
//!
//! RC-100-12 adds what a plugin author runs: the conformance run (`conformance`) and the
//! completion of a bundled manifest from `plugins/metadata.toml` ([`complete_manifest`]).

#![warn(unreachable_pub)]

mod bindings;
mod call_gate;
mod compile_cache;
mod conformance;
mod diagnostics;
mod engine;
mod failure;
mod health;
mod host_impls;
mod http;
mod install;
pub mod installed;
mod limits;
mod link;
mod locales;
mod log_budget;
mod manifest;
mod package;
mod packager;
mod preview;
mod revocation;
mod runtime;
mod secrets;
pub mod sign;
mod streams;
mod verifier;
mod versions;
mod viewer;

#[cfg(test)]
mod test_fixtures;
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
/// Viewer components that crash, hang, flood or echo, for the tests of this crate and, behind the
/// `test-support` and `testbed` features, of the crates above (RC-100-09, RC-100-13).
#[cfg(any(test, feature = "test-support", feature = "testbed"))]
#[cfg_attr(
    not(test),
    allow(dead_code, reason = "the crates above use only some of the test bed")
)]
pub mod testbed;

pub use call_gate::CallSlot;
pub use compile_cache::{
    prune::{CachePrune, prune_compile_cache},
    sha256_hex,
};
pub use conformance::{ConformanceCheck, ConformanceReport, check_package};
pub use diagnostics::{
    ExecutionLog, ExecutionOutcome, ExecutionRecord, ExecutionSink, Invocation,
    MAX_EXECUTION_RECORDS, PluginIdentity,
};
pub use engine::configure_compile_cache;
pub use failure::{codes, readable};
pub use health::{
    DEFAULT_CRASH_LIMIT, FAILURE_WINDOW, MAX_CRASH_LIMIT, MIN_CRASH_LIMIT, PluginHealth,
};
pub use install::{InstalledPackage, PluginInstaller};
pub use installed::{InUse, InstalledPlugin, PluginState, VersionEntry, hold, in_use, inventory};
pub use limits::{
    DEFAULT_FUEL, DEFAULT_MAX_CONCURRENT, DEFAULT_MAX_RESPONSE_BYTES, DEFAULT_MEMORY_BYTES,
    DEFAULT_SECRETS_TIMEOUT_MILLISECONDS, DEFAULT_TIMEOUT_MILLISECONDS,
    DEFAULT_VIEWER_TIMEOUT_MILLISECONDS, MAX_CONCURRENT, MAX_FUEL, MAX_MEMORY_BYTES,
    MAX_RESPONSE_BYTES, MAX_TIMEOUT_MILLISECONDS, PluginLimits,
};
pub use locales::{
    MAX_LOCALE_BYTES, MAX_LOCALE_FILES, PluginLocale, REQUIRED_LOCALE, locale_member_language,
    parse_locale, valid_language, validate_locales,
};
pub use log_budget::MAX_LOG_LINES_PER_CALL;
pub use manifest::{
    COMPLETED_MARKER, Capabilities, KNOWN_CAPABILITIES, MANIFEST_VERSION, MAX_MANIFEST_BYTES,
    MAX_SETTING_LINE_CHARS, MAX_SETTING_TEXT_CHARS, MagicManifest, ManifestDefaults,
    ManifestRejection, MatchSection, NET_HTTP, NetHttp, PluginManifest, PluginMetadata, PluginType,
    SUPPORTED_API_VERSIONS, SettingKind, SettingManifest, ViewerSection, check_app_version,
    complete_manifest, net_http, parse_manifest,
};
pub use package::{
    COMPONENT_MEMBER, MANIFEST_MEMBER, MAX_COMPONENT_BYTES, MAX_PACKAGE_BYTES, MAX_SIGNATURE_BYTES,
    PACKAGE_EXTENSION, SIGNATURE_MEMBER, VerifiedPackage, VerifyError, package_digest,
};
pub use packager::{
    GeneratedKey, generate_signing_key, load_signing_key_pem, package_plugin, public_key_base64,
    verify_package,
};
pub use preview::{KeyStatus, PackagePreview, Permissions, preview_package};
pub use revocation::{RevokedDigests, RevokedKeys, format_package_digest, parse_package_digest};
pub use runtime::CANCEL_GRACE;
pub use secrets::{
    MAX_ENTRY_ID_CHARS, MAX_ENTRY_TEXT_CHARS, MAX_ENTRY_TITLE_CHARS, MAX_QUERY_CHARS,
    MAX_SEARCH_ENTRIES, MAX_SECRET_BYTES, SecretsOptions, SecretsPlugin,
};
pub use sign::{key_fingerprint, sign_detached, verify_detached};
pub use streams::{BytesSource, MAX_WINDOW_BYTES, MemorySink, SinkWriter, SourceReader};
pub use verifier::{APP_VERSION, PluginVerifier, REFUSAL_CODES, Refusal, refusal_code};
pub use versions::{
    VersionChoice, VersionChoices, VersionRole, compare_versions, default_version,
    previous_version, roles_by_id, version_role,
};
pub use viewer::{
    DEFAULT_OUTPUT_LIMIT_BYTES, ImageFormat, ImageOutput, MAX_LABEL_CHARS, MAX_NEEDLE_CHARS,
    MAX_PAGES, MAX_PIXELS, MAX_SIDE, MAX_TITLE_CHARS, MAX_TOKEN_LINES, MAX_TOKENS_PER_LINE,
    MAX_TREE_CHILDREN, MAX_TREE_DEPTH, MAX_TREE_NODES, MAX_VALUE_CHARS, TextOutput, TreeChildren,
    ViewerOptions, ViewerPlugin, ViewerSession, image_format, raster_bytes,
};
