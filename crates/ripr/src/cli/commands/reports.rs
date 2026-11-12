//! CLI adapter for `ripr reports` index, gap-ledger, and TypeScript routes (#4854).

use crate::cli::commands_options::*;
use crate::cli::commands_timestamps::generated_at_unix_ms;
use crate::cli::help;
use crate::cli::parse::reconcile_root;
use crate::cli::suggest::{unknown_argument, unknown_value};
use crate::output;
use std::path::{Path, PathBuf};

use super::ci_packet;
use super::ci_summary;
use super::kani_report;
use super::{non_empty_path_arg, non_empty_string_arg};
use super::{read_optional_text_for_report, write_text_file};

pub(in crate::cli) fn reports(args: &[String]) -> Result<(), String> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        help::print_group_help("reports", args);
        return Ok(());
    }
    let Some((subcommand, rest)) = args.split_first() else {
        help::print_reports_help();
        return Ok(());
    };
    match subcommand.as_str() {
        "index" => report_packet_index(rest),
        "ci-packet" => ci_packet::ci_packet(rest),
        "ci-summary" => ci_summary::ci_summary(rest),
        "gap-ledger" => gap_decision_ledger(rest),
        "kani" => kani_report::run(rest),
        "ts-limitations" => typescript_limitations(rest),
        "ts-false-actionable" => typescript_false_actionable(rest),
        _ => Err(unknown_value(
            "reports subcommand",
            subcommand,
            &[
                "index",
                "ci-packet",
                "ci-summary",
                "gap-ledger",
                "kani",
                "ts-limitations",
                "ts-false-actionable",
            ],
        )),
    }
}

fn report_packet_index(args: &[String]) -> Result<(), String> {
    let options = parse_report_packet_index_options(args)?;
    let input = output::report_packet_index::ReportPacketIndexInput {
        root: options.root,
        generated_at: report_packet_index_generated_at()?,
        reports_dir: options.reports_dir,
        review_dir: options.review_dir,
        receipts_dir: options.receipts_dir,
        workflow_dir: options.workflow_dir,
        agent_dir: options.agent_dir,
        pilot_dir: options.pilot_dir,
        ci_dir: options.ci_dir,
    };
    let report = output::report_packet_index::build_report_packet_index_report(input);
    let rendered_json = output::report_packet_index::render_report_packet_index_json(&report)?;
    let rendered_md = output::report_packet_index::render_report_packet_index_markdown(&report);
    write_text_file(&options.out, &rendered_json)?;
    write_text_file(&options.out_md, &rendered_md)?;
    println!("Wrote {}", output::path::human_path(&options.out));
    println!("Wrote {}", output::path::human_path(&options.out_md));
    Ok(())
}

fn gap_decision_ledger(args: &[String]) -> Result<(), String> {
    let options = parse_gap_decision_ledger_options(args)?;
    // A root that is not a directory cannot be analyzed, whether or not the
    // source reads: refuse (exit 2 per docs/EXIT_CODES.md) before writing a
    // ledger and exiting 0 as if the run succeeded.
    if !Path::new(&options.root).is_dir() {
        return Err(format!(
            "reports gap-ledger --root {} is not a directory; pass the repository root with --root",
            options.root
        ));
    }
    // Persist the producer's selected directory, not a label that a consumer
    // would resolve in its own invocation directory (#7382).
    let selected_root = std::fs::canonicalize(&options.root).map_err(|error| {
        format!(
            "resolve gap-ledger root {} failed: {error}; pass an existing directory as `--root`",
            options.root
        )
    })?;
    let records_path = ledger_source_path(options.source.path())?;
    // An unreadable source means the command could not complete for the same
    // reason: refuse before writing a `blocked` ledger.
    let records_json = read_optional_text_for_report(options.source.label(), options.source.path())
        .map_err(|error| {
            format!(
                "{error}; generate it first (for example `ripr pilot --root {}`) or pass the correct path",
                crate::agent::loop_commands::shell_arg(&options.root)
            )
        })?;
    let input = output::gap_decision_ledger::GapDecisionLedgerInput {
        // `root` is consumed as a native path. Display escaping would turn
        // a literal `%` into a different directory named `%25` (#7382).
        root: selected_root
            .to_str()
            .ok_or_else(|| {
                format!(
                    "reports gap-ledger root {} resolves to a non-UTF-8 path; select a UTF-8 repository root",
                    options.root
                )
            })?
            .to_string(),
        generated_at: gap_decision_ledger_generated_at()?,
        source_kind: options.source.kind(),
        records_path,
        records_json: Ok(records_json),
    };
    let mut report = output::gap_decision_ledger::build_gap_decision_ledger_report(input);
    output::gap_decision_ledger::stamp_gap_decision_ledger_source_subject(
        &mut report,
        &selected_root,
    )?;
    let rendered_json = output::gap_decision_ledger::render_gap_decision_ledger_json(&report)?;
    let rendered_md = output::gap_decision_ledger::render_gap_decision_ledger_markdown(&report);
    write_text_file(&options.out, &rendered_json)?;
    write_text_file(&options.out_md, &rendered_md)?;
    println!("Wrote {}", output::path::human_path(&options.out));
    println!("Wrote {}", output::path::human_path(&options.out_md));
    // A readable source that is not this kind's input (invalid JSON, an
    // empty file, another artifact) means the command could not complete
    // (exit 2, docs/EXIT_CODES.md). The `blocked` ledger is still written
    // first so it replaces any earlier ledger at `--out`: a stale ledger left
    // in place would be read downstream as this run's result (#14311).
    if let Some(cause) = report.source_parse_error() {
        return Err(gap_ledger_source_refusal(
            &options.source,
            &options.root,
            cause,
        ));
    }
    // A source that parsed but is a producer refusal (`downstream_consumable:
    // false`) means nothing was analyzed, so the ledger cannot be built from
    // it either: same exit 2 after the `blocked` write (#14434).
    if let Some(refusal) = report.source_refusal() {
        // A saved `blocked` ledger keeps `run_limitations[]` routes, but a
        // refusal disclosed only in `analysis_scope` is saved marker-only, so
        // only name a route the source actually records.
        let repair = match report.source_refusal_repair_route() {
            Some(route) => format!(
                "Follow its repair_route `{route}`, re-run the producing check, then rebuild this ledger"
            ),
            None => "It records no repair_route: re-run the check that produced it (with a narrower diff or scope budget if it was refused as oversized), then rebuild this ledger from that fresh output".to_string(),
        };
        return Err(format!(
            "reports gap-ledger {} {} is {refusal}: nothing was analyzed, so the ledger was written as `blocked` with no records. {repair}",
            source_flag(&options.source),
            output::path::human_path(options.source.path()),
        ));
    }
    Ok(())
}

fn source_flag(source: &GapDecisionLedgerSource) -> &'static str {
    match source {
        GapDecisionLedgerSource::Records(_) => "--records",
        GapDecisionLedgerSource::RepoExposure(_) => "--repo-exposure",
        GapDecisionLedgerSource::CheckOutput(_) => "--check-output",
    }
}

fn ledger_source_path(path: &Path) -> Result<String, String> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| format!("resolve gap-ledger source path failed: {error}; run `ripr` from an existing directory (the current one may have been deleted)"))?
            .join(path)
    };
    Ok(output::baseline_delta::display_path(&absolute))
}

