//! #13037: a finding with a strong exact assertion whose observation is
//! unconfirmed never contradicts itself.
//!
//! Regression pin for the agent-loop defect where `ripr check` reported
//! "No strong assertion was detected" in the missing-check line while the
//! same finding's Next step said the related test "already has an exact
//! assertion". The precise gap is the observation link (no assertion text
//! names the changed expression), not the assertion; the verdict stays
//! `weakly_exposed` — this test also pins that non-promotion as a control.

use ripr::domain::RelatedTestMiss;
use ripr::domain::{OracleKind, OracleStrength};
use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const LIB: &str =
    "pub fn zero_fee(x: u32) -> u32 {\n    if x >= 5 {\n        return 0;\n    }\n    7\n}\n";

const TESTS: &str = "use app::zero_fee;\n\n#[test]\nfn fees() {\n    assert_eq!(zero_fee(5), 0);\n    assert_eq!(zero_fee(1), 7);\n}\n";

const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1 +1,6 @@
-pub fn placeholder() {}
+pub fn zero_fee(x: u32) -> u32 {
+    if x >= 5 {
+        return 0;
+    }
+    7
+}
";

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create() -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-observation-unconfirmed-wording-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let this = Self { root };
        std::fs::create_dir_all(this.root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            this.root.join("Cargo.toml"),
            "[package]\nname = \"app\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(this.root.join("src/lib.rs"), LIB)
            .map_err(|error| format!("write lib.rs failed: {error}"))?;
        std::fs::create_dir_all(this.root.join("tests"))
            .map_err(|error| format!("create tests directory failed: {error}"))?;
        std::fs::write(this.root.join("tests/zero_fee.rs"), TESTS)
            .map_err(|error| format!("write test failed: {error}"))?;
        std::fs::write(this.root.join("diff.patch"), DIFF)
            .map_err(|error| format!("write diff failed: {error}"))?;
        Ok(this)
    }

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
        let mut probes = output.findings.into_iter().filter(|finding| {
            finding.probe.family == ProbeFamily::ReturnValue
                && finding.probe.expression.contains("return 0")
        });
        match (probes.next(), probes.next()) {
            (Some(finding), None) => Ok(finding),
            other => Err(format!(
                "premise: exactly one `return 0` return-value finding, got {other:?}"
            )),
        }
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn unconfirmed_exact_assertion_finding_never_contradicts_itself() -> Result<(), String> {
    let repo = TempRepo::create()?;
    let finding = repo.finding()?;

    // Should-stay control (#13037 non-goal): renaming the gap never promotes
    // the verdict; the observation link is still unconfirmed.
    if finding.class != ExposureClass::WeaklyExposed {
        return Err(format!(
            "premise: the finding stays weakly_exposed, got {:?}",
            finding.class
        ));
    }

    // The repro shape: a contributing strong exact row whose observation is
    // unconfirmed (no assertion text names the changed expression).
    let unconfirmed_exact = finding.related_tests.iter().any(|test| {
        test.miss == Some(RelatedTestMiss::ObservationUnconfirmed)
            && test.oracle_strength == OracleStrength::Strong
            && test.oracle_kind == OracleKind::ExactValue
    });
    if !unconfirmed_exact {
        return Err(format!(
            "premise: a strong exact observation-unconfirmed row, got {:?}",
            finding.related_tests
        ));
    }

    // The missing-check line names the actual gap instead of claiming no
    // strong assertion exists.
    if finding
        .missing
        .iter()
        .any(|line| line.contains("No strong discriminator was detected"))
    {
        return Err(format!(
            "missing-check line still claims no strong assertion: {:?}",
            finding.missing
        ));
    }
    if !finding
        .missing
        .iter()
        .any(|line| line.contains("observation of the changed returned value is unconfirmed"))
    {
        return Err(format!(
            "missing-check line does not name the unconfirmed observation link: {:?}",
            finding.missing
        ));
    }

    // One story: the Next step extends the missing-check line; it never
    // prescribes the assertion the related test already has.
    let step = finding.recommended_next_step.as_deref().unwrap_or_default();
    if !step.contains("already has an exact assertion") {
        return Err(format!(
            "Next step lost the exact-assertion context: {step}"
        ));
    }
    if step.contains("add a separating input and assert the exact returned value") {
        return Err(format!(
            "Next step prescribes the assertion the related test already has: {step}"
        ));
    }
    if !step.contains("what remains unconfirmed is the observation link") {
        return Err(format!(
            "Next step does not name the unconfirmed link: {step}"
        ));
    }

    // The #13037 invariant across every finding: "no strong assertion" and
    // "already has an exact assertion" never co-occur in one finding.
    let output = check_workspace(CheckInput {
        root: repo.root.clone(),
        base: None,
        diff_file: Some(repo.root.join("diff.patch")),
        mode: Mode::Ready,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    for finding in &output.findings {
        let claims_none = finding
            .missing
            .iter()
            .any(|line| line.contains("No strong discriminator was detected"));
        let step = finding.recommended_next_step.as_deref().unwrap_or_default();
        let claims_exact = step.contains("already has an exact assertion")
            || step.contains("already asserts an exact value");
        if claims_none && claims_exact {
            return Err(format!(
                "finding {} contradicts itself: {:?} vs {step}",
                finding.id, finding.missing
            ));
        }
    }
    Ok(())
}
