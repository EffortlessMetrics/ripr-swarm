//! RIPR-SPEC-0347 (#8999): a diff that changes trybuild `tests/ui` fixtures
//! names them on stderr as not analyzed, and the machine-readable report is
//! byte-identical to the report for the same diff in a crate without trybuild.
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const TRYBUILD_MANIFEST: &str = "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dev-dependencies]\ntrybuild = \"1\"\n";
const PLAIN_MANIFEST: &str =
    "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2024\"\n";

const FIXTURE_DIFF: &str = "diff --git a/tests/ui/fail_len.rs b/tests/ui/fail_len.rs
new file mode 100644
--- /dev/null
+++ b/tests/ui/fail_len.rs
@@ -0,0 +1,3 @@
+fn main() {
+    let _: u8 = 300;
+}
diff --git a/tests/ui/fail_len.stderr b/tests/ui/fail_len.stderr
new file mode 100644
--- /dev/null
+++ b/tests/ui/fail_len.stderr
@@ -0,0 +1,2 @@
+error: literal out of range for `u8`
+ --> tests/ui/fail_len.rs:2:17
";

struct Scratch(PathBuf);

impl Scratch {
    fn create(label: &str, manifest: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-trybuild-disclosure-{label}-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
        std::fs::create_dir_all(root.join("tests/ui")).map_err(|error| error.to_string())?;
        std::fs::write(root.join("Cargo.toml"), manifest).map_err(|error| error.to_string())?;
        std::fs::write(
            root.join("src/lib.rs"),
            "pub fn id(x: u8) -> u8 {\n    x\n}\n",
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(
            root.join("tests/ui/fail_len.rs"),
            "fn main() {\n    let _: u8 = 300;\n}\n",
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(root.join("change.patch"), FIXTURE_DIFF)
            .map_err(|error| error.to_string())?;
        Ok(Self(root))
    }

    fn check(&self) -> Result<Output, String> {
        run_check(&self.0)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn run_check(root: &Path) -> Result<Output, String> {
    Command::new(env!("CARGO_BIN_EXE_ripr"))
        .args(["check", "--root"])
        .arg(root)
        .arg("--diff")
        .arg(root.join("change.patch"))
        .args(["--mode", "fast", "--format", "json"])
        .output()
        .map_err(|error| format!("spawn ripr check: {error}"))
}

#[test]
fn trybuild_fixture_diff_is_named_on_stderr_without_changing_the_report() -> Result<(), String> {
    let with_trybuild = Scratch::create("with", TRYBUILD_MANIFEST)?;
    let without_trybuild = Scratch::create("without", PLAIN_MANIFEST)?;
    let named = with_trybuild.check()?;
    let control = without_trybuild.check()?;
    let named_stderr = String::from_utf8_lossy(&named.stderr).into_owned();
    let control_stderr = String::from_utf8_lossy(&control.stderr).into_owned();
    assert!(
        named_stderr.contains(
            "2 changed trybuild fixture file(s) under tests/ui were not analyzed as test oracles"
        ) && named_stderr.contains("tests/ui/fail_len.rs, tests/ui/fail_len.stderr"),
        "{named_stderr}"
    );
    assert!(
        !control_stderr.contains("trybuild"),
        "a tests/ui diff in a crate without trybuild must not be named: {control_stderr}"
    );
    assert_eq!(named.status.code(), control.status.code());
    // The scratch root differs per run and is echoed in several fields;
    // everything else must match.
    let normalize = |output: &Output, scratch: &Scratch| -> String {
        String::from_utf8_lossy(&output.stdout)
            .replace(scratch.0.to_string_lossy().as_ref(), "<root>")
    };
    assert!(!normalize(&named, &with_trybuild).is_empty());
    assert_eq!(
        normalize(&named, &with_trybuild),
        normalize(&control, &without_trybuild),
        "the disclosure must not change the report bytes"
    );
    Ok(())
}

fn workspace_manifest(workspace_entry: &str, table: &str, member_entry: &str) -> String {
    format!(
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[workspace]\n\n[workspace.dependencies]\n{workspace_entry}\n\n[{table}]\n{member_entry}\n"
    )
}

#[test]
fn inherited_workspace_trybuild_never_names_and_direct_forms_still_do() -> Result<(), String> {
    let control = Scratch::create("inherit-control", PLAIN_MANIFEST)?;
    let control_output = control.check()?;
    let normalize = |output: &Output, scratch: &Scratch| {
        String::from_utf8_lossy(&output.stdout)
            .replace(scratch.0.to_string_lossy().as_ref(), "<root>")
    };
    let unrelated = "trybuild = { package = \"other_harness\", path = \"support/other\" }";
    let mut cases: Vec<(String, String, bool)> = Vec::new();
    for table in ["dev-dependencies", "target.'cfg(unix)'.dev-dependencies"] {
        for (workspace_entry, tag) in [("trybuild = \"1\"", "actual"), (unrelated, "unrelated")] {
            for (member_entry, member_tag) in [
                ("trybuild = { workspace = true }", "inherited"),
                (
                    "trybuild = { workspace = true, package = \"trybuild\" }",
                    "ignored-package",
                ),
            ] {
                cases.push((
                    format!("{table}-{tag}-{member_tag}"),
                    workspace_manifest(workspace_entry, table, member_entry),
                    false,
                ));
            }
        }
        cases.push((
            format!("{table}-direct"),
            workspace_manifest("other = \"1\"", table, "trybuild = \"1\""),
            true,
        ));
    }
    for (label, manifest, named) in cases {
        let scratch = Scratch::create(&label.replace(['\'', '(', ')', '.'], "_"), &manifest)?;
        let output = scratch.check()?;
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        assert_eq!(
            stderr.contains("changed trybuild fixture file(s)"),
            named,
            "{label}: {stderr}"
        );
        assert_eq!(
            output.status.code(),
            control_output.status.code(),
            "{label}"
        );
        assert_eq!(
            normalize(&output, &scratch),
            normalize(&control_output, &control),
            "{label}: report bytes must not change"
        );
    }
    // An explicit `[package].workspace` selector also stays silent.
    let selector = "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2024\"\nworkspace = \"../authority\"\n\n[dev-dependencies]\ntrybuild = { workspace = true }\n";
    let scratch = Scratch::create("selector", selector)?;
    let output = scratch.check()?;
    assert!(
        !String::from_utf8_lossy(&output.stderr).contains("trybuild fixture"),
        "package.workspace selector must not name fixtures"
    );
    Ok(())
}
