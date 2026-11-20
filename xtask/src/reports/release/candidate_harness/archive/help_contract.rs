//! The rendered-help contract, run against the installed qualified candidate.
//!
//! `crates/ripr/tests/cli_help_hierarchy.rs` and its two helpers are
//! development tests, so they stay out of the published source package
//! (#15216). Qualification instead compiles the admitted commit's copy of that
//! harness with `CARGO_BIN_EXE_ripr` bound to the exact executable installed
//! from the qualified archive, and requires every declared case to run and
//! pass. Cargo is not involved: the harness uses only `std`.
use super::{InstalledCandidate, qualified_command_with_environment};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::time::Duration;

/// The harness entry point and its helpers, relative to the package root.
pub(crate) const HELP_HARNESS_ENTRY: &str = "tests/cli_help_hierarchy.rs";
pub(crate) const HELP_HARNESS_FILES: [&str; 3] = [
    HELP_HARNESS_ENTRY,
    "tests/common/help_contract.rs",
    "tests/common/help_workspace.rs",
];
/// The only binding the harness may use to find the executable under test.
const BINARY_BINDING: &str = "env!(\"CARGO_BIN_EXE_ripr\")";

impl InstalledCandidate {
    /// Compile the admitted harness against this candidate and run every case.
    /// `owned_root` must not exist yet; it holds the harness and its scratch.
    pub(crate) fn run_help_contract(&self, owned_root: &Path) -> Result<serde_json::Value, String> {
        self.revalidate()?;
        let source = &self.archive.source;
        let mut files = Vec::new();
        for relative in HELP_HARNESS_FILES {
            let committed = format!("{}{relative}", source.package_prefix());
            let bytes = source
                .committed_file(&committed)
                .ok_or_else(|| format!("admitted source lacks help harness {committed}"))?;
            files.push((relative, committed, bytes.to_vec()));
        }
        let ((_, _, entry), helpers) = files
            .split_first()
            .ok_or_else(|| "help harness has no entry point".to_string())?;
        let entry = std::str::from_utf8(entry)
            .map_err(|error| format!("help harness is not UTF-8: {error}"))?;
        let declared = declared_cases(entry)?;
        for (_, committed, bytes) in helpers {
            let helper = String::from_utf8_lossy(bytes);
            if ["CARGO_BIN_EXE", "option_env!", "env::var", "use std::env"]
                .iter()
                .any(|needle| helper.contains(needle))
            {
                return Err(format!(
                    "help harness helper {committed} locates an executable"
                ));
            }
        }
        let binary = self
            .binary()
            .to_str()
            .ok_or_else(|| "installed candidate path is not UTF-8".to_string())?
            .to_string();
        let toolchain = &self.producer.toolchain;
        let compiled = compile_harness(
            owned_root,
            &files
                .iter()
                .map(|(relative, _, bytes)| (*relative, bytes.as_slice()))
                .collect::<Vec<_>>(),
            Path::new(&toolchain.rustc_command),
            &toolchain.environment(),
            &binary,
        )?;
        self.revalidate()?;
        let outcome = run_harness(owned_root, &compiled, declared)?;
        self.revalidate()?;
        Ok(serde_json::json!({
            "contract": "every case of the admitted rendered-help harness passes against the installed candidate executable",
            "harness_sources": files
                .iter()
                .map(|(_, committed, bytes)| serde_json::json!({
                    "path": committed,
                    "sha256": format!("{:x}", Sha256::digest(bytes)),
                }))
                .collect::<Vec<_>>(),
            "binary_binding": "CARGO_BIN_EXE_ripr",
            "executable": binary,
            "executable_sha256": format!("{:x}", Sha256::digest(&self.executable_bytes)),
            "rustc_verbose_version": toolchain.rustc_verbose_version,
            "cases_declared": declared,
            "cases_listed": outcome.listed,
            "cases_passed": outcome.passed,
        }))
    }
}

/// The number of `#[test]` cases the harness declares, and a check that it
/// finds the executable only through `CARGO_BIN_EXE_ripr`.
pub(crate) fn declared_cases(entry: &str) -> Result<usize, String> {
    // Comment lines are ignored, so a commented-out binding does not count.
    let code = entry
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    if code.matches(BINARY_BINDING).count() != 1
        || entry.contains("option_env!")
        || entry.contains("env::var")
        || entry.contains("use std::env")
    {
        return Err(format!(
            "help harness must locate ripr only through one {BINARY_BINDING}"
        ));
    }
    let declared = entry
        .lines()
        .filter(|line| line.trim() == "#[test]")
        .count();
    if declared == 0 {
        return Err("help harness declares no test cases".to_string());
    }
    Ok(declared)
}

