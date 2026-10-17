//! Exercise the retained workflow block: command status and package cwd are
//! independent literal oracles. No real Cargo build or publication is invoked.
#![cfg(target_os = "linux")]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const WORKFLOW: &str = include_str!("../../../.github/workflows/ci.yml");
type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

fn production_shell() -> TestResult<String> {
    let marker = "      - name: Unlocked install resolution (release surface)\n";
    assert_eq!(WORKFLOW.matches(marker).count(), 1);
    let (_, step) = WORKFLOW.split_once(marker).ok_or("production step")?;
    let (step, _) = step.split_once("      - name:").ok_or("next step")?;
    let (_, body) = step.split_once("        run: |\n").ok_or("shell block")?;
    let script = body
        .lines()
        .map(|line| line.strip_prefix("          ").ok_or("shell indentation"))
        .collect::<Result<Vec<_>, _>>()?
        .join("\n");
    assert!(!script.trim().is_empty());
    Ok(script)
}

struct Fixture {
    root: PathBuf,
    package: PathBuf,
}

impl Fixture {
    fn new(case: &str) -> TestResult<Self> {
        let root = std::env::temp_dir().join(format!(
            "ripr-unlocked-status-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(std::env::temp_dir())?;
        fs::create_dir(&root)?;
        let package = root.join("runner temp/ripr-0.11.0");
        let fixture = Self { root, package };
        fs::create_dir(fixture.root.join("checkout"))?;
        // A real source checkout already has a lockfile. Without this, a
        // swallowed cd failure could coincidentally fail at the next rm and
        // satisfy the expected exit without enforcing the cwd boundary.
        fs::write(fixture.root.join("checkout/Cargo.lock"), "source lock\n")?;
        fs::create_dir(fixture.root.join("bin"))?;
        fs::create_dir_all(fixture.package.parent().ok_or("package parent")?)?;
        if case != "missing_package" {
            fs::create_dir(&fixture.package)?;
            if case == "lock_failure" {
                fs::create_dir(fixture.package.join("Cargo.lock"))?;
            } else {
                fs::write(fixture.package.join("Cargo.lock"), "original lock\n")?;
            }
        }
        if case == "unavailable" {
            fixture.executable(
                "cargo",
                &format!("#!{}\n", fixture.root.join("missing-interpreter").display()),
            )?;
        } else {
            fixture.executable(
                "cargo",
                r#"#!/bin/bash
printf '%s|%s|%s\n' "$1" "$PWD" "$*" >> "$TRACE"
case "$1" in
  package)
    [ "$CASE" != package_failure ] || exit 17
    if [ "$CASE" = timeout ]; then exec /bin/sleep 30; fi
    ;;
  pkgid)
    [ "$CASE" != identity_failure ] || exit 19
    [ "$CASE" != empty_identity ] || exit 0
    printf '%s\n' 'path+file:///fixture#0.11.0'
    ;;
  generate-lockfile)
    [ "$CASE" != resolution_failure ] || exit 29
    [ ! -e Cargo.lock ] || exit 61
    printf '%s\n' 'regenerated lock' > Cargo.lock
    ;;
  check) [ "$CASE" != check_failure ] || exit 37 ;;
  *) exit 63 ;;
esac
"#,
            )?;
        }
        fixture.executable(
            "tar",
            r#"#!/bin/bash
printf 'tar|%s|%s\n' "$PWD" "$*" >> "$TRACE"
[ "$CASE" != extraction_failure ] || exit 23
"#,
        )?;
        Ok(fixture)
    }

    fn executable(&self, name: &str, source: &str) -> TestResult {
        let path = self.root.join("bin").join(name);
        fs::write(&path, source)?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
        Ok(())
    }

    fn run(&self, script: &str, case: &str, pipefail: bool) -> TestResult<Output> {
        // GNU timeout bounds the shell and its stub children together. This is
        // the same Linux test prerequisite as rust_only_routing.rs.
        let mut command = Command::new("/usr/bin/timeout");
        command.args([
            "--kill-after=1s",
            if case == "timeout" { "1s" } else { "10s" },
            "/bin/bash",
            "--noprofile",
            "--norc",
            "-e",
        ]);
        if pipefail {
            command.args(["-o", "pipefail"]);
        }
        Ok(command
            .args(["-c", script])
            .current_dir(self.root.join("checkout"))
            .env_clear()
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", self.root.join("bin").display()),
            )
            .env(
                "RUNNER_TEMP",
                self.package.parent().ok_or("package parent")?,
            )
            .env("CASE", case)
            .env("TRACE", self.root.join("trace"))
            .env("LC_ALL", "C")
            .output()?)
    }

    fn trace(&self) -> TestResult<Vec<String>> {
        let path = self.root.join("trace");
        if !path.exists() {
            return Ok(Vec::new());
        }
        Ok(fs::read_to_string(path)?
            .lines()
            .map(str::to_owned)
            .collect())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn unlocked_success_resolves_identity_once_and_checks_only_regenerated_package() -> TestResult {
    let script = production_shell()?;
    for pipefail in [false, true] {
        let fixture = Fixture::new("success")?;
        let output = fixture.run(&script, "success", pipefail)?;
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        let trace = fixture.trace()?;
        let commands: Vec<_> = trace.iter().map(|line| line.split('|').next()).collect();
        assert_eq!(
            commands,
            [
                Some("package"),
                Some("pkgid"),
                Some("tar"),
                Some("generate-lockfile"),
                Some("check")
            ]
        );
        let checkout = fixture.root.join("checkout");
        assert_eq!(
            trace[0],
            format!("package|{}|package -p ripr --no-verify", checkout.display())
        );
        assert_eq!(
            trace[1],
            format!("pkgid|{}|pkgid -p ripr", checkout.display())
        );
        assert_eq!(
            trace[2],
            format!(
                "tar|{}|xzf target/package/ripr-0.11.0.crate -C {}",
                checkout.display(),
                fixture.package.parent().ok_or("package parent")?.display()
            )
        );
        assert_eq!(
            trace[3],
            format!(
                "generate-lockfile|{}|generate-lockfile",
                fixture.package.display()
            )
        );
        assert_eq!(
            trace[4],
            format!(
                "check|{}|check --target-dir {}/unlocked-target",
                fixture.package.display(),
                fixture.package.parent().ok_or("package parent")?.display()
            )
        );
        assert_eq!(
            fs::read_to_string(fixture.package.join("Cargo.lock"))?,
            "regenerated lock\n"
        );
        assert_eq!(
            fs::read_to_string(fixture.root.join("checkout/Cargo.lock"))?,
            "source lock\n"
        );
    }
    Ok(())
}

#[test]
fn unlocked_prerequisite_failure_unavailable_and_timeout_never_reach_later_commands() -> TestResult
{
    let script = production_shell()?;
    for pipefail in [false, true] {
        for (case, exit, expected) in [
            ("package_failure", 17, vec!["package"]),
            ("identity_failure", 19, vec!["package", "pkgid"]),
            ("empty_identity", 1, vec!["package", "pkgid"]),
            ("extraction_failure", 23, vec!["package", "pkgid", "tar"]),
            ("missing_package", 1, vec!["package", "pkgid", "tar"]),
            ("lock_failure", 1, vec!["package", "pkgid", "tar"]),
            (
                "resolution_failure",
                29,
                vec!["package", "pkgid", "tar", "generate-lockfile"],
            ),
            (
                "check_failure",
                37,
                vec!["package", "pkgid", "tar", "generate-lockfile", "check"],
            ),
            ("unavailable", 127, vec![]),
            ("timeout", 124, vec!["package"]),
        ] {
            let fixture = Fixture::new(case)?;
            let output = fixture.run(&script, case, pipefail)?;
            let trace = fixture.trace()?;
            assert_eq!(
                output.status.code(),
                Some(exit),
                "case={case}, pipefail={pipefail}, trace={trace:?}, {output:?}"
            );
            let commands: Vec<_> = trace
                .iter()
                .filter_map(|line| line.split('|').next())
                .collect();
            assert_eq!(commands, expected, "case={case}, pipefail={pipefail}");
        }
    }
    Ok(())
}
