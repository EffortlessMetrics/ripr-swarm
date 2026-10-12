use super::{
    assert_success, first_finding_id, init_git_fixture_repo, printed_ripr_args, run_command,
    run_git, run_ripr, unique_temp_workspace,
};
use std::process::Output;

fn drill_in_lines(human: &str) -> impl Iterator<Item = &str> {
    human
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("ripr explain ") || line.starts_with("ripr context "))
}

/// #6304: the wire card embeds the canonical decision its `next_action`
/// reference is projected from. The reference and the decision agree on
/// identity in both directions (executable or refused), and the human prose
/// renders the same DTO block instead of a hand-written line.
#[test]
fn agent_card_json_embeds_the_canonical_decision() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_workspace("agent-card-canonical");
    let result: Result<(), Box<dyn std::error::Error>> = (|| {
        std::fs::create_dir_all(root.join("src"))?;
        std::fs::create_dir_all(root.join("tests"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"boundary_gap_fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\nname = \"boundary_gap_fixture\"\npath = \"src/lib.rs\"\n",
        )?;
        std::fs::write(
            root.join("src/lib.rs"),
            "pub fn discounted_total(amount: i32, discount_threshold: i32) -> i32 {\n    if amount >= discount_threshold {\n        amount - 10\n    } else {\n        amount\n    }\n}\n",
        )?;
        std::fs::write(
            root.join("tests/pricing.rs"),
            "use boundary_gap_fixture::discounted_total;\n\n#[test]\nfn below_threshold_has_no_discount() {\n    assert_eq!(discounted_total(50, 100), 50);\n}\n\n#[test]\nfn far_above_threshold_discounts() {\n    assert_eq!(discounted_total(10_000, 100), 9_990);\n}\n",
        )?;
        init_git_fixture_repo(&root)?;
        run_git(&root, &["add", "Cargo.toml", "src", "tests"])?;
        let commit = run_command(
            "git",
            Some(&root),
            &[
                "-c",
                "user.name=RIPR test",
                "-c",
                "user.email=ripr@example.invalid",
                "commit",
                "-m",
                "fixture source",
            ],
        )?;
        assert!(
            commit.status.success(),
            "fixture source commit failed: {commit:?}"
        );
        std::fs::write(
            root.join("src/lib.rs"),
            "pub fn discounted_total(amount: i32, discount_threshold: i32) -> i32 {\n    if amount >= discount_threshold {\n        amount - 20\n    } else {\n        amount\n    }\n}\n",
        )?;

        let root_arg = root.display().to_string();
        let card = run_ripr(&[
            "agent",
            "card",
            "--root",
            &root_arg,
            "--seam-id",
            "67fc764ba37d77bd",
            "--json",
        ]);
        assert_success(&card);
        let card_stdout = String::from_utf8_lossy(&card.stdout);
        let card_json: serde_json::Value = serde_json::from_str(&card_stdout)?;
        let canonical = &card_json["canonical_next_action"];
        assert_eq!(
            canonical["schema_version"], "canonical_next_action.v1",
            "wire card must embed the canonical decision:\n{card_stdout}"
        );
        assert_eq!(canonical["producer"], "repair_card");
        assert_eq!(canonical["subject"]["item"], "67fc764ba37d77bd");
        // The reference and the decision agree in both directions.
        if card_json["next_action"].is_null() {
            assert_ne!(
                canonical["action_class"], "run_command",
                "a refused card must not decide executable:\n{card_stdout}"
            );
            assert!(
                canonical
                    .get("command")
                    .is_none_or(|command| command.is_null()),
                "a refused decision must carry no command:\n{card_stdout}"
            );
            assert!(
                canonical["stop"]["kind"].is_string(),
                "a refused decision must name its typed stop:\n{card_stdout}"
            );
        } else {
            assert_eq!(
                canonical["action_class"], "run_command",
                "an exposed reference needs an executable decision:\n{card_stdout}"
            );
            assert_eq!(
                canonical["command"]["command_id"], card_json["next_action"]["command_id"],
                "reference and decision must share the command identity:\n{card_stdout}"
            );
            assert_eq!(
                canonical["command"]["display"], card_json["next_action"]["display"],
                "reference and decision must share the display:\n{card_stdout}"
            );
        }

        // The human prose renders the same DTO block.
        let human = run_ripr(&[
            "agent",
            "card",
            "--root",
            &root_arg,
            "--seam-id",
            "67fc764ba37d77bd",
        ]);
        assert_success(&human);
        let human_stdout = String::from_utf8_lossy(&human.stdout);
        let class = canonical["action_class"]
            .as_str()
            .ok_or("canonical decision must name its class")?;
        for needle in [
            format!("  next action: {class}"),
            "  producer: repair_card".to_string(),
            "67fc764ba37d77bd".to_string(),
        ] {
            assert!(
                human_stdout.contains(&needle),
                "prose must render the canonical block ({needle:?}):\n{human_stdout}"
            );
        }
        Ok(())
    })();
    std::fs::remove_dir_all(&root)?;
    result
}