/// The refusal for a readable `reports gap-ledger` source that is not a valid
/// input of its kind: the flag and path, the parse cause, that the ledger was
/// written as `blocked`, and the command that produces a valid input (#14311).
fn gap_ledger_source_refusal(source: &GapDecisionLedgerSource, root: &str, cause: &str) -> String {
    let root = crate::agent::loop_commands::shell_arg(root);
    let (flag, expected, producer) = match source {
        GapDecisionLedgerSource::Records(_) => (
            "--records",
            "GapRecord JSON (a records/gap_records/cases object or a record array)",
            "`ripr reports gap-ledger --repo-exposure PATH` or `--check-output PATH` to derive records instead".to_string(),
        ),
        GapDecisionLedgerSource::RepoExposure(_) => (
            "--repo-exposure",
            "repo-exposure JSON",
            format!("`ripr pilot --root {root}` (pass the repo-exposure.json it writes)"),
        ),
        GapDecisionLedgerSource::CheckOutput(_) => (
            "--check-output",
            "`ripr check` JSON output",
            "`ripr check --format json > check.json`".to_string(),
        ),
    };
    // The parsers end their causes with a generic regenerate remedy; this
    // refusal names the concrete producer instead, so keep one remedy.
    let cause = cause
        .strip_suffix("; regenerate the input with the command that wrote it, then rerun")
        .unwrap_or(cause);
    format!(
        "reports gap-ledger {flag} {} could not be read as {expected}: {cause}; the ledger was written as `blocked` with no records. Pass the right file, or generate one with {producer}",
        output::path::human_path(source.path()),
    )
}

fn typescript_limitations(args: &[String]) -> Result<(), String> {
    let options = parse_typescript_limitations_options(args)?;
    // An unreadable check output means the command could not complete:
    // surface the read failure (exit 2 per docs/EXIT_CODES.md) instead of
    // writing a `blocked` report and exiting 0 as if the run succeeded.
    let check_output_json = read_optional_text_for_report("check output", &options.check_output)?;
    let input = output::typescript_limitations::TypeScriptLimitationLeaderboardInput {
        root: options.root,
        generated_at: typescript_limitations_generated_at()?,
        check_output_path: output::baseline_delta::display_path(&options.check_output),
        check_output_json: Ok(check_output_json),
    };
    let report =
        output::typescript_limitations::build_typescript_limitation_leaderboard_report(input);
    let rendered_json =
        output::typescript_limitations::render_typescript_limitation_leaderboard_json(&report)?;
    let rendered_md =
        output::typescript_limitations::render_typescript_limitation_leaderboard_markdown(&report);
    write_text_file(&options.out, &rendered_json)?;
    write_text_file(&options.out_md, &rendered_md)?;
    println!("Wrote {}", output::path::human_path(&options.out));
    println!("Wrote {}", output::path::human_path(&options.out_md));
    Ok(())
}

fn typescript_false_actionable(args: &[String]) -> Result<(), String> {
    let options = parse_typescript_false_actionable_options(args)?;
    // An unreadable corpus means the command could not complete: surface the
    // read failure (exit 2 per docs/EXIT_CODES.md) instead of writing a
    // `blocked` report and exiting 0 as if the run succeeded.
    let corpus_json =
        read_optional_text_for_report("TypeScript false-actionable audit corpus", &options.corpus)?;
    let input = output::typescript_false_actionable::TypeScriptFalseActionableAuditInput {
        root: options.root,
        generated_at: typescript_false_actionable_generated_at()?,
        corpus_path: output::baseline_delta::display_path(&options.corpus),
        corpus_json: Ok(corpus_json),
    };
    let report =
        output::typescript_false_actionable::build_typescript_false_actionable_audit_report(input);
    let rendered_json =
        output::typescript_false_actionable::render_typescript_false_actionable_audit_json(
            &report,
        )?;
    let rendered_md =
        output::typescript_false_actionable::render_typescript_false_actionable_audit_markdown(
            &report,
        );
    write_text_file(&options.out, &rendered_json)?;
    write_text_file(&options.out_md, &rendered_md)?;
    println!("Wrote {}", output::path::human_path(&options.out));
    println!("Wrote {}", output::path::human_path(&options.out_md));
    Ok(())
}

fn parse_report_packet_index_options(args: &[String]) -> Result<ReportPacketIndexOptions, String> {
    let mut root = ".".to_string();
    let mut root_explicit = false;
    let mut reports_dir = PathBuf::from("target/ripr/reports");
    let mut review_dir = PathBuf::from("target/ripr/review");
    let mut receipts_dir = PathBuf::from("target/ripr/receipts");
    let mut workflow_dir = PathBuf::from("target/ripr/workflow");
    let mut agent_dir = PathBuf::from("target/ripr/agent");
    let mut pilot_dir = PathBuf::from("target/ripr/pilot");
    let mut ci_dir = PathBuf::from("target/ci");
    let mut out = PathBuf::from(output::report_packet_index::DEFAULT_REPORT_PACKET_INDEX_OUT);
    let mut out_md = PathBuf::from(output::report_packet_index::DEFAULT_REPORT_PACKET_INDEX_MD_OUT);

    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--root" => {
                i += 1;
                let value = non_empty_string_arg(args, i, "--root", "reports index")?;
                let reconciled = reconcile_root(
                    std::path::Path::new(&root),
                    root_explicit,
                    &value,
                    "reports index",
                )?;
                root = reconciled.display().to_string();
                root_explicit = true;
            }
            "--reports-dir" => {
                i += 1;
                reports_dir = non_empty_path_arg(args, i, "--reports-dir", "reports index")?;
            }
            "--review-dir" => {
                i += 1;
                review_dir = non_empty_path_arg(args, i, "--review-dir", "reports index")?;
            }
            "--receipts-dir" => {
                i += 1;
                receipts_dir = non_empty_path_arg(args, i, "--receipts-dir", "reports index")?;
            }
            "--workflow-dir" => {
                i += 1;
                workflow_dir = non_empty_path_arg(args, i, "--workflow-dir", "reports index")?;
            }
            "--agent-dir" => {
                i += 1;
                agent_dir = non_empty_path_arg(args, i, "--agent-dir", "reports index")?;
            }
            "--pilot-dir" => {
                i += 1;
                pilot_dir = non_empty_path_arg(args, i, "--pilot-dir", "reports index")?;
            }
            "--ci-dir" => {
                i += 1;
                ci_dir = non_empty_path_arg(args, i, "--ci-dir", "reports index")?;
            }
            "--out" => {
                i += 1;
                out = non_empty_path_arg(args, i, "--out", "reports index")?;
            }
            "--out-md" => {
                i += 1;
                out_md = non_empty_path_arg(args, i, "--out-md", "reports index")?;
            }
            other => return Err(unknown_argument("reports index", other)),
        }
        i += 1;
    }

    Ok(ReportPacketIndexOptions {
        root,
        reports_dir,
        review_dir,
        receipts_dir,
        workflow_dir,
        agent_dir,
        pilot_dir,
        ci_dir,
        out,
        out_md,
    })
}

fn parse_gap_decision_ledger_options(args: &[String]) -> Result<GapDecisionLedgerOptions, String> {
    let mut root = ".".to_string();
    let mut root_explicit = false;
    let mut records = None;
    let mut repo_exposure = None;
    let mut check_output = None;
    let mut out = PathBuf::from(output::gap_decision_ledger::DEFAULT_GAP_DECISION_LEDGER_OUT);
    let mut out_md = PathBuf::from(output::gap_decision_ledger::DEFAULT_GAP_DECISION_LEDGER_MD_OUT);

    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--root" => {
                i += 1;
                let value = non_empty_string_arg(args, i, "--root", "reports gap-ledger")?;
                let reconciled = reconcile_root(
                    std::path::Path::new(&root),
                    root_explicit,
                    &value,
                    "reports gap-ledger",
                )?;
                root = reconciled.display().to_string();
                root_explicit = true;
            }
            "--records" => {
                i += 1;
                records = Some(non_empty_path_arg(
                    args,
                    i,
                    "--records",
                    "reports gap-ledger",
                )?);
            }
            "--repo-exposure" => {
                i += 1;
                repo_exposure = Some(non_empty_path_arg(
                    args,
                    i,
                    "--repo-exposure",
                    "reports gap-ledger",
                )?);
            }
            "--check-output" => {
                i += 1;
                check_output = Some(non_empty_path_arg(
                    args,
                    i,
                    "--check-output",
                    "reports gap-ledger",
                )?);
            }
            "--out" => {
                i += 1;
                out = non_empty_path_arg(args, i, "--out", "reports gap-ledger")?;
            }
            "--out-md" => {
                i += 1;
                out_md = non_empty_path_arg(args, i, "--out-md", "reports gap-ledger")?;
            }
            other => return Err(unknown_argument("reports gap-ledger", other)),
        }
        i += 1;
    }

    let supplied_sources =
        records.is_some() as u8 + repo_exposure.is_some() as u8 + check_output.is_some() as u8;
    if supplied_sources == 0 {
        return Err(
            "reports gap-ledger requires --records PATH, --repo-exposure PATH, or --check-output PATH"
                .to_string(),
        );
    }
    if supplied_sources > 1 {
        return Err(
            "reports gap-ledger accepts only one of --records, --repo-exposure, or --check-output"
                .to_string(),
        );
    }
    let source = if let Some(records) = records {
        GapDecisionLedgerSource::Records(records)
    } else if let Some(repo_exposure) = repo_exposure {
        GapDecisionLedgerSource::RepoExposure(repo_exposure)
    } else if let Some(check_output) = check_output {
        GapDecisionLedgerSource::CheckOutput(check_output)
    } else {
        return Err(
            "reports gap-ledger requires --records PATH, --repo-exposure PATH, or --check-output PATH"
                .to_string(),
        );
    };

    Ok(GapDecisionLedgerOptions {
        root,
        source,
        out,
        out_md,
    })
}

