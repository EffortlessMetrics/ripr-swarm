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
const FIRST_RUN_REPORT_TEMPLATE: &str =
    include_str!("../../.github/ISSUE_TEMPLATE/first_run_report.yml");
const WRONG_VERDICT_TEMPLATE: &str = include_str!("../../.github/ISSUE_TEMPLATE/wrong_verdict.yml");

/// The templates live in this repo. GitHub ignores an unknown `template=`
/// query, so sending readers to EffortlessMetrics/ripr opens a blank form.
const SWARM_FIRST_RUN_REPORT: &str =
    "https://github.com/EffortlessMetrics/ripr-swarm/issues/new?template=first_run_report.yml";
const SWARM_WRONG_VERDICT: &str =
    "https://github.com/EffortlessMetrics/ripr-swarm/issues/new?template=wrong_verdict.yml";
const RIPR_FIRST_RUN_REPORT: &str =
    "https://github.com/EffortlessMetrics/ripr/issues/new?template=first_run_report.yml";
const RIPR_WRONG_VERDICT: &str =
    "https://github.com/EffortlessMetrics/ripr/issues/new?template=wrong_verdict.yml";

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

/// The C2 `cmd:feedback.record` example in
/// `crates/ripr/src/cli/command_metadata.rs`. Bare `ripr feedback record`
/// always refuses without `--snapshot` and `--reason`.
const FEEDBACK_RECORD_EXAMPLE: &str = "ripr feedback record --snapshot ID --reason CODE";

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
        ("Record result usefulness", FEEDBACK_RECORD_EXAMPLE),
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
    let bare_feedback = COMMAND_HIERARCHY_DOC.replace(
        &format!("`{FEEDBACK_RECORD_EXAMPLE}`"),
        "`ripr feedback record`",
    );
    for (case, changed) in [
        ("missing", missing),
        ("wrong", wrong),
        ("duplicate", duplicate),
        ("bare-feedback", bare_feedback),
    ] {
        if assert_doc_command_routes(&changed).is_ok() {
            return Err(format!("doc route guard accepted the {case} task mutation"));
        }
    }
    Ok(())
}

/// Keep the hierarchy task row on the same spelling the typed catalog example
/// already ships. A docs-only re-pin of the refusing bare command must fail.
#[test]
fn hierarchy_feedback_record_row_matches_the_c2_example() -> Result<(), String> {
    const COMMAND_METADATA: &str = include_str!("../../crates/ripr/src/cli/command_metadata.rs");
    let example_field = format!("example: \"{FEEDBACK_RECORD_EXAMPLE}\"");
    if !COMMAND_METADATA.contains(&example_field) {
        return Err(format!(
            "command_metadata.rs lost the C2 example `{FEEDBACK_RECORD_EXAMPLE}`"
        ));
    }
    assert_doc_command_routes(COMMAND_HIERARCHY_DOC)?;
    Ok(())
}

/// Live default-help dispatch, not the HELP constant: `ripr`, `--help`, `-h`,
/// and `help` must share one unqualified first-run command.
fn canonical_first_run_command(help: &str) -> Result<String, String> {
    let lf = help.replace("\r\n", "\n");
    let Some((_, after)) = lf.split_once("Try this first:\n") else {
        return Err("default help lost its first-run section".to_string());
    };
    let section = after.split("\n\n").next().unwrap_or_default();
    let commands: Vec<_> = section
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("ripr "))
        .collect();
    let [line] = commands.as_slice() else {
        return Err(format!(
            "first run must name exactly one command, got:\n{section}"
        ));
    };
    let command = line.split_once("  ").map(|(part, _)| part).unwrap_or(line);
    if normalized(command) != "ripr check" {
        return Err(format!(
            "first run must be `ripr check` without a fixed base, got `{command}`"
        ));
    }
    if section.contains("--base") {
        return Err("first-run command must not hardcode --base".to_string());
    }
    Ok(command.to_string())
}

#[test]
fn rendered_default_help_routes_share_one_canonical_first_run() -> Result<(), String> {
    for args in [
        [].as_slice(),
        ["--help"].as_slice(),
        ["-h"].as_slice(),
        ["help"].as_slice(),
    ] {
        let stdout = rendered_help(args)?;
        if stdout.replace("\r\n", "\n").lines().count() > 40 {
            return Err(format!(
                "default help {args:?} exceeded the 40-line screen budget"
            ));
        }
        canonical_first_run_command(&stdout)?;
    }
    Ok(())
}

