//! Arg-parsing and dispatch for `ripr baseline create / diff / update`.
//!
//! This is the CLI adapter layer only. Baseline report construction and
//! rendering live in `crate::output::baseline`, `crate::output::baseline_delta`,
//! and `crate::output::baseline_update`. This module owns argv parsing, output
//! destination selection, and exit mapping for the baseline command family.

use crate::cli::commands_options::{
    BaselineCreateOptions, BaselineDiffOptions, BaselineUpdateOptions,
};
use crate::cli::help;
use crate::cli::suggest::{unknown_argument, unknown_value};
use crate::output;
use std::path::{Path, PathBuf};

use super::{
    baseline_created_at, non_empty_path_arg, read_optional_text_for_report, write_text_file,
};

/// Recovery route for a missing gate-decision input, in the sibling commands'
/// vocabulary (#9851): `baseline create --from` / `baseline update --current`
/// consume the artifact `ripr gate evaluate` writes.
const GATE_DECISION_RECOVERY: &str = "Run `ripr gate evaluate --gap-ledger <gap-ledger> --out <gate-decision.json> --out-md <gate-decision.md>` first, or pass the correct path";

/// Recovery route for a missing `--baseline` ledger (#9851 review follow-up):
/// `baseline update` requires the reviewed historical-debt ledger with kind
/// `gate_baseline` and an `entries` array, which only `baseline create`
/// writes — pointing at `gate evaluate` hands the caller a gate-decision
/// artifact the consumer still rejects. The route chains through a
/// gate-decision file: create adopts it, update consumes the adoption.
/// Recovery route for a missing `--from-check` artifact (#11206 item 3): the
/// whole-repository producer is `ripr check` over the candidate tree.
const CHECK_ARTIFACT_RECOVERY: &str = "Run `ripr check --candidate-tree $(git rev-parse HEAD^{tree}) --format json > <check.json>` first, or pass the correct path";

type BaselineBuilder =
    fn(&str, &str, &str) -> Result<output::baseline::BaselineCreateReport, String>;

const BASELINE_LEDGER_RECOVERY: &str = "Run `ripr baseline create --from <gate-decision.json> --out <baseline.json>` first, or pass the correct path";

pub(in crate::cli) fn baseline(args: &[String]) -> Result<(), String> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        help::print_group_help("baseline", args);
        return Ok(());
    }
    let Some((subcommand, rest)) = args.split_first() else {
        help::print_baseline_help();
        return Ok(());
    };
    match subcommand.as_str() {
        "create" => baseline_create(rest),
        "diff" => baseline_diff(rest),
        "update" => baseline_update(rest),
        _ => Err(unknown_value(
            "baseline subcommand",
            subcommand,
            &["create", "diff", "update"],
        )),
    }
}

fn baseline_create(args: &[String]) -> Result<(), String> {
    let options = parse_baseline_create_options(args)?;
    let (recovery, build_report): (&str, BaselineBuilder) = if options.from_check {
        (
            CHECK_ARTIFACT_RECOVERY,
            output::baseline::baseline_create_report_from_check_json,
        )
    } else {
        (
            GATE_DECISION_RECOVERY,
            output::baseline::baseline_create_report_from_gate_decision_json,
        )
    };
    let source_json = crate::bounded_input::read_required_input(
        "read baseline create source",
        &options.from,
        Some(recovery),
    )?;
    let created_at = baseline_created_at()?;
    let source_report = output::baseline::display_path(&options.from);
    let report = build_report(&source_report, &created_at, &source_json)?;
    let rendered = output::baseline::render_baseline_create_json(&report)?;
    if options.dry_run {
        print!("{rendered}");
        return Ok(());
    }
    if options.out.exists() && !options.force {
        return Err(baseline_create_overwrite_error(
            &options.from,
            options.from_check,
            &options.out,
            cfg!(windows),
        ));
    }
    write_text_file(&options.out, &rendered)?;
    println!("Wrote {}", output::path::human_path(&options.out));
    println!(
        "Entries: {}",
        output::baseline::baseline_entry_count(&report)
    );
    // Creation-time disclosure (issue #7421 review): a source report that
    // predates producing-build identity must warn the operator here, not
    // only inside the written ledger. Warnings ride stderr like every
    // other advisory surface, so piping stdout cannot drop them (#9806).
    let warnings = output::baseline::baseline_warnings(&report);
    if !warnings.is_empty() {
        eprintln!("Warnings: {}", warnings.len());
        for warning in warnings {
            eprintln!("Warning: {warning}");
        }
    }
    Ok(())
}

