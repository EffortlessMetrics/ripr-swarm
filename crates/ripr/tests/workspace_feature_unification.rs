//! Public-API controls for `required-features` met only through workspace
//! feature unification (#15079).
//!
//! A workspace-wide `cargo test` builds every member and unifies each
//! package's features across them. When member `b` depends on member `a`
//! with `features = ["special"]`, an `a` target that requires `special`
//! builds and its tests run, so ripr keeps their credit. A request for a
//! different feature, an inactive optional edge, a feature chain the
//! dependent does not enable and a build dependency (resolver 2) leave the
//! target skipped. The skip is read from cargo metadata, so these controls
//! need the `cargo` that runs them.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const OWNER: &str = "pub fn f(a: i32) -> i32 {\n    a + 2\n}\n";

const UNIT_TESTS: &str = "\n#[cfg(test)]\nmod tests {\n    use super::*;\n    #[test]\n    fn t() {\n        assert_eq!(f(5), 7);\n    }\n}\n";

const WORKSPACE: &str = "[workspace]\nmembers = [\"a\", \"b\"]\nresolver = \"2\"\n";

const WORKSPACE_ABC: &str = "[workspace]\nmembers = [\"a\", \"b\", \"c\"]\nresolver = \"2\"\n";

/// Member `a`: a `[[bin]]` whose unit tests need `special`, which `a`'s
/// own defaults leave off.
const BIN_MEMBER_A: &str = "[package]\nname = \"a\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n[features]\nspecial = []\nother = []\n\n[[bin]]\nname = \"tool\"\npath = \"src/main.rs\"\nrequired-features = [\"special\"]\n";

/// The same member with `special` in its own defaults: the per-package
/// control that the fixture can read `exposed` at all.
const BIN_MEMBER_A_DEFAULT: &str = "[package]\nname = \"a\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n[features]\ndefault = [\"special\"]\nspecial = []\nother = []\n\n[[bin]]\nname = \"tool\"\npath = \"src/main.rs\"\nrequired-features = [\"special\"]\n";

/// Member `a`: a library and a `[[test]]` target that needs `special`
/// (#7266).
const TEST_MEMBER_A: &str = "[package]\nname = \"a\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n[features]\nspecial = []\nother = []\n\n[[test]]\nname = \"gate\"\nrequired-features = [\"special\"]\n";

const GATE_TEST: &str = "#[test]\nfn gate() {\n    assert_eq!(a::f(5), 7);\n}\n";

/// Member `b`'s manifest with `tail` appended.
fn member_b(tail: &str) -> String {
    format!("[package]\nname = \"b\"\nversion = \"0.0.0\"\nedition = \"2021\"\n{tail}")
}

/// The unified diff that turns `a + 1` into `a + 2` in `file`, where `fn f`
/// opens on line 1.
fn diff(file: &str) -> String {
    format!(
        "diff --git a/{file} b/{file}\nindex 1111111..2222222 100644\n--- a/{file}\n+++ b/{file}\n@@ -1,3 +1,3 @@\n pub fn f(a: i32) -> i32 {{\n-    a + 1\n+    a + 2\n }}\n"
    )
}

struct TempWorkspace {
    root: PathBuf,
}

