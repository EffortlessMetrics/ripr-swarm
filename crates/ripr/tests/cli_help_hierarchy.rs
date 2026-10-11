//! Rendered command help contract, portable in the published source package.
//! Repository guide audits live in the unpublished xtask package.

use std::process::Command;

#[path = "common/help_contract.rs"]
mod help_contract;
use help_contract::{assert_contains, normalized, undocumented_doc_flags};

fn run_ripr(args: &[&str]) -> Result<std::process::Output, String> {
    Command::new(env!("CARGO_BIN_EXE_ripr"))
        .args(args)
        .output()
        .map_err(|error| format!("failed to run ripr {args:?}: {error}"))
}

fn rendered_help(args: &[&str]) -> Result<String, String> {
    let output = run_ripr(args)?;
    if !output.status.success() {
        return Err(format!(
            "ripr {args:?} failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    String::from_utf8(output.stdout)
        .map_err(|error| format!("ripr {args:?} emitted non-UTF-8 stdout: {error}"))
}

#[test]
fn workflow_help_lists_the_five_governed_identities() -> Result<(), String> {
    let stdout = rendered_help(&["help", "workflow"])?;
    for id in [
        "inspect-change",
        "guided-adoption",
        "repair-gap",
        "compose-pr-evidence",
        "adopt-ci",
    ] {
        assert_contains("`ripr help workflow`", &stdout, id)?;
    }
    assert_contains(
        "`ripr help workflow`",
        &stdout,
        "Run `ripr help workflow <name>` for one workflow's steps",
    )?;
    // The listing is guidance only: it must not silently execute anything.
    assert_contains(
        "`ripr help workflow`",
        &stdout,
        "Nothing on this screen runs a command",
    )
}

#[test]
fn workflow_help_renders_one_workflow_with_bounded_sections() -> Result<(), String> {
    let stdout = rendered_help(&["help", "workflow", "repair-gap"])?;
    for section in [
        "Workflow: repair-gap",
        "Purpose:",
        "Applies when:",
        "Commands (ordered):",
        "Result families:",
        "Artifacts:",
        "Recovery:",
        "Stop conditions:",
        "Limitations:",
    ] {
        assert_contains("`ripr help workflow repair-gap`", &stdout, section)?;
    }
    // Aliases resolve on the render path.
    assert_contains(
        "`ripr help workflow repair-gap`",
        &rendered_help(&["help", "workflow", "adoption"])?,
        "Workflow: guided-adoption",
    )?;
    Ok(())
}

#[test]
fn unknown_workflow_fails_with_a_family_distinct_from_unknown_command() -> Result<(), String> {
    let output = run_ripr(&["help", "workflow", "repar-gap"])?;
    if output.status.success() {
        return Err(format!(
            "unknown workflow exited 0\nstdout:\n{}",
            String::from_utf8_lossy(&output.stdout)
        ));
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stderr.contains("unknown workflow") {
        return Err(format!(
            "unknown workflow error missing its family marker:\n{stderr}"
        ));
    }
    if stderr.contains("unknown command") {
        return Err(format!(
            "unknown workflow error leaked the unknown-command family:\n{stderr}"
        ));
    }
    // Suggestions, when present, must come from workflow identities only and
    // must not suggest a command spelling for a workflow typo.
    if stderr.contains("repair-loop") || stderr.contains("ripr agent") {
        return Err(format!(
            "unknown workflow error suggested command machinery instead of a workflow:\n{stderr}"
        ));
    }
    Ok(())
}

#[test]
fn default_help_keeps_the_task_roles_distinct() -> Result<(), String> {
    for args in [["--help"].as_slice(), ["help"].as_slice()] {
        let stdout = normalized(&rendered_help(args)?);
        for needle in [
            "Diagnose setup ripr doctor",
            "Inspect one change ripr check",
            "Guided repo adoption ripr pilot --root .",
            "Repair one named gap ripr repair",
            "Compose PR evidence ripr first-pr",
            "Adopt advisory CI ripr init --ci github",
            "ripr help --all",
            "ripr help --json",
        ] {
            assert_contains(
                "default help (`ripr --help` / `ripr help`)",
                &stdout,
                needle,
            )?;
        }
    }
    Ok(())
}

#[test]
fn exhaustive_help_keeps_the_same_roles_and_boundaries() -> Result<(), String> {
    let stdout = normalized(&rendered_help(&["help", "--all"])?);
    for needle in [
        "Diagnose setup ripr doctor",
        "Inspect one change ripr check",
        "Guided repo adoption ripr pilot --root .",
        "Repair one named gap ripr repair [<item>] | ripr continue | ripr status",
        "Compose PR evidence ripr first-pr --root . --base BASE --head HEAD",
        "Adopt advisory CI ripr init --ci github",
        "`ripr check` is the ordinary first-value analysis; `ripr pilot` is the guided repo-adoption workflow.",
        "`ripr first-pr` and `ripr start-here` compose `target/ripr/reports/start-here.{json,md}` from existing artifacts; they do not run analysis or repair a gap.",
        "ripr help --json",
        "this route accepts no other arguments",
        "except on ripr help --json",
    ] {
        assert_contains("exhaustive help (`ripr help --all`)", &stdout, needle)?;
    }
    Ok(())
}

/// Non-public rows are visibly distinct in the exhaustive reference
/// (issue #4823): advanced rows carry an `[advanced]` line marker and the
/// compatibility alias carries `[compatibility]`, so a reader can tell the
/// primary surface from control and legacy commands without opening each
/// help page. The projection is validated against the typed tables in
/// `command_metadata::tests::human_surfaces_agree_with_the_typed_tables`.
#[test]
fn exhaustive_help_marks_non_public_rows_visibly() -> Result<(), String> {
    let rendered = rendered_help(&["help", "--all"])?;
    let stdout = normalized(&rendered);
    for needle in [
        "ripr agent repair --root . --seam-id ID --phase before [advanced]",
        "ripr agent repair --root . (--attempt ID | --seam-id ID) --phase after [advanced]",
        "ripr agent repair --root . --attempt ID --phase verify --verify-authorized --verify-authority ID [advanced]",
        "ripr agent status --root . [--json] [advanced]",
        "ripr agent start --root . --seam-id ID [--out target/ripr/workflow] [advanced]",
        "ripr agent brief --root . (--diff PATH|--base REV|--files PATHS|--seam-id ID) --json [advanced]",
        "ripr agent packet --root . (--seam-id ID | --gap-ledger PATH --gap-id ID) --json [advanced]",
        "ripr agent card --root . --seam-id ID [--json] [advanced]",
        "ripr agent verify --root . --before before.json --after after.json --json [advanced]",
        "ripr agent verify-execute --root . --packet packet.json --result-json result.json --authorize --json [advanced]",
        "ripr agent receipt --root . --verify-json agent-verify.json --seam-id ID --json [advanced]",
        "ripr agent review-summary --root . [--json] [advanced]",
        "ripr start-here [same options as first-pr] [compatibility]",
    ] {
        assert_contains("exhaustive help (`ripr help --all`)", &stdout, needle)?;
    }
    // The primary public rows stay unmarked. Inspect each original rendered
    // row: a class marker anywhere on the row (not just adjacent to the
    // route) is a leak, and the canonical route must still be present.
    let rendered_rows: Vec<String> = rendered.lines().map(normalized).collect();
    for needle in [
        "ripr check [--base REV] [--worktree] [--diff PATH] [--mode draft] [--format FORMAT]",
        "ripr repair [<item>] [--root PATH]",
        "ripr continue [--attempt ID] [--root PATH]",
        "ripr status [--attempt ID] [--root PATH] [--json]",
    ] {
        let mut found = false;
        for row in &rendered_rows {
            if row.contains(needle) {
                found = true;
                for marker in ["[advanced]", "[compatibility]"] {
                    if row.contains(marker) {
                        return Err(format!(
                            "public row unexpectedly carries a class marker: {needle} {marker}"
                        ));
                    }
                }
            }
        }
        if !found {
            return Err(format!(
                "exhaustive help (`ripr help --all`) lost the canonical route `{needle}`"
            ));
        }
    }
    Ok(())
}

/// The help screens are the exhaustive reference, so a line on one is a claim
/// about what the command accepts. Each needle below was wrong against source
/// until this test existed: the repair rows printed `--phase verify` forms that
/// always fail (the verify phase refuses without `--verify-authorized` and
/// `--verify-authority`, and takes only `--attempt`), two report rows named
/// default outputs the producers never write, and four rows omitted selectors
/// the parsers accept. The negative asserts are the discriminators — they fail
/// on the exact prior wording.
#[test]
fn help_screens_state_the_surfaces_the_parsers_accept() -> Result<(), String> {
    // The default screen is a bounded first screen with a line budget
    // (`help_leads_with_a_bounded_first_screen_that_routes_onward`), so the
    // verify phase does not belong on it at all: it is reachable only for
    // trust-bound Python attempts and needs an authorization pair that would
    // not fit. Its repair teaser is before -> edit -> after, the ordinary Rust
    // path, and the full invocation lives in `help --all` and
    // `agent repair --help`.
    let default_help = normalized(&rendered_help(&["--help"])?);
    for needle in ["ripr repair [<item>]", "ripr continue"] {
        assert_contains("default help (`ripr --help`)", &default_help, needle)?;
    }
    if default_help.contains("ripr agent repair --attempt ID --phase verify") {
        return Err(
            "the bounded first screen must not print a verify invocation it cannot make runnable"
                .to_string(),
        );
    }

    let all = normalized(&rendered_help(&["help", "--all"])?);
    for needle in [
        // The verify phase takes `--attempt`, never `--seam-id`, and refuses
        // without both authorization signals.
        "ripr agent repair --root . --attempt ID --phase verify --verify-authorized --verify-authority ID",
        "ripr agent repair --root . (--attempt ID | --seam-id ID) --phase after",
        // Producer defaults, from typescript_limitations.rs and
        // typescript_false_actionable.rs.
        "target/ripr/reports/typescript-limitations.json",
        "target/ripr/reports/typescript-false-actionable-audit.json",
        // `gate evaluate --help` lists four modes; this screen listed two.
        "--mode visible-only|acknowledgeable|baseline-check|calibrated-gate",
        // Selectors the parsers accept that this screen did not name.
        "ripr reports gap-ledger (--records PATH | --repo-exposure PATH | --check-output PATH)",
        "ripr agent packet --root . (--seam-id ID | --gap-ledger PATH --gap-id ID) --json",
        "--gap CANONICAL_GAP_ID --gap-ledger PATH",
        // `ripr mcp --help` states `--stdio` is optional and the default.
        "ripr mcp [--stdio] [--root PATH]",
    ] {
        assert_contains("exhaustive help (`ripr help --all`)", &all, needle)?;
    }

    // Discriminators. The `Repair one named gap` role line is the task-first
    // alternation `repair | continue | status`: it names the ordinary
    // commands rather than the internal phase vocabulary, and
    // `default_help_keeps_the_task_roles_distinct` pins it as role
    // vocabulary.
    for (surface, text, stale) in [
        (
            "exhaustive help",
            &all,
            "ripr agent repair --root . --seam-id ID --phase before|after|verify",
        ),
        (
            "exhaustive help",
            &all,
            "target/ripr/reports/ts-limitations.json",
        ),
        (
            "exhaustive help",
            &all,
            "target/ripr/reports/ts-false-actionable.json",
        ),
        (
            "exhaustive help",
            &all,
            "--mode visible-only|acknowledgeable]",
        ),
        ("exhaustive help", &all, "ripr mcp --stdio [--root PATH]"),
    ] {
        if text.contains(stale) {
            return Err(format!("{surface} still prints the stale form `{stale}`"));
        }
    }
    Ok(())
}

/// #7202 drift guard. `ripr outcome --help` must describe the canonical-gap
/// refusal as it is enforced: Rust `ripr check --json` findings carry
/// `canonical_gap_id` and are compared by it, and the refusal keys on
/// findings that lack one. The help once claimed Rust check JSON "is refused
/// rather than compared" outright, sending agents down a needless
/// `repo-exposure-json` detour for a path that already worked.
#[test]
fn outcome_help_matches_the_enforced_canonical_gap_contract() -> Result<(), String> {
    let help = normalized(&rendered_help(&["outcome", "--help"])?);
    assert_contains(
        "outcome help (`ripr outcome --help`)",
        &help,
        "Rust `ripr check --json` findings carry canonical_gap_id and are compared by it",
    )?;
    assert_contains(
        "outcome help (`ripr outcome --help`)",
        &help,
        "Check output whose findings carry no canonical_gap_id is refused rather than compared",
    )?;
    if help.contains("no canonical_gap_id (Rust") {
        return Err(
            "outcome help claims Rust check JSON findings lack canonical_gap_id; they carry \
             it and are compared by it (#7202)"
                .to_string(),
        );
    }
    Ok(())
}

#[test]
fn agent_help_makes_repair_primary_without_removing_control_surfaces() -> Result<(), String> {
    for args in [["agent", "--help"].as_slice(), ["agent"].as_slice()] {
        let stdout = rendered_help(args)?;
        let collapsed = normalized(&stdout);
        assert_contains("agent help", &collapsed, "Primary workflow:")?;

        assert_contains(
            "agent help",
            &collapsed,
            "repair Run the before/edit/after repair transaction and its verification phase for one seam.",
        )?;
        assert_contains(
            "agent help",
            &collapsed,
            "status Report existing agent-loop artifacts and the exact next command.",
        )?;
        let advanced = stdout
            .split("Advanced and compatibility workflows:")
            .nth(1)
            .ok_or_else(|| {
                "agent help lost the explicit advanced/compatibility boundary".to_string()
            })?;
        // Match each advanced command as the first token of a listed entry, so
        // a dropped entry cannot hide behind a substring of another command's
        // name or description (`verify` inside `verify-execute`, and so on).
        for command in [
            "start",
            "brief",
            "packet",
            "verify",
            "verify-execute",
            "receipt",
            "review-summary",
        ] {
            let listed = advanced
                .lines()
                .any(|line| line.split_whitespace().next() == Some(command));
            if !listed {
                return Err(format!(
                    "agent help lost the advanced command entry `{command}`"
                ));
            }
        }
    }
    Ok(())
}

/// The primary repair transaction has its own help surface; it must keep the
/// before/edit/after sequence and the explicit limits (no test generation, no
/// mutation execution, no merge authority) that keep the route advisory.
#[test]
fn agent_repair_help_names_the_primary_transaction_and_its_limits() -> Result<(), String> {
    let stdout = normalized(&rendered_help(&["agent", "repair", "--help"])?);
    for needle in [
        "Run the before/edit/after repair transaction and its verification phase for one named gap.",
        "ripr agent repair --seam-id ID --phase before",
        "# edit one focused test outside RIPR",
        "ripr agent repair --attempt ID --phase after",
        "--verify-authorized",
        "--verify-authority ID",
        "--verify-rollback",
        "ripr agent repair [--root PATH] [--store PATH] --seam-id ID --phase before",
        "ripr agent repair [--root PATH] [--store PATH] (--attempt ID|--seam-id ID) --phase after",
        "ripr agent repair [--root PATH] [--store PATH] --attempt ID --phase verify",
        "--store PATH Explicit repair-attempt store, resolved against --root.",
        "The repair command does not generate or apply tests, execute mutation testing, or declare the repository safe to merge.",
        "Inline `#[cfg(test)]` modules don't qualify their file: a repository whose only tests are inline in non-test-surface files is permanently out of repair scope",
    ] {
        assert_contains(
            "agent repair help (`ripr agent repair --help`)",
            &stdout,
            needle,
        )?;
    }
    if stdout.contains("ripr agent repair [--root PATH] --attempt ID --phase verify") {
        return Err(
            "agent repair help still prints verify usage without optional [--store PATH]"
                .to_string(),
        );
    }
    Ok(())
}

#[test]
fn agent_status_help_names_the_selected_store_and_exact_attempt_selection() -> Result<(), String> {
    let stdout = normalized(&rendered_help(&["agent", "status", "--help"])?);
    for needle in [
        "Usage: ripr agent status [--root PATH] [--store PATH] [--attempt ID] [--json] [--out PATH]",
        "--store PATH Explicit repair-attempt store, resolved against --root.",
        "Missing explicit stores do not fall back to the default.",
        "--attempt ID Select exactly one repair attempt by ID and report its",
        "corrupt_or_unavailable result, never as another attempt's state.",
    ] {
        assert_contains(
            "agent status help (`ripr agent status --help`)",
            &stdout,
            needle,
        )?;
    }
    Ok(())
}

/// The `help` index must route a subcommand to that subcommand's own help
/// (#4378): `ripr help agent repair` printed the parent `agent` overview
/// because the injected `--help` preceded `repair`. An unknown subcommand
/// errors instead of printing unrelated help.
#[test]
fn help_index_routes_agent_repair_to_repair_help() -> Result<(), String> {
    let via_index = rendered_help(&["help", "agent", "repair"])?;
    let direct = rendered_help(&["agent", "repair", "--help"])?;
    if via_index != direct {
        return Err(format!(
            "`ripr help agent repair` must print the same help as `ripr agent repair --help`\n\
             help index:\n{via_index}\ndirect:\n{direct}"
        ));
    }
    let unknown = run_ripr(&["help", "agent", "no-such-subcommand"])?;
    if unknown.status.success() {
        return Err(format!(
            "`ripr help agent no-such-subcommand` must fail, got stdout:\n{}",
            String::from_utf8_lossy(&unknown.stdout)
        ));
    }
    assert_contains(
        "unknown agent subcommand via help index",
        &String::from_utf8_lossy(&unknown.stderr),
        "unknown agent subcommand",
    )
}

/// `check` and `context` must say what each `--mode` value means and where
/// the full table lives (#4378), not only list the bare names.
#[test]
fn mode_help_names_index_scope_and_points_at_the_mode_table() -> Result<(), String> {
    for command in ["check", "context"] {
        let help = normalized(&rendered_help(&[command, "--help"])?);
        for needle in [
            "instant (changed files only, cheapest)",
            "deep and ready (whole workspace, slowest)",
            "See docs/CONFIGURATION.md \"Analysis modes\"",
        ] {
            assert_contains(&format!("{command} --help --mode line"), &help, needle)?;
        }
    }
    Ok(())
}

/// `check --help` must teach the loader's real default-base resolution order
/// (#3885), not the old `origin/main` shorthand. `diff` resolves an omitted
/// base through the same authority (#3952), so its help states the same order.
#[test]
fn check_and_diff_help_state_the_real_base_default() -> Result<(), String> {
    let check = normalized(&rendered_help(&["check", "--help"])?);
    for needle in [
        "the local origin/HEAD ref, then origin/main, origin/master, main, and master in order",
        "when none of those resolves, the analysis does not run",
    ] {
        assert_contains("check help (`ripr check --help`)", &check, needle)?;
    }
    if check.contains("Defaults to origin/main") {
        return Err(
            "check help still teaches the origin/main default instead of the resolution order"
                .to_string(),
        );
    }
    let diff = normalized(&rendered_help(&["diff", "--help"])?);
    assert_contains(
        "diff help (`ripr diff --help`)",
        &diff,
        "the local origin/HEAD ref, then origin/main, origin/master, main, and master",
    )?;
    if diff.contains("Defaults to origin/main") {
        return Err("diff help still teaches the origin/main default".to_string());
    }
    Ok(())
}

/// #5211 direction B: `check --help` keeps every format but chooses one
/// per task, so a newcomer maps their job to a format without re-reading
/// the group list. Each task below must keep exactly its recommended path.
#[test]
fn check_help_chooses_one_format_per_task() -> Result<(), String> {
    let check = normalized(&rendered_help(&["check", "--help"])?);
    for needle in [
        "Choose by task:",
        "eye review -> human",
        "every finding with drill-in commands -> human-full",
        "machine consumer (jq, CI scripts) -> json",
        "file annotations in Actions logs -> github",
        "code scanning upload -> sarif",
        "README badge -> repo-badge-shields (repo ledger)",
        "PR/CI status badge -> badge-shields (diff)",
        "whole-repo inventory -> repo-exposure-json",
        "agent repair evidence -> agent-seam-packets-json",
    ] {
        assert_contains("check help (`ripr check --help`)", &check, needle)?;
    }
    // The chooser orients before the group list it summarizes.
    let chooser = check
        .find("Choose by task:")
        .ok_or("chooser missing from check help")?;
    let groups = check
        .find("Analysis (diff-scoped):")
        .ok_or("format groups missing from check help")?;
    if chooser > groups {
        return Err("the task chooser must precede the format groups".to_string());
    }
    // BADGE_ADOPTION.md rule 1: README badges are repo-scoped. The README
    // mapping must name only the repo format; the diff format belongs to
    // the PR/CI mapping.
    let readme = check
        .find("README badge ->")
        .ok_or("README mapping missing from the chooser")?;
    let pr = check
        .find("PR/CI status badge ->")
        .ok_or("PR/CI mapping missing from the chooser")?;
    if readme > pr {
        return Err("the README mapping must precede the PR/CI mapping".to_string());
    }
    let readme_span = &check[readme..pr];
    if !readme_span.contains("repo-badge-shields") {
        return Err("the README mapping must recommend repo-badge-shields".to_string());
    }
    if readme_span.contains("badge-shields (diff)") {
        return Err("the README mapping must not include the diff format".to_string());
    }
    Ok(())
}

#[test]
fn doc_flag_guard_rejects_removed_flags_and_accepts_prose() -> Result<(), String> {
    let mut cache = std::collections::BTreeMap::new();
    let bad = "Run `ripr check --no-such-flag` first.\n";
    let drift = undocumented_doc_flags(&[("bad.md", bad)], &mut cache, &run_ripr)?;
    if drift.len() != 1 || !drift[0].contains("bad.md:1 `ripr check --no-such-flag`") {
        return Err(format!("guard missed a removed check flag: {drift:?}"));
    }
    let sub =
        "```bash\nripr agent repair --root . \\\n  --seam-id ID --phase before --bogus\n```\n";
    let drift = undocumented_doc_flags(&[("sub.md", sub)], &mut cache, &run_ripr)?;
    if drift.len() != 1 || !drift[0].contains("`ripr agent repair --bogus`") {
        return Err(format!(
            "guard missed a continued-line subcommand flag: {drift:?}"
        ));
    }
    let quoted = "Run `ripr check --diff \"a b.patch\" --bogus`.\n";
    let drift = undocumented_doc_flags(&[("quoted.md", quoted)], &mut cache, &run_ripr)?;
    if drift.len() != 1 || !drift[0].contains("`ripr check --bogus`") {
        return Err(format!(
            "guard missed a flag after a quoted value: {drift:?}"
        ));
    }
    let good = "Use ripr check with --json, or `ripr pilot --root .`. The ripr is static; \
                cargo-ripr --x and ripr-swarm --y are not invocations.\n";
    let drift = undocumented_doc_flags(&[("good.md", good)], &mut cache, &run_ripr)?;
    if !drift.is_empty() {
        return Err(format!("guard rejected valid prose: {drift:?}"));
    }
    Ok(())
}