/// Write the harness under `owned_root/src` and compile it as a libtest
/// executable whose `CARGO_BIN_EXE_ripr` is `binary`.
pub(crate) fn compile_harness(
    owned_root: &Path,
    files: &[(&str, &[u8])],
    rustc: &Path,
    toolchain_environment: &[(&str, &str)],
    binary: &str,
) -> Result<std::path::PathBuf, String> {
    std::fs::create_dir(owned_root)
        .map_err(|error| format!("create owned help harness root: {error}"))?;
    let sources = owned_root.join("src");
    for (relative, bytes) in files {
        let path = sources.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("create help harness source directory: {error}"))?;
        }
        std::fs::write(&path, bytes)
            .map_err(|error| format!("write help harness source {relative}: {error}"))?;
    }
    let executable = owned_root.join(format!("help-contract{}", std::env::consts::EXE_SUFFIX));
    let args = vec![
        "--edition=2024".to_string(),
        "--test".to_string(),
        "-C".to_string(),
        "debuginfo=0".to_string(),
        HELP_HARNESS_ENTRY.to_string(),
        "-o".to_string(),
        executable.to_string_lossy().into_owned(),
    ];
    let mut environment = toolchain_environment.to_vec();
    environment.push(("CARGO_BIN_EXE_ripr", binary));
    environment.push(("RUSTUP_AUTO_INSTALL", "0"));
    qualified_command_with_environment(
        rustc,
        &args,
        &sources,
        Duration::from_mins(5),
        "compile help harness",
        &environment,
    )?;
    Ok(executable)
}

pub(crate) struct HarnessOutcome {
    pub(crate) listed: usize,
    pub(crate) passed: usize,
}

/// List and run the compiled harness. Every declared case must be listed and
/// pass; a failed, ignored or filtered case refuses the candidate.
pub(crate) fn run_harness(
    owned_root: &Path,
    executable: &Path,
    declared: usize,
) -> Result<HarnessOutcome, String> {
    let scratch = owned_root.join("scratch");
    std::fs::create_dir(&scratch)
        .map_err(|error| format!("create help harness scratch: {error}"))?;
    let scratch_text = scratch
        .to_str()
        .ok_or_else(|| "help harness scratch path is not UTF-8".to_string())?;
    // The harness creates its workspaces under the temporary directory.
    let environment = [
        ("TMPDIR", scratch_text),
        ("TEMP", scratch_text),
        ("TMP", scratch_text),
    ];
    let list = harness_command(executable, &["--list"], &scratch, &environment)?;
    if !list.status.is_some_and(|status| status.success()) {
        return Err(format!(
            "help harness inventory failed: {}",
            String::from_utf8_lossy(&list.stderr)
        ));
    }
    let listed = String::from_utf8_lossy(&list.stdout)
        .lines()
        .filter(|line| line.ends_with(": test"))
        .count();
    if listed != declared {
        return Err(format!(
            "help harness lists {listed} cases but declares {declared}"
        ));
    }
    let run = harness_command(executable, &["--test-threads=1"], &scratch, &environment)?;
    let stdout = String::from_utf8_lossy(&run.stdout);
    let summary = format!(
        "test result: ok. {declared} passed; 0 failed; 0 ignored; 0 measured; 0 filtered out"
    );
    if !run.status.is_some_and(|status| status.success()) || !stdout.contains(&summary) {
        return Err(format!(
            "help harness did not pass all {declared} cases against the installed candidate:\n{}\n{}",
            tail(&stdout),
            tail(&String::from_utf8_lossy(&run.stderr))
        ));
    }
    std::fs::remove_dir_all(&scratch)
        .map_err(|error| format!("remove help harness scratch: {error}"))?;
    Ok(HarnessOutcome {
        listed,
        passed: declared,
    })
}

