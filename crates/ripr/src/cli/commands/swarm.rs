use crate::cli::commands_context::ensure_command_root;
use crate::cli::commands_numeric::parse_positive_usize;
use crate::cli::parse::expect_value;
use crate::output;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SwarmQueueOptions {
    pub(super) root: PathBuf,
    pub(super) gap_ledger: PathBuf,
    pub(super) language: String,
    pub(super) top: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SwarmIngestOptions {
    pub(super) root: PathBuf,
    pub(super) result: PathBuf,
}

pub(super) fn parse_swarm_queue_options(args: &[String]) -> Result<SwarmQueueOptions, String> {
    let mut root = PathBuf::from(".");
    let mut gap_ledger = PathBuf::from("target/ripr/reports/gap-decision-ledger.json");
    let mut language = "python".to_string();
    let mut top = 10usize;

    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--root" => {
                i += 1;
                let value = expect_value(args, i, "--root")?;
                if value.trim().is_empty() {
                    return Err("swarm queue --root requires a non-empty path".to_string());
                }
                root = PathBuf::from(value);
            }
            "--gap-ledger" => {
                i += 1;
                let value = expect_value(args, i, "--gap-ledger")?;
                if value.trim().is_empty() {
                    return Err("swarm queue --gap-ledger requires a non-empty path".to_string());
                }
                gap_ledger = PathBuf::from(value);
            }
            "--language" => {
                i += 1;
                let value = expect_value(args, i, "--language")?;
                if value.trim().is_empty() {
                    return Err("swarm queue --language requires a non-empty language".to_string());
                }
                language = value.to_string();
            }
            "--top" => {
                i += 1;
                top = parse_positive_usize(expect_value(args, i, "--top")?, "swarm queue --top")?;
            }
            "--format" => {
                i += 1;
                let value = expect_value(args, i, "--format")?;
                if value != "json" {
                    return Err(format!(
                        "unknown swarm queue format {value:?}; expected `json`"
                    ));
                }
            }
            "--json" => {}
            other => return Err(format!("unknown swarm queue argument {other:?}")),
        }
        i += 1;
    }

    Ok(SwarmQueueOptions {
        root,
        gap_ledger,
        language,
        top,
    })
}

pub(super) fn parse_swarm_ingest_options(args: &[String]) -> Result<SwarmIngestOptions, String> {
    let mut root = PathBuf::from(".");
    let mut result = None;

    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--root" => {
                i += 1;
                let value = expect_value(args, i, "--root")?;
                if value.trim().is_empty() {
                    return Err("swarm ingest --root requires a non-empty path".to_string());
                }
                root = PathBuf::from(value);
            }
            "--result" => {
                i += 1;
                let value = expect_value(args, i, "--result")?;
                if value.trim().is_empty() {
                    return Err("swarm ingest --result requires a non-empty path".to_string());
                }
                result = Some(PathBuf::from(value));
            }
            "--format" => {
                i += 1;
                let value = expect_value(args, i, "--format")?;
                if value != "json" {
                    return Err(format!(
                        "unknown swarm ingest format {value:?}; expected `json`"
                    ));
                }
            }
            "--json" => {}
            other => return Err(format!("unknown swarm ingest argument {other:?}")),
        }
        i += 1;
    }

    Ok(SwarmIngestOptions {
        root,
        result: result.ok_or_else(|| "swarm ingest requires --result <path>".to_string())?,
    })
}

pub(super) fn run_swarm_queue(options: SwarmQueueOptions) -> Result<(), String> {
    ensure_command_root(&options.root, "swarm queue")?;
    let contents = std::fs::read_to_string(&options.gap_ledger).map_err(|err| {
        format!(
            "swarm queue --gap-ledger {} is invalid: read failed: {err}",
            options.gap_ledger.display()
        )
    })?;
    let rendered = render_swarm_queue_from_gap_ledger_contents(&options, &contents)?;
    print!("{rendered}");
    Ok(())
}