fn baseline_create_overwrite_error(
    from: &Path,
    from_check: bool,
    out: &Path,
    windows: bool,
) -> String {
    let from_flag = if from_check { "--from-check" } else { "--from" };
    let quoted_from = crate::agent::loop_commands::shell_arg_for(&from.to_string_lossy(), windows);
    let quoted_out = crate::agent::loop_commands::shell_arg_for(&out.to_string_lossy(), windows);
    let shell_hint = if windows {
        ". On Windows, use Git Bash (not WSL) if a path contains $, backticks or %, or this command uses printf escape fragments. Command Prompt does not support those forms."
    } else {
        ""
    };
    format!(
        "{} already exists; rerun `ripr baseline create {from_flag} {quoted_from} --out {quoted_out} --force` to overwrite it{shell_hint}",
        out.display()
    )
}

fn baseline_diff(args: &[String]) -> Result<(), String> {
    let options = parse_baseline_diff_options(args)?;
    let baseline_path = output::baseline_delta::display_path(&options.baseline);
    let current_path = output::baseline_delta::display_path(&options.current);
    let baseline_json = read_optional_text_for_report("baseline", &options.baseline);
    let current_json = read_optional_text_for_report("current gate-decision", &options.current);
    let report = output::baseline_delta::build_baseline_delta_report(
        output::baseline_delta::BaselineDeltaInput {
            root: ".".to_string(),
            baseline_path,
            current_gate_decision_path: current_path,
            baseline_json,
            current_gate_decision_json: current_json,
        },
    );
    let rendered_json = output::baseline_delta::render_baseline_delta_json(&report)?;
    let rendered_md = output::baseline_delta::render_baseline_delta_markdown(&report);
    write_text_file(&options.out, &rendered_json)?;
    write_text_file(&options.out_md, &rendered_md)?;
    println!("Wrote {}", output::path::human_path(&options.out));
    println!("Wrote {}", output::path::human_path(&options.out_md));
    println!(
        "Items: {}",
        output::baseline_delta::baseline_delta_item_count(&report)
    );
    // Missing-input warnings ride stderr like every other advisory surface
    // (#9856): a run that routed from nothing must not look like a healthy
    // "no debt movement" run in the terminal or in `2>&1` CI logs.
    super::emit_report_warnings(output::baseline_delta::baseline_delta_warnings(&report));
    Ok(())
}

fn baseline_update(args: &[String]) -> Result<(), String> {
    let options = parse_baseline_update_options(args)?;
    if !options.remove_resolved && !options.migrate_legacy_identities {
        return Err(
            "baseline update requires --remove-resolved or --migrate-legacy-identities; adopting new debt is not supported; pass `--remove-resolved` or `--migrate-legacy-identities`"
                .to_string(),
        );
    }
    let baseline_path = output::baseline_update::display_path(&options.baseline);
    let current_path = output::baseline_update::display_path(&options.current);
    let baseline_json = crate::bounded_input::read_required_input(
        "read baseline update baseline",
        &options.baseline,
        Some(BASELINE_LEDGER_RECOVERY),
    )?;
    let current_json = crate::bounded_input::read_required_input(
        "read baseline update current gate-decision",
        &options.current,
        Some(GATE_DECISION_RECOVERY),
    )?;
    let report = output::baseline_update::build_baseline_update_remove_resolved(
        output::baseline_update::BaselineUpdateInput {
            baseline_path,
            current_gate_decision_path: current_path,
            baseline_json,
            current_gate_decision_json: current_json,
            remove_resolved: options.remove_resolved,
            migrate_legacy_identities: options.migrate_legacy_identities,
        },
    )?;
    let rendered = output::baseline_update::render_baseline_update_json(&report)?;
    let out = options.out.unwrap_or_else(|| options.baseline.clone());
    write_text_file(&out, &rendered)?;
    println!("Wrote {}", output::path::human_path(&out));
    println!(
        "Entries: {} -> {}",
        output::baseline_update::baseline_update_before_entry_count(&report),
        output::baseline_update::baseline_update_after_entry_count(&report)
    );
    println!(
        "Removed resolved: {}",
        output::baseline_update::baseline_update_removed_resolved_count(&report)
    );
    println!(
        "Ignored new current: {}",
        output::baseline_update::baseline_update_ignored_new_current_count(&report)
    );
    if options.migrate_legacy_identities {
        println!(
            "Migrated legacy identities: {}",
            output::baseline_update::baseline_update_migrated_count(&report)
        );
    }
    if output::baseline_update::baseline_update_warning_count(&report) > 0 {
        eprintln!(
            "Warnings: {}",
            output::baseline_update::baseline_update_warning_count(&report)
        );
    }
    Ok(())
}

