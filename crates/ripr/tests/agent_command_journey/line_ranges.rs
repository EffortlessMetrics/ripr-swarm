//! RIPR-SPEC-0010: production CLI proof for lossless brief locations (#13395).

use super::*;
use serde_json::json;

fn with_fixture(test: impl FnOnce(&Path) -> Result<(), String>) -> Result<(), String> {
    let root = unique_temp_workspace("brief-line-ranges");
    let result = init_producer_fixture_repo(&root).and_then(|()| test(&root));
    let _ = std::fs::remove_dir_all(&root);
    result
}

fn brief_output(root: &Path, scope: &[&str], ranges: bool) -> Result<Output, String> {
    let mut args = vec!["agent", "brief", "--root", ".", "--json", "--max-seams", "1"];
    args.extend_from_slice(scope);
    if ranges {
        args.push("--line-ranges");
    }
    run_ripr(root, &args)
}

fn parse_success(output: &Output) -> Result<Value, String> {
    assert_success(output, "agent brief line-ranges comparison")?;
    serde_json::from_slice(&output.stdout).map_err(|error| format!("parse brief: {error}"))
}

fn expand_profile(mut ranged: Value) -> Result<Value, String> {
    let object = ranged.as_object_mut().ok_or("brief must be an object")?;
    assert_eq!(
        object.remove("rendering_profile"),
        Some(json!("agent-brief-line-ranges-v1"))
    );
    let working_set = object
        .get_mut("working_set")
        .and_then(Value::as_object_mut)
        .ok_or("brief must have a working set")?;
    assert!(!working_set.contains_key("changed_lines"));
    let ranges = working_set
        .remove("changed_line_ranges")
        .ok_or("profile must carry ranges")?;
    let mut lines = Vec::new();
    for group in ranges.as_array().ok_or("groups must be an array")? {
        let file = group["file"].as_str().ok_or("group file must be text")?;
        for pair in group["ranges"].as_array().ok_or("ranges must be an array")? {
            let [start, end]: [u64; 2] =
                serde_json::from_value(pair.clone()).map_err(|error| error.to_string())?;
            assert!(start <= end);
            for line in start..=end {
                lines.push(json!({"file": file, "line": line}));
            }
        }
    }
    working_set.insert("changed_lines".to_string(), json!(lines));
    Ok(ranged)
}

fn compare_profiles(root: &Path, scope: &[&str]) -> Result<(Value, usize, usize), String> {
    let full = brief_output(root, scope, false)?;
    let full_value = parse_success(&full)?;
    assert_eq!(full_value["limits"]["returned"], 1, "need a selected seam");
    assert_eq!(full_value["limits"]["requested"], 1);
    assert_eq!(full_value["top_seams"].as_array().map(Vec::len), Some(1));
    assert!(full_value.get("rendering_profile").is_none());
    assert!(full_value["working_set"].get("changed_line_ranges").is_none());
    let pretty = serde_json::to_string_pretty(&full_value).map_err(|e| e.to_string())?;
    assert_eq!(full.stdout, format!("{pretty}\n").into_bytes());
    let ranged = brief_output(root, scope, true)?;
    let ranged_value = parse_success(&ranged)?;
    assert_eq!(expand_profile(ranged_value.clone())?, full_value);
    assert_eq!(
        ranged.stdout,
        format!(
            "{}\n",
            serde_json::to_string(&ranged_value).map_err(|e| e.to_string())?
        )
        .into_bytes()
    );
    // Neither opting in once nor changing representation may mutate the full
    // default output or remove warnings/commands/seam evidence from either form.
    let full_again = brief_output(root, scope, false)?;
    assert_success(&full_again, "default brief after opting in")?;
    assert_eq!(full.stdout, full_again.stdout);
    let compact_bytes = serde_json::to_vec(&full_value)
        .map_err(|e| e.to_string())?
        .len()
        + 1;
    println!(
        "brief bytes: full={}, full_minified={compact_bytes}, line_ranges={}",
        full.stdout.len(),
        ranged.stdout.len()
    );
    Ok((full_value, compact_bytes, ranged.stdout.len()))
}

#[test]
fn brief_line_ranges_scale_with_runs_not_seam_cap() -> Result<(), String> {
    with_fixture(|root| {
        let source = std::fs::read_to_string(root.join("src/lib.rs"))
            .map_err(|error| error.to_string())?;
        assert!(source.contains("amount >= discount_threshold"));
        let first_added = source.lines().count() + 1;
        for count in [10, 1_000, 10_000] {
            let mut changed = source.replace(
                "amount >= discount_threshold",
                "amount > discount_threshold",
            );
            for line in 0..count {
                changed.push_str(&format!("// added line {line}\n"));
            }
            std::fs::write(root.join("src/lib.rs"), changed).map_err(|e| e.to_string())?;
            let diff = fixture_git_output(
                root,
                &["diff", "--no-ext-diff", "--unified=0", "--", "src/lib.rs"],
            )?;
            assert!(diff.contains("+    if amount > discount_threshold"));
            std::fs::write(root.join("change.diff"), diff).map_err(|e| e.to_string())?;
            let (full, compact_bytes, range_bytes) =
                compare_profiles(root, &["--diff", "change.diff"])?;
            let mut expected = vec![json!({"file": "src/lib.rs", "line": 2})];
            expected.extend(
                (first_added..first_added + count)
                    .map(|line| json!({"file": "src/lib.rs", "line": line})),
            );
            assert_eq!(full["working_set"]["changed_lines"], json!(expected));
            if count >= 1_000 {
                // Compare equally minified documents: whitespace savings alone
                // or merely echoing the profile marker cannot satisfy this.
                assert!(range_bytes < compact_bytes / 2);
            }
        }
        Ok(())
    })
}

