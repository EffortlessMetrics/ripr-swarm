use super::{DeclaredHunkScratch, run_command_with_env, unique_external_workspace, workspace_root};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

fn project(source_name: &str) -> TestResult<(PathBuf, DeclaredHunkScratch)> {
    let fixture = workspace_root().join("fixtures/boundary_gap");
    let root = unique_external_workspace("check-output-paths")?;
    let cleanup = DeclaredHunkScratch(root.clone());
    fs::create_dir_all(root.join("src"))?;
    fs::create_dir_all(root.join("tests"))?;
    fs::create_dir_all(root.join("py"))?;
    fs::copy(
        fixture.join("input/src/lib.rs"),
        root.join("src").join(source_name),
    )?;
    fs::copy(
        fixture.join("input/tests/pricing.rs"),
        root.join("tests/pricing.rs"),
    )?;
    for (input, output) in [
        ("input/Cargo.toml", "Cargo.toml"),
        ("diff.patch", "change.diff"),
    ] {
        let text = fs::read_to_string(fixture.join(input))?
            .replace("src/lib.rs", &format!("src/{source_name}"));
        fs::write(root.join(output), text)?;
    }
    Ok((root, cleanup))
}

fn run(cwd: &Path, root: &str, args: &[&str]) -> TestResult<String> {
    let mut args = args.to_vec();
    args.extend(["--root", root, "--diff", "../change.diff", "--mode", "fast"]);
    let output = run_command_with_env(
        env!("CARGO_BIN_EXE_ripr"),
        cwd,
        &args,
        &[("RIPR_CACHE_DIR", "")],
    )?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?)
}

#[test]
fn relative_root_human_file_opens_from_subdirectory() -> TestResult {
    for (name, encoded) in [
        ("lib.rs", "lib.rs"),
        ("price%tax.rs", "price%25tax.rs"),
        ("price%0Atax.rs", "price%250Atax.rs"),
    ] {
        let (root, _cleanup) = project(name)?;
        let cwd = root.join("py");
        let report = run(&cwd, "..", &["check", "--format", "human"])?;
        let location = report
            .lines()
            .find_map(|line| line.trim_start().strip_prefix("File: "))
            .ok_or_else(|| format!("missing File row:\n{report}"))?;
        let (path, line) = location.rsplit_once(':').ok_or("missing finding line")?;
        assert_eq!(line, "2");
        assert_eq!(path, format!("../src/{name}"));
        assert_eq!(
            fs::canonicalize(cwd.join(path))?,
            fs::canonicalize(root.join("src").join(name))?
        );
        let json: Value = serde_json::from_str(&run(&cwd, "..", &["check", "--format", "json"])?)?;
        assert_eq!(
            json["findings"][0]["probe"]["file"],
            format!("./src/{encoded}")
        );
        let selector = format!("src/{encoded}:2");
        let context: Value =
            serde_json::from_str(&run(&cwd, "..", &["context", "--at", &selector])?)?;
        assert_eq!(context["probe"]["id"], json["findings"][0]["probe"]["id"]);
    }
    Ok(())
}

#[test]
fn agent_gap_and_test_paths_from_subdirectory() -> TestResult {
    let (root, _cleanup) = project("price%tax.rs")?;
    let cwd = root.join("py");
    for root_arg in [root.to_str().ok_or("fixture path must be UTF-8")?, ".."] {
        let report = run(&cwd, root_arg, &["check", "--format", "agent"])?;
        let gap = report
            .lines()
            .find(|line| line.starts_with("weakly_exposed|"))
            .ok_or_else(|| format!("missing gap row:\n{report}"))?;
        assert_eq!(
            gap.split('|').nth(1),
            Some("src/price%25tax.rs:2"),
            "{report}"
        );
        assert!(gap.contains("|test=t1|"), "{report}");
        assert!(
            report
                .lines()
                .any(|line| line.starts_with("t1=tests/pricing.rs:4 ")),
            "{report}"
        );
    }
    Ok(())
}