fn parse_typescript_limitations_options(
    args: &[String],
) -> Result<TypeScriptLimitationsOptions, String> {
    let mut root = ".".to_string();
    let mut root_explicit = false;
    let mut check_output = None;
    let mut out = PathBuf::from(output::typescript_limitations::DEFAULT_TYPESCRIPT_LIMITATIONS_OUT);
    let mut out_md =
        PathBuf::from(output::typescript_limitations::DEFAULT_TYPESCRIPT_LIMITATIONS_MD_OUT);

    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--root" => {
                i += 1;
                let value = non_empty_string_arg(args, i, "--root", "reports ts-limitations")?;
                let reconciled = reconcile_root(
                    std::path::Path::new(&root),
                    root_explicit,
                    &value,
                    "reports ts-limitations",
                )?;
                root = reconciled.display().to_string();
                root_explicit = true;
            }
            "--check-output" => {
                i += 1;
                check_output = Some(non_empty_path_arg(
                    args,
                    i,
                    "--check-output",
                    "reports ts-limitations",
                )?);
            }
            "--out" => {
                i += 1;
                out = non_empty_path_arg(args, i, "--out", "reports ts-limitations")?;
            }
            "--out-md" => {
                i += 1;
                out_md = non_empty_path_arg(args, i, "--out-md", "reports ts-limitations")?;
            }
            other => return Err(unknown_argument("reports ts-limitations", other)),
        }
        i += 1;
    }

    let Some(check_output) = check_output else {
        return Err("reports ts-limitations requires --check-output PATH; pass the JSON saved by `ripr check --format json > check.json`".to_string());
    };

    Ok(TypeScriptLimitationsOptions {
        root,
        check_output,
        out,
        out_md,
    })
}

fn parse_typescript_false_actionable_options(
    args: &[String],
) -> Result<TypeScriptFalseActionableOptions, String> {
    let mut root = ".".to_string();
    let mut root_explicit = false;
    let mut corpus = None;
    let mut out =
        PathBuf::from(output::typescript_false_actionable::DEFAULT_TYPESCRIPT_FALSE_ACTIONABLE_OUT);
    let mut out_md = PathBuf::from(
        output::typescript_false_actionable::DEFAULT_TYPESCRIPT_FALSE_ACTIONABLE_MD_OUT,
    );

    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--root" => {
                i += 1;
                let value = non_empty_string_arg(args, i, "--root", "reports ts-false-actionable")?;
                let reconciled = reconcile_root(
                    std::path::Path::new(&root),
                    root_explicit,
                    &value,
                    "reports ts-false-actionable",
                )?;
                root = reconciled.display().to_string();
                root_explicit = true;
            }
            "--corpus" => {
                i += 1;
                corpus = Some(non_empty_path_arg(
                    args,
                    i,
                    "--corpus",
                    "reports ts-false-actionable",
                )?);
            }
            "--out" => {
                i += 1;
                out = non_empty_path_arg(args, i, "--out", "reports ts-false-actionable")?;
            }
            "--out-md" => {
                i += 1;
                out_md = non_empty_path_arg(args, i, "--out-md", "reports ts-false-actionable")?;
            }
            other => {
                return Err(unknown_argument("reports ts-false-actionable", other));
            }
        }
        i += 1;
    }

    let Some(corpus) = corpus else {
        return Err("reports ts-false-actionable requires --corpus PATH; pass the TypeScript preview audit corpus JSON (run `ripr help reports` for the format)".to_string());
    };

    Ok(TypeScriptFalseActionableOptions {
        root,
        corpus,
        out,
        out_md,
    })
}

fn report_packet_index_generated_at() -> Result<String, String> {
    generated_at_unix_ms()
}

fn gap_decision_ledger_generated_at() -> Result<String, String> {
    generated_at_unix_ms()
}

fn typescript_limitations_generated_at() -> Result<String, String> {
    generated_at_unix_ms()
}

fn typescript_false_actionable_generated_at() -> Result<String, String> {
    generated_at_unix_ms()
}

#[cfg(test)]
mod tests {
    use super::super::tests::{
        args, repo_root, unique_command_test_dir, unique_repo_relative_test_dir,
    };
    use super::reports;
    use std::path::Path;

    #[test]
    fn reports_gap_ledger_requires_records_input() {
        assert_eq!(
        super::reports(&args(&["gap-ledger"])),
        Err(
            "reports gap-ledger requires --records PATH, --repo-exposure PATH, or --check-output PATH"
                .to_string()
        )
    );
        assert_eq!(
            super::reports(&args(&["gap-ledger", "--records"])),
            Err("missing value for --records; pass a value right after it".to_string())
        );
        assert_eq!(
            super::reports(&args(&["gap-ledger", "--repo-exposure"])),
            Err("missing value for --repo-exposure; pass a value right after it".to_string())
        );
        assert_eq!(
            super::reports(&args(&["gap-ledger", "--check-output"])),
            Err("missing value for --check-output; pass a value right after it".to_string())
        );
        assert_eq!(
        super::reports(&args(&[
            "gap-ledger",
            "--records",
            "records.json",
            "--check-output",
            "check.json"
        ])),
        Err(
            "reports gap-ledger accepts only one of --records, --repo-exposure, or --check-output"
                .to_string()
        )
    );
        assert_eq!(
        super::reports(&args(&["unknown"])),
        Err(
            "unknown reports subcommand \"unknown\". Accepted: index, ci-packet, ci-summary, gap-ledger, kani, ts-limitations, ts-false-actionable."
                .to_string()
        )
    );
    }

    #[test]
    fn reports_gap_ledger_hinted_recovery_route_parses_and_root_only_does_not() -> Result<(), String>
    {
        // #9851 review follow-up: the producer command named in the
        // missing-gap-ledger diagnostics must parse — the old root-only
        // example was rejected by this parser, so following the hint
        // reached another usage error and wrote nothing.
        let options = super::parse_gap_decision_ledger_options(&args(&[
            "--root",
            ".",
            "--repo-exposure",
            "target/ripr/reports/repo-exposure.json",
            "--out",
            "gap-ledger.json",
        ]))
        .map_err(|err| format!("hinted recovery route must parse: {err}"))?;
        assert_eq!(options.out, std::path::PathBuf::from("gap-ledger.json"));
        // Negative control: the retired root-only example still returns
        // the required-source diagnostic.
        assert_eq!(
        super::parse_gap_decision_ledger_options(&args(&["--root", "."])),
        Err(
            "reports gap-ledger requires --records PATH, --repo-exposure PATH, or --check-output PATH"
                .to_string()
        )
    );
        Ok(())
    }

    #[test]
    fn reports_ts_limitations_requires_check_output_input() {
        assert_eq!(
            super::reports(&args(&["ts-limitations"])),
            Err("reports ts-limitations requires --check-output PATH; pass the JSON saved by `ripr check --format json > check.json`".to_string())
        );
        assert_eq!(
            super::reports(&args(&["ts-limitations", "--check-output"])),
            Err("missing value for --check-output; pass a value right after it".to_string())
        );
    }

