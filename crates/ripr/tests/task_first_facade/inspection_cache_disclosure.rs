use super::{Fixture, assert_exit, run_git, run_ripr_env, stdout_text, weaken_boundary_predicate};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

// Include directories and exact file bytes, including .git, so no-write controls
// detect new empty directories and changed existing files as well as new files.
fn inventory(root: &Path) -> Result<BTreeMap<PathBuf, Option<Vec<u8>>>, String> {
    fn visit(
        root: &Path,
        directory: &Path,
        entries: &mut BTreeMap<PathBuf, Option<Vec<u8>>>,
    ) -> Result<(), String> {
        for entry in std::fs::read_dir(directory).map_err(|e| e.to_string())? {
            let path = entry.map_err(|e| e.to_string())?.path();
            let relative = path
                .strip_prefix(root)
                .map_err(|e| e.to_string())?
                .to_path_buf();
            if path.is_dir() {
                entries.insert(relative, None);
                visit(root, &path, entries)?;
            } else {
                entries.insert(
                    relative,
                    Some(std::fs::read(path).map_err(|e| e.to_string())?),
                );
            }
        }
        Ok(())
    }
    let mut entries = BTreeMap::new();
    visit(root, root, &mut entries)?;
    Ok(entries)
}

#[test]
fn inspection_discovery_discloses_observed_cache_writes() -> Result<(), String> {
    let cwd = Fixture::dir("inspection-cache-cwd")?;
    let discovery = run_ripr_env(&cwd.root, &["help", "--json"], &[])?;
    assert_exit(&discovery, 0, "machine discovery");
    let document: serde_json::Value =
        serde_json::from_slice(&discovery.stdout).map_err(|e| e.to_string())?;
    for id in ["cmd:status", "cmd:agent.status", "cmd:cache.status"] {
        let row = document["commands"]
            .as_array()
            .ok_or("commands missing")?
            .iter()
            .find(|row| row["id"] == id)
            .ok_or("status row missing")?;
        assert_eq!(
            row["operation"], "read-only",
            "{id} changed disclosure unexpectedly"
        );
    }
    for verb in ["explain", "context"] {
        let fixture = Fixture::boundary(&format!("inspection-cache-{verb}"))?;
        let root = &fixture.root;
        weaken_boundary_predicate(root)?;
        let patch = root.join("change.patch");
        std::fs::write(
            &patch,
            run_git(root, &["diff", "HEAD", "--", "src/lib.rs"])?,
        )
        .map_err(|e| e.to_string())?;
        let root_arg = root.to_string_lossy();
        let patch_arg = patch.to_string_lossy();
        let cache = root.join("target/ripr/cache");
        assert!(!cache.exists(), "positive control must start cold");
        let mut args = vec![verb];
        if verb == "context" {
            args.push("--at");
        }
        args.push("src/lib.rs:2");
        args.extend(["--root", &root_arg, "--diff", &patch_arg]);
        let before = inventory(root)?;
        let fresh = run_ripr_env(&cwd.root, &args, &[("RIPR_CACHE_DIR", "")])?;
        assert_exit(&fresh, 0, "fresh inspection");
        assert!(
            stdout_text(&fresh).contains("src/lib.rs"),
            "selected finding missing"
        );
        assert!(
            cache.is_dir(),
            "fresh {verb} did not exercise the cache writer"
        );
        let after = inventory(root)?;
        assert!(
            after
                .iter()
                .any(|(path, bytes)| path.starts_with("target/ripr/cache") && bytes.is_some()),
            "cache contained no files"
        );
        for (path, bytes) in &before {
            assert_eq!(
                after.get(path),
                Some(bytes),
                "fresh inspection changed {path:?}"
            );
        }
        let id = format!("cmd:{verb}");
        let human_help = run_ripr_env(&cwd.root, &[verb, "--help"], &[])?;
        assert_exit(&human_help, 0, "inspection help");
        let help = stdout_text(&human_help);
        assert!(help.contains("target/ripr/cache/") && help.contains("RIPR_CACHE_DIR"));
        let row = document["commands"]
            .as_array()
            .ok_or("commands missing")?
            .iter()
            .find(|row| row["id"] == id)
            .ok_or("inspection row missing")?;
        assert_eq!(
            row["operation"], "writes-artifacts",
            "{verb} wrote cache but discovery claims otherwise"
        );
        let output = row["outputs"]["default"]
            .as_str()
            .ok_or("cache output missing")?;
        assert!(output.contains("target/ripr/cache/") && output.contains("RIPR_CACHE_DIR"));
        let workflow = document["workflows"]
            .as_array()
            .ok_or("workflows missing")?
            .iter()
            .find(|row| row["id"] == "inspect-change")
            .ok_or("inspect-change missing")?;
        let step = workflow["steps"]
            .as_array()
            .ok_or("steps missing")?
            .iter()
            .find(|step| step["command"] == id)
            .ok_or("inspection step missing")?;
        assert_eq!(step["operation"], row["operation"]);
        assert!(
            step["writes"]
                .as_array()
                .ok_or("writes missing")?
                .iter()
                .any(|v| v == output)
        );
        assert!(
            workflow["artifacts_written"]
                .as_array()
                .ok_or("workflow artifact summary missing")?
                .iter()
                .any(|v| v == output),
            "inspect-change JSON summary omits {verb} cache output"
        );
        let workflow_help = run_ripr_env(&cwd.root, &["help", "workflow", "inspect-change"], &[])?;
        assert_exit(&workflow_help, 0, "inspect-change help");
        let rendered_workflow = stdout_text(&workflow_help);
        let summary = rendered_workflow
            .lines()
            .find(|line| line.trim_start().starts_with("writes: "))
            .ok_or("workflow help artifact summary missing")?;
        assert!(
            summary.contains(output),
            "inspect-change help summary omits {verb} cache output"
        );
    }
    Ok(())
}

