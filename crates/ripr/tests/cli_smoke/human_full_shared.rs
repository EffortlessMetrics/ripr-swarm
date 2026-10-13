//! #7394: exhaustive CLI sharing must retain the complete finding and commands.

use super::{run_command, run_git, shell_words, unique_temp_workspace, workspace_root};
use serde_json::Value;
use std::path::{Path, PathBuf};

#[path = "human_full_reconstruction.rs"]
mod reconstruction;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const IDS: [&str; 7] = [
    "probe:src_lib.rs:match_arm:6b71a4f5",
    "probe:src_lib.rs:match_arm:35759cb1",
    "probe:src_lib.rs:predicate:27488362",
    "probe:src_lib.rs:return_value:d9dae5de",
    "probe:src_lib.rs:return_value:a207aa93",
    "probe:src_lib.rs:error_path:d71ee10b",
    "probe:src_lib.rs:match_arm:d87ff4a4",
];

fn fixture(scratch: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let root = scratch.join("parse port");
    std::fs::create_dir_all(root.join("src"))?;
    let source = workspace_root().join("fixtures/human_full_shared_parse_port");
    std::fs::copy(source.join("input/Cargo.toml"), root.join("Cargo.toml"))?;
    std::fs::copy(source.join("before.rs"), root.join("src/lib.rs"))?;
    run_git(&root, &["init"])?;
    run_git(&root, &["add", "Cargo.toml", "src/lib.rs"])?;
    run_git(
        &root,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "original accepts zero",
        ],
    )?;
    std::fs::copy(source.join("input/src/lib.rs"), root.join("src/lib.rs"))?;
    run_git(&root, &["add", "src/lib.rs"])?;
    run_git(
        &root,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "reject zero port",
        ],
    )?;
    Ok(root)
}

fn check(root: &Path, format: &str, extra: &[&str]) -> Result<String, Box<dyn std::error::Error>> {
    let root_arg = root.display().to_string();
    let mut args = vec![
        "check", "--root", &root_arg, "--base", "HEAD~1", "--format", format,
    ];
    args.extend_from_slice(extra);
    let output = run_command(env!("CARGO_BIN_EXE_ripr"), Some(root), &args)?;
    assert!(output.status.success(), "check {format}: {output:?}");
    Ok(String::from_utf8(output.stdout)?)
}