fn harness_command(
    executable: &Path,
    args: &[&str],
    cwd: &Path,
    environment: &[(&str, &str)],
) -> Result<crate::run::TimedBytesOutput, String> {
    let args = args
        .iter()
        .map(|arg| (*arg).to_string())
        .collect::<Vec<_>>();
    let output = crate::run::capture_bytes_in_dir_with_timeout(
        executable,
        &args,
        cwd,
        environment,
        &[
            "CARGO_TARGET_DIR",
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_INDEX_FILE",
        ],
        Duration::from_mins(10),
        "help harness",
    )?;
    if output.timed_out {
        return Err("help harness exceeded its deadline".to_string());
    }
    Ok(output)
}

fn tail(text: &str) -> String {
    let lines = text.lines().collect::<Vec<_>>();
    lines[lines.len().saturating_sub(40)..].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    const PASSING: &str = "\
#[path = \"common/help_contract.rs\"]
mod help_contract;
fn ripr() -> &'static str { env!(\"CARGO_BIN_EXE_ripr\") }
#[test]
fn names_the_candidate() { assert!(help_contract::named(ripr())); }
#[test]
fn second_case() { assert!(!ripr().is_empty()); }
";
    const HELPER: &str = "pub fn named(path: &str) -> bool { path.ends_with(\"candidate\") }\n";

    fn harness(root: &Path, entry: &str, binary: &str) -> Result<HarnessOutcome, String> {
        let declared = declared_cases(entry)?;
        let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
        let executable = compile_harness(
            root,
            &[
                (HELP_HARNESS_ENTRY, entry.as_bytes()),
                ("tests/common/help_contract.rs", HELPER.as_bytes()),
            ],
            Path::new(&rustc),
            &[],
            binary,
        )?;
        run_harness(root, &executable, declared)
    }

    fn owned(label: &str) -> std::path::PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        std::env::temp_dir().join(format!(
            "ripr-help-contract-{label}-{}-{stamp}",
            std::process::id()
        ))
    }

    #[test]
    fn help_contract_runs_every_declared_case_against_the_bound_binary() -> Result<(), String> {
        let root = owned("pass");
        let result = harness(&root, PASSING, "/qualified/candidate");
        let _ = std::fs::remove_dir_all(&root);
        let outcome = result?;
        assert_eq!((outcome.listed, outcome.passed), (2, 2));

        // The binding is compiled in: a different executable path fails the
        // same harness, so a pass names the candidate it was bound to.
        let root = owned("other-binary");
        let result = harness(&root, PASSING, "/checkout/target/debug/ripr");
        let _ = std::fs::remove_dir_all(&root);
        let error = result
            .err()
            .ok_or("a harness bound to another binary passed")?;
        assert!(error.contains("did not pass all 2 cases"), "{error}");
        assert!(error.contains("names_the_candidate"), "{error}");
        Ok(())
    }

    #[test]
    fn help_contract_refuses_ignored_cases_and_other_bindings() -> Result<(), String> {
        let ignored = PASSING.replace(
            "#[test]\nfn second_case",
            "#[test]\n#[ignore]\nfn second_case",
        );
        let root = owned("ignored");
        let result = harness(&root, &ignored, "/qualified/candidate");
        let _ = std::fs::remove_dir_all(&root);
        let error = result.err().ok_or("an ignored case passed the contract")?;
        assert!(error.contains("did not pass all 2 cases"), "{error}");

        // A case the harness compiles out is declared but never listed.
        let gated = PASSING.replace(
            "#[test]\nfn second_case",
            "#[test]\n#[cfg(any())]\nfn second_case",
        );
        let root = owned("gated");
        let result = harness(&root, &gated, "/qualified/candidate");
        let _ = std::fs::remove_dir_all(&root);
        let error = result
            .err()
            .ok_or("a compiled-out case passed the contract")?;
        assert!(error.contains("lists 1 cases but declares 2"), "{error}");

        for entry in [
            PASSING.replace(
                "env!(\"CARGO_BIN_EXE_ripr\")",
                "option_env!(\"RIPR\").unwrap_or(\"x\")",
            ),
            PASSING.replace(
                "fn second_case() {",
                "fn second_case() { let _ = std::env::var(\"RIPR\");",
            ),
            PASSING.replace("#[test]", "#[tokio::test]"),
            PASSING.replace(
                "fn ripr() -> &'static str { env!(\"CARGO_BIN_EXE_ripr\") }",
                "// env!(\"CARGO_BIN_EXE_ripr\")\nfn ripr() -> &'static str { \"ripr\" }",
            ),
        ] {
            assert!(declared_cases(&entry).is_err(), "accepted: {entry}");
        }
        Ok(())
    }
}
