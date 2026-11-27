use super::*;

const LIMIT: &str = "rust_framework_oracle_unresolved";
const SETTINGS: &str = "let mut settings = insta::Settings::clone_current();
        settings.add_redaction(\".id\", \"[id]\");
        let _guard = settings.bind_to_scope();
        insta::assert_json_snapshot!(record);";
const LITERAL: &str = "insta::assert_json_snapshot!(record, {\".id\" => \"[id]\"});";

pub(super) fn populate(root: &Path, oracle: &str, test_name: &str) -> Result<(), String> {
    std::fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
    std::fs::write(root.join("Cargo.toml"), FIXTURE_CARGO_TOML)
        .map_err(|error| error.to_string())?;
    std::fs::write(
        root.join("src/lib.rs"),
        format!(
            "pub struct Record {{
    pub id: String,
    pub items: usize,
}}

pub fn make_record(prefix: &str, items: &[u32]) -> Record {{
    Record {{
        id: format!(\"{{}}-1\", prefix),
        items: items.iter().count(),
    }}
}}

#[cfg(test)]
mod tests {{
    use super::*;
    #[test]
    fn {test_name}() {{
        let record = make_record(\"ord\", &[1, 2]);
        {oracle}
    }}
}}
"
        ),
    )
    .map_err(|error| error.to_string())?;
    init_repo(root)?;
    run_git(root, &["add", "Cargo.toml", "src"])?;
    git_commit(root, "field snapshot fixture")
}

pub(super) fn read_json(path: &Path) -> Result<serde_json::Value, String> {
    serde_json::from_slice(&std::fs::read(path).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())
}

#[test]
fn pilot_and_brief_honor_real_field_snapshot_limits_and_preserve_controls() -> Result<(), String> {
    for (name, oracle, limited, selected, actionable) in [
        ("settings", SETTINGS, [true, true], [false, false], 1),
        ("literal", LITERAL, [false, true], [true, false], 2),
        (
            "literal-same-line",
            LITERAL,
            [false, true],
            [true, false],
            2,
        ),
        ("literal-reversed", LITERAL, [false, true], [true, false], 2),
        (
            "literal-inline-record",
            LITERAL,
            [false, true],
            [true, false],
            2,
        ),
        (
            "literal-multiline",
            LITERAL,
            [false, true],
            [true, false],
            2,
        ),
        (
            "weak",
            "assert!(record.items > 0);",
            [false, false],
            [true, true],
            3,
        ),
        (
            "marker-name",
            "assert!(record.items > 0);",
            [false, false],
            [true, true],
            3,
        ),
        (
            "exact",
            "assert_eq!(record.items, 2);",
            [false, false],
            [true, false],
            1,
        ),
        (
            "other",
            "let other = Record { id: String::new(), items: 0 };
             insta::assert_json_snapshot!(other);",
            [false, false],
            [true, true],
            3,
        ),
        (
            "exact-snapshot",
            "let settings = insta::Settings::clone_current();
             let _guard = settings.bind_to_scope();
             assert_eq!(record.items, 2);
             insta::assert_json_snapshot!(record);",
            [true, false],
            [false, false],
            0,
        ),
        (
            "inline",
            "insta::assert_json_snapshot!(record, @\"inline\");",
            [false, false],
            [true, true],
            2,
        ),
    ] {
        let fixture = Fixture::dir(&format!("facade-field-snapshot-{name}"))?;
        let root = &fixture.root;
        let test_name = if name == "marker-name" {
            LIMIT
        } else {
            "record_oracle"
        };
        populate(root, oracle, test_name)?;
        let source_path = root.join("src/lib.rs");
        let source = std::fs::read_to_string(&source_path).map_err(|error| error.to_string())?;
        let changed = match name {
            "literal-same-line" => source.replace("prefix),\n        items:", "prefix), items:"),
            "literal-reversed" => source.replace(
                "id: format!(\"{}-1\", prefix),\n        items: items.iter().count(),",
                "items: items.iter().count(), id: format!(\"{}-1\", prefix),",
            ),
            "literal-inline-record" => source
                .replace("Record {\n        id:", "Record { id:")
                .replace("prefix),\n        items:", "prefix), items:")
                .replace("count(),\n    }", "count() }"),
            "literal-multiline" => source
                .replace("id: format!", "id:\n            format!")
                .replace("items: items.iter()", "items:\n            items.iter()"),
            _ => source.clone(),
        };
        if name.starts_with("literal-") {
            assert_ne!(
                changed, source,
                "the requested layout must change the source"
            );
            std::fs::write(&source_path, changed).map_err(|error| error.to_string())?;
            run_git(root, &["add", "src/lib.rs"])?;
            git_commit(root, "field initializer layout")?;
        }
        let root_arg = root.to_string_lossy().into_owned();
        let pilot = run_ripr(root, &["pilot", "--root", &root_arg, "--quiet"])?;
        assert_exit(&pilot, 0, "pilot on the field snapshot fixture");
        let summary_path = root.join("target/ripr/pilot/pilot-summary.json");
        let exposure_path = root.join("target/ripr/pilot/repo-exposure.json");
        let summary = read_json(&summary_path)?;
        let exposure = read_json(&exposure_path)?;
        assert_eq!(summary["status"], "complete", "{summary}");
        assert_eq!(
            summary["actionable_seams_total"], actionable,
            "{name}: {summary}"
        );
        assert_eq!(
            summary["withheld_static_limitations_total"],
            limited.iter().filter(|limited| **limited).count(),
            "{name}: {summary}"
        );
        let ranked = summary["top_actionable_seams"]
            .as_array()
            .ok_or("pilot must retain top_actionable_seams")?;
        let seams = exposure["seams"].as_array().ok_or("missing repo seams")?;
        assert_eq!(seams.len(), 3, "one return and two field seams: {exposure}");
        for (index, expression) in [
            (0, "id: format!(\"{}-1\", prefix)"),
            (1, "items: items.iter().count()"),
        ] {
            let matches: Vec<_> = seams
                .iter()
                .filter(|entry| {
                    entry["kind"] == "field_construction"
                        && entry["expression"].as_str().is_some_and(|text| {
                            let tokens = text.split_whitespace();
                            tokens.eq(expression.split_whitespace())
                        })
                })
                .collect();
            assert_eq!(
                matches.len(),
                1,
                "one intended field expression {expression}"
            );
            let seam = matches[0];
            if !name.starts_with("literal-") {
                assert_eq!(seam["line"], [8, 9][index]);
            }
            let seam_id = seam["seam_id"].as_str().ok_or("missing seam identity")?;
            let record = &seam["evidence_record"];
            let limits = record["static_limitations"]
                .as_array()
                .ok_or("missing static limitations")?;
            assert_eq!(
                limits.iter().any(|limit| limit["category"] == LIMIT),
                limited[index],
                "{name}: {record}"
            );
            let expected_class = if matches!(name, "exact" | "exact-snapshot") && index == 1 {
                "strongly_gripped"
            } else {
                "weakly_gripped"
            };
            assert_eq!(record["grip_class"], expected_class, "{name}: {record}");
            for stage in ["reach", "activate", "propagate", "observe"] {
                assert_eq!(record["evidence_path"][stage]["state"], "yes", "{record}");
            }
            assert_eq!(
                record["evidence_path"]["discriminate"]["state"],
                if expected_class == "strongly_gripped" {
                    "yes"
                } else {
                    "weak"
                },
                "{record}"
            );
            assert_eq!(
                ranked.iter().any(|entry| entry["seam_id"] == seam_id),
                selected[index],
                "{name}: {summary}"
            );
            let brief = run_ripr(
                root,
                &[
                    "agent",
                    "brief",
                    "--root",
                    &root_arg,
                    "--seam-id",
                    seam_id,
                    "--json",
                ],
            )?;
            assert_exit(&brief, 0, "brief on the exact field seam");
            let brief: serde_json::Value =
                serde_json::from_slice(&brief.stdout).map_err(|error| error.to_string())?;
            let top = brief["top_seams"]
                .as_array()
                .ok_or("missing brief top_seams")?;
            assert_eq!(top.len(), usize::from(selected[index]), "{name}: {brief}");
            if selected[index] {
                assert_eq!(top[0]["seam_id"], seam_id);
            }
            if limited[index] {
                assert!(
                    brief["warnings"].as_array().is_some_and(|warnings| {
                        warnings.iter().any(|warning| {
                            warning.as_str().is_some_and(|warning| {
                                warning.contains(
                                    "cannot tell whether the framework oracle notices the changed field",
                                )
                            })
                        })
                    }),
                    "{brief}"
                );
            }
        }
        let return_seam = seams
            .iter()
            .find(|entry| entry["kind"] == "return_value")
            .ok_or("missing return control")?;
        assert!(
            return_seam["evidence_record"]["static_limitations"]
                .as_array()
                .is_some_and(|limits| limits.iter().all(|limit| limit["category"] != LIMIT)),
            "return-value selection is outside this change: {return_seam}"
        );

        let warm = run_ripr_env(
            root,
            &["pilot", "--root", &root_arg, "--quiet"],
            &[("RIPR_REPO_EXPOSURE_LATENCY_TRACE", "1")],
        )?;
        assert_exit(&warm, 0, "warm pilot on the field snapshot fixture");
        assert!(
            stderr_text(&warm).contains("phase=cache_load status=hit"),
            "{}",
            stderr_text(&warm)
        );
        assert_eq!(read_json(&exposure_path)?["seams"], exposure["seams"]);
        assert_eq!(
            read_json(&summary_path)?["actionable_seams_total"],
            summary["actionable_seams_total"]
        );
    }
    Ok(())
}
