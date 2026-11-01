//! RIPR-SPEC-0373: a diff whose only changed Rust lines are Kani verification
//! artifacts (deleted or added proof harnesses, `cfg(kani)` modules, `cfg_attr(kani, ..)`
//! contracts) seeds no production probe and is disclosed as
//! `verification_only_change`. The shape is the "use function contracts instead
//! of proofs" conversion: a `#[cfg(kani)] mod proofs` is deleted while contracts
//! and nested `proof_for_contract` harnesses are added; no production line moves.
//!
//! Every control drives the real `ripr check` binary on a diff file.

use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

/// Post-image of the toy conversion: contracts + nested harness added.
const NEW_SOURCE: &str = "/// Rounds down.
#[cfg_attr(
    kani,
    kani::requires(align > 0),
    kani::ensures(|&m| m <= n)
)]
pub fn round_down(n: usize, align: usize) -> usize {
    #[cfg(kani)]
    #[kani::proof_for_contract(round_down)]
    fn proof() {
        round_down(kani::any(), kani::any());
    }

    n / align * align
}

#[cfg(test)]
mod tests {
    use super::round_down;
    #[test]
    fn rounds() { assert_eq!(round_down(7, 4), 4); }
}
";

/// Whole-file conversion diff: contracts and nested harness added, the old
/// `#[cfg(kani)] mod proofs` deleted. Production line `n / align * align` is context.
const CONVERSION_DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,14 @@
 /// Rounds down.
+#[cfg_attr(
+    kani,
+    kani::requires(align > 0),
+    kani::ensures(|&m| m <= n)
+)]
 pub fn round_down(n: usize, align: usize) -> usize {
+    #[cfg(kani)]
+    #[kani::proof_for_contract(round_down)]
+    fn proof() {
+        round_down(kani::any(), kani::any());
+    }
+
     n / align * align
@@ -10,19 +21,2 @@
     fn rounds() { assert_eq!(round_down(7, 4), 4); }
 }
-
-#[cfg(kani)]
-mod proofs {
-    use super::*;
-
-    #[kani::proof]
-    fn prove_round_down() {
-        fn model(n: usize, align: usize) -> usize {
-            let mul = n / align;
-            mul * align
-        }
-        let align: usize = kani::any();
-        kani::assume(align > 0);
-        let n: usize = kani::any();
-        assert_eq!(model(n, align), round_down(n, align));
-    }
-}
";

/// Only the deletion of an old `mod proofs` (new side has no trace of it).
const DELETE_ONLY_SOURCE: &str = "pub fn round_down(n: usize, align: usize) -> usize {
    n / align * align
}
";

fn delete_only_diff(module_attr: &str, harness_attr: &str) -> String {
    format!(
        "diff --git a/src/lib.rs b/src/lib.rs
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -3,12 +3,1 @@
 }}
-
-{module_attr}
-mod proofs {{
-    use super::*;
-
-    {harness_attr}
-    fn prove_round_down() {{
-        let n: usize = kani::any();
-        assert_eq!(round_down(n, 1), n);
-    }}
-}}
"
    )
}

struct Run {
    json: Value,
}

impl Run {
    fn kind(&self) -> &str {
        self.json["analysis_outcome"]["outcome"]["kind"]
            .as_str()
            .unwrap_or_default()
    }

    fn limitation_kinds(&self) -> Vec<&str> {
        self.json["analysis_outcome"]["outcome"]["limitations"]
            .as_array()
            .map(|items| items.iter().filter_map(|l| l["kind"].as_str()).collect())
            .unwrap_or_default()
    }

    fn finding_lines(&self) -> Vec<u64> {
        self.json["findings"]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|f| f["probe"]["line"].as_u64())
                    .collect()
            })
            .unwrap_or_default()
    }
}

