use crate::analysis;
use crate::app::{self, CheckInput, Mode, OutputFormat};
use crate::cli::commands_numeric::{parse_positive_u64, parse_positive_usize};
use crate::cli::commands_options::{PilotOptions, PilotStdoutFormat};
use crate::cli::help;
use crate::cli::parse::{expect_value, parse_mode};
use crate::cli::progress::{CliProgressSink, ProgressPolicy};
use crate::cli::suggest::{unknown_argument, unknown_value};
use crate::config::{CheckInputExplicit, RiprConfig, apply_to_check_input, load_for_root};
use crate::core_error::CoreError;
use crate::output;
use crate::output::human::terminal_safe;
use crate::output::repo_exposure::RepoExposureLimit;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

const DEFAULT_PILOT_TIMEOUT_MS: u64 = 30_000;

/// Budget for the auto-retry when the default timeout fires (#2424).
/// A cold fact cache on a multi-crate workspace can need ~155s; the
/// default 30s is too low for first-run. The retry budget is set high
/// enough to cover a cold cache on the ripr-swarm repo itself.
const PILOT_RETRY_TIMEOUT_MS: u64 = 240_000;

/// Write pilot's `repo-exposure.json`.
///
/// When pilot saw the same seam population `ripr check --format
/// repo-exposure-json` would, the snapshot carries the same producer-owned
/// `artifact` identity, so it can be the `--before` of `ripr agent verify`
/// that the first-PR workflow and the evidence records name (#3906). When the
/// pilot seam budget truncated the population, or pilot added the current
/// change's seams past the inventory seam limit (#6943), the snapshot is
/// written without that identity: a stamped snapshot of another population
/// would pass verify's comparability check against a `ripr check` after
/// snapshot and compare two different populations. Verify then refuses it
/// rather than misreporting.
fn write_pilot_repo_exposure_json(
    path: &Path,
    input: &CheckInput,
    classified: &[analysis::ClassifiedSeam],
    limit_info: Option<RepoExposureLimit<'_>>,
    generated_skip: Option<&output::repo_exposure::GeneratedRustSkip>,
    context: Option<&crate::agent::artifact::RepoExposureArtifactContext>,
) -> Result<(), String> {
    let ts_guidance = output::render::detect_ts_full_repo_guidance_pub(&input.root, classified);
    let python_guidance =
        output::render::detect_python_repo_exposure_guidance_pub(&input.root, classified);
    let write_failed = |err: String| format!("write {} failed: {err}", path.display());
    let Some(context) = context else {
        return write_pilot_file(
            path,
            output::repo_exposure::render_repo_exposure_json_with_limit(
                classified,
                limit_info,
                ts_guidance.as_ref(),
                python_guidance.as_ref(),
                generated_skip,
            ),
        );
    };
    let mut render_error = None;
    output::file_write::write_with(path, |file| {
        let mut writer = std::io::BufWriter::new(file);
        if let Err(err) = output::repo_exposure::write_repo_exposure_json_with_context(
            classified,
            limit_info.map(RepoExposureLimit::info),
            ts_guidance.as_ref(),
            python_guidance.as_ref(),
            generated_skip,
            context,
            &mut writer,
        ) {
            render_error = Some(err);
            return Err(std::io::Error::other("repo exposure rendering failed"));
        }
        std::io::Write::flush(&mut writer)
    })
    .map_err(|err| write_failed(render_error.take().unwrap_or_else(|| err.to_string())))
}

