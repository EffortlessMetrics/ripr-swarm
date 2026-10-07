use super::{
    assert_success, init_git_fixture_repo, run_command, run_git, run_ripr, unique_temp_workspace,
};

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