#[test]
fn first_run_guard_does_not_credit_a_later_correct_example() -> Result<(), String> {
    let changed = "```bash\nripr pilot\n```\nLater:\n```bash\nripr check\n```\n";
    if first_bash_block(changed)? == "ripr check" {
        return Err("a later command must not hide a wrong first-run route".to_string());
    }
    for (surface, section, expected) in [
        (
            "README",
            doc_section(ROOT_README, "## The first useful run")?,
            "cargo install ripr ripr check",
        ),
        (
            "Quickstart CLI",
            doc_section(QUICKSTART_DOC, "## CLI First Hour")?,
            "ripr check",
        ),
    ] {
        for command in [
            "ripr doctor",
            "ripr pilot --root .",
            "ripr check --base origin/main",
        ] {
            let wrong = section.replacen("\nripr check\n", &format!("\n{command}\n"), 1);
            if wrong == section {
                return Err(format!(
                    "{surface} negative control did not change a command"
                ));
            }
            if first_bash_block(&wrong)? == expected {
                return Err(format!(
                    "{surface} first-run guard accepted `{command}` before a later correct example"
                ));
            }
        }
    }
    Ok(())
}

/// First-run usability: a clean checkout needs a tracked source edit, and a
/// new file needs `git add -N PATH`. Command-only checks stay green if this
/// note is deleted (#8179 review 5543109598).
fn assert_clean_checkout_guidance(surface: &str, text: &str) -> Result<(), String> {
    let n = normalized(text);
    for needle in ["tracked source", "git add -N PATH"] {
        assert_contains(surface, &n, needle)?;
    }
    if !(n.contains("clean base") || n.contains("clean checkout")) {
        return Err(format!(
            "{surface} lost clean-base-branch first-run guidance"
        ));
    }
    for forbidden in ["clean bill of health", "commit your changes first"] {
        if n.contains(forbidden) {
            return Err(format!(
                "{surface} presents `{forbidden}` as first-run guidance"
            ));
        }
    }
    Ok(())
}

fn without_clean_checkout_note(text: &str) -> String {
    text.replace("git add -N PATH", "")
        .replace("tracked source", "")
}

#[test]
fn first_run_surfaces_require_clean_checkout_guidance() -> Result<(), String> {
    let readme = doc_section(ROOT_README, "## The first useful run")?;
    let quickstart = doc_section(QUICKSTART_DOC, "## CLI First Hour")?;
    let mut surfaces = vec![
        ("README first useful run".to_string(), readme),
        ("Quickstart CLI First Hour".to_string(), quickstart),
    ];
    for args in [
        [].as_slice(),
        ["--help"].as_slice(),
        ["-h"].as_slice(),
        ["help"].as_slice(),
    ] {
        surfaces.push((
            format!("rendered default help {args:?}"),
            rendered_help(args)?,
        ));
    }
    for (surface, text) in &surfaces {
        assert_clean_checkout_guidance(surface, text)?;
        let removed = without_clean_checkout_note(text);
        if assert_clean_checkout_guidance(surface, &removed).is_ok() {
            return Err(format!(
                "{surface} still passed after removing tracked-edit / git add -N PATH guidance"
            ));
        }
        let wrong = format!("{text}\nAn empty diff is a clean bill of health.\n");
        if assert_clean_checkout_guidance(surface, &wrong).is_ok() {
            return Err(format!(
                "{surface} accepted empty-diff-as-clean-bill-of-health guidance"
            ));
        }
    }
    let help = rendered_help(&["--help"])?;
    canonical_first_run_command(&without_clean_checkout_note(&help))?;
    Ok(())
}

/// #7661: troubleshooting report links must open the swarm templates that
/// exist in this repo. The README already routes problem reports to
/// EffortlessMetrics/ripr-swarm/issues; QUICKSTART must not send the same
/// reader to EffortlessMetrics/ripr, which has neither template.
fn markdown_link(label: &str, url: &str) -> String {
    format!("[{label}]({url})")
}

fn assert_quickstart_report_links(doc: &str) -> Result<(), String> {
    let troubleshooting = doc_section(doc, "## Troubleshooting")?;
    for (label, url) in [
        ("first run report", SWARM_FIRST_RUN_REPORT),
        ("wrong finding report", SWARM_WRONG_VERDICT),
    ] {
        assert_contains(
            "docs/QUICKSTART.md troubleshooting",
            &troubleshooting,
            &markdown_link(label, url),
        )?;
    }
    for forbidden in [RIPR_FIRST_RUN_REPORT, RIPR_WRONG_VERDICT] {
        if troubleshooting.contains(forbidden) {
            return Err(format!(
                "docs/QUICKSTART.md troubleshooting still opens `{forbidden}`; those templates exist only on EffortlessMetrics/ripr-swarm"
            ));
        }
    }
    Ok(())
}