pub(in crate::cli) fn pilot(args: &[String]) -> Result<(), String> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        help::print_pilot_help();
        return Ok(());
    }

    let options = parse_pilot_options(args)?;
    if !options.root.is_dir() {
        return Err(format!(
            "pilot root {} is not a directory; pass the directory that contains the workspace (for a Cargo.toml path, its parent directory)",
            options.root.display()
        ));
    }

    let config = load_for_root(&options.root)?;
    // Refuse an invalid RIPR_PILOT_SEAM_BUDGET (#4529) before the analysis
    // it would bound, not after it.
    analysis::pilot_seam_budget()?;
    // RIPR_GIT_TIMEOUT bounds pilot's current-change git calls as it bounds
    // check's (#2613): seconds, `0` disables the deadline, and an invalid
    // value fails closed here rather than running with the default.
    let git_timeout = super::check::git_timeout_from_env(false, std::env::var("RIPR_GIT_TIMEOUT"))?
        .unwrap_or(Some(app::default_cli_git_timeout()));
    let mut input = CheckInput {
        root: options.root.clone(),
        mode: options.mode.clone(),
        ..CheckInput::default()
    };
    apply_to_check_input(&mut input, &config, options.explicit);

    let artifacts = pilot_artifacts(&options.out_dir);
    output::file_write::create_output_dir(&options.out_dir, "--out")?;

    // #5019: pilot's repo inventory is the same multi-minute walk `ripr
    // check` projects progress for, so route it through the shared
    // app-layer progress-bearing entry point instead of calling the
    // analyzer directly.
    //
    // When the default deadline fires and the user did not pass an explicit
    // --timeout-ms (#2424), the deadline extends on the same run instead of
    // cancelling it and starting over. A cold fact cache on a multi-crate
    // workspace can need minutes, and a cancelled first attempt leaves
    // nothing the restart can reuse, so restarting redid the first 30s of
    // work (serde cold: 75s with the restart, 44s without, identical
    // output).
    //
    // #5205: pilot ranks Rust seams only, so without Rust enabled it ranks
    // nothing. Determine that BEFORE the inventory (Codex P1): the walk
    // would analyze a disabled language for minutes, and on a large
    // workspace it can exhaust the deadline and return through the timeout
    // branch without ever emitting the exclusion. The bypassed report is
    // exactly what the post-inventory filter produced (empty classified,
    // no limit, no skips), so downstream artifacts are identical; only the
    // wasted walk is gone. Preview-language work is unaffected: it runs
    // its own checks below, outside the inventory.
    let rust_enabled = config
        .languages()
        .enabled()
        .contains(&crate::domain::LanguageId::Rust);
    let extension_ms = pilot_deadline_extension_ms(&options);
    let progress = pilot_progress_sink(options.quiet);
    let analysis_started = Instant::now();
    let analysis_result = if rust_enabled {
        run_pilot_analysis_with_timeout(
            options.timeout_ms,
            extension_ms,
            || {
                eprintln!(
                    "ripr: pilot still running after {}ms; extending the deadline by {}ms (a cold cache needs more time)...",
                    DEFAULT_PILOT_TIMEOUT_MS, PILOT_RETRY_TIMEOUT_MS
                );
            },
            {
                let root = input.root.clone();
                let cfg = config.clone();
                let sink = progress.as_ref().map(Arc::clone);
                move || run_pilot_inventory(&root, &cfg, sink.as_ref())
            },
        )?
    } else {
        PilotAnalysisResult::Complete(analysis::ClassifiedSeamsReport {
            classified: Vec::new(),
            corpus_key: None,
            limit_info: None,
            skipped_generated: Vec::new(),
            naming_only_skips: Vec::new(),
        })
    };
    // The timeout hint floors at the budget actually spent, so an
    // extended run does not suggest a smaller --timeout-ms than it used
    // (#5443). The printed retry also stays inside an interactive cap
    // instead of multiplying the spent budget by four (#7427).
    let spent_timeout_ms = options.timeout_ms.saturating_add(extension_ms.unwrap_or(0));
    let PilotAnalysisResult::Complete(report) = analysis_result else {
        // #7425: a timeout overwrites only the two summaries. Remove leftover
        // repo-exposure and seam-packet files from an earlier complete run so
        // the packet directory does not present them as this run's outputs.
        // Removal, not in-place stale markers: no schema change, and a
        // missing file cannot be read as current evidence.
        publish_timeout_packet(
            &artifacts,
            options.format,
            output::pilot::PilotSummaryContext {
                root: &input.root,
                mode: &input.mode,
                config_path: config.source_path(),
                max_seams: options.max_seams,
                timeout_ms: spent_timeout_ms,
                timeout_extension_ms: extension_ms.unwrap_or(0),
                artifacts: &artifacts,
                python_first_use: None,
                language_routes: None,
                current_change: None,
                seam_limit: None,
            },
        )?;
        return Ok(());
    };

    // #5205: the inventory was bypassed above when Rust is disabled, so a
    // disabled run always arrives here with an empty report. Disclose only
    // when the exclusion changed the result meaning: Rust files exist but
    // were not analyzed.
    let rust_files = analysis::workspace_rust_files(&input.root);
    let rust_excluded = (!rust_enabled && !rust_files.is_empty()).then_some(rust_files.len());

    // Apply the pilot artifact seam budget.  The inventory may already have
    // been capped by the repo-exposure seam limit; we then further cap the
    // classified slice for the two pilot artifacts so they stay under a
    // manageable size.  `limit_info` carries whichever cap fired (pilot
    // budget wins when both fire; inventory limit is the outer bound).
    let mut classified = report.classified;
    let corpus_key = report.corpus_key;
    let mut inventory_limit_info = report.limit_info;
    let generated_skip = output::repo_exposure::GeneratedRustSkip::from_paths(
        report.skipped_generated,
        report.naming_only_skips,
    );
    // The current change is loaded before the budget cut so an actionable
    // changed seam past the cut is kept and can still rank change-first. A
    // changed seam pilot cannot recommend is not kept: it would displace an
    // actionable seam and leave nothing to recommend.
    let current_change = load_pilot_current_change(&input, git_timeout);
    // #6943: the inventory seam limit cuts before the change is known, so on
    // a large repo it can drop the very seams the developer changed. Classify
    // the change's own files in what is left of pilot's deadline and add its
    // cut seams; they count as analyzed. A failed or late classification
    // never fails pilot: the seam-limit caveat on the change stays.
    let change_classification = classify_change_past_seam_limit(
        &input.root,
        &config,
        &current_change,
        inventory_limit_info.is_some(),
        spent_timeout_ms.saturating_sub(elapsed_ms(analysis_started)),
        !options.quiet,
    );
    // #6944: name the changed files `ripr check` analyzes but the repo
    // inventory leaves out (build scripts, `xtask/`, roots outside `src`),
    // so a change there is not read as a change with no seams. It runs after
    // the change's own classification, so this explanation never spends the
    // deadline that ranks changed seams. Without Rust enabled neither
    // command analyzes these files, so there is nothing to name.
    let current_change = if rust_enabled {
        let diff_only = name_diff_only_files(
            &input.root,
            &config,
            &current_change,
            spent_timeout_ms.saturating_sub(elapsed_ms(analysis_started)),
        );
        current_change.with_diff_only_files(diff_only)
    } else {
        current_change
    };
    let folded = current_change.fold_classified_change(
        &mut classified,
        &mut inventory_limit_info,
        change_classification,
    );
    if let Some(error) = &folded.error {
        eprintln!(
            "{}",
            terminal_safe(format!(
                "ripr pilot: could not classify the current change past the seam limit ({error})"
            ))
        );
    }
    // A snapshot holding seams `ripr check` never classifies is not the same
    // population as its after snapshot, so it is written without identity.
    let change_seams_added = folded.added > 0;
    // Source admission is independent of the inventory cap. Without a
    // supplement (or after a failed one), an absent changed source still
    // needs the shared disclosure; observing file presence does not clear
    // the failed supplement's classification caveat.
    let absent_file_limitations = if let Some(limitations) = folded.absent_file_limitations {
        Some(limitations)
    } else if rust_enabled {
        if current_change.changed_rust_files().is_empty() {
            None
        } else {
            Some(analysis::absent_changed_rust_file_limitations(
                &input.root,
                &config,
                &current_change.changed_files(),
            )?)
        }
    } else {
        None
    };
    let current_change = current_change
        .with_absent_file_limitations(absent_file_limitations)
        .with_seams_counted(&classified, folded.caveat_limit);
    let pilot_budget_info = analysis::apply_pilot_seam_budget(&mut classified, |entry| {
        current_change.keeps_past_budget(entry)
    })?;
    let pilot_budget_truncated = pilot_budget_info.is_some();
    // The summary's disclosure counts from the outermost population: when
    // both caps fired, the pilot budget's total is the already-capped
    // inventory, which would hide the seams the inventory limit dropped.
    let summary_limit = match (&pilot_budget_info, &inventory_limit_info) {
        (Some(budget), Some(inventory)) => Some(analysis::SeamLimitInfo {
            analyzed: budget.analyzed,
            total: inventory.total,
            source: budget.source.clone(),
        }),
        (budget, inventory) => budget.clone().or_else(|| inventory.clone()),
    };
    let limit_info = pilot_budget_info.or(inventory_limit_info);
    let repo_exposure_limit = limit_info.as_ref().map(|info| {
        if pilot_budget_truncated {
            RepoExposureLimit::PilotBudget(info)
        } else {
            RepoExposureLimit::Inventory(info)
        }
    });
    let (causal_projection, causal_projection_warning) =
        crate::app::causal_projection::CausalDeltaArtifact::load_optional(&input.root);
    if let Some(warning) = causal_projection_warning {
        eprintln!("{}", terminal_safe(format!("ripr pilot: {warning}")));
    }

    let python_first_use = collect_pilot_python_first_use(&input, &config);
    // #3906 / #7665: pilot ranks Rust seams only. Name the languages it did
    // not rank so an empty ranking is never read as a clean result for them;
    // when an enabled supported language is present, the headline names its
    // `ripr check` route instead of "none".
    let language_routes = output::pilot::PilotLanguageRoutes::from_discovered(
        &input.root,
        !classified.is_empty(),
        config.languages().enabled(),
        &analysis::workspace_preview_language_files(&input.root),
    )
    .with_unanalyzed(
        analysis::workspace_unanalyzed_source_languages(&input.root),
        !rust_files.is_empty(),
    )
    .with_rust_exclusion(rust_excluded);
    let context = output::pilot::PilotSummaryContext {
        root: &input.root,
        mode: &input.mode,
        config_path: config.source_path(),
        max_seams: options.max_seams,
        timeout_ms: options.timeout_ms,
        timeout_extension_ms: 0,
        artifacts: &artifacts,
        python_first_use: python_first_use.as_ref(),
        language_routes: Some(&language_routes),
        current_change: Some(&current_change),
        seam_limit: summary_limit.as_ref(),
    };

    let ts_guidance = output::render::detect_ts_full_repo_guidance_pub(&input.root, &classified);
    let python_guidance =
        output::render::detect_python_repo_exposure_guidance_pub(&input.root, &classified);
    // The existing context is available only for the check-comparable full
    // population. Keep base None, matching pilot's printed after command.
    let snapshot_context = if pilot_budget_truncated || change_seams_added {
        None
    } else {
        Some(
            crate::agent::artifact::RepoExposureArtifactContext::for_repo_exposure(
                input.root.clone(),
                input.mode.as_str().to_string(),
                None,
                &config,
            )?
            .with_rust_corpus_key(corpus_key.as_ref()),
        )
    };
    write_pilot_repo_exposure_json(
        &artifacts.repo_exposure_json,
        &input,
        &classified,
        repo_exposure_limit,
        generated_skip.as_ref(),
        snapshot_context.as_ref(),
    )?;
    write_pilot_file(
        &artifacts.repo_exposure_md,
        output::repo_exposure::render_repo_exposure_md_with_limit(
            &classified,
            repo_exposure_limit,
            ts_guidance.as_ref(),
            python_guidance.as_ref(),
            generated_skip.as_ref(),
        ),
    )?;
    write_pilot_file(
        &artifacts.agent_seam_packets_json,
        output::agent_seam_packets::render_agent_seam_packets_json_for_root(
            &classified,
            repo_exposure_limit,
            causal_projection.as_ref(),
            &crate::agent::loop_commands::bound_root(&options.root.to_string_lossy()),
        ),
    )?;

    let summary_json = output::pilot::render_pilot_summary_json(&classified, context);
    write_pilot_file(&artifacts.pilot_summary_json, &summary_json)?;
    write_pilot_file(
        &artifacts.pilot_summary_md,
        output::pilot::render_pilot_summary_md(&classified, context),
    )?;

    print!(
        "{}",
        pilot_stdout(
            options.format,
            &summary_json,
            &output::pilot::render_pilot_terminal(&classified, context),
        )
    );
    // #5019: producer `completed` was held until every artifact write and
    // the terminal render succeeded; commit it now. An earlier failure
    // already dropped the sink, which projected `failed` instead.
    if let Some(sink) = progress.as_ref() {
        sink.commit_success();
    }
    Ok(())
}

