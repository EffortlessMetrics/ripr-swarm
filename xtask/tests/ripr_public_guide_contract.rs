//! Current repository guides must agree with the candidate's rendered help.
//! These seven source-checkout audits are deliberately outside the published crate.

#[path = "../../crates/ripr/tests/common/help_contract.rs"]
mod help_contract;
#[path = "common/ripr_help.rs"]
mod ripr_help;
use help_contract::{assert_contains, normalized, undocumented_doc_flags};
use ripr_help::{rendered_help, run_ripr};

const COMMAND_HIERARCHY_DOC: &str = include_str!("../../docs/COMMAND_HIERARCHY.md");
const ROOT_README: &str = include_str!("../../README.md");
const QUICKSTART_DOC: &str = include_str!("../../docs/QUICKSTART.md");
const EXIT_CODES_DOC: &str = include_str!("../../docs/EXIT_CODES.md");

/// User-facing guides whose `ripr ...` commands a reader or agent copies.
/// Internal specs, plans, and handoffs are historical records and stay out.
const PUBLIC_COMMAND_DOCS: &[(&str, &str)] = &[
    ("README.md", ROOT_README),
    (
        "crates/ripr/README.md",
        include_str!("../../crates/ripr/README.md"),
    ),
    (
        "editors/vscode/README.md",
        include_str!("../../editors/vscode/README.md"),
    ),
    ("docs/QUICKSTART.md", QUICKSTART_DOC),
    ("docs/COMMAND_HIERARCHY.md", COMMAND_HIERARCHY_DOC),
    ("docs/EXIT_CODES.md", EXIT_CODES_DOC),
    (
        "docs/LLM_OPERATOR_GUIDE.md",
        include_str!("../../docs/LLM_OPERATOR_GUIDE.md"),
    ),
    (
        "docs/FIRST_PR_WORKFLOW.md",
        include_str!("../../docs/FIRST_PR_WORKFLOW.md"),
    ),
    (
        "docs/CONFIGURATION.md",
        include_str!("../../docs/CONFIGURATION.md"),
    ),
    (
        "docs/AGENT_WORKFLOWS.md",
        include_str!("../../docs/AGENT_WORKFLOWS.md"),
    ),
    (
        "docs/TARGETED_TEST_WORKFLOW.md",
        include_str!("../../docs/TARGETED_TEST_WORKFLOW.md"),
    ),
    (
        "docs/interop/mcp.md",
        include_str!("../../docs/interop/mcp.md"),
    ),
    (
        "docs/interop/neovim-lsp.md",
        include_str!("../../docs/interop/neovim-lsp.md"),
    ),
    (
        "docs/interop/other-editors-lsp.md",
        include_str!("../../docs/interop/other-editors-lsp.md"),
    ),
    (
        "docs/releases/0.11.0-release-notes.md",
        include_str!("../../docs/releases/0.11.0-release-notes.md"),
    ),
];

/// Validate the command cell of each task row, not mentions elsewhere in the
/// document. The result prose can change without changing the command's job.
fn assert_doc_command_routes(doc: &str) -> Result<(), String> {
    for (task, expected) in [
        ("Inspect one change", "ripr check"),
        ("Explore the repository", "ripr pilot --root ."),
        ("Repair a selected Rust gap", "ripr repair"),
        ("Resume a repair", "ripr status --root ."),
        ("Compose PR evidence", "ripr first-pr"),
        ("Add advisory CI", "ripr init --ci github"),
        ("Diagnose setup", "ripr doctor"),
        ("Record result usefulness", "ripr feedback record"),
    ] {
        let mut matches = doc.lines().filter_map(|line| {
            let mut cells = line.trim().strip_prefix('|')?.split('|');
            let role = cells.next()?.trim();
            let command = cells.next()?.trim();
            let result = cells.next()?.trim();
            (role == task && !result.is_empty()).then_some(command)
        });
        let cell = matches
            .next()
            .ok_or_else(|| format!("command guide lost task row `{task}`"))?;
        if matches.next().is_some() {
            return Err(format!(
                "command guide has duplicate task rows for `{task}`"
            ));
        }
        let correct_command = cell
            .split('`')
            .skip(1)
            .step_by(2)
            .any(|span| normalized(span) == expected);
        if !correct_command {
            return Err(format!("task `{task}` must route to `{expected}`"));
        }
    }
    Ok(())
}