    #[test]
    fn reports_ts_limitations_writes_json_and_markdown_reports() -> Result<(), String> {
        let dir = unique_command_test_dir("ts-limitations");
        std::fs::create_dir_all(&dir)
            .map_err(|err| format!("create TypeScript limitations dir: {err}"))?;
        let check_output =
            repo_root().join("fixtures/typescript_static_limit_taxonomy/expected/check.json");
        let out = dir.join("typescript-limitations.json");
        let out_md = dir.join("typescript-limitations.md");

        super::reports(&args(&[
            "ts-limitations",
            "--check-output",
            &check_output.display().to_string(),
            "--out",
            &out.display().to_string(),
            "--out-md",
            &out_md.display().to_string(),
        ]))?;

        let json_text = std::fs::read_to_string(&out)
            .map_err(|err| format!("read TypeScript limitations JSON: {err}"))?;
        let value: serde_json::Value = serde_json::from_str(&json_text)
            .map_err(|err| format!("parse TypeScript limitations JSON: {err}"))?;
        assert_eq!(value["kind"], "typescript_limitation_leaderboard");
        assert_eq!(value["status"], "advisory");
        assert_eq!(value["summary"]["typescript_family_findings_total"], 4);
        assert_eq!(
            value["summary"]["top_limitation_kind"],
            "typescript_package_root_unresolved"
        );
        assert!(json_text.contains("typescript_import_graph_unresolved"));
        assert!(json_text.contains("static_limit_kind"));

        let markdown = std::fs::read_to_string(&out_md)
            .map_err(|err| format!("read TypeScript limitations Markdown: {err}"))?;
        assert!(markdown.contains("# RIPR TypeScript Limitation Leaderboard"));
        assert!(markdown.contains("typescript_package_root_unresolved"));
        assert!(markdown.contains("badge artifacts keep their existing authority"));

        std::fs::remove_dir_all(&dir)
            .map_err(|err| format!("remove TypeScript limitations dir: {err}"))?;
        Ok(())
    }

    #[test]
    fn reports_ts_limitations_fails_closed_on_unreadable_check_output() -> Result<(), String> {
        // Per docs/EXIT_CODES.md an unreadable input means the command could
        // not complete: the CLI must return the read error (exit 2 at the
        // process boundary) instead of writing a `blocked` report and
        // exiting 0.
        let missing =
            unique_command_test_dir("ts-limitations-missing").join("definitely-missing-check.json");
        let result = super::reports(&args(&[
            "ts-limitations",
            "--check-output",
            &missing.display().to_string(),
        ]));
        let err = result.err().ok_or_else(|| {
            "unreadable check output must fail closed, not exit cleanly".to_string()
        })?;
        assert!(
            err.contains("read check output") && err.contains("failed"),
            "error must name the unreadable input, got: {err}"
        );
        Ok(())
    }

    #[test]
    fn reports_ts_false_actionable_requires_corpus_input() {
        assert_eq!(
            super::reports(&args(&["ts-false-actionable"])),
            Err("reports ts-false-actionable requires --corpus PATH; pass the TypeScript preview audit corpus JSON (run `ripr help reports` for the format)".to_string())
        );
        assert_eq!(
            super::reports(&args(&["ts-false-actionable", "--corpus"])),
            Err("missing value for --corpus; pass a value right after it".to_string())
        );
    }

    #[test]
    fn reports_ts_false_actionable_writes_json_and_markdown_reports() -> Result<(), String> {
        let dir = unique_command_test_dir("ts-false-actionable");
        std::fs::create_dir_all(&dir)
            .map_err(|err| format!("create TypeScript false-actionable dir: {err}"))?;
        let corpus =
            repo_root().join("fixtures/typescript-preview-false-actionable-audit/corpus.json");
        let out = dir.join("typescript-false-actionable-audit.json");
        let out_md = dir.join("typescript-false-actionable-audit.md");

        super::reports(&args(&[
            "ts-false-actionable",
            "--corpus",
            &corpus.display().to_string(),
            "--out",
            &out.display().to_string(),
            "--out-md",
            &out_md.display().to_string(),
        ]))?;

        let json_text = std::fs::read_to_string(&out)
            .map_err(|err| format!("read TypeScript false-actionable JSON: {err}"))?;
        let value: serde_json::Value = serde_json::from_str(&json_text)
            .map_err(|err| format!("parse TypeScript false-actionable JSON: {err}"))?;
        assert_eq!(value["kind"], "typescript_false_actionable_audit");
        assert_eq!(value["status"], "advisory");
        assert_eq!(value["summary"]["cases_total"], 15);
        assert_eq!(value["summary"]["false_actionable_total"], 0);
        assert_eq!(value["summary"]["false_actionable_rate"], 0.0);
        assert_eq!(value["summary"]["preview_boundary_violation_total"], 0);
        assert!(
            json_text.contains("This report does not edit source, generate tests, call providers")
        );

        let markdown = std::fs::read_to_string(&out_md)
            .map_err(|err| format!("read TypeScript false-actionable Markdown: {err}"))?;
        assert!(markdown.contains("# RIPR TypeScript False-Actionable Audit"));
        assert!(markdown.contains("False actionable: `0` / `15` (`0.000`)"));
        assert!(
            markdown.contains("Gate-decision and badge artifacts keep their existing authority")
        );

        std::fs::remove_dir_all(&dir)
            .map_err(|err| format!("remove TypeScript false-actionable dir: {err}"))?;
        Ok(())
    }

    #[test]
    fn reports_ts_false_actionable_fails_closed_on_unreadable_corpus() -> Result<(), String> {
        // Per docs/EXIT_CODES.md an unreadable input means the command could
        // not complete: the CLI must return the read error (exit 2 at the
        // process boundary) instead of writing a `blocked` report and
        // exiting 0.
        let missing = unique_command_test_dir("ts-false-actionable-missing")
            .join("definitely-missing-corpus.json");
        let result = super::reports(&args(&[
            "ts-false-actionable",
            "--corpus",
            &missing.display().to_string(),
        ]));
        let err = result
            .err()
            .ok_or_else(|| "unreadable corpus must fail closed, not exit cleanly".to_string())?;
        assert!(
            err.contains("read TypeScript false-actionable audit corpus") && err.contains("failed"),
            "error must name the unreadable input, got: {err}"
        );
        Ok(())
    }

    #[test]
    fn reports_gap_ledger_writes_json_and_markdown_reports() -> Result<(), String> {
        let dir = unique_command_test_dir("gap-ledger");
        std::fs::create_dir_all(&dir).map_err(|err| format!("create gap ledger dir: {err}"))?;
        let records = repo_root().join("fixtures/gap-decision-ledger/corpus.json");
        let out = dir.join("gap-decision-ledger.json");
        let out_md = dir.join("gap-decision-ledger.md");

        super::reports(&args(&[
            "gap-ledger",
            "--records",
            &records.display().to_string(),
            "--out",
            &out.display().to_string(),
            "--out-md",
            &out_md.display().to_string(),
        ]))?;

        let json_text =
            std::fs::read_to_string(&out).map_err(|err| format!("read gap ledger JSON: {err}"))?;
        assert!(json_text.contains("\"kind\": \"gap_decision_ledger\""));
        assert!(json_text.contains("\"records_total\": 18"));
        let markdown = std::fs::read_to_string(&out_md)
            .map_err(|err| format!("read gap ledger Markdown: {err}"))?;
        assert!(markdown.contains("# RIPR Gap Decision Ledger"));
        assert!(markdown.contains("gate candidates=`1`"));

        std::fs::remove_dir_all(&dir).map_err(|err| format!("remove gap ledger dir: {err}"))?;
        Ok(())
    }