/// Classify the current change's Rust files on their own when the
/// inventory seam limit fired (#6943), under the `remaining_ms` left of
/// pilot's deadline. `None` when the limit did not fire or the change has no
/// Rust file; an error (including running out of time) when the change could
/// not be classified, which pilot reports without failing.
fn classify_change_past_seam_limit(
    root: &Path,
    config: &RiprConfig,
    change: &output::pilot::PilotCurrentChange,
    inventory_limited: bool,
    remaining_ms: u64,
    announce: bool,
) -> Option<Result<analysis::ClassifiedChangeReport, String>> {
    if !inventory_limited {
        return None;
    }
    let files = change.changed_rust_files();
    if files.is_empty() {
        return None;
    }
    if remaining_ms == 0 {
        return Some(Err("the inventory used pilot's whole deadline".to_string()));
    }
    if announce {
        eprintln!(
            "ripr: pilot hit the seam limit; classifying the current change's {} Rust file(s) on their own...",
            files.len()
        );
    }
    let (root, config, change) = (root.to_path_buf(), config.clone(), change.clone());
    let changed_paths = change.changed_files();
    Some(
        match run_pilot_analysis_with_timeout(
            remaining_ms,
            None,
            || {},
            move || {
                analysis::classify_seams_in_files_at_with_config(
                    &root,
                    &config,
                    &files,
                    &changed_paths,
                    &|seam| change.touches_seam(seam),
                )
            },
        ) {
            Ok(PilotAnalysisResult::Complete(classified)) => Ok(classified),
            Ok(PilotAnalysisResult::TimedOut) => {
                Err("it did not finish within pilot's deadline".to_string())
            }
            Err(error) => Err(error),
        },
    )
}

