//! Dispatch and environment reporting for `ripr doctor`.
//!
//! This is the CLI adapter layer only. Analysis, evaluation, and rendering
//! semantics live in `crate::app`, `crate::analysis`, and `crate::output`.
//! This module owns argv parsing, probe/report printing, and exit mapping
//! for the doctor command family.

use crate::analysis;
use crate::app::Mode;
use crate::cli::help;
use crate::cli::suggest::unknown_argument;
use crate::config::{CONFIG_FILE_NAME, DEFAULT_LSP_SEAM_DIAGNOSTICS, RiprConfig};
use crate::domain::{LanguageId, LanguageStatus};
use crate::output;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

pub(in crate::cli) fn doctor(args: &[String]) -> Result<(), String> {
    let mut json_output = false;
    let mut profile = output::doctor::DoctorProfile::Analysis;
    let mut root: Option<String> = None;
    let mut arguments = args.iter();
    while let Some(arg) = arguments.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                help::print_doctor_help();
                return Ok(());
            }
            "--json" => json_output = true,
            "--profile" => {
                profile = match arguments.next().map(String::as_str) {
                    Some("analysis") => output::doctor::DoctorProfile::Analysis,
                    Some("source-build") => output::doctor::DoctorProfile::SourceBuild,
                    Some(other) => {
                        return Err(format!(
                            "unknown doctor profile `{other}`; expected `analysis` or `source-build`"
                        ));
                    }
                    None => return Err("missing value for --profile".to_string()),
                };
            }
            "--root" => {
                let value = match arguments.next().map(String::as_str) {
                    // A known doctor flag in the value position means the
                    // root was omitted; consuming it ran the report against a
                    // directory named after the flag (#4318 review). The
                    // pre-#4318 parser answered `missing value for --root`
                    // here. Other dash-prefixed paths stay legitimate values.
                    Some("--help" | "-h" | "--json" | "--profile" | "--root") => {
                        return Err("missing value for --root".to_string());
                    }
                    Some(value) => value,
                    None => return Err("missing value for --root".to_string()),
                };
                // #4318: a repeated documented flag is a usage mistake with a
                // name of its own; accusing `--root` of being unknown sends
                // the user to help for a flag they already used.
                if let Some(existing) = &root {
                    return Err(format!(
                        "doctor accepts at most one --root; found both {existing:?} and {value:?}. Run `ripr doctor --help`."
                    ));
                }
                root = Some(value.to_string());
            }
            other if other.starts_with('-') => {
                return Err(unknown_argument("doctor", other));
            }
            other => {
                return Err(format!(
                    "doctor does not accept positional arguments; got {other:?}; pass the workspace root with `--root <path>`. Run `ripr doctor --help`."
                ));
            }
        }
    }
    let root = root.map_or_else(|| PathBuf::from("."), PathBuf::from);

    // One marker scan per run, shared by both surfaces. `--json` used to branch
    // away before any of the environment facts were gathered, so the machine
    // surface reported none of them (#5214).
    let detected = detect_languages(&root);

    if json_output {
        return doctor_json(&root, profile, &detected);
    }

    // Human-readable path. Gather every typed fact first, then project a
    // compact screen (or the verbose full screen). Printing while probing
    // used to bury failures under later passing checks (#8168).
    let core_evaluation =
        output::doctor::evaluate_doctor_core_with_config_for_profile(&root, &detected, profile);
    let mut report = core_evaluation.report;
    let mut ok = matches!(report.status, output::doctor::DoctorStatus::Pass);
    let enabled_languages = enabled_languages(&core_evaluation.config);
    let environment = DoctorEnvironment::probe(&root, &detected, &core_evaluation.config);
    let binary = output::doctor_binary::probe_binary_identity();
    ok &= add_language_runtime_probes(
        &root,
        &detected,
        &enabled_languages,
        &mut report,
        false,
        probe_runtime,
    );
    for name in ["root_directory", "cargo_toml", "git_repository"] {
        if check_named(&report, name).is_none() {
            ok = false;
        }
    }
    for tool in output::doctor::DOCTOR_TOOLS {
        if check_named(&report, &format!("tool_{tool}")).is_none() {
            ok = false;
        }
    }
    let passed = ok && report.status == output::doctor::DoctorStatus::Pass;
    for line in render_doctor_human_screen(DoctorHumanScreen {
        root: &root,
        profile,
        report: &report,
        config: &core_evaluation.config,
        environment: &environment,
        binary: &binary,
        verbose: crate::is_verbose(),
        passed,
    }) {
        println!("{line}");
    }

    if passed {
        Ok(())
    } else {
        Err("doctor found issues".to_string())
    }
}

/// Typed JSON doctor output. Carries the same environment facts as the human
/// screen as typed fields: every value below comes from one
/// [`DoctorEnvironment`] probe that the human path also reads, so `--json` is
/// not a second, thinner report (#5214).
fn doctor_json(
    root: &Path,
    profile: output::doctor::DoctorProfile,
    detected: &[LanguageId],
) -> Result<(), String> {
    let evaluation =
        output::doctor::evaluate_doctor_core_with_config_for_profile(root, detected, profile);
    let mut report = evaluation.report;
    report.binary = Some(output::doctor_binary::probe_binary_identity());
    let enabled_languages = enabled_languages(&evaluation.config);
    let _ = add_language_runtime_probes(
        root,
        detected,
        &enabled_languages,
        &mut report,
        false,
        probe_runtime,
    );
    if let Some(advisory) = generated_workflow_advisory(root) {
        report.add_advisory_check("generated_workflow", advisory);
    }
    let environment = DoctorEnvironment::probe(root, detected, &evaluation.config);
    report.apply_environment(environment.facts);
    if profile == output::doctor::DoctorProfile::SourceBuild
        && let Some(advisory) = linker_temp_redirect_advisory(root)
    {
        report.add_advisory_check("linker_temp_redirect", advisory);
    }
    println!("{}", report.render_json()?);
    output::doctor::doctor_report_result(&report)
}

/// The run's environment facts, plus the one input the human enablement tip
/// needs that is not part of the JSON document.
///
/// The typed facts live in one owner
/// ([`output::doctor::DoctorEnvironmentFacts`]) that both surfaces read, so a
/// fact cannot be printed for a user and missing from the machine document.
/// Each one reuses a probe the human screen already performs; nothing here adds
/// a new observation, and none of it can change a check's status or the exit
/// code. The tip's own inputs stay here because the tip is prose, not an
/// observation.
struct DoctorEnvironment {
    facts: output::doctor::DoctorEnvironmentFacts,
    preview_enablement: Option<PreviewEnablement>,
}

impl DoctorEnvironment {
    fn probe(root: &Path, detected: &[LanguageId], config: &Result<RiprConfig, String>) -> Self {
        let loaded = config.as_ref().ok();
        let preview_enablement = preview_languages_to_enable(detected, config);
        Self {
            facts: output::doctor::DoctorEnvironmentFacts {
                detected_languages: detected_language_entries(
                    detected,
                    loaded.map(|config| config.languages().enabled()),
                ),
                unanalyzed_source_languages: unanalyzed_language_entries(root),
                preview_language_gaps: preview_language_gaps(preview_enablement.as_ref()),
                config_defaults: config_defaults(config),
                cache: cache_status(root),
                test_surfaces: detect_test_surfaces(root, detected),
                perl_preview: probe_perl_preview(root, loaded),
            },
            preview_enablement,
        }
    }
}

fn detected_language_entries(
    detected: &[LanguageId],
    enabled: Option<&[LanguageId]>,
) -> Vec<output::doctor::DoctorDetectedLanguage> {
    detected
        .iter()
        .map(|id| output::doctor::DoctorDetectedLanguage {
            language: id.as_str().to_string(),
            status: language_status(*id),
            adapter_available: id.is_available(),
            // JavaScript has no config entry of its own: the `typescript`
            // entry analyzes it, so enablement is read through the same
            // mapping the gaps path uses. `None` (no loaded configuration)
            // stays unknown rather than reading as disabled.
            enabled: enabled.map(|list| list.contains(&config_entry(*id))),
        })
        .collect()
}

fn unanalyzed_language_entries(root: &Path) -> Vec<output::doctor::DoctorUnanalyzedLanguage> {
    crate::analysis::workspace_unanalyzed_source_languages(root)
        .into_iter()
        .map(
            |(language, file_count)| output::doctor::DoctorUnanalyzedLanguage {
                language: language.to_string(),
                file_count,
            },
        )
        .collect()
}

/// The enablement tip's payload as typed data: which `[languages] enabled`
/// entry to add, and which detected source it analyzes. They differ for a
/// JavaScript-only workspace, which the `typescript` entry analyzes.
fn preview_language_gaps(
    enablement: Option<&PreviewEnablement>,
) -> Vec<output::doctor::DoctorPreviewLanguageGap> {
    let Some(enablement) = enablement else {
        return Vec::new();
    };
    enablement
        .gaps
        .iter()
        .map(|gap| output::doctor::DoctorPreviewLanguageGap {
            config_entry: gap.entry.as_str().to_string(),
            detected_language: gap.label.clone(),
        })
        .collect()
}

/// The effective configuration defaults, or `None` when the configuration could
/// not be loaded. The load error itself stays on the `config` check, redacted
/// for the JSON surface (RIPR-SPEC-0007).
fn config_defaults(
    config: &Result<RiprConfig, String>,
) -> Option<output::doctor::DoctorConfigDefaults> {
    let config = config.as_ref().ok()?;
    Some(output::doctor::DoctorConfigDefaults {
        source_path: config.source_path().map(output::path::human_path),
        analysis_mode: config
            .analysis()
            .mode()
            .map(Mode::as_str)
            .unwrap_or_else(|| Mode::Draft.as_str())
            .to_string(),
        lsp_seam_diagnostics: config
            .lsp()
            .seam_diagnostics()
            .unwrap_or(DEFAULT_LSP_SEAM_DIAGNOSTICS),
        suppressions_path: config.suppressions().display_path(),
        bun_ub_profile_configured: config.profiles().bun_ub().is_some(),
        bun_ub_test_roots: config
            .profiles()
            .bun_ub()
            .map(|profile| profile.test_roots().to_vec())
            .unwrap_or_default(),
        bun_ub_bridge_hints: config
            .profiles()
            .bun_ub()
            .map(|profile| profile.display_bridge_hints()),
    })
}

/// The seam cache doctor will use for `root`, with its measured size.
fn cache_status(root: &Path) -> output::doctor::DoctorCacheStatus {
    let cache_dir = analysis::seam_cache::cache_base_dir(root);
    let relocated =
        std::env::var(analysis::seam_cache::CACHE_DIR_ENV).is_ok_and(|v| !v.trim().is_empty());
    let size_bytes = dir_size_bytes(&cache_dir);
    output::doctor::DoctorCacheStatus {
        cache_dir: output::path::human_path(&cache_dir),
        relocated_by_env: relocated,
        size_bytes,
        size_display: format_bytes(size_bytes),
        artifact_ignore: ripr_artifact_ignore(root),
        artifact_tracking: ripr_artifact_tracking(root),
    }
}

fn enabled_languages(config: &Result<RiprConfig, String>) -> Vec<LanguageId> {
    config
        .as_ref()
        .map(|config| config.languages().enabled().to_vec())
        .unwrap_or_default()
}

fn check_named<'a>(
    report: &'a output::doctor::DoctorReport,
    name: &str,
) -> Option<&'a output::doctor::DoctorCheck> {
    report.checks.iter().find(|check| check.name == name)
}

/// Default human doctor must fit one 80x24 screen (#8168).
#[cfg(test)]
const DOCTOR_HUMAN_SCREEN_BUDGET: usize = 24;

struct DoctorHumanScreen<'a> {
    root: &'a Path,
    profile: output::doctor::DoctorProfile,
    report: &'a output::doctor::DoctorReport,
    config: &'a Result<RiprConfig, String>,
    environment: &'a DoctorEnvironment,
    binary: &'a output::doctor_binary::DoctorBinaryIdentity,
    verbose: bool,
    passed: bool,
}

fn doctor_profile_name(profile: output::doctor::DoctorProfile) -> &'static str {
    match profile {
        output::doctor::DoctorProfile::Analysis => "analysis",
        output::doctor::DoctorProfile::SourceBuild => "source-build",
    }
}

fn split_state_and_fix(evidence: &str) -> (&str, Option<&str>) {
    match evidence.split_once("; ") {
        Some((state, fix)) if !fix.is_empty() => (state, Some(fix)),
        _ => (evidence, None),
    }
}

fn display_text(value: &str) -> String {
    crate::terminal_text::single_line(value)
}

fn display_path(path: &Path) -> String {
    display_text(&output::path::human_path(path))
}

fn failure_block(evidence: &str) -> Vec<String> {
    let (state, fix) = split_state_and_fix(evidence);
    let mut lines = vec![format!("! {}", display_text(state))];
    if let Some(fix) = fix.or_else(|| tool_probe_renderer_fix(state)) {
        lines.push(format!("  Fix: {}", display_text(fix)));
    }
    lines
}

/// Renderer-only repair for tool-probe Fail evidence that has no `; ` clause.
/// JSON evidence stays the producer string; these are not missing-tool
/// install cases.
fn tool_probe_renderer_fix(evidence: &str) -> Option<&'static str> {
    output::doctor::DOCTOR_TOOLS.iter().find_map(|tool| {
        let rest = evidence.strip_prefix(tool)?;
        if rest.starts_with(" --version failed") {
            Some(
                "the binary on PATH ran but `--version` failed; repair or replace it so the probe can succeed",
            )
        } else if rest.starts_with(" timed out after") {
            Some(
                "retry after reducing load, or repair the binary on PATH so `--version` answers in time",
            )
        } else if rest.starts_with(" could not be launched") {
            Some("repair permissions or replace the binary on PATH so doctor can spawn it")
        } else if rest.starts_with(" could not be waited on") {
            Some("retry the probe; doctor could not confirm the process ended")
        } else {
            None
        }
    })
}

fn compact_tilde_line(text: &str) -> String {
    format!("~ {}", display_text(text))
}

fn runtime_failure_block(probe: &output::doctor::DoctorRuntimeProbe) -> Vec<String> {
    vec![
        format!(
            "! {} verify-route runtime: {}",
            probe.language,
            display_text(&probe.evidence)
        ),
        format!("  Fix: {}", display_text(&probe.hint)),
    ]
}

fn primary_runtime_probe(probe: &output::doctor::DoctorRuntimeProbe) -> bool {
    primary_runtime(&probe.language).is_some_and(|(tool, _)| tool == probe.tool)
}

fn join_compact(label: &str, parts: &[String]) -> Option<String> {
    if parts.is_empty() {
        return None;
    }
    Some(format!("✓ {label}: {}", parts.join(" · ")))
}

fn render_doctor_human_screen(screen: DoctorHumanScreen<'_>) -> Vec<String> {
    let DoctorHumanScreen {
        root,
        profile,
        report,
        config,
        environment,
        binary,
        verbose,
        passed,
    } = screen;
    let mut lines = Vec::new();
    lines.push(format!(
        "ripr doctor ({}) · ripr {}",
        doctor_profile_name(profile),
        report.ripr_version
    ));
    lines.push(format!(
        "Root: {} · Rust {} MSRV",
        display_path(root),
        report.ripr_build_msrv
    ));
    lines.push(String::new());

    // Failures first, each with its own fix line (#8168).
    for name in ["root_directory", "cargo_toml", "git_repository"] {
        match check_named(report, name) {
            Some(check) if check.status == output::doctor::DoctorCheckStatus::Fail => {
                let evidence = check.evidence.as_deref().unwrap_or(check.name.as_str());
                lines.extend(failure_block(evidence));
            }
            None => lines.push(format!("! missing doctor core check: {name}")),
            Some(_) => {}
        }
    }
    if let Err(err) = config {
        let path = display_path(&root.join(CONFIG_FILE_NAME));
        lines.push(format!("! Config: invalid {CONFIG_FILE_NAME}"));
        lines.push(format!(
            "  Fix: correct {path} so it loads, then rerun `ripr doctor`"
        ));
        lines.push(format!("- Config path: {path}"));
        for error in crate::config::config_error_display_lines(err, verbose) {
            lines.push(format!("  error: {}", display_text(&error)));
        }
    }
    for tool in output::doctor::DOCTOR_TOOLS {
        let name = format!("tool_{tool}");
        match check_named(report, &name) {
            Some(check) if check.status == output::doctor::DoctorCheckStatus::Fail => {
                let evidence = check.evidence.as_deref().unwrap_or(check.name.as_str());
                lines.extend(failure_block(evidence));
            }
            None => lines.push(format!("! missing doctor core check: {name}")),
            Some(_) => {}
        }
    }
    for probe in &report.runtime_probes {
        if probe.required && probe.status == output::doctor::DoctorStatus::Fail {
            lines.extend(runtime_failure_block(probe));
        }
    }

    // Advisories that are themselves the repair signal stay on the compact
    // screen: unborn HEAD, generated workflow, cache, linker temp, PATH
    // substitution.
    if let Some(check) = check_named(report, "git_head") {
        let evidence = check.evidence.as_deref().unwrap_or(check.name.as_str());
        lines.push(compact_tilde_line(evidence));
    }
    if let Some(advisory) = generated_workflow_advisory(root) {
        lines.push(compact_tilde_line(&format!(
            "Generated workflow: {advisory}"
        )));
    }
    if profile == output::doctor::DoctorProfile::SourceBuild
        && let Some(advisory) = linker_temp_redirect_advisory(root)
    {
        lines.push(compact_tilde_line(&advisory));
    }
    for warning in &binary.warnings {
        lines.push(format!("- warning: {}", display_text(warning)));
    }

    // Compact passing groups, then leftover skip/advisory lines that tests
    // and SPEC-0007 still name by their evidence text.
    let mut workspace = Vec::new();
    let mut toolchains = Vec::new();
    for name in ["root_directory", "cargo_toml", "git_repository"] {
        if let Some(check) = check_named(report, name) {
            match check.status {
                output::doctor::DoctorCheckStatus::Pass => {
                    workspace.push(display_text(
                        check.evidence.as_deref().unwrap_or(check.name.as_str()),
                    ));
                }
                output::doctor::DoctorCheckStatus::Skipped => {
                    lines.push(format!(
                        "- {}",
                        display_text(check.evidence.as_deref().unwrap_or(check.name.as_str()))
                    ));
                }
                _ => {}
            }
        }
    }
    if config.is_ok() {
        if let Some(defaults) = environment.facts.config_defaults.as_ref() {
            match &defaults.source_path {
                Some(path) => {
                    workspace.push(format!("Config: loaded {CONFIG_FILE_NAME}"));
                    lines.push(format!("- Config path: {}", display_text(path)));
                    // Init follow-ups replay bare `ripr doctor` to confirm
                    // the written policy (#8168 keeps the rest verbose).
                    lines.push(format!(
                        "- Analysis mode default: {}",
                        defaults.analysis_mode
                    ));
                    lines.push(format!(
                        "- LSP seam diagnostics default: {}",
                        defaults.lsp_seam_diagnostics
                    ));
                }
                None => workspace.push("Config: not found; using built-in defaults".to_string()),
            }
        } else {
            lines.push(format!("! Config: invalid {CONFIG_FILE_NAME}"));
            lines.push(format!(
                "- Config path: {}",
                display_path(&root.join(CONFIG_FILE_NAME))
            ));
            lines.push("  error: configuration loaded but produced no defaults".to_string());
        }
    }
    if let Some(line) = join_compact("Workspace", &workspace) {
        lines.push(line);
    }
    for tool in output::doctor::DOCTOR_TOOLS {
        let name = format!("tool_{tool}");
        if let Some(check) = check_named(report, &name) {
            match check.status {
                output::doctor::DoctorCheckStatus::Pass => {
                    toolchains.push(display_text(check.evidence.as_deref().unwrap_or(tool)));
                }
                output::doctor::DoctorCheckStatus::Skipped
                | output::doctor::DoctorCheckStatus::Advisory => {
                    lines.push(format!(
                        "{} {}",
                        if check.status == output::doctor::DoctorCheckStatus::Advisory {
                            "~"
                        } else {
                            "-"
                        },
                        display_text(check.evidence.as_deref().unwrap_or(check.name.as_str()))
                    ));
                }
                output::doctor::DoctorCheckStatus::Fail => {}
            }
        }
    }
    if let Some(line) = join_compact("Toolchains", &toolchains) {
        lines.push(line);
    }
    lines.extend(detected_languages_lines(
        &environment.facts.detected_languages,
        &environment.facts.unanalyzed_source_languages,
    ));
    lines.extend(perl_not_enabled_line(
        &environment.facts.detected_languages,
        environment.facts.perl_preview.as_ref(),
    ));
    // Compact keeps the ignored-[perl].executable disclosure: the full
    // Perl preview dump is verbose-only, but this line is a safety
    // contract (`repo_config_perl_executable_runs_only_with_user_opt_in`).
    if !verbose
        && let Some(line) =
            ignored_perl_executable_compact_line(environment.facts.perl_preview.as_ref())
    {
        lines.push(line);
    }
    let cache = &environment.facts.cache;
    let cache_loc = if cache.relocated_by_env {
        format!("{} (RIPR_CACHE_DIR active)", cache.cache_dir)
    } else {
        cache.cache_dir.clone()
    };
    lines.push(format!(
        "✓ Cache: {} ({})",
        display_text(&cache_loc),
        cache.size_display
    ));
    // Keep the historical `- Cache location:` / `- Cache size:` spellings on
    // verbose so JSON/human parity still names the same words.
    if !cache.relocated_by_env && !root.is_dir() {
        // A missing root already failed `root_directory`; do not probe.
    } else if let Some(reason) =
        cache_unwritable_reason(&analysis::seam_cache::cache_base_dir(root))
    {
        lines.push(format!(
            "! Cache not writable: {}; results stay correct but every run recomputes. \
Point RIPR_CACHE_DIR at a writable directory.",
            display_text(&reason)
        ));
    }
    // #7663: an un-ignored artifact root is an advisory that is itself the
    // repair signal, so it stays on the compact screen. The line renders the
    // typed verdict the JSON report carries (#8257 review).
    if let output::doctor::DoctorArtifactIgnore::Unignored { warning } = &cache.artifact_ignore {
        lines.push(warning.clone());
    }
    if let output::doctor::DoctorArtifactTracking::Tracked { warning } = &cache.artifact_tracking {
        lines.push(warning.clone());
    }

    lines.extend(preview_language_enable_suggestions(
        environment.preview_enablement.as_ref(),
    ));

    for probe in &report.runtime_probes {
        if probe.required && probe.status == output::doctor::DoctorStatus::Fail {
            continue;
        }
        // Compact keeps enabled primary runtimes visible so source-build
        // still reports them as advisory (RIPR-SPEC-0007). Optional extras
        // (pnpm, pytest, …) stay behind `-v`.
        if verbose
            || (probe.required && probe.status == output::doctor::DoctorStatus::Pass)
            || primary_runtime_probe(probe)
        {
            lines.push(language_runtime_probe_display_line(probe));
        }
    }

    if profile == output::doctor::DoctorProfile::Analysis {
        let packet = root.join("target/ripr/reports/start-here.md");
        if verbose {
            lines.extend(doctor_start_here_lines(
                root,
                report,
                environment.preview_enablement.as_ref(),
            ));
        } else {
            let first = output::doctor::DoctorFirstCommand::resolve_for_report(report, || {
                analysis::working_tree_has_tracked_changes(root)
            });
            // A present packet is the user's next artifact, but Recovery /
            // Proof / Refresh-scope stay behind `-v` so the default screen
            // still fits 80x24 (#8168).
            if passed && packet.is_file() {
                lines.extend(doctor_packet_status_lines(root));
            } else if let Err(error) = first.command_line_for_root(root) {
                lines.push(format!(
                    "- Safe next action: {error}; restore access or select a lossless root alias, then rerun doctor."
                ));
            } else if let Some(line) =
                output::doctor::safe_next_action_recovery_line(report, first, root)
                    .filter(|_| !passed)
            {
                lines.push(line.to_string());
            }
            lines.extend(first.recommendation_lines_for(root));
            if matches!(first, output::doctor::DoctorFirstCommand::Worktree) {
                lines.push(
                    "- Scope note: `--worktree` analyzes staged and unstaged tracked edits; untracked files remain out of scope until staged or supplied through `--diff`."
                        .to_string(),
                );
            }
            if let Some(line) =
                enable_before_first_command_line(environment.preview_enablement.as_ref())
            {
                lines.push(line);
            }
        }
    }

    if passed {
        lines.push("✓ doctor checks passed".to_string());
    } else {
        lines.push(
            output::doctor::DOCTOR_FAILED_LINE
                .trim_end_matches('\n')
                .to_string(),
        );
    }

    if verbose {
        lines.push(String::new());
        lines.extend(binary.human_lines());
        lines.extend(config_verbose_lines(
            config,
            environment.facts.config_defaults.as_ref(),
            report,
        ));
        lines.extend(cache_detail_lines(cache));
        if !environment.facts.test_surfaces.is_empty() {
            let evidence = environment
                .facts
                .test_surfaces
                .iter()
                .map(|surface| surface.evidence.as_str())
                .collect::<Vec<_>>()
                .join("; ");
            lines.push(format!("- Detected test surfaces: {evidence}"));
        }
        if let Some(preview) = environment.facts.perl_preview.as_ref() {
            lines.extend(perl_preview_lines(preview));
        }
        lines.extend(known_limitation_lines());
        if profile == output::doctor::DoctorProfile::Analysis && !passed {
            // Failures already named the repair. Verbose still shows the
            // adapter/limitation dump above, not a second start-here wall.
        }
    }

    lines
}

fn cache_detail_lines(cache: &output::doctor::DoctorCacheStatus) -> Vec<String> {
    let mut lines = Vec::new();
    if cache.relocated_by_env {
        lines.push(format!(
            "- Cache location: {} (RIPR_CACHE_DIR active)",
            cache.cache_dir
        ));
    } else {
        lines.push(format!("- Cache location: {}", cache.cache_dir));
    }
    lines.push(format!(
        "- Cache size: {} (run `ripr cache status` for details)",
        cache.size_display
    ));
    // Same typed verdict the compact screen renders (#7663, #8257 review).
    if let output::doctor::DoctorArtifactIgnore::Unignored { warning } = &cache.artifact_ignore {
        lines.push(warning.clone());
    }
    if let output::doctor::DoctorArtifactTracking::Tracked { warning } = &cache.artifact_tracking {
        lines.push(warning.clone());
    }
    lines
}

fn config_verbose_lines(
    config: &Result<RiprConfig, String>,
    defaults: Option<&output::doctor::DoctorConfigDefaults>,
    report: &output::doctor::DoctorReport,
) -> Vec<String> {
    let mut lines = Vec::new();
    let Ok(_) = config else {
        return lines;
    };
    let Some(defaults) = defaults else {
        return lines;
    };
    // Compact already printed these two when a config file loaded.
    if defaults.source_path.is_none() {
        lines.push(format!(
            "- Analysis mode default: {}",
            defaults.analysis_mode
        ));
        lines.push(format!(
            "- LSP seam diagnostics default: {}",
            defaults.lsp_seam_diagnostics
        ));
    }
    lines.push(format!(
        "- Suppressions path: {}",
        defaults.suppressions_path
    ));
    lines.push(format!(
        "- Enabled languages: {}",
        report.languages.join(", ")
    ));
    if defaults.bun_ub_profile_configured {
        lines.push("- Bun UB profile: configured (preview advisory only)".to_string());
        lines.push(format!(
            "- Bun UB test roots: {}",
            defaults.bun_ub_test_roots.join(", ")
        ));
        lines.push(format!(
            "- Bun UB bridge hints: {}",
            defaults.bun_ub_bridge_hints.as_deref().unwrap_or("unknown")
        ));
        lines.push(
            "- Bun UB authority: no runtime Bun, tsc, tsserver, generated tests, gates, badges, baselines, or support-tier promotion"
                .to_string(),
        );
    } else {
        lines.push("- Bun UB profile: not configured".to_string());
    }
    lines
}

