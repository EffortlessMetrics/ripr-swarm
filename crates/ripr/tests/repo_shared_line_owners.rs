//! Real repo CLI ownership controls for #9135. Same-line evidence must neither
//! erase a production seam nor become a production seam under another owner.
#![cfg(feature = "lang-rust")]

use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

struct Fixture(PathBuf);
impl Fixture {
    fn new(source: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-shared-line-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(std::env::temp_dir()).map_err(|e| e.to_string())?;
        std::fs::create_dir(&root).map_err(|e| e.to_string())?;
        let fixture = Self(root);
        std::fs::create_dir(fixture.0.join("src")).map_err(|e| e.to_string())?;
        std::fs::write(
            fixture.0.join("Cargo.toml"),
            "[package]\nname='shared-line'\nversion='0.0.0'\nedition='2024'\n[workspace]\n",
        )
        .map_err(|e| e.to_string())?;
        std::fs::write(fixture.0.join("src/lib.rs"), source).map_err(|e| e.to_string())?;
        Ok(fixture)
    }

    fn seams(&self) -> Result<Vec<Value>, String> {
        let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
            .args(["check", "--root"])
            .arg(&self.0)
            .args([
                "--mode",
                "instant",
                "--format",
                "repo-exposure-json",
                "--quiet",
            ])
            .env("RIPR_CACHE_DIR", self.0.join("cache"))
            .output()
            .map_err(|e| e.to_string())?;
        if !output.status.success() {
            return Err(format!(
                "CLI failed: {:?}: {}",
                output.status.code(),
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        let report: Value = serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())?;
        report
            .get("seams")
            .and_then(Value::as_array)
            .cloned()
            .ok_or("missing seams array".into())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn repo_cli_preserves_production_and_excludes_same_line_or_nested_test_shapes() -> Result<(), String>
{
    for (source, line) in [
        (include_str!("data/repo-shared-line/opening.rs"), 1),
        (include_str!("data/repo-shared-line/closing.rs"), 2),
        (
            include_str!("data/repo-shared-line/production-before-test.rs"),
            1,
        ),
        (
            include_str!("data/repo-shared-line/production-after-test.rs"),
            1,
        ),
        (include_str!("data/repo-shared-line/nested-helper.rs"), 1),
    ] {
        for boundary in [9, 10] {
            let expression = format!("n > {boundary}");
            let fixture = Fixture::new(&source.replace("n > 10", &expression))?;
            let cold = fixture.seams()?;
            let warm = fixture.seams()?;
            for seams in [&cold, &warm] {
                assert_eq!(
                    seams.len(),
                    1,
                    "production missing or test predicate leaked: {seams:?}"
                );
                let seam = &seams[0];
                assert_eq!(seam["owner"], "src/lib.rs::price");
                assert_eq!(seam["expression"], expression);
                assert_eq!(seam["line"], line);
                assert_eq!(seam["kind"], "predicate_boundary");
            }
            assert_eq!(cold[0]["seam_id"], warm[0]["seam_id"]);
            assert_eq!(cold[0]["grip_class"], warm[0]["grip_class"]);
        }
    }
    Ok(())
}

#[test]
fn repo_cli_keeps_distinct_same_line_definitions() -> Result<(), String> {
    let fixture = Fixture::new(include_str!("data/repo-shared-line/definitions.rs"))?;
    let seams: Vec<_> = fixture
        .seams()?
        .into_iter()
        .filter(|s| s["kind"] == "predicate_boundary")
        .collect();
    assert_eq!(seams.len(), 2);
    for (owner, expression) in [
        ("src/lib.rs::a::price", "n > 10"),
        ("src/lib.rs::b::price", "n > 20"),
    ] {
        assert_eq!(
            seams
                .iter()
                .filter(|s| s["owner"] == owner && s["expression"] == expression)
                .count(),
            1
        );
    }
    assert_ne!(seams[0]["seam_id"], seams[1]["seam_id"]);
    Ok(())
}

#[test]
fn repo_cli_field_write_keeps_the_impl_owner_beside_a_test() -> Result<(), String> {
    let fixture = Fixture::new(include_str!("data/repo-shared-line/field.rs"))?;
    let seams = fixture.seams()?;
    let fields: Vec<_> = seams
        .iter()
        .filter(|s| s["expression"] == "self.count = 2")
        .collect();
    assert_eq!(
        fields.len(),
        1,
        "field admission lost the real impl owner: {seams:?}"
    );
    assert_eq!(fields[0]["owner"], "src/lib.rs::impl C::bump");
    assert_eq!(fields[0]["line"], 4);
    Ok(())
}

#[test]
fn repo_cli_shared_line_classification_matches_ordinary_layout() -> Result<(), String> {
    let control = Fixture::new(include_str!("data/repo-shared-line/ordinary.rs"))?;
    let ordinary = control.seams()?;
    let ordinary = ordinary
        .iter()
        .find(|s| s["expression"] == "n > 10")
        .ok_or("missing ordinary predicate")?;
    assert_eq!(ordinary["grip_class"], "weakly_gripped");
    for source in [
        include_str!("data/repo-shared-line/production-before-test.rs"),
        include_str!("data/repo-shared-line/production-after-test.rs"),
        include_str!("data/repo-shared-line/opening.rs"),
        include_str!("data/repo-shared-line/closing.rs"),
    ] {
        let fixture = Fixture::new(source)?;
        for _ in 0..2 {
            let shared = fixture.seams()?;
            assert_eq!(shared.len(), 1);
            let shared = &shared[0];
            assert_eq!(shared["owner"], "src/lib.rs::price");
            for field in ["grip_class", "evidence", "missing_discriminators"] {
                assert_eq!(
                    shared[field], ordinary[field],
                    "formatting changed {field}: {shared:?}"
                );
            }
            assert_eq!(
                shared["related_tests"][0]["relation_reason"],
                "direct_owner_call"
            );
        }
    }
    Ok(())
}

#[test]
fn repo_cli_private_field_admission_uses_the_same_owner_as_the_seam() -> Result<(), String> {
    let shared_source = "pub struct C {\n    count: u32,\n}\nimpl C {\n    pub fn new() -> Self {\n        C { count: 0 }\n    }\n    pub fn bump(&mut self) {\n        self.count = 2; } } #[cfg(test)] mod tests { use super::*; #[test] fn t() {\n        let mut c = C::new();\n        c.bump();\n        assert!(c.count > 0);\n    }\n}\n";
    let ordinary_source = shared_source.replace("self.count = 2; } } #[cfg(test)] mod tests { use super::*; #[test] fn t() {", "self.count = 2;\n    }\n}\n#[cfg(test)]\nmod tests {\n    use super::*;\n    #[test]\n    fn t() {");
    for source in [&ordinary_source, &shared_source.to_string()] {
        let fixture = Fixture::new(source)?;
        for _ in 0..2 {
            let seams = fixture.seams()?;
            let fields: Vec<_> = seams
                .iter()
                .filter(|s| s["expression"] == "self.count = 2")
                .collect();
            assert_eq!(
                fields.len(),
                1,
                "private field lost its impl owner: {seams:?}"
            );
            assert_eq!(fields[0]["owner"], "src/lib.rs::impl C::bump");
        }
    }
    Ok(())
}