#[test]
fn saved_inspection_and_status_controls_preserve_inventories() -> Result<(), String> {
    let cwd = Fixture::dir("inspection-readonly-cwd")?;
    let fixture = Fixture::boundary("inspection-readonly")?;
    let root = &fixture.root;
    weaken_boundary_predicate(root)?;
    let patch = root.join("change.patch");
    std::fs::write(
        &patch,
        run_git(root, &["diff", "HEAD", "--", "src/lib.rs"])?,
    )
    .map_err(|e| e.to_string())?;
    let artifact = root.join("findings.json");
    let root_arg = root.to_string_lossy();
    let patch_arg = patch.to_string_lossy();
    let artifact_arg = artifact.to_string_lossy();
    let producer = run_ripr_env(
        &cwd.root,
        &[
            "check",
            "--root",
            &root_arg,
            "--diff",
            &patch_arg,
            "--write-artifact",
            &artifact_arg,
        ],
        &[("RIPR_CACHE_DIR", "")],
    )?;
    assert_exit(&producer, 0, "real findings-artifact producer");
    assert!(artifact.is_file());
    let target = root.join("target");
    assert!(
        target.join("ripr/cache").is_dir(),
        "producer did not create cache"
    );
    std::fs::remove_dir_all(&target).map_err(|e| e.to_string())?;
    let before = inventory(root)?;
    let cwd_before = inventory(&cwd.root)?;
    // Prove this inventory can detect a write, independently of product output.
    let sentinel = root.join("inventory-sentinel");
    std::fs::write(&sentinel, "detected").map_err(|e| e.to_string())?;
    assert_ne!(inventory(root)?, before);
    std::fs::remove_file(sentinel).map_err(|e| e.to_string())?;
    let controls: &[&[&str]] = &[
        &[
            "explain",
            "src/lib.rs:2",
            "--root",
            &root_arg,
            "--from",
            &artifact_arg,
        ],
        &[
            "context",
            "--at",
            "src/lib.rs:2",
            "--root",
            &root_arg,
            "--from",
            &artifact_arg,
        ],
        &["status", "--root", &root_arg, "--json"],
        &["agent", "status", "--root", &root_arg, "--json"],
        &["cache", "status", "--json"],
        &["explain", "--help"],
        &["context", "--help"],
        &["help", "--json"],
    ];
    for args in controls {
        let result = run_ripr_env(&cwd.root, args, &[("RIPR_CACHE_DIR", "")])?;
        assert_exit(&result, 0, "read-only control");
        assert_eq!(inventory(root)?, before, "{args:?} changed selected root");
        assert_eq!(
            inventory(&cwd.root)?,
            cwd_before,
            "{args:?} changed caller directory"
        );
    }
    Ok(())
}