/// The change's diff-only files (#6944), under the `remaining_ms` left of
/// pilot's deadline: the module-graph evidence behind them can read the
/// whole corpus. A late or failed lookup names nothing, so pilot keeps the
/// plain "no seam pilot analyzed" reason rather than naming a file it did
/// not establish. The miss is reported on stderr even under `--quiet`,
/// which silences progress, not failures.
fn name_diff_only_files(
    root: &Path,
    config: &RiprConfig,
    change: &output::pilot::PilotCurrentChange,
    remaining_ms: u64,
) -> Vec<(PathBuf, analysis::DiffOnlySource)> {
    let files = change.changed_rust_files();
    if files.is_empty() {
        return Vec::new();
    }
    let missed = || {
        eprintln!(
            "ripr pilot: could not check the current change for build scripts or xtask files within its deadline"
        );
        Vec::new()
    };
    if remaining_ms == 0 {
        return missed();
    }
    let (root, config) = (root.to_path_buf(), config.clone());
    match run_pilot_analysis_with_timeout(
        remaining_ms,
        None,
        || {},
        move || Ok(analysis::diff_only_rust_files(&root, &config, &files)),
    ) {
        Ok(PilotAnalysisResult::Complete(named)) => named,
        Ok(PilotAnalysisResult::TimedOut) | Err(_) => missed(),
    }
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// Build the stderr progress sink for one pilot analysis attempt (#5019).
///
/// `--quiet` opts out of the progress stream per the #2608 contract; the
/// one-shot timeout notice and command errors still reach stderr. The
/// sink is terminal-once, so a timed-out attempt (or its #2424 retry)
/// needs a fresh one from `pilot_progress_sink`.
fn pilot_progress_sink(quiet: bool) -> Option<Arc<CliProgressSink>> {
    if quiet {
        return None;
    }
    Some(Arc::new(CliProgressSink::for_stderr(
        std::io::stderr().is_terminal(),
        ProgressPolicy::STANDARD,
    )))
}

/// Run the pilot repo inventory through the shared app-layer
/// progress-bearing entry point (#5019): the same producer-owned
/// `loading_input`/`analyzing`/`building_output`/`completed` boundaries
/// `ripr check` projects, at repo scope, so a cold multi-minute first
/// run shows bounded stage lines and throttled heartbeats instead of
/// minutes of silence. Stage identity stays producer-owned; pilot adds
/// no stage vocabulary of its own.
fn run_pilot_inventory(
    root: &Path,
    config: &RiprConfig,
    sink: Option<&Arc<CliProgressSink>>,
) -> Result<analysis::ClassifiedSeamsReport, String> {
    app::repo_inventory_with_progress(
        sink.map(|sink| &**sink as &dyn crate::app::AnalysisProgressSink),
        || analysis::inventory_classified_seams_report_at_with_config(root, config),
        Ok,
    )
}

/// The current change: the base (resolved as `ripr check` resolves it,
/// RIPR-SPEC-0084) against the live working tree when it has uncommitted
/// tracked changes, else `<base>...HEAD`. Plain `ripr check` reads committed
/// history only, so a working-tree change is recorded as such and pilot's
/// printed `ripr check` command carries `--worktree` to analyze the same
/// diff. Keep this the one place pilot picks its diff. Pilot ranks seams on
/// the changed lines first and says whether its top recommendation is part
/// of the change. A load failure (not a Git work tree, no resolvable default
/// base, a git error) never fails pilot: it is recorded as `unavailable` and
/// the ranking stays repo-wide.
fn load_pilot_current_change(
    input: &CheckInput,
    git_timeout: Option<std::time::Duration>,
) -> output::pilot::PilotCurrentChange {
    let mut from_working_tree = false;
    // Resolve the base first: a root with no resolvable base (outside a Git
    // work tree, say) is `unavailable` without running the working-tree
    // probe at all.
    let loaded = analysis::resolve_effective_base(&input.root, input.base.as_deref(), git_timeout)
        .map_err(|err| {
            current_change_unavailable_reason(&CoreError::from(err), "no default base resolved")
        })
        .and_then(|base| {
            // A failed probe is not a clean tree: loading `<base>...HEAD`
            // then would silently drop uncommitted edits from the change.
            from_working_tree =
                analysis::probe_working_tree_tracked_changes_within(&input.root, git_timeout)
                    .map_err(|err| {
                        current_change_unavailable_reason(
                            &CoreError::from(err),
                            "git status failed",
                        )
                    })?;
            if from_working_tree {
                analysis::load_worktree_diff_with_effective_base_core(
                    &input.root,
                    Some(&base),
                    git_timeout,
                )
            } else {
                analysis::load_diff_with_effective_base_core(
                    &input.root,
                    Some(&base),
                    None,
                    git_timeout,
                )
            }
            .map_err(|err| current_change_unavailable_reason(&err, "git diff failed"))
        })
        .map(|loaded| (loaded.text, loaded.effective_base));
    output::pilot::PilotCurrentChange::from_diff_load(&input.root, loaded)
        .with_working_tree(from_working_tree)
}

/// A few fixed words for why the current change could not be loaded, shown
/// in pilot's scope line. Fixed phrases keep pilot artifacts deterministic;
/// the loader's full message (paths, git stderr) belongs to `ripr check`.
fn current_change_unavailable_reason(error: &CoreError, fallback: &'static str) -> &'static str {
    let message = error.to_string();
    if crate::git::is_git_not_found_on_path(&message) {
        "git is not on PATH"
    } else if error.is_git_invocation_timeout() {
        "git timed out"
    } else if message.contains("is not inside a Git work tree") {
        "not a Git work tree"
    } else {
        fallback
    }
}

fn collect_pilot_python_first_use(
    input: &CheckInput,
    config: &RiprConfig,
) -> Option<output::pilot::PilotPythonFirstUse> {
    if !config
        .languages()
        .enabled()
        .contains(&crate::domain::LanguageId::Python)
    {
        return None;
    }

    let mut check_input = input.clone();
    check_input.format = OutputFormat::Json;
    Some(
        match app::check_workspace_with_config(check_input, config) {
            Ok(output) => output::pilot::PilotPythonFirstUse::from_check_output(&output),
            Err(error) => output::pilot::PilotPythonFirstUse::analysis_unavailable(error),
        },
    )
}

fn parse_pilot_options(args: &[String]) -> Result<PilotOptions, String> {
    let mut options = PilotOptions {
        root: PathBuf::from("."),
        out_dir: PathBuf::from(DEFAULT_PILOT_OUT_DIR),
        mode: Mode::Draft,
        explicit: CheckInputExplicit::default(),
        max_seams: 5,
        timeout_ms: DEFAULT_PILOT_TIMEOUT_MS,
        timeout_explicit: false,
        quiet: false,
        format: PilotStdoutFormat::Human,
    };
    let mut out_explicit = false;
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--root" => {
                i += 1;
                options.root = PathBuf::from(expect_value(args, i, "--root")?);
            }
            "--out" => {
                i += 1;
                options.out_dir = PathBuf::from(expect_value(args, i, "--out")?);
                out_explicit = true;
            }
            "--mode" => {
                i += 1;
                options.mode = parse_mode(expect_value(args, i, "--mode")?)?;
                options.explicit.mode = true;
            }
            "--max-seams" => {
                i += 1;
                options.max_seams =
                    parse_positive_usize(expect_value(args, i, "--max-seams")?, "--max-seams")?;
            }
            "--timeout-ms" => {
                i += 1;
                options.timeout_ms =
                    parse_positive_u64(expect_value(args, i, "--timeout-ms")?, "--timeout-ms")?;
                options.timeout_explicit = true;
            }
            "--quiet" => {
                options.quiet = true;
            }
            "--format" => {
                i += 1;
                options.format = parse_pilot_format(expect_value(args, i, "--format")?)?;
            }
            other => return Err(unknown_argument("pilot", other)),
        }
        i += 1;
    }
    if !out_explicit {
        options.out_dir = default_pilot_out_dir(&options.root);
    }
    Ok(options)
}

fn parse_pilot_format(value: &str) -> Result<PilotStdoutFormat, String> {
    const ACCEPTED: &[&str] = &["human", "json"];
    match value {
        "human" => Ok(PilotStdoutFormat::Human),
        "json" => Ok(PilotStdoutFormat::Json),
        other => Err(unknown_value("format", other, ACCEPTED)),
    }
}

/// Choose the stdout body for one finished pilot run (#7428).
///
/// `--format json` emits the same `pilot-summary.json` document already
/// written under `--out` and suppresses the human terminal. Packet files
/// and stderr progress are unchanged.
fn pilot_stdout<'a>(format: PilotStdoutFormat, json: &'a str, terminal: &'a str) -> &'a str {
    match format {
        PilotStdoutFormat::Human => terminal,
        PilotStdoutFormat::Json => json,
    }
}

/// Where pilot writes its packet when `--out` is not given, relative to the
/// working directory.
const DEFAULT_PILOT_OUT_DIR: &str = "target/ripr/pilot";

/// The default packet directory sits under `--root`, where the cache already
/// goes and where `ripr agent status --root` reads `pilot-summary.json`
/// (#5324). Anchoring it at the working directory wrote the packet beside
/// whatever repository the shell was in, overwriting that repository's own
/// packet. A `.` root keeps the bare relative path so printed commands are
/// unchanged; an explicit `--out` still resolves against the working
/// directory.
fn default_pilot_out_dir(root: &Path) -> PathBuf {
    if root == Path::new(".") {
        PathBuf::from(DEFAULT_PILOT_OUT_DIR)
    } else {
        root.join(DEFAULT_PILOT_OUT_DIR)
    }
}

/// The deadline extension applies only to the default budget. A typed
/// `--timeout-ms`, even one equal to the default, is a hard limit.
fn pilot_deadline_extension_ms(options: &PilotOptions) -> Option<u64> {
    (!options.timeout_explicit).then_some(PILOT_RETRY_TIMEOUT_MS)
}

enum PilotAnalysisResult<T> {
    Complete(T),
    TimedOut,
}