fn check(source: &str, diff: &str) -> Result<Run, String> {
    let base: PathBuf = std::env::temp_dir().join(format!(
        "ripr-kani-proof-only-{}-{}",
        std::process::id(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&base);
    let root = base.join("fx");
    let write = |path: PathBuf, text: &str| {
        fs::write(&path, text).map_err(|error| format!("write {}: {error}", path.display()))
    };
    fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
    write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fx\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
    )?;
    write(root.join("src/lib.rs"), source)?;
    write(base.join("change.diff"), diff)?;
    let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
        .args(["check", "--mode", "draft", "--root"])
        .arg(&root)
        .arg("--diff")
        .arg(base.join("change.diff"))
        .args(["--format", "json"])
        .env("RIPR_CACHE_DIR", base.join("cache"))
        .output()
        .map_err(|e| format!("run ripr check: {e}"))?;
    let _ = fs::remove_dir_all(&base);
    if !output.status.success() {
        return Err(format!(
            "ripr check failed: {}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let json = serde_json::from_slice(&output.stdout).map_err(|e| format!("parse json: {e}"))?;
    Ok(Run { json })
}

#[test]
fn proofs_to_contracts_conversion_seeds_no_probe_and_is_disclosed() -> Result<(), String> {
    let run = check(NEW_SOURCE, CONVERSION_DIFF)?;
    assert!(run.finding_lines().is_empty(), "{:?}", run.json["findings"]);
    // The added nested harness also carries SPEC-0367's own disclosure, which
    // keeps the outcome partial; the point is zero production probes.
    assert_eq!(run.kind(), "partial_with_limitations", "{:?}", run.json);
    let kinds = run.limitation_kinds();
    assert!(kinds.contains(&"verification_only_change"), "{kinds:?}");
    assert_eq!(
        run.json["analysis_outcome"]["outcome"]["counts"]["probe_count"],
        0
    );
    Ok(())
}

#[test]
fn a_deleted_cfg_kani_proof_module_alone_is_verification_only() -> Result<(), String> {
    let run = check(
        DELETE_ONLY_SOURCE,
        &delete_only_diff("#[cfg(kani)]", "#[kani::proof]"),
    )?;
    assert!(run.finding_lines().is_empty(), "{:?}", run.json["findings"]);
    assert_eq!(run.limitation_kinds(), ["verification_only_change"]);
    Ok(())
}

#[test]
fn a_deleted_harness_in_an_ungated_module_is_still_disclosed() -> Result<(), String> {
    // The harness attribute alone makes the item verification source, even
    // without a `cfg(kani)` gate on its module.
    let run = check(
        DELETE_ONLY_SOURCE,
        &delete_only_diff("", "#[kani::proof_for_contract(round_down)]"),
    )?;
    assert!(run.finding_lines().is_empty(), "{:?}", run.json["findings"]);
    assert_eq!(run.limitation_kinds(), ["verification_only_change"]);
    Ok(())
}

#[test]
fn a_deleted_ungated_module_without_a_harness_attribute_stays_probed() -> Result<(), String> {
    // Fail closed: nothing marks this deleted code as verification-only.
    let run = check(DELETE_ONLY_SOURCE, &delete_only_diff("", "#[test]"))?;
    assert!(
        run.limitation_kinds()
            .iter()
            .all(|kind| *kind != "verification_only_change"),
        "{:?}",
        run.json["analysis_outcome"]
    );
    Ok(())
}

#[test]
fn a_gated_module_that_is_not_kani_stays_probed() -> Result<(), String> {
    let run = check(
        DELETE_ONLY_SOURCE,
        &delete_only_diff("#[cfg(not(kani))]", "#[test]"),
    )?;
    assert!(
        run.limitation_kinds()
            .iter()
            .all(|kind| *kind != "verification_only_change"),
        "{:?}",
        run.json["analysis_outcome"]
    );
    Ok(())
}

#[test]
fn a_production_edit_beside_a_deleted_harness_is_still_probed() -> Result<(), String> {
    // Same deletion, but the production line also changes: that line keeps
    // its finding; only the verification lines are disclosed.
    let source = "pub fn round_down(n: usize, align: usize) -> usize {
    n / align * align
}
";
    let diff = "diff --git a/src/lib.rs b/src/lib.rs
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,13 +1,3 @@
 pub fn round_down(n: usize, align: usize) -> usize {
-    n / align
+    n / align * align
 }
-
-#[cfg(kani)]
-mod proofs {
-    use super::*;
-
-    #[kani::proof]
-    fn prove() {
-        kani::assume(round_down(4, 2) == 4);
-    }
-}
";
    let run = check(source, diff)?;
    assert_eq!(run.finding_lines(), [2], "{:?}", run.json["findings"]);
    Ok(())
}
