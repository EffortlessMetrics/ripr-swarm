//! `autobins = false` and `autolib = false` turn Cargo's target discovery
//! off (#15877): an undeclared `src/main.rs`, `src/bin/*.rs` or `src/lib.rs`
//! is never compiled, so a change to it, or to a module only it mounts, is
//! not behavior any build runs. Each case pairs the disabled manifest with
//! an enabled or declared control that still reports the same change.

use ripr::{CheckInput, Mode, OutputFormat, check_workspace};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const BULK: &str = "pub fn bulk(qty: u32) -> bool {\n    qty >= 10\n}\n";

fn manifest(package_extra: &str, targets: &str) -> String {
    format!(
        "[package]\nname = \"price\"\nversion = \"0.1.0\"\nedition = \"2024\"\n{package_extra}\n{targets}\n[workspace]\n"
    )
}

fn predicate_diff(path: &str) -> String {
    format!(
        "diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n@@ -1,3 +1,3 @@\n pub fn bulk(qty: u32) -> bool {{\n-    qty > 10\n+    qty >= 10\n }}\n"
    )
}

struct Fixture(PathBuf);

impl Fixture {
    fn with_files(files: &[(&str, &str)]) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let fixture = Self(std::env::temp_dir().join(format!(
            "ripr-disabled-target-discovery-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        )));
        for (relative, text) in files {
            let path = fixture.0.join(relative);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| format!("create directory {}: {error}", parent.display()))?;
            }
            std::fs::write(&path, text)
                .map_err(|error| format!("write file {}: {error}", path.display()))?;
        }
        Ok(fixture)
    }

