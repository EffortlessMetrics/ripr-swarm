//! Exercise the public dispatch boundary with the built binary in another workspace.
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[path = "common/xtask_bin.rs"]
mod xtask_bin;

struct Fixture(PathBuf);

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn invoke(cwd: &Path, gate: &str) -> Result<std::process::Output, String> {
    xtask_bin::output(|command| {
        command.current_dir(cwd).arg(gate);
    })
}

#[test]
fn public_gates_refuse_nested_calls_before_touching_the_workspace()
-> Result<(), Box<dyn std::error::Error>> {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let root = std::env::temp_dir().join(format!("ripr-gates-{}-{nonce}", std::process::id()));
    fs::create_dir(&root)?;
    let fixture = Fixture(root.canonicalize()?);
    let root = &fixture.0;
    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/ripr\", \"xtask\"]\nresolver = \"2\"\n",
    )?;
    let unformatted = "pub fn gate_probe( ) { }\n";
    for (dir, name) in [("crates/ripr", "ripr"), ("xtask", "xtask")] {
        let member = root.join(dir);
        fs::create_dir_all(member.join("src"))?;
        fs::write(
            member.join("Cargo.toml"),
            format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n"),
        )?;
        fs::write(member.join("src/lib.rs"), unformatted)?;
    }
    fs::create_dir(root.join("docs"))?;
    for subdir in ["xtask", "docs"] {
        let cwd = root.join(subdir);
        let allowlist = cwd.join("policy/probe.txt");
        fs::create_dir(cwd.join("policy"))?;
        fs::write(&allowlist, "zeta\nalpha\n")?;
        for gate in ["check-fast", "ci-fast", "precommit", "shape"] {
            let output = invoke(&cwd, gate)?;
            let stderr = String::from_utf8(output.stderr)?;
            let expected = format!(
                "run from the RIPR workspace root {} (current directory: {})",
                root.display(),
                cwd.display()
            );
            assert_eq!(output.status.code(), Some(1), "{gate}: {stderr}");
            assert!(stderr.contains(&expected), "{gate}: {stderr}");
            assert!(output.stdout.is_empty(), "{gate} emitted gate output");
            assert!(
                !stderr.lines().any(|line| line.starts_with("$ ")),
                "{gate} started a gate command: {stderr}"
            );
            assert!(
                !cwd.join("target/ripr/reports").exists(),
                "{gate} wrote a report directory"
            );
            assert!(
                !root.join("target/ripr/reports").exists(),
                "{gate} wrote reports in another directory"
            );
            assert_eq!(
                fs::read_to_string(root.join("xtask/src/lib.rs"))?,
                unformatted
            );
            assert_eq!(fs::read_to_string(&allowlist)?, "zeta\nalpha\n");
        }
    }

    // The same binary must accept this workspace's root and reach the real
    // formatting gate. Its intentionally unformatted input keeps the run bounded.
    let output = invoke(root, "ci-fast")?;
    assert!(!output.status.success());
    assert!(String::from_utf8(output.stdout)?.contains("gate_probe"));

    // A foreign Cargo workspace with the same directory layout is not RIPR.
    fs::write(
        root.join("crates/ripr/Cargo.toml"),
        "[package]\nname = \"other\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )?;
    let output = invoke(root, "shape")?;
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8(output.stderr)?.contains("is not the RIPR workspace"));
    assert_eq!(
        fs::read_to_string(root.join("xtask/src/lib.rs"))?,
        unformatted
    );
    Ok(())
}