/// Present/stale packet plus the refresh command. Recovery states, proof
/// rail, and refresh-scope stay on the verbose dump so default doctor
/// still fits one 80x24 screen (#8168).
fn doctor_packet_status_lines(root: &Path) -> Vec<String> {
    let mut lines = Vec::new();
    let md = root.join("target/ripr/reports/start-here.md");
    if !md.is_file() {
        return lines;
    }
    // First-run honesty: name the packet as present only when it exists and
    // was written by this ripr. An unconditional path reads as an existing
    // artifact on a fresh workspace where `ripr first-pr` has never run
    // (RIPR-SPEC-0051 names the path, not its existence). A packet without
    // this ripr's `ripr_version` is stale_evidence after an upgrade: 0.10
    // packets have no version field, so existence alone cannot be trusted
    // (#4757). `is_file` (not `exists`) so a directory squatting the packet
    // path cannot read as openable evidence.
    use crate::agent::loop_commands::shell_arg;
    let json = root.join("target/ripr/reports/start-here.json");
    let freshness = crate::output::first_pr::start_here_json_version_freshness(&json);
    let stale_detail = crate::output::first_pr::start_here_version_stale_detail(&freshness);
    if let Some(detail) = &stale_detail {
        lines.push(format!(
            "- Start-here packet: target/ripr/reports/start-here.md ({detail})"
        ));
    } else {
        lines.push(
            "- Start-here packet: target/ripr/reports/start-here.md (present; open it first)"
                .to_string(),
        );
    }
    // Packet reads above follow filesystem path resolution. Resolve the
    // existing selected directory the same way: lexical cleanup of a
    // symlink followed by `..` can name a different repository. Keep the
    // shared lexical helper unchanged for not-yet-created output paths.
    let refresh_root = root
        .canonicalize()
        .map_err(|error| error.to_string())
        .and_then(|resolved| output::doctor::doctor_command_root_display(root, &resolved));
    match refresh_root {
        Ok(resolved_root) => {
            let refresh = format!(
                "ripr first-pr --root {} --head HEAD",
                shell_arg(&resolved_root)
            );
            if stale_detail.is_some() {
                lines.push(format!("- Safe next action: `{refresh}` refreshes it"));
            } else {
                lines.push(format!(
                    "- Safe next action: open that packet; `{refresh}` refreshes it"
                ));
            }
            if let output::markdown::PowershellForm::Translated(powershell) =
                output::markdown::powershell_form(&refresh)
            {
                lines.push(format!("- Refresh command (PowerShell): {powershell}"));
            }
        }
        Err(error) => lines.push(format!(
            "- Safe next action: refresh unavailable because the selected root could not be bound: {error}; restore access to that directory or use a lossless alias and rerun doctor."
        )),
    }
    lines
}

fn doctor_start_here_lines(
    root: &Path,
    report: &output::doctor::DoctorReport,
    preview_enablement: Option<&PreviewEnablement>,
) -> Vec<String> {
    let mut lines = Vec::new();
    // Both the first action and the recommendation consume this one fallible
    // route. No-packet guidance must not point at a command below when its
    // selected root cannot be rendered losslessly. The report states decide
    // before any probe: a missing root (#4531) is not probed for work-tree
    // changes, and a git binary that cannot run still wins over the
    // repository state (#4735).
    let first = output::doctor::DoctorFirstCommand::resolve_for_report(report, || {
        analysis::working_tree_has_tracked_changes(root)
    });
    let recommendation = first.command_line_for_root(root);
    // The safe next action follows the packet's freshness, because `first-pr`
    // composes the packet out of artifacts `ripr check` produces -- it runs no
    // analysis of its own (the boundary `help --all` states). Recommending it
    // on a fresh workspace dead-ends: measured, it returns `missing_artifacts`
    // and answers with `Regeneration command: ripr check ...`, which is the
    // command this same screen already prints three lines below. Two different
    // first commands on one screen, one of which bounces straight back to the
    // other, is not a route.
    let md = root.join("target/ripr/reports/start-here.md");
    if md.is_file() {
        lines.extend(doctor_packet_status_lines(root));
        if root
            .canonicalize()
            .ok()
            .and_then(|resolved| output::doctor::doctor_command_root_display(root, &resolved).ok())
            .is_some()
        {
            // First-pr owns default-base resolution; an old packet is
            // not authority for a custom comparison.
            lines.push(
                "- Refresh scope: the repository's default base and HEAD; add --base REF and --head REF for a custom comparison."
                    .to_string(),
            );
        }
    } else {
        lines.push(
            "- Start-here packet: target/ripr/reports/start-here.md (not yet generated; `ripr first-pr` composes it once analysis evidence exists)"
                .to_string(),
        );
        match &recommendation {
            Ok(_) => {
                // The missing-root recommendation is a recovery action, not a
                // runnable command (#5252 item 3): sending the reader to "run"
                // it would prescribe the failure doctor just diagnosed.
                lines.push(
                    output::doctor::safe_next_action_recovery_line(report, first, root)
                        .unwrap_or(output::doctor::SAFE_NEXT_ACTION_RUN_FIRST_LINE)
                        .to_string(),
                );
            }
            Err(error) => lines.push(format!(
                "- Safe next action: {error}; restore access or select a lossless root alias, then rerun doctor."
            )),
        }
    }
    lines.push(
        "- Recovery states: missing artifact, stale evidence, wrong root, malformed artifact, no actionable gap, preview-limited evidence"
            .to_string(),
    );
    lines.push(
        "- Proof rail: verify command, receipt command, and receipt path are advisory static movement evidence"
            .to_string(),
    );
    // First-run honesty: when the working tree has uncommitted changes,
    // a committed-history `ripr check` analyzes committed history only and would
    // silently exclude the user's draft (the RIPR-SPEC-0112 dirty-worktree case).
    // Route them to the command that actually covers their edits instead of the
    // one that looks clean while ignoring them. Reuses the same helper as the
    // check-time disclosure (reuse, don't fork). When git cannot run, both
    // `ripr check` and `--worktree` fail the same way; name the `--diff` route
    // instead and do not probe the worktree (#4735). A missing root or a root
    // Git will not read (#4531) cannot run the diff-scoped first command, and
    // probing its working tree only prints a raw git failure for a problem the
    // checks above already name.
    lines.extend(first.recommendation_lines_for(root));
    match first {
        output::doctor::DoctorFirstCommand::Worktree => {
            lines.push(
                "- Scope note: `--worktree` analyzes staged and unstaged tracked edits; untracked files remain out of scope until staged or supplied through `--diff`."
                    .to_string(),
            );
        }
        output::doctor::DoctorFirstCommand::DefaultCheck => {
            // No `--base origin/main`: this screen is read in whatever repository
            // the user has, and that ref does not exist in one whose default
            // branch is not `main`. Without a base, the loader resolves the
            // repository's own default (`analysis::diff::load::resolve_default_base`).
        }
        output::doctor::DoctorFirstCommand::SavedDiff => {}
        output::doctor::DoctorFirstCommand::MissingRoot
        | output::doctor::DoctorFirstCommand::OutsideGit
        | output::doctor::DoctorFirstCommand::InvalidConfig
        | output::doctor::DoctorFirstCommand::FailedPrerequisites => {}
        // #5259: the recommended line above already names the commit-first
        // repair; the advisory `git_head` check line carries the same state.
        output::doctor::DoctorFirstCommand::UnbornHead => {}
    }
    // A detected preview language that is not enabled is skipped by `ripr
    // check`, so in a TypeScript-only repository the recommended command is a
    // guaranteed no-op. Name the enable step next to the command.
    if let Some(line) = enable_before_first_command_line(preview_enablement) {
        lines.push(line);
    }
    lines
}

fn enable_before_first_command_line(enablement: Option<&PreviewEnablement>) -> Option<String> {
    // Name the config entries here: this line says what to write in
    // ripr.toml, while the Tip names the detected source.
    let names = enablement?
        .gaps
        .iter()
        .map(|gap| gap.entry.as_str())
        .collect::<Vec<_>>()
        .join(" and ");
    Some(format!(
        "- Before that: enable {names} in ripr.toml (see the Tip above); until then `ripr check` skips those files"
    ))
}

/// Language-to-status mapping used by the doctor first-run diagnosis.
///
/// Only Rust is `Stable`. All preview surfaces (TypeScript, JavaScript,
/// Python, Perl) carry `Preview` per `LanguageStatus::as_str()` and
/// RIPR-SPEC-0026.
fn language_status(id: LanguageId) -> LanguageStatus {
    match id {
        LanguageId::Rust => LanguageStatus::Stable,
        LanguageId::TypeScript | LanguageId::JavaScript | LanguageId::Python | LanguageId::Perl => {
            LanguageStatus::Preview
        }
    }
}

/// Shallow marker scan: look for files/dirs that indicate a language is
/// present. Only inspects `root`, `root/src/`, and immediate child dirs of
/// `root` — no recursion, no AST parsing, no workspace pipeline.
///
/// Returns `false` (no marker found) when any `read_dir` call fails; doctor
/// must never panic or OOM on a scan error.
fn shallow_has_extension(root: &Path, extension: &str) -> bool {
    let dirs_to_scan: [&Path; 2] = [root, &root.join("src")];
    for dir in dirs_to_scan {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some(extension) {
                    return true;
                }
            }
        }
    }
    // Also scan one level of child dirs of root.
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let child = entry.path();
            if child.is_dir() {
                let sub_entries = std::fs::read_dir(&child).into_iter().flatten().flatten();
                for sub in sub_entries {
                    let path = sub.path();
                    if path.extension().and_then(|e| e.to_str()) == Some(extension) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

fn shallow_has_file(root: &Path, name: &str) -> bool {
    root.join(name).exists() || root.join("src").join(name).exists()
}

/// Detect which languages have concrete file markers in this workspace.
/// Returns detected `LanguageId`s in a stable order (Rust first, then
/// TypeScript, JavaScript, Python, Perl).
fn detect_languages(root: &Path) -> Vec<LanguageId> {
    let mut found = Vec::new();

    // Rust: Cargo.toml at root OR .rs files in root/src (shared with the
    // enablement tip so a TypeScript-only root is not told to enable Rust).
    if crate::domain::rust_markers_at(root) {
        found.push(LanguageId::Rust);
    }

    // TypeScript: package.json, tsconfig.json, or a TypeScript-source
    // extension (.ts/.tsx/.mts/.cts) at a shallow depth. #4116: both
    // extension families consume the shared TS/JS extension authority.
    if shallow_has_file(root, "package.json")
        || shallow_has_file(root, "tsconfig.json")
        || analysis::TYPESCRIPT_SOURCE_EXTENSIONS
            .iter()
            .any(|extension| shallow_has_extension(root, extension))
    {
        found.push(LanguageId::TypeScript);
    }

    // JavaScript: JavaScript-source extensions (.js/.jsx/.mjs/.cjs), only
    // when no TypeScript markers were found.
    if !found.contains(&LanguageId::TypeScript)
        && analysis::JAVASCRIPT_SOURCE_EXTENSIONS
            .iter()
            .any(|extension| shallow_has_extension(root, extension))
    {
        found.push(LanguageId::JavaScript);
    }

    // Python: pyproject.toml, setup.py, setup.cfg, pytest.ini, or .py files
    if shallow_has_file(root, "pyproject.toml")
        || shallow_has_file(root, "setup.py")
        || shallow_has_file(root, "setup.cfg")
        || shallow_has_file(root, "pytest.ini")
        || shallow_has_extension(root, "py")
    {
        found.push(LanguageId::Python);
    }

    // Perl: the same predicate that gates the Perl preview section, so a
    // CPAN layout with modules below `lib/Name/` is detected and a `.t` under
    // `target/` is not.
    if perl_project_detected(root) {
        found.push(LanguageId::Perl);
    }

    found
}

/// Probe the runtimes the verify/proof route needs for each detected
/// language (#2071). A primary runtime is required only when its language is
/// enabled in the effective config. Detected-but-disabled preview runtimes
/// and optional test/package runners remain advisory.
fn add_language_runtime_probes<F>(
    root: &Path,
    detected: &[LanguageId],
    enabled: &[LanguageId],
    report: &mut output::doctor::DoctorReport,
    print: bool,
    mut probe: F,
) -> bool
where
    F: FnMut(&str) -> (output::doctor::DoctorStatus, String),
{
    let mut ok = true;
    for (language, tool, hint) in language_runtime_probes_for(root, detected, enabled) {
        let (status, evidence) = probe(tool);
        // A language runtime is an analysis capability, not a prerequisite
        // for building RIPR from source: the source-build profile keeps the
        // probe visible but never lets it decide that profile's status.
        let required = report.profile == output::doctor::DoctorProfile::Analysis
            && runtime_probe_is_required(language, tool, enabled);
        report.add_runtime_probe(language, tool, status, &evidence, required, hint);
        if required && status == output::doctor::DoctorStatus::Fail {
            ok = false;
        }
        if print && let Some(probe) = report.runtime_probes.last() {
            println!("{}", language_runtime_probe_display_line(probe));
        }
    }
    ok
}

fn probe_runtime(tool: &str) -> (output::doctor::DoctorStatus, String) {
    // Probe every runtime outside the checkout. yarn loads project config on
    // --version (#2183 review), and pnpm fetches and runs the release a
    // project `packageManager` names; version managers read project files too.
    // A hostile checkout must not choose what doctor executes.
    output::doctor::doctor_tool_check_isolated(tool)
}

fn runtime_probe_is_required(language: &str, tool: &str, enabled: &[LanguageId]) -> bool {
    // The language's primary runtime is the configured capability doctor can
    // require. A detected package manager or test runner is still useful
    // evidence, but remains advisory because a project may use another route.
    primary_runtime(language).is_some_and(|(primary, _)| primary == tool)
        && enabled.iter().any(|id| id.as_str() == language)
}

/// The primary runtime probe per language. Python's command name and install
/// hint depend on the host ([`python_runtime_for_os`]).
fn primary_runtime_probes() -> [(&'static str, &'static str, &'static str); 3] {
    let (python_tool, python_hint) = python_runtime_for_os(std::env::consts::OS);
    [
        ("typescript", "node", "install Node.js"),
        ("javascript", "node", "install Node.js"),
        ("python", python_tool, python_hint),
    ]
}

/// The Python command doctor probes and the install hint it prints, for a
/// host OS name as `std::env::consts::OS` spells it (#4378). Pure so every
/// host's answer is testable from any host.
///
/// The python.org and winget installers put `python` (not `python3`) on a
/// Windows PATH, and `apt` does not exist there, so Windows probes `python`
/// with a Windows install route. RIPR never runs the interpreter itself; the
/// probe only tells a user whether their Python verify route can start.
fn python_runtime_for_os(os: &str) -> (&'static str, &'static str) {
    match os {
        "windows" => (
            "python",
            "install Python and put `python` on PATH (e.g. winget install Python.Python.3.13)",
        ),
        "macos" => ("python3", "install python3 (e.g. brew install python)"),
        _ => ("python3", "install python3 (e.g. apt install python3)"),
    }
}

fn primary_runtime(language: &str) -> Option<(&'static str, &'static str)> {
    primary_runtime_probes()
        .into_iter()
        .find(|(candidate, _, _)| *candidate == language)
        .map(|(_, tool, hint)| (tool, hint))
}

fn language_runtime_probes_for(
    root: &Path,
    detected: &[LanguageId],
    enabled: &[LanguageId],
) -> Vec<(&'static str, &'static str, &'static str)> {
    let mut probes = language_runtime_probes_for_detected(root, detected);
    append_missing_primary_runtime_probes(&mut probes, enabled);
    probes
}

fn append_missing_primary_runtime_probes(
    probes: &mut Vec<(&'static str, &'static str, &'static str)>,
    enabled: &[LanguageId],
) {
    for (language, tool, hint) in primary_runtime_probes() {
        let enabled_language = enabled.iter().any(|id| id.as_str() == language);
        if enabled_language
            && !probes
                .iter()
                .any(|(found, detected_tool, _)| *found == language && *detected_tool == tool)
        {
            probes.push((language, tool, hint));
        }
    }
}

fn language_runtime_probe_display_line(probe: &output::doctor::DoctorRuntimeProbe) -> String {
    let evidence = display_text(&probe.evidence);
    let hint = display_text(&probe.hint);
    if probe.status == output::doctor::DoctorStatus::Fail && !probe.required {
        format!(
            "- {} verify-route runtime: {} — optional; {}",
            probe.language, evidence, hint
        )
    } else {
        language_runtime_probe_line(&probe.language, probe.status, &evidence, &hint)
    }
}

/// One doctor output line for a runtime probe (#2071). Pure so the emitted
/// contract (labels, evidence, install hint) is directly testable (#2183
/// review).
fn language_runtime_probe_line(
    language: &str,
    status: output::doctor::DoctorStatus,
    evidence: &str,
    hint: &str,
) -> String {
    match status {
        output::doctor::DoctorStatus::Pass => {
            format!("✓ {language} verify-route runtime: {evidence}")
        }
        output::doctor::DoctorStatus::Fail => {
            format!("! {language} verify-route runtime: {evidence} — {hint}")
        }
    }
}

/// The (language, tool, install hint) runtime probes for a root (#2071).
/// Factored from the printer so the probe list is directly testable.
/// [`language_runtime_probes_for_detected`] over the root's own marker scan.
#[cfg(test)]
fn language_runtime_probes(root: &Path) -> Vec<(&'static str, &'static str, &'static str)> {
    language_runtime_probes_for_detected(root, &detect_languages(root))
}

/// [`language_runtime_probes`] over an already-computed marker scan, so the
/// run's four consumers of the scan share one walk.
fn language_runtime_probes_for_detected(
    root: &Path,
    detected: &[LanguageId],
) -> Vec<(&'static str, &'static str, &'static str)> {
    let mut probes: Vec<(&str, &str, &str)> = Vec::new();
    if detected.contains(&LanguageId::Python) {
        let (python_tool, python_hint) = python_runtime_for_os(std::env::consts::OS);
        probes.push(("python", python_tool, python_hint));
        // Reuse the shared framework detector (#2183 review) — no parallel
        // marker list. Gated behind lang-python: the detector lives in the
        // Python adapter which is not compiled under --no-default-features
        // --features lang-rust (#2418).
        #[cfg(feature = "lang-python")]
        {
            if analysis::detect_python_test_frameworks(root).contains(&"pytest") {
                probes.push((
                    "python",
                    "pytest",
                    "install pytest (e.g. pip install pytest)",
                ));
            }
        }
    }
    for id in [LanguageId::TypeScript, LanguageId::JavaScript] {
        if !detected.contains(&id) {
            continue;
        }
        // Label with the actually-detected language (#2183 review): a
        // JS-only workspace must not read "typescript".
        let lang = id.as_str();
        probes.push((lang, "node", "install Node.js"));
        if root.join("bun.lockb").exists() || root.join("bun.lock").exists() {
            probes.push((lang, "bun", "install Bun"));
        }
        if root.join("pnpm-lock.yaml").exists() {
            probes.push((lang, "pnpm", "install pnpm"));
        }
        if root.join("yarn.lock").exists() {
            probes.push((lang, "yarn", "install Yarn"));
        }
    }
    probes
}

/// Print `- Detected languages: rust (stable), typescript (preview), …`
///
/// Each entry shows its canonical `LanguageStatus` tier in parentheses.
/// Appends `[adapter not compiled]` when `LanguageId::is_available()` is
/// false for the detected language. If no markers are found, prints
/// `none detected` rather than claiming any language.
/// The detected-languages line, followed by the unanalyzed-languages line
/// whenever such source exists: a mixed Rust and Go workspace needs the Go
/// half named as much as a Go-only one does.
///
/// Renders the same typed entries the JSON document serializes, so the two
/// surfaces cannot disagree about what was found.
fn detected_languages_lines(
    detected: &[output::doctor::DoctorDetectedLanguage],
    unanalyzed: &[output::doctor::DoctorUnanalyzedLanguage],
) -> Vec<String> {
    let mut lines = Vec::new();
    if detected.is_empty() {
        lines.push("- Detected languages: none detected".to_string());
    } else {
        let entries: Vec<String> = detected
            .iter()
            .map(|language| {
                if language.adapter_available {
                    format!("{} ({})", language.language, language.status.as_str())
                } else {
                    format!(
                        "{} ({}) [adapter not compiled]",
                        language.language,
                        language.status.as_str()
                    )
                }
            })
            .collect();
        lines.push(format!("- Detected languages: {}", entries.join(", ")));
    }
    lines.extend(unanalyzed_languages_line(unanalyzed));
    lines
}

/// Names source ripr cannot analyze, so a Go or Java repository is told why
/// `ripr check` will find nothing instead of being sent there as the
/// recommended first command, and a mixed workspace learns which half is
/// reported as not analyzed.
fn unanalyzed_languages_line(
    unanalyzed: &[output::doctor::DoctorUnanalyzedLanguage],
) -> Option<String> {
    if unanalyzed.is_empty() {
        return None;
    }
    let found = unanalyzed
        .iter()
        .map(|language| format!("{} ({} file(s))", language.language, language.file_count))
        .collect::<Vec<_>>()
        .join(", ");
    Some(format!(
        "~ Unanalyzed languages: {found}; ripr analyzes Rust, plus TypeScript/JavaScript and Python as previews, so changes to this source are reported as not analyzed, never as clean"
    ))
}

/// When a preview language is detected at the root but is not yet enabled in
/// `ripr.toml`, print a copy-paste-ready TOML block so the user can enable
/// it in a single edit.
///
/// Gated on BOTH conditions to stay fail-closed:
/// 1. `LanguageId::is_available()` — the adapter was compiled into this binary
///    (`cfg!(feature = "lang-<x>")`). If the feature was not compiled in, a
///    user cannot enable the adapter regardless of `ripr.toml`.
/// 2. The language is NOT already in `config.languages().enabled()`.
///
/// Emits nothing when either condition fails, when the root has no config
/// file, or when the config cannot be loaded.
/// Pure rendering of the preview-enablement tip — the tip lines (ready to
/// print) for each preview language that is detected, available (compiled in),
/// and not yet enabled in `ripr.toml`.
///
/// Returns an empty vec when there is nothing to suggest. The same
/// `PreviewEnablement` becomes the typed `preview_language_gaps` field, so the
/// tip and the JSON document name the same languages.
fn preview_language_enable_suggestions(enablement: Option<&PreviewEnablement>) -> Vec<String> {
    let Some(PreviewEnablement { gaps, suggestion }) = enablement else {
        return Vec::new();
    };
    // One snippet for every language to add, built on the languages already
    // enabled that belong in this repo: a per-language `["typescript"]`
    // snippet would disable Python in a mixed repository, so following one
    // tip would produce the other. Default `rust` is kept only when Rust is
    // actually detected (#7648).
    let names = gaps
        .iter()
        .map(|gap| gap.label.as_str())
        .collect::<Vec<_>>()
        .join(" and ");
    let enabled_names: Vec<&str> = suggestion.iter().map(LanguageId::as_str).collect();
    let enabled_line = crate::domain::format_languages_enabled_toml(&enabled_names);
    let javascript_note = if gaps.iter().any(|gap| gap.label == "javascript") {
        " (the `typescript` entry also analyzes JavaScript)"
    } else {
        ""
    };
    vec![format!(
        "- Tip: {names} files detected but not enabled, so `ripr check` does not analyze them. Set `{enabled_line}` under `[languages]` in ripr.toml{javascript_note}"
    )]
}

/// The enabled languages and the detected, compiled-in preview languages that
/// are not enabled. `None` when there is nothing to suggest or the config
/// cannot be loaded (fail closed: no tip).
fn preview_languages_to_enable(
    detected: &[LanguageId],
    config: &Result<RiprConfig, String>,
) -> Option<PreviewEnablement> {
    // JavaScript is analyzed by the TypeScript adapter and has no config
    // entry of its own (`parse_languages_enabled` accepts only `typescript`),
    // so a detected JavaScript source maps to the `typescript` entry. Using
    // the scanner id would print `"javascript"`, which config loading rejects.
    let mut preview_detected: Vec<LanguageId> = Vec::new();
    for id in detected
        .iter()
        .copied()
        .filter(|id| matches!(language_status(*id), LanguageStatus::Preview))
    {
        let entry = config_entry(id);
        if !preview_detected.contains(&entry) {
            preview_detected.push(entry);
        }
    }
    if preview_detected.is_empty() {
        return None;
    }
    let config = config.as_ref().ok()?;
    let enabled = config.languages().enabled().to_vec();
    // One pass builds each (config entry, detected source) pair together, so the
    // count and the pairing are the same fact the tip prints and the JSON
    // document serializes (#5214 review).
    let gaps: Vec<PreviewLanguageGap> = preview_detected
        .into_iter()
        // Perl detects as a preview language (see language_status). In a
        // default build, `LanguageId::Perl.is_available()` is
        // `cfg!(feature="lang-perl")` == false, so the Tip never fires for
        // Perl anyway. This guard is defense-in-depth for the
        // `--features lang-perl` build: even when the Cargo feature is ON,
        // the adapter is still scaffold-only (#[cfg(test)] mod perl; not
        // production-routable, pipeline fail-closed stub). Suggesting
        // `perl` in that build would mislead: the user would enable it and
        // get zero analysis plus an explicit error. Detection at
        // detect_languages() stays honest; only the enablement Tip is
        // suppressed for Perl until Campaign 31 (#1379) lands the production
        // bridge. TypeScript/Python are real preview adapters and remain
        // Tip-eligible.
        .filter(|id| id.is_available() && !enabled.contains(id) && !matches!(id, LanguageId::Perl))
        .map(|entry| {
            let javascript_only = entry == LanguageId::TypeScript
                && !detected.contains(&LanguageId::TypeScript)
                && detected.contains(&LanguageId::JavaScript);
            PreviewLanguageGap {
                entry,
                label: if javascript_only {
                    "javascript".to_string()
                } else {
                    entry.as_str().to_string()
                },
            }
        })
        .collect();
    if gaps.is_empty() {
        return None;
    }
    let suggestion = crate::domain::enablement_tip_languages(
        &enabled,
        detected.contains(&LanguageId::Rust),
        gaps.iter().map(|gap| gap.entry),
    );
    Some(PreviewEnablement { gaps, suggestion })
}

/// The `[languages].enabled` entries a doctor tip may add, each paired with the
/// detected source it turns on.
///
/// The pair is built once and stored together, so the human tip and the typed
/// `preview_language_gaps` can never disagree about how many entries there are
/// or which label belongs to which entry (#5214 review: two parallel vectors
/// zipped together truncate silently the moment they diverge, which would drop a
/// gap from JSON while the screen still printed it).
struct PreviewEnablement {
    /// One (config entry, detected source name) pair per language to add.
    gaps: Vec<PreviewLanguageGap>,
    /// Copy-paste `[languages] enabled` list: already-enabled languages that
    /// belong in this repo, plus the gaps. Default `rust` is omitted when
    /// Rust is not detected (#7648).
    suggestion: Vec<LanguageId>,
}

/// One `[languages].enabled` entry and the detected source it analyzes.
struct PreviewLanguageGap {
    /// The config entry to add, a value `parse_languages_enabled` accepts.
    entry: LanguageId,
    /// The user-facing name of the detected source that entry analyzes.
    label: String,
}

/// The `[languages].enabled` entry that turns on analysis of `id`.
fn config_entry(id: LanguageId) -> LanguageId {
    match id {
        LanguageId::JavaScript => LanguageId::TypeScript,
        other => other,
    }
}

/// Print the detected test surfaces the environment probe found.
///
/// `framework` stays `None` rather than guessing when no clear marker is
/// found — doctor never claims a framework it cannot confirm. The
/// `<language>: …` fragment printed here is the same string the JSON
/// `test_surfaces[].evidence` carries.
/// Build the detected test surfaces for doctor (#2106). Each entry is one
/// `<language>: …` fragment the human screen prints, plus the detected
/// framework name typed separately for machine consumers.
fn detect_test_surfaces(
    root: &Path,
    detected: &[LanguageId],
) -> Vec<output::doctor::DoctorTestSurface> {
    use output::doctor::DoctorTestSurface;
    if detected.is_empty() {
        return Vec::new();
    }
    let surface = |language: &str, framework: Option<&str>, evidence: String| DoctorTestSurface {
        language: language.to_string(),
        framework: framework.map(str::to_string),
        evidence,
    };
    let mut surfaces: Vec<DoctorTestSurface> = Vec::new();
    for id in detected {
        match id {
            LanguageId::Rust => {
                // Cargo.toml presence is the Rust test surface marker
                // (`cargo test` and `#[cfg(test)]` are available in any
                // Cargo workspace).
                if root.join("Cargo.toml").exists() {
                    surfaces.push(surface(
                        "rust",
                        Some("cargo test"),
                        "rust: cargo test (#[cfg(test)])".to_string(),
                    ));
                } else {
                    surfaces.push(surface(
                        "rust",
                        None,
                        "rust: test framework not detected".to_string(),
                    ));
                }
            }
            LanguageId::Python => {
                // One shared detector (#2106 / #7664): the same pytest/unittest
                // marker set the adapter's code-level detection implies.
                // Mixed evidence emits one surface per confirmed framework.
                #[cfg(feature = "lang-python")]
                let frameworks = analysis::detect_python_test_frameworks(root);
                #[cfg(not(feature = "lang-python"))]
                let frameworks: Vec<&'static str> =
                    if root.join("pytest.ini").exists() || root.join("pyproject.toml").exists() {
                        vec!["pytest"]
                    } else {
                        Vec::new()
                    };
                if frameworks.is_empty() {
                    surfaces.push(surface(
                        "python",
                        None,
                        "python: test framework not detected".to_string(),
                    ));
                } else {
                    for name in frameworks {
                        surfaces.push(surface("python", Some(name), format!("python: {name}")));
                    }
                }
            }
            LanguageId::TypeScript | LanguageId::JavaScript => {
                // One shared detector (#2106): the same package.json /
                // config-file signals the adapter's package discovery trusts.
                let lang = id.as_str();
                #[cfg(feature = "lang-typescript")]
                let framework = analysis::detect_typescript_test_framework(root);
                #[cfg(not(feature = "lang-typescript"))]
                let framework: Option<&'static str> = if root.join("jest.config.js").exists()
                    || root.join("jest.config.ts").exists()
                    || root.join("jest.config.mjs").exists()
                    || root.join("jest.config.cjs").exists()
                {
                    Some("jest")
                } else if root.join("vitest.config.ts").exists()
                    || root.join("vitest.config.js").exists()
                    || root.join("vitest.config.mjs").exists()
                {
                    Some("vitest")
                } else if root.join("bun.lockb").exists() {
                    Some("bun")
                } else {
                    None
                };
                match framework {
                    Some(name) => {
                        surfaces.push(surface(lang, Some(name), format!("{lang}: {name}")))
                    }
                    None => surfaces.push(surface(
                        lang,
                        None,
                        format!("{lang}: test framework not detected"),
                    )),
                }
            }
            LanguageId::Perl => {
                // Phase D PR 2 (#1408): upgraded Perl doctor diagnostics.
                let pm_count = count_files(root, "pm");
                let pl_count = count_files(root, "pl");
                let t_count = count_files(root, "t");
                if pm_count > 0 || pl_count > 0 || t_count > 0 {
                    let framework = detect_perl_framework(root);
                    surfaces.push(surface(
                        "perl",
                        perl_framework_name(framework),
                        format!(
                            "perl: {} .pm, {} .pl, {} .t; framework: {}",
                            pm_count, pl_count, t_count, framework
                        ),
                    ));
                    // Report adapter availability.
                    if id.is_available() {
                        surfaces.push(surface(
                            "perl",
                            None,
                            "perl: adapter compiled (lang-perl feature ON)".to_string(),
                        ));
                    } else {
                        // #7434: this catalog line is a semicolon-joined
                        // detection list, not the recovery owner. The full
                        // `unavailable_adapter_recovery()` paragraph already
                        // prints as Perl preview `next:` (and JSON
                        // `perl_preview.next_command`). Repeating it here
                        // made the human screen print the same ~340-character
                        // block twice. JSON `test_surfaces[].evidence` is this
                        // same catalog fragment by contract, so the field
                        // stays; only the catalog wording shortens.
                        surfaces.push(surface(
                            "perl",
                            None,
                            "perl: adapter NOT compiled (see Perl preview below)".to_string(),
                        ));
                    }
                    // Report runner availability from PATH, never the checkout cwd
                    // (#5103 / #4742 residual), through the same typed surface the
                    // JSON document serializes.
                    surfaces.push(surface("perl", None, perl_prove_path_line()));
                    // Report exact first command.
                    if id.is_available() {
                        surfaces.push(surface(
                            "perl",
                            None,
                            "perl: first command: ripr check --perl-facts <packet.json> --diff <diff.patch> --json".to_string(),
                        ));
                    }
                } else {
                    surfaces.push(surface(
                        "perl",
                        None,
                        "perl: no Perl files detected".to_string(),
                    ));
                }
            }
        }
    }
    surfaces
}

/// The framework name a `detect_perl_framework` answer carries, or `None` for
/// its two "not detected" spellings, which name no framework.
fn perl_framework_name(framework: &str) -> Option<&str> {
    if framework.starts_with("not detected") {
        None
    } else {
        Some(framework)
    }
}

/// Count files with a given extension under the root (recursive). Used by the
/// Perl preview to report real .pm/.pl/.t counts. Campaign 31 item 5: the
/// prior `shallow_has_extension as usize` returned only 0/1, not a real count.
/// Hidden and build/dependency directories that the Perl file walks skip,
/// so vendored or generated files neither inflate counts nor detect Perl.
fn is_skipped_walk_dir(path: &Path) -> bool {
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    name.starts_with('.') || matches!(name, "target" | "node_modules" | "blib")
}

/// True for a real directory entry. `DirEntry::file_type` does not follow
/// symlinks, so a `src/loop -> .` link cannot send doctor's walks into an
/// unbounded descent (the other workspace walkers already skip links). An
/// entry whose type cannot be read is not descended. Non-directory entries
/// are counted only when `Path::is_file` holds, so a directory link named
/// `x.pm` is neither descended nor counted as a Perl file.
fn is_walkable_dir(entry: &std::fs::DirEntry) -> bool {
    entry.file_type().is_ok_and(|kind| kind.is_dir())
}

/// True when the workspace has a CPAN build marker or any `.pm`, `.pl` or
/// `.t` file outside skipped directories. One walk that stops at the first
/// hit; `detect_languages` and the Perl preview share this definition.
fn perl_project_detected(root: &Path) -> bool {
    fn any_perl_file(dir: &Path) -> bool {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return false;
        };
        entries.flatten().any(|entry| {
            let path = entry.path();
            if is_walkable_dir(&entry) {
                !is_skipped_walk_dir(&path) && any_perl_file(&path)
            } else {
                path.is_file()
                    && matches!(
                        path.extension().and_then(|e| e.to_str()),
                        Some("pm" | "pl" | "t")
                    )
            }
        })
    }
    has_perl_project_markers(root) || any_perl_file(root)
}

fn count_files(root: &Path, ext: &str) -> usize {
    fn count_recursive(dir: &Path, ext: &str) -> usize {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return 0;
        };
        let mut n = 0;
        for entry in entries.flatten() {
            let path = entry.path();
            if is_walkable_dir(&entry) {
                if is_skipped_walk_dir(&path) {
                    continue;
                }
                n += count_recursive(&path, ext);
            } else if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some(ext) {
                n += 1;
            }
        }
        n
    }
    count_recursive(root, ext)
}

/// Detect the Perl test framework from .t files (shallow scan).
fn detect_perl_framework(root: &Path) -> &'static str {
    let t_dir = root.join("t");
    let Ok(entries) = std::fs::read_dir(&t_dir) else {
        return "not detected (no t/ directory)";
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "t")
            && let Ok(content) = std::fs::read_to_string(&path)
        {
            if content.contains("use Test2::V0") {
                return "Test2::V0";
            }
            if content.contains("use Test::More") {
                return "Test::More";
            }
            if content.contains("use Test::Exception") {
                return "Test::Exception";
            }
            if content.contains("use Test::Fatal") {
                return "Test::Fatal";
            }
        }
    }
    "not detected"
}

