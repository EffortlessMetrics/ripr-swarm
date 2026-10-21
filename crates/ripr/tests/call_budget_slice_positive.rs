//! Decision record for #8600: an independent empty-slice assertion beside a
//! saturated slice table stays `infection_unknown`, and that is not a
//! call-budget under-credit.
//!
//! The eager slice-table family reads every related test's call to the owner.
//! A related test that passes a literal slice (`weight(&[])`) instead of a
//! table row is outside that family, so the boundary is unresolved with no
//! saturation at all. The unsaturated twin below is the discriminating
//! control: it reads exactly as the saturated one, minus the budget reason.
//! Crediting the independent assertion would therefore be a new table-family
//! rule, not a call-budget one, and is deliberately not changed here (see
//! docs/STATIC_LIMITS.md, `rust_call_fact_budget`).

use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

const OWNER: &str = "pub fn weight(samples: &[u64]) -> u64 {
    if samples.len() == 0 { return 0; }
    samples[0]
}
";

const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,3 @@
 pub fn weight(samples: &[u64]) -> u64 {
-    if samples.is_empty() { return 0; }
+    if samples.len() == 0 { return 0; }
     samples[0]
";

const TABLE: &str =
    "let samples: [&[u64]; 2] = [&[5], &[6]]; for s in samples { let _ = weight(s); }";

struct Repo {
    root: PathBuf,
}

impl Repo {
    fn create(noise: usize, independent_positive: bool) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-call-budget-slice-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"slice-budget\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        let calls: String = (0..noise).map(|i| format!("h{i}();")).collect();
        let helpers: String = (0..300).map(|i| format!("fn h{i}() {{}}\n")).collect();
        let positive = if independent_positive {
            "    #[test]\n    fn positive() {\n        assert_eq!(weight(&[]), 0);\n    }\n"
        } else {
            ""
        };
        std::fs::write(
            root.join("src/lib.rs"),
            format!(
                "{OWNER}{helpers}#[cfg(test)]\nmod tests {{\n    use super::*;\n    #[test]\n    fn checks() {{\n        {calls}\n        {TABLE}\n    }}\n{positive}}}\n"
            ),
        )
        .map_err(|error| format!("write source failed: {error}"))?;
        std::fs::write(root.join("diff.patch"), DIFF)
            .map_err(|error| format!("write diff failed: {error}"))?;
        Ok(Self { root })
    }

    /// The finding on the changed guard: its class and typed limit.
    fn guard(&self) -> Result<(String, Option<String>), String> {
        let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
            .args(["check", "--root"])
            .arg(&self.root)
            .args(["--mode", "ready", "--format", "json", "--quiet", "--diff"])
            .arg(self.root.join("diff.patch"))
            .env("RIPR_CACHE_DIR", self.root.join("cache"))
            .output()
            .map_err(|error| format!("run ripr failed: {error}"))?;
        if !matches!(output.status.code(), Some(0 | 1)) {
            return Err(format!(
                "instrument: ripr exited {:?}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        let document: Value = serde_json::from_slice(&output.stdout)
            .map_err(|error| format!("instrument: stdout is not JSON: {error}"))?;
        let findings = document
            .get("findings")
            .and_then(Value::as_array)
            .ok_or_else(|| "instrument: no findings array".to_string())?;
        let mut on_guard = findings
            .iter()
            .filter(|finding| finding.pointer("/probe/line").and_then(Value::as_u64) == Some(2));
        let (Some(finding), None) = (on_guard.next(), on_guard.next()) else {
            return Err(format!(
                "premise: exactly one guard finding in {findings:?}"
            ));
        };
        let class = finding
            .get("classification")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("finding without a class: {finding}"))?
            .to_string();
        let limit = finding
            .get("static_limit_kind")
            .and_then(Value::as_str)
            .map(str::to_string);
        Ok((class, limit))
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn the_empty_slice_positive_is_unresolved_with_and_without_saturation() -> Result<(), String> {
    // Premise: the table alone leaves infection unknown with no budget reason.
    let (class, limit) = Repo::create(10, false)?.guard()?;
    assert_eq!((class.as_str(), limit), ("infection_unknown", None));
    // The independent positive does not change that, with no saturation: the
    // non-table call is outside the eager table family. This is the control
    // that makes the saturated reading below not a budget under-credit.
    let (class, limit) = Repo::create(10, true)?.guard()?;
    assert_eq!((class.as_str(), limit), ("infection_unknown", None));
    // Saturation adds the typed reason and leaves the class where it was.
    let (class, limit) = Repo::create(257, true)?.guard()?;
    assert_eq!(
        (class.as_str(), limit.as_deref()),
        ("infection_unknown", Some("rust_call_fact_budget"))
    );
    Ok(())
}
