//! Byte-level native equivalence and refusal/exit controls for #8169.
use super::{ignore_remove_dir_all, run_command_with_env, unique_external_workspace};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

type TestResult = Result<(), Box<dyn std::error::Error>>;

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        ignore_remove_dir_all(&self.0);
    }
}
impl Fixture {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let root = unique_external_workspace("native-fanout")?;
        std::fs::create_dir(&root)?;
        let fixture = Self(root);
        std::fs::create_dir(fixture.0.join("src"))?;
        std::fs::write(
            fixture.0.join("Cargo.toml"),
            "[package]\nname = \"fanout-fixture\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[workspace]\n",
        )?;
        std::fs::write(
            fixture.0.join("src/lib.rs"),
            "pub fn positive(value: i32) -> bool { value > 0 }\n",
        )?;
        std::fs::write(
            fixture.0.join("input.diff"),
            "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-pub fn positive(value: i32) -> bool { value >= 0 }\n+pub fn positive(value: i32) -> bool { value > 0 }\n",
        )?;
        std::fs::write(
            fixture.0.join("ripr.toml"),
            "[analysis]\nmode = \"ready\"\ninclude_unchanged_tests = false\n[languages]\nenabled = [\"rust\"]\n[severity.findings]\nno_static_path = \"info\"\n",
        )?;
        // Human-only preparation must retain the unlinked Python note without
        // adding a machine field or changing machine projection policy.
        std::fs::write(
            fixture.0.join("test_unlinked.py"),
            "def test_decoy():\n    assert True\n",
        )?;
        Ok(fixture)
    }

    fn run(
        &self,
        extra: &[&str],
        env: &[(&str, &str)],
    ) -> Result<std::process::Output, std::io::Error> {
        let root = self.0.to_string_lossy();
        let mut args = vec!["check", "--root", &root, "--diff", "input.diff", "--quiet"];
        args.extend_from_slice(extra);
        run_command_with_env(env!("CARGO_BIN_EXE_ripr"), &self.0, &args, env)
    }
}