fn first_bash_block(doc: &str) -> Result<String, String> {
    let (_, after) = doc
        .split_once("```bash")
        .ok_or_else(|| "document has no Bash example".to_string())?;
    let (body, _) = after
        .split_once("```")
        .ok_or_else(|| "Bash example has no closing fence".to_string())?;
    Ok(normalized(body))
}

fn doc_section(doc: &str, heading: &str) -> Result<String, String> {
    let lf = doc.replace("\r\n", "\n");
    let marker = format!("\n{heading}\n");
    let (_, after) = lf
        .split_once(&marker)
        .ok_or_else(|| format!("document lost linked section `{heading}`"))?;
    after
        .split("\n## ")
        .next()
        .map(str::to_string)
        .ok_or_else(|| format!("document has no content for `{heading}`"))
}

/// Keep task ownership and executable entry points aligned without pinning
/// editorial prose.
#[test]
fn docs_keep_the_canonical_role_vocabulary() -> Result<(), String> {
    assert_doc_command_routes(COMMAND_HIERARCHY_DOC)?;
    if first_bash_block(ROOT_README)? != "cargo install ripr ripr check" {
        return Err("README first run must install ripr and inspect a change".to_string());
    }
    for target in [
        "docs/QUICKSTART.md#cli-first-hour",
        "docs/QUICKSTART.md#agent-or-reviewer-first-hour",
        "docs/QUICKSTART.md#vs-code-first-hour",
        "docs/QUICKSTART.md#ci-first-hour",
        "docs/COMMAND_HIERARCHY.md",
    ] {
        assert_contains("README task navigation", ROOT_README, target)?;
    }
    for (heading, command) in [
        ("## CLI First Hour", "ripr check"),
        ("## CI First Hour", "ripr init --ci github"),
        ("## Agent Or Reviewer First Hour", "ripr pilot --root ."),
    ] {
        let section = doc_section(QUICKSTART_DOC, heading)?;
        if first_bash_block(&section)? != command {
            return Err(format!(
                "Quickstart `{heading}` must start with `{command}`"
            ));
        }
    }
    let repair = doc_section(QUICKSTART_DOC, "## Agent Or Reviewer First Hour")?;
    for target in [
        "--phase before",
        "--attempt",
        "--phase after",
        "ripr agent status --root .",
        "REPAIR_ATTEMPT.md#governed-python-sequence",
    ] {
        assert_contains("Quickstart repair continuation", &repair, target)?;
    }
    Ok(())
}

/// #2930 drift rule: the discovery chain (#4873, #4962, #4965, #4971) shipped
/// after #2931 closed the original prose-alignment claim, so the hierarchy
/// guide must name the landed discovery surfaces in its drift rule and its
/// help row, and must not keep deferring them to #1613 as future work.
#[test]
fn hierarchy_doc_points_at_landed_discovery_surfaces() -> Result<(), String> {
    let drift_rule = doc_section(COMMAND_HIERARCHY_DOC, "## Drift rule")?;
    for needle in ["RIPR-SPEC-0187", "RIPR-SPEC-0189", "RIPR-SPEC-0190"] {
        assert_contains("docs/COMMAND_HIERARCHY.md drift rule", &drift_rule, needle)?;
    }
    if drift_rule.contains("remain tracked") {
        return Err(
            "docs/COMMAND_HIERARCHY.md drift rule still defers shipped discovery surfaces as future work"
                .to_string(),
        );
    }
    let help_row = COMMAND_HIERARCHY_DOC
        .lines()
        .find(|line| line.contains("Read detailed help"))
        .ok_or_else(|| "command guide lost task row `Read detailed help`".to_string())?;
    assert_contains(
        "docs/COMMAND_HIERARCHY.md help row",
        help_row,
        "ripr help workflow",
    )?;
    Ok(())
}

