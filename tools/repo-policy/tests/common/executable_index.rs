//! Deterministic executable-policy index, shared by both frontdoor fixtures.
use std::path::Path;

pub fn initialize(
    root: &Path,
    mut git: impl FnMut(&[&str]) -> Result<(), String>,
) -> Result<(), String> {
    git(&["init", "--quiet"])?;
    for name in ["allowed", "demoted", "ordinary", "unregistered"] {
        std::fs::write(root.join(name), format!("{name}\n")).map_err(|e| e.to_string())?;
    }
    // Do not stage copied policy inputs. Explicit paths keep the index and
    // filesystem-mode independence identical to the per-file setup.
    git(&[
        "-c",
        "core.filemode=false",
        "add",
        "--",
        "allowed",
        "demoted",
        "ordinary",
        "unregistered",
    ])?;
    git(&[
        "update-index",
        "--chmod=-x",
        "--",
        "allowed",
        "demoted",
        "ordinary",
        "unregistered",
    ])?;
    git(&[
        "update-index",
        "--chmod=+x",
        "--",
        "allowed",
        "demoted",
        "unregistered",
    ])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_stops_at_the_first_git_failure() -> Result<(), String> {
        let root = std::env::temp_dir().join(format!(
            "executable-index-failures-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|error| error.to_string())?
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
        for failure in 1..=4 {
            let mut calls = 0;
            let result = initialize(&root, |_| {
                calls += 1;
                if calls == failure {
                    Err(format!("git failure control {failure}"))
                } else {
                    Ok(())
                }
            });
            assert_eq!(result, Err(format!("git failure control {failure}")));
            assert_eq!(calls, failure, "no commands after the failed setup phase");
        }
        std::fs::remove_dir_all(root).map_err(|error| error.to_string())
    }
}

/// Exercise literal identities through a real Git index and the actual frontdoor.
pub fn literal_paths(
    root: &Path,
    mut git: impl FnMut(&[&str]) -> Result<(), String>,
    mut run: impl FnMut() -> Result<std::process::Output, String>,
) -> Result<(), String> {
    let names = if cfg!(unix) {
        vec![
            "space name",
            "non-ASCII-é",
            "tab\tname",
            "quote\"name",
            "back\\slash",
        ]
    } else {
        vec!["space name", "non-ASCII-é"]
    };
    for name in &names {
        std::fs::write(root.join(name), b"literal\n").map_err(|e| e.to_string())?;
        git(&["add", "--", name])?;
        git(&["update-index", "--chmod=+x", "--", name])?;
    }
    let rows = format!("allowed\ndemoted\nunregistered\n{}\n", names.join("\n"));
    let allowlist = root.join("policy/executable_allowlist.txt");
    std::fs::write(&allowlist, &rows).map_err(|e| e.to_string())?;
    let output = run()?;
    assert!(
        output.status.success(),
        "literal paths: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    // A literal backslash must never authorize a slash-separated identity.
    #[cfg(unix)]
    {
        std::fs::write(&allowlist, rows.replace("back\\slash", "back/slash"))
            .map_err(|e| e.to_string())?;
        let output = run()?;
        assert!(!output.status.success());
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(error.contains("not allowlisted"), "{error}");
        assert!(
            error.contains("back/slash allowlist entry is stale"),
            "{error}"
        );
    }
    std::fs::write(&allowlist, &rows).map_err(|e| e.to_string())?;
    git(&["update-index", "--chmod=-x", "--", names[0]])?;
    let output = run()?;
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("mode is 100644"));
    git(&["update-index", "--chmod=+x", "--", names[0]])?;
    #[cfg(unix)]
    for name in ["line\nbreak", " leading", "trailing ", "#comment"] {
        std::fs::write(root.join(name), b"unsupported\n").map_err(|e| e.to_string())?;
        git(&["add", "--", name])?;
        // Even ordinary staged paths must not be silently decoded or normalized.
        git(&["update-index", "--chmod=-x", "--", name])?;
        let output = run()?;
        assert!(!output.status.success(), "unsupported identity {name:?}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("unsupported Git index path"));
        let report = std::fs::read_to_string(root.join("target/ripr/reports/executable-files.md"))
            .map_err(|e| e.to_string())?;
        assert!(report.contains("Status: fail"));
        assert!(report.contains(&format!("{name:?}")), "{report}");
        git(&["rm", "--cached", "--", name])?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        let directory = root.join("nonutf8");
        std::fs::create_dir(&directory).map_err(|e| e.to_string())?;
        let name = std::ffi::OsString::from_vec(b"invalid-\xff".to_vec());
        std::fs::write(directory.join(name), b"bytes\n").map_err(|e| e.to_string())?;
        git(&["add", "--", "nonutf8"])?;
        let output = run()?;
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("non-UTF-8 bytes"));
        git(&["rm", "-r", "--cached", "--", "nonutf8"])?;
    }
    std::fs::write(&allowlist, b"invalid-\xff\n").map_err(|e| e.to_string())?;
    let output = run()?;
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("as UTF-8"));
    std::fs::write(&allowlist, rows).map_err(|e| e.to_string())?;
    assert!(run()?.status.success());
    // Use actual unresolved index stages, rather than fabricated ls-files output.
    let conflict = root.join("conflict");
    std::fs::write(&conflict, b"base\n").map_err(|e| e.to_string())?;
    git(&["add", "--", "conflict"])?;
    let commit = [
        "-c",
        "user.name=Fixture",
        "-c",
        "user.email=fixture@example.invalid",
        "-c",
        "commit.gpgsign=false",
        "commit",
        "-qm",
        "fixture",
    ];
    git(&commit)?;
    git(&["branch", "side"])?;
    std::fs::write(&conflict, b"ours\n").map_err(|e| e.to_string())?;
    git(&["add", "--", "conflict"])?;
    git(&commit)?;
    git(&["checkout", "-q", "side"])?;
    std::fs::write(&conflict, b"theirs\n").map_err(|e| e.to_string())?;
    git(&["add", "--", "conflict"])?;
    git(&commit)?;
    git(&["checkout", "-q", "-"])?;
    // read-tree populates the index without invoking a merge editor or hooks.
    git(&["read-tree", "-m", "HEAD~1", "HEAD", "side"])?;
    let output = run()?;
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(
        error.contains("unmerged Git index path: conflict"),
        "{error}"
    );
    assert!(error.contains("requires stage 0"), "{error}");
    Ok(())
}