#[test]
fn quickstart_report_links_open_the_swarm_templates() -> Result<(), String> {
    assert_quickstart_report_links(QUICKSTART_DOC)?;
    assert_contains(
        "README.md problem reports",
        ROOT_README,
        "https://github.com/EffortlessMetrics/ripr-swarm/issues",
    )?;
    assert_contains(
        ".github/ISSUE_TEMPLATE/first_run_report.yml",
        FIRST_RUN_REPORT_TEMPLATE,
        "name: First run report",
    )?;
    assert_contains(
        ".github/ISSUE_TEMPLATE/wrong_verdict.yml",
        WRONG_VERDICT_TEMPLATE,
        "name: Wrong or misleading finding",
    )?;
    Ok(())
}

#[test]
fn quickstart_report_links_reject_the_release_repo_and_a_dropped_close() -> Result<(), String> {
    let release_repo = QUICKSTART_DOC
        .replace(SWARM_FIRST_RUN_REPORT, RIPR_FIRST_RUN_REPORT)
        .replace(SWARM_WRONG_VERDICT, RIPR_WRONG_VERDICT);
    if assert_quickstart_report_links(&release_repo).is_ok() {
        return Err("report-link guard accepted EffortlessMetrics/ripr template URLs".to_string());
    }
    let dropped = QUICKSTART_DOC.replace(
        "Still stuck, or ripr gave a wrong or confusing answer?",
        "Still stuck?",
    );
    let without_close = dropped
        .lines()
        .filter(|line| !line.contains("issues/new?template="))
        .collect::<Vec<_>>()
        .join("\n");
    if assert_quickstart_report_links(&without_close).is_ok() {
        return Err(
            "report-link guard accepted a troubleshooting section with no report URLs".to_string(),
        );
    }
    let unclickable = QUICKSTART_DOC
        .replace(
            &markdown_link("first run report", SWARM_FIRST_RUN_REPORT),
            &format!(
                "[first run report](https://github.com/EffortlessMetrics/ripr-swarm/issues) {SWARM_FIRST_RUN_REPORT}"
            ),
        )
        .replace(
            &markdown_link("wrong finding report", SWARM_WRONG_VERDICT),
            &format!(
                "[wrong finding report](https://github.com/EffortlessMetrics/ripr-swarm/issues) {SWARM_WRONG_VERDICT}"
            ),
        );
    if assert_quickstart_report_links(&unclickable).is_ok() {
        return Err(
            "report-link guard credited a plain-text swarm URL whose markdown link pointed elsewhere"
                .to_string(),
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

/// #7450: the targeted-test workflow must produce cargo-mutants artifacts,
/// not assume `target/ripr/workflow/cargo-mutants.json` already exists.
#[test]
fn targeted_test_workflow_names_the_cargo_mutants_producer() -> Result<(), String> {
    let doc = include_str!("../../docs/TARGETED_TEST_WORKFLOW.md");
    let help = rendered_help(&["calibrate", "cargo-mutants", "--help"])?;
    for needle in [
        "cargo install cargo-mutants --locked",
        "cargo mutants",
        "mutants.out",
    ] {
        assert_contains("docs/TARGETED_TEST_WORKFLOW.md", doc, needle)?;
        assert_contains("ripr calibrate cargo-mutants --help", &help, needle)?;
    }
    if doc.contains("target/ripr/workflow/cargo-mutants.json") {
        return Err(
            "docs/TARGETED_TEST_WORKFLOW.md still points at a producer-less cargo-mutants.json path"
                .to_string(),
        );
    }
    Ok(())
}

/// #7451: naming mutants.json as a valid path is not the denominator
/// caveat. Help and the targeted-test workflow must say a mutants.json-only
/// import counts unexecuted candidates in matched_total.
#[test]
fn targeted_test_workflow_discloses_mutants_json_only_denominator() -> Result<(), String> {
    let doc = include_str!("../../docs/TARGETED_TEST_WORKFLOW.md");
    let help = rendered_help(&["calibrate", "cargo-mutants", "--help"])?;
    for needle in [
        "mutants.json-only",
        "unexecuted candidates",
        "matched_total",
        "no runtime observation",
    ] {
        assert_contains("docs/TARGETED_TEST_WORKFLOW.md", doc, needle)?;
        assert_contains("ripr calibrate cargo-mutants --help", &help, needle)?;
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
    "calibrate_help_names_how_to_produce_the_imported_artifacts",
    "calibrate_help_discloses_mutants_json_only_denominator",
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
    if actual != expected || actual.len() != 18 {
        return Err(format!(
            "portable test inventory drifted: expected {expected:?}, got {actual:?}"
        ));
    }
    let mut execute = std::process::Command::new(&executable);
    execute.arg("--test-threads=1").current_dir(&scratch.0);
    let result = require_success(run(execute)?, "extracted help execution")?;
    let text = String::from_utf8(result.stdout).map_err(|error| error.to_string())?;
    if !text.contains("test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out")
    {
        return Err(format!(
            "extracted target did not run all 18 portable cases:\n{text}"
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