/// Perl test runners doctor may name. Availability is PATH-only (#5103).
const PERL_PATH_RUNNERS: [&str; 4] = ["prove", "yath", "carton", "dzil"];

fn perl_prove_path_line() -> String {
    if path_command("prove").is_some() {
        "perl: prove available on PATH".to_string()
    } else {
        "perl: prove NOT found on PATH".to_string()
    }
}

/// Resolve a doctor probe name to a program.
///
/// Bare names (`prove`, `perllsp`) come from PATH only. Windows `where`
/// searches the process cwd first, so a repo-local `prove.cmd` must not
/// count. Names that already contain a path separator are explicit paths
/// (the opted-in `[perl].executable`).
fn doctor_program(candidate: &str) -> Option<PathBuf> {
    if program_name_is_explicit_path(candidate) {
        Some(PathBuf::from(candidate))
    } else {
        path_command(candidate)
    }
}

fn program_name_is_explicit_path(name: &str) -> bool {
    !name.is_empty() && (name.contains('/') || name.contains('\\'))
}

/// True when `name` is a file on PATH. Does not search the process cwd.
fn path_command(name: &str) -> Option<PathBuf> {
    let pathext = std::env::var("PATHEXT").ok();
    path_command_in(
        name,
        std::env::var_os("PATH").unwrap_or_default(),
        pathext.as_deref(),
        cfg!(windows),
        &path_command_exists,
    )
}

fn path_command_exists(path: &Path) -> bool {
    let Ok(meta) = path.metadata() else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// PATH-only lookup. Empty and `.` PATH entries are cwd aliases (`where` and
/// Windows empty PATH components); they are skipped so a checkout `prove.cmd`
/// cannot read as "available on PATH". Pure over its inputs so the Windows
/// cwd-first branch is testable on any host (#5103).
fn path_command_in(
    name: &str,
    path: impl AsRef<OsStr>,
    pathext: Option<&str>,
    windows: bool,
    is_present: &dyn Fn(&Path) -> bool,
) -> Option<PathBuf> {
    if name.is_empty() || program_name_is_explicit_path(name) {
        return None;
    }
    std::env::split_paths(path.as_ref()).find_map(|dir| {
        if is_cwd_path_entry(&dir) {
            return None;
        }
        path_command_candidates(&dir, name, pathext, windows)
            .into_iter()
            .find(|candidate| is_present(candidate))
    })
}

fn is_cwd_path_entry(dir: &Path) -> bool {
    // Empty and `.` are cwd aliases (`where` / Windows empty PATH components).
    // `./` equals `.` via Path components on every host. `.\\` equals `.` on
    // Windows, but Unix Path treats `.\\` as a Normal filename, so keep the
    // token for host-independent Windows PATH strings (#5103).
    dir.as_os_str().is_empty() || dir == Path::new(".") || dir == Path::new(".\\")
}

fn path_command_candidates(
    dir: &Path,
    name: &str,
    pathext: Option<&str>,
    windows: bool,
) -> Vec<PathBuf> {
    if !windows {
        return vec![dir.join(name)];
    }
    // PATHEXT suffixes first, then the extensionless name as a fallback. An
    // extensionless `perllsp` must not hide `perllsp.bat` from exporter spawn.
    let mut candidates = Vec::new();
    let exts = pathext.unwrap_or(".COM;.EXE;.BAT;.CMD");
    for ext in exts.split(';') {
        let ext = ext.trim();
        if ext.is_empty() || ext == "." {
            continue;
        }
        let ext = if ext.starts_with('.') {
            ext.to_ascii_lowercase()
        } else {
            format!(".{}", ext.to_ascii_lowercase())
        };
        // `CreateProcessW` (what process spawning uses) launches only these
        // suffixes directly; interpreter-only entries such as `.PS1`, `.VBS`
        // or `.JS` would be chosen first and then fail to start (#5351).
        if !matches!(ext.as_str(), ".com" | ".exe" | ".bat" | ".cmd") {
            continue;
        }
        let candidate = dir.join(format!("{name}{ext}"));
        if !candidates.contains(&candidate) {
            candidates.push(candidate);
        }
    }
    let bare = dir.join(name);
    if !candidates.contains(&bare) {
        candidates.push(bare);
    }
    candidates
}

/// Rich Perl preview for the doctor (Campaign 31 item 5). Reports everything a
/// maintainer needs to know whether Perl analysis is available and how to
/// invoke it: project markers, lang-perl compiled, [perl] producer configured,
/// perllsp/perl-lsp found + version, schema compatible, t/ and t2/ roots,
/// detected test frameworks, runner availability, and an exact next command.
///
/// Conservative throughout: every line reports only what the static layer can
/// determine. No claim is made that the producer works end-to-end (that is the
/// two-binary proof, item 3). Prints only when Perl markers are detected.
/// Shared wording for a refused `[perl].executable`. Compact and verbose
/// both include this exact phrase so the opt-in smoke test cannot pass one
/// surface and miss the other. Control and bidi characters are spelled
/// `\u{XX}` so a crafted path cannot forge extra doctor lines.
fn ignored_perl_executable_disclosure(refused: &str) -> String {
    format!(
        "ignoring [perl].executable `{}` from ripr.toml (not run)",
        display_text(refused)
    )
}

/// One line for a workspace where Perl is detected and its adapter is compiled
/// in but `[languages] enabled` omits it: `ripr check` skips Perl until the
/// config says otherwise. Names the managed-mode config and whether the
/// canonical exporter is on PATH. Silent when Perl is enabled, not detected,
/// not compiled (the detected-languages line already says so), or when
/// enablement is unknown (config failed to load).
fn perl_not_enabled_line(
    detected: &[output::doctor::DoctorDetectedLanguage],
    preview: Option<&output::doctor::DoctorPerlPreview>,
) -> Option<String> {
    let perl = detected
        .iter()
        .find(|language| language.language == LanguageId::Perl.as_str())?;
    if !perl.adapter_available || perl.enabled != Some(false) {
        return None;
    }
    let exporter = crate::domain::PERL_FACT_EXPORTER;
    let on_path = match preview {
        Some(p) if p.canonical_exporter_on_path => "is on PATH",
        Some(_) => "is NOT on PATH",
        None => "PATH state unknown",
    };
    Some(format!(
        "~ Perl is detected but not enabled, so `ripr check` skips it. Add \"perl\" to `enabled` under `[languages]` and set `producer = \"{exporter}\"` under `[perl]` in ripr.toml; `{exporter}` {on_path}"
    ))
}

fn ignored_perl_executable_compact_line(
    preview: Option<&output::doctor::DoctorPerlPreview>,
) -> Option<String> {
    preview
        .and_then(|preview| preview.ignored_configured_executable.as_deref())
        .map(|refused| format!("- {}", ignored_perl_executable_disclosure(refused)))
}

/// The Perl preview lines, in the order the human screen prints them. Rendered
/// from the same typed value the JSON `perl_preview` object serializes, so the
/// two surfaces cannot drift.
fn perl_preview_lines(preview: &output::doctor::DoctorPerlPreview) -> Vec<String> {
    let mut lines = vec![
        "- Perl preview:".to_string(),
        format!(
            "project: {} .pm, {} .pl, {} .t",
            preview.pm_files, preview.pl_files, preview.t_files
        ),
    ];

    // lang-perl compiled? (cfg!(feature = "lang-perl") is build-time constant.)
    if preview.adapter_compiled {
        lines.push("adapter: compiled (lang-perl feature ON)".to_string());
    } else {
        lines.push("adapter: NOT compiled in this ripr binary (see next)".to_string());
    }

    // [perl] producer configured?
    lines.push(match preview.producer.as_deref() {
        Some("perl-ripr-facts") => {
            "producer: configured as `perl-ripr-facts` (canonical)".to_string()
        }
        Some("perllsp") => "producer: configured as `perllsp` (compatibility wrapper)".to_string(),
        Some("perl-lsp") => {
            "producer: configured as `perl-lsp` (compatibility wrapper)".to_string()
        }
        Some(other) => format!("producer: configured as `{other}`"),
        None => "producer: not configured (managed mode off)".to_string(),
    });

    // A configured executable the opt-in gate refused, so the user can see why
    // the configured path was not probed.
    if let Some(refused) = &preview.ignored_configured_executable {
        lines.push(format!(
            "executable: {}; set {}=1 to trust it",
            ignored_perl_executable_disclosure(refused),
            crate::config::PERL_EXECUTABLE_OPT_IN_ENV
        ));
    }

    // Find a compatible exporter: one that answers `--version` AND accepts
    // the managed `ripr-facts` subcommand. A binary that only answers
    // `--version` (for example the published perllsp LSP server) is reported
    // as found-but-incompatible, never as a working exporter.
    lines.extend(perl_exporter_lines(&preview.exporter));

    // schema compatible? (always reports the schema this ripr build consumes.)
    lines.push(format!("schema: {} expected", preview.expected_schema));

    // t/ and t2/ roots detected?
    lines.push(format!(
        "test roots: {}",
        perl_test_roots_display(&preview.test_roots)
    ));

    // Detected test frameworks.
    lines.push(format!(
        "frameworks: {}",
        perl_list_display(&preview.frameworks, "none detected")
    ));

    // Runner availability: prove/yath/carton/dzil.
    lines.push(format!(
        "runners: {}",
        perl_list_display(&preview.runners, "none found on PATH")
    ));

    // Exact next command: branch on whether the adapter is compiled in,
    // whether managed mode is configured, and whether a COMPATIBLE exporter
    // is present.
    lines.push(format!("next: {}", preview.next_command));

    // The heading is the one line without the two-space body indent.
    let mut rendered = Vec::new();
    for (index, line) in lines.into_iter().enumerate() {
        if index == 0 {
            rendered.push(line);
        } else {
            rendered.push(format!("  {line}"));
        }
    }
    rendered
}

/// The `test roots:` value for the detected Perl test roots.
fn perl_test_roots_display(roots: &[&str]) -> String {
    match roots {
        [t, t2] => format!("{t} and {t2} detected"),
        [only] => format!("{only} detected"),
        [] => "none detected".to_string(),
        _ => roots.join(", "),
    }
}

fn perl_list_display(values: &[&str], empty: &str) -> String {
    if values.is_empty() {
        empty.to_string()
    } else {
        values.join(", ")
    }
}

/// The typed Perl preview state, or `None` when the marker scan found no Perl
/// project. Computed once per run and read by both surfaces.
fn probe_perl_preview(
    root: &Path,
    config: Option<&RiprConfig>,
) -> Option<output::doctor::DoctorPerlPreview> {
    if !perl_project_detected(root) {
        return None;
    }
    let pm_count = count_files(root, "pm");
    let pl_count = count_files(root, "pl");
    let t_count = count_files(root, "t");
    let producer_configured = perl_producer_configured(config);
    let refused_executable =
        config.and_then(|config| config.perl().refused_executable().map(Path::to_path_buf));
    let exporter = probe_perl_exporter(config);
    let adapter_compiled = cfg!(feature = "lang-perl");
    Some(output::doctor::DoctorPerlPreview {
        pm_files: pm_count,
        pl_files: pl_count,
        t_files: t_count,
        adapter_compiled,
        producer: producer_configured.clone(),
        ignored_configured_executable: refused_executable.map(|path| path.display().to_string()),
        canonical_exporter_on_path: doctor_program(crate::domain::PERL_FACT_EXPORTER).is_some(),
        exporter: exporter_typed(&exporter),
        expected_schema: crate::app::PERL_FACT_PACKET_SCHEMA.to_string(),
        test_roots: detect_perl_test_roots(root),
        frameworks: detect_perl_frameworks(root),
        runners: perl_runners(),
        // Exact next command: branch on whether the adapter is compiled in,
        // whether managed mode is configured, and whether a COMPATIBLE exporter
        // is present.
        next_command: perl_next_command(
            adapter_compiled,
            producer_configured.as_deref(),
            exporter.compatible_bin(),
        ),
    })
}

fn exporter_typed(exporter: &PerlExporterProbe) -> output::doctor::DoctorPerlExporter {
    use output::doctor::{DoctorPerlExporter, DoctorPerlExporterState};
    let (state, (executable, version)) = match exporter {
        PerlExporterProbe::Compatible { bin, version } => (
            DoctorPerlExporterState::Compatible,
            (Some(bin.clone()), Some(version.clone())),
        ),
        PerlExporterProbe::Incompatible { bin, version } => (
            DoctorPerlExporterState::Incompatible,
            (Some(bin.clone()), Some(version.clone())),
        ),
        PerlExporterProbe::NotFound => (DoctorPerlExporterState::NotFound, (None, None)),
    };
    DoctorPerlExporter {
        state,
        executable,
        version,
    }
}

/// Perl test runners found on PATH, in a fixed order.
///
/// Availability is PATH-only and comes from the shared
/// [`PERL_PATH_RUNNERS`] order and resolver (#5103 / #4742 residual): a
/// repo-local `prove.cmd` in the checkout must never read as "available on
/// PATH". This feeds the typed `perl_preview.runners` the JSON document
/// serializes, so the machine surface inherits the same rule rather than a
/// second copy of it.
fn perl_runners() -> Vec<&'static str> {
    PERL_PATH_RUNNERS
        .into_iter()
        .filter(|name| path_command(name).is_some())
        .collect()
}

/// Whether `[perl].producer` is configured in the root's ripr config. Returns
/// the configured producer name, or None if not set / config unreadable.
/// Takes the already-loaded configuration so the preview agrees with
/// `config_defaults` instead of racing a second load.
fn perl_producer_configured(config: Option<&RiprConfig>) -> Option<String> {
    let config = config?;
    config.perl().producer().map(|s| s.to_string())
}

/// Result of probing for a Perl fact exporter.
#[derive(Debug, PartialEq, Eq)]
enum PerlExporterProbe {
    /// Answers `--version` and accepts the managed `ripr-facts` subcommand.
    Compatible { bin: String, version: String },
    /// Answers `--version` but rejects `ripr-facts`, so managed mode would
    /// fail against it. Not an exporter ripr can use.
    Incompatible { bin: String, version: String },
    /// No candidate answered `--version`.
    NotFound,
}

impl PerlExporterProbe {
    fn compatible_bin(&self) -> Option<&str> {
        match self {
            PerlExporterProbe::Compatible { bin, .. } => Some(bin),
            _ => None,
        }
    }
}

/// Upper bound on bytes captured from each probe stream. Help and version
/// text is small; an unknown binary must not flood the doctor.
const PERL_EXPORTER_PROBE_OUTPUT_LIMIT: usize = 64 * 1024;

/// Probe for a compatible Perl fact exporter. Honors `[perl].executable`
/// when set; otherwise probes PATH for `perl-ripr-facts` (canonical, post
/// perl-lsp-swarm #3294), then `perllsp` (the compatibility name managed
/// mode invokes for `producer = "perllsp"` or `"perl-lsp"`). A bare
/// `perl-lsp` binary is not probed: managed mode never invokes that name,
/// and unrelated crates install binaries called `perl-lsp`.
///
/// Compatibility is a capability probe, not an end-to-end proof: the
/// candidate must exit successfully for `ripr-facts --help` and its help
/// must mention `--schema`, the first flag of the managed argv. Both probes
/// run under the configured `[perl].timeout_ms` deadline with bounded
/// capture and a null stdin, so an LSP server that waits on stdin cannot
/// hang the doctor. Packet validity is still only checked by `ripr check`.
fn probe_perl_exporter(config: Option<&RiprConfig>) -> PerlExporterProbe {
    let timeout =
        std::time::Duration::from_millis(config.map_or(30_000, |c| c.perl().timeout_ms()));
    // `[perl].executable` from ripr.toml is only probed when the user opts
    // in (see `PerlConfig::executable`); doctor is usually the first command
    // run in a fresh clone and must not execute a repository-chosen program.
    let explicit = config.and_then(|c| c.perl().executable().map(|p| p.display().to_string()));
    let candidates: Vec<String> = match explicit {
        Some(path) => vec![path],
        None => vec![
            crate::domain::PERL_FACT_EXPORTER.to_string(),
            "perllsp".to_string(),
        ],
    };
    let mut first_incompatible = None;
    for candidate in &candidates {
        let Some(program) = doctor_program(candidate) else {
            continue;
        };
        let bin = program.display().to_string();
        let Some(version) = run_exporter_probe(&program, &["--version"], timeout)
            .filter(|output| output.status.success())
            .map(|output| {
                String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string()
            })
        else {
            continue;
        };
        let version = if version.is_empty() {
            "version unknown".to_string()
        } else {
            version
        };
        if exporter_accepts_ripr_facts(&program, timeout) {
            return PerlExporterProbe::Compatible { bin, version };
        }
        first_incompatible.get_or_insert(PerlExporterProbe::Incompatible { bin, version });
    }
    first_incompatible.unwrap_or(PerlExporterProbe::NotFound)
}

/// Whether `candidate ripr-facts --help` succeeds and documents `--schema`.
fn exporter_accepts_ripr_facts(candidate: &Path, timeout: std::time::Duration) -> bool {
    run_exporter_probe(candidate, &["ripr-facts", "--help"], timeout).is_some_and(|output| {
        output.status.success()
            && (String::from_utf8_lossy(&output.stdout).contains("--schema")
                || String::from_utf8_lossy(&output.stderr).contains("--schema"))
    })
}

/// The single exporter spawn site for doctor: bounded, deadline-enforced,
/// null stdin. `None` when the binary cannot be spawned, times out, or
/// exceeds the capture limit.
fn run_exporter_probe(
    candidate: &Path,
    args: &[&str],
    timeout: std::time::Duration,
) -> Option<std::process::Output> {
    let mut command = std::process::Command::new(candidate);
    command.args(args);
    crate::git::collect_output_with_deadline_and_limit(
        command,
        timeout,
        PERL_EXPORTER_PROBE_OUTPUT_LIMIT,
        &format!("Perl fact exporter probe `{}`", candidate.display()),
    )
    .ok()
}

/// Doctor lines for the typed exporter probe result. The `Compatible` and
/// `Incompatible` arms name the probed binary and its version because the
/// probe only recorded them for those two states.
fn perl_exporter_lines(exporter: &output::doctor::DoctorPerlExporter) -> Vec<String> {
    use output::doctor::DoctorPerlExporterState;
    match exporter.state {
        DoctorPerlExporterState::Compatible => vec![format!(
            "exporter: compatible `{}` ({}) accepts `ripr-facts` (capability probe only; packets are validated by `ripr check`)",
            exporter.executable.as_deref().unwrap_or_default(),
            exporter.version.as_deref().unwrap_or_default()
        )],
        DoctorPerlExporterState::Incompatible => vec![
            format!(
                "exporter: found `{}` ({}) but it does not accept `ripr-facts`; not a compatible exporter",
                exporter.executable.as_deref().unwrap_or_default(),
                exporter.version.as_deref().unwrap_or_default()
            ),
            format!(
                "note: managed mode runs `<exporter> ripr-facts --schema {} ...`; the compatible exporter is `{}`, which is not yet published",
                crate::app::PERL_FACT_PACKET_SCHEMA,
                crate::domain::PERL_FACT_EXPORTER
            ),
        ],
        DoctorPerlExporterState::NotFound => vec![format!(
            "exporter: NOT found (expected `{}` or a `perllsp` wrapper on PATH, or [perl].executable with {}=1); `{}` is not yet published",
            crate::domain::PERL_FACT_EXPORTER,
            crate::config::PERL_EXECUTABLE_OPT_IN_ENV,
            crate::domain::PERL_FACT_EXPORTER
        )],
    }
}

/// Detect CPAN-style project markers beyond .pm/.pl/.t files: Makefile.PL,
/// Build.PL, cpanfile. These confirm a real CPAN-style project a producer can
/// index.
fn has_perl_project_markers(root: &Path) -> bool {
    ["Makefile.PL", "Build.PL", "cpanfile"]
        .iter()
        .any(|marker| root.join(marker).is_file())
}

/// Detect Perl test directories: `t/` and `t2/`.
fn detect_perl_test_roots(root: &Path) -> Vec<&'static str> {
    let mut roots = Vec::new();
    if root.join("t").is_dir() {
        roots.push("t/");
    }
    if root.join("t2").is_dir() {
        roots.push("t2/");
    }
    roots
}

/// Detect Perl test frameworks from .t files in t/ and t2/ (Test::More,
/// Test2::V0/V1/Suite, Test::Exception, Test::Fatal). Empty means none were
/// found; doctor never names a framework it did not read.
fn detect_perl_frameworks(root: &Path) -> Vec<&'static str> {
    let mut found: Vec<&'static str> = Vec::new();
    let mut contents: Vec<String> = Vec::new();
    for dir in ["t", "t2"] {
        let test_dir = root.join(dir);
        let Ok(entries) = std::fs::read_dir(&test_dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "t")
                && let Ok(content) = std::fs::read_to_string(&path)
            {
                contents.push(content);
            }
        }
    }
    let blob = contents.join("\n");
    if blob.contains("use Test2::V1") || blob.contains("use Test2::Bundle::More") {
        found.push("Test2::V1");
    }
    if blob.contains("use Test2::V0") || blob.contains("use Test2::Tools::Basic") {
        found.push("Test2::V0");
    }
    if blob.contains("use Test2::Suite") {
        found.push("Test2::Suite");
    }
    if blob.contains("use Test::More") {
        found.push("Test::More");
    }
    if blob.contains("use Test::Exception") {
        found.push("Test::Exception");
    }
    if blob.contains("use Test::Fatal") {
        found.push("Test::Fatal");
    }
    found
}

