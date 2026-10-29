use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

fn main() {
    println!("cargo:rerun-if-changed=windows-resource.rc");
    println!("cargo:rerun-if-changed=resources/windows-long-path.manifest");
    println!("cargo:rerun-if-changed=resources/rcommander.ico");
    stamp_build();
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        write_version_resource("rCommander", "rcommander", "rcommander.exe");
        embed_resource::compile("windows-resource.rc", embed_resource::NONE)
            .manifest_required()
            .expect("failed to compile Windows resources");
    }
}

/// Compiles the commit and the build time into the binary (`rcommander --version`, the About
/// page of the settings and `app.about`, which `main` hands them to).
///
/// A package build may export both (`RC_BUILD_COMMIT`, `RC_BUILD_TIME`), and whatever form it
/// gives is taken verbatim. A development build works them out here: eight characters of the
/// commit, `-dirty` when the tree differs from it, `unknown` without git.
fn stamp_build() {
    println!("cargo:rerun-if-env-changed=RC_BUILD_COMMIT");
    println!("cargo:rerun-if-env-changed=RC_BUILD_TIME");
    let commit = exported("RC_BUILD_COMMIT").unwrap_or_else(git_commit);
    let built = exported("RC_BUILD_TIME").unwrap_or_else(utc_now);
    println!("cargo:rustc-env=RC_BUILD_COMMIT={commit}");
    println!("cargo:rustc-env=RC_BUILD_TIME={built}");
}

fn exported(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.trim().is_empty())
}

fn git(arguments: &[&str]) -> Option<String> {
    let output = Command::new("git").args(arguments).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn git_commit() -> String {
    // Without these the script would run once and a development binary would name the commit
    // it was first built at for good. The index stands in for the working tree: `-dirty` is as
    // current as the last `git add`, which is as close as a build script can watch cheaply.
    let mut watched = vec![
        "HEAD".to_owned(),
        "index".to_owned(),
        "packed-refs".to_owned(),
    ];
    watched.extend(git(&["symbolic-ref", "-q", "HEAD"]));
    for name in watched {
        if let Some(path) = git(&["rev-parse", "--git-path", &name])
            && Path::new(&path).exists()
        {
            println!("cargo:rerun-if-changed={path}");
        }
    }
    let Some(commit) = git(&["rev-parse", "--short=8", "HEAD"]) else {
        return "unknown".to_owned();
    };
    // Exit code 1 is "differs"; anything else is git failing, which is not a dirty tree.
    let dirty = Command::new("git")
        .args(["diff", "--quiet", "HEAD", "--"])
        .status()
        .is_ok_and(|status| status.code() == Some(1));
    if dirty {
        format!("{commit}-dirty")
    } else {
        commit
    }
}

/// The current time as `YYYY-MM-DDTHH:MM:SSZ`, without a date crate in the build graph.
fn utc_now() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    let (days, rest) = (seconds / 86_400, seconds % 86_400);
    // Days since 1970-01-01 to a civil date (Howard Hinnant's `civil_from_days`).
    let z = days + 719_468;
    let era = z / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + u64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

/// Writes the Windows version resource `rcommander-version.rc2` into `OUT_DIR`, where
/// `windows-resource.rc` includes it. Run only for a Windows target.
fn write_version_resource(description: &str, internal_name: &str, original_filename: &str) {
    let major = env::var("CARGO_PKG_VERSION_MAJOR").expect("package major version");
    let minor = env::var("CARGO_PKG_VERSION_MINOR").expect("package minor version");
    let patch = env::var("CARGO_PKG_VERSION_PATCH").expect("package patch version");
    let version = env::var("CARGO_PKG_VERSION").expect("package version");
    let resource = format!(
        r#"1 VERSIONINFO
FILEVERSION {major},{minor},{patch},0
PRODUCTVERSION {major},{minor},{patch},0
FILEFLAGSMASK 0x3fL
FILEFLAGS 0x0L
FILEOS 0x40004L
FILETYPE 0x1L
FILESUBTYPE 0x0L
BEGIN
    BLOCK "StringFileInfo"
    BEGIN
        BLOCK "040904B0"
        BEGIN
            VALUE "CompanyName", "DEGOYA.digital\0"
            VALUE "FileDescription", "{description}\0"
            VALUE "FileVersion", "{version}\0"
            VALUE "InternalName", "{internal_name}\0"
            VALUE "LegalCopyright", "Copyright (c) DEGOYA.digital\0"
            VALUE "OriginalFilename", "{original_filename}\0"
            VALUE "ProductName", "rCommander\0"
            VALUE "ProductVersion", "{version}\0"
        END
    END
    BLOCK "VarFileInfo"
    BEGIN
        VALUE "Translation", 0x0409, 1200
    END
END
"#
    );
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("build output directory"))
        .join("rcommander-version.rc2");
    std::fs::write(output, resource).expect("write Windows version resource");
}