#[test]
fn doc_routes_allow_editorial_rewording_and_table_spacing() -> Result<(), String> {
    let reworded = COMMAND_HIERARCHY_DOC
        .replace(
            "Static findings, or an explicit no-action or limited result.",
            "Findings for the selected change, with any limits disclosed.",
        )
        .replace("| Inspect one change |", "|   Inspect one change   |")
        .replace("`ripr check`", "`ripr   check`")
        .replace('\n', "\r\n");
    assert_doc_command_routes(&reworded)?;
    if first_bash_block("```bash\r\nripr   check\r\n```\r\n")? != "ripr check" {
        return Err("first-run matching must allow whitespace changes".to_string());
    }
    Ok(())
}

#[test]
fn doc_routes_reject_missing_wrong_and_duplicate_tasks() -> Result<(), String> {
    let missing = COMMAND_HIERARCHY_DOC
        .lines()
        .filter(|line| !line.starts_with("| Inspect one change |"))
        .collect::<Vec<_>>()
        .join("\n");
    let wrong = COMMAND_HIERARCHY_DOC.replace(
        "| Inspect one change | `ripr check` |",
        "| Inspect one change | `ripr pilot` |",
    );
    let duplicate =
        format!("{COMMAND_HIERARCHY_DOC}\n| Inspect one change | `ripr check` | Duplicate. |\n");
    for (case, changed) in [
        ("missing", missing),
        ("wrong", wrong),
        ("duplicate", duplicate),
    ] {
        if assert_doc_command_routes(&changed).is_ok() {
            return Err(format!("doc route guard accepted the {case} task mutation"));
        }
    }
    Ok(())
}

#[test]
fn first_run_guard_does_not_credit_a_later_correct_example() -> Result<(), String> {
    let changed = "```bash\nripr pilot\n```\nLater:\n```bash\nripr check\n```\n";
    if first_bash_block(changed)? == "ripr check" {
        return Err("a later command must not hide a wrong first-run route".to_string());
    }
    let cli = doc_section(QUICKSTART_DOC, "## CLI First Hour")?;
    let wrong = cli.replacen("ripr check", "ripr pilot", 1);
    if first_bash_block(&wrong)? == "ripr check" {
        return Err(
            "the CLI section credited a later check instead of its first command".to_string(),
        );
    }
    Ok(())
}

/// PR #4196 review: the doctor exit-code guide must describe both profiles.
/// The default analysis profile keeps a missing or old toolchain advisory
/// (exit `0`); only `--profile source-build` fails on it. A guide that still
/// says a Rust root fails on a missing toolchain contradicts the binary.
#[test]
fn doctor_exit_code_guide_distinguishes_analysis_and_source_build() -> Result<(), String> {
    let section = normalized(&doc_section(EXIT_CODES_DOC, "## `ripr doctor` exit codes")?);
    let help = normalized(&rendered_help(&["doctor", "--help"])?);
    for needle in [
        "--profile source-build",
        "`advisory`",
        "does not change the exit code",
    ] {
        assert_contains("docs/EXIT_CODES.md doctor section", &section, needle)?;
    }
    assert_contains(
        "ripr doctor --help",
        &help,
        "--profile analysis|source-build",
    )?;
    if section.contains("fail on a missing manifest or toolchain") {
        return Err(
            "docs/EXIT_CODES.md still says the default doctor fails on a missing toolchain"
                .to_string(),
        );
    }
    Ok(())
}

/// Every flag a public guide passes to a `ripr` command must be an option that
/// command's rendered help lists. The parser/help direction is already pinned
/// (#2342); this pins the guide/help direction so a renamed or removed flag
/// cannot survive in copy-paste examples.
#[test]
fn public_docs_only_pass_flags_the_command_help_lists() -> Result<(), String> {
    let mut cache = std::collections::BTreeMap::new();
    let drift = undocumented_doc_flags(PUBLIC_COMMAND_DOCS, &mut cache, &run_ripr)?;
    if !drift.is_empty() {
        return Err(format!(
            "documented flags drifted from the CLI:\n{}",
            drift.join("\n")
        ));
    }
    let checked = cache.values().filter(|help| help.is_some()).count();
    if checked < 10 {
        return Err(format!(
            "doc flag guard resolved only {checked} command help screens; the extractor lost its subjects"
        ));
    }
    Ok(())
}