/// Choose the exact next command based on adapter availability, producer
/// configuration, and whether a COMPATIBLE exporter was found.
///
/// An uncompiled adapter comes first: every other recommendation (a
/// `ripr.toml` edit, `--perl-facts`) fails against this binary, so the only
/// honest next step is the shared prerequisite text.
fn perl_next_command(
    adapter_compiled: bool,
    producer_configured: Option<&str>,
    compatible_exporter: Option<&str>,
) -> String {
    if !adapter_compiled {
        return LanguageId::Perl.unavailable_adapter_recovery();
    }
    let managed = producer_configured.is_some_and(crate::app::is_managed_perl_producer);
    if managed && compatible_exporter.is_some() {
        // Managed mode + compatible producer present: ripr invokes the
        // exporter itself. There is no --languages flag (#2105): perl is
        // enabled through config, and check then runs the enabled set.
        // Name the additive edit, not a replacement list, so a user with
        // TypeScript/Python already enabled keeps them (#2105 review).
        "add \"perl\" to [languages] enabled in ripr.toml, then: ripr check".to_string()
    } else if managed {
        // Managed mode configured but no compatible producer.
        format!(
            "install a compatible Perl fact exporter (`{}`, not yet published) on PATH, or set [perl].executable and {}=1, and add \"perl\" to [languages] enabled in ripr.toml, then: ripr check",
            crate::domain::PERL_FACT_EXPORTER,
            crate::config::PERL_EXECUTABLE_OPT_IN_ENV
        )
    } else {
        // Explicit packet mode (or producer absent): supply --perl-facts.
        "ripr check --perl-facts <packet.json> --diff <diff.patch> --json".to_string()
    }
}

/// Print static limitation notes for the doctor first-run diagnosis.
///
/// Every statement is conservative: no claim is made beyond what the static
/// analysis layer can actually determine. Wording sources:
///   - `language.rs` doc comment: TypeScript/JavaScript/Python/Perl are
///     preview surfaces.
///   - `StaticLimitKind::CrossLanguageOracleVisibilityUnresolved` wire string
///     and its doc comment.
///   - 0.9.0 CHANGELOG non-claims.
fn known_limitation_lines() -> Vec<String> {
    vec![
        "- Known limitations:".to_string(),
        "  TypeScript/JavaScript/Bun analysis is preview (advisory only); \
        not stable support — findings are additive, not gating"
            .to_string(),
        "  Cross-language oracle visibility is fail-closed: an FFI/binding seam tested \
        from another language reads as cross_language_oracle_visibility_unresolved, \
        not a Rust gap — verify the external oracle directly"
            .to_string(),
        format!(
            "  Full-repo repo-exposure analysis applies a default cap of {} seams; \
        set RIPR_REPO_EXPOSURE_SEAM_LIMIT=0 to analyze all seams.",
            analysis::DEFAULT_REPO_EXPOSURE_SEAM_LIMIT
        ),
        "  Preview-language evidence is advisory and does not block by default; \
        it yields repair cards or packets only for findings that satisfy the full \
        actionability, edit, verify, and receipt contract"
            .to_string(),
    ]
}

/// The typed evidence on whether git ignore rules cover ripr's own artifact
/// root under the doctor root (#7663). Standard Python `.gitignore`
/// templates cover `__pycache__/` and `.pytest_cache/` but never
/// Rust-conventional `target/`, so a routine `git add -A` commits ripr's
/// own reports and caches into the user's history; the next `ripr check`
/// then diffs them. `git check-ignore --no-index` applies the full ignore
/// semantics (nested `.gitignore` files, `.git/info/exclude`, global
/// excludes) without letting already-tracked descendants veto the answer,
/// so it — not a line-by-line reimplementation — decides coverage. Exit 1
/// means the path is positively not ignored; exit 0 means covered; any
/// other outcome (not a repository, git unavailable, deadline) keeps
/// doctor honest by reporting `NotChecked` instead of claiming a checked
/// fact it could not establish. Tracked descendants are a separate fact
/// the ignore verdict deliberately does not fold in (#8257 review
/// follow-up).
fn ripr_artifact_ignore(root: &Path) -> output::doctor::DoctorArtifactIgnore {
    use output::doctor::DoctorArtifactIgnore;
    let artifact_root = root.join("target").join("ripr");
    if !artifact_root.is_dir() {
        return DoctorArtifactIgnore::NoArtifacts;
    }
    // `--no-index` asks only whether ignore RULES cover the path. Without it,
    // a tracked descendant (the exact state #7663 addresses: ripr reports
    // already committed into history) makes check-ignore answer "not
    // ignored" even when `/target/ripr/` is configured, so doctor would
    // demand a rule that already exists.
    let probe = match crate::git::run_git_output_with_deadline(
        root,
        &["check-ignore", "--no-index", "-q", "--", "target/ripr"],
        Some(std::time::Duration::from_secs(5)),
    ) {
        Ok(probe) => probe,
        Err(_) => return DoctorArtifactIgnore::NotChecked,
    };
    match probe.status.code() {
        Some(0) => DoctorArtifactIgnore::Ignored,
        Some(1) => DoctorArtifactIgnore::Unignored {
            warning: "! ripr artifacts under target/ripr are not covered by git ignore rules; \
                      `git add -A` would commit ripr's own reports into your history. \
                      Add this line to .gitignore: /target/ripr/"
                .to_string(),
        },
        _ => DoctorArtifactIgnore::NotChecked,
    }
}

/// The typed evidence on whether git's index tracks files under ripr's
/// artifact root (#8276). An ignore rule never untracks files committed
/// earlier, so a repository can cover `/target/ripr/` in `.gitignore` while
/// `git add -A` keeps refreshing tracked reports and `ripr check` keeps
/// diffing them. Only a positive non-empty `git ls-files --cached` answer
/// warns; a missing repository, git failure, deadline, or output over the
/// cap reports `NotChecked` rather than a fact doctor did not establish.
fn ripr_artifact_tracking(root: &Path) -> output::doctor::DoctorArtifactTracking {
    use output::doctor::DoctorArtifactTracking;
    // The index can name tracked files whose directory was deleted from the
    // worktree, so this probe does not depend on `target/ripr` existing.
    let probe = match crate::git::run_git_output_with_deadline_and_limit(
        root,
        &["ls-files", "--cached", "-z", "--", "target/ripr"],
        std::time::Duration::from_secs(5),
        1024 * 1024,
    ) {
        Ok(probe) => probe,
        Err(_) => return DoctorArtifactTracking::NotChecked,
    };
    if !probe.status.success() {
        return DoctorArtifactTracking::NotChecked;
    }
    if probe.stdout.is_empty() {
        return DoctorArtifactTracking::Untracked;
    }
    DoctorArtifactTracking::Tracked {
        warning: "! ripr artifacts under target/ripr are tracked by git; `git add -A` keeps \
                  refreshing them and `ripr check` diffs them. Untrack them with: \
                  git rm --cached -r -- target/ripr"
            .to_string(),
    }
}

/// Why a cache entry could not be created under `cache_dir`, or `None` when
/// one could. For every directory a production cache writes entries into,
/// probes the nearest existing ancestor with a short-lived
/// exclusive file, so doctor never creates the cache or any layer itself. A
/// regular file anywhere on one of those paths is reported by name.
fn cache_unwritable_reason(cache_dir: &Path) -> Option<String> {
    // Only the entry directories matter: their nearest existing ancestor is
    // the base itself whenever the base is missing, blocked or still empty,
    // and a read-only base whose entry directories already exist and accept
    // writes does not stop caching.
    let mut probed: Vec<(PathBuf, bool)> = Vec::new();
    for (target, makes_subdirs) in &analysis::seam_cache::production_entry_dirs(cache_dir) {
        let probe_dir = match nearest_existing_dir(target) {
            Ok(Some(dir)) => dir,
            Ok(None) => continue,
            Err(reason) => return Some(reason),
        };
        // A missing entry directory is created by the producer first, and a
        // sharded cache creates generation subdirectories in it; an ACL
        // (Windows) can allow adding files but deny adding directories.
        let needs_dir = *makes_subdirs || probe_dir != *target;
        if probed
            .iter()
            .any(|(dir, dir_probed)| *dir == probe_dir && (*dir_probed || !needs_dir))
        {
            continue;
        }
        if let Some(reason) = probe_writable(&probe_dir) {
            return Some(reason);
        }
        if needs_dir && let Some(reason) = probe_creatable_dir(&probe_dir) {
            return Some(reason);
        }
        probed.push((probe_dir, needs_dir));
    }
    None
}

/// Creates and removes a short-lived subdirectory, the operation a producer
/// needs before it can write under a missing entry directory.
fn probe_creatable_dir(probe_dir: &Path) -> Option<String> {
    let probe = probe_dir.join(probe_name("dir"));
    match std::fs::create_dir(&probe) {
        Ok(()) => {
            let _ = std::fs::remove_dir(&probe);
            None
        }
        Err(error) => Some(format!(
            "cannot create a directory in {}: {error}",
            probe_dir.display()
        )),
    }
}

fn probe_name(kind: &str) -> String {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0);
    format!(".ripr-doctor-probe-{kind}-{}-{nonce}", std::process::id())
}

/// The nearest existing directory at or above `path`; an error naming the
/// component that blocks it (a file, a dangling symlink, an unreadable
/// entry). Only a missing component walks up to its parent.
fn nearest_existing_dir(path: &Path) -> Result<Option<PathBuf>, String> {
    let mut probe_dir = path;
    loop {
        match std::fs::metadata(probe_dir) {
            Ok(metadata) if metadata.is_dir() => return Ok(Some(probe_dir.to_path_buf())),
            Ok(_) => return Err(format!("{} is not a directory", probe_dir.display())),
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound
                    && std::fs::symlink_metadata(probe_dir).is_ok() =>
            {
                return Err(format!("{} is a dangling symlink", probe_dir.display()));
            }
            Err(error)
                if !matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                ) =>
            {
                return Err(format!("cannot inspect {}: {error}", probe_dir.display()));
            }
            Err(_) => match probe_dir.parent() {
                Some(parent) if parent.as_os_str().is_empty() => {
                    probe_dir = Path::new(".");
                }
                Some(parent) => probe_dir = parent,
                None => return Ok(None),
            },
        }
    }
}

/// Creates and removes one exclusive probe file in `dir`; the error when
/// that fails.
fn probe_writable(probe_dir: &Path) -> Option<String> {
    let probe = probe_dir.join(probe_name("file"));
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
    {
        Ok(file) => {
            drop(file);
            let _ = std::fs::remove_file(&probe);
            None
        }
        Err(error) => Some(format!("cannot write in {}: {error}", probe_dir.display())),
    }
}

const GENERATED_WORKFLOW_PATH: &str = ".github/workflows/ripr.yml";

/// Largest generated workflow doctor reads. The template is a few KiB; a
/// bigger file is not one `ripr init` wrote.
const GENERATED_WORKFLOW_MAX_BYTES: u64 = 1024 * 1024;

/// Flags a `ripr init --ci github` workflow generated by another ripr
/// version (#4738). The 0.10-and-earlier template installed ripr unpinned,
/// so after a release CI ran the new binary against the old steps; later
/// templates pin the generating version. Advisory only: a stale template is
/// not a failed check. It recognizes the template's own `cargo install ripr`
/// step, not every way a hand-written workflow could install ripr.
fn generated_workflow_advisory(root: &Path) -> Option<String> {
    let path = root.join(GENERATED_WORKFLOW_PATH);
    // A repository can commit this path as a symlink (to `/dev/zero`, say);
    // `ripr init` only ever writes a regular file, so read nothing else.
    if !std::fs::symlink_metadata(&path).is_ok_and(|meta| meta.is_file()) {
        return None;
    }
    let workflow =
        crate::bounded_input::read_to_string_with_limit(&path, GENERATED_WORKFLOW_MAX_BYTES)
            .ok()?;
    generated_workflow_line(&workflow, env!("CARGO_PKG_VERSION"))
}

fn generated_workflow_line(workflow: &str, current_version: &str) -> Option<String> {
    let install = workflow.lines().find_map(|line| {
        let command = line.trim().trim_start_matches("run:").trim();
        let rest = command.strip_prefix("cargo install ripr")?;
        (rest.is_empty() || rest.starts_with(char::is_whitespace)).then_some(rest)
    })?;
    let words: Vec<&str> = install.split_whitespace().collect();
    let pinned = words
        .windows(2)
        .find(|pair| pair[0] == "--version")
        .map(|pair| pair[1])
        .or_else(|| {
            words
                .iter()
                .find_map(|word| word.strip_prefix("--version="))
        });
    let refresh =
        "refresh it with `ripr init --ci github --force` and review the diff before committing";
    match pinned {
        None => Some(format!(
            "{GENERATED_WORKFLOW_PATH} installs ripr without a version (the ripr 0.10-and-earlier template), so CI runs whatever release is newest against these steps; {refresh}"
        )),
        Some(version) if version.trim_start_matches('=') != current_version => {
            let pinned = version.trim_start_matches('=');
            // #5208: an unreleased ripr intentionally pins the latest
            // release instead of itself. Refreshing would rewrite the
            // identical pin, so name the fallback and its real repair
            // (upgrade ripr first) instead of prescribing the loop.
            if pinned == super::init_workflow::workflow_install_version(current_version) {
                Some(format!(
                    "{GENERATED_WORKFLOW_PATH} installs ripr {pinned}, the latest-release fallback pin for this unreleased ripr {current_version} (refreshing now would rewrite the same pin); upgrade ripr to a release, then {refresh}"
                ))
            } else {
                Some(format!(
                    "{GENERATED_WORKFLOW_PATH} installs ripr {version}, but this is ripr {current_version}; {refresh}"
                ))
            }
        }
        Some(_) => None,
    }
}

/// Workspace-relative Cargo config paths Cargo reads `[env]` from, in Cargo's
/// own precedence order: the extensionless name wins when both exist ("If
/// both files exist, Cargo will use the file without the extension", Cargo
/// reference), so the first existing regular file here is the config a build
/// against this root uses.
const CARGO_CONFIG_PATHS: [&str; 2] = [".cargo/config", ".cargo/config.toml"];

/// The variables MSVC `link.exe` and other native linkers take their temp
/// directory from.
const LINKER_TEMP_VARS: [&str; 3] = ["TEMP", "TMP", "TMPDIR"];

/// Largest Cargo config doctor reads. A real config is a few hundred bytes;
/// a bigger file is not a config this preflight can vouch for.
const CARGO_CONFIG_MAX_BYTES: u64 = 1024 * 1024;

/// The `[env]` linker-temp redirect a workspace's Cargo config declares: the
/// variables it touches, the distinct workspace-relative directories it
/// targets, and whether every entry is `force = true`.
#[derive(Debug, Eq, PartialEq)]
struct LinkerTempRedirect {
    vars: Vec<&'static str>,
    dirs: Vec<String>,
    all_forced: bool,
}

/// Whether an `[env]` entry's `relative = true` value stays inside the
/// workspace. Cargo joins such a value to the config's parent directory, but
/// an absolute value replaces it (on Windows a root-only `/x` does too) and
/// a `..` component escapes it — those are not workspace-relative
/// directories, so the preflight must not label or probe them as ones it
/// owns.
fn is_workspace_relative_value(dir: &str) -> bool {
    let path = Path::new(dir);
    !path.is_absolute()
        && !path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
}

/// Parse the `[env]` linker-temp redirect out of a Cargo config's text. Only
/// table entries with `relative = true` whose value stays inside the
/// workspace redirect into a workspace directory; plain string values,
/// absolute or parent-escaping entries and other variables do not, so they
/// are not reported — an over-broad match would warn on workspaces the
/// failure cannot hit, and a missing directory is only a defect while it is
/// the linker's temp directory.
fn linker_temp_redirect(config_text: &str) -> Option<LinkerTempRedirect> {
    let config = toml::from_str::<toml::Value>(config_text).ok()?;
    let env = config.get("env")?.as_table()?;
    let mut redirect = LinkerTempRedirect {
        vars: Vec::new(),
        dirs: Vec::new(),
        all_forced: true,
    };
    for var in LINKER_TEMP_VARS {
        let Some(entry) = env.get(var) else {
            continue;
        };
        if !entry
            .get("relative")
            .and_then(|relative| relative.as_bool())
            .unwrap_or(false)
        {
            continue;
        }
        let Some(dir) = entry.get("value").and_then(|value| value.as_str()) else {
            continue;
        };
        if !is_workspace_relative_value(dir) {
            continue;
        }
        if !redirect.vars.contains(&var) {
            redirect.vars.push(var);
        }
        if !redirect.dirs.iter().any(|known| known == dir) {
            redirect.dirs.push(dir.to_string());
        }
        if !entry
            .get("force")
            .and_then(|force| force.as_bool())
            .unwrap_or(false)
        {
            redirect.all_forced = false;
        }
    }
    (!redirect.vars.is_empty()).then_some(redirect)
}

/// Read the workspace's own Cargo config, in Cargo's precedence order (the
/// extensionless `.cargo/config` wins when both names exist). Returns the
/// relative path for evidence and the text. A repository can commit these
/// paths as a symlink (to `/dev/zero`, say); read nothing but a regular
/// file, and an unreadable or oversized config stays silent — the preflight
/// never fires on evidence it could not read.
fn read_workspace_cargo_config(root: &Path) -> Option<(&'static str, String)> {
    for relative in CARGO_CONFIG_PATHS {
        let path = root.join(relative);
        if !std::fs::symlink_metadata(&path).is_ok_and(|meta| meta.is_file()) {
            continue;
        }
        return crate::bounded_input::read_to_string_with_limit(&path, CARGO_CONFIG_MAX_BYTES)
            .ok()
            .map(|text| (relative, text));
    }
    None
}

/// Preflight for the workspace-relative linker-temp redirect (#5280).
///
/// ripr's own `.cargo/config.toml` force-sets `TEMP`/`TMP`/`TMPDIR` to
/// workspace-relative `target/` so MSVC `link.exe` never writes its temp
/// files to a full system temp drive (PR #397). The redirect is deliberate
/// and stays; the cost is that a checkout where the workspace-local
/// directory does not exist — a fresh `git worktree add` building with an
/// isolated `CARGO_TARGET_DIR` — fails linking with an opaque
/// `LNK1104 ... <workspace>\target\lnk{GUID}.tmp` even though objects and
/// `/OUT` land in the isolated target. This surfaces the constraint and the
/// one-line repair before the build hits it. Advisory only: a normal
/// workspace build creates `target/` itself, and a present directory means
/// the failure cannot occur.
fn linker_temp_redirect_advisory(root: &Path) -> Option<String> {
    let (relative_path, config_text) = read_workspace_cargo_config(root)?;
    let redirect = linker_temp_redirect(&config_text)?;
    let absent: Vec<&str> = redirect
        .dirs
        .iter()
        .map(|dir| dir.as_str())
        .filter(|dir| !root.join(dir).is_dir())
        .collect();
    if absent.is_empty() {
        return None;
    }
    // Grammar and repair follow the count. The repair names the workspace-
    // rooted path, not a bare relative name, so running it from a different
    // working directory than the selected root still creates the directory
    // cargo will actually use; it is quoted through the shared shell-arg
    // owner so a root containing spaces survives as one argument, and the
    // plural repair names every absent directory.
    let repair_paths = absent
        .iter()
        .map(|dir| {
            crate::agent::loop_commands::shell_arg(&output::path::human_path(&root.join(dir)))
        })
        .collect::<Vec<_>>()
        .join(" ");
    let (dirs_phrase, existence, persistence, repair) = if absent.len() == 1 {
        (
            format!("{}/", absent[0]),
            "does not exist",
            "it exists",
            format!("Create it before building from source (for example `mkdir {repair_paths}`)"),
        )
    } else {
        let dirs_list = absent
            .iter()
            .map(|dir| format!("{dir}/"))
            .collect::<Vec<_>>()
            .join(", ");
        (
            dirs_list,
            "do not exist",
            "they exist",
            format!("Create each before building from source (for example `mkdir {repair_paths}`)"),
        )
    };
    let override_note = if redirect.all_forced {
        "exported TEMP/TMP cannot override it (force = true)"
    } else {
        "an exported TEMP/TMP overrides entries without force = true"
    };
    Some(format!(
        "{relative_path} redirects linker temp variables ({}) into workspace-relative \
         {dirs_phrase}, which {existence}; building with an isolated CARGO_TARGET_DIR fails \
         MSVC linking (LNK1104 naming {}/lnk*.tmp) until {persistence}. {repair}; \
         {override_note}",
        redirect.vars.join(", "),
        output::path::human_path(&root.join(absent[0])),
    ))
}

/// Recursively sum file sizes under `dir`. Returns 0 when the directory
/// does not exist or cannot be read — cache absence is not a problem.
fn dir_size_bytes(dir: &Path) -> u64 {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return 0,
    };
    let mut total: u64 = 0;
    for entry in entries.flatten() {
        if is_walkable_dir(&entry) {
            total = total.saturating_add(dir_size_bytes(&entry.path()));
        } else if let Ok(meta) = entry.metadata() {
            total = total.saturating_add(meta.len());
        }
    }
    total
}