    /// #14311: a readable source that is not a valid input of its kind exits
    /// non-zero with a refusal naming the flag, the cause and the producer,
    /// after writing the `blocked` ledger over any earlier one. A valid empty
    /// record array stays a successful `no_records` run.
    #[test]
    fn reports_gap_ledger_refuses_a_malformed_source_after_writing_blocked() -> Result<(), String> {
        let dir = unique_command_test_dir("gap-ledger-malformed");
        std::fs::create_dir_all(&dir).map_err(|err| format!("create dir: {err}"))?;
        let root = dir.display().to_string();
        for (flag, body, producer) in [
            ("--records", "not json {", "--repo-exposure PATH"),
            ("--records", "", "--repo-exposure PATH"),
            ("--records", "{}", "--repo-exposure PATH"),
            ("--repo-exposure", "not json {", "ripr pilot --root"),
            (
                "--check-output",
                "{\"schema\":\"other\"",
                "ripr check --format json",
            ),
        ] {
            let input = dir.join("input.json");
            std::fs::write(&input, body).map_err(|err| format!("write input: {err}"))?;
            let out = dir.join("ledger.json");
            std::fs::write(
                &out,
                r#"{"kind":"gap_decision_ledger","status":"advisory"}"#,
            )
            .map_err(|err| format!("write stale ledger: {err}"))?;
            let Err(err) = gap_ledger_from(&root, flag, &input, &out) else {
                return Err(format!("{flag} {body:?} must be refused, not exit 0"));
            };
            assert!(
                err.contains(&format!("reports gap-ledger {flag} "))
                    && err.contains("could not be read as")
                    && err.contains("the ledger was written as `blocked`")
                    && !err.contains("regenerate the input with the command that wrote it")
                    && err.contains(producer),
                "{flag} {body:?}: {err}"
            );
            assert_eq!(
                read_json_file(&out)?["status"],
                "blocked",
                "{flag} {body:?}"
            );
        }
        let input = dir.join("empty-array.json");
        std::fs::write(&input, "[]").map_err(|err| format!("write input: {err}"))?;
        let ledger = gap_ledger_from(&root, "--records", &input, &dir.join("ok.json"))?;
        assert_eq!(ledger["status"], "no_records");
        std::fs::remove_dir_all(&dir).map_err(|err| format!("remove dir: {err}"))?;
        Ok(())
    }

    /// #14434: a source that parses but is a producer refusal envelope
    /// (`downstream_consumable: false`) means nothing was analyzed, so the
    /// command exits non-zero after writing `blocked`, like a malformed
    /// source. A complete check output with no findings stays a successful
    /// run, and a bounded refusal keeps its own typed handling.
    #[test]
    fn reports_gap_ledger_refuses_a_producer_refusal_source_after_writing_blocked()
    -> Result<(), String> {
        let dir = unique_command_test_dir("gap-ledger-refusal-source");
        std::fs::create_dir_all(&dir).map_err(|err| format!("create dir: {err}"))?;
        let root = dir.display().to_string();
        let refusal = serde_json::json!({
            "schema_version": "0.2",
            "tool": "ripr",
            "mode": "fast",
            "root": ".",
            "summary": {
                "changed_rust_files": 0,
                "probes": 0,
                "findings": 0,
                "exposed": 0,
                "weakly_exposed": 0,
                "reachable_unrevealed": 0,
                "no_static_path": 0,
                "infection_unknown": 0,
                "propagation_unknown": 0,
                "static_unknown": 0,
                "changed_files_by_language": []
            },
            "findings": [],
            "analysis_scope": {
                "scope": "diff",
                "run_status": "diff_scope_oversized",
                "basis": "rust_diff_scope_budget",
                "downstream_consumable": false,
                "limitation": "diff_scope_oversized",
                "repair_route": "analysis/diff-scope-budget"
            }
        });
        let input = dir.join("refusal.json");
        std::fs::write(&input, refusal.to_string()).map_err(|err| format!("write input: {err}"))?;
        let out = dir.join("ledger.json");
        std::fs::write(
            &out,
            r#"{"kind":"gap_decision_ledger","status":"advisory"}"#,
        )
        .map_err(|err| format!("write stale ledger: {err}"))?;
        let Err(err) = gap_ledger_from(&root, "--check-output", &input, &out) else {
            return Err("a producer refusal source must be refused, not exit 0".to_string());
        };
        assert!(
            err.contains("reports gap-ledger --check-output ")
                && err.contains("is a producer refusal (run_status: diff_scope_oversized)")
                && err.contains("the ledger was written as `blocked`")
                && err.contains("Follow its repair_route `analysis/diff-scope-budget`"),
            "{err}"
        );
        assert_eq!(read_json_file(&out)?["status"], "blocked");

        // This refusal is disclosed only in `analysis_scope`, so its saved
        // `blocked` ledger keeps a marker-only entry and no route: fed back as
        // `--records` it is refused too, and the error must not point at a
        // repair_route the ledger lacks.
        let Err(err) = gap_ledger_from(&root, "--records", &out, &dir.join("refed.json")) else {
            return Err("a saved refusal ledger must be refused, not exit 0".to_string());
        };
        assert!(
            err.contains("reports gap-ledger --records ")
                && err.contains("It records no repair_route: re-run the check that produced it")
                && !err.contains("Follow its repair_route"),
            "{err}"
        );

        // The check producer's real envelope also discloses the refusal (and
        // its route) in `run_limitations[]`, which the saved ledger keeps:
        // re-fed as `--records`, the error names that route.
        let mut enveloped = refusal.clone();
        enveloped["run_limitations"] = serde_json::json!([{
            "category": "diff_scope_oversized",
            "run_status": "diff_scope_oversized",
            "downstream_consumable": false,
            "repair_route": "analysis/diff-scope-budget"
        }]);
        let input = dir.join("enveloped.json");
        std::fs::write(&input, enveloped.to_string())
            .map_err(|err| format!("write input: {err}"))?;
        let saved = dir.join("enveloped-ledger.json");
        if gap_ledger_from(&root, "--check-output", &input, &saved).is_ok() {
            return Err("an enveloped producer refusal must be refused".to_string());
        }
        let Err(err) = gap_ledger_from(&root, "--records", &saved, &dir.join("refed2.json")) else {
            return Err("a saved enveloped refusal ledger must be refused".to_string());
        };
        assert!(
            err.contains("reports gap-ledger --records ")
                && err.contains("Follow its repair_route `analysis/diff-scope-budget`"),
            "{err}"
        );

        // Control: a findings-bounded run also carries the explicit false
        // flag but keeps its own typed handling (the gate refuses it by
        // name), so this path must not refuse it.
        let mut bounded = refusal.clone();
        bounded["analysis_scope"] = serde_json::json!({"scope": "diff", "run_status": "complete"});
        bounded["run_limitations"] = serde_json::json!([{
            "category": crate::output::json::FINDINGS_BOUND_RUN_STATUS,
            "run_status": crate::output::json::FINDINGS_BOUND_RUN_STATUS,
            "downstream_consumable": false
        }]);
        let input = dir.join("bounded.json");
        std::fs::write(&input, bounded.to_string()).map_err(|err| format!("write input: {err}"))?;
        gap_ledger_from(
            &root,
            "--check-output",
            &input,
            &dir.join("bounded-ledger.json"),
        )?;

        // Control: the same document as a complete, consumable run with no
        // findings is a healthy empty source, not a refusal.
        let mut complete = refusal;
        complete["analysis_scope"] = serde_json::json!({"scope": "diff", "run_status": "complete"});
        let input = dir.join("complete.json");
        std::fs::write(&input, complete.to_string())
            .map_err(|err| format!("write input: {err}"))?;
        let ledger = gap_ledger_from(&root, "--check-output", &input, &dir.join("ok.json"))?;
        assert_eq!(ledger["status"], "no_records");
        std::fs::remove_dir_all(&dir).map_err(|err| format!("remove dir: {err}"))?;
        Ok(())
    }

    fn read_json_file(path: &Path) -> Result<serde_json::Value, String> {
        serde_json::from_str(
            &std::fs::read_to_string(path)
                .map_err(|err| format!("read {}: {err}", path.display()))?,
        )
        .map_err(|err| format!("parse {}: {err}", path.display()))
    }