impl TempWorkspace {
    fn create(root_manifest: &str, files: &[(&str, &str)], changed: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-feature-unification-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let this = Self { root };
        let write = |relative: &str, text: &str| -> Result<(), String> {
            let path = this.root.join(relative);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| format!("create {relative} parent failed: {error}"))?;
            }
            std::fs::write(&path, text).map_err(|error| format!("write {relative} failed: {error}"))
        };
        write("Cargo.toml", root_manifest)?;
        for (relative, text) in files {
            write(relative, text)?;
        }
        write("diff.patch", &diff(changed))?;
        Ok(this)
    }

    /// The one return-value finding for the changed `a + 2` on line 2.
    fn finding(&self) -> Result<Finding, String> {
        let output = check_workspace(CheckInput {
            root: self.root.clone(),
            base: None,
            diff_file: Some(self.root.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let mut found = output.findings.into_iter().filter(|finding| {
            finding.probe.family == ProbeFamily::ReturnValue && finding.probe.location.line == 2
        });
        match (found.next(), found.next()) {
            (Some(finding), None) => Ok(finding),
            other => Err(format!(
                "premise: exactly one return-value finding on line 2, got {other:?}"
            )),
        }
    }
}

impl Drop for TempWorkspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn bin_source() -> String {
    format!("{OWNER}\nfn main() {{\n    println!(\"{{}}\", f(1));\n}}\n{UNIT_TESTS}")
}

/// Member `a` with the `required-features` bin, and member `b` as given.
fn bin_workspace_with(member_a: &str, b_manifest: &str) -> Result<TempWorkspace, String> {
    let main = bin_source();
    TempWorkspace::create(
        WORKSPACE,
        &[
            ("a/Cargo.toml", member_a),
            ("a/src/main.rs", &main),
            ("b/Cargo.toml", b_manifest),
            ("b/src/lib.rs", ""),
        ],
        "a/src/main.rs",
    )
}

fn bin_workspace(b_tail: &str) -> Result<TempWorkspace, String> {
    bin_workspace_with(BIN_MEMBER_A, &member_b(b_tail))
}

fn related_names(finding: &Finding) -> Vec<String> {
    finding
        .related_tests
        .iter()
        .map(|test| test.name.clone())
        .collect()
}

fn credits_a_discriminator(class: &ExposureClass) -> bool {
    matches!(class, ExposureClass::Exposed | ExposureClass::WeaklyExposed)
}

fn assert_credited(finding: &Finding, test: &str, label: &str) {
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "{label}: {finding:?}"
    );
    assert!(
        related_names(finding).iter().any(|name| name == test),
        "{label}: `{test}` must be a related test: {finding:?}"
    );
}

fn assert_withheld(finding: &Finding, test: &str, label: &str) {
    assert!(
        !credits_a_discriminator(&finding.class),
        "{label}: cargo test --workspace skips the target: {finding:?}"
    );
    assert!(
        !related_names(finding).iter().any(|name| name == test),
        "{label}: `{test}` never runs, so it is not a related test: {finding:?}"
    );
}

#[test]
fn a_bin_whose_required_feature_only_a_member_dependency_enables_keeps_credit() -> Result<(), String>
{
    // Premise: the fixture reads `exposed` when `a` enables `special` itself.
    let control = bin_workspace_with(BIN_MEMBER_A_DEFAULT, &member_b(""))?.finding()?;
    assert_credited(&control, "t", "own default");
    // A renamed dev-dependency is how the request reached `a` in #15079's
    // review; cargo test --workspace then builds and runs `tool`'s tests.
    let unified = bin_workspace(
        "\n[dev-dependencies]\naa = { package = \"a\", path = \"../a\", features = [\"special\"] }\n",
    )?
    .finding()?;
    assert_credited(&unified, "t", "renamed dev-dependency request");
    let normal =
        bin_workspace("\n[dependencies]\na = { path = \"../a\", features = [\"special\"] }\n")?
            .finding()?;
    assert_credited(&normal, "t", "normal dependency request");
    Ok(())
}

#[test]
fn a_bin_whose_required_feature_no_member_enables_stays_withheld() -> Result<(), String> {
    let alone = bin_workspace("")?.finding()?;
    assert_withheld(&alone, "t", "no dependency");
    let plain = bin_workspace("\n[dependencies]\na = { path = \"../a\" }\n")?.finding()?;
    assert_withheld(&plain, "t", "dependency without features");
    Ok(())
}

#[test]
fn a_dependency_requesting_a_different_feature_keeps_the_skip() -> Result<(), String> {
    let other =
        bin_workspace("\n[dependencies]\na = { path = \"../a\", features = [\"other\"] }\n")?
            .finding()?;
    assert_withheld(&other, "t", "request for `other`");
    Ok(())
}

#[test]
fn a_feature_chain_counts_only_when_the_dependent_enables_it() -> Result<(), String> {
    let on = bin_workspace(
        "\n[dependencies]\na = { path = \"../a\" }\n\n[features]\ndefault = [\"x\"]\nx = [\"a/special\"]\n",
    )?
    .finding()?;
    assert_credited(&on, "t", "default-enabled `a/special` chain");
    let off = bin_workspace(
        "\n[dependencies]\na = { path = \"../a\" }\n\n[features]\nx = [\"a/special\"]\n",
    )?
    .finding()?;
    assert_withheld(&off, "t", "`a/special` behind a feature `b` leaves off");
    Ok(())
}

#[test]
fn an_optional_dependency_counts_only_when_enabled() -> Result<(), String> {
    let off = bin_workspace(
        "\n[dependencies]\na = { path = \"../a\", optional = true, features = [\"special\"] }\n",
    )?
    .finding()?;
    assert_withheld(&off, "t", "inactive optional dependency");
    let weak = bin_workspace(
        "\n[dependencies]\na = { path = \"../a\", optional = true }\n\n[features]\ndefault = [\"x\"]\nx = [\"a?/special\"]\n",
    )?
    .finding()?;
    assert_withheld(&weak, "t", "weak `a?/special` on an inactive dependency");
    let on = bin_workspace(
        "\n[dependencies]\na = { path = \"../a\", optional = true, features = [\"special\"] }\n\n[features]\ndefault = [\"a\"]\n",
    )?
    .finding()?;
    assert_credited(&on, "t", "optional dependency enabled by default");
    Ok(())
}

#[test]
fn a_build_dependency_request_keeps_the_skip() -> Result<(), String> {
    // Resolver 2 builds a build dependency's features separately from the
    // target build that runs `tool`'s tests.
    let build = bin_workspace(
        "\n[build-dependencies]\na = { path = \"../a\", features = [\"special\"] }\n",
    )?
    .finding()?;
    assert_withheld(&build, "t", "build-dependency request");
    Ok(())
}

#[test]
fn a_request_through_an_intermediate_member_keeps_credit() -> Result<(), String> {
    // `c` enables `b`'s `x`, and `x` enables `a/special`.
    let main = bin_source();
    let b =
        member_b("\n[dependencies]\na = { path = \"../a\" }\n\n[features]\nx = [\"a/special\"]\n");
    let c = "[package]\nname = \"c\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n[dependencies]\nb = { path = \"../b\", features = [\"x\"] }\n";
    let finding = TempWorkspace::create(
        WORKSPACE_ABC,
        &[
            ("a/Cargo.toml", BIN_MEMBER_A),
            ("a/src/main.rs", &main),
            ("b/Cargo.toml", &b),
            ("b/src/lib.rs", ""),
            ("c/Cargo.toml", c),
            ("c/src/lib.rs", ""),
        ],
        "a/src/main.rs",
    )?
    .finding()?;
    assert_credited(&finding, "t", "c -> b/x -> a/special");
    Ok(())
}

#[test]
fn an_integration_target_whose_required_feature_a_member_enables_keeps_credit() -> Result<(), String>
{
    // The same unified set decides a `[[test]]` target (#7266).
    let lib = OWNER.to_string();
    let workspace = |b_tail: &str| {
        TempWorkspace::create(
            WORKSPACE,
            &[
                ("a/Cargo.toml", TEST_MEMBER_A),
                ("a/src/lib.rs", &lib),
                ("a/tests/gate.rs", GATE_TEST),
                ("b/Cargo.toml", &member_b(b_tail)),
                ("b/src/lib.rs", ""),
            ],
            "a/src/lib.rs",
        )
    };
    let unified =
        workspace("\n[dev-dependencies]\na = { path = \"../a\", features = [\"special\"] }\n")?
            .finding()?;
    assert_credited(&unified, "gate", "member request for `special`");
    let other =
        workspace("\n[dev-dependencies]\na = { path = \"../a\", features = [\"other\"] }\n")?
            .finding()?;
    assert_withheld(&other, "gate", "member request for `other`");
    Ok(())
}

/// Member `a`: a library and a `test = true` example that needs `special`
/// (#14972).
const EXAMPLE_MEMBER_A: &str = "[package]\nname = \"a\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n[features]\nspecial = []\nother = []\n\n[[example]]\nname = \"demo\"\ntest = true\nrequired-features = [\"special\"]\n";

const EXAMPLE_TESTS: &str = "fn main() {}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn t() {\n        assert_eq!(a::f(5), 7);\n    }\n}\n";

#[test]
fn a_tested_example_whose_required_feature_a_member_enables_keeps_credit() -> Result<(), String> {
    // The same unified set decides a `test = true` example (#14972).
    let lib = OWNER.to_string();
    let workspace = |b_tail: &str| {
        TempWorkspace::create(
            WORKSPACE,
            &[
                ("a/Cargo.toml", EXAMPLE_MEMBER_A),
                ("a/src/lib.rs", &lib),
                ("a/examples/demo.rs", EXAMPLE_TESTS),
                ("b/Cargo.toml", &member_b(b_tail)),
                ("b/src/lib.rs", ""),
            ],
            "a/src/lib.rs",
        )
    };
    let unified =
        workspace("\n[dev-dependencies]\na = { path = \"../a\", features = [\"special\"] }\n")?
            .finding()?;
    assert_credited(&unified, "t", "member request for `special`");
    let other =
        workspace("\n[dev-dependencies]\na = { path = \"../a\", features = [\"other\"] }\n")?
            .finding()?;
    assert_withheld(&other, "t", "member request for `other`");
    let alone = workspace("")?.finding()?;
    assert_withheld(&alone, "t", "no member request");
    Ok(())
}