#[test]
fn brief_line_ranges_reduce_fragmented_changes_within_one_file() -> Result<(), String> {
    with_fixture(|root| {
        let source = std::fs::read_to_string(root.join("src/lib.rs"))
            .map_err(|error| error.to_string())?;
        assert!(source.contains("amount >= discount_threshold"));
        let first_added = source.lines().count() + 1;
        let mut baseline = source.clone();
        for line in 0..1_000 {
            baseline.push_str(&format!("// unchanged anchor {line}\n"));
        }
        std::fs::write(root.join("src/lib.rs"), baseline).map_err(|e| e.to_string())?;
        fixture_git_ok(root, &["add", "src/lib.rs"])?;
        commit_fixture(root, "fragmented brief baseline")?;
        let mut changed = source.replace(
            "amount >= discount_threshold",
            "amount > discount_threshold",
        );
        for line in 0..1_000 {
            changed.push_str(&format!("// inserted {line}\n// unchanged anchor {line}\n"));
        }
        std::fs::write(root.join("src/lib.rs"), changed).map_err(|e| e.to_string())?;
        let diff = fixture_git_output(
            root,
            &["diff", "--no-ext-diff", "--unified=0", "--", "src/lib.rs"],
        )?;
        std::fs::write(root.join("change.diff"), diff).map_err(|e| e.to_string())?;
        let (full, compact_bytes, range_bytes) =
            compare_profiles(root, &["--diff", "change.diff"])?;
        let mut expected = vec![json!({"file": "src/lib.rs", "line": 2})];
        expected.extend((0..1_000).map(|index| {
            json!({"file": "src/lib.rs", "line": first_added + 2 * index})
        }));
        assert_eq!(full["working_set"]["changed_lines"], json!(expected));
        assert!(range_bytes < compact_bytes / 2);
        Ok(())
    })
}

#[test]
fn brief_line_ranges_preserve_multiple_files_and_empty_locations() -> Result<(), String> {
    with_fixture(|root| {
        let source = std::fs::read_to_string(root.join("src/lib.rs"))
            .map_err(|error| error.to_string())?;
        let changed = format!(
            "{}\n#[path = \"λ space [x] #%.rs\"]\nmod extra;\n",
            source.replace("amount >= discount_threshold", "amount > discount_threshold")
        );
        std::fs::write(root.join("src/lib.rs"), changed).map_err(|e| e.to_string())?;
        let extra = "src/λ space [x] #%.rs";
        std::fs::write(
            root.join(extra),
            "pub fn other(value: i32) -> bool {\n    value > 5\n}\n",
        )
        .map_err(|e| e.to_string())?;
        fixture_git_ok(root, &["config", "core.quotePath", "false"])?;
        fixture_git_ok(root, &["add", "--intent-to-add", extra])?;
        let diff = fixture_git_output(root, &["diff", "--no-ext-diff", "--unified=0"])?;
        std::fs::write(root.join("change.diff"), diff).map_err(|e| e.to_string())?;
        let (full, _, _) = compare_profiles(root, &["--diff", "change.diff"])?;
        let lines = full["working_set"]["changed_lines"]
            .as_array()
            .ok_or("missing line records")?;
        assert!(
            lines
                .iter()
                .any(|line| line["file"] == "src/lib.rs" && line["line"] == 2)
        );
        assert!(
            lines
                .iter()
                .any(|line| line["file"] == extra && line["line"] == 3)
        );
        assert!(lines.len() > 4, "multiple file subjects must remain nonempty");
        let (files, _, _) = compare_profiles(root, &["--files", "src/lib.rs"])?;
        assert_eq!(files["working_set"]["changed_lines"], json!([]));
        Ok(())
    })
}

#[test]
fn brief_line_ranges_help_usage_and_refusal_preserve_the_contract() -> Result<(), String> {
    with_fixture(|root| {
        let help = run_ripr(root, &["agent", "brief", "--help"])?;
        assert_success(&help, "agent brief --help")?;
        let help = String::from_utf8(help.stdout).map_err(|e| e.to_string())?;
        assert!(help.contains("--line-ranges"));
        assert!(help.contains("inclusive"));
        let no_json = run_ripr(
            root,
            &["agent", "brief", "--files", "src/lib.rs", "--line-ranges"],
        )?;
        assert_eq!(no_json.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&no_json.stderr).contains("requires --json"));
        let scope = ["--base", "missing-brief-range-base-13395"];
        let full = brief_output(root, &scope, false)?;
        let ranged = brief_output(root, &scope, true)?;
        assert_eq!(full.status.code(), Some(2));
        assert_eq!(ranged.status.code(), full.status.code());
        assert!(!full.stdout.is_empty(), "need the JSON refusal subject");
        assert_eq!(ranged.stdout, full.stdout);
        assert_eq!(ranged.stderr, full.stderr);
        let refusal: Value = serde_json::from_slice(&ranged.stdout).map_err(|e| e.to_string())?;
        assert!(refusal.get("rendering_profile").is_none());
        Ok(())
    })
}