    fn gap_ledger_from(
        root: &str,
        flag: &str,
        input: &Path,
        out: &Path,
    ) -> Result<serde_json::Value, String> {
        let out_md = out.with_extension("md");
        super::reports(&args(&[
            "gap-ledger",
            "--root",
            root,
            flag,
            &input.display().to_string(),
            "--out",
            &out.display().to_string(),
            "--out-md",
            &out_md.display().to_string(),
        ]))?;
        read_json_file(out)
    }

    fn pricing_gap_record(anchor_file: &str) -> serde_json::Value {
        serde_json::json!({
            "gap_id": "gap:pr:pricing",
            "canonical_gap_id": "gap:rust:pricing",
            "kind": "MissingBoundaryAssertion",
            "language": "rust",
            "language_status": "stable",
            "gap_state": "actionable",
            "repair_route": {
                "route_kind": "AddBoundaryAssertion",
                "related_test": "tests/pricing.rs::discount_threshold"
            },
            "anchor": {"file": anchor_file, "line": 1}
        })
    }

    /// #4544: the ledger writer never hashes the workspace. A records input
    /// without an analysis stamp yields no usable stamp (the LSP then reports
    /// `unverifiable_subject`), and re-rendering that unstamped ledger through
    /// `--records` does not mint a fresh stamp for the current bytes.
    #[test]
    fn reports_gap_ledger_without_an_input_stamp_is_unavailable_and_stays_so() -> Result<(), String>
    {
        use crate::output::gap_source_subject::{SourceSubjectCheck, check_source_subject};
        let dir = unique_command_test_dir("gap-ledger-source-subject-missing");
        std::fs::create_dir_all(dir.join("src")).map_err(|err| format!("create src: {err}"))?;
        std::fs::write(dir.join("src/pricing.rs"), "abc\n")
            .map_err(|err| format!("write anchor: {err}"))?;
        let records = dir.join("records.json");
        std::fs::write(
            &records,
            serde_json::json!({"records": [pricing_gap_record("src/pricing.rs")]}).to_string(),
        )
        .map_err(|err| format!("write records: {err}"))?;
        let root = dir.display().to_string();

        let ledger = gap_ledger_from(&root, "--records", &records, &dir.join("ledger.json"))?;
        assert_eq!(ledger.get("source_subject"), None);
        assert_eq!(
            ledger["source_subject_unavailable"],
            serde_json::json!("input_source_subject_missing")
        );
        let required = std::collections::BTreeSet::from([
            "src/pricing.rs".to_string(),
            "tests/pricing.rs".to_string(),
        ]);
        assert!(matches!(
            check_source_subject(&dir, ledger.get("source_subject"), &required),
            SourceSubjectCheck::Unverifiable(_)
        ));

        let rerendered = gap_ledger_from(
            &root,
            "--records",
            &dir.join("ledger.json"),
            &dir.join("rerendered.json"),
        )?;
        assert_eq!(rerendered.get("source_subject"), None);
        assert_eq!(
            rerendered["source_subject_unavailable"],
            serde_json::json!("input_source_subject_missing")
        );

        std::fs::remove_dir_all(&dir).map_err(|err| format!("remove dir: {err}"))?;
        Ok(())
    }

    /// #4544: a stamped input's digests are copied, never recomputed: the
    /// ledger carries the input's digest even though the file changed before
    /// the ledger was written, so the LSP reports the ledger stale. The root is
    /// relative and the record names the anchor by absolute path, which must
    /// still resolve to the same repo-relative stamp entry.
    #[test]
    fn reports_gap_ledger_copies_the_input_stamp_for_absolute_paths_under_a_relative_root()
    -> Result<(), String> {
        use crate::output::gap_source_subject::{SourceSubjectCheck, check_source_subject};
        let dir = unique_repo_relative_test_dir("gap-ledger-source-subject-copy");
        assert!(dir.is_relative(), "{}", dir.display());
        std::fs::create_dir_all(dir.join("src")).map_err(|err| format!("create src: {err}"))?;
        std::fs::write(dir.join("src/pricing.rs"), "abc\n")
            .map_err(|err| format!("write anchor: {err}"))?;
        let absolute_anchor = std::env::current_dir()
            .map_err(|err| format!("cwd: {err}"))?
            .join(&dir)
            .join("src/pricing.rs");
        let root = dir.display().to_string();
        let analysis_stamp = serde_json::json!({
            "digest_algorithm": "sha256",
            "files": [
                {
                    "path": "src/pricing.rs",
                    "digest": "sha256:edeaaff3f1774ad2888673770c6d64097e391bc362d7d6fb34982ddf0efd18cb"
                },
                {"path": "tests/pricing.rs", "digest": null}
            ]
        });
        let records = dir.join("records.json");
        std::fs::write(
            &records,
            serde_json::json!({
                "root": root,
                "source_subject": analysis_stamp,
                "records": [pricing_gap_record(&absolute_anchor.display().to_string())]
            })
            .to_string(),
        )
        .map_err(|err| format!("write records: {err}"))?;
        // The workspace moves on after the analysis stamped it.
        std::fs::write(dir.join("src/pricing.rs"), "abd\n")
            .map_err(|err| format!("edit anchor: {err}"))?;

        let ledger = gap_ledger_from(&root, "--records", &records, &dir.join("ledger.json"))?;
        assert_eq!(ledger["source_subject"], analysis_stamp);
        assert_eq!(ledger.get("source_subject_unavailable"), None);
        let required = std::collections::BTreeSet::from([
            "src/pricing.rs".to_string(),
            "tests/pricing.rs".to_string(),
        ]);
        assert_eq!(
            check_source_subject(&dir, ledger.get("source_subject"), &required),
            SourceSubjectCheck::Stale("src/pricing.rs".to_string())
        );

        // Re-rendering the written ledger keeps the analysis stamp.
        let rerendered = gap_ledger_from(
            &root,
            "--records",
            &dir.join("ledger.json"),
            &dir.join("rerendered.json"),
        )?;
        assert_eq!(rerendered["source_subject"], analysis_stamp);

        // A stamp that omits a named file yields no usable stamp.
        std::fs::write(
            &records,
            serde_json::json!({
                "root": root,
                "source_subject": {"digest_algorithm": "sha256", "files": [
                    analysis_stamp["files"][0].clone()
                ]},
                "records": [pricing_gap_record("src/pricing.rs")]
            })
            .to_string(),
        )
        .map_err(|err| format!("rewrite records: {err}"))?;
        let incomplete =
            gap_ledger_from(&root, "--records", &records, &dir.join("incomplete.json"))?;
        assert_eq!(incomplete.get("source_subject"), None);
        assert_eq!(
            incomplete["source_subject_unavailable"],
            serde_json::json!("input_source_subject_incomplete")
        );

        std::fs::remove_dir_all(&dir).map_err(|err| format!("remove dir: {err}"))?;
        Ok(())
    }