    /// Whether the check reported at least one finding in `changed`.
    fn reports(&self, changed: &str) -> Result<bool, String> {
        let output = check_workspace(CheckInput {
            root: self.0.clone(),
            diff_file: Some(self.0.join("diff.patch")),
            mode: Mode::Draft,
            format: OutputFormat::Json,
            ..CheckInput::default()
        })?;
        Ok(output.findings.iter().any(|finding| {
            let file = &finding.probe.location.file;
            let relative = file.strip_prefix(&self.0).unwrap_or(file);
            let relative = relative.to_string_lossy().replace('\\', "/");
            Path::new(relative.trim_start_matches("./")) == Path::new(changed)
        }))
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A package whose `src/main.rs` mounts `src/tool.rs`, with a change to
/// the mounted module.
fn main_mounting_tool(manifest: &str) -> Result<Fixture, String> {
    Fixture::with_files(&[
        ("Cargo.toml", manifest),
        ("src/lib.rs", "pub fn unrelated() {}\n"),
        ("src/main.rs", "mod tool;\n\nfn main() {}\n"),
        ("src/tool.rs", BULK),
        ("diff.patch", &predicate_diff("src/tool.rs")),
    ])
}

#[test]
fn autobins_false_drops_an_undeclared_main_and_its_modules() -> Result<(), String> {
    let enabled = main_mounting_tool(&manifest("", ""))?;
    assert!(
        enabled.reports("src/tool.rs")?,
        "control: discovered main.rs"
    );

    let disabled = main_mounting_tool(&manifest("autobins = false", ""))?;
    assert!(
        !disabled.reports("src/tool.rs")?,
        "autobins = false never builds src/main.rs, so src/tool.rs is not production"
    );

    // A path-less `[[bin]]` carrying the package's name resolves to
    // `src/main.rs` even with discovery off.
    let declared =
        main_mounting_tool(&manifest("autobins = false", "[[bin]]\nname = \"price\"\n"))?;
    assert!(
        declared.reports("src/tool.rs")?,
        "declared package-named bin"
    );
    Ok(())
}

fn extra_bin(manifest: &str) -> Result<Fixture, String> {
    Fixture::with_files(&[
        ("Cargo.toml", manifest),
        ("src/lib.rs", "pub fn unrelated() {}\n"),
        ("src/bin/extra.rs", BULK),
        ("diff.patch", &predicate_diff("src/bin/extra.rs")),
    ])
}

#[test]
fn autobins_false_drops_undeclared_src_bin_targets() -> Result<(), String> {
    let enabled = extra_bin(&manifest("", ""))?;
    assert!(
        enabled.reports("src/bin/extra.rs")?,
        "control: discovered bin"
    );

    let disabled = extra_bin(&manifest("autobins = false", ""))?;
    assert!(!disabled.reports("src/bin/extra.rs")?, "undeclared bin");

    let declared = extra_bin(&manifest("autobins = false", "[[bin]]\nname = \"extra\"\n"))?;
    assert!(
        declared.reports("src/bin/extra.rs")?,
        "a path-less [[bin]] defaults to src/bin/<name>.rs"
    );
    Ok(())
}

fn library(manifest: &str) -> Result<Fixture, String> {
    Fixture::with_files(&[
        ("Cargo.toml", manifest),
        ("src/lib.rs", BULK),
        ("src/main.rs", "fn main() {}\n"),
        ("diff.patch", &predicate_diff("src/lib.rs")),
    ])
}

#[test]
fn autolib_false_drops_an_undeclared_library() -> Result<(), String> {
    let enabled = library(&manifest("", ""))?;
    assert!(
        enabled.reports("src/lib.rs")?,
        "control: discovered library"
    );

    let disabled = library(&manifest("autolib = false", ""))?;
    assert!(!disabled.reports("src/lib.rs")?, "undeclared library");

    let declared = library(&manifest("autolib = false", "[lib]\nname = \"price\"\n"))?;
    assert!(
        declared.reports("src/lib.rs")?,
        "a [lib] table without path defaults to src/lib.rs"
    );
    Ok(())
}

#[test]
fn a_test_only_package_judges_its_undiscovered_sources() -> Result<(), String> {
    // Cargo accepts `autobins = false` with only a `tests/` target. With no
    // library, the walk still judges the package from its manifest: the
    // undeclared `src/main.rs` and the module it mounts are never built.
    let files = |manifest: String| -> Result<Fixture, String> {
        Fixture::with_files(&[
            ("Cargo.toml", &manifest),
            ("src/main.rs", "mod tool;\n\nfn main() {}\n"),
            ("src/tool.rs", BULK),
            ("tests/smoke.rs", "#[test]\nfn smoke() {}\n"),
            ("diff.patch", &predicate_diff("src/tool.rs")),
        ])
    };
    let enabled = files(manifest("", ""))?;
    assert!(
        enabled.reports("src/tool.rs")?,
        "control: discovered main.rs"
    );
    let disabled = files(manifest("autobins = false", ""))?;
    assert!(
        !disabled.reports("src/tool.rs")?,
        "a test-only package builds neither src/main.rs nor src/tool.rs"
    );
    Ok(())
}

#[test]
fn an_edition_2015_bin_keeps_the_legacy_main_fallback() -> Result<(), String> {
    // Edition 2015 resolves a path-less `[[bin]] name = "cli"` with no
    // `src/bin/cli.rs` to `src/main.rs` (Cargo's legacy fallback), so the
    // module it mounts is built and still reports.
    let legacy = main_mounting_tool(
        "[package]\nname = \"price\"\nversion = \"0.1.0\"\nautobins = false\n\n[[bin]]\nname = \"cli\"\n\n[workspace]\n",
    )?;
    assert!(
        legacy.reports("src/tool.rs")?,
        "legacy fallback to src/main.rs"
    );
    let undeclared = main_mounting_tool(
        "[package]\nname = \"price\"\nversion = \"0.1.0\"\nautobins = false\n\n[workspace]\n",
    )?;
    assert!(
        !undeclared.reports("src/tool.rs")?,
        "edition 2015 without a [[bin]] still builds no src/main.rs"
    );
    Ok(())
}

#[test]
fn an_edition_2015_declared_bin_turns_off_bin_discovery() -> Result<(), String> {
    // Without an `edition` key the package is edition 2015, where declaring
    // any `[[bin]]` leaves `autobins` off unless set (#15877): Cargo builds
    // `src/cli.rs` only, so `src/main.rs` and the module it mounts are never
    // compiled. Edition 2021 and `autobins = true` build them.
    let legacy = |package_keys: &str| {
        main_mounting_tool(&format!(
            "[package]\nname = \"price\"\nversion = \"0.1.0\"\n{package_keys}\n[[bin]]\nname = \"cli\"\npath = \"src/cli.rs\"\n\n[workspace]\n"
        ))
    };
    let disabled = legacy("")?;
    std::fs::write(disabled.0.join("src/cli.rs"), "fn main() {}\n")
        .map_err(|error| format!("write cli: {error}"))?;
    assert!(
        !disabled.reports("src/tool.rs")?,
        "edition 2015 with a declared [[bin]] never builds src/main.rs"
    );
    for keys in ["edition = \"2021\"\n", "autobins = true\n"] {
        let built = legacy(keys)?;
        std::fs::write(built.0.join("src/cli.rs"), "fn main() {}\n")
            .map_err(|error| format!("write cli: {error}"))?;
        assert!(built.reports("src/tool.rs")?, "{keys} builds src/main.rs");
    }
    Ok(())
}

/// Cargo resolves a path-less edition 2015 `[[bin]]` through its inferred
/// layout first, so `src/bin/cli.rs` wins and a stray `src/cli.rs` is never
/// compiled (#16051 review). Without `src/bin/cli.rs` the legacy fallback
/// builds `src/cli.rs`, which then reports.
#[test]
fn an_edition_2015_bin_prefers_the_inferred_layout_over_the_legacy_fallback() -> Result<(), String>
{
    let manifest = "[package]\nname = \"price\"\nversion = \"0.1.0\"\n\n[[bin]]\nname = \"cli\"\n\n[workspace]\n";
    let stray = Fixture::with_files(&[
        ("Cargo.toml", manifest),
        ("src/lib.rs", "pub fn unrelated() {}\n"),
        ("src/bin/cli.rs", "fn main() {}\n"),
        ("src/cli.rs", BULK),
        ("diff.patch", &predicate_diff("src/cli.rs")),
    ])?;
    assert!(
        !stray.reports("src/cli.rs")?,
        "src/bin/cli.rs is the `cli` target, so src/cli.rs is never compiled"
    );

    let legacy = Fixture::with_files(&[
        ("Cargo.toml", manifest),
        ("src/lib.rs", "pub fn unrelated() {}\n"),
        ("src/cli.rs", BULK),
        ("diff.patch", &predicate_diff("src/cli.rs")),
    ])?;
    assert!(
        legacy.reports("src/cli.rs")?,
        "control: with no inferred layout the legacy fallback builds src/cli.rs"
    );
    Ok(())
}