#[test]
fn fanout_matches_native_projections_and_binds_members() -> TestResult {
    let fixture = Fixture::new()?;
    for (label, policy) in [
        ("plain", "schema_version = 1\n"),
        (
            "expired",
            "schema_version = 1\n[[suppressions]]\nkind = \"exposure_gap\"\npath = \"src/**\"\nowner = \"fixture\"\nreason = \"expired policy control\"\nexpires = \"2000-01-01\"\n",
        ),
        (
            "policy",
            "schema_version = 1\n[[suppressions]]\nkind = \"exposure_gap\"\npath = \"src/**\"\nowner = \"fixture\"\nreason = \"projection policy control\"\nexpires = \"2099-01-01\"\n",
        ),
    ] {
        std::fs::create_dir_all(fixture.0.join(".ripr"))?;
        let mut mode = Vec::new();
        let policy_path = if label == "policy" {
            let config_path = fixture.0.join("ripr.toml");
            let mut text = std::fs::read_to_string(&config_path)?;
            text.push_str("[suppressions]\npath = \"selected-policy.toml\"\n");
            std::fs::write(config_path, text)?;
            mode.extend_from_slice(&["--mode", "deep"]);
            fixture.0.join("selected-policy.toml")
        } else {
            fixture.0.join(".ripr/suppressions.toml")
        };
        std::fs::write(policy_path, policy)?;
        let mut combined_args = vec!["--output-dir", label];
        combined_args.extend_from_slice(&mode);
        let output = fixture.run(&combined_args, &[])?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.is_empty());
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(fixture.0.join(label).join("manifest.json"))?)?;
        assert_eq!(manifest["complete_set"], true);
        assert_eq!(manifest["command_success"], true);
        assert!(
            manifest["identity"]["producer"]
                .as_str()
                .is_some_and(|value| value.contains("0."))
        );
        for (format, file) in [
            ("badge-json", "badge.json"),
            ("json", "detail.json"),
            ("human", "human.txt"),
        ] {
            let mut native_args = vec!["--format", format];
            native_args.extend_from_slice(&mode);
            let standalone = fixture.run(&native_args, &[])?;
            assert!(
                standalone.status.success(),
                "{}",
                String::from_utf8_lossy(&standalone.stderr)
            );
            let bytes = std::fs::read(fixture.0.join(label).join(file))?;
            assert_eq!(
                bytes, standalone.stdout,
                "{label}/{format} native bytes diverged"
            );
            let member = manifest["members"]
                .as_array()
                .and_then(|members| members.iter().find(|member| member["file"] == file))
                .ok_or("missing member")?;
            assert_eq!(member["sha256"], format!("{:x}", Sha256::digest(&bytes)));
            assert_eq!(member["bytes"], bytes.len());
            if format == "json" {
                let detail: serde_json::Value = serde_json::from_slice(&bytes)?;
                assert!(
                    detail["findings"]
                        .as_array()
                        .is_some_and(|findings| !findings.is_empty())
                );
                assert!(
                    detail.get("canonical_next_action").is_some(),
                    "CLI navigation context must survive"
                );
            }
            if format == "human" {
                assert!(
                    String::from_utf8_lossy(&bytes).contains("Python"),
                    "nonempty Python note control"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn fanout_refusal_documents_and_unsupported_exit_are_native() -> TestResult {
    let fixture = Fixture::new()?;
    for (label, diff) in [
        ("malformed", "not a diff"),
        (
            "unsupported",
            "--- a/new.go\n+++ b/new.go\n@@ -0,0 +1 @@\n+package main\n",
        ),
    ] {
        std::fs::write(fixture.0.join("input.diff"), diff)?;
        let combined = fixture.run(&["--output-dir", label], &[])?;
        for (format, file) in [
            ("badge-json", "badge.json"),
            ("json", "detail.json"),
            ("human", "human.txt"),
        ] {
            let standalone = fixture.run(&["--format", format], &[])?;
            assert_eq!(
                combined.status.code(),
                standalone.status.code(),
                "{label}/{format} exit"
            );
            assert_eq!(
                std::fs::read(fixture.0.join(label).join(file))?,
                standalone.stdout,
                "{label}/{format} refusal"
            );
        }
    }
    Ok(())
}

#[test]
fn fanout_render_failures_never_admit_a_successful_set() -> TestResult {
    let fixture = Fixture::new()?;
    for (label, env) in [
        ("budget", vec![("RIPR_CHECK_FINDINGS_BYTES", "invalid")]),
        ("suppression", vec![]),
    ] {
        if label == "suppression" {
            std::fs::create_dir_all(fixture.0.join(".ripr"))?;
            std::fs::write(
                fixture.0.join(".ripr/suppressions.toml"),
                "schema_version = 1\n[[suppressions]]\nkind = \"invalid\"\n",
            )?;
        }
        let combined = fixture.run(&["--output-dir", label], &env)?;
        assert_eq!(combined.status.code(), Some(2));
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(fixture.0.join(label).join("manifest.json"))?)?;
        assert_eq!(manifest["complete_set"], false);
        assert_eq!(manifest["command_success"], false);
        for file in ["badge.json", "detail.json"] {
            let refusal: serde_json::Value =
                serde_json::from_slice(&std::fs::read(fixture.0.join(label).join(file))?)?;
            assert!(refusal.is_object());
        }
    }
    Ok(())
}

#[test]
fn fanout_rejects_stale_destinations_and_conflicting_interfaces() -> TestResult {
    let fixture = Fixture::new()?;
    std::fs::create_dir(fixture.0.join("stale"))?;
    std::fs::write(fixture.0.join("stale/manifest.json"), "old witness")?;
    for args in [
        vec!["--output-dir", "stale"],
        vec!["--output-dir", "mixed", "--format", "json"],
        vec![
            "--output-dir",
            "artifact",
            "--write-artifact",
            "explain.json",
        ],
        vec![
            "--output-dir",
            "explicit-policy",
            "--suppression-policy",
            ".ripr/suppressions.toml",
        ],
    ] {
        let output = fixture.run(&args, &[])?;
        assert_eq!(output.status.code(), Some(2));
    }
    assert_eq!(
        std::fs::read(fixture.0.join("stale/manifest.json"))?,
        b"old witness"
    );
    assert!(!fixture.0.join("mixed").exists());
    assert!(!fixture.0.join("artifact").exists());
    assert!(!fixture.0.join("explicit-policy").exists());
    Ok(())
}

#[cfg(feature = "lang-python")]
#[test]
fn fanout_python_source_stamp_and_navigation_are_native() -> TestResult {
    let fixture = Fixture::new()?;
    std::fs::create_dir(fixture.0.join("tests"))?;
    std::fs::write(
        fixture.0.join("src/discount.py"),
        "def apply_discount(amount, threshold):\n    if amount >= threshold:\n        return amount - 10\n    return amount\n",
    )?;
    std::fs::write(
        fixture.0.join("tests/test_discount.py"),
        "from src.discount import apply_discount\n\ndef test_apply_discount_smoke():\n    result = apply_discount(100, 50)\n    if result is None:\n        raise AssertionError(\"missing result\")\n",
    )?;
    std::fs::write(
        fixture.0.join("input.diff"),
        "--- a/src/discount.py\n+++ b/src/discount.py\n@@ -1,4 +1,4 @@\n def apply_discount(amount, threshold):\n-    if amount > threshold:\n+    if amount >= threshold:\n         return amount - 10\n     return amount\n",
    )?;
    std::fs::write(
        fixture.0.join("ripr.toml"),
        "[analysis]\nmode = \"ready\"\n[languages]\nenabled = [\"rust\", \"python\"]\n",
    )?;
    let combined = fixture.run(&["--output-dir", "python"], &[])?;
    assert!(
        combined.status.success(),
        "{}",
        String::from_utf8_lossy(&combined.stderr)
    );
    for (format, file) in [
        ("badge-json", "badge.json"),
        ("json", "detail.json"),
        ("human", "human.txt"),
    ] {
        let standalone = fixture.run(&["--format", format], &[])?;
        assert!(standalone.status.success());
        let bytes = std::fs::read(fixture.0.join("python").join(file))?;
        assert_eq!(bytes, standalone.stdout, "Python {format} byte parity");
        if format == "json" {
            let report: serde_json::Value = serde_json::from_slice(&bytes)?;
            let files = report["source_subject"]["files"]
                .as_array()
                .ok_or("native source stamp missing")?;
            assert!(files.len() >= 2, "source and test stamps must be nonempty");
            for file in files {
                let path = file["path"].as_str().ok_or("source stamp path missing")?;
                let expected = format!(
                    "sha256:{:x}",
                    Sha256::digest(std::fs::read(fixture.0.join(path))?)
                );
                assert_eq!(file["digest"], expected);
            }
            assert!(report.get("canonical_next_action").is_some());
        }
    }
    Ok(())
}

#[test]
fn fanout_invalid_configuration_refusals_match_native() -> TestResult {
    let fixture = Fixture::new()?;
    std::fs::write(fixture.0.join("ripr.toml"), "[broken")?;
    let combined = fixture.run(&["--output-dir", "config-refused"], &[])?;
    assert_eq!(combined.status.code(), Some(2));
    for (format, file) in [
        ("badge-json", "badge.json"),
        ("json", "detail.json"),
        ("human", "human.txt"),
    ] {
        let standalone = fixture.run(&["--format", format], &[])?;
        assert_eq!(standalone.status.code(), combined.status.code());
        assert_eq!(
            std::fs::read(fixture.0.join("config-refused").join(file))?,
            standalone.stdout
        );
    }
    Ok(())
}

#[test]
fn fanout_external_module_reads_refuse_without_changing_standalone() -> TestResult {
    let fixture = Fixture::new()?;
    let external = fixture.0.with_extension("external.rs");
    std::fs::write(&external, "pub mod changed;\n")?;
    let source = format!("include!({:?});\n", external.to_string_lossy());
    std::fs::write(fixture.0.join("src/lib.rs"), source)?;
    std::fs::write(
        fixture.0.join("src/changed.rs"),
        "pub fn positive(value: i32) -> bool { value > 0 }\n",
    )?;
    std::fs::write(
        fixture.0.join("input.diff"),
        "--- a/src/changed.rs\n+++ b/src/changed.rs\n@@ -1 +1 @@\n-pub fn positive(value: i32) -> bool { value >= 0 }\n+pub fn positive(value: i32) -> bool { value > 0 }\n",
    )?;
    let combined = fixture.run(&["--output-dir", "external-refused"], &[])?;
    let standalone = fixture.run(&["--format", "json"], &[])?;
    std::fs::remove_file(external)?;
    assert!(
        standalone.status.success(),
        "single-format behavior must remain available"
    );
    assert_eq!(combined.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&combined.stderr).contains("outside the root"));
    let manifest: serde_json::Value = serde_json::from_slice(&std::fs::read(
        fixture.0.join("external-refused/manifest.json"),
    )?)?;
    assert_eq!(manifest["complete_set"], false);
    Ok(())
}

#[test]
fn fanout_limited_json_keeps_native_counts_and_admission_policy() -> TestResult {
    let fixture = Fixture::new()?;
    std::fs::create_dir(fixture.0.join("tests"))?;
    std::fs::write(
        fixture.0.join("Cargo.toml"),
        "[package]\nname = \"ripr-sample\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[workspace]\n",
    )?;
    std::fs::write(
        fixture.0.join("src/lib.rs"),
        include_str!("../../examples/sample/src/lib.rs"),
    )?;
    std::fs::write(
        fixture.0.join("tests/pricing.rs"),
        include_str!("../../examples/sample/tests/pricing.rs"),
    )?;
    std::fs::write(
        fixture.0.join("input.diff"),
        include_str!("../../examples/sample/example.diff")
            .replace("crates/ripr/examples/sample/", ""),
    )?;
    std::fs::write(
        fixture.0.join("ripr.toml"),
        "[analysis]\nmode = \"ready\"\n[languages]\nenabled = [\"rust\"]\n",
    )?;
    let env = [("RIPR_CHECK_FINDINGS_BYTES", "1")];
    let combined = fixture.run(&["--output-dir", "limited"], &env)?;
    assert!(
        combined.status.success(),
        "{}",
        String::from_utf8_lossy(&combined.stderr)
    );
    for (format, file) in [
        ("badge-json", "badge.json"),
        ("json", "detail.json"),
        ("human", "human.txt"),
    ] {
        let standalone = fixture.run(&["--format", format], &env)?;
        assert!(standalone.status.success());
        let bytes = std::fs::read(fixture.0.join("limited").join(file))?;
        assert_eq!(bytes, standalone.stdout, "limited {format} native parity");
        if format == "json" {
            let report: serde_json::Value = serde_json::from_slice(&bytes)?;
            assert_eq!(report["findings"].as_array().map(Vec::len), Some(1));
            assert!(
                report["summary"]["findings"]
                    .as_u64()
                    .is_some_and(|count| count > 1)
            );
            assert_eq!(report["run_limitations"][0]["downstream_consumable"], false);
            assert!(report.get("canonical_next_action").is_some());
        }
    }
    Ok(())
}

#[cfg(feature = "lang-typescript")]
#[test]
fn fanout_typescript_native_projections_match() -> TestResult {
    let fixture = Fixture::new()?;
    std::fs::create_dir(fixture.0.join("tests"))?;
    std::fs::write(
        fixture.0.join("src/discount.ts"),
        "export function discount(amount: number): number {\n    if (amount >= 100) { return amount - 10; }\n    return amount;\n}\n",
    )?;
    std::fs::write(
        fixture.0.join("tests/discount.test.ts"),
        "test('discount smoke', () => { const result = discount(200); if (result !== 190) { throw new Error('expected discount'); } });\n",
    )?;
    std::fs::write(
        fixture.0.join("input.diff"),
        "--- a/src/discount.ts\n+++ b/src/discount.ts\n@@ -1,4 +1,4 @@\n export function discount(amount: number): number {\n-    if (amount > 100) { return amount - 10; }\n+    if (amount >= 100) { return amount - 10; }\n     return amount;\n }\n",
    )?;
    std::fs::write(
        fixture.0.join("ripr.toml"),
        "[analysis]\nmode = \"ready\"\n[languages]\nenabled = [\"rust\", \"typescript\"]\n",
    )?;
    let combined = fixture.run(&["--output-dir", "typescript"], &[])?;
    assert!(
        combined.status.success(),
        "{}",
        String::from_utf8_lossy(&combined.stderr)
    );
    for (format, file) in [
        ("badge-json", "badge.json"),
        ("json", "detail.json"),
        ("human", "human.txt"),
    ] {
        let standalone = fixture.run(&["--format", format], &[])?;
        assert!(standalone.status.success());
        let bytes = std::fs::read(fixture.0.join("typescript").join(file))?;
        assert_eq!(
            bytes, standalone.stdout,
            "TypeScript {format} native parity"
        );
        if format == "json" {
            let report: serde_json::Value = serde_json::from_slice(&bytes)?;
            assert!(report["findings"].as_array().is_some_and(|findings| {
                findings
                    .iter()
                    .any(|finding| finding["language"] == "typescript")
            }));
        }
    }
    Ok(())
}