fn parse_baseline_create_options(args: &[String]) -> Result<BaselineCreateOptions, String> {
    let mut parse = BaselineCreateParseState::default();

    let mut i = 0usize;
    while i < args.len() {
        parse.apply_arg(args, &mut i)?;
        i += 1;
    }

    parse.into_options()
}

#[derive(Debug)]
struct BaselineCreateParseState {
    from: Option<PathBuf>,
    from_check: bool,
    out: PathBuf,
    dry_run: bool,
    force: bool,
}

impl Default for BaselineCreateParseState {
    fn default() -> Self {
        Self {
            from: None,
            from_check: false,
            out: PathBuf::from(output::baseline::DEFAULT_BASELINE_OUT),
            dry_run: false,
            force: false,
        }
    }
}

impl BaselineCreateParseState {
    fn apply_arg(&mut self, args: &[String], i: &mut usize) -> Result<(), String> {
        match args[*i].as_str() {
            "--from" => self.parse_from(args, i, false),
            "--from-check" => self.parse_from(args, i, true),
            "--out" => self.parse_out(args, i),
            "--dry-run" => {
                self.dry_run = true;
                Ok(())
            }
            "--force" => {
                self.force = true;
                Ok(())
            }
            other => Err(unknown_argument("baseline create", other)),
        }
    }

    fn parse_from(
        &mut self,
        args: &[String],
        i: &mut usize,
        from_check: bool,
    ) -> Result<(), String> {
        let flag = if from_check { "--from-check" } else { "--from" };
        if self.from.is_some() {
            return Err(format!(
                "baseline create takes exactly one source: pass either --from <gate-decision.json> or --from-check <check.json>, once ({flag} repeated or combined)"
            ));
        }
        *i += 1;
        self.from = Some(non_empty_path_arg(args, *i, flag, "baseline create")?);
        self.from_check = from_check;
        Ok(())
    }

    fn parse_out(&mut self, args: &[String], i: &mut usize) -> Result<(), String> {
        *i += 1;
        self.out = non_empty_path_arg(args, *i, "--out", "baseline create")?;
        Ok(())
    }

    fn into_options(self) -> Result<BaselineCreateOptions, String> {
        Ok(BaselineCreateOptions {
            from: self.from.ok_or_else(|| {
                "baseline create requires --from <gate-decision.json> or --from-check <check.json>"
                    .to_string()
            })?,
            from_check: self.from_check,
            out: self.out,
            dry_run: self.dry_run,
            force: self.force,
        })
    }
}

