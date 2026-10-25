//! A broken or missing `Cargo.toml` degrades path-dependency reach. The
//! report must say so with a reason and a next step instead of reading as
//! `analysis_complete` while only stderr warns (#10182).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

const LIB_RS: &str = "pub fn discount(price: u32, qty: u32) -> u32 {\n    if qty >= 10 { price * qty * 9 / 10 } else { price * qty }\n}\n";
const TEST_RS: &str =
    "use fx::discount;\n#[test]\nfn small() {\n    assert!(discount(5, 2) > 0);\n}\n";
const MANIFEST: &str = "[package]\nname = \"fx\"\nversion = \"0.0.0\"\nedition = \"2021\"\n";
const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn discount(price: u32, qty: u32) -> u32 {\n-    if qty >= 10 { price * qty * 9 / 10 } else { price * qty }\n+    if qty >= 12 { price * qty * 9 / 10 } else { price * qty }\n }\n";

struct Repo {
    base: PathBuf,
    root: PathBuf,
}

impl Repo {
    fn new(label: &str, manifest: Option<&str>) -> Result<Self, String> {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let base = std::env::temp_dir().join(format!(
            "ripr-broken-manifest-{label}-{}-{id}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&base);
        let root = base.join("fx");
        fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
        fs::create_dir_all(root.join("tests")).map_err(|e| e.to_string())?;
        if let Some(manifest) = manifest {
            write(&root.join("Cargo.toml"), manifest)?;
        }
        write(&root.join("src/lib.rs"), LIB_RS)?;
        write(&root.join("tests/discount.rs"), TEST_RS)?;
        write(&base.join("change.diff"), DIFF)?;
        Ok(Self { base, root })
    }

    fn check(&self, format: &str) -> Result<(String, String), String> {
        self.check_mode(format, "draft")
    }

    fn check_mode(&self, format: &str, mode: &str) -> Result<(String, String), String> {
        let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
            .args(["check", "--mode", mode, "--root"])
            .arg(&self.root)
            .arg("--diff")
            .arg(self.base.join("change.diff"))
            .args(["--format", format])
            .env("RIPR_CACHE_DIR", self.base.join("cache"))
            .output()
            .map_err(|e| format!("run ripr check: {e}"))?;
        if !output.status.success() {
            return Err(format!(
                "ripr check failed: {}\n{}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        Ok((
            String::from_utf8_lossy(&output.stdout).into_owned(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ))
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

fn write(path: &Path, text: &str) -> Result<(), String> {
    fs::write(path, text).map_err(|e| format!("write {}: {e}", path.display()))
}

fn outcome(json: &str) -> Result<serde_json::Value, String> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("report is not JSON: {e}\n{json}"))?;
    value
        .get("analysis_outcome")
        .cloned()
        .ok_or_else(|| format!("report has no analysis_outcome:\n{json}"))
}

fn path_dependency_limitation(outcome: &serde_json::Value) -> Option<&serde_json::Value> {
    outcome["outcome"]["limitations"]
        .as_array()?
        .iter()
        .find(|l| {
            l["bounded_detail"]
                .as_str()
                .is_some_and(|d| d.contains("path-dependency graph"))
        })
}

#[test]
fn broken_manifest_is_a_limitation_not_a_complete_analysis() -> Result<(), String> {
    let repo = Repo::new("broken", Some("not toml [[["))?;
    let (stdout, stderr) = repo.check("json")?;
    assert!(stderr.contains("path-dependency graph status is limited"));
    let outcome = outcome(&stdout)?;
    assert_eq!(outcome["analysis_complete"], false, "{outcome}");
    assert_eq!(outcome["outcome"]["kind"], "partial_with_limitations");
    assert!(
        outcome["outcome"]["counts"]["finding_count"]
            .as_u64()
            .is_some_and(|n| n > 0),
        "findings stay retained next to the limitation: {outcome}"
    );
    let limitation = path_dependency_limitation(&outcome)
        .ok_or_else(|| format!("no path-dependency limitation: {outcome}"))?;
    assert!(
        limitation["bounded_detail"]
            .as_str()
            .is_some_and(|d| d.contains("limited") && d.contains("may have missed")),
        "{limitation}"
    );
    assert_eq!(limitation["kind"], "producer_failure", "{limitation}");
    assert_eq!(limitation["producer_stage"], "language_adapter");
    assert_eq!(limitation["recovery"]["kind"], "inspect_failure");
    assert!(
        !limitation["bounded_detail"]
            .as_str()
            .unwrap_or_default()
            .contains("unavailable"),
        "a parse failure is limited, not unavailable: {limitation}"
    );
    assert!(
        limitation["recovery"]["detail"]
            .as_str()
            .is_some_and(|d| d.contains("Cargo.toml") && d.contains("re-run")),
        "recovery must name a next step: {limitation}"
    );
    Ok(())
}

#[test]
fn missing_manifest_is_a_limitation_with_unavailable_reason() -> Result<(), String> {
    let repo = Repo::new("missing", None)?;
    let (stdout, _) = repo.check("json")?;
    let outcome = outcome(&stdout)?;
    assert_eq!(outcome["analysis_complete"], false, "{outcome}");
    let limitation = path_dependency_limitation(&outcome)
        .ok_or_else(|| format!("no path-dependency limitation: {outcome}"))?;
    assert!(
        limitation["bounded_detail"]
            .as_str()
            .is_some_and(|d| d.contains("unavailable") && !d.contains("is limited")),
        "{limitation}"
    );
    Ok(())
}

#[test]
fn parsed_single_package_manifest_adds_no_limitation() -> Result<(), String> {
    let repo = Repo::new("clean", Some(MANIFEST))?;
    let (stdout, stderr) = repo.check("json")?;
    assert!(!stderr.contains("path-dependency graph status"), "{stderr}");
    let outcome = outcome(&stdout)?;
    assert!(path_dependency_limitation(&outcome).is_none(), "{outcome}");
    assert_eq!(outcome["analysis_complete"], true, "{outcome}");
    Ok(())
}

/// Hostile input: a manifest with NULs, invalid UTF-8 and a huge line must
/// still end in a typed, bounded limitation, never a crash or a complete run.
#[test]
fn hostile_manifest_bytes_still_yield_a_bounded_limitation() -> Result<(), String> {
    let repo = Repo::new("hostile", Some("placeholder"))?;
    let mut bytes = b"[package\nname = \"\xff\xfe\x00".to_vec();
    bytes.extend(std::iter::repeat_n(b'[', 200_000));
    fs::write(repo.root.join("Cargo.toml"), bytes).map_err(|e| e.to_string())?;
    let (stdout, stderr) = repo.check("json")?;
    assert!(stderr.contains("path-dependency graph status is limited"));
    let outcome = outcome(&stdout)?;
    assert_eq!(outcome["analysis_complete"], false, "{outcome}");
    let limitation = path_dependency_limitation(&outcome)
        .ok_or_else(|| format!("no path-dependency limitation: {outcome}"))?;
    let detail = limitation["bounded_detail"].as_str().unwrap_or_default();
    assert!(detail.chars().count() < 1_000, "detail must stay bounded");
    Ok(())
}

#[test]
fn human_report_does_not_read_as_plain_complete() -> Result<(), String> {
    let repo = Repo::new("human", Some("not toml [[["))?;
    let (stdout, _) = repo.check("human")?;
    assert!(
        !stdout.contains("(analysis complete)"),
        "human headline must not claim completeness:\n{stdout}"
    );
    assert!(
        stdout.contains("analysis incomplete")
            && stdout.contains("Fix the Cargo.toml that fails to parse"),
        "human report must name the limitation and its next step:\n{stdout}"
    );
    Ok(())
}

/// A Limited graph with no parse failure: every manifest parses, but a
/// package's `workspace = "..."` redirect is not followed. The limitation must
/// not claim a parse failure.
#[test]
fn unfollowed_workspace_redirect_is_limited_without_a_parse_claim() -> Result<(), String> {
    let repo = Repo::new("redirect", Some(MANIFEST))?;
    let sub = repo.root.join("sub");
    let other = repo.root.join("other-root");
    fs::create_dir_all(&sub).map_err(|e| e.to_string())?;
    fs::create_dir_all(&other).map_err(|e| e.to_string())?;
    write(
        &other.join("Cargo.toml"),
        "[workspace]\nmembers = []\n\n[workspace.dependencies]\nshared = { path = \"other-shared\" }\n",
    )?;
    write(
        &sub.join("Cargo.toml"),
        "[package]\nname = \"sub\"\nversion = \"0.1.0\"\nworkspace = \"../other-root\"\n\n[dependencies]\nshared = { workspace = true }\n",
    )?;
    let (stdout, _) = repo.check("json")?;
    let outcome = outcome(&stdout)?;
    let limitation = path_dependency_limitation(&outcome)
        .ok_or_else(|| format!("no path-dependency limitation: {outcome}"))?;
    let detail = limitation["bounded_detail"].as_str().unwrap_or_default();
    assert!(detail.contains("parsed or followed"), "{detail}");
    assert!(
        limitation["recovery"]["detail"]
            .as_str()
            .is_some_and(|d| d.contains("redirect")),
        "recovery must cover a redirect, not only a parse failure: {limitation}"
    );
    Ok(())
}

/// Instant mode never runs the expansion, so a broken manifest adds nothing
/// there (documented contract; the limitation follows the expansion).
#[test]
fn instant_mode_adds_no_path_dependency_limitation() -> Result<(), String> {
    let repo = Repo::new("instant", Some("not toml [[["))?;
    let (stdout, _) = repo.check_mode("json", "instant")?;
    let outcome = outcome(&stdout)?;
    assert!(path_dependency_limitation(&outcome).is_none(), "{outcome}");
    Ok(())
}