/// Run the pilot analysis on a worker thread under a deadline.
///
/// When `extension_ms` is set and the first deadline passes, `on_extend` runs
/// once and the same analysis keeps going for `extension_ms` more; work done
/// before the first deadline is never thrown away.
fn run_pilot_analysis_with_timeout<T, F>(
    timeout_ms: u64,
    extension_ms: Option<u64>,
    on_extend: impl FnOnce(),
    runner: F,
) -> Result<PilotAnalysisResult<T>, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    let cancellation_token = crate::analysis::cancellation::AnalysisCancellationToken::new();
    let worker_token = cancellation_token.clone();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = crate::analysis::cancellation::with_token(&worker_token, runner);
        let _ignored = tx.send(result);
    });

    let mut outcome = rx.recv_timeout(Duration::from_millis(timeout_ms));
    if let (Err(mpsc::RecvTimeoutError::Timeout), Some(extension_ms)) = (&outcome, extension_ms) {
        on_extend();
        outcome = rx.recv_timeout(Duration::from_millis(extension_ms));
    }
    match outcome {
        Ok(result) => result.map(PilotAnalysisResult::Complete),
        Err(mpsc::RecvTimeoutError::Timeout) => {
            cancellation_token
                .cancel(crate::analysis::cancellation::AnalysisAbortKind::DeadlineExceeded);
            // #5019: the detached worker owns its progress run's terminal
            // (`cancelled`), and the caller exits soon after this return.
            // Give the worker a bounded window to reach a cancellation
            // checkpoint and finish, so the terminal is not lost to process
            // exit. A worker stuck past the window cannot be forced (it is
            // a detached thread): a known, bounded limitation.
            let _ignored = rx.recv_timeout(Duration::from_secs(5));
            Ok(PilotAnalysisResult::TimedOut)
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            Err("pilot analysis stopped before producing a result".to_string())
        }
    }
}

fn pilot_artifacts(out_dir: &Path) -> output::pilot::PilotArtifacts {
    output::pilot::PilotArtifacts {
        repo_exposure_json: out_dir.join("repo-exposure.json"),
        repo_exposure_md: out_dir.join("repo-exposure.md"),
        agent_seam_packets_json: out_dir.join("agent-seam-packets.json"),
        pilot_summary_json: out_dir.join("pilot-summary.json"),
        pilot_summary_md: out_dir.join("pilot-summary.md"),
    }
}

/// Pilot artifacts default under the analyzed repository, which may commit a
/// symlink at an artifact path; never write through it.
fn write_pilot_file(path: &Path, contents: impl AsRef<[u8]>) -> Result<(), String> {
    output::file_write::write(path, contents.as_ref())
        .map_err(|err| format!("write output {} failed: {err}", path.display()))
}

/// Drop leftover complete-run siblings so a timeout packet cannot be read as
/// the previous run (#7425). Missing paths are a no-op; a directory at a
/// sibling path fails closed rather than leaving mixed evidence.
fn remove_unwritten_timeout_packet_siblings(
    artifacts: &output::pilot::PilotArtifacts,
) -> Result<(), String> {
    for path in artifacts.unwritten_timeout_siblings() {
        remove_timeout_packet_sibling(path)?;
    }
    Ok(())
}

fn remove_timeout_packet_sibling(path: &Path) -> Result<(), String> {
    match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "inspect leftover timeout packet file {} failed: {error}",
            path.display()
        )),
        Ok(metadata) if metadata.file_type().is_dir() => Err(format!(
            "leftover timeout packet path {} is a directory; refusing to leave it as if it were this run's output",
            path.display()
        )),
        Ok(_) => std::fs::remove_file(path).map_err(|error| {
            format!(
                "remove leftover timeout packet file {} failed: {error}",
                path.display()
            )
        }),
    }
}