const PORTABLE_TESTS: &[&str] = &[
    "workflow_help_lists_the_five_governed_identities",
    "workflow_help_renders_one_workflow_with_bounded_sections",
    "unknown_workflow_fails_with_a_family_distinct_from_unknown_command",
    "default_help_keeps_the_task_roles_distinct",
    "exhaustive_help_keeps_the_same_roles_and_boundaries",
    "exhaustive_help_marks_non_public_rows_visibly",
    "help_screens_state_the_surfaces_the_parsers_accept",
    "outcome_help_matches_the_enforced_canonical_gap_contract",
    "agent_help_makes_repair_primary_without_removing_control_surfaces",
    "agent_repair_help_names_the_primary_transaction_and_its_limits",
    "agent_status_help_names_the_selected_store_and_exact_attempt_selection",
    "help_index_routes_agent_repair_to_repair_help",
    "mode_help_names_index_scope_and_points_at_the_mode_table",
    "check_and_diff_help_state_the_real_base_default",
    "check_help_chooses_one_format_per_task",
    "doc_flag_guard_rejects_removed_flags_and_accepts_prose",
];

#[test]
fn packaged_help_target_compiles_and_runs_every_portable_case() -> Result<(), String> {
    use ripr_help::{Scratch, compile_portable, extracted_package, require_success, run};
    let scratch = Scratch::new("packaged-help-positive")?;
    let package = extracted_package(&scratch.0)?;
    let executable = scratch
        .0
        .join(format!("help-tests{}", std::env::consts::EXE_SUFFIX));
    require_success(
        compile_portable(&package, &executable)?,
        "extracted help compile",
    )?;
    let mut list = std::process::Command::new(&executable);
    list.arg("--list").current_dir(&scratch.0);
    let inventory = require_success(run(list)?, "extracted help inventory")?;
    let inventory = String::from_utf8(inventory.stdout).map_err(|error| error.to_string())?;
    let actual = inventory
        .lines()
        .filter_map(|line| line.strip_suffix(": test"))
        .collect::<std::collections::BTreeSet<_>>();
    let expected = PORTABLE_TESTS
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    if actual != expected || actual.len() != 16 {
        return Err(format!(
            "portable test inventory drifted: expected {expected:?}, got {actual:?}"
        ));
    }
    let mut execute = std::process::Command::new(&executable);
    execute.arg("--test-threads=1").current_dir(&scratch.0);
    let result = require_success(run(execute)?, "extracted help execution")?;
    let text = String::from_utf8(result.stdout).map_err(|error| error.to_string())?;
    if !text.contains("test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out")
    {
        return Err(format!(
            "extracted target did not run all 16 portable cases:\n{text}"
        ));
    }
    Ok(())
}

#[test]
fn packaged_help_target_rejects_a_missing_shared_test_asset() -> Result<(), String> {
    use ripr_help::{Scratch, compile_portable, extracted_package, require_success};
    let scratch = Scratch::new("packaged-help-negative")?;
    let package = extracted_package(&scratch.0)?;
    let pristine = scratch
        .0
        .join(format!("pristine-tests{}", std::env::consts::EXE_SUFFIX));
    require_success(
        compile_portable(&package, &pristine)?,
        "negative-control precondition",
    )?;
    let helper = package.join("tests/common/help_contract.rs");
    std::fs::remove_file(&helper).map_err(|error| error.to_string())?;
    let broken = scratch
        .0
        .join(format!("broken-tests{}", std::env::consts::EXE_SUFFIX));
    let output = compile_portable(&package, &broken)?;
    let diagnostic = String::from_utf8_lossy(&output.stderr);
    if output.status.success() || broken.exists() || !diagnostic.contains("help_contract.rs") {
        return Err(format!(
            "removed helper did not reject the package compile ({})\n{diagnostic}",
            output.status
        ));
    }
    Ok(())
}