fn parse_baseline_diff_options(args: &[String]) -> Result<BaselineDiffOptions, String> {
    let mut baseline = None;
    let mut current = None;
    let mut out = PathBuf::from(output::baseline_delta::DEFAULT_BASELINE_DELTA_OUT);
    let mut out_md = PathBuf::from(output::baseline_delta::DEFAULT_BASELINE_DELTA_MD_OUT);

    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--baseline" => {
                i += 1;
                baseline = Some(baseline_diff_path_arg(args, i, "--baseline")?);
            }
            "--current" => {
                i += 1;
                current = Some(baseline_diff_path_arg(args, i, "--current")?);
            }
            "--out" => {
                i += 1;
                out = baseline_diff_path_arg(args, i, "--out")?;
            }
            "--out-md" => {
                i += 1;
                out_md = baseline_diff_path_arg(args, i, "--out-md")?;
            }
            other => return Err(unknown_argument("baseline diff", other)),
        }
        i += 1;
    }

    Ok(BaselineDiffOptions {
        baseline: baseline.ok_or_else(|| {
            "baseline diff requires --baseline <path>; add `--baseline <path>` to the command"
                .to_string()
        })?,
        current: current.ok_or_else(|| {
            "baseline diff requires --current <path>; add `--current <path>` to the command"
                .to_string()
        })?,
        out,
        out_md,
    })
}

// A following option is a missing value, not an advisory input filename.
// Literal option-shaped filenames remain available as `./--name`.
fn baseline_diff_path_arg(args: &[String], index: usize, flag: &str) -> Result<PathBuf, String> {
    if args.get(index).is_some_and(|value| value.starts_with("--")) {
        return Err(format!(
            "missing value for {flag}; pass a value right after it"
        ));
    }
    non_empty_path_arg(args, index, flag, "baseline diff")
}