fn normalize_library_report(report: &str) -> String {
    let mut header = true;
    report
        .split_inclusive('\n')
        .map(|line| {
            let suffix = if line.ends_with("\r\n") {
                "\r\n"
            } else if line.ends_with('\n') {
                "\n"
            } else {
                ""
            };
            if line == "\n" || line == "\r\n" {
                header = false;
            }
            if header && line.starts_with("root: ") {
                format!("root: <fixture>{suffix}")
            } else if header && line.starts_with("base: HEAD~1 ") {
                format!("base: HEAD~1 <base>{suffix}")
            } else if header && line.starts_with("head: HEAD ") {
                format!("head: HEAD <head>{suffix}")
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("")
}

#[test]
fn human_full_shared_real_git_diff_keeps_every_finding_and_replays_commands() -> TestResult {
    let scratch = unique_temp_workspace("human-full-shared");
    let result: TestResult = (|| {
        std::fs::create_dir_all(&scratch)?;
        let root = fixture(&scratch)?;
        let json: Value = serde_json::from_str(&check(&root, "json", &[])?)?;
        let findings = json["findings"].as_array().ok_or("missing findings")?;
        assert_eq!(
            findings.len(),
            7,
            "nonempty reported subject before sharing assertions"
        );
        for (finding, id) in findings.iter().zip(IDS) {
            assert_eq!(finding["id"], id);
            assert_eq!(finding["classification"], "weakly_exposed");
        }
        assert_eq!(findings[1]["source_currentness"], "base_deleted");
        assert_eq!(findings[4]["source_currentness"], "base_deleted");
        assert_eq!(findings[2]["missing_discriminators"][0]["value"], "p == 0");
        assert_eq!(findings[2]["related_tests"][0]["miss"], "missing_input");
        assert_eq!(
            findings[0]["related_tests"][0]["miss"],
            "observation_unconfirmed"
        );
        let human = check(&root, "human-full", &[])?;
        for id in IDS {
            assert_eq!(human.matches(&format!("  id: {id}\n")).count(), 1);
        }
        assert_eq!(human.matches("Changed\n").count(), 7);
        assert_eq!(human.matches("Drill in:\n").count(), 7);
        assert_eq!(
            human
                .matches("base_deleted — base-side evidence, not a candidate edit target")
                .count(),
            2
        );
        assert!(human.contains("missing discriminator p == 0:"));
        assert_eq!(
            human
                .matches(
                    "related test src/lib.rs:14 parses_valid_port uses strong exact value oracle"
                )
                .count(),
            3,
            "one candidate shared fact, one historical shared fact, and the distinct boundary miss; no repeated per-probe copies"
        );
        assert!(
            human.lines().count() < 265,
            "shared exhaustive view must reduce the real report"
        );
        let expected = std::fs::read_to_string(
            workspace_root()
                .join("fixtures/human_full_shared_parse_port/expected/library-human-lines.json"),
        )?;
        let expected: Vec<String> = serde_json::from_str(&expected)?;
        let expected = expected.concat();
        assert_eq!(
            reconstruction::content_lines(&normalize_library_report(&reconstruction::reconstruct(
                &human
            )?)),
            reconstruction::content_lines(&expected),
            "complete ordered record, including changed fragments, all evidence and guidance"
        );
        assert_eq!(
            reconstruction::definition_members(
                &human,
                "related test src/lib.rs:14 parses_valid_port uses strong exact value oracle"
            )?,
            vec![vec![1, 4, 6, 7], vec![2, 5]],
            "candidate/history attribution; predicate remains local"
        );

        let commands: Vec<_> = human
            .lines()
            .filter_map(|line| line.strip_prefix("  ripr "))
            .collect();
        assert_eq!(commands.len(), 14);
        for (ordinal, command) in commands.iter().enumerate() {
            let args = shell_words::posix_words(command)?;
            let borrowed: Vec<_> = args.iter().map(String::as_str).collect();
            let replay = run_command(env!("CARGO_BIN_EXE_ripr"), Some(&scratch), &borrowed)?;
            assert!(
                replay.status.success(),
                "actual printed command `{command}`: {replay:?}"
            );
            let id = IDS[ordinal / 2];
            if ordinal % 2 == 0 {
                assert!(String::from_utf8(replay.stdout)?.contains(&format!("  id: {id}\n")));
            } else {
                let context: Value = serde_json::from_slice(&replay.stdout)?;
                assert_eq!(context["probe"]["id"], id);
            }
        }
        assert_eq!(check(&root, "text-full", &[])?, human);
        Ok(())
    })();
    let cleanup = std::fs::remove_dir_all(&scratch);
    result?;
    cleanup?;
    Ok(())
}

#[test]
fn human_full_shared_preserves_complete_public_library_bytes() -> TestResult {
    let scratch = unique_temp_workspace("human-full-library");
    let result: TestResult = (|| {
        std::fs::create_dir_all(&scratch)?;
        let root = fixture(&scratch)?;
        let output = ripr::check_workspace(ripr::CheckInput {
            root,
            base: Some("HEAD~1".to_string()),
            ..ripr::CheckInput::default()
        })?;
        assert_eq!(output.findings.len(), 7);
        for (finding, id) in output.findings.iter().zip(IDS) {
            assert_eq!(finding.id, id);
        }
        let report = ripr::render_check(&output, &ripr::OutputFormat::HumanFull)?;
        let expected = std::fs::read_to_string(
            workspace_root()
                .join("fixtures/human_full_shared_parse_port/expected/library-human-lines.json"),
        )?;
        let expected: Vec<String> = serde_json::from_str(&expected)?;
        assert_eq!(normalize_library_report(&report), expected.concat());
        Ok(())
    })();
    let cleanup = std::fs::remove_dir_all(&scratch);
    result?;
    cleanup?;
    Ok(())
}

#[test]
fn human_full_library_normalization_preserves_line_terminators() {
    let text = "root: volatile\nbase: HEAD~1 hash\nhead: HEAD hash\nbody\n\n";
    assert_ne!(
        normalize_library_report(text),
        normalize_library_report(&text.replace("\n", "\r\n"))
    );
    assert_ne!(
        normalize_library_report(text),
        normalize_library_report(text.trim_end_matches('\n'))
    );
}

#[test]
fn human_full_library_normalization_keeps_header_looking_source_literals() {
    let text = "root: volatile\nbase: HEAD~1 hash\nhead: HEAD hash\n\nroot: source\nbase: HEAD~1 source\nhead: HEAD source\n";
    assert!(
        normalize_library_report(text)
            .ends_with("\nroot: source\nbase: HEAD~1 source\nhead: HEAD source\n")
    );
}