/// Publish the timeout packet: remove the prior-run siblings this run will
/// not rewrite (#7425), then overwrite the two summaries with the partial
/// timeout result. Extracted verbatim from `pilot`'s timeout branch so the
/// packet contract is testable deterministically, without racing a real
/// process deadline.
fn publish_timeout_packet(
    artifacts: &output::pilot::PilotArtifacts,
    format: PilotStdoutFormat,
    context: output::pilot::PilotSummaryContext<'_>,
) -> Result<(), String> {
    remove_unwritten_timeout_packet_siblings(artifacts)?;
    let summary_json = output::pilot::render_pilot_timeout_summary_json(context);
    write_pilot_file(&artifacts.pilot_summary_json, &summary_json)?;
    write_pilot_file(
        &artifacts.pilot_summary_md,
        output::pilot::render_pilot_timeout_summary_md(context),
    )?;
    print!(
        "{}",
        pilot_stdout(
            format,
            &summary_json,
            &output::pilot::render_pilot_timeout_terminal(context),
        )
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn pilot_requires_values_for_value_flags() {
        assert_eq!(
            pilot(&args(&["--root"])),
            Err("missing value for --root".to_string())
        );
        assert_eq!(
            pilot(&args(&["--out"])),
            Err("missing value for --out".to_string())
        );
        assert_eq!(
            pilot(&args(&["--mode"])),
            Err("missing value for --mode".to_string())
        );
        assert_eq!(
            pilot(&args(&["--max-seams"])),
            Err("missing value for --max-seams".to_string())
        );
        assert_eq!(
            pilot(&args(&["--timeout-ms"])),
            Err("missing value for --timeout-ms".to_string())
        );
        assert_eq!(
            pilot(&args(&["--format"])),
            Err("missing value for --format".to_string())
        );
    }

    #[test]
    fn pilot_rejects_unknown_arguments() {
        assert_eq!(
            pilot(&args(&["--wat"])),
            Err("unknown pilot argument \"--wat\". Run `ripr pilot --help`.".to_string())
        );
    }

    #[test]
    fn pilot_rejects_non_positive_max_seams() {
        assert_eq!(
            parse_pilot_options(&args(&["--max-seams", "0"])),
            Err("--max-seams requires a positive integer; got \"0\"".to_string())
        );
    }

    #[test]
    fn pilot_rejects_non_positive_timeout() {
        assert_eq!(
            parse_pilot_options(&args(&["--timeout-ms", "0"])),
            Err("--timeout-ms requires a positive integer; got \"0\"".to_string())
        );
    }

    #[test]
    fn pilot_parses_root_out_mode_max_seams_quiet_and_timeout() {
        let options = parse_pilot_options(&args(&[
            "--root",
            "repo",
            "--out",
            "target/pilot",
            "--mode",
            "ready",
            "--max-seams",
            "3",
            "--timeout-ms",
            "120000",
            "--quiet",
        ]));

        assert_eq!(
            options,
            Ok(PilotOptions {
                root: PathBuf::from("repo"),
                out_dir: PathBuf::from("target/pilot"),
                mode: Mode::Ready,
                explicit: CheckInputExplicit {
                    mode: true,
                    include_unchanged_tests: false,
                },
                max_seams: 3,
                timeout_ms: 120_000,
                timeout_explicit: true,
                quiet: true,
                format: PilotStdoutFormat::Human,
            })
        );
    }

    #[test]
    fn parse_pilot_options_accepts_format_json_and_human() -> Result<(), String> {
        // #7428: `ripr check --format json` is the machine path; pilot must
        // accept the same flag rather than failing closed as unknown.
        let json = parse_pilot_options(&args(&["--format", "json"]))?;
        assert_eq!(json.format, PilotStdoutFormat::Json);
        let human = parse_pilot_options(&args(&["--format", "human"]))?;
        assert_eq!(human.format, PilotStdoutFormat::Human);
        let omitted = parse_pilot_options(&args(&[]))?;
        assert_eq!(omitted.format, PilotStdoutFormat::Human);
        Ok(())
    }

    #[test]
    fn parse_pilot_options_rejects_unknown_format_and_json_alias() {
        assert_eq!(
            parse_pilot_options(&args(&["--format", "yaml"])),
            Err("unknown format \"yaml\". Accepted: human, json.".to_string())
        );
        assert_eq!(
            parse_pilot_options(&args(&["--format", "jsonn"])),
            Err(
                "unknown format \"jsonn\". Did you mean `json`? Accepted: human, json.".to_string()
            )
        );
        // `--json` never existed on this command; do not invent an alias.
        assert_eq!(
            parse_pilot_options(&args(&["--json"])),
            Err("unknown pilot argument \"--json\". Run `ripr pilot --help`.".to_string())
        );
    }

    #[test]
    fn json_stdout_is_the_summary_document_not_the_human_block() {
        let json = "{\n  \"status\": \"complete\"\n}\n";
        let terminal = "Next: add a focused test\n";
        assert_eq!(pilot_stdout(PilotStdoutFormat::Json, json, terminal), json);
        assert_eq!(
            pilot_stdout(PilotStdoutFormat::Human, json, terminal),
            terminal
        );
    }

    #[test]
    fn default_out_dir_sits_under_the_root() -> Result<(), String> {
        // #5324: without --out the packet goes under --root, where the cache
        // and `ripr agent status --root` already look, not under the shell's
        // directory. A `.` root keeps the bare relative path.
        let other = parse_pilot_options(&args(&["--root", "../other"]))?;
        assert_eq!(other.out_dir, Path::new("../other/target/ripr/pilot"));
        let here = parse_pilot_options(&args(&["--root", "."]))?;
        assert_eq!(here.out_dir, Path::new("target/ripr/pilot"));
        let omitted = parse_pilot_options(&args(&[]))?;
        assert_eq!(omitted.out_dir, Path::new("target/ripr/pilot"));
        // An explicit --out stays as typed, before or after --root.
        let out_first = parse_pilot_options(&args(&["--out", "packet", "--root", "../other"]))?;
        assert_eq!(out_first.out_dir, Path::new("packet"));
        Ok(())
    }

    #[test]
    fn explicit_default_timeout_is_a_hard_limit() -> Result<(), String> {
        // Only an omitted --timeout-ms earns the cold-run extension; typing
        // the default value must not turn a 30s limit into 270s.
        let omitted = parse_pilot_options(&args(&["--quiet"]))?;
        assert_eq!(
            pilot_deadline_extension_ms(&omitted),
            Some(PILOT_RETRY_TIMEOUT_MS)
        );
        let explicit = parse_pilot_options(&args(&["--timeout-ms", "30000"]))?;
        assert_eq!(explicit.timeout_ms, DEFAULT_PILOT_TIMEOUT_MS);
        assert_eq!(pilot_deadline_extension_ms(&explicit), None);
        Ok(())
    }

    #[test]
    fn pilot_analysis_timeout_cancels_worker() {
        let (cancelled_tx, cancelled_rx) = mpsc::channel();
        let result = run_pilot_analysis_with_timeout::<(), _>(
            1,
            None,
            || {},
            move || {
                loop {
                    if crate::analysis::cancellation::checkpoint().is_err() {
                        let _ignored = cancelled_tx.send(());
                        return Err("analysis cancelled".to_string());
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
            },
        );

        assert!(matches!(result, Ok(PilotAnalysisResult::TimedOut)));
        assert_eq!(cancelled_rx.recv_timeout(Duration::from_secs(1)), Ok(()));
    }

    #[test]
    fn pilot_deadline_extension_keeps_the_running_analysis() {
        // #2424: when the default deadline passes, the same analysis keeps
        // running under the extension. A restart would invoke the runner a
        // second time and redo the work done before the first deadline.
        let runs = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let worker_runs = std::sync::Arc::clone(&runs);
        let extended = std::cell::Cell::new(0);
        let result = run_pilot_analysis_with_timeout(
            20,
            Some(5_000),
            || extended.set(extended.get() + 1),
            move || {
                worker_runs.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(200));
                crate::analysis::cancellation::checkpoint()?;
                Ok(analysis::ClassifiedSeamsReport {
                    classified: Vec::new(),
                    corpus_key: None,
                    limit_info: None,
                    skipped_generated: Vec::new(),
                    naming_only_skips: Vec::new(),
                })
            },
        );

        assert!(
            matches!(result, Ok(PilotAnalysisResult::Complete(_))),
            "the extended run must complete instead of timing out"
        );
        assert_eq!(extended.get(), 1, "the extension notice prints once");
        assert_eq!(
            runs.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "the analysis must run once, not restart"
        );
    }

    #[test]
    fn pilot_deadline_without_extension_still_cancels() {
        // Negative control for the extension: an explicit --timeout-ms
        // passes no extension, so the first deadline cancels the run and
        // the extension callback never fires.
        let extended = std::cell::Cell::new(false);
        let result = run_pilot_analysis_with_timeout::<(), _>(
            20,
            None,
            || extended.set(true),
            || {
                std::thread::sleep(Duration::from_millis(200));
                crate::analysis::cancellation::checkpoint()?;
                Err("analysis was not cancelled".to_string())
            },
        );

        assert!(matches!(result, Ok(PilotAnalysisResult::TimedOut)));
        assert!(!extended.get());
    }

    #[derive(Clone)]
    struct ProgressBuffer(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

    impl ProgressBuffer {
        fn new() -> Self {
            Self(std::sync::Arc::new(std::sync::Mutex::new(Vec::new())))
        }

        fn text(&self) -> String {
            match self.0.lock() {
                Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
                Err(poisoned) => String::from_utf8_lossy(&poisoned.into_inner()).into_owned(),
            }
        }
    }

    impl std::io::Write for ProgressBuffer {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            match self.0.lock() {
                Ok(mut bytes) => bytes.extend_from_slice(buf),
                Err(poisoned) => poisoned.into_inner().extend_from_slice(buf),
            }
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn pilot_progress_run_emits_stage_heartbeat_and_cancelled_on_timeout() {
        // #5019: a long-running pilot analysis (a deliberately blocked
        // inventory stands in for a cold-cache multi-minute repo walk) must
        // project the same repo-scope stage lines and throttled heartbeat
        // evidence `ripr check` emits, and the deadline must close the run
        // as `cancelled`, never `completed`.
        let buffer = ProgressBuffer::new();
        let sink = std::sync::Arc::new(CliProgressSink::with_writer(
            Box::new(buffer.clone()),
            false,
            ProgressPolicy::STANDARD,
        ));
        let (done_tx, done_rx) = mpsc::channel();
        let result = run_pilot_analysis_with_timeout(
            3_000,
            None,
            || {},
            move || {
                let result = app::repo_inventory_with_progress(
                    Some(&*sink),
                    || -> Result<analysis::ClassifiedSeamsReport, String> {
                        loop {
                            if crate::analysis::cancellation::checkpoint().is_err() {
                                return Err("analysis cancelled".to_string());
                            }
                            std::thread::sleep(Duration::from_millis(10));
                        }
                    },
                    Ok,
                );
                let _ignored = done_tx.send(());
                result
            },
        );

        assert!(matches!(result, Ok(PilotAnalysisResult::TimedOut)));
        // The bounded handoff joins the cancelled worker before returning,
        // so the run's terminal must already be projected; no waiting on
        // the worker is needed to observe it.
        let text = buffer.text();
        assert!(
            text.contains("ripr progress: cancelled [repo]"),
            "deadline must close the run as cancelled: {text}"
        );
        assert_eq!(done_rx.recv_timeout(Duration::from_secs(5)), Ok(()));
        let text = buffer.text();
        assert!(
            text.contains("ripr progress: loading_input [repo]"),
            "missing repo-scope loading_input: {text}"
        );
        assert!(
            text.contains("ripr progress: analyzing [repo]"),
            "missing repo-scope analyzing: {text}"
        );
        assert!(
            text.contains("still active after 2s"),
            "a stage held past first_heartbeat must heartbeat: {text}"
        );
        assert!(
            !text.contains("completed"),
            "a timed-out run must never project completed: {text}"
        );
        assert!(
            !text.contains("[diff]"),
            "pilot must project repo scope, not diff scope: {text}"
        );
    }

    #[test]
    fn pilot_progress_stream_absent_when_quiet_or_sink_removed() {
        // #5019 removal experiment per #2608's closure rule: disabling the
        // producer-side progress stream (--quiet, or no sink at all) removes
        // every stage and heartbeat line while the timeout machinery still
        // behaves identically.
        assert!(
            pilot_progress_sink(true).is_none(),
            "--quiet must suppress the pilot progress sink"
        );
        assert!(pilot_progress_sink(false).is_some());

        let result = run_pilot_analysis_with_timeout(
            50,
            None,
            || {},
            || {
                app::repo_inventory_with_progress(
                    None,
                    || -> Result<analysis::ClassifiedSeamsReport, String> {
                        loop {
                            if crate::analysis::cancellation::checkpoint().is_err() {
                                return Err("analysis cancelled".to_string());
                            }
                            std::thread::sleep(Duration::from_millis(5));
                        }
                    },
                    Ok,
                )
            },
        );
        assert!(
            matches!(result, Ok(PilotAnalysisResult::TimedOut)),
            "removing the sink must not change the timeout behavior"
        );
    }

    #[cfg(unix)]
    #[test]
    fn unwritable_out_dir_names_out_not_out_dir() -> Result<(), String> {
        use crate::testing::unwritable_output::OutputDirFixture;

        let env = OutputDirFixture::unwritable("pilot-ro", "pilot")?;
        let root = OutputDirFixture::path_arg(&env.root)?;
        let out = OutputDirFixture::path_arg(&env.target)?;
        let error = match pilot(&args(&["--root", root, "--out", out])) {
            Err(error) => error,
            Ok(()) => return Err("unwritable --out must fail before analysis".to_string()),
        };
        assert!(error.contains(&format!("create {out} failed:")), "{error}");
        assert!(
            error.contains("write elsewhere with --out PATH"),
            "pilot must name --out PATH, got {error}"
        );
        assert!(
            !error.contains("--out-dir"),
            "pilot must not name first-pr's flag, got {error}"
        );
        Ok(())
    }

    #[test]
    fn remove_unwritten_timeout_packet_siblings_is_a_no_op_when_absent() -> Result<(), String> {
        let dir = TempPacketDir::new("absent")?;
        let artifacts = packet_dir_artifacts(&dir);
        remove_unwritten_timeout_packet_siblings(&artifacts)?;
        for path in artifacts.unwritten_timeout_siblings() {
            assert!(
                !path.exists(),
                "absent sibling must stay absent: {}",
                path.display()
            );
        }
        Ok(())
    }

    #[test]
    fn remove_unwritten_timeout_packet_siblings_deletes_prior_run_files() -> Result<(), String> {
        let dir = TempPacketDir::new("stale")?;
        let artifacts = packet_dir_artifacts(&dir);
        for path in artifacts.unwritten_timeout_siblings() {
            std::fs::write(path, "PREVIOUS_COMPLETE_RUN")
                .map_err(|err| format!("write {}: {err}", path.display()))?;
        }
        std::fs::write(dir.join("unrelated.txt"), "keep-me")
            .map_err(|err| format!("write unrelated: {err}"))?;
        std::fs::write(&artifacts.pilot_summary_json, "PREVIOUS_SUMMARY")
            .map_err(|err| format!("write prior summary: {err}"))?;

        remove_unwritten_timeout_packet_siblings(&artifacts)?;

        for path in artifacts.unwritten_timeout_siblings() {
            assert!(
                !path.exists(),
                "timeout must drop leftover sibling {}",
                path.display()
            );
        }
        assert_eq!(
            std::fs::read_to_string(dir.join("unrelated.txt"))
                .map_err(|err| format!("read unrelated: {err}"))?,
            "keep-me",
            "unrelated files in the packet directory must stay"
        );
        assert_eq!(
            std::fs::read_to_string(&artifacts.pilot_summary_json)
                .map_err(|err| format!("read prior summary: {err}"))?,
            "PREVIOUS_SUMMARY",
            "summaries are rewritten by the timeout path, not this helper"
        );
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn remove_unwritten_timeout_packet_siblings_unlinks_a_symlink_and_keeps_its_target()
    -> Result<(), String> {
        let dir = TempPacketDir::new("symlink")?;
        let artifacts = packet_dir_artifacts(&dir);
        let target = dir.join("previous-run-repo-exposure.json");
        std::fs::write(&target, "PREVIOUS_COMPLETE_RUN")
            .map_err(|err| format!("write symlink target: {err}"))?;
        std::os::unix::fs::symlink(&target, &artifacts.repo_exposure_json)
            .map_err(|err| format!("symlink: {err}"))?;

        remove_unwritten_timeout_packet_siblings(&artifacts)?;

        assert!(
            artifacts.repo_exposure_json.symlink_metadata().is_err(),
            "the packet-dir name must not still present the prior run"
        );
        assert_eq!(
            std::fs::read_to_string(&target).map_err(|err| format!("read target: {err}"))?,
            "PREVIOUS_COMPLETE_RUN",
            "unlinking the packet-dir name must not delete the symlink target"
        );
        Ok(())
    }

    #[test]
    fn remove_unwritten_timeout_packet_siblings_refuses_a_directory() -> Result<(), String> {
        let dir = TempPacketDir::new("dir-sibling")?;
        let artifacts = packet_dir_artifacts(&dir);
        std::fs::create_dir(&artifacts.repo_exposure_json)
            .map_err(|err| format!("create directory sibling: {err}"))?;
        std::fs::write(artifacts.repo_exposure_json.join("nested.json"), "stale")
            .map_err(|err| format!("write nested: {err}"))?;

        let error = match remove_unwritten_timeout_packet_siblings(&artifacts) {
            Err(error) => error,
            Ok(()) => {
                return Err(
                    "a directory sibling must fail closed, not be left or walked".to_string(),
                );
            }
        };
        assert!(
            error.contains("is a directory"),
            "directory sibling must name the refusal, got: {error}"
        );
        assert!(
            artifacts.repo_exposure_json.is_dir(),
            "fail-closed must leave the unexpected directory in place"
        );
        Ok(())
    }

    #[test]
    fn timed_out_pilot_removes_prior_run_packet_siblings() -> Result<(), String> {
        // Drive the timeout packet publisher directly. Racing a real 1 ms
        // `--timeout-ms` deadline against cold process startup let a tiny
        // fixture's analysis finish inside the window on a fast host, so
        // `pilot` took the complete branch and rewrote the packet before the
        // timeout assertions could bind (#7937 residual). Which branch a
        // real deadline race takes is host timing, not code behavior; the
        // deadline machinery that selects the timeout branch is covered
        // deterministically by the `run_pilot_analysis_with_timeout` tests
        // above.
        let dir = TempPacketDir::new("timeout-stale")?;
        let artifacts = packet_dir_artifacts(&dir);
        let stale = "PREVIOUS_COMPLETE_RUN";
        for path in [
            &artifacts.repo_exposure_json,
            &artifacts.repo_exposure_md,
            &artifacts.agent_seam_packets_json,
            &artifacts.pilot_summary_json,
            &artifacts.pilot_summary_md,
        ] {
            std::fs::write(path, stale)
                .map_err(|err| format!("write stale {}: {err}", path.display()))?;
        }
        std::fs::write(dir.join("unrelated.txt"), "keep-me")
            .map_err(|err| format!("write unrelated: {err}"))?;
        let root = dir.join("repo");
        std::fs::create_dir(&root).map_err(|err| format!("create fixture root: {err}"))?;

        publish_timeout_packet(
            &artifacts,
            PilotStdoutFormat::Human,
            output::pilot::PilotSummaryContext {
                root: &root,
                mode: &Mode::Draft,
                config_path: None,
                max_seams: 5,
                timeout_ms: 1,
                timeout_extension_ms: 0,
                artifacts: &artifacts,
                python_first_use: None,
                language_routes: None,
                current_change: None,
                seam_limit: None,
            },
        )?;

        let summary = std::fs::read_to_string(&artifacts.pilot_summary_json)
            .map_err(|err| format!("read timeout summary: {err}"))?;
        assert!(
            summary.contains(r#""status": "partial""#)
                && summary.contains(r#""reason": "timeout""#),
            "production timeout path must run; got:\n{summary}"
        );
        assert!(
            !summary.contains(stale),
            "the partial summary must not carry the previous run's bytes:\n{summary}"
        );
        let summary_md = std::fs::read_to_string(&artifacts.pilot_summary_md)
            .map_err(|err| format!("read timeout summary md: {err}"))?;
        assert!(
            summary_md.contains("- Status: `partial`"),
            "the timeout path must rewrite the markdown summary; got:\n{summary_md}"
        );
        assert!(
            !summary_md.contains(stale),
            "the markdown summary must not carry the previous run's bytes:\n{summary_md}"
        );
        for name in [
            "repo-exposure.json",
            "repo-exposure.md",
            "agent-seam-packets.json",
        ] {
            let path = dir.join(name);
            assert!(
                !path.exists(),
                "timed-out pilot must not leave leftover {name} from the previous run"
            );
        }
        assert_eq!(
            std::fs::read_to_string(dir.join("unrelated.txt"))
                .map_err(|err| format!("read unrelated: {err}"))?,
            "keep-me"
        );
        Ok(())
    }

    fn packet_dir_artifacts(dir: &Path) -> output::pilot::PilotArtifacts {
        output::pilot::PilotArtifacts {
            repo_exposure_json: dir.join("repo-exposure.json"),
            repo_exposure_md: dir.join("repo-exposure.md"),
            agent_seam_packets_json: dir.join("agent-seam-packets.json"),
            pilot_summary_json: dir.join("pilot-summary.json"),
            pilot_summary_md: dir.join("pilot-summary.md"),
        }
    }

    struct TempPacketDir(PathBuf);

    impl TempPacketDir {
        fn new(label: &str) -> Result<Self, String> {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|err| format!("clock: {err}"))?
                .as_nanos();
            let dir = std::env::temp_dir().join(format!(
                "ripr-pilot-timeout-{label}-{}-{stamp}",
                std::process::id()
            ));
            std::fs::create_dir_all(&dir)
                .map_err(|err| format!("create {}: {err}", dir.display()))?;
            Ok(Self(dir))
        }
    }

    impl std::ops::Deref for TempPacketDir {
        type Target = Path;

        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    impl Drop for TempPacketDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn occupying_file_out_dir_does_not_name_the_relocate_flag() -> Result<(), String> {
        use crate::testing::unwritable_output::OutputDirFixture;

        let env = OutputDirFixture::occupying_file("pilot-file", "pilot")?;
        let root = OutputDirFixture::path_arg(&env.root)?;
        let out = OutputDirFixture::path_arg(&env.target)?;
        let error = match pilot(&args(&["--root", root, "--out", out])) {
            Err(error) => error,
            Ok(()) => return Err("file occupying --out must fail".to_string()),
        };
        assert!(error.contains(&format!("create {out} failed:")), "{error}");
        assert!(
            !error.contains("write elsewhere"),
            "a file occupying --out is not a not-writable tree: {error}"
        );
        Ok(())
    }

    #[test]
    fn change_classification_past_the_limit_runs_only_when_it_can_matter() {
        let change = output::pilot::PilotCurrentChange::from_diff_text(
            Path::new("."),
            None,
            "diff --git a/src/a.rs b/src/a.rs\n--- a/src/a.rs\n+++ b/src/a.rs\n@@ -1 +1 @@\n-a\n+b\n",
        );
        let config = RiprConfig::default();
        let root = Path::new("/nonexistent/ripr-pilot-root");
        // No limit fired: the inventory already holds every seam.
        assert!(
            classify_change_past_seam_limit(root, &config, &change, false, 1_000, false).is_none()
        );
        // No Rust file in the change: nothing to classify.
        let docs = output::pilot::PilotCurrentChange::from_diff_text(
            Path::new("."),
            None,
            "diff --git a/README.md b/README.md\n--- a/README.md\n+++ b/README.md\n@@ -1 +1 @@\n-a\n+b\n",
        );
        assert!(
            classify_change_past_seam_limit(root, &config, &docs, true, 1_000, false).is_none()
        );
        // The inventory spent the deadline: an error, so the caveat stays.
        assert_eq!(
            classify_change_past_seam_limit(root, &config, &change, true, 0, false)
                .map(|result| result.map(|report| report.classified.len())),
            Some(Err("the inventory used pilot's whole deadline".to_string()))
        );
    }
}
