use crate::analysis;
use crate::app::{self, CheckInput, Mode, OutputFormat};
use crate::cli::commands_numeric::{parse_positive_u64, parse_positive_usize};
use crate::cli::commands_options::PilotOptions;
use crate::cli::help;
use crate::cli::parse::{expect_value, parse_mode};
use crate::config::{CheckInputExplicit, RiprConfig, apply_to_check_input, load_for_root};
use crate::output;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

const DEFAULT_PILOT_TIMEOUT_MS: u64 = 30_000;
pub(crate) fn pilot(args: &[String]) -> Result<(), String> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        help::print_pilot_help();
        return Ok(());
    }

    let options = parse_pilot_options(args)?;
    if !options.root.is_dir() {
        return Err(format!(
            "pilot root {} is not a directory",
            options.root.display()
        ));
    }

    let config = load_for_root(&options.root)?;
    let mut input = CheckInput {
        root: options.root.clone(),
        mode: options.mode.clone(),
        ..CheckInput::default()
    };
    apply_to_check_input(&mut input, &config, options.explicit);

    let artifacts = pilot_artifacts(&options.out_dir);
    std::fs::create_dir_all(&options.out_dir)
        .map_err(|err| format!("create {} failed: {err}", options.out_dir.display()))?;

    let analysis_root = input.root.clone();
    let analysis_config = config.clone();
    let analysis_result = run_pilot_analysis_with_timeout(options.timeout_ms, move || {
        analysis::inventory_classified_seams_at_with_config(&analysis_root, &analysis_config)
    })?;
    let PilotAnalysisResult::Complete(classified) = analysis_result else {
        let context = output::pilot::PilotSummaryContext {
            root: &input.root,
            mode: &input.mode,
            config_path: config.source_path(),
            max_seams: options.max_seams,
            timeout_ms: options.timeout_ms,
            artifacts: &artifacts,
            python_first_use: None,
        };
        std::fs::write(
            &artifacts.pilot_summary_json,
            output::pilot::render_pilot_timeout_summary_json(context),
        )
        .map_err(|err| {
            format!(
                "write {} failed: {err}",
                artifacts.pilot_summary_json.display()
            )
        })?;
        std::fs::write(
            &artifacts.pilot_summary_md,
            output::pilot::render_pilot_timeout_summary_md(context),
        )
        .map_err(|err| {
            format!(
                "write {} failed: {err}",
                artifacts.pilot_summary_md.display()
            )
        })?;
        print!("{}", output::pilot::render_pilot_timeout_terminal(context));
        return Ok(());
    };

    let python_first_use = collect_pilot_python_first_use(&input, &config);
    let context = output::pilot::PilotSummaryContext {
        root: &input.root,
        mode: &input.mode,
        config_path: config.source_path(),
        max_seams: options.max_seams,
        timeout_ms: options.timeout_ms,
        artifacts: &artifacts,
        python_first_use: python_first_use.as_ref(),
    };

    std::fs::write(
        &artifacts.repo_exposure_json,
        output::repo_exposure::render_repo_exposure_json(&classified),
    )
    .map_err(|err| {
        format!(
            "write {} failed: {err}",
            artifacts.repo_exposure_json.display()
        )
    })?;
    std::fs::write(
        &artifacts.repo_exposure_md,
        output::repo_exposure::render_repo_exposure_md(&classified),
    )
    .map_err(|err| {
        format!(
            "write {} failed: {err}",
            artifacts.repo_exposure_md.display()
        )
    })?;
    std::fs::write(
        &artifacts.agent_seam_packets_json,
        output::agent_seam_packets::render_agent_seam_packets_json(&classified),
    )
    .map_err(|err| {
        format!(
            "write {} failed: {err}",
            artifacts.agent_seam_packets_json.display()
        )
    })?;

    std::fs::write(
        &artifacts.pilot_summary_json,
        output::pilot::render_pilot_summary_json(&classified, context),
    )
    .map_err(|err| {
        format!(
            "write {} failed: {err}",
            artifacts.pilot_summary_json.display()
        )
    })?;
    std::fs::write(
        &artifacts.pilot_summary_md,
        output::pilot::render_pilot_summary_md(&classified, context),
    )
    .map_err(|err| {
        format!(
            "write {} failed: {err}",
            artifacts.pilot_summary_md.display()
        )
    })?;

    print!(
        "{}",
        output::pilot::render_pilot_terminal(&classified, context)
    );
    Ok(())
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

pub(super) fn parse_pilot_options(args: &[String]) -> Result<PilotOptions, String> {
    let mut options = PilotOptions {
        root: PathBuf::from("."),
        out_dir: PathBuf::from("target/ripr/pilot"),
        mode: Mode::Draft,
        explicit: CheckInputExplicit::default(),
        max_seams: 5,
        timeout_ms: DEFAULT_PILOT_TIMEOUT_MS,
    };
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
            }
            other => return Err(format!("unknown pilot argument {other:?}")),
        }
        i += 1;
    }
    Ok(options)
}

pub(super) enum PilotAnalysisResult {
    Complete(Vec<analysis::ClassifiedSeam>),
    TimedOut,
}

pub(super) fn run_pilot_analysis_with_timeout<F>(
    timeout_ms: u64,
    runner: F,
) -> Result<PilotAnalysisResult, String>
where
    F: FnOnce() -> Result<Vec<analysis::ClassifiedSeam>, String> + Send + 'static,
{
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = runner();
        let _ignored = tx.send(result);
    });

    match rx.recv_timeout(Duration::from_millis(timeout_ms)) {
        Ok(result) => result.map(PilotAnalysisResult::Complete),
        Err(mpsc::RecvTimeoutError::Timeout) => Ok(PilotAnalysisResult::TimedOut),
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            Err("pilot analysis stopped before producing a result".to_string())
        }
    }
}

pub(crate) fn pilot_artifacts(out_dir: &Path) -> output::pilot::PilotArtifacts {
    output::pilot::PilotArtifacts {
        repo_exposure_json: out_dir.join("repo-exposure.json"),
        repo_exposure_md: out_dir.join("repo-exposure.md"),
        agent_seam_packets_json: out_dir.join("agent-seam-packets.json"),
        pilot_summary_json: out_dir.join("pilot-summary.json"),
        pilot_summary_md: out_dir.join("pilot-summary.md"),
    }
}