/// Format a byte count in human-readable form (B, KB, MB, GB).
fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1_024;
    const MB: u64 = 1_024 * KB;
    const GB: u64 = 1_024 * MB;
    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{args, unique_command_test_dir};
    use super::*;

    fn quiet_binary() -> output::doctor_binary::DoctorBinaryIdentity {
        output::doctor_binary::DoctorBinaryIdentity {
            version: "ripr 0.11.0".to_string(),
            commit: None,
            commit_dirty: false,
            executable: None,
            executable_is_cargo_build_output: false,
            path_ripr: None,
            path_ripr_is_cargo_build_output: false,
            path_ripr_is_running_executable: None,
            warnings: Vec::new(),
        }
    }

    fn empty_environment() -> DoctorEnvironment {
        DoctorEnvironment {
            facts: output::doctor::DoctorEnvironmentFacts {
                detected_languages: Vec::new(),
                unanalyzed_source_languages: Vec::new(),
                preview_language_gaps: Vec::new(),
                config_defaults: Some(output::doctor::DoctorConfigDefaults {
                    source_path: None,
                    analysis_mode: "draft".to_string(),
                    lsp_seam_diagnostics: true,
                    suppressions_path: ".ripr/suppressions.toml".to_string(),
                    bun_ub_profile_configured: false,
                    bun_ub_test_roots: Vec::new(),
                    bun_ub_bridge_hints: None,
                }),
                cache: output::doctor::DoctorCacheStatus {
                    cache_dir: "target/ripr/cache".to_string(),
                    relocated_by_env: false,
                    size_bytes: 0,
                    size_display: "0 B".to_string(),
                    artifact_ignore: output::doctor::DoctorArtifactIgnore::Ignored,
                    artifact_tracking: output::doctor::DoctorArtifactTracking::Untracked,
                },
                test_surfaces: Vec::new(),
                perl_preview: None,
            },
            preview_enablement: None,
        }
    }

    fn passing_report() -> output::doctor::DoctorReport {
        let mut report = output::doctor::DoctorReport::new(".");
        report.add_check(
            "root_directory",
            output::doctor::DoctorStatus::Pass,
            Some("root directory exists at .".to_string()),
        );
        report.add_check(
            "cargo_toml",
            output::doctor::DoctorStatus::Pass,
            Some("Cargo.toml found at Cargo.toml".to_string()),
        );
        report.add_check(
            "git_repository",
            output::doctor::DoctorStatus::Pass,
            Some("inside a Git work tree at .".to_string()),
        );
        for tool in ["git", "cargo", "rustc"] {
            report.add_check(
                &format!("tool_{tool}"),
                output::doctor::DoctorStatus::Pass,
                Some(format!("{tool} 1.0")),
            );
        }
        report
    }

    fn render_screen(
        report: &output::doctor::DoctorReport,
        environment: &DoctorEnvironment,
        verbose: bool,
        passed: bool,
    ) -> Vec<String> {
        render_screen_with_profile(
            report,
            environment,
            output::doctor::DoctorProfile::Analysis,
            &Ok(RiprConfig::default()),
            verbose,
            passed,
        )
    }

    fn render_screen_with_profile(
        report: &output::doctor::DoctorReport,
        environment: &DoctorEnvironment,
        profile: output::doctor::DoctorProfile,
        config: &Result<RiprConfig, String>,
        verbose: bool,
        passed: bool,
    ) -> Vec<String> {
        let binary = quiet_binary();
        render_doctor_human_screen(DoctorHumanScreen {
            root: Path::new("."),
            profile,
            report,
            config,
            environment,
            binary: &binary,
            verbose,
            passed,
        })
    }

    #[test]
    fn failure_block_keeps_the_bang_line_and_adds_a_fix_line() {
        let lines = failure_block(
            "no Cargo.toml found at .; pass `--root <dir>` to target a crate or run `cargo init` in the selected root to create a package",
        );
        assert!(
            lines[0].starts_with('!') && lines[0].contains("no Cargo.toml found at ."),
            "{lines:?}"
        );
        assert_eq!(
            lines[1],
            "  Fix: pass `--root <dir>` to target a crate or run `cargo init` in the selected root to create a package"
        );
    }

    fn printed_line_count(lines: &[String]) -> usize {
        lines.iter().map(|line| line.lines().count().max(1)).sum()
    }

    fn assert_fits_one_screen(lines: &[String]) {
        let physical = printed_line_count(lines);
        assert!(
            physical <= DOCTOR_HUMAN_SCREEN_BUDGET,
            "compact doctor must fit 80x24 ({physical} physical lines):\n{}",
            lines.join("\n")
        );
        assert!(
            lines
                .iter()
                .all(|line| !line.contains('\n') && !line.contains('\r')),
            "printer entries must stay single-line:\n{}",
            lines.join("\n")
        );
    }

    fn version_failed_fix_is_not_install(lines: &[String]) {
        let fixes: Vec<&String> = lines
            .iter()
            .filter(|line| line.starts_with("  Fix:"))
            .collect();
        assert!(
            !fixes.is_empty(),
            "expected at least one Fix line:\n{}",
            lines.join("\n")
        );
        for fix in fixes {
            let lower = fix.to_ascii_lowercase();
            assert!(
                !lower.contains("install") && !lower.contains("rustup"),
                "nonzero `--version` is not a missing-tool install: {fix}"
            );
            assert!(
                fix.contains("repair or replace"),
                "nonzero `--version` must name a PATH-binary repair: {fix}"
            );
        }
    }

    #[test]
    fn nonzero_tool_probe_gets_a_fix_without_install() {
        let lines = failure_block("git --version failed (exit status: 127)");
        assert_eq!(lines[0], "! git --version failed (exit status: 127)");
        version_failed_fix_is_not_install(&lines);
        let rustc = failure_block(
            "rustc --version failed (exit status: 1): error: toolchain 1.81.0 is not installed",
        );
        assert!(
            rustc[0].contains("toolchain 1.81.0 is not installed"),
            "{rustc:?}"
        );
        version_failed_fix_is_not_install(&rustc);
        let timeout = failure_block("cargo timed out after 5s");
        assert_eq!(timeout[0], "! cargo timed out after 5s");
        assert!(
            timeout
                .get(1)
                .is_some_and(|line| line.starts_with("  Fix:") && line.contains("retry")),
            "timeout evidence has no `; ` clause: {timeout:?}"
        );
        let lower = timeout[1].to_ascii_lowercase();
        assert!(
            !lower.contains("install") && !lower.contains("rustup"),
            "timeout is not a missing-tool install: {timeout:?}"
        );
    }

    #[test]
    fn compact_tilde_line_spells_newlines() {
        let line = compact_tilde_line("target/\n! doctor checks passed");
        assert!(
            !line.contains('\n') && !line.contains('\r') && line.contains("\\u{0a}"),
            "{line}"
        );
        assert!(
            !line.contains("! doctor checks passed") || line.contains("\\u{0a}! doctor"),
            "a newline in advisory text must not forge a bang line: {line}"
        );
    }

    #[test]
    fn missing_git_spawn_still_names_install() {
        let lines = failure_block(crate::git::GIT_NOT_FOUND_ON_PATH_MESSAGE);
        assert!(
            lines[0].starts_with('!') && lines[0].contains("git was not found on PATH"),
            "{lines:?}"
        );
        assert!(
            lines
                .get(1)
                .is_some_and(|line| line.starts_with("  Fix:") && line.contains("install git")),
            "spawn-missing git keeps the install route: {lines:?}"
        );
    }

    #[test]
    fn compact_passing_screen_fits_one_terminal_and_hides_verbose_detail() {
        let report = passing_report();
        let environment = empty_environment();
        let lines = render_screen(&report, &environment, false, true);
        assert_fits_one_screen(&lines);
        let text = lines.join("\n");
        assert!(text.contains("✓ doctor checks passed"), "{text}");
        assert!(text.contains("Recommended first command:"), "{text}");
        assert!(!text.contains("Known limitations:"), "{text}");
        assert!(!text.contains("- Perl preview:"), "{text}");
        assert!(!text.contains("- ripr binary:"), "{text}");
        assert!(!text.contains("Analysis mode default:"), "{text}");
        assert!(!text.contains("Start-here packet:"), "{text}");
        assert!(!text.contains("Recovery states:"), "{text}");
        assert!(!text.contains("Proof rail:"), "{text}");
        assert!(!text.contains("Refresh scope:"), "{text}");
    }

    #[test]
    fn verbose_screen_restores_limitations_and_config_defaults() {
        let report = passing_report();
        let environment = empty_environment();
        let lines = render_screen(&report, &environment, true, true);
        let text = lines.join("\n");
        assert!(text.contains("Known limitations:"), "{text}");
        assert!(
            text.contains("cross_language_oracle_visibility_unresolved"),
            "{text}"
        );
        assert!(text.contains("- ripr binary:"), "{text}");
        assert!(text.contains("Analysis mode default: draft"), "{text}");
        assert!(text.contains("Start-here packet:"), "{text}");
        assert!(text.contains("Recovery states:"), "{text}");
        assert!(text.contains("Proof rail:"), "{text}");
    }

    #[test]
    fn failing_screen_puts_the_fix_before_passing_groups() {
        let mut report = passing_report();
        report.add_check(
            "git_repository",
            output::doctor::DoctorStatus::Fail,
            Some(
                "not inside a Git work tree at /tmp/scratch; the diff-scoped commands read committed \
                 history and cannot run here. For a repository-free scan, run `ripr check --root /tmp/scratch --format repo-exposure-md`"
                    .to_string(),
            ),
        );
        // The second git_repository check is the fail; drop the passing one.
        report.checks.retain(|check| {
            check.name != "git_repository"
                || check.status == output::doctor::DoctorCheckStatus::Fail
        });
        report.status = output::doctor::DoctorStatus::Fail;
        let environment = empty_environment();
        let lines = render_screen(&report, &environment, false, false);
        assert_fits_one_screen(&lines);
        let text = lines.join("\n");
        let fail_at = text
            .find("! not inside a Git work tree")
            .unwrap_or(usize::MAX);
        let fix_at = text.find("  Fix:").unwrap_or(usize::MAX);
        let pass_at = text.find("✓ Workspace:").unwrap_or(usize::MAX);
        assert!(
            fail_at < fix_at && fix_at < pass_at && pass_at != usize::MAX,
            "{text}"
        );
        assert!(text.contains("repo-exposure-md"), "{text}");
        assert!(!text.contains("Known limitations:"), "{text}");
        assert!(!text.contains("Start-here packet:"), "{text}");
        assert!(
            !text.contains("Safe next action: run the recommended first command below"),
            "{text}"
        );
    }

    #[test]
    fn required_runtime_failure_gets_a_fix_line() {
        let mut report = passing_report();
        report.add_runtime_probe(
            "python",
            "python3",
            output::doctor::DoctorStatus::Fail,
            "python3 not available",
            true,
            "install python3 (e.g. apt install python3)",
        );
        let environment = empty_environment();
        let lines = render_screen(&report, &environment, false, false);
        assert_fits_one_screen(&lines);
        let text = lines.join("\n");
        let fail_at = text
            .find("! python verify-route runtime: python3 not available")
            .unwrap_or(usize::MAX);
        let fix_at = text
            .find("  Fix: install python3 (e.g. apt install python3)")
            .unwrap_or(usize::MAX);
        let pass_at = text.find("✓ Workspace:").unwrap_or(usize::MAX);
        assert!(
            fail_at < fix_at && fix_at < pass_at && pass_at != usize::MAX,
            "{text}"
        );
        assert!(
            !text.contains("python3 not available — install python3"),
            "required runtime fail must not bury the fix on the bang line:\n{text}"
        );
        assert!(
            !text.contains("Recommended first command: ripr check"),
            "a required runtime failure must not recommend a check that hits the same gap:\n{text}"
        );
        assert!(
            text.contains("fix the `!` checks above, then rerun `ripr doctor`"),
            "{text}"
        );
    }

    #[test]
    fn cargo_toml_failure_suppresses_check() {
        let mut report = passing_report();
        report.add_check(
            "cargo_toml",
            output::doctor::DoctorStatus::Fail,
            Some(
                "no Cargo.toml found at .; pass `--root <dir>` to target a crate or run `cargo init` in the selected root to create a package"
                    .to_string(),
            ),
        );
        report.checks.retain(|check| {
            check.name != "cargo_toml" || check.status == output::doctor::DoctorCheckStatus::Fail
        });
        report.status = output::doctor::DoctorStatus::Fail;
        let environment = empty_environment();
        let lines = render_screen(&report, &environment, false, false);
        assert_fits_one_screen(&lines);
        let text = lines.join("\n");
        assert!(text.contains("! no Cargo.toml found at ."), "{text}");
        assert!(text.contains("  Fix: pass `--root <dir>`"), "{text}");
        assert!(
            !text.contains("Recommended first command: ripr check"),
            "a missing Cargo.toml must not recommend a check that will fail the same way:\n{text}"
        );
        assert!(
            text.contains("fix the `!` checks above, then rerun `ripr doctor`"),
            "{text}"
        );
    }

    fn perl_preview_with_exporter_on_path(on_path: bool) -> output::doctor::DoctorPerlPreview {
        output::doctor::DoctorPerlPreview {
            pm_files: 1,
            pl_files: 0,
            t_files: 0,
            adapter_compiled: true,
            producer: None,
            ignored_configured_executable: None,
            canonical_exporter_on_path: on_path,
            exporter: output::doctor::DoctorPerlExporter {
                state: output::doctor::DoctorPerlExporterState::NotFound,
                executable: None,
                version: None,
            },
            expected_schema: "1".to_string(),
            test_roots: Vec::new(),
            frameworks: Vec::new(),
            runners: Vec::new(),
            next_command: "next".to_string(),
        }
    }

    #[test]
    fn perl_detected_but_not_enabled_names_config_and_exporter_path_state() {
        let mut perl = detected_entry(LanguageId::Perl, true);
        perl.enabled = Some(false);
        let detected = [perl];

        let missing =
            perl_not_enabled_line(&detected, Some(&perl_preview_with_exporter_on_path(false)))
                .unwrap_or_default();
        assert!(missing.contains("detected but not enabled"), "{missing}");
        assert!(missing.contains("[languages]"), "{missing}");
        assert!(
            missing.contains("producer = \"perl-ripr-facts\""),
            "{missing}"
        );
        assert!(missing.contains("[perl]"), "{missing}");
        assert!(
            missing.contains("`perl-ripr-facts` is NOT on PATH"),
            "{missing}"
        );

        let found =
            perl_not_enabled_line(&detected, Some(&perl_preview_with_exporter_on_path(true)))
                .unwrap_or_default();
        assert!(found.contains("`perl-ripr-facts` is on PATH"), "{found}");
    }

    #[test]
    fn perl_not_enabled_line_stays_silent_when_it_would_mislead() {
        let preview = perl_preview_with_exporter_on_path(true);
        let mut enabled = detected_entry(LanguageId::Perl, true);
        enabled.enabled = Some(true);
        let mut unknown = detected_entry(LanguageId::Perl, true);
        unknown.enabled = None;
        let mut not_compiled = detected_entry(LanguageId::Perl, false);
        not_compiled.enabled = Some(false);
        let mut rust = detected_entry(LanguageId::Rust, true);
        rust.enabled = Some(false);
        for case in [enabled, unknown, not_compiled, rust] {
            assert_eq!(
                perl_not_enabled_line(std::slice::from_ref(&case), Some(&preview)),
                None,
                "{case:?}"
            );
        }
    }

    #[test]
    fn compact_screen_carries_perl_not_enabled_line() {
        let report = passing_report();
        let mut environment = empty_environment();
        let mut perl = detected_entry(LanguageId::Perl, true);
        perl.enabled = Some(false);
        environment.facts.detected_languages = vec![perl];
        environment.facts.perl_preview = Some(perl_preview_with_exporter_on_path(false));
        let text = render_screen(&report, &environment, false, true).join("\n");
        assert!(text.contains("Perl is detected but not enabled"), "{text}");
    }

    #[test]
    fn compact_discloses_ignored_perl_executable() {
        let report = passing_report();
        let mut environment = empty_environment();
        environment.facts.perl_preview = Some(output::doctor::DoctorPerlPreview {
            pm_files: 1,
            pl_files: 0,
            t_files: 0,
            adapter_compiled: false,
            producer: Some("perl-ripr-facts".to_string()),
            ignored_configured_executable: Some("sh".to_string()),
            canonical_exporter_on_path: false,
            exporter: output::doctor::DoctorPerlExporter {
                state: output::doctor::DoctorPerlExporterState::NotFound,
                executable: None,
                version: None,
            },
            expected_schema: "1".to_string(),
            test_roots: Vec::new(),
            frameworks: Vec::new(),
            runners: Vec::new(),
            next_command: "next".to_string(),
        });
        let compact = render_screen(&report, &environment, false, true);
        assert_fits_one_screen(&compact);
        let text = compact.join("\n");
        assert!(
            text.contains("ignoring [perl].executable `sh` from ripr.toml (not run)"),
            "compact doctor must keep the opt-in refusal visible:\n{text}"
        );
        assert!(
            !text.contains("- Perl preview:"),
            "full Perl preview stays behind --verbose:\n{text}"
        );
        let verbose = render_screen(&report, &environment, true, true).join("\n");
        assert!(
            verbose.contains("ignoring [perl].executable `sh` from ripr.toml (not run)"),
            "verbose must still name the refused executable:\n{verbose}"
        );
        assert!(
            verbose.contains("- Perl preview:"),
            "verbose restores the Perl preview dump:\n{verbose}"
        );
    }

    #[test]
    fn compact_perl_executable_disclosure_cannot_forge_lines() {
        let report = passing_report();
        let mut environment = empty_environment();
        environment.facts.perl_preview = Some(output::doctor::DoctorPerlPreview {
            pm_files: 1,
            pl_files: 0,
            t_files: 0,
            adapter_compiled: false,
            producer: Some("perl-ripr-facts".to_string()),
            ignored_configured_executable: Some("sh\n! doctor checks passed\u{1b}[2J".to_string()),
            canonical_exporter_on_path: false,
            exporter: output::doctor::DoctorPerlExporter {
                state: output::doctor::DoctorPerlExporterState::NotFound,
                executable: None,
                version: None,
            },
            expected_schema: "1".to_string(),
            test_roots: Vec::new(),
            frameworks: Vec::new(),
            runners: Vec::new(),
            next_command: "next".to_string(),
        });
        let compact = render_screen(&report, &environment, false, true);
        let text = compact.join("\n");
        assert!(
            text.contains("ignoring [perl].executable `sh\\u{0a}! doctor checks passed\\u{1b}[2J` from ripr.toml (not run)"),
            "control characters must be spelled, not injected:\n{text}"
        );
        assert!(
            !compact
                .iter()
                .any(|line| line.as_str() == "! doctor checks passed"),
            "a newline in the refused path must not forge a closing line:\n{text}"
        );
        assert!(
            !text.contains('\u{1b}'),
            "ESC must not reach the terminal:\n{text}"
        );
    }

    #[test]
    fn compact_newline_root_stays_one_physical_line_per_entry() {
        let nasty = format!("{}界", "line\n".repeat(8));
        let mut report = passing_report();
        for check in &mut report.checks {
            if let Some(evidence) = &mut check.evidence {
                *evidence = format!("{evidence} {nasty}");
            }
        }
        let mut environment = empty_environment();
        environment.facts.cache.cache_dir = format!("{nasty}/target/ripr/cache");
        let binary = quiet_binary();
        let lines = render_doctor_human_screen(DoctorHumanScreen {
            root: Path::new(&nasty),
            profile: output::doctor::DoctorProfile::Analysis,
            report: &report,
            config: &Ok(RiprConfig::default()),
            environment: &environment,
            binary: &binary,
            verbose: false,
            passed: true,
        });
        assert_fits_one_screen(&lines);
        assert!(
            lines.iter().any(|line| line.contains("\\u{0a}")),
            "newlines in the root must be spelled:\n{}",
            lines.join("\n")
        );
    }

    #[test]
    fn verbose_invalid_config_keeps_parser_reasons_without_source() {
        let err = match crate::config::tests_only_parse("mode = \"deep\"\n") {
            Err(err) => err,
            Ok(_) => "expected a parse error for top-level mode".to_string(),
        };
        let mut report = passing_report();
        report.add_check(
            "config",
            output::doctor::DoctorStatus::Fail,
            Some("invalid ripr.toml".to_string()),
        );
        report.status = output::doctor::DoctorStatus::Fail;
        let environment = empty_environment();
        let compact = render_screen_with_profile(
            &report,
            &environment,
            output::doctor::DoctorProfile::Analysis,
            &Err(err.clone()),
            false,
            false,
        )
        .join("\n");
        assert!(
            compact.contains("belongs under [analysis]"),
            "compact must keep the allowlisted hint:\n{compact}"
        );
        assert!(
            !compact.contains("mode = \"deep\""),
            "compact must not print the source assignment:\n{compact}"
        );
        let verbose = render_screen_with_profile(
            &report,
            &environment,
            output::doctor::DoctorProfile::Analysis,
            &Err(err),
            true,
            false,
        )
        .join("\n");
        assert!(
            verbose.contains("unknown field `mode`"),
            "verbose must keep the parser reason:\n{verbose}"
        );
        assert!(
            verbose.contains("belongs under [analysis]"),
            "verbose must keep the allowlisted hint:\n{verbose}"
        );
        assert!(
            !verbose.contains("mode = \"deep\""),
            "verbose must not restore the source excerpt:\n{verbose}"
        );
    }

    #[test]
    fn source_build_compact_reports_enabled_runtime_as_advisory() {
        let mut report = passing_report();
        report.profile = output::doctor::DoctorProfile::SourceBuild;
        report.add_runtime_probe(
            "typescript",
            "node",
            output::doctor::DoctorStatus::Fail,
            "node not available",
            false,
            "install Node.js",
        );
        let environment = empty_environment();
        let lines = render_screen_with_profile(
            &report,
            &environment,
            output::doctor::DoctorProfile::SourceBuild,
            &Ok(RiprConfig::default()),
            false,
            true,
        );
        assert_fits_one_screen(&lines);
        let text = lines.join("\n");
        assert!(
            text.contains("typescript verify-route runtime: node not available"),
            "SPEC-0007: source-build still reports enabled language runtimes:\n{text}"
        );
        assert!(
            text.contains("optional"),
            "source-build runtime failure must stay advisory:\n{text}"
        );
        assert!(
            !text.contains("! typescript verify-route runtime"),
            "source-build must not fail the profile on an enabled runtime:\n{text}"
        );
        assert!(text.contains("✓ doctor checks passed"), "{text}");
    }

    #[test]
    fn compact_hides_optional_non_primary_runtime() {
        let mut report = passing_report();
        report.add_runtime_probe(
            "typescript",
            "pnpm",
            output::doctor::DoctorStatus::Fail,
            "pnpm not available",
            false,
            "install pnpm",
        );
        let environment = empty_environment();
        let compact = render_screen(&report, &environment, false, true).join("\n");
        assert!(
            !compact.contains("pnpm"),
            "optional non-primary runtimes stay behind --verbose:\n{compact}"
        );
        let verbose = render_screen(&report, &environment, true, true).join("\n");
        assert!(
            verbose.contains("pnpm not available"),
            "verbose must restore optional runtime probes:\n{verbose}"
        );
    }

    #[test]
    fn invalid_config_compact_names_a_fix_and_suppresses_check() {
        let mut report = passing_report();
        report.add_check(
            "config",
            output::doctor::DoctorStatus::Fail,
            Some("invalid ripr.toml".to_string()),
        );
        report.status = output::doctor::DoctorStatus::Fail;
        let environment = empty_environment();
        let config = Err("TOML parse error at line 1".to_string());
        let compact = render_screen_with_profile(
            &report,
            &environment,
            output::doctor::DoctorProfile::Analysis,
            &config,
            false,
            false,
        );
        assert_fits_one_screen(&compact);
        let text = compact.join("\n");
        assert!(text.contains("! Config: invalid ripr.toml"), "{text}");
        assert!(
            text.contains("  Fix: correct") && text.contains("ripr.toml"),
            "{text}"
        );
        assert!(
            !text.contains("Recommended first command: ripr check"),
            "invalid config must not recommend a check that will hit the same error:\n{text}"
        );
        assert!(text.contains("correct `ripr.toml` so it loads"), "{text}");
        let verbose = render_screen_with_profile(
            &report,
            &environment,
            output::doctor::DoctorProfile::Analysis,
            &config,
            true,
            false,
        )
        .join("\n");
        assert!(
            !verbose.contains("Recommended first command: ripr check"),
            "verbose invalid config must not recommend ripr check:\n{verbose}"
        );
        assert!(
            verbose.contains("correct `ripr.toml` so it loads"),
            "{verbose}"
        );
    }

    #[test]
    fn compact_tool_version_failures_each_get_a_fix_and_fit() {
        let mut report = passing_report();
        for tool in ["git", "cargo", "rustc"] {
            let name = format!("tool_{tool}");
            report.add_check(
                &name,
                output::doctor::DoctorStatus::Fail,
                Some(format!("{tool} --version failed (exit status: 1)")),
            );
            report.checks.retain(|check| {
                check.name != name || check.status == output::doctor::DoctorCheckStatus::Fail
            });
        }
        report.status = output::doctor::DoctorStatus::Fail;
        let lines = render_screen(&report, &empty_environment(), false, false);
        assert_fits_one_screen(&lines);
        let text = lines.join("\n");
        for tool in ["git", "cargo", "rustc"] {
            assert!(
                text.contains(&format!("! {tool} --version failed (exit status: 1)")),
                "{text}"
            );
        }
        version_failed_fix_is_not_install(&lines);
        assert_eq!(
            lines
                .iter()
                .filter(|line| line.starts_with("  Fix:"))
                .count(),
            3,
            "each tool bang needs its own Fix:\n{text}"
        );
    }

    #[test]
    fn compact_ordinary_path_with_enablement_tip_fits_one_screen() {
        let mut report = passing_report();
        for tool in ["cargo", "rustc"] {
            let name = format!("tool_{tool}");
            report.add_advisory_check(
                &name,
                format!("{tool} not available; static analysis continues"),
            );
            report.checks.retain(|check| {
                check.name != name || check.status == output::doctor::DoctorCheckStatus::Advisory
            });
        }
        report.add_runtime_probe(
            "python",
            "python3",
            output::doctor::DoctorStatus::Fail,
            "python3 not available",
            true,
            "install python3 (e.g. apt install python3)",
        );
        report.status = output::doctor::DoctorStatus::Fail;
        let mut environment = empty_environment();
        if let Some(defaults) = environment.facts.config_defaults.as_mut() {
            defaults.source_path = Some("ripr.toml".to_string());
            defaults.analysis_mode = "deep".to_string();
        }
        environment.facts.detected_languages = vec![
            detected_entry(LanguageId::Rust, true),
            detected_entry(LanguageId::TypeScript, true),
            detected_entry(LanguageId::JavaScript, true),
        ];
        environment.facts.unanalyzed_source_languages = vec![unanalyzed_entry("Go", 3)];
        environment.facts.perl_preview = Some(output::doctor::DoctorPerlPreview {
            pm_files: 1,
            pl_files: 0,
            t_files: 0,
            adapter_compiled: false,
            producer: Some("perl-ripr-facts".to_string()),
            ignored_configured_executable: Some("sh".to_string()),
            canonical_exporter_on_path: false,
            exporter: output::doctor::DoctorPerlExporter {
                state: output::doctor::DoctorPerlExporterState::NotFound,
                executable: None,
                version: None,
            },
            expected_schema: "1".to_string(),
            test_roots: Vec::new(),
            frameworks: Vec::new(),
            runners: Vec::new(),
            next_command: "next".to_string(),
        });
        environment.preview_enablement = Some(PreviewEnablement {
            gaps: vec![
                PreviewLanguageGap {
                    entry: LanguageId::TypeScript,
                    label: "typescript".to_string(),
                },
                PreviewLanguageGap {
                    entry: LanguageId::TypeScript,
                    label: "javascript".to_string(),
                },
            ],
            suggestion: vec![LanguageId::Rust, LanguageId::TypeScript],
        });
        let lines = render_screen(&report, &environment, false, false);
        assert_fits_one_screen(&lines);
        let text = lines.join("\n");
        let tip = lines
            .iter()
            .find(|line| line.starts_with("- Tip:"))
            .map(String::as_str)
            .unwrap_or("");
        assert!(
            tip.starts_with("- Tip:")
                && !tip.contains('\n')
                && tip.contains("[languages]")
                && tip.contains("enabled =")
                && !tip.contains("[languages] enabled"),
            "enablement tip must be one physical line of valid TOML instructions:\n{text}"
        );
        assert!(
            tip.contains("the `typescript` entry also analyzes JavaScript"),
            "{tip}"
        );
        assert!(
            text.contains("! python verify-route runtime: python3 not available"),
            "{text}"
        );
        assert!(
            text.contains("  Fix: install python3 (e.g. apt install python3)"),
            "{text}"
        );
        assert!(text.contains("~ cargo not available"), "{text}");
        assert!(text.contains("~ rustc not available"), "{text}");
        assert!(text.contains("Config: loaded ripr.toml"), "{text}");
        assert!(text.contains("Analysis mode default: deep"), "{text}");
        assert!(
            text.contains("LSP seam diagnostics default: true"),
            "{text}"
        );
        assert!(
            !text.contains("Recommended first command: ripr check"),
            "a required runtime failure must not recommend a check:\n{text}"
        );
        assert!(text.contains("- Before that: enable typescript"), "{text}");
        assert!(
            !text.contains("To analyze them, set in ripr.toml:"),
            "the four-row TOML block must not return:\n{text}"
        );
    }

    #[test]
    fn cache_unwritable_reason_names_a_blocking_file_and_accepts_a_creatable_path()
    -> Result<(), String> {
        let root = unique_command_test_dir("cache-writable");
        std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
        let blocker = root.join("blocker");
        std::fs::write(&blocker, "file").map_err(|error| error.to_string())?;

        let reason = cache_unwritable_reason(&blocker.join("cache"));
        assert!(
            reason
                .as_deref()
                .is_some_and(|text| text.contains("is not a directory")),
            "{reason:?}"
        );
        // A missing cache under a writable parent is fine and is not created.
        let missing = root.join("not-yet").join("cache");
        assert_eq!(cache_unwritable_reason(&missing), None);
        assert!(!root.join("not-yet").exists());
        let probes = std::fs::read_dir(&root)
            .map_err(|error| error.to_string())?
            .flatten()
            .filter(|entry| entry.file_name().to_string_lossy().contains("doctor-probe"))
            .count();
        assert_eq!(probes, 0, "probe file must be removed");
        // An existing layer directory is probed too; a layer path that is a
        // regular file blocks every entry of that layer even though the
        // base itself is writable.
        let base = root.join("cache-base");
        std::fs::create_dir_all(base.join("repo-seam-facts")).map_err(|error| error.to_string())?;
        assert_eq!(cache_unwritable_reason(&base), None);
        assert!(
            !base.join("repo-file-facts").exists(),
            "missing layers must not be created"
        );
        std::fs::write(base.join("repo-file-facts"), "file").map_err(|error| error.to_string())?;
        let layer_reason = cache_unwritable_reason(&base);
        assert!(
            layer_reason.as_deref().is_some_and(
                |text| text.contains("repo-file-facts") && text.contains("is not a directory")
            ),
            "{layer_reason:?}"
        );
        // A regular file at a versioned entry directory blocks that cache
        // even though its layer directory is writable.
        std::fs::remove_file(base.join("repo-file-facts")).map_err(|error| error.to_string())?;
        std::fs::create_dir_all(base.join("repo-file-facts")).map_err(|error| error.to_string())?;
        std::fs::write(
            base.join("repo-file-facts")
                .join(analysis::seam_cache::FILE_FACT_CACHE_SCHEMA_VERSION),
            "file",
        )
        .map_err(|error| error.to_string())?;
        let version_reason = cache_unwritable_reason(&base);
        assert!(
            version_reason.as_deref().is_some_and(|text| text
                .contains(analysis::seam_cache::FILE_FACT_CACHE_SCHEMA_VERSION)
                && text.contains("is not a directory")),
            "{version_reason:?}"
        );
        // A relative cache path with no parent component probes the cwd.
        assert_eq!(
            cache_unwritable_reason(Path::new("ripr-cache-nonexistent")),
            None
        );
        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn ripr_artifact_ignore_names_the_gitignore_entry() -> Result<(), String> {
        use output::doctor::DoctorArtifactIgnore;
        let root = unique_command_test_dir("artifact-unignored");
        std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
        // No artifact root: nothing to warn about, and no git probe needed.
        assert_eq!(
            ripr_artifact_ignore(&root),
            DoctorArtifactIgnore::NoArtifacts
        );

        let artifact = root.join("target").join("ripr").join("pilot");
        std::fs::create_dir_all(&artifact).map_err(|error| error.to_string())?;
        // Establish the repository before asserting coverage: outside a
        // repository the probe's outcome depends on the machine (an
        // ancestor worktree or a global exclude file can answer for a
        // directory that is not itself a repo), so the NotChecked arm is
        // not pinned here.
        // Pin the repository-local excludes file to an empty file: a host
        // whose global `core.excludesFile` ignores `target/` would otherwise
        // make the Unignored arm below read as Ignored for a valid host
        // configuration (#8257 review).
        let empty_excludes = root.join("empty-excludes");
        std::fs::write(&empty_excludes, "").map_err(|error| error.to_string())?;
        for args in [
            ["init"].as_slice(),
            ["config", "user.email", "ripr@example.invalid"].as_slice(),
            ["config", "user.name", "ripr tests"].as_slice(),
            [
                "config",
                "core.excludesFile",
                empty_excludes.to_str().ok_or("non-utf8 excludes path")?,
            ]
            .as_slice(),
        ] {
            crate::git::run_git(&root, args).map_err(|error| error.to_string())?;
        }
        // A repository whose ignore rules do not cover target/ripr warns and
        // names the exact entry to add (#7663). The typed evidence is what
        // both the human line and the JSON report carry (#8257 review).
        match ripr_artifact_ignore(&root) {
            DoctorArtifactIgnore::Unignored { warning } => {
                assert!(warning.contains("/target/ripr/"), "{warning}");
            }
            other => return Err(format!("expected Unignored, got {other:?}")),
        }
        // The covering entry silences it.
        std::fs::write(root.join(".gitignore"), "/target/ripr/\n")
            .map_err(|error| error.to_string())?;
        assert_eq!(ripr_artifact_ignore(&root), DoctorArtifactIgnore::Ignored);
        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn ripr_artifact_tracking_names_the_untrack_command() -> Result<(), String> {
        use output::doctor::{DoctorArtifactIgnore, DoctorArtifactTracking};
        let root = unique_command_test_dir("artifact-tracked");
        std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
        // Outside a repository nothing is established.
        let empty_excludes = root.join("empty-excludes");
        std::fs::write(&empty_excludes, "").map_err(|error| error.to_string())?;
        for args in [
            ["init"].as_slice(),
            ["config", "user.email", "ripr@example.invalid"].as_slice(),
            ["config", "user.name", "ripr tests"].as_slice(),
            [
                "config",
                "core.excludesFile",
                empty_excludes.to_str().ok_or("non-utf8 excludes path")?,
            ]
            .as_slice(),
        ] {
            crate::git::run_git(&root, args).map_err(|error| error.to_string())?;
        }
        // A repository with an empty index is positively untracked.
        assert_eq!(
            ripr_artifact_tracking(&root),
            DoctorArtifactTracking::Untracked
        );

        let artifact = root.join("target").join("ripr").join("pilot");
        std::fs::create_dir_all(&artifact).map_err(|error| error.to_string())?;
        std::fs::write(artifact.join("report.json"), "{}").map_err(|error| error.to_string())?;
        std::fs::write(root.join(".gitignore"), "/target/ripr/\n")
            .map_err(|error| error.to_string())?;
        // Ignored but never added: the ignore rule is not tracking.
        assert_eq!(ripr_artifact_ignore(&root), DoctorArtifactIgnore::Ignored);
        assert_eq!(
            ripr_artifact_tracking(&root),
            DoctorArtifactTracking::Untracked
        );

        // Force-add past the ignore rule: the exact state of an earlier
        // commit that predates the rule. The ignore verdict stays Ignored.
        crate::git::run_git(&root, &["add", "-f", "--", "target/ripr/pilot/report.json"])
            .map_err(|error| error.to_string())?;
        assert_eq!(ripr_artifact_ignore(&root), DoctorArtifactIgnore::Ignored);
        match ripr_artifact_tracking(&root) {
            DoctorArtifactTracking::Tracked { warning } => {
                assert!(
                    warning.contains("git rm --cached -r -- target/ripr"),
                    "{warning}"
                );
            }
            other => return Err(format!("expected Tracked, got {other:?}")),
        }
        let _ = std::fs::remove_dir_all(&root);

        // A root git cannot even run in establishes nothing: the spawn
        // fails and doctor reports NotChecked rather than a verdict.
        assert_eq!(
            ripr_artifact_tracking(&root),
            DoctorArtifactTracking::NotChecked
        );
        Ok(())
    }

    #[test]
    fn generated_workflow_line_flags_unpinned_and_other_version_installs() {
        // The exact install step `ripr 0.10.0 init --ci github` wrote.
        let v010 = "      - name: Install ripr\n        run: cargo install ripr --locked\n";
        let pinned = "      - name: Install ripr\n        run: cargo install ripr --version 0.11.0 --locked\n";

        let unpinned = generated_workflow_line(v010, "0.11.0");
        assert!(
            unpinned
                .as_deref()
                .is_some_and(|line| line.contains("without a version")
                    && line.contains("ripr init --ci github --force")),
            "{unpinned:?}"
        );
        // #5208: keep this leg release-proof: the stale pin must differ
        // from both the current version and its computed fallback, or the
        // next release bump flips this case into the fallback arm.
        let current = "99.0.0";
        let fallback = crate::cli::commands::init_workflow::workflow_install_version(current);
        assert_ne!(
            fallback, current,
            "test setup: {current} must stay unreleased"
        );
        let stale = "98.0.0";
        assert_ne!(
            stale, fallback,
            "test setup: stale pin must not equal the fallback"
        );
        let stale_workflow = format!(
            "      - name: Install ripr\n        run: cargo install ripr --version {stale} --locked\n"
        );
        let older = generated_workflow_line(&stale_workflow, current);
        assert!(
            older.as_deref().is_some_and(|line| line.contains(&format!(
                "installs ripr {stale}, but this is ripr {current}"
            ))),
            "{older:?}"
        );
        assert_eq!(generated_workflow_line(pinned, "0.11.0"), None);
        assert_eq!(generated_workflow_line("jobs: {}\n", "0.11.0"), None);
        assert_eq!(
            generated_workflow_line("        run: cargo install ripr-tools --locked\n", "0.11.0"),
            None
        );
    }

    #[test]
    fn generated_workflow_line_names_the_fallback_pin_without_a_refresh_loop() {
        // #5208 (Codex P2): an unreleased ripr intentionally pins the latest
        // release. Doctor must recognize that pin and prescribe upgrading
        // ripr first: a bare "refresh now" would rewrite the identical pin.
        let current = "99.0.0";
        let fallback = crate::cli::commands::init_workflow::workflow_install_version(current);
        assert_ne!(
            fallback, current,
            "test setup: {current} must stay unreleased"
        );
        let workflow = format!(
            "      - name: Install ripr\n        run: cargo install ripr --version {fallback} --locked\n"
        );
        let line = generated_workflow_line(&workflow, current);
        assert!(
            line.as_deref().is_some_and(|line| {
                line.contains("latest-release fallback pin")
                    && line.contains("upgrade ripr to a release")
                    && line.contains("refreshing now would rewrite the same pin")
            }),
            "{line:?}"
        );
    }

    #[test]
    fn generated_workflow_advisory_reads_only_a_bounded_regular_file() -> Result<(), String> {
        let root = unique_command_test_dir("workflow-advisory");
        let workflows = root.join(".github/workflows");
        std::fs::create_dir_all(&workflows).map_err(|err| format!("create dir: {err}"))?;
        let path = root.join(GENERATED_WORKFLOW_PATH);
        std::fs::write(&path, "        run: cargo install ripr --locked\n")
            .map_err(|err| format!("write workflow: {err}"))?;
        let unpinned = generated_workflow_advisory(&root);
        let mut oversized = "        run: cargo install ripr --locked\n".to_string();
        oversized.push_str(&"#".repeat(GENERATED_WORKFLOW_MAX_BYTES as usize));
        std::fs::write(&path, oversized).map_err(|err| format!("write workflow: {err}"))?;
        let too_big = generated_workflow_advisory(&root);
        #[cfg(unix)]
        let through_link = {
            std::fs::remove_file(&path).map_err(|err| format!("remove workflow: {err}"))?;
            // A link to a real workflow outside the checkout is still not a
            // file `ripr init` wrote.
            let outside = root.join("outside.yml");
            std::fs::write(&outside, "        run: cargo install ripr --locked\n")
                .map_err(|err| format!("write outside: {err}"))?;
            std::os::unix::fs::symlink(&outside, &path).map_err(|err| format!("symlink: {err}"))?;
            generated_workflow_advisory(&root)
        };
        std::fs::remove_dir_all(&root).map_err(|err| format!("remove root: {err}"))?;

        assert!(
            unpinned.as_deref().is_some_and(|line| line
                .starts_with(".github/workflows/ripr.yml installs ripr without a version")),
            "{unpinned:?}"
        );
        assert_eq!(too_big, None);
        #[cfg(unix)]
        assert_eq!(through_link, None);
        Ok(())
    }

    /// The exact `[env]` shape ripr's own `.cargo/config.toml` ships.
    const REPO_SHAPED_CARGO_CONFIG: &str = "[env]\n\
        TEMP = { value = \"target\", relative = true, force = true }\n\
        TMP = { value = \"target\", relative = true, force = true }\n\
        TMPDIR = { value = \"target\", relative = true, force = true }\n";

    #[test]
    fn linker_temp_redirect_parses_only_relative_table_entries() {
        // The repo shape: three variables, one distinct directory, all forced.
        let redirect = linker_temp_redirect(REPO_SHAPED_CARGO_CONFIG);
        assert!(
            redirect.as_ref().is_some_and(|redirect| {
                redirect.vars == ["TEMP", "TMP", "TMPDIR"]
                    && redirect.dirs == ["target"]
                    && redirect.all_forced
            }),
            "ripr's own [env] shape must parse as a redirect: {redirect:?}"
        );

        // Plain-string entries (never tables, so never `relative`) do not
        // redirect into the workspace; unrelated variables are ignored; a
        // non-forced entry keeps all_forced honest for the override note.
        let mixed = "[env]\n\
            TEMP = \"elsewhere\"\n\
            TMPDIR = { value = \"target\", relative = true }\n";
        let redirect = linker_temp_redirect(mixed);
        assert!(
            redirect.as_ref().is_some_and(|redirect| {
                redirect.vars == ["TMPDIR"] && redirect.dirs == ["target"] && !redirect.all_forced
            }),
            "the relative TMPDIR entry must redirect: {redirect:?}"
        );

        // Absolute and parent-escaping values are not workspace-relative
        // directories, so they are not recorded at all — not as variables,
        // directories, or force summary — even when a valid entry coexists.
        let escaping = "[env]\n\
            TEMP = { value = \"/elsewhere\", relative = true }\n\
            TMP = { value = \"../outside\", relative = true, force = true }\n\
            TMPDIR = { value = \"target\", relative = true }\n";
        let redirect = linker_temp_redirect(escaping);
        assert!(
            redirect.as_ref().is_some_and(|redirect| {
                redirect.vars == ["TMPDIR"] && redirect.dirs == ["target"] && !redirect.all_forced
            }),
            "absolute and parent-escaping entries must be rejected, valid ones kept: {redirect:?}"
        );
        assert_eq!(
            linker_temp_redirect("[env]\nTEMP = { value = \"../outside\", relative = true }\n",),
            None
        );
        // Windows drive-relative values ("C:target") carry a Prefix component
        // without a root, and joining them discards the workspace base too.
        #[cfg(windows)]
        assert_eq!(
            linker_temp_redirect("[env]\nTEMP = { value = \"C:target\", relative = true }\n",),
            None
        );

        assert_eq!(linker_temp_redirect("[env]\nTEMP = \"target\"\n"), None);
        assert_eq!(linker_temp_redirect("[env]\nRUST_LOG = \"debug\"\n"), None);
        assert_eq!(linker_temp_redirect("[profile.dev]\nopt-level = 0\n"), None);
        assert_eq!(linker_temp_redirect("not toml ["), None);
    }

    #[test]
    fn linker_temp_redirect_advisory_names_the_missing_dir_and_repair() -> Result<(), String> {
        let root = unique_command_test_dir("linker-temp-redirect");
        std::fs::create_dir_all(root.join(".cargo"))
            .map_err(|err| format!("create .cargo: {err}"))?;
        let config = root.join(".cargo/config.toml");
        std::fs::write(&config, REPO_SHAPED_CARGO_CONFIG)
            .map_err(|err| format!("write config: {err}"))?;
        // The issue's setup (#5280): redirect present, workspace-local
        // target/ absent — a fresh worktree about to build with an isolated
        // CARGO_TARGET_DIR.
        let absent = linker_temp_redirect_advisory(&root);
        // Once the directory exists (a normal workspace build creates it
        // itself) the failure cannot occur and the preflight is silent.
        std::fs::create_dir_all(root.join("target"))
            .map_err(|err| format!("create target: {err}"))?;
        let present = linker_temp_redirect_advisory(&root);
        // No redirect, no warning: the config's presence alone is not the
        // defect, the redirect into a missing directory is.
        std::fs::write(&config, "[alias]\nxtask = \"run -p xtask --\"\n")
            .map_err(|err| format!("write plain config: {err}"))?;
        std::fs::remove_dir_all(root.join("target"))
            .map_err(|err| format!("remove target: {err}"))?;
        let no_redirect = linker_temp_redirect_advisory(&root);
        // The legacy extensionless name is the config Cargo reads when
        // config.toml does not exist.
        std::fs::remove_file(&config).map_err(|err| format!("remove config: {err}"))?;
        std::fs::write(root.join(".cargo/config"), REPO_SHAPED_CARGO_CONFIG)
            .map_err(|err| format!("write legacy config: {err}"))?;
        let legacy = linker_temp_redirect_advisory(&root);
        // Cargo's precedence: when both names exist, the extensionless file
        // wins, so the advisory must reflect its redirect, not config.toml's.
        std::fs::write(
            &config,
            "[env]\nTMP = { value = \"other\", relative = true }\n",
        )
        .map_err(|err| format!("write conflicting config.toml: {err}"))?;
        let conflicting = linker_temp_redirect_advisory(&root);
        // An unreadable (here: oversized) config stays silent: with the
        // extensionless file removed, the oversized config.toml is the first
        // existing name, and an unreadable first name is never traded for a
        // later readable one.
        std::fs::remove_file(root.join(".cargo/config"))
            .map_err(|err| format!("remove legacy config: {err}"))?;
        let mut oversized = REPO_SHAPED_CARGO_CONFIG.to_string();
        oversized.push_str(&"#".repeat(CARGO_CONFIG_MAX_BYTES as usize));
        std::fs::write(&config, oversized).map_err(|err| format!("write oversized: {err}"))?;
        let too_big = linker_temp_redirect_advisory(&root);
        // A root whose path contains a space must get a shell-quoted repair,
        // so the example stays one argument instead of splitting.
        let spaced_root = root.join("dir with spaces");
        std::fs::create_dir_all(spaced_root.join(".cargo"))
            .map_err(|err| format!("create spaced root: {err}"))?;
        std::fs::write(
            spaced_root.join(".cargo/config.toml"),
            REPO_SHAPED_CARGO_CONFIG,
        )
        .map_err(|err| format!("write spaced config: {err}"))?;
        let spaced = linker_temp_redirect_advisory(&spaced_root);
        // Compute the rooted-repair expectations before the tree is removed.
        let rooted_target = output::path::human_path(&root.join("target"));
        let spaced_target = crate::agent::loop_commands::shell_arg(&output::path::human_path(
            &spaced_root.join("target"),
        ));
        std::fs::remove_dir_all(&root).map_err(|err| format!("remove root: {err}"))?;

        let absent = absent.unwrap_or_default();
        assert!(
            absent.starts_with(".cargo/config.toml redirects linker temp variables"),
            "absent workspace target must warn, got: {absent:?}"
        );
        assert!(absent.contains("(TEMP, TMP, TMPDIR)"), "{absent}");
        assert!(absent.contains("workspace-relative target/"), "{absent}");
        assert!(absent.contains("which does not exist"), "{absent}");
        assert!(absent.contains("LNK1104"), "{absent}");
        assert!(
            absent.contains(&format!("mkdir {rooted_target}")),
            "the repair must name the workspace-rooted directory, not a cwd-relative one: {absent}"
        );
        let spaced = spaced.unwrap_or_default();
        assert!(
            spaced.contains(&format!("mkdir {spaced_target}")),
            "a root with spaces must get a quoted repair argument: {spaced}"
        );
        assert!(
            absent.contains("exported TEMP/TMP cannot override it (force = true)"),
            "{absent}"
        );
        assert_eq!(present, None);
        assert_eq!(no_redirect, None);
        let legacy = legacy.unwrap_or_default();
        assert!(
            legacy.starts_with(".cargo/config redirects linker temp variables"),
            "legacy extensionless config must be read, got: {legacy:?}"
        );
        let conflicting = conflicting.unwrap_or_default();
        assert!(
            conflicting.starts_with(".cargo/config redirects linker temp variables")
                && conflicting.contains("workspace-relative target/")
                && !conflicting.contains("other"),
            "when both config names exist the extensionless redirect must win: {conflicting:?}"
        );
        assert_eq!(too_big, None);
        Ok(())
    }

    #[test]
    fn linker_temp_redirect_advisory_pluralizes_multiple_absent_dirs() -> Result<(), String> {
        // Two variables redirected into two distinct absent directories:
        // the evidence must read as plural and the repair must name every
        // directory, not just the first.
        let config_text = "[env]\n\
            TEMP = { value = \"target\", relative = true, force = true }\n\
            TMPDIR = { value = \"elsewhere\", relative = true }\n";
        let redirect = linker_temp_redirect(config_text);
        assert!(
            redirect.as_ref().is_some_and(|redirect| {
                redirect.dirs == ["target", "elsewhere"] && !redirect.all_forced
            }),
            "distinct directories must both be recorded: {redirect:?}"
        );
        let root = unique_command_test_dir("linker-temp-redirect-multi");
        std::fs::create_dir_all(root.join(".cargo"))
            .map_err(|err| format!("create .cargo: {err}"))?;
        std::fs::write(root.join(".cargo/config.toml"), config_text)
            .map_err(|err| format!("write config: {err}"))?;
        let advisory = linker_temp_redirect_advisory(&root).unwrap_or_default();
        // Compute both rooted-repair expectations before the tree is removed.
        let target_arg =
            crate::agent::loop_commands::shell_arg(&output::path::human_path(&root.join("target")));
        let elsewhere_arg = crate::agent::loop_commands::shell_arg(&output::path::human_path(
            &root.join("elsewhere"),
        ));
        std::fs::remove_dir_all(&root).map_err(|err| format!("remove root: {err}"))?;
        assert!(
            advisory.contains("target/, elsewhere/, which do not exist"),
            "{advisory}"
        );
        assert!(
            advisory.contains(&format!("mkdir {target_arg} {elsewhere_arg}")),
            "the plural repair must name every absent directory: {advisory}"
        );
        assert!(advisory.contains("until they exist"), "{advisory}");
        assert!(
            advisory.contains("Create each before building"),
            "{advisory}"
        );
        Ok(())
    }

    #[test]
    #[cfg(all(feature = "lang-python", feature = "lang-typescript"))]
    fn language_runtime_probes_follow_detected_languages() -> Result<(), String> {
        // #2071: rust-only roots get no probes; a python root with pytest
        // markers gets python3 + pytest; a bun workspace adds bun.
        let root = unique_command_test_dir("probe-rust");
        std::fs::create_dir_all(&root).map_err(|err| format!("create root: {err}"))?;
        assert!(language_runtime_probes(&root).is_empty());
        std::fs::remove_dir_all(&root).map_err(|err| format!("remove root: {err}"))?;

        let root = unique_command_test_dir("probe-python");
        let tests_dir = root.join("tests");
        std::fs::create_dir_all(&tests_dir).map_err(|err| format!("create tests dir: {err}"))?;
        std::fs::write(tests_dir.join("test_x.py"), "import unittest\n")
            .map_err(|err| format!("write test: {err}"))?;
        std::fs::write(root.join("conftest.py"), "")
            .map_err(|err| format!("write conftest: {err}"))?;
        let tools: Vec<&str> = language_runtime_probes(&root)
            .iter()
            .map(|(_, tool, _)| *tool)
            .collect();
        let host_python = python_runtime_for_os(std::env::consts::OS).0;
        assert_eq!(tools, vec![host_python, "pytest"]);
        std::fs::remove_dir_all(&root).map_err(|err| format!("remove root: {err}"))?;

        let root = unique_command_test_dir("probe-bun");
        std::fs::create_dir_all(&root).map_err(|err| format!("create root: {err}"))?;
        std::fs::write(root.join("package.json"), "{}")
            .map_err(|err| format!("write pkg: {err}"))?;
        std::fs::write(root.join("bun.lockb"), "").map_err(|err| format!("write lock: {err}"))?;
        let tools: Vec<&str> = language_runtime_probes(&root)
            .iter()
            .map(|(_, tool, _)| *tool)
            .collect();
        assert_eq!(tools, vec!["node", "bun"]);
        std::fs::remove_dir_all(&root).map_err(|err| format!("remove root: {err}"))?;

        // #2183 review: a bare pyproject.toml is PEP 517 packaging, not
        // pytest evidence — only python3 is probed.
        let root = unique_command_test_dir("probe-pyproject-only");
        std::fs::create_dir_all(root.join("tests"))
            .map_err(|err| format!("create tests: {err}"))?;
        std::fs::write(root.join("pyproject.toml"), "[project]\nname = \"x\"\n")
            .map_err(|err| format!("write pyproject: {err}"))?;
        std::fs::write(root.join("tests/test_x.py"), "import unittest\n")
            .map_err(|err| format!("write test: {err}"))?;
        let tools: Vec<&str> = language_runtime_probes(&root)
            .iter()
            .map(|(_, tool, _)| *tool)
            .collect();
        assert_eq!(tools, vec![host_python]);
        std::fs::remove_dir_all(&root).map_err(|err| format!("remove root: {err}"))?;

        // A JS-only workspace is labeled javascript, not typescript (#2183
        // review).
        let root = unique_command_test_dir("probe-js-only");
        std::fs::create_dir_all(&root).map_err(|err| format!("create root: {err}"))?;
        std::fs::write(root.join("index.js"), "console.log(1);\n")
            .map_err(|err| format!("write js: {err}"))?;
        let labels: Vec<&str> = language_runtime_probes(&root)
            .iter()
            .map(|(language, _, _)| *language)
            .collect();
        assert_eq!(labels, vec!["javascript"]);
        std::fs::remove_dir_all(&root).map_err(|err| format!("remove root: {err}"))?;

        // pnpm and yarn lockfiles add their runners.
        for (lockfile, tool) in [("pnpm-lock.yaml", "pnpm"), ("yarn.lock", "yarn")] {
            let root = unique_command_test_dir("probe-pm");
            std::fs::create_dir_all(&root).map_err(|err| format!("create root: {err}"))?;
            std::fs::write(root.join("package.json"), "{}")
                .map_err(|err| format!("write pkg: {err}"))?;
            std::fs::write(root.join(lockfile), "").map_err(|err| format!("write lock: {err}"))?;
            let tools: Vec<&str> = language_runtime_probes(&root)
                .iter()
                .map(|(_, candidate, _)| *candidate)
                .collect();
            assert_eq!(tools, vec!["node", tool], "lockfile {lockfile}");
            std::fs::remove_dir_all(&root).map_err(|err| format!("remove root: {err}"))?;
        }
        Ok(())
    }

    /// A detected-language entry with every field the human screen reads.
    fn detected_entry(
        language: LanguageId,
        adapter_available: bool,
    ) -> output::doctor::DoctorDetectedLanguage {
        output::doctor::DoctorDetectedLanguage {
            language: language.as_str().to_string(),
            status: language_status(language),
            adapter_available,
            enabled: Some(true),
        }
    }

    #[test]
    fn detected_entries_map_javascript_to_its_typescript_entry() {
        let entries =
            detected_language_entries(&[LanguageId::JavaScript], Some(&[LanguageId::TypeScript]));
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].language, "javascript");
        assert_eq!(
            entries[0].enabled,
            Some(true),
            "the typescript entry analyzes javascript, so it reads as enabled"
        );
        let entries =
            detected_language_entries(&[LanguageId::JavaScript], Some(&[LanguageId::Rust]));
        assert_eq!(
            entries[0].enabled,
            Some(false),
            "without the typescript entry nothing analyzes javascript"
        );
    }

    #[test]
    fn detected_entries_stay_unknown_without_a_loaded_configuration() {
        let entries = detected_language_entries(&[LanguageId::Rust, LanguageId::Python], None);
        assert!(
            entries.iter().all(|entry| entry.enabled.is_none()),
            "an unloadable config must not read as disabled: {entries:?}"
        );
    }

    fn unanalyzed_entry(
        language: &str,
        file_count: usize,
    ) -> output::doctor::DoctorUnanalyzedLanguage {
        output::doctor::DoctorUnanalyzedLanguage {
            language: language.to_string(),
            file_count,
        }
    }

    #[test]
    fn unanalyzed_languages_line_names_go_as_not_analyzed() {
        assert_eq!(unanalyzed_languages_line(&[]), None);
        let line =
            unanalyzed_languages_line(&[unanalyzed_entry("Go", 2), unanalyzed_entry("Shell", 1)])
                .unwrap_or_default();
        assert!(
            line.starts_with("~ Unanalyzed languages: Go (2 file(s)), Shell (1 file(s));"),
            "{line}"
        );
        assert!(line.contains("never as clean"), "{line}");

        // Mixed workspace: the Go half is named beside the detected Rust.
        let lines = detected_languages_lines(
            &[detected_entry(LanguageId::Rust, true)],
            &[unanalyzed_entry("Go", 2)],
        );
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(
            lines[0].starts_with("- Detected languages: rust"),
            "{lines:?}"
        );
        assert!(
            lines[1].starts_with("~ Unanalyzed languages: Go (2 file(s))"),
            "{lines:?}"
        );
        let lines = detected_languages_lines(&[], &[unanalyzed_entry("Go", 2)]);
        assert_eq!(lines[0], "- Detected languages: none detected", "{lines:?}");
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert_eq!(
            detected_languages_lines(&[detected_entry(LanguageId::Rust, true)], &[]).len(),
            1
        );
        // An adapter that was not compiled into this binary says so on the
        // same line; the typed `adapter_available` field is what says it in JSON.
        // The expectation follows the compiled features: `--all-features`
        // builds (like CI's JUnit job) compile the Perl adapter (#7137).
        let unavailable = detected_language_entries(&[LanguageId::Perl], Some(&[]));
        let first = &detected_languages_lines(&unavailable, &[])[0];
        if cfg!(feature = "lang-perl") {
            assert!(unavailable[0].adapter_available);
            assert!(!first.contains("[adapter not compiled]"), "{first}");
        } else {
            assert!(!unavailable[0].adapter_available);
            assert!(first.contains("[adapter not compiled]"), "{first}");
        }
    }

    #[test]
    fn detect_languages_finds_a_cpan_layout_perl_project() -> Result<(), String> {
        // A module two levels under `lib/` is below the shallow scan, yet
        // the Perl preview counts it; detection must agree, or doctor prints
        // "none detected" beside a Perl section counting files. Files under a
        // skipped directory (`target/`, `node_modules/`) detect nothing.
        let cases: [(&str, &[&str]); 4] = [
            ("deep", &["lib/Acme/Calc.pm", "README"]),
            ("tests", &["t/calc.t"]),
            ("marker", &["Makefile.PL"]),
            ("none", &["target/example.t", "node_modules/pkg/lib/X.pm"]),
        ];
        for (name, files) in cases {
            let root = unique_command_test_dir(&format!("detect-perl-{name}"));
            for file in files {
                let path = root.join(file);
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent).map_err(|err| format!("mkdir: {err}"))?;
                }
                std::fs::write(&path, "1;\n").map_err(|err| format!("write {file}: {err}"))?;
            }
            let expected = if name == "none" {
                Vec::new()
            } else {
                vec![LanguageId::Perl]
            };
            assert_eq!(detect_languages(&root), expected, "{name}");
            std::fs::remove_dir_all(&root).map_err(|err| format!("remove root: {err}"))?;
        }
        Ok(())
    }

    #[test]
    #[cfg(unix)]
    fn perl_walks_do_not_follow_directory_symlink_loops() -> Result<(), String> {
        // A `src/loop -> .` link made `ripr doctor` descend forever. The Perl
        // and cache-size walks must terminate, and a `.pm` reachable only
        // through a link does not count, matching the Rust/Python/TypeScript
        // walkers.
        let root = unique_command_test_dir("perl-symlink-loop");
        let src = root.join("src");
        std::fs::create_dir_all(&src).map_err(|err| format!("mkdir: {err}"))?;
        std::os::unix::fs::symlink(".", src.join("loop"))
            .map_err(|err| format!("symlink: {err}"))?;
        std::os::unix::fs::symlink("../src", src.join("up"))
            .map_err(|err| format!("symlink: {err}"))?;
        assert!(!perl_project_detected(&root));
        assert_eq!(count_files(&root, "pm"), 0);

        // A directory link whose name ends in `.pm` is not a Perl file.
        let other = root.join("other");
        std::fs::create_dir_all(&other).map_err(|err| format!("mkdir: {err}"))?;
        std::os::unix::fs::symlink("../other", src.join("linked.pm"))
            .map_err(|err| format!("symlink: {err}"))?;
        assert!(!perl_project_detected(&root));
        assert_eq!(count_files(&root, "pm"), 0);
        // The cache-size walk terminates on the same loops.
        let size_before = dir_size_bytes(&root);

        std::fs::write(src.join("Real.pm"), "1;\n").map_err(|err| format!("write: {err}"))?;
        assert!(perl_project_detected(&root));
        assert_eq!(count_files(&root, "pm"), 1);
        assert_eq!(dir_size_bytes(&root), size_before + 3);
        std::fs::remove_dir_all(&root).map_err(|err| format!("remove root: {err}"))?;
        Ok(())
    }

    fn joined_path(dirs: &[&str]) -> Result<std::ffi::OsString, String> {
        std::env::join_paths(dirs.iter().copied().map(Path::new))
            .map_err(|err| format!("join PATH: {err}"))
    }

    fn windows_pathext() -> &'static str {
        ".COM;.EXE;.BAT;.CMD"
    }

    #[test]
    fn path_command_in_ignores_a_repo_local_prove_cmd() -> Result<(), String> {
        // Windows `where prove` searches the process cwd first, so a checkout
        // `prove.cmd` would otherwise read as "prove available on PATH" (#5103).
        let cwd_cmd = PathBuf::from("").join("prove.cmd");
        let path = joined_path(&["", "."])?;
        let found = path_command_in(
            "prove",
            &path,
            Some(windows_pathext()),
            true,
            &|candidate| candidate == cwd_cmd || candidate == Path::new(".").join("prove.cmd"),
        );
        assert_eq!(
            found, None,
            "cwd prove.cmd must not count as PATH: {found:?}"
        );
        Ok(())
    }

    #[test]
    fn path_command_in_prefers_path_prove_over_repo_local_prove_cmd() -> Result<(), String> {
        let cwd_cmd = PathBuf::from("").join("prove.cmd");
        let path_dir = PathBuf::from("strawberry-bin");
        let path_cmd = path_dir.join("prove.cmd");
        let path = joined_path(&["", path_dir.to_str().ok_or("path dir utf-8")?])?;
        let found = path_command_in(
            "prove",
            &path,
            Some(windows_pathext()),
            true,
            &|candidate| candidate == cwd_cmd || candidate == path_cmd.as_path(),
        );
        assert_eq!(found, Some(path_cmd), "PATH prove.cmd must win: {found:?}");
        Ok(())
    }

    #[test]
    fn path_command_in_windows_prefers_pathext_over_extensionless() -> Result<(), String> {
        let path_dir = PathBuf::from("bin");
        let bare = path_dir.join("prove");
        let cmd = path_dir.join("prove.cmd");
        let path = joined_path(&[path_dir.to_str().ok_or("path dir utf-8")?])?;
        let found = path_command_in("prove", &path, Some(".CMD"), true, &|candidate| {
            candidate == bare.as_path() || candidate == cmd.as_path()
        });
        assert_eq!(
            found,
            Some(cmd),
            "Windows PATHEXT prove.cmd must beat extensionless prove: {found:?}"
        );
        let bare_only = path_command_in("prove", &path, Some(".CMD"), true, &|candidate| {
            candidate == bare.as_path()
        });
        assert_eq!(
            bare_only,
            Some(bare),
            "extensionless PATH prove remains a fallback: {bare_only:?}"
        );
        Ok(())
    }

    #[test]
    fn path_command_in_windows_skips_interpreter_only_pathext() -> Result<(), String> {
        // Customized PATHEXT lists `.PS1` ahead of `.EXE`; `perllsp.ps1` cannot
        // be spawned directly, so the working `perllsp.exe` must win (#5351).
        let path_dir = PathBuf::from("bin");
        let ps1 = path_dir.join("perllsp.ps1");
        let exe = path_dir.join("perllsp.exe");
        let path = joined_path(&[path_dir.to_str().ok_or("path dir utf-8")?])?;
        let pathext = Some(".PS1;.VBS;.JS;.COM;.EXE;.BAT;.CMD");
        let found = path_command_in("perllsp", &path, pathext, true, &|candidate| {
            candidate == ps1.as_path() || candidate == exe.as_path()
        });
        assert_eq!(found, Some(exe), "perllsp.exe must beat .ps1: {found:?}");
        let ps1_only = path_command_in("perllsp", &path, pathext, true, &|candidate| {
            candidate == ps1.as_path()
        });
        assert_eq!(
            ps1_only, None,
            "interpreter-only script is not launchable: {ps1_only:?}"
        );
        let candidates = path_command_candidates(&path_dir, "perllsp", pathext, true);
        assert!(
            !candidates.contains(&ps1),
            "candidates must exclude .ps1: {candidates:?}"
        );
        Ok(())
    }

    #[test]
    fn is_cwd_path_entry_treats_empty_dot_and_backslash_dot_as_cwd() {
        assert!(is_cwd_path_entry(Path::new("")));
        assert!(is_cwd_path_entry(Path::new(".")));
        assert!(is_cwd_path_entry(Path::new("./")));
        assert!(is_cwd_path_entry(Path::new(".\\")));
        assert!(!is_cwd_path_entry(Path::new("bin")));
        assert!(!is_cwd_path_entry(Path::new("strawberry-bin")));
    }

    #[test]
    fn path_command_in_unix_prove_cmd_is_not_a_prove_binary() -> Result<(), String> {
        let path_dir = PathBuf::from("bin");
        let decoy = path_dir.join("prove.cmd");
        let prove = path_dir.join("prove");
        let path = joined_path(&[path_dir.to_str().ok_or("path dir utf-8")?])?;
        let from_cmd = path_command_in("prove", &path, None, false, &|candidate| {
            candidate == decoy.as_path()
        });
        assert_eq!(
            from_cmd, None,
            "Unix prove.cmd is not `prove`: {from_cmd:?}"
        );
        let from_prove = path_command_in("prove", &path, None, false, &|candidate| {
            candidate == prove.as_path()
        });
        assert_eq!(
            from_prove,
            Some(prove),
            "Unix PATH prove must resolve: {from_prove:?}"
        );
        Ok(())
    }

    #[test]
    fn doctor_program_keeps_explicit_paths_and_skips_bare_names_off_path() {
        assert_eq!(
            doctor_program("/opt/perl/bin/perllsp"),
            Some(PathBuf::from("/opt/perl/bin/perllsp"))
        );
        assert_eq!(
            doctor_program(r"fixture-bin\perllsp"),
            Some(PathBuf::from(r"fixture-bin\perllsp"))
        );
        assert_eq!(doctor_program(""), None);
        // A bare name is PATH-only; this process PATH is not under test here.
        assert!(program_name_is_explicit_path("/usr/bin/prove"));
        assert!(program_name_is_explicit_path(r"tools\prove.cmd"));
        assert!(!program_name_is_explicit_path("prove"));
        assert!(!program_name_is_explicit_path("prove.cmd"));
    }

    #[test]
    fn detect_languages_treats_modern_ts_js_extensions_as_language_markers() -> Result<(), String> {
        // #4116: .mts/.cts are TypeScript markers and .mjs/.cjs are
        // JavaScript markers through the shared extension authority; a
        // mixed root is labeled TypeScript only, and near-misses stay
        // undetected.
        for (extension, expected) in [
            ("mts", LanguageId::TypeScript),
            ("cts", LanguageId::TypeScript),
            ("mjs", LanguageId::JavaScript),
            ("cjs", LanguageId::JavaScript),
        ] {
            let root = unique_command_test_dir(&format!("detect-{extension}"));
            std::fs::create_dir_all(&root).map_err(|err| format!("create root: {err}"))?;
            std::fs::write(root.join(format!("index.{extension}")), "export {};\n")
                .map_err(|err| format!("write source: {err}"))?;
            assert_eq!(detect_languages(&root), vec![expected], ".{extension} root");
            std::fs::remove_dir_all(&root).map_err(|err| format!("remove root: {err}"))?;
        }

        let root = unique_command_test_dir("detect-mixed-ts-js");
        std::fs::create_dir_all(&root).map_err(|err| format!("create root: {err}"))?;
        std::fs::write(root.join("index.ts"), "export {};\n")
            .map_err(|err| format!("write source: {err}"))?;
        std::fs::write(root.join("helper.mjs"), "export {};\n")
            .map_err(|err| format!("write source: {err}"))?;
        assert_eq!(detect_languages(&root), vec![LanguageId::TypeScript]);
        std::fs::remove_dir_all(&root).map_err(|err| format!("remove root: {err}"))?;

        let root = unique_command_test_dir("detect-near-miss");
        std::fs::create_dir_all(&root).map_err(|err| format!("create root: {err}"))?;
        std::fs::write(root.join("index.mjsx"), "export {};\n")
            .map_err(|err| format!("write source: {err}"))?;
        assert!(
            !detect_languages(&root).contains(&LanguageId::TypeScript)
                && !detect_languages(&root).contains(&LanguageId::JavaScript),
            ".mjsx must not detect as a TS/JS marker"
        );
        std::fs::remove_dir_all(&root).map_err(|err| format!("remove root: {err}"))?;
        Ok(())
    }

    /// #7506 fence: nested `rust/Cargo.toml` does not become a Rust language
    /// marker. Scope uses the cargo-only locator; `detect_languages` stays
    /// a shallow root scan.
    #[test]
    fn detect_languages_stays_shallow_when_nested_rust_lives_under_typescript_root()
    -> Result<(), String> {
        let root = unique_command_test_dir("detect-polyglot-ts-nested-rust");
        std::fs::create_dir_all(root.join("rust/crates/x/src"))
            .map_err(|err| format!("create nested crate: {err}"))?;
        std::fs::write(root.join("package.json"), "{\"name\": \"site\"}\n")
            .map_err(|err| format!("write package.json: {err}"))?;
        std::fs::write(
            root.join("rust/Cargo.toml"),
            "[workspace]\nmembers = [\"crates/x\"]\n",
        )
        .map_err(|err| format!("write workspace manifest: {err}"))?;
        std::fs::write(
            root.join("rust/crates/x/Cargo.toml"),
            "[package]\nname = \"x\"\nversion = \"0.1.0\"\n",
        )
        .map_err(|err| format!("write package manifest: {err}"))?;
        std::fs::write(root.join("rust/crates/x/src/lib.rs"), "pub fn f() {}\n")
            .map_err(|err| format!("write lib.rs: {err}"))?;
        let detected = detect_languages(&root);
        let _ = std::fs::remove_dir_all(&root);
        if detected != vec![LanguageId::TypeScript] {
            return Err(format!(
                "detect_languages must stay shallow (TypeScript only): {detected:?}"
            ));
        }
        Ok(())
    }

    #[test]
    fn enabled_language_keeps_primary_runtime_required_when_other_tool_is_detected() {
        let mut probes = vec![("typescript", "yarn", "install Yarn")];
        append_missing_primary_runtime_probes(&mut probes, &[LanguageId::TypeScript]);
        assert_eq!(
            probes,
            vec![
                ("typescript", "yarn", "install Yarn"),
                ("typescript", "node", "install Node.js"),
            ]
        );
        assert!(runtime_probe_is_required(
            "typescript",
            "node",
            &[LanguageId::TypeScript]
        ));
        assert!(!runtime_probe_is_required(
            "typescript",
            "yarn",
            &[LanguageId::TypeScript]
        ));
    }

    /// #4378: `ripr doctor` on Windows suggested `apt install python3` and
    /// failed a host whose installer put `python` (not `python3`) on PATH.
    #[test]
    fn python_runtime_probe_and_hint_follow_the_host() {
        let (windows_tool, windows_hint) = python_runtime_for_os("windows");
        assert_eq!(windows_tool, "python");
        assert!(windows_hint.contains("winget install"), "{windows_hint}");
        assert!(!windows_hint.contains("apt"), "{windows_hint}");

        let (macos_tool, macos_hint) = python_runtime_for_os("macos");
        assert_eq!(macos_tool, "python3");
        assert!(macos_hint.contains("brew install"), "{macos_hint}");
        assert!(!macos_hint.contains("apt"), "{macos_hint}");

        assert_eq!(
            python_runtime_for_os("linux"),
            ("python3", "install python3 (e.g. apt install python3)")
        );

        // The configured-language probe and the detected-language probe use
        // the same host answer, so a Windows root is never probed twice.
        let host = python_runtime_for_os(std::env::consts::OS);
        assert_eq!(primary_runtime("python"), Some(host));
    }

    #[test]
    fn language_runtime_probe_line_names_labels_evidence_and_hint() {
        // #2183 review: the emitted contract is pinned, not just the list.
        let pass = language_runtime_probe_line(
            "python",
            output::doctor::DoctorStatus::Pass,
            "Python 3.12.3",
            "install python3",
        );
        assert!(pass.starts_with('✓'));
        assert!(pass.contains("python verify-route runtime: Python 3.12.3"));
        assert!(!pass.contains("install python3"));
        let fail = language_runtime_probe_line(
            "python",
            output::doctor::DoctorStatus::Fail,
            "python3 not available",
            "install python3 (e.g. apt install python3)",
        );
        assert!(fail.starts_with('!'));
        assert!(
            fail.contains("python3 not available — install python3 (e.g. apt install python3)")
        );
    }

    #[test]
    fn configured_primary_runtime_failure_fails_report_and_optional_preview_does_not()
    -> Result<(), String> {
        let root = unique_command_test_dir("runtime-probe-requirement");
        std::fs::create_dir_all(&root).map_err(|err| format!("create root: {err}"))?;
        std::fs::write(root.join("package.json"), "{}")
            .map_err(|err| format!("write package marker: {err}"))?;

        let mut required_report = output::doctor::DoctorReport::new(&root.display().to_string());
        let required_ok = add_language_runtime_probes(
            &root,
            &[LanguageId::TypeScript],
            &[LanguageId::TypeScript],
            &mut required_report,
            false,
            |tool| {
                if tool == "node" {
                    (
                        output::doctor::DoctorStatus::Fail,
                        "node not available".to_string(),
                    )
                } else {
                    (
                        output::doctor::DoctorStatus::Pass,
                        format!("{tool} available"),
                    )
                }
            },
        );
        assert!(!required_ok);
        assert_eq!(required_report.status, output::doctor::DoctorStatus::Fail);
        if output::doctor::doctor_report_result(&required_report).is_ok() {
            return Err("required runtime failure unexpectedly passed doctor".to_string());
        }
        let node_probe = required_report
            .runtime_probes
            .iter()
            .find(|probe| probe.tool == "node")
            .ok_or_else(|| "missing node runtime probe".to_string())?;
        assert!(node_probe.required);
        assert_eq!(node_probe.status, output::doctor::DoctorStatus::Fail);

        let mut optional_report = output::doctor::DoctorReport::new(&root.display().to_string());
        let optional_ok = add_language_runtime_probes(
            &root,
            &[LanguageId::TypeScript],
            &[LanguageId::Rust],
            &mut optional_report,
            false,
            |tool| {
                if tool == "node" {
                    (
                        output::doctor::DoctorStatus::Fail,
                        "node not available".to_string(),
                    )
                } else {
                    (
                        output::doctor::DoctorStatus::Pass,
                        format!("{tool} available"),
                    )
                }
            },
        );
        assert!(optional_ok);
        assert_eq!(optional_report.status, output::doctor::DoctorStatus::Pass);
        output::doctor::doctor_report_result(&optional_report)
            .map_err(|error| format!("optional runtime failure should remain advisory: {error}"))?;
        let node_probe = optional_report
            .runtime_probes
            .iter()
            .find(|probe| probe.tool == "node")
            .ok_or_else(|| "missing optional node runtime probe".to_string())?;
        assert!(!node_probe.required);
        assert_eq!(node_probe.status, output::doctor::DoctorStatus::Fail);

        std::fs::remove_dir_all(&root).map_err(|err| format!("remove root: {err}"))?;

        let configured_only_root = unique_command_test_dir("runtime-probe-configured-only");
        std::fs::create_dir_all(&configured_only_root)
            .map_err(|err| format!("create configured-only root: {err}"))?;
        let mut configured_only_report =
            output::doctor::DoctorReport::new(&configured_only_root.display().to_string());
        let configured_only_ok = add_language_runtime_probes(
            &configured_only_root,
            &[],
            &[LanguageId::Python],
            &mut configured_only_report,
            false,
            |tool| {
                (
                    output::doctor::DoctorStatus::Fail,
                    format!("{tool} not available"),
                )
            },
        );
        assert!(!configured_only_ok);
        assert!(
            configured_only_report
                .runtime_probes
                .iter()
                .any(|probe| probe.language == "python"
                    && probe.tool == python_runtime_for_os(std::env::consts::OS).0)
        );
        std::fs::remove_dir_all(&configured_only_root)
            .map_err(|err| format!("remove configured-only root: {err}"))?;
        Ok(())
    }

    #[test]
    fn source_build_profile_keeps_enabled_language_runtime_failure_advisory() -> Result<(), String>
    {
        // PR #4196 review: an enabled TypeScript root with no node is an
        // analysis failure, but not a RIPR source-build prerequisite. The
        // same probe outcome must fail analysis and stay advisory for
        // source-build, while still being reported.
        let root = unique_command_test_dir("runtime-probe-source-build");
        std::fs::create_dir_all(&root).map_err(|err| format!("create root: {err}"))?;
        std::fs::write(root.join("package.json"), "{}")
            .map_err(|err| format!("write package marker: {err}"))?;
        let missing_node = |tool: &str| {
            if tool == "node" {
                (
                    output::doctor::DoctorStatus::Fail,
                    "node not available".to_string(),
                )
            } else {
                (
                    output::doctor::DoctorStatus::Pass,
                    format!("{tool} available"),
                )
            }
        };
        let mut outcomes = Vec::new();
        for profile in [
            output::doctor::DoctorProfile::Analysis,
            output::doctor::DoctorProfile::SourceBuild,
        ] {
            let mut report = output::doctor::DoctorReport::new(&root.display().to_string());
            report.profile = profile;
            let ok = add_language_runtime_probes(
                &root,
                &[LanguageId::TypeScript],
                &[LanguageId::TypeScript],
                &mut report,
                false,
                missing_node,
            );
            let node = report
                .runtime_probes
                .iter()
                .find(|probe| probe.tool == "node")
                .ok_or_else(|| format!("missing node probe for {profile:?}"))?;
            outcomes.push((
                profile,
                ok,
                report.status,
                node.required,
                node.status,
                output::doctor::doctor_report_result(&report).is_ok(),
            ));
        }
        std::fs::remove_dir_all(&root).map_err(|err| format!("remove root: {err}"))?;
        assert_eq!(
            outcomes,
            vec![
                (
                    output::doctor::DoctorProfile::Analysis,
                    false,
                    output::doctor::DoctorStatus::Fail,
                    true,
                    output::doctor::DoctorStatus::Fail,
                    false,
                ),
                (
                    output::doctor::DoctorProfile::SourceBuild,
                    true,
                    output::doctor::DoctorStatus::Pass,
                    false,
                    output::doctor::DoctorStatus::Fail,
                    true,
                ),
            ]
        );
        Ok(())
    }

    #[test]
    #[cfg(all(feature = "lang-python", feature = "lang-typescript"))]
    fn doctor_reports_unittest_and_package_only_ts_frameworks() -> Result<(), String> {
        // #2106 review: doctor output coverage for frameworks only visible
        // through the shared detectors.
        let root = unique_command_test_dir("doctor-unittest");
        std::fs::create_dir_all(&root).map_err(|err| format!("create root: {err}"))?;
        std::fs::write(root.join("test_pricing.py"), "import unittest\n")
            .map_err(|err| format!("write test file: {err}"))?;
        let lines = detect_test_surfaces(&root, &detect_languages(&root));
        assert!(
            lines
                .iter()
                .any(|surface| surface.evidence == "python: unittest"
                    && surface.framework.as_deref() == Some("unittest")),
            "expected python: unittest in {lines:?}"
        );
        std::fs::remove_dir_all(&root).map_err(|err| format!("remove root: {err}"))?;

        let root = unique_command_test_dir("doctor-ava");
        std::fs::create_dir_all(&root).map_err(|err| format!("create root: {err}"))?;
        std::fs::write(
            root.join("package.json"),
            r#"{"name":"ky","scripts":{"test":"xo && npm run build && ava"}}"#,
        )
        .map_err(|err| format!("write package.json: {err}"))?;
        let lines = detect_test_surfaces(&root, &detect_languages(&root));
        assert!(
            lines
                .iter()
                .any(|surface| surface.evidence == "typescript: ava"
                    && surface.framework.as_deref() == Some("ava")),
            "expected typescript: ava in {lines:?}"
        );
        std::fs::remove_dir_all(&root).map_err(|err| format!("remove root: {err}"))?;
        Ok(())
    }

    #[test]
    #[cfg(feature = "lang-python")]
    fn doctor_reports_pytest_from_zero_config_test_file_import() -> Result<(), String> {
        // #7662: doctor must agree with check on a zero-config pytest repo
        // whose tests import pytest without pytest.ini/conftest/section markers.
        let root = unique_command_test_dir("doctor-pytest-import");
        let tests_dir = root.join("tests");
        std::fs::create_dir_all(&tests_dir).map_err(|err| format!("create tests dir: {err}"))?;
        std::fs::write(
            tests_dir.join("test_module.py"),
            "import pytest\n\ndef test_n():\n    assert True\n",
        )
        .map_err(|err| format!("write test file: {err}"))?;
        let lines = detect_test_surfaces(&root, &detect_languages(&root));
        assert!(
            lines
                .iter()
                .any(|surface| surface.evidence == "python: pytest"
                    && surface.framework.as_deref() == Some("pytest")),
            "expected python: pytest in {lines:?}"
        );
        std::fs::remove_dir_all(&root).map_err(|err| format!("remove root: {err}"))?;
        Ok(())
    }

    #[cfg(feature = "lang-python")]
    fn python_surface_evidence(surfaces: &[output::doctor::DoctorTestSurface]) -> Vec<&str> {
        surfaces
            .iter()
            .filter(|surface| surface.language == "python")
            .map(|surface| surface.evidence.as_str())
            .collect()
    }

    #[test]
    #[cfg(feature = "lang-python")]
    fn doctor_reports_both_pytest_and_unittest_surfaces_for_mixed_imports() -> Result<(), String> {
        // #7664: mixed pytest+unittest imports, no config markers. After
        // #7995 the primary Option is pytest; unittest must still appear as
        // its own catalog entry.
        let root = unique_command_test_dir("doctor-mixed-py");
        let tests_dir = root.join("tests");
        std::fs::create_dir_all(&tests_dir).map_err(|err| format!("create tests dir: {err}"))?;
        std::fs::write(
            tests_dir.join("test_pytest_style.py"),
            "import pytest\n\ndef test_multiply():\n    assert 2 * 3 == 6\n",
        )
        .map_err(|err| format!("write pytest file: {err}"))?;
        std::fs::write(
            tests_dir.join("test_unit_style.py"),
            "import unittest\n\nclass TestClamp(unittest.TestCase):\n    def test_clamp_lower(self):\n        self.assertEqual(1, 1)\n",
        )
        .map_err(|err| format!("write unittest file: {err}"))?;
        let surfaces = detect_test_surfaces(&root, &detect_languages(&root));
        assert_eq!(
            python_surface_evidence(&surfaces),
            vec!["python: pytest", "python: unittest"],
            "mixed imports must emit both Python surfaces, got {surfaces:?}"
        );
        assert!(
            surfaces.iter().any(|surface| {
                surface.language == "python" && surface.framework.as_deref() == Some("unittest")
            }),
            "unittest must be typed, not only present in evidence: {surfaces:?}"
        );
        std::fs::remove_dir_all(&root).map_err(|err| format!("remove root: {err}"))?;
        Ok(())
    }

    #[test]
    #[cfg(feature = "lang-python")]
    fn doctor_reports_both_when_pytest_config_and_unittest_imports_coexist() -> Result<(), String> {
        // #7664: pytest.ini adds pytest; it must not skip the unittest scan.
        let root = unique_command_test_dir("doctor-mixed-cfg");
        std::fs::create_dir_all(&root).map_err(|err| format!("create root: {err}"))?;
        std::fs::write(root.join("pytest.ini"), "[pytest]\n")
            .map_err(|err| format!("write pytest.ini: {err}"))?;
        std::fs::write(
            root.join("test_unit_style.py"),
            "import unittest\n\nclass TestClamp(unittest.TestCase):\n    def test_clamp_lower(self):\n        self.assertEqual(1, 1)\n",
        )
        .map_err(|err| format!("write unittest file: {err}"))?;
        let surfaces = detect_test_surfaces(&root, &detect_languages(&root));
        assert_eq!(
            python_surface_evidence(&surfaces),
            vec!["python: pytest", "python: unittest"],
            "config markers must not hide unittest, got {surfaces:?}"
        );
        std::fs::remove_dir_all(&root).map_err(|err| format!("remove root: {err}"))?;
        Ok(())
    }

    #[test]
    #[cfg(feature = "lang-python")]
    fn doctor_does_not_invent_a_second_python_surface_for_pytest_only() -> Result<(), String> {
        // Negative: a pytest-only repo must not grow a unittest row.
        let root = unique_command_test_dir("doctor-pytest-only");
        let tests_dir = root.join("tests");
        std::fs::create_dir_all(&tests_dir).map_err(|err| format!("create tests dir: {err}"))?;
        std::fs::write(
            tests_dir.join("test_module.py"),
            "import pytest\n\ndef test_n():\n    assert True\n",
        )
        .map_err(|err| format!("write test file: {err}"))?;
        let surfaces = detect_test_surfaces(&root, &detect_languages(&root));
        assert_eq!(
            python_surface_evidence(&surfaces),
            vec!["python: pytest"],
            "pytest-only must stay a single surface, got {surfaces:?}"
        );
        std::fs::remove_dir_all(&root).map_err(|err| format!("remove root: {err}"))?;
        Ok(())
    }

    #[test]
    #[cfg(feature = "lang-python")]
    fn doctor_does_not_treat_pytest_lookalike_import_as_a_second_surface() -> Result<(), String> {
        // Negative: `import pytesting` is not pytest evidence (#7662).
        let root = unique_command_test_dir("doctor-lookalike-mixed");
        std::fs::create_dir_all(&root).map_err(|err| format!("create root: {err}"))?;
        std::fs::write(root.join("test_lookalike.py"), "import pytesting\n")
            .map_err(|err| format!("write lookalike: {err}"))?;
        std::fs::write(root.join("test_unit_style.py"), "import unittest\n")
            .map_err(|err| format!("write unittest: {err}"))?;
        let surfaces = detect_test_surfaces(&root, &detect_languages(&root));
        assert_eq!(
            python_surface_evidence(&surfaces),
            vec!["python: unittest"],
            "lookalike pytest import must not mint a pytest surface, got {surfaces:?}"
        );
        std::fs::remove_dir_all(&root).map_err(|err| format!("remove root: {err}"))?;
        Ok(())
    }

    #[test]
    #[cfg(feature = "lang-python")]
    fn doctor_does_not_treat_unittest_mock_as_a_second_surface() -> Result<(), String> {
        // #7664 review: pytest.ini + `from unittest import mock` is pytest-only.
        let root = unique_command_test_dir("doctor-unittest-mock");
        std::fs::create_dir_all(&root).map_err(|err| format!("create root: {err}"))?;
        std::fs::write(root.join("pytest.ini"), "[pytest]\n")
            .map_err(|err| format!("write pytest.ini: {err}"))?;
        std::fs::write(
            root.join("test_api.py"),
            "from unittest import mock\n\ndef test_api():\n    assert True\n",
        )
        .map_err(|err| format!("write test file: {err}"))?;
        let surfaces = detect_test_surfaces(&root, &detect_languages(&root));
        assert_eq!(
            python_surface_evidence(&surfaces),
            vec!["python: pytest"],
            "unittest.mock must not mint a unittest surface, got {surfaces:?}"
        );
        std::fs::remove_dir_all(&root).map_err(|err| format!("remove root: {err}"))?;
        Ok(())
    }

    #[test]
    fn doctor_requires_root_value() {
        assert_eq!(
            doctor(&args(&["--root"])),
            Err("missing value for --root".to_string())
        );
    }

    #[test]
    fn perl_next_command_never_recommends_a_flag_check_rejects() {
        // #2105: `ripr check` has no --languages flag; every doctor
        // recommendation must stay within the check parser's contract.
        for compiled in [true, false] {
            for (producer, found) in [
                (Some("perllsp"), Some("perllsp")),
                (Some("perllsp"), None),
                (Some("perl-ripr-facts"), None),
                (None, None),
            ] {
                let command = perl_next_command(compiled, producer, found);
                assert!(
                    !command.contains("--languages"),
                    "recommendation must not name --languages: {command}"
                );
            }
        }
        // The managed-present branch points at the config-driven route.
        let managed = perl_next_command(true, Some("perllsp"), Some("perllsp"));
        assert!(managed.contains("[languages]"));
        // #3886: a bare `ripr check` resolves the default branch; `check`
        // has no `--head`, and `origin/main` need not exist.
        assert!(managed.ends_with("then: ripr check"));
        let unpublished = perl_next_command(true, Some("perllsp"), None);
        assert!(unpublished.ends_with("then: ripr check"));
        // A repo `[perl].executable` is ignored without the user opt-in, so
        // recommending it must name the opt-in.
        assert!(
            unpublished.contains("set [perl].executable and RIPR_ALLOW_REPO_PERL_EXECUTABLE=1"),
            "{unpublished}"
        );
        // The packet-mode branch is unchanged.
        let packet = perl_next_command(true, None, None);
        assert!(packet.contains("--perl-facts"));
    }

    #[test]
    fn perl_next_command_names_the_unpublished_exporter_not_perllsp() {
        // Managed mode without a compatible exporter: the install hint must
        // name the canonical exporter and say it is not published, not the
        // argv-incompatible `perllsp` LSP server.
        let missing = perl_next_command(true, Some("perllsp"), None);
        assert!(
            !missing.contains("install perllsp"),
            "must not recommend installing perllsp: {missing}"
        );
        assert!(
            missing.contains("`perl-ripr-facts`") && missing.contains("not yet published"),
            "must name the unpublished canonical exporter: {missing}"
        );
    }

    #[test]
    fn perl_next_command_without_adapter_never_suggests_a_rejected_edit() {
        // In a build without `lang-perl`, adding perl to [languages] makes
        // `ripr check` exit 2, and --perl-facts cannot be analyzed either,
        // so every branch must return the shared prerequisite text.
        for (producer, found) in [
            (Some("perllsp"), Some("perllsp")),
            (Some("perl-ripr-facts"), None),
            (None, None),
        ] {
            let command = perl_next_command(false, producer, found);
            assert_eq!(command, LanguageId::Perl.unavailable_adapter_recovery());
            assert!(
                !command.contains("then: ripr check") && !command.contains("--perl-facts <"),
                "uncompiled adapter must not get a runnable-looking next step: {command}"
            );
        }
    }

    /// #7434: the Perl recovery paragraph belongs on `next:`. The
    /// test-surfaces catalog is a semicolon-joined detection list; embedding
    /// the same paragraph there prints it twice on one human screen.
    #[test]
    fn uncompiled_perl_adapter_recovery_prints_once_on_the_human_doctor_screen()
    -> Result<(), String> {
        let root = unique_command_test_dir("doctor-perl-recovery-once");
        std::fs::create_dir_all(root.join("lib")).map_err(|err| format!("create lib: {err}"))?;
        std::fs::create_dir_all(root.join("t")).map_err(|err| format!("create t: {err}"))?;
        std::fs::write(root.join("lib/Pricing.pm"), "package Pricing;\n1;\n")
            .map_err(|err| format!("write pm: {err}"))?;
        std::fs::write(
            root.join("t/pricing.t"),
            "use Test::More;\nok(1);\ndone_testing();\n",
        )
        .map_err(|err| format!("write t: {err}"))?;

        let detected = detect_languages(&root);
        assert!(
            detected.contains(&LanguageId::Perl),
            "fixture must detect Perl: {detected:?}"
        );
        let surfaces = detect_test_surfaces(&root, &detected);
        assert!(
            !surfaces.is_empty(),
            "fixture must produce test-surface evidence"
        );
        let catalog = surfaces
            .iter()
            .map(|surface| surface.evidence.as_str())
            .collect::<Vec<_>>()
            .join("; ");
        let recovery = LanguageId::Perl.unavailable_adapter_recovery();
        let preview = probe_perl_preview(&root, None)
            .ok_or_else(|| format!("fixture must produce a Perl preview: {catalog}"))?;
        let preview_text = perl_preview_lines(&preview).join("\n");
        let screen = format!("- Detected test surfaces: {catalog}\n{preview_text}");
        let occurrences = screen.matches(recovery.as_str()).count();

        assert!(
            !catalog.contains(&recovery),
            "the test-surfaces catalog must not embed the recovery paragraph: {catalog}"
        );

        if cfg!(feature = "lang-perl") {
            assert_eq!(
                occurrences, 0,
                "a lang-perl build has no recovery paragraph to print: {screen}"
            );
            assert!(
                catalog.contains("perl: adapter compiled (lang-perl feature ON)"),
                "compiled adapter must stay a catalog fact: {catalog}"
            );
        } else {
            assert_eq!(
                occurrences, 1,
                "the recovery paragraph must appear once, on next:, not twice: {screen}"
            );
            assert!(
                catalog.contains("perl: adapter NOT compiled (see Perl preview below)"),
                "the catalog must name adapter state and point at the preview: {catalog}"
            );
            assert!(
                preview_text.contains(&format!("next: {recovery}")),
                "next: must keep the recovery paragraph: {preview_text}"
            );
            assert_eq!(
                preview.next_command, recovery,
                "JSON next_command must keep the recovery paragraph"
            );
        }

        std::fs::remove_dir_all(&root).map_err(|err| format!("remove root: {err}"))?;
        Ok(())
    }

    #[test]
    fn perl_exporter_lines_never_call_an_incompatible_binary_found() {
        let incompatible = perl_exporter_lines(&exporter_typed(&PerlExporterProbe::Incompatible {
            bin: "/opt/bin/perllsp".to_string(),
            version: "perllsp 0.17.0".to_string(),
        }));
        let first = incompatible.first().map(String::as_str).unwrap_or("");
        assert!(
            first.contains("does not accept `ripr-facts`")
                && first.contains("not a compatible exporter")
                && !first.starts_with("exporter: found at"),
            "incompatible exporter must not read as found/working: {incompatible:?}"
        );
        // The second line is the prerequisite pointer: the argv ripr sends and
        // the exporter that would accept it.
        assert_eq!(incompatible.len(), 2, "{incompatible:?}");
        let note = incompatible.get(1).map(String::as_str).unwrap_or("");
        assert!(
            note.starts_with("note: ")
                && note.contains(crate::app::PERL_FACT_PACKET_SCHEMA)
                && note.contains(crate::domain::PERL_FACT_EXPORTER)
                && note.contains("not yet published"),
            "incompatible exporter must name the argv and the compatible exporter: {note:?}"
        );
        let compatible = perl_exporter_lines(&exporter_typed(&PerlExporterProbe::Compatible {
            bin: "/opt/bin/perl-ripr-facts".to_string(),
            version: "perl-ripr-facts 0.1.0".to_string(),
        }));
        assert!(
            compatible
                .first()
                .is_some_and(|line| line.starts_with("exporter: compatible")),
            "compatible exporter line: {compatible:?}"
        );
    }

    #[test]
    fn doctor_rejects_unknown_arguments() {
        assert_eq!(
            doctor(&args(&["--bogus"])),
            Err("unknown doctor argument \"--bogus\". Run `ripr doctor --help`.".to_string())
        );
    }

    /// #4318: a repeated documented flag is not an unknown argument. The
    /// error names the real condition and echoes both values.
    #[test]
    fn doctor_rejects_a_second_root_by_naming_the_condition() {
        assert_eq!(
            doctor(&args(&["--root", "a", "--root", "b"])),
            Err(
                "doctor accepts at most one --root; found both \"a\" and \"b\". Run `ripr doctor --help`."
                    .to_string()
            )
        );
    }

    /// #4318: a positional is not an unknown flag either; the error points at
    /// the flag that carries a root instead of the help screen alone.
    #[test]
    fn doctor_rejects_positional_arguments_by_naming_the_condition() {
        assert_eq!(
            doctor(&args(&["some/path"])),
            Err(
                "doctor does not accept positional arguments; got \"some/path\"; pass the workspace root with `--root <path>`. Run `ripr doctor --help`."
                    .to_string()
            )
        );
    }

    /// #4318 review: a known doctor flag in the `--root` value position means
    /// the root was omitted, not that a directory named `--json` was chosen.
    /// The report must not run against a path named after a flag.
    #[test]
    fn doctor_reports_a_missing_root_value_when_a_known_flag_follows() {
        for flag in ["--json", "--profile", "--root", "--help", "-h"] {
            assert_eq!(
                doctor(&args(&["--root", flag])),
                Err("missing value for --root".to_string()),
                "a known flag cannot be the --root value: {flag}"
            );
        }
    }

    #[test]
    fn doctor_accepts_default_root() {
        assert_eq!(doctor(&args(&[])), Ok(()));
    }

    /// #5214 review (B1): the registered verification subject
    /// `tests/fixtures/verification/ripr/doctor.valid.json` must describe a
    /// state the producer can actually emit, not merely a shape the schema
    /// tolerates. Replaying the fixture's declared `adapter_compiled`,
    /// `producer` and `exporter.state` through the producer's own branch is
    /// what establishes that. The first draft of the fixture paired
    /// `adapter_compiled: false` with the packet-mode `next_command`, which
    /// `perl_next_command` can never return.
    #[test]
    fn registered_doctor_fixture_perl_state_is_reachable_from_the_producer() -> Result<(), String> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/verification/ripr/doctor.valid.json");
        let text = std::fs::read_to_string(&path)
            .map_err(|error| format!("read {}: {error}", path.display()))?;
        let fixture: serde_json::Value = serde_json::from_str(&text)
            .map_err(|error| format!("parse {}: {error}", path.display()))?;
        let preview = fixture
            .get("perl_preview")
            .ok_or_else(|| format!("fixture carries no perl_preview: {fixture}"))?;

        let adapter_compiled = preview["adapter_compiled"].as_bool().ok_or_else(|| {
            format!("fixture perl_preview.adapter_compiled is not a bool: {preview}")
        })?;
        let producer = preview["producer"].as_str().map(str::to_string);
        // `PerlExporterProbe::compatible_bin` is the producer's own answer per
        // exporter state; replay it rather than restating the rule here.
        let exporter_state = preview["exporter"]["state"]
            .as_str()
            .ok_or_else(|| format!("fixture exporter.state is not a string: {preview}"))?;
        let compatible_bin = match exporter_state {
            "compatible" => Some(
                preview["exporter"]["executable"]
                    .as_str()
                    .ok_or_else(|| {
                        format!("a compatible exporter must name its executable: {preview}")
                    })?
                    .to_string(),
            ),
            "incompatible" | "not_found" => None,
            other => {
                return Err(format!(
                    "fixture declares an unknown exporter state `{other}`"
                ));
            }
        };
        let expected = preview["next_command"].as_str().ok_or_else(|| {
            format!("fixture perl_preview.next_command is not a string: {preview}")
        })?;
        let produced = perl_next_command(
            adapter_compiled,
            producer.as_deref(),
            compatible_bin.as_deref(),
        );
        if produced != expected {
            return Err(format!(
                "the registered fixture's perl_preview is not producible: adapter_compiled={adapter_compiled} \
                 producer={producer:?} exporter.state={exporter_state} makes the producer emit \
                 {produced:?}, but the fixture declares {expected:?}"
            ));
        }

        // Cross-field coherence: `adapter_available` on the detected-language
        // entry and `perl_preview.adapter_compiled` read the same build
        // constant, so a fixture that disagrees with itself describes no build.
        let perl_detected = fixture["detected_languages"]
            .as_array()
            .and_then(|entries| entries.iter().find(|entry| entry["language"] == "perl"))
            .ok_or_else(|| format!("fixture must declare perl as detected: {fixture}"))?;
        if perl_detected["adapter_available"] != preview["adapter_compiled"] {
            return Err(format!(
                "the fixture's detected_languages[perl].adapter_available ({}) disagrees with \
                 perl_preview.adapter_compiled ({}); both read the same lang-perl build constant",
                perl_detected["adapter_available"], preview["adapter_compiled"]
            ));
        }

        // Every branch the producer can take must be one of the states this
        // document vocabulary names, or the fixture asserts a state space the
        // producer does not have.
        for compiled in [false, true] {
            for configured in [
                None,
                Some("perl-ripr-facts"),
                Some("perllsp"),
                Some("custom"),
            ] {
                for found in [None, Some("perl-ripr-facts")] {
                    let command = perl_next_command(compiled, configured, found);
                    let classified = command == LanguageId::Perl.unavailable_adapter_recovery()
                        || command
                            == "add \"perl\" to [languages] enabled in ripr.toml, then: ripr check"
                        || command.starts_with("install a compatible Perl fact exporter")
                        || command
                            == "ripr check --perl-facts <packet.json> --diff <diff.patch> --json";
                    if !classified {
                        return Err(format!(
                            "perl_next_command produced an unclassified state for compiled={compiled} \
                             producer={configured:?} exporter={found:?}: {command}"
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    #[test]
    fn doctor_core_report_fails_closed_for_invalid_config() -> Result<(), String> {
        let dir = unique_command_test_dir("doctor-invalid-config");
        std::fs::create_dir_all(&dir).map_err(|err| format!("create temp dir: {err}"))?;
        std::fs::write(dir.join(CONFIG_FILE_NAME), "[invalid\n")
            .map_err(|err| format!("write invalid config: {err}"))?;

        let report = output::doctor::evaluate_doctor_core(&dir, &detect_languages(&dir));
        if report.status != output::doctor::DoctorStatus::Fail {
            return Err(format!(
                "invalid config should fail, got {:?}",
                report.status
            ));
        }
        let config_check = report
            .checks
            .iter()
            .find(|check| check.name == "config")
            .ok_or_else(|| "missing config check".to_string())?;
        if config_check.status != output::doctor::DoctorCheckStatus::Fail {
            return Err(format!(
                "invalid config check should fail, got {:?}",
                config_check.status
            ));
        }
        if !config_check
            .evidence
            .as_deref()
            .is_some_and(|evidence| evidence.contains("invalid ripr.toml"))
        {
            return Err(format!(
                "invalid config evidence was not actionable: {:?}",
                config_check.evidence
            ));
        }

        let json = report.render_json()?;
        let value: serde_json::Value =
            serde_json::from_str(&json).map_err(|err| format!("parse report JSON: {err}"))?;
        // Found by name, not by position: the check list grows, and an index
        // pins the order rather than the claim.
        let config_in_json = value["checks"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|check| check["name"] == "config");
        if value["status"] != "fail" || config_in_json.is_none_or(|check| check["status"] != "fail")
        {
            return Err(format!("unexpected invalid-config JSON report: {value}"));
        }

        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn doctor_core_report_fails_closed_for_missing_root() -> Result<(), String> {
        let root = unique_command_test_dir("doctor-missing-root");
        if root.exists() {
            return Err(format!("test root unexpectedly exists: {}", root.display()));
        }

        let report = output::doctor::evaluate_doctor_core(&root, &detect_languages(&root));
        if report.status != output::doctor::DoctorStatus::Fail {
            return Err(format!("missing root should fail, got {:?}", report.status));
        }
        let root_check = report
            .checks
            .iter()
            .find(|check| check.name == "root_directory")
            .ok_or_else(|| "missing root-directory check".to_string())?;
        if root_check.status != output::doctor::DoctorCheckStatus::Fail {
            return Err(format!(
                "missing root check should fail, got {:?}",
                root_check.status
            ));
        }
        if !root_check
            .evidence
            .as_deref()
            .is_some_and(|evidence| evidence.contains("does not exist"))
        {
            return Err(format!(
                "missing root evidence was not actionable: {:?}",
                root_check.evidence
            ));
        }
        Ok(())
    }

    #[test]
    fn doctor_core_report_fails_closed_for_file_root() -> Result<(), String> {
        // #5101: an existing file passed as --root must fail as "not a
        // directory", not as "does not exist".
        let dir = unique_command_test_dir("doctor-file-root");
        std::fs::create_dir_all(&dir).map_err(|err| format!("create temp dir: {err}"))?;
        let root = dir.join("Cargo.toml");
        std::fs::write(&root, "[package]\nname = \"file-root\"\n")
            .map_err(|err| format!("write file root: {err}"))?;
        if !root.is_file() {
            let _ = std::fs::remove_dir_all(&dir);
            return Err(format!(
                "file-root fixture is not a file: {}",
                root.display()
            ));
        }

        let report = output::doctor::evaluate_doctor_core(&root, &detect_languages(&root));
        let root_check = report
            .checks
            .iter()
            .find(|check| check.name == "root_directory")
            .ok_or_else(|| "missing root-directory check".to_string())?;
        let evidence = root_check.evidence.as_deref().unwrap_or_default();
        let result = if report.status != output::doctor::DoctorStatus::Fail
            || root_check.status != output::doctor::DoctorCheckStatus::Fail
            || !evidence.contains("is not a directory")
            || evidence.contains("does not exist")
        {
            Err(format!(
                "file root should fail as not-a-directory, got status={:?} check={root_check:?}",
                report.status
            ))
        } else {
            Ok(())
        };
        let _ = std::fs::remove_dir_all(&dir);
        result
    }

    // Deterministic missing-tool and empty-report-passes assertions live with
    // the moved model in `output::doctor::tests` now
    // (`doctor_tool_check_fails_closed_for_guaranteed_missing_tool`,
    // `empty_report_is_pass`). This test keeps the integration-level proof
    // that the `--json` doctor path fails closed for a malformed config.
    #[test]
    fn doctor_json_and_tool_failures_return_errors() -> Result<(), String> {
        let dir = unique_command_test_dir("doctor-json-invalid-config");
        std::fs::create_dir_all(&dir).map_err(|err| format!("create temp dir: {err}"))?;
        std::fs::write(dir.join(CONFIG_FILE_NAME), "[invalid\n")
            .map_err(|err| format!("write invalid config: {err}"))?;
        if doctor_json(
            &dir,
            output::doctor::DoctorProfile::Analysis,
            &detect_languages(&dir),
        )
        .is_ok()
        {
            let _ = std::fs::remove_dir_all(&dir);
            return Err("invalid JSON doctor report unexpectedly passed".to_string());
        }
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn doctor_human_projection_fails_for_missing_root() -> Result<(), String> {
        let root = unique_command_test_dir("doctor-human-missing-root");
        if root.exists() {
            return Err(format!("test root unexpectedly exists: {}", root.display()));
        }
        let root_arg = root.to_string_lossy().into_owned();
        if doctor(&args(&["--root", &root_arg])).is_ok() {
            return Err("human doctor unexpectedly passed for missing root".to_string());
        }
        Ok(())
    }

    #[test]
    fn doctor_human_projection_fails_for_file_root() -> Result<(), String> {
        let dir = unique_command_test_dir("doctor-human-file-root");
        std::fs::create_dir_all(&dir).map_err(|err| format!("create temp dir: {err}"))?;
        let root = dir.join("afile.txt");
        std::fs::write(&root, "not a workspace\n")
            .map_err(|err| format!("write file root: {err}"))?;
        let root_arg = root.to_string_lossy().into_owned();
        let result = doctor(&args(&["--root", &root_arg]));
        let _ = std::fs::remove_dir_all(&dir);
        if result.is_ok() {
            return Err("human doctor unexpectedly passed for a file root".to_string());
        }
        Ok(())
    }

    #[test]
    fn doctor_json_flag_accepts_explicit_root() -> Result<(), String> {
        doctor(&args(&["--json", "--root", "."]))
    }

    // --- preview_language_enable_suggestions tests ---

    /// The run's preview-enablement inputs, computed the way `doctor`
    /// computes them, so a test exercises the shared probe rather than a
    /// second derivation.
    fn test_preview_enablement(root: &Path) -> Option<PreviewEnablement> {
        preview_languages_to_enable(&detect_languages(root), &crate::config::load_for_root(root))
    }

    fn test_enablement_suggestions(root: &Path) -> Vec<String> {
        preview_language_enable_suggestions(test_preview_enablement(root).as_ref())
    }

    fn test_enable_before_first_command(root: &Path) -> Option<String> {
        enable_before_first_command_line(test_preview_enablement(root).as_ref())
    }

    /// When TypeScript files are detected in a directory that has no ripr.toml
    /// (so the config defaults to `["rust"]`) AND the `lang-typescript` feature
    /// was compiled in, we expect a suggestion line containing the copy-paste
    /// TOML block.
    #[cfg(feature = "lang-typescript")]
    #[test]
    fn doctor_suggests_typescript_when_detected_and_not_enabled() -> Result<(), String> {
        let dir = unique_command_test_dir("suggest-ts-detected");
        std::fs::create_dir_all(&dir).map_err(|err| format!("create dir: {err}"))?;
        // Drop a .ts file so TypeScript is detected.
        std::fs::write(dir.join("index.ts"), "export const x = 1;\n")
            .map_err(|err| format!("write ts: {err}"))?;
        // No ripr.toml → defaults to enabled = ["rust"] only.
        let suggestions = test_enablement_suggestions(&dir);
        let before = test_enable_before_first_command(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            !suggestions.is_empty(),
            "expected a suggestion when TS detected and not enabled"
        );
        assert!(
            before
                .as_deref()
                .is_some_and(|line| line.contains("enable typescript in ripr.toml")),
            "the first command must name the enable step; got {before:?}"
        );
        let joined = suggestions.join("\n");
        assert!(
            joined.contains("typescript"),
            "suggestion must name the language; got:\n{joined}"
        );
        assert!(
            joined.contains(r#"enabled = ["typescript"]"#),
            "a TypeScript-only root must not be told to enable Rust; got:\n{joined}"
        );
        assert!(
            !joined.contains(r#""rust""#),
            "default rust must not leak into a TypeScript-only tip; got:\n{joined}"
        );
        Ok(())
    }

    /// A mixed Rust + TypeScript root still names both languages, so following
    /// the tip does not disable Rust.
    #[cfg(feature = "lang-typescript")]
    #[test]
    fn doctor_enable_tip_keeps_rust_when_rust_is_detected() -> Result<(), String> {
        let dir = unique_command_test_dir("suggest-ts-with-rust");
        std::fs::create_dir_all(&dir).map_err(|err| format!("create dir: {err}"))?;
        std::fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"mixed\"\nversion = \"0.1.0\"\n",
        )
        .map_err(|err| format!("write Cargo.toml: {err}"))?;
        std::fs::write(dir.join("index.ts"), "export const x = 1;\n")
            .map_err(|err| format!("write ts: {err}"))?;
        let suggestions = test_enablement_suggestions(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(suggestions.len(), 1, "one tip: {suggestions:?}");
        assert!(
            !suggestions[0].contains('\n')
                && !suggestions[0].contains('\r')
                && !suggestions[0].contains("[languages] enabled"),
            "compact tip must stay one physical line of valid TOML instructions:\n{}",
            suggestions[0]
        );
        assert!(
            suggestions[0].contains(r#"enabled = ["rust", "typescript"]"#),
            "mixed repos keep rust; got:\n{}",
            suggestions[0]
        );
        Ok(())
    }

    /// A mixed repository must get one snippet that keeps the languages
    /// already enabled: a `["rust", "typescript"]` snippet would switch off
    /// Python, and the next doctor run would then suggest `["rust",
    /// "python"]`, undoing the first edit.
    #[cfg(all(feature = "lang-typescript", feature = "lang-python"))]
    #[test]
    fn doctor_enable_tip_keeps_already_enabled_languages() -> Result<(), String> {
        let dir = unique_command_test_dir("suggest-mixed-keeps-enabled");
        std::fs::create_dir_all(dir.join("src")).map_err(|err| format!("create dir: {err}"))?;
        std::fs::write(dir.join("src/index.ts"), "export const x = 1;\n")
            .map_err(|err| format!("write ts: {err}"))?;
        std::fs::write(dir.join("src/calc.py"), "def a():\n    return 1\n")
            .map_err(|err| format!("write py: {err}"))?;
        std::fs::write(
            dir.join("ripr.toml"),
            "[languages]\nenabled = [\"rust\", \"python\"]\n",
        )
        .map_err(|err| format!("write ripr.toml: {err}"))?;
        let suggestions = test_enablement_suggestions(&dir);
        let before = test_enable_before_first_command(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(suggestions.len(), 1, "one combined tip: {suggestions:?}");
        assert!(
            !suggestions[0].contains('\n')
                && suggestions[0].contains("[languages]")
                && !suggestions[0].contains("[languages] enabled"),
            "compact tip must stay one physical line of valid TOML instructions:\n{}",
            suggestions[0]
        );
        assert!(
            suggestions[0].contains(r#"enabled = ["python", "typescript"]"#),
            "the snippet must keep python and omit undetected rust; got:\n{}",
            suggestions[0]
        );
        assert!(
            !suggestions[0].contains(r#""rust""#),
            "no Rust markers means the tip must not name rust; got:\n{}",
            suggestions[0]
        );
        assert_eq!(
            before.as_deref(),
            Some(
                "- Before that: enable typescript in ripr.toml (see the Tip above); until then `ripr check` skips those files"
            )
        );
        Ok(())
    }

    /// JavaScript has no `[languages].enabled` entry of its own: the
    /// TypeScript adapter analyzes it. A JavaScript-only root must get a
    /// snippet that config loading accepts, and JavaScript with `typescript`
    /// already enabled needs no tip at all.
    #[cfg(feature = "lang-typescript")]
    #[test]
    fn doctor_enable_tip_maps_javascript_to_the_typescript_entry() -> Result<(), String> {
        let dir = unique_command_test_dir("suggest-javascript-only");
        std::fs::create_dir_all(dir.join("src")).map_err(|err| format!("create dir: {err}"))?;
        std::fs::write(dir.join("src/index.js"), "export const x = 1;\n")
            .map_err(|err| format!("write js: {err}"))?;
        let suggestions = test_enablement_suggestions(&dir);
        let before = test_enable_before_first_command(&dir);
        assert_eq!(suggestions.len(), 1, "one tip: {suggestions:?}");
        assert!(
            suggestions[0].starts_with("- Tip: javascript files detected")
                && suggestions[0].contains(r#"enabled = ["typescript"]"#)
                && !suggestions[0].contains(r#""javascript""#)
                && !suggestions[0].contains(r#""rust""#),
            "the snippet must name the typescript entry without rust; got:\n{}",
            suggestions[0]
        );
        assert_eq!(
            before.as_deref(),
            Some(
                "- Before that: enable typescript in ripr.toml (see the Tip above); until then `ripr check` skips those files"
            )
        );
        // The printed snippet must load, and after it the tip goes away.
        std::fs::write(
            dir.join("ripr.toml"),
            "[languages]\nenabled = [\"typescript\"]\n",
        )
        .map_err(|err| format!("write ripr.toml: {err}"))?;
        let loaded =
            crate::config::load_for_root(&dir).map(|config| config.languages().enabled().to_vec());
        let after = test_enablement_suggestions(&dir);
        let after_before = test_enable_before_first_command(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(
            loaded,
            Ok(vec![LanguageId::TypeScript]),
            "the suggested snippet must load"
        );
        assert!(after.is_empty(), "typescript covers javascript: {after:?}");
        assert_eq!(after_before, None);
        Ok(())
    }

    /// Without a detected-but-disabled preview language there is no enable
    /// step to name beside the first command.
    #[test]
    fn doctor_first_command_has_no_enable_step_for_rust_only() -> Result<(), String> {
        let dir = unique_command_test_dir("first-command-rust-only");
        std::fs::create_dir_all(&dir).map_err(|err| format!("create dir: {err}"))?;
        std::fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"test\"\nversion = \"0.1.0\"\n",
        )
        .map_err(|err| format!("write Cargo.toml: {err}"))?;
        let before = test_enable_before_first_command(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(before, None);
        Ok(())
    }

    /// When TypeScript is explicitly listed in ripr.toml `enabled`, no
    /// suggestion should appear even if .ts files are present.
    #[cfg(feature = "lang-typescript")]
    #[test]
    fn doctor_no_suggestion_when_typescript_already_enabled() -> Result<(), String> {
        let dir = unique_command_test_dir("suggest-ts-already-enabled");
        std::fs::create_dir_all(&dir).map_err(|err| format!("create dir: {err}"))?;
        std::fs::write(dir.join("index.ts"), "export const x = 1;\n")
            .map_err(|err| format!("write ts: {err}"))?;
        // ripr.toml explicitly enables typescript.
        std::fs::write(
            dir.join("ripr.toml"),
            "[languages]\nenabled = [\"rust\", \"typescript\"]\n",
        )
        .map_err(|err| format!("write ripr.toml: {err}"))?;
        let suggestions = test_enablement_suggestions(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            suggestions.is_empty(),
            "expected no suggestions when typescript already enabled; got: {suggestions:?}"
        );
        Ok(())
    }

    /// When no preview-language files are detected (only Rust), the suggestion
    /// list must be empty regardless of config.
    #[test]
    fn doctor_no_suggestion_when_no_preview_language_detected() -> Result<(), String> {
        let dir = unique_command_test_dir("suggest-no-preview");
        std::fs::create_dir_all(&dir).map_err(|err| format!("create dir: {err}"))?;
        // Only a Cargo.toml → Rust only, no preview language detected.
        std::fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"test\"\nversion = \"0.1.0\"\n",
        )
        .map_err(|err| format!("write Cargo.toml: {err}"))?;
        let suggestions = test_enablement_suggestions(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            suggestions.is_empty(),
            "expected no suggestions for Rust-only dir; got: {suggestions:?}"
        );
        Ok(())
    }

    /// When the binary was built WITHOUT the `lang-typescript` feature (adapter
    /// not compiled in), the suggestion must be suppressed even if .ts files are
    /// present. The user cannot enable an adapter that isn't in the binary.
    #[cfg(not(feature = "lang-typescript"))]
    #[test]
    fn doctor_no_suggestion_when_typescript_adapter_not_compiled() -> Result<(), String> {
        let dir = unique_command_test_dir("suggest-ts-not-compiled");
        std::fs::create_dir_all(&dir).map_err(|err| format!("create dir: {err}"))?;
        std::fs::write(dir.join("index.ts"), "export const x = 1;\n")
            .map_err(|err| format!("write ts: {err}"))?;
        // No ripr.toml → defaults to enabled = ["rust"] only.
        let suggestions = test_enablement_suggestions(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            suggestions.is_empty(),
            "expected no suggestions when lang-typescript feature is not compiled; got: {suggestions:?}"
        );
        Ok(())
    }
}
