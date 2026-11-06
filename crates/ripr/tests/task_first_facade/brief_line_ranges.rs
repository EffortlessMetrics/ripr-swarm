use super::{Fixture, assert_exit, git_commit, run_git, run_ripr, weaken_boundary_predicate};
use serde_json::{Value, json};

fn expand(mut value: Value) -> Result<Value, String> {
    assert_eq!(value["rendering_profile"], "agent-brief-line-ranges-v1");
    value
        .as_object_mut()
        .ok_or("brief object")?
        .remove("rendering_profile");
    let working = value["working_set"].as_object_mut().ok_or("working set")?;
    assert!(!working.contains_key("changed_lines"));
    let groups = working.remove("changed_line_ranges").ok_or("ranges")?;
    let mut lines = Vec::new();
    for group in groups.as_array().ok_or("groups")? {
        for pair in group["ranges"].as_array().ok_or("pairs")? {
            let start = pair[0].as_u64().ok_or("start")?;
            let end = pair[1].as_u64().ok_or("end")?;
            assert!(start <= end);
            for line in start..=end {
                lines.push(json!({"file":group["file"],"line":line}));
            }
        }
    }
    working.insert("changed_lines".to_string(), Value::Array(lines));
    Ok(value)
}

#[test]
fn brief_line_ranges_real_diff_preserves_full_document_and_measures_bytes() -> Result<(), String> {
    for shape in ["contiguous", "isolated", "multiple-files"] {
        let fixture = Fixture::boundary(&format!("brief-ranges-{shape}"))?;
        let source = fixture.root.join("src/lib.rs");
        let original = std::fs::read_to_string(&source).map_err(|e| e.to_string())?;
        let comments = |new: bool| -> String {
            (0..1000)
                .map(|i| {
                    let marker = if new { "after" } else { "before" };
                    if shape == "isolated" {
                        format!("// {marker} {i}\n// unchanged separator {i}\n")
                    } else {
                        format!("// {marker} {i}\n")
                    }
                })
                .collect()
        };
        let before = if shape == "contiguous" {
            original.clone()
        } else {
            format!("{}{original}", comments(false))
        };
        std::fs::write(&source, before).map_err(|e| e.to_string())?;
        run_git(&fixture.root, &["add", "."])?;
        git_commit(&fixture.root, "line range before")?;
        std::fs::write(&source, format!("{}{original}", comments(true)))
            .map_err(|e| e.to_string())?;
        if shape == "multiple-files" {
            for i in 0..20 {
                std::fs::write(
                    fixture.root.join(format!("src/file_{i}.rs")),
                    format!("// new file {i}\n"),
                )
                .map_err(|e| e.to_string())?;
            }
            run_git(&fixture.root, &["add", "."])?;
        }
        weaken_boundary_predicate(&fixture.root)?;
        let patch = run_git(&fixture.root, &["diff", "HEAD", "--", "src"])?;
        assert!(patch.contains("+    if amount > discount_threshold"));
        let path = fixture.root.join("change.patch");
        std::fs::write(&path, patch).map_err(|e| e.to_string())?;
        let root = fixture.root.to_string_lossy();
        let diff = path.to_string_lossy();
        let mut args = vec![
            "agent",
            "brief",
            "--root",
            &root,
            "--diff",
            &diff,
            "--json",
            "--max-seams",
            "1",
        ];
        let full_output = run_ripr(&fixture.root, &args)?;
        assert_exit(&full_output, 0, "full brief");
        let full: Value = serde_json::from_slice(&full_output.stdout).map_err(|e| e.to_string())?;
        assert_eq!(full["top_seams"].as_array().ok_or("seams")?.len(), 1);
        assert_eq!(full["limits"]["requested"], 1);
        let lines = full["working_set"]["changed_lines"]
            .as_array()
            .ok_or("lines")?;
        assert!(
            lines.len() >= 1001,
            "changed lines cannot follow the seam cap"
        );
        args.push("--line-ranges");
        let ranged_output = run_ripr(&fixture.root, &args)?;
        assert_exit(&ranged_output, 0, "range brief");
        let ranged: Value =
            serde_json::from_slice(&ranged_output.stdout).map_err(|e| e.to_string())?;
        assert_eq!(expand(ranged.clone())?, full);
        assert!(
            !ranged_output.stdout[..ranged_output.stdout.len() - 1].contains(&b'\n'),
            "profile must be minified"
        );
        let minified = serde_json::to_vec(&full).map_err(|e| e.to_string())?;
        println!(
            "{shape}: actual stdout full={} range={}, minified full={} bytes",
            full_output.stdout.len(),
            ranged_output.stdout.len(),
            minified.len()
        );
        assert!(
            ranged_output.stdout.len() < minified.len(),
            "these specific repeated-path fixtures must shrink; no universal claim"
        );
        // Deliberately wrong profile forms must fail the semantic oracle.
        let mut dropped = ranged.clone();
        dropped["working_set"]["changed_line_ranges"]
            .as_array_mut()
            .ok_or("groups")?
            .clear();
        assert_ne!(expand(dropped)?, full);
        let mut merged = ranged;
        merged["working_set"]["changed_line_ranges"][0]["ranges"] = json!([[1, 3000]]);
        assert_ne!(expand(merged)?, full);
        // The profile never changes the default bytes on repeated invocation.
        args.pop();
        assert_eq!(run_ripr(&fixture.root, &args)?.stdout, full_output.stdout);
    }
    Ok(())
}

#[test]
fn brief_line_ranges_help_usage_and_refusal_keep_their_contract() -> Result<(), String> {
    let fixture = Fixture::boundary("brief-range-refusal")?;
    let help = run_ripr(&fixture.root, &["agent", "brief", "--help"])?;
    assert_exit(&help, 0, "brief help");
    let help = String::from_utf8_lossy(&help.stdout);
    assert!(help.contains("--line-ranges") && help.contains("requires --json"));
    let root = fixture.root.to_string_lossy();
    let missing_json = run_ripr(
        &fixture.root,
        &[
            "agent",
            "brief",
            "--root",
            &root,
            "--files",
            "src/lib.rs",
            "--line-ranges",
        ],
    )?;
    assert_exit(&missing_json, 2, "JSON-only profile");
    assert!(
        String::from_utf8_lossy(&missing_json.stderr).contains("--line-ranges requires --json")
    );
    let mut args = vec![
        "agent",
        "brief",
        "--root",
        &root,
        "--diff",
        "absent.patch",
        "--json",
    ];
    let full = run_ripr(&fixture.root, &args)?;
    args.push("--line-ranges");
    let ranged = run_ripr(&fixture.root, &args)?;
    assert_eq!(full.status.code(), ranged.status.code());
    assert!(!full.status.success());
    assert_eq!(full.stdout, ranged.stdout);
    Ok(())
}