fn parse_baseline_update_options(args: &[String]) -> Result<BaselineUpdateOptions, String> {
    let mut baseline = None;
    let mut current = None;
    let mut out = None;
    let mut remove_resolved = false;
    let mut migrate_legacy_identities = false;

    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--baseline" => {
                i += 1;
                baseline = Some(non_empty_path_arg(
                    args,
                    i,
                    "--baseline",
                    "baseline update",
                )?);
            }
            "--current" => {
                i += 1;
                current = Some(non_empty_path_arg(args, i, "--current", "baseline update")?);
            }
            "--out" => {
                i += 1;
                out = Some(non_empty_path_arg(args, i, "--out", "baseline update")?);
            }
            "--remove-resolved" => remove_resolved = true,
            "--migrate-legacy-identities" => migrate_legacy_identities = true,
            other => return Err(unknown_argument("baseline update", other)),
        }
        i += 1;
    }

    Ok(BaselineUpdateOptions {
        baseline: baseline.ok_or_else(|| {
            "baseline update requires --baseline <path>; add `--baseline <path>` to the command"
                .to_string()
        })?,
        current: current.ok_or_else(|| {
            "baseline update requires --current <path>; add `--current <path>` to the command"
                .to_string()
        })?,
        out,
        remove_resolved,
        migrate_legacy_identities,
    })
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::super::tests::{args, repo_root, unique_command_test_dir};
    use super::*;

    #[test]
    fn baseline_create_overwrite_discloses_windows_shell_routing() {
        for (from, quoted_from) in [
            (r"fixtures\gate$it's.json", r"'fixtures\gate$it'\''s.json'"),
            (r"fixtures\gate`it's.json", r"'fixtures\gate`it'\''s.json'"),
            (r"fixtures\gate%it's.json", r#""fixtures\gate%it's.json""#),
            (r"fixtures\gate.json", r#""fixtures\gate.json""#),
            (
                "fixtures\\gate\u{202e}.json",
                r#"'fixtures\gate'"$(printf '\342\200\256')"'.json'"#,
            ),
        ] {
            assert_eq!(
                baseline_create_overwrite_error(
                    Path::new(from),
                    false,
                    Path::new(r"fixtures\chosen baseline.json"),
                    true,
                ),
                format!(
                    "fixtures\\chosen baseline.json already exists; rerun `ripr baseline create --from {quoted_from} --out \"fixtures\\chosen baseline.json\" --force` to overwrite it. On Windows, use Git Bash (not WSL) if a path contains $, backticks or %, or this command uses printf escape fragments. Command Prompt does not support those forms."
                )
            );
        }
    }

    #[test]
    fn baseline_create_overwrite_keeps_posix_retry_without_windows_hint() {
        assert_eq!(
            baseline_create_overwrite_error(
                Path::new("gate $it's.json"),
                false,
                Path::new("chosen baseline.json"),
                false,
            ),
            r"chosen baseline.json already exists; rerun `ripr baseline create --from 'gate $it'\''s.json' --out 'chosen baseline.json' --force` to overwrite it"
        );
    }

    #[test]
    fn baseline_create_missing_source_names_path_and_recovery_route() -> Result<(), String> {
        let err = baseline(&args(&[
            "create",
            "--from",
            "missing-gate-decision.json",
            "--out",
            "unused-baseline.json",
        ]))
        .err()
        .ok_or("missing --from source must fail")?;
        assert!(
            err.starts_with("read baseline create source missing-gate-decision.json failed:"),
            "{err}"
        );
        assert!(err.contains("not found"), "{err}");
        assert!(
            err.contains("ripr gate evaluate --gap-ledger"),
            "error should name the gate-evaluate recovery route, got: {err}"
        );
        assert!(!err.contains("os error"), "{err}");
        Ok(())
    }

    #[test]
    fn baseline_update_missing_current_names_recovery_route() -> Result<(), String> {
        let dir = unique_command_test_dir("baseline-update-missing-current");
        std::fs::create_dir_all(&dir).map_err(|error| format!("create dir: {error}"))?;
        let baseline_path = dir.join("baseline.json");
        std::fs::write(&baseline_path, "{\"entries\":[]}")
            .map_err(|error| format!("write baseline: {error}"))?;
        let err = baseline(&args(&[
            "update",
            "--baseline",
            &baseline_path.to_string_lossy(),
            "--current",
            "missing-gate-decision.json",
            "--remove-resolved",
        ]))
        .err()
        .ok_or("missing --current source must fail")?;
        assert!(
            err.starts_with("read baseline update current gate-decision"),
            "{err}"
        );
        assert!(err.contains("not found"), "{err}");
        assert!(
            err.contains("ripr gate evaluate --gap-ledger"),
            "error should name the gate-evaluate recovery route, got: {err}"
        );
        assert!(!err.contains("os error"), "{err}");
        std::fs::remove_dir_all(&dir).map_err(|error| format!("remove dir: {error}"))?;
        Ok(())
    }

    #[test]
    fn baseline_update_missing_baseline_points_at_baseline_create() -> Result<(), String> {
        // #9851 review follow-up: a missing --baseline ledger must name the
        // command that actually produces the reviewed historical-debt
        // ledger. The gate-evaluate route hands the caller a gate-decision
        // artifact whose kind the update consumer still rejects.
        let dir = unique_command_test_dir("baseline-update-missing-baseline");
        std::fs::create_dir_all(&dir).map_err(|error| format!("create dir: {error}"))?;
        let current_path = dir.join("current.json");
        std::fs::write(&current_path, "{\"entries\":[]}")
            .map_err(|error| format!("write current: {error}"))?;
        let err = baseline(&args(&[
            "update",
            "--baseline",
            "missing-baseline.json",
            "--current",
            &current_path.to_string_lossy(),
            "--remove-resolved",
        ]))
        .err()
        .ok_or("missing --baseline source must fail")?;
        assert!(err.starts_with("read baseline update baseline"), "{err}");
        assert!(err.contains("not found"), "{err}");
        assert!(
            err.contains("ripr baseline create --from"),
            "error should name the baseline-create recovery route, got: {err}"
        );
        assert!(
            !err.contains("ripr gate evaluate"),
            "the missing-baseline route must not point at gate evaluate, got: {err}"
        );
        assert!(!err.contains("os error"), "{err}");
        std::fs::remove_dir_all(&dir).map_err(|error| format!("remove dir: {error}"))?;
        Ok(())
    }

    #[test]
    fn baseline_diff_missing_inputs_complete_with_warnings() -> Result<(), String> {
        // #9856 contract: two missing inputs keep the documented exit-0
        // contract (the typed answer completed) while the warnings that were
        // previously report-only become visible on stderr; the report itself
        // is unchanged.
        let dir = unique_command_test_dir("baseline-diff-missing-inputs");
        std::fs::create_dir_all(&dir).map_err(|error| format!("create dir: {error}"))?;
        let missing_a = dir.join("missing-a.json");
        let missing_b = dir.join("missing-b.json");
        let out = dir.join("baseline-debt-delta.json");
        let out_md = dir.join("baseline-debt-delta.md");
        baseline(&args(&[
            "diff",
            "--baseline",
            &missing_a.display().to_string(),
            "--current",
            &missing_b.display().to_string(),
            "--out",
            &out.display().to_string(),
            "--out-md",
            &out_md.display().to_string(),
        ]))
        .map_err(|error| format!("missing-input baseline diff must complete (exit 0): {error}"))?;
        let json = std::fs::read_to_string(&out)
            .map_err(|error| format!("read baseline delta JSON: {error}"))?;
        let value: serde_json::Value = serde_json::from_str(&json)
            .map_err(|error| format!("baseline delta JSON must parse: {error}"))?;
        let warnings = value
            .get("warnings")
            .and_then(serde_json::Value::as_array)
            .ok_or("baseline delta JSON must carry a warnings array")?;
        assert!(!warnings.is_empty(), "warnings must not be empty: {json}");
        for missing in [&missing_a, &missing_b] {
            let missing_display = output::baseline_delta::display_path(missing);
            assert!(
                warnings.iter().any(|warning| warning
                    .as_str()
                    .is_some_and(|text| text.contains(&missing_display))),
                "warnings must name the missing input {missing_display}: {json}"
            );
        }
        // The stderr surface renders the same warnings: non-empty and naming
        // the missing inputs.
        let rendered = super::super::render_report_warnings(
            &warnings
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>(),
        );
        assert!(!rendered.is_empty(), "stderr warnings must be non-empty");
        assert!(
            rendered.contains(&output::baseline_delta::display_path(&missing_a)),
            "{rendered}"
        );
        std::fs::remove_dir_all(&dir).map_err(|error| format!("remove dir: {error}"))?;
        Ok(())
    }

    #[test]
    fn baseline_create_parses_option_surface() {
        assert_eq!(
            parse_baseline_create_options(&args(&[
                "--from",
                "target/ripr/reports/gate-decision.json",
                "--out",
                ".ripr/gate-baseline.json",
                "--dry-run",
                "--force",
            ])),
            Ok(BaselineCreateOptions {
                from: PathBuf::from("target/ripr/reports/gate-decision.json"),
                from_check: false,
                out: PathBuf::from(".ripr/gate-baseline.json"),
                dry_run: true,
                force: true,
            })
        );
        assert_eq!(
            parse_baseline_create_options(&args(&["--from", "gate.json"])),
            Ok(BaselineCreateOptions {
                from: PathBuf::from("gate.json"),
                from_check: false,
                out: PathBuf::from(".ripr/gate-baseline.json"),
                dry_run: false,
                force: false,
            })
        );
    }

    #[test]
    fn baseline_create_from_check_parses_and_names_its_own_flag() {
        assert_eq!(
            parse_baseline_create_options(&args(&["--from-check", "check.json"])),
            Ok(BaselineCreateOptions {
                from: PathBuf::from("check.json"),
                from_check: true,
                out: PathBuf::from(".ripr/gate-baseline.json"),
                dry_run: false,
                force: false,
            })
        );
        assert_eq!(
            parse_baseline_create_options(&args(&["--from-check", ""])),
            Err("baseline create --from-check requires a non-empty value".to_string())
        );
        assert_eq!(
            baseline_create_overwrite_error(
                Path::new("check.json"),
                true,
                Path::new("b.json"),
                false
            ),
            "b.json already exists; rerun `ripr baseline create --from-check check.json --out b.json --force` to overwrite it"
        );
    }

    #[test]
    fn baseline_create_takes_exactly_one_source() {
        for argv in [
            ["--from", "a.json", "--from-check", "b.json"],
            ["--from-check", "b.json", "--from", "a.json"],
            ["--from", "a.json", "--from", "b.json"],
            ["--from-check", "a.json", "--from-check", "b.json"],
        ] {
            let error = parse_baseline_create_options(&args(&argv)).err();
            assert!(
                error
                    .as_deref()
                    .is_some_and(|text| text.contains("exactly one source")),
                "{argv:?} -> {error:?}"
            );
        }
    }

    #[test]
    fn baseline_create_requires_source_and_rejects_unknown_args() {
        assert_eq!(baseline(&args(&[])), Ok(()));
        assert_eq!(
            baseline(&args(&["unknown"])),
            Err(
                "unknown baseline subcommand \"unknown\". Accepted: create, diff, update."
                    .to_string()
            )
        );
        assert_eq!(
            parse_baseline_create_options(&args(&[])),
            Err(
                "baseline create requires --from <gate-decision.json> or --from-check <check.json>"
                    .to_string()
            )
        );
        assert_eq!(
            parse_baseline_create_options(&args(&["--from", ""])),
            Err("baseline create --from requires a non-empty value".to_string())
        );
        assert_eq!(
            parse_baseline_create_options(&args(&["--bad"])),
            Err(
                "unknown baseline create argument \"--bad\". Run `ripr baseline create --help`."
                    .to_string()
            )
        );
    }

    #[test]
    fn unknown_baseline_subcommand_suggests_create_for_creat() {
        assert_eq!(
            baseline(&args(&["creat"])),
            Err(
                "unknown baseline subcommand \"creat\". Did you mean `create`? Accepted: create, diff, update."
                    .to_string()
            )
        );
    }

    #[test]
    fn baseline_diff_parses_option_surface() {
        assert_eq!(
            parse_baseline_diff_options(&args(&[
                "--baseline",
                ".ripr/gate-baseline.json",
                "--current",
                "target/ripr/reports/gate-decision.json",
                "--out",
                "target/ripr/reports/baseline-debt-delta.json",
                "--out-md",
                "target/ripr/reports/baseline-debt-delta.md",
            ])),
            Ok(BaselineDiffOptions {
                baseline: PathBuf::from(".ripr/gate-baseline.json"),
                current: PathBuf::from("target/ripr/reports/gate-decision.json"),
                out: PathBuf::from("target/ripr/reports/baseline-debt-delta.json"),
                out_md: PathBuf::from("target/ripr/reports/baseline-debt-delta.md"),
            })
        );
    }

    #[test]
    fn baseline_diff_requires_inputs_and_rejects_unknown_args() {
        assert_eq!(
            parse_baseline_diff_options(&args(&[])),
            Err(
                "baseline diff requires --baseline <path>; add `--baseline <path>` to the command"
                    .to_string()
            )
        );
        assert_eq!(
            parse_baseline_diff_options(&args(&["--baseline", ".ripr/gate-baseline.json"])),
            Err(
                "baseline diff requires --current <path>; add `--current <path>` to the command"
                    .to_string()
            )
        );
        assert_eq!(
            parse_baseline_diff_options(&args(&["--baseline", ""])),
            Err("baseline diff --baseline requires a non-empty value".to_string())
        );
        assert_eq!(
            parse_baseline_diff_options(&args(&["--bad"])),
            Err(
                "unknown baseline diff argument \"--bad\". Run `ripr baseline diff --help`."
                    .to_string()
            )
        );
    }

    #[test]
    fn baseline_update_parses_option_surface() {
        assert_eq!(
            parse_baseline_update_options(&args(&[
                "--baseline",
                ".ripr/gate-baseline.json",
                "--current",
                "target/ripr/reports/gate-decision.json",
                "--remove-resolved",
                "--out",
                ".ripr/gate-baseline.updated.json",
            ])),
            Ok(BaselineUpdateOptions {
                baseline: PathBuf::from(".ripr/gate-baseline.json"),
                current: PathBuf::from("target/ripr/reports/gate-decision.json"),
                out: Some(PathBuf::from(".ripr/gate-baseline.updated.json")),
                remove_resolved: true,
                migrate_legacy_identities: false,
            })
        );
        assert_eq!(
            parse_baseline_update_options(&args(&[
                "--baseline",
                ".ripr/gate-baseline.json",
                "--current",
                "target/ripr/reports/gate-decision.json",
                "--migrate-legacy-identities",
            ])),
            Ok(BaselineUpdateOptions {
                baseline: PathBuf::from(".ripr/gate-baseline.json"),
                current: PathBuf::from("target/ripr/reports/gate-decision.json"),
                out: None,
                remove_resolved: false,
                migrate_legacy_identities: true,
            })
        );
        assert_eq!(
            parse_baseline_update_options(&args(&[
                "--baseline",
                ".ripr/gate-baseline.json",
                "--current",
                "target/ripr/reports/gate-decision.json",
            ])),
            Ok(BaselineUpdateOptions {
                baseline: PathBuf::from(".ripr/gate-baseline.json"),
                current: PathBuf::from("target/ripr/reports/gate-decision.json"),
                out: None,
                remove_resolved: false,
                migrate_legacy_identities: false,
            })
        );
    }

    #[test]
    fn baseline_update_requires_inputs_remove_resolved_and_rejects_unknown_args() {
        assert_eq!(
            parse_baseline_update_options(&args(&[])),
            Err("baseline update requires --baseline <path>; add `--baseline <path>` to the command".to_string())
        );
        assert_eq!(
            parse_baseline_update_options(&args(&["--baseline", ".ripr/gate-baseline.json"])),
            Err(
                "baseline update requires --current <path>; add `--current <path>` to the command"
                    .to_string()
            )
        );
        assert_eq!(
            parse_baseline_update_options(&args(&["--baseline", ""])),
            Err("baseline update --baseline requires a non-empty value".to_string())
        );
        assert_eq!(
            parse_baseline_update_options(&args(&["--bad"])),
            Err(
                "unknown baseline update argument \"--bad\". Run `ripr baseline update --help`."
                    .to_string()
            )
        );
        assert_eq!(
            parse_baseline_update_options(&args(&["--adopt-new"])),
            Err("unknown baseline update argument \"--adopt-new\". Run `ripr baseline update --help`.".to_string())
        );
        assert_eq!(
            baseline(&args(&[
                "update",
                "--baseline",
                ".ripr/gate-baseline.json",
                "--current",
                "target/ripr/reports/gate-decision.json",
            ])),
            Err(
                "baseline update requires --remove-resolved or --migrate-legacy-identities; adopting new debt is not supported; pass `--remove-resolved` or `--migrate-legacy-identities`"
                    .to_string()
            )
        );
    }

    #[test]
    fn baseline_create_writes_baseline_without_overwriting_by_default() -> Result<(), String> {
        let dir = unique_command_test_dir("baseline-create");
        std::fs::create_dir_all(&dir).map_err(|err| format!("create baseline dir: {err}"))?;
        let out = dir.join("gate-baseline.json");
        let from = repo_root().join(
            "fixtures/boundary_gap/expected/calibrated-gate/visible-only-advisory/gate-decision.json",
        );
        baseline(&args(&[
            "create",
            "--from",
            &from.display().to_string(),
            "--out",
            &out.display().to_string(),
        ]))?;

        let json_text =
            std::fs::read_to_string(&out).map_err(|err| format!("read baseline json: {err}"))?;
        assert!(json_text.contains("\"kind\": \"gate_baseline\""));
        assert!(json_text.contains("\"reviewed\": false"));
        assert!(json_text.contains("\"source_report\""));
        assert!(json_text.contains("\"seam_id\": \"8f7fa8644fd12280\""));
        assert!(json_text.contains("\"entries\": 1"));

        let second = baseline(&args(&[
            "create",
            "--from",
            &from.display().to_string(),
            "--out",
            &out.display().to_string(),
        ]));
        assert!(matches!(second, Err(message) if message.contains("--force")));

        baseline(&args(&[
            "create",
            "--from",
            &from.display().to_string(),
            "--out",
            &out.display().to_string(),
            "--force",
        ]))?;

        std::fs::remove_dir_all(&dir).map_err(|err| format!("remove baseline dir: {err}"))?;
        Ok(())
    }
}