pub(super) fn render_swarm_queue_from_gap_ledger_contents(
    options: &SwarmQueueOptions,
    contents: &str,
) -> Result<String, String> {
    let source =
        output::gap_decision_ledger::parse_gap_record_source_json(contents).map_err(|err| {
            format!(
                "swarm queue --gap-ledger {} is invalid: {err}",
                options.gap_ledger.display()
            )
        })?;
    let root_display = output::outcome::display_path(&options.root);
    let gap_ledger_display = output::outcome::display_path(&options.gap_ledger);
    let rendered = match gap_ledger_root_status(&options.root, source.root.as_deref()) {
        GapLedgerRootStatus::Missing => {
            output::agent_seam_packets::render_agent_gap_record_queue_missing_root_json(
                &root_display,
                &gap_ledger_display,
                source.generated_at.as_deref(),
                &source.records,
                &options.language,
                options.top,
            )?
        }
        GapLedgerRootStatus::Mismatch { ledger_root, .. } => {
            output::agent_seam_packets::render_agent_gap_record_queue_wrong_root_json(
                &root_display,
                &gap_ledger_display,
                &ledger_root,
                source.generated_at.as_deref(),
                &source.records,
                &options.language,
                options.top,
            )?
        }
        GapLedgerRootStatus::Match => {
            output::agent_seam_packets::render_agent_gap_record_queue_json(
                &root_display,
                &gap_ledger_display,
                &source.records,
                &options.language,
                options.top,
            )?
        }
    };
    Ok(rendered)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum GapLedgerRootStatus {
    Match,
    Missing,
    Mismatch { ledger_root: String, reason: String },
}

pub(super) fn gap_ledger_root_status(
    requested_root: &Path,
    ledger_root: Option<&str>,
) -> GapLedgerRootStatus {
    let Some(raw_ledger_root) = ledger_root.map(str::trim).filter(|root| !root.is_empty()) else {
        return GapLedgerRootStatus::Missing;
    };
    let requested_root_display = output::outcome::display_path(requested_root);
    let ledger_root_display = output::path::display_path_text(raw_ledger_root);
    if requested_root_display == ledger_root_display {
        return GapLedgerRootStatus::Match;
    }

    let requested_canonical = requested_root.canonicalize().ok();
    let ledger_root_path = Path::new(raw_ledger_root);
    let ledger_canonical = ledger_root_path.canonicalize().ok();
    if requested_canonical.is_some()
        && ledger_canonical.is_some()
        && requested_canonical == ledger_canonical
    {
        return GapLedgerRootStatus::Match;
    }

    GapLedgerRootStatus::Mismatch {
        ledger_root: ledger_root_display.clone(),
        reason: format!(
            "gap ledger root {ledger_root_display} does not match requested --root {requested_root_display}; regenerate the gap decision ledger for the selected root before assigning swarm work"
        ),
    }
}

pub(super) fn run_swarm_ingest(options: SwarmIngestOptions) -> Result<(), String> {
    ensure_command_root(&options.root, "swarm ingest")?;
    let result_path = validate_swarm_ingest_result_path(&options.root, &options.result)?;
    let contents = std::fs::read_to_string(&result_path).map_err(|err| {
        format!(
            "read swarm ingest --result {} failed: {err}",
            options.result.display()
        )
    })?;
    let rendered = output::swarm_ingest::render_swarm_ingest_json(
        &contents,
        &output::outcome::display_path(&options.result),
    )?;
    print!("{rendered}");
    Ok(())
}

pub(super) fn validate_swarm_ingest_result_path(
    root: &Path,
    path: &Path,
) -> Result<PathBuf, String> {
    let root = root.canonicalize().map_err(|err| {
        format!(
            "canonicalize swarm ingest root {} failed: {err}",
            root.display()
        )
    })?;
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    let candidate = candidate.canonicalize().map_err(|err| {
        format!(
            "canonicalize swarm ingest --result {} failed: {err}",
            path.display()
        )
    })?;

    if !candidate.starts_with(&root) {
        return Err(format!(
            "swarm ingest --result {} must stay under root {}",
            path.display(),
            root.display()
        ));
    }

    Ok(candidate)
}