    /// #4544 regression: analysis, then an edit, then the ledger write. The
    /// repo-exposure artifact stamps the bytes the analysis run read; the
    /// ledger derived after the edit copies that stamp, so the LSP-side check
    /// reports the edited file stale instead of vouching for the new bytes.
    #[test]
    fn reports_gap_ledger_after_an_edit_keeps_the_analysis_time_stamp() -> Result<(), String> {
        use crate::output::gap_source_subject::{SourceSubjectCheck, check_source_subject};
        let dir = unique_command_test_dir("gap-ledger-source-subject-analysis");
        let fixture = repo_root().join("fixtures/boundary_gap/input");
        for file in ["Cargo.toml", "src/lib.rs", "tests/pricing.rs"] {
            let target = dir.join(file);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|err| format!("create {}: {err}", parent.display()))?;
            }
            std::fs::copy(fixture.join(file), &target)
                .map_err(|err| format!("copy {file}: {err}"))?;
        }
        let config = crate::config::RiprConfig::default();
        let (classified, limit_info) =
            crate::analysis::inventory_classified_seams_at_with_config(&dir, &config)?;
        let context = crate::agent::artifact::RepoExposureArtifactContext::for_repo_exposure(
            dir.clone(),
            "draft".to_string(),
            None,
            &config,
        )?;
        let repo_exposure_json =
            crate::output::repo_exposure::render_repo_exposure_json_with_context(
                &classified,
                limit_info.as_ref(),
                None,
                None,
                None,
                &[],
                &context,
            )?;
        let repo_exposure = dir.join("repo-exposure.json");
        std::fs::write(&repo_exposure, &repo_exposure_json)
            .map_err(|err| format!("write repo exposure: {err}"))?;
        let analysis_stamp = read_json_file(&repo_exposure)?["source_subject"].clone();
        let stamped_paths = analysis_stamp["files"]
            .as_array()
            .map(|files| {
                files
                    .iter()
                    .filter_map(|file| file["path"].as_str().map(ToOwned::to_owned))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        assert_eq!(stamped_paths, ["src/lib.rs", "tests/pricing.rs"]);

        std::fs::write(
            dir.join("src/lib.rs"),
            format!(
                "{}\n// edited after analysis\n",
                std::fs::read_to_string(dir.join("src/lib.rs"))
                    .map_err(|err| format!("read lib: {err}"))?
            ),
        )
        .map_err(|err| format!("edit lib: {err}"))?;

        let ledger = gap_ledger_from(
            &dir.display().to_string(),
            "--repo-exposure",
            &repo_exposure,
            &dir.join("ledger.json"),
        )?;
        assert_eq!(ledger.get("source_subject_unavailable"), None);
        assert_eq!(ledger["source_subject"], analysis_stamp);
        let required = stamped_paths.into_iter().collect();
        assert_eq!(
            check_source_subject(&dir, ledger.get("source_subject"), &required),
            SourceSubjectCheck::Stale("src/lib.rs".to_string())
        );

        std::fs::remove_dir_all(&dir).map_err(|err| format!("remove dir: {err}"))?;
        Ok(())
    }

    #[test]
    fn reports_gap_ledger_fails_closed_for_blank_and_mixed_verification_routes()
    -> Result<(), String> {
        for (case, commands) in [
            ("blank", serde_json::json!([" \t "])),
            ("mixed", serde_json::json!(["cargo test", "  "])),
        ] {
            let dir = unique_command_test_dir(&format!("gap-ledger-{case}-verify"));
            std::fs::create_dir_all(&dir)
                .map_err(|error| format!("create {case} gap-ledger dir: {error}"))?;
            let corpus_text = std::fs::read_to_string(
                repo_root().join("fixtures/gap-decision-ledger/corpus.json"),
            )
            .map_err(|error| format!("read gap-ledger corpus: {error}"))?;
            let corpus: serde_json::Value = serde_json::from_str(&corpus_text)
                .map_err(|error| format!("parse gap-ledger corpus: {error}"))?;
            let mut record = corpus
                .get("cases")
                .and_then(serde_json::Value::as_array)
                .and_then(|cases| cases.first())
                .and_then(|case| case.get("expected_gap_record"))
                .cloned()
                .ok_or_else(|| "gap-ledger corpus first expected record missing".to_string())?;
            record["verification_commands"] = commands;
            let records = dir.join("records.json");
            std::fs::write(
                &records,
                serde_json::json!({"records": [record]}).to_string(),
            )
            .map_err(|error| format!("write {case} gap-ledger records: {error}"))?;
            let out = dir.join("gap-decision-ledger.json");
            let out_md = dir.join("gap-decision-ledger.md");

            super::reports(&args(&[
                "gap-ledger",
                "--records",
                &records.display().to_string(),
                "--out",
                &out.display().to_string(),
                "--out-md",
                &out_md.display().to_string(),
            ]))?;

            let json_text = std::fs::read_to_string(&out)
                .map_err(|error| format!("read {case} gap-ledger JSON: {error}"))?;
            let value: serde_json::Value = serde_json::from_str(&json_text)
                .map_err(|error| format!("parse {case} gap-ledger JSON: {error}"))?;
            assert_eq!(value["summary"]["projection_pr_comment_eligible"], 0);
            assert_eq!(value["summary"]["projection_gate_candidate"], 0);
            assert_eq!(value["summary"]["projection_agent_packet_eligible"], 0);
            let markdown = std::fs::read_to_string(&out_md)
                .map_err(|error| format!("read {case} gap-ledger Markdown: {error}"))?;
            assert!(markdown.contains("Verify: `unavailable_incomplete_command_list`"));
            assert!(!markdown.contains("  - `cargo test`"));

            std::fs::remove_dir_all(&dir)
                .map_err(|error| format!("remove {case} gap-ledger dir: {error}"))?;
        }
        Ok(())
    }

    #[test]
    fn reports_gap_ledger_derives_output_contract_gap_from_check_output() -> Result<(), String> {
        let dir = unique_command_test_dir("gap-ledger-check-output");
        std::fs::create_dir_all(&dir)
            .map_err(|err| format!("create gap ledger check output dir: {err}"))?;
        let check_output = dir.join("check.json");
        let out = dir.join("gap-decision-ledger.json");
        let out_md = dir.join("gap-decision-ledger.md");
        std::fs::write(&check_output, check_output_with_presentation_text_gap())
            .map_err(|err| format!("write check output: {err}"))?;

        super::reports(&args(&[
            "gap-ledger",
            "--check-output",
            &check_output.display().to_string(),
            "--out",
            &out.display().to_string(),
            "--out-md",
            &out_md.display().to_string(),
        ]))?;

        let json_text = std::fs::read_to_string(&out)
            .map_err(|err| format!("read check-output gap ledger JSON: {err}"))?;
        assert!(json_text.contains("\"source_kind\": \"check_output\""));
        assert!(json_text.contains("\"kind\": \"MissingOutputContract\""));
        assert!(json_text.contains("\"route_kind\": \"AddOutputGolden\""));
        assert!(json_text.contains("cargo xtask goldens check"));
        assert!(json_text.contains("\"projection_pr_comment_eligible\": 1"));
        assert!(json_text.contains("\"projection_gate_candidate\": 0"));
        let markdown = std::fs::read_to_string(&out_md)
            .map_err(|err| format!("read check-output gap ledger Markdown: {err}"))?;
        assert!(markdown.contains("MissingOutputContract"));
        assert!(markdown.contains("AddOutputGolden"));

        std::fs::remove_dir_all(&dir)
            .map_err(|err| format!("remove gap ledger check output dir: {err}"))?;
        Ok(())
    }

    #[test]
    fn reports_gap_ledger_derives_python_static_limit_as_report_only() -> Result<(), String> {
        let dir = unique_command_test_dir("gap-ledger-python-static-limit");
        std::fs::create_dir_all(&dir)
            .map_err(|err| format!("create python static-limit gap ledger dir: {err}"))?;
        let check_output =
            repo_root().join("fixtures/python_dynamic_dispatch_limit/expected/check.json");
        let out = dir.join("gap-decision-ledger.json");
        let out_md = dir.join("gap-decision-ledger.md");

        super::reports(&args(&[
            "gap-ledger",
            "--check-output",
            &check_output.display().to_string(),
            "--out",
            &out.display().to_string(),
            "--out-md",
            &out_md.display().to_string(),
        ]))?;

        let json_text = std::fs::read_to_string(&out)
            .map_err(|err| format!("read python static-limit gap ledger JSON: {err}"))?;
        let value: serde_json::Value = serde_json::from_str(&json_text)
            .map_err(|err| format!("parse python static-limit gap ledger JSON: {err}"))?;
        assert_eq!(value["status"], "advisory");
        assert_eq!(value["summary"]["records_total"], 1);
        assert_eq!(value["summary"]["static_limitation_total"], 1);
        assert_eq!(value["summary"]["projection_agent_packet_eligible"], 0);
        assert_eq!(value["records"][0]["kind"], "StaticLimitation");
        assert_eq!(value["records"][0]["repairability"], "analyzer_limitation");
        assert_eq!(value["records"][0]["static_limit_kind"], "dynamic_dispatch");
        assert_eq!(
            value["records"][0]["projection_eligibility"]["agent_packet"]["eligible"],
            false
        );
        assert!(value["records"][0].get("repair_route").is_none());

        std::fs::remove_dir_all(&dir)
            .map_err(|err| format!("remove python static-limit gap ledger dir: {err}"))?;
        Ok(())
    }

    #[test]
    fn reports_gap_ledger_derives_repo_scoped_records_from_repo_exposure() -> Result<(), String> {
        let dir = unique_command_test_dir("gap-ledger-repo-exposure");
        std::fs::create_dir_all(&dir)
            .map_err(|err| format!("create gap ledger repo exposure dir: {err}"))?;
        let repo_exposure = dir.join("repo-exposure.json");
        let out = dir.join("gap-decision-ledger.json");
        let out_md = dir.join("gap-decision-ledger.md");
        std::fs::write(
            &repo_exposure,
            repo_exposure_with_actionable_evidence_record(),
        )
        .map_err(|err| format!("write repo exposure: {err}"))?;

        super::reports(&args(&[
            "gap-ledger",
            "--repo-exposure",
            &repo_exposure.display().to_string(),
            "--out",
            &out.display().to_string(),
            "--out-md",
            &out_md.display().to_string(),
        ]))?;

        let json_text = std::fs::read_to_string(&out)
            .map_err(|err| format!("read derived gap ledger JSON: {err}"))?;
        assert!(json_text.contains("\"source_kind\": \"repo_exposure\""));
        assert!(json_text.contains("\"records_total\": 1"));
        assert!(json_text.contains("\"kind\": \"MissingBoundaryAssertion\""));
        assert!(json_text.contains("\"scope\": \"repo_scoped\""));
        assert!(json_text.contains("\"route_kind\": \"AddBoundaryAssertion\""));
        assert!(json_text.contains("\"ripr_zero_count\":"));
        let markdown = std::fs::read_to_string(&out_md)
            .map_err(|err| format!("read derived gap ledger Markdown: {err}"))?;
        assert!(markdown.contains("AddBoundaryAssertion"));
        assert!(markdown.contains("repo_scoped"));

        std::fs::remove_dir_all(&dir)
            .map_err(|err| format!("remove gap ledger repo exposure dir: {err}"))?;
        Ok(())
    }

    #[test]
    fn gap_ledger_source_path_is_bound_to_invocation_cwd() -> Result<(), String> {
        let rendered =
            super::ledger_source_path(Path::new("repo/target/ripr/reports/repo-exposure.json"))?;
        let expected_suffix = Path::new("repo/target/ripr/reports/repo-exposure.json")
            .to_string_lossy()
            .replace('\\', "/");
        assert!(rendered.ends_with(&expected_suffix), "{rendered}");
        assert!(Path::new(&rendered).is_absolute(), "{rendered}");
        Ok(())
    }

    fn repo_exposure_with_actionable_evidence_record() -> &'static str {
        r#"{
  "schema_version": "0.3",
  "scope": "repo",
  "seams": [
{
  "seam_id": "seam-pricing-threshold",
  "file": "src/pricing.rs",
  "line": 88,
  "evidence_record": {
    "schema_version": "0.1",
    "seam_id": "seam-pricing-threshold",
    "canonical_gap_id": "gap:rust:pricing:threshold",
    "raw_findings": [
      {
        "file": "src/pricing.rs",
        "line": 88,
        "kind": "weakly_gripped",
        "expression": "amount >= discount_threshold",
        "probe_kind": "predicate_boundary",
        "source_id": "seam-pricing-threshold",
        "evidence_record_ref": "seam-pricing-threshold"
      }
    ],
    "canonical_item": {
      "canonical_gap_id": "gap:rust:pricing:threshold",
      "raw_group_size": 1,
      "canonical_item_kind": "gap",
      "evidence_class": "predicate_boundary",
      "gap_state": "actionable",
      "actionability": "upgrade_assertion",
      "group_reason": "same owner and missing discriminator",
      "why": "related tests reach the seam but miss the boundary discriminator",
      "recommended_repair": "Add an exact boundary assertion.",
      "related_test": {
        "name": "below_threshold_has_no_discount",
        "file": "tests/pricing_tests.rs",
        "line": 12,
        "reason": "direct owner call"
      },
      "verify_command": "ripr agent verify --root . --before target/ripr/pilot/repo-exposure.json --after target/ripr/pilot/after.repo-exposure.json --json",
      "confidence": {
        "basis": "static_only",
        "notes": ["no imported runtime calibration data"]
      }
    },
    "owner": "pricing::discounted_total",
    "location": {
      "file": "src/pricing.rs",
      "line": 88
    },
    "seam_kind": "predicate_boundary",
    "grip_class": "weakly_gripped",
    "headline_eligible": true,
    "recommendation": {
      "action": "write_targeted_test",
      "reason": "add the missing boundary assertion",
      "recommended_test": {
        "name": "discounts_at_threshold",
        "file": "tests/pricing_tests.rs",
        "reason": "nearest pricing test module"
      },
      "assertion_shape": {
        "kind": "exact_return_value",
        "example": "assert_eq!(discounted_total(100, 100), 90)"
      },
      "verify_command": "ripr agent verify --root . --before target/ripr/pilot/repo-exposure.json --after target/ripr/pilot/after.repo-exposure.json --json"
    },
    "actionability": {
      "class": "actionable_assertion_upgrade",
      "reason": "related tests reach the seam but still miss a concrete discriminator",
      "has_concrete_guidance": true
    }
  }
}
  ]
}"#
    }

    fn check_output_with_presentation_text_gap() -> &'static str {
        r#"{
  "schema_version": "0.1",
  "tool": "ripr",
  "findings": [
{"id": "help-label-decl", "source_currentness": "candidate_current"},
{"id": "help-label-literal", "source_currentness": "candidate_current"}
  ],
  "finding_alignment": {
"scope": "supported_classes",
"items": [
  {
    "canonical_gap_id": "presentation_text::HELP_DEVICE_LABEL",
    "canonical_item_kind": "gap",
    "evidence_class": "presentation_text",
    "gap_state": "actionable",
    "actionability": "add_output_observer",
    "raw_group_size": 2,
    "group_reason": "declaration_and_literal_same_text_constant",
    "why": "Changed text flows to CLI help output and no supported output observer is found.",
    "recommended_repair": "Add or update a help-output snapshot assertion for HELP_DEVICE_LABEL.",
    "related_test": null,
    "verify_command": "cargo xtask evidence-quality-scorecard",
    "static_limitations": [],
    "confidence": {
      "basis": "fixture_backed",
      "notes": ["Visible unobserved presentation text is actionable only for supported sink patterns."]
    },
    "raw_findings": [
      {
        "file": "crates/ripr/src/cli/help.rs",
        "line": 42,
        "kind": "exposed",
        "expression": "pub const HELP_DEVICE_LABEL: &str =",
        "probe_kind": "field_construction",
        "source_id": "help-label-decl",
        "evidence_record_ref": "help-label-decl"
      },
      {
        "file": "crates/ripr/src/cli/help.rs",
        "line": 43,
        "kind": "static_unknown",
        "expression": "\"Device label\";",
        "probe_kind": "static_unknown",
        "source_id": "help-label-literal",
        "evidence_record_ref": "help-label-literal"
      }
    ],
    "presentation_text": {
      "constant_name": "HELP_DEVICE_LABEL",
      "text_literal": "Device label",
      "visibility": "user_visible",
      "observer": "none",
      "actionability": "add_output_observer",
      "source_kind": "const_decl",
      "canonical_group_reason": "declaration_and_literal_same_text_constant",
      "recommended_observer": "cli_help_output",
      "repair_kind": "output_observer",
      "target_test_type": "help_output_snapshot",
      "suggested_assertion": "Assert CLI help output includes the HELP_DEVICE_LABEL text."
    }
  }
]
  }
}"#
    }

    #[test]
    fn reports_dispatch_wiring_help_exits_cleanly() {
        assert_eq!(reports(&args(&["--help"])), Ok(()));
    }
}
