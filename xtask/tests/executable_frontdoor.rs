//! Both real front doors retain the same predecessor report snapshots.
use std::path::PathBuf;
use std::process::Command;

#[path = "../../tools/repo-policy/tests/common/executable_index.rs"]
mod executable_index;

#[path = "../../tools/repo-policy/tests/common/git_environment.rs"]
mod git_environment;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Result<Self, String> {
        let root = std::env::temp_dir().join(format!(
            "xtask-executable-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_nanos()
        ));
        let fixture = Self(root);
        std::fs::create_dir_all(fixture.0.join("policy")).map_err(|e| e.to_string())?;
        let mut git_calls = 0;
        executable_index::initialize(&fixture.0, |args| {
            git_calls += 1;
            fixture.git(args).map(|_| ())
        })?;
        assert_eq!(git_calls, 4, "executable-index setup subprocess budget");
        Ok(fixture)
    }
    fn git(&self, args: &[&str]) -> Result<String, String> {
        let mut command = Command::new("git");
        command.args(args).current_dir(&self.0);
        git_environment::isolate_git_environment(&mut command);
        let output = command.output().map_err(|e| e.to_string())?;
        if !output.status.success() {
            return Err(format!(
                "fixture git {args:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if let Err(e) = std::fs::remove_dir_all(&self.0) {
            eprintln!("retain fixture {}: {e}", self.0.display());
        }
    }
}

fn assert_executable_fixture_index(fixture: &Fixture) -> Result<(), String> {
    let index = fixture.git(&["ls-files", "--stage"])?;
    let rows = index.lines().collect::<Vec<_>>();
    assert_eq!(
        rows.len(),
        4,
        "only intended fixture files are staged: {index}"
    );
    for (name, mode) in [
        ("allowed", "100755"),
        ("demoted", "100755"),
        ("ordinary", "100644"),
        ("unregistered", "100755"),
    ] {
        let row = rows
            .iter()
            .find(|row| row.ends_with(&format!("\t{name}")))
            .ok_or_else(|| format!("missing {name}: {index}"))?;
        assert!(row.starts_with(&format!("{mode} ")), "wrong mode: {row}");
        assert_eq!(
            fixture.git(&["show", &format!(":{name}")])?,
            format!("{name}\n"),
            "index content for {name}"
        );
        assert_eq!(
            std::fs::read(fixture.0.join(name)).map_err(|e| e.to_string())?,
            format!("{name}\n").as_bytes(),
            "working content for {name}"
        );
    }
    Ok(())
}

#[test]
fn retained_xtask_frontdoor_matches_lightweight_policy_snapshots() -> Result<(), String> {
    let fixture = Fixture::new()?;
    assert_executable_fixture_index(&fixture)?;
    let allowlist = fixture.0.join("policy/executable_allowlist.txt");
    for (case, rows, expected) in [
        (
            "allowed",
            "allowed\ndemoted\nunregistered\n",
            include_str!("../../tools/repo-policy/tests/executable-files/allowed.md"),
        ),
        (
            "unregistered",
            "allowed\ndemoted\n",
            include_str!("../../tools/repo-policy/tests/executable-files/unregistered.md"),
        ),
        (
            "missing",
            "allowed\ndemoted\nunregistered\ndeleted\n",
            include_str!("../../tools/repo-policy/tests/executable-files/missing.md"),
        ),
        (
            "demoted",
            "allowed\ndemoted\nunregistered\n",
            include_str!("../../tools/repo-policy/tests/executable-files/demoted.md"),
        ),
        (
            "mixed_order",
            "z-missing\nallowed\ndemoted\na-missing\n",
            include_str!("../../tools/repo-policy/tests/executable-files/mixed_order.md"),
        ),
        (
            "missing_optional",
            "",
            include_str!("../../tools/repo-policy/tests/executable-files/missing_optional.md"),
        ),
    ] {
        if case == "demoted" {
            fixture.git(&["update-index", "--chmod=-x", "demoted"])?;
        }
        if case == "missing_optional" {
            std::fs::remove_file(&allowlist).map_err(|e| e.to_string())?;
            for name in ["allowed", "unregistered"] {
                fixture.git(&["update-index", "--chmod=-x", name])?;
            }
        } else {
            std::fs::write(&allowlist, rows).map_err(|e| e.to_string())?;
        }
        let mut command = Command::new(env!("CARGO_BIN_EXE_xtask"));
        command
            .arg("check-executable-files")
            .current_dir(&fixture.0);
        git_environment::isolate_git_environment(&mut command);
        let output = command.output().map_err(|e| e.to_string())?;
        assert_eq!(
            output.status.success(),
            matches!(case, "allowed" | "missing_optional"),
            "case {case}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            std::fs::read(fixture.0.join("target/ripr/reports/executable-files.md"))
                .map_err(|e| e.to_string())?,
            expected.as_bytes(),
            "case {case}"
        );
    }
    Ok(())
}
