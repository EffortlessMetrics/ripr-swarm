use super::*;

pub(super) fn prepare(
    root: &Path,
    manifest: &[u8],
    read: &mut impl FnMut(&str, &str) -> Result<Vec<u8>, String>,
    git: &mut impl FnMut(&Path, &[&str], Option<&str>) -> Result<CommandResult, String>,
) -> Result<Value, String> {
    let manifest: Value = serde_json::from_slice(manifest).map_err(|e| e.to_string())?;
    if manifest["schema_version"] != "blind_journey_installed_python_fixture.v1" {
        return Err("unsupported Python fixture manifest".into());
    }
    fs::create_dir(root).map_err(|e| e.to_string())?;
    for dir in ["pricing", "tests"] {
        fs::create_dir(root.join(dir)).map_err(|e| e.to_string())?;
    }
    git_ok(
        git,
        root,
        &[
            "init",
            "--quiet",
            "--template=",
            "--initial-branch=main",
            "--object-format=sha1",
        ],
        None,
    )?;
    for (key, value) in [
        ("core.autocrlf", "false"),
        ("core.fsmonitor", "false"),
        ("core.hooksPath", ""),
        ("commit.gpgSign", "false"),
    ] {
        git_ok(git, root, &["config", "--local", key, value], None)?;
    }
    let mut snapshots = Vec::new();
    for (snapshot, date, message) in [
        (
            "base",
            "2026-10-03T00:00:00Z",
            "base: exclusive bulk-discount boundary with below/above tests",
        ),
        (
            "head",
            "2026-10-03T00:01:00Z",
            "head: move bulk-discount boundary to include exactly 20 units",
        ),
    ] {
        let expected = &manifest["repository"]["snapshots"][snapshot];
        if expected["files"].as_object().map(serde_json::Map::len) != Some(FILES.len()) {
            return Err("unexpected Python snapshot inventory".into());
        }
        for path in FILES {
            let bytes = read(snapshot, path)?;
            if expected["files"][path] != digest(&bytes) {
                return Err(format!(
                    "Python committed fixture digest differs: {snapshot}/{path}"
                ));
            }
            fs::write(root.join(path), bytes).map_err(|e| e.to_string())?;
        }
        git_ok(git, root, &["add", "--all"], None)?;
        git_ok(
            git,
            root,
            &["commit", "--quiet", "--no-gpg-sign", "-m", message],
            Some(date),
        )?;
        let commit = git_ok(git, root, &["rev-parse", "HEAD"], None)?
            .stdout
            .trim()
            .to_string();
        let tree = git_ok(git, root, &["rev-parse", "HEAD^{tree}"], None)?
            .stdout
            .trim()
            .to_string();
        if expected["commit"] != commit || expected["tree"] != tree {
            return Err(format!(
                "Python {snapshot} history differs from retained pins: {commit} {tree}"
            ));
        }
        snapshots.push(json!({"snapshot":snapshot,"commit":commit,"tree":tree}));
    }
    // Public QUICKSTART prerequisite; keep the exact retained snapshots above,
    // then record this visible operator preparation separately.
    fs::write(root.join(".gitignore"), "/target/ripr/\n").map_err(|e| e.to_string())?;
    git_ok(git, root, &["add", ".gitignore"], None)?;
    git_ok(
        git,
        root,
        &[
            "commit",
            "--quiet",
            "--no-gpg-sign",
            "-m",
            "operator: documented RIPR artifact ignore",
        ],
        Some("2026-10-03T00:02:00Z"),
    )?;
    let prepared_head = git_ok(git, root, &["rev-parse", "HEAD"], None)?
        .stdout
        .trim()
        .to_string();
    Ok(json!({"snapshots":snapshots,"prepared_head":prepared_head,
        "base":manifest["repository"]["snapshots"]["base"]["commit"],
        "operator_preparation":{"source":"docs/QUICKSTART.md#python-first-hour-preview","path":".gitignore","content":"/target/ripr/\n"}}))
}