/// #7257: a default dirty-workspace `check` analyzes the working tree, binds
/// the same effective source as explicit `--worktree`, and its printed
/// follow-up reopens that finding. `--committed` and `--diff` stay their
/// existing scopes. Provenance itself is bound in the producer; this public
/// path proves analysis, action identity, and the follow-up agree.
#[test]
fn default_dirty_check_binds_worktree_provenance_and_reopens_the_finding()
-> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_workspace("check-dirty-default-provenance");
    let result: Result<(), Box<dyn std::error::Error>> = (|| {
        std::fs::create_dir_all(root.join("src"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"dirty-default-provenance\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )?;
        std::fs::write(
            root.join("src/lib.rs"),
            "pub fn over_threshold(amount: i32, threshold: i32) -> bool {\n    amount >= threshold\n}\n",
        )?;
        init_git_fixture_repo(&root)?;
        run_git(&root, &["add", "Cargo.toml", "src"])?;
        let commit = run_command(
            "git",
            Some(&root),
            &[
                "-c",
                "user.name=RIPR test",
                "-c",
                "user.email=ripr@example.invalid",
                "commit",
                "-m",
                "fixture source",
            ],
        )?;
        assert!(
            commit.status.success(),
            "fixture source commit failed: {commit:?}"
        );
        std::fs::write(
            root.join("src/lib.rs"),
            "pub fn over_threshold(amount: i32, threshold: i32) -> bool {\n    amount > threshold\n}\n",
        )?;

        let root_arg = root.display().to_string();
        let parse = |output: &Output| -> Result<serde_json::Value, Box<dyn std::error::Error>> {
            assert_success(output);
            Ok(serde_json::from_slice(&output.stdout)?)
        };
        let ids = |report: &serde_json::Value| -> Vec<String> {
            report["findings"]
                .as_array()
                .map(|findings| {
                    findings
                        .iter()
                        .filter_map(|finding| finding["id"].as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default()
        };

        let worktree = parse(&run_ripr(&[
            "check",
            "--root",
            &root_arg,
            "--worktree",
            "--json",
        ]))?;
        let worktree_ids = ids(&worktree);
        if worktree_ids.is_empty() {
            return Err(format!(
                "fixture must carry an analyzable uncommitted edit; --worktree found none:\n{worktree}"
            )
            .into());
        }

        let default = parse(&run_ripr(&["check", "--root", &root_arg, "--json"]))?;
        if ids(&default) != worktree_ids {
            return Err(format!(
                "dirty default must analyze the working tree like --worktree:\n{default}"
            )
            .into());
        }
        if default["head"]["source"] != "working_tree" {
            return Err(format!("dirty default must name a working-tree head:\n{default}").into());
        }
        if default.get("unanalyzed_working_tree").is_some() {
            return Err(
                format!("dirty default must not claim the edit was excluded:\n{default}").into(),
            );
        }
        let default_action = &default["canonical_next_action"];
        let worktree_action = &worktree["canonical_next_action"];
        if default_action["schema_version"] != "canonical_next_action.v1" {
            return Err(
                format!("public check JSON must embed the canonical action:\n{default}").into(),
            );
        }
        if default_action["action_class"] != worktree_action["action_class"]
            || default_action["subject"]["item"] != worktree_action["subject"]["item"]
            || default_action["stop"]["detail_route"] != worktree_action["stop"]["detail_route"]
        {
            return Err(format!(
                "dirty default and --worktree must agree on the canonical action:\ndefault: {default_action}\n--worktree: {worktree_action}"
            )
            .into());
        }
        if default_action["subject"]["diff_source"]
            .get("working_tree")
            .is_none()
        {
            return Err(format!(
                "dirty-default JSON action must name the working tree:\n{default_action}"
            )
            .into());
        }

        let listing = run_ripr(&["check", "--root", &root_arg]);
        assert_success(&listing);
        let human = String::from_utf8_lossy(&listing.stdout).into_owned();
        let worktree_listing = run_ripr(&["check", "--root", &root_arg, "--worktree"]);
        assert_success(&worktree_listing);
        let worktree_human = String::from_utf8_lossy(&worktree_listing.stdout).into_owned();
        if !human.contains(" --worktree ") || !worktree_human.contains(" --worktree ") {
            return Err(format!(
                "dirty default and explicit --worktree must both print --worktree follow-ups:\ndefault:\n{human}\n--worktree:\n{worktree_human}"
            )
            .into());
        }

        let finding_id =
            first_finding_id(&run_ripr(&["check", "--root", &root_arg, "--json"]).stdout)?;
        let printed_explain = human
            .lines()
            .map(str::trim)
            .find(|line| line.starts_with("ripr explain "))
            .map(str::to_string)
            .ok_or_else(|| format!("dirty default must print an explain follow-up:\n{human}"))?;
        if !printed_explain.contains("--worktree") || !printed_explain.contains(&finding_id) {
            return Err(format!(
                "explain follow-up must keep --worktree and the finding id:\n{printed_explain}"
            )
            .into());
        }

        let json_route = default_action["stop"]["detail_route"]
            .as_str()
            .ok_or_else(|| {
                format!("JSON action must carry a followable inspect route:\n{default_action}")
            })?;
        if json_route.trim() != printed_explain {
            return Err(format!(
                "JSON inspect route must match the printed explain:\njson: {json_route}\nhuman: {printed_explain}"
            )
            .into());
        }

        let decoy = unique_temp_workspace("check-dirty-default-provenance-decoy");
        std::fs::create_dir_all(&decoy)?;
        let args = printed_ripr_args(json_route)?;
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let explained = run_command(env!("CARGO_BIN_EXE_ripr"), Some(&decoy), &args)?;
        assert_success(&explained);
        let explanation = String::from_utf8_lossy(&explained.stdout).into_owned();
        if !explanation.contains(&format!("id: {finding_id}")) {
            let _ = std::fs::remove_dir_all(&decoy);
            return Err(format!(
                "the printed explain must reopen finding {finding_id}:\n{explanation}"
            )
            .into());
        }
        let _ = std::fs::remove_dir_all(&decoy);

        let committed = parse(&run_ripr(&[
            "check",
            "--root",
            &root_arg,
            "--committed",
            "--json",
        ]))?;
        if committed.get("unanalyzed_working_tree") != Some(&serde_json::Value::Bool(true)) {
            return Err(format!(
                "--committed on a dirty tree must retain the unanalysed-worktree disclosure:\n{committed}"
            )
            .into());
        }
        if committed["canonical_next_action"]["subject"]["diff_source"]
            .get("committed")
            .is_none()
        {
            return Err(format!(
                "--committed JSON action must retain committed identity:\n{}",
                committed["canonical_next_action"]
            )
            .into());
        }
        let committed_human = run_ripr(&["check", "--root", &root_arg, "--committed"]);
        assert_success(&committed_human);
        let committed_out = String::from_utf8_lossy(&committed_human.stdout).into_owned();
        if drill_in_lines(&committed_out).any(|line| line.contains("--worktree")) {
            return Err(format!(
                "--committed follow-ups must not carry --worktree:\n{committed_out}"
            )
            .into());
        }

        let diff = run_command("git", Some(&root), &["diff", "HEAD"])?;
        if !diff.status.success() || diff.stdout.is_empty() {
            return Err(format!("fixture git diff must capture the dirty edit: {diff:?}").into());
        }
        let diff_path = root.join("change.diff");
        std::fs::write(&diff_path, &diff.stdout)?;
        let diff_arg = diff_path.display().to_string();
        let supplied = parse(&run_ripr(&[
            "check", "--root", &root_arg, "--diff", &diff_arg, "--json",
        ]))?;
        if ids(&supplied).is_empty() {
            return Err(format!("--diff of the dirty edit must still find it:\n{supplied}").into());
        }
        if supplied["canonical_next_action"]["subject"]["diff_source"]
            .get("working_tree")
            .is_some()
        {
            return Err(format!(
                "--diff JSON action must not become a live-tree claim:\n{}",
                supplied["canonical_next_action"]
            )
            .into());
        }
        let supplied_human = run_ripr(&["check", "--root", &root_arg, "--diff", &diff_arg]);
        assert_success(&supplied_human);
        let supplied_out = String::from_utf8_lossy(&supplied_human.stdout).into_owned();
        let supplied_drill_ins: Vec<&str> = drill_in_lines(&supplied_out).collect();
        if supplied_drill_ins
            .iter()
            .any(|line| line.contains("--worktree"))
            || !supplied_drill_ins
                .iter()
                .any(|line| line.contains("--diff"))
        {
            return Err(format!(
                "--diff on a dirty checkout must stay a supplied scope, not a live-tree claim:\n{supplied_out}"
            )
            .into());
        }
        Ok(())
    })();
    std::fs::remove_dir_all(&root)?;
    result
}
