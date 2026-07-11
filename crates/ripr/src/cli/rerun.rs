use crate::analysis::{
    canonical_gap::canonical_gap_identity, inventory_changed_test_classified_seams_at_with_config,
    inventory_diff_scoped_classified_seams_at_with_config,
};
use crate::cli::commands_context::{ensure_command_root, load_root_input_and_config};
use crate::cli::help;
use crate::cli::parse::expect_value;
use crate::output::gap_decision_ledger::{GapRecord, parse_gap_record_source_json};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq, Eq)]
struct RerunOptions {
    root: PathBuf,
    selector: RerunSelector,
    json: bool,
    out: Option<PathBuf>,
}

#[derive(Debug, PartialEq, Eq)]
enum RerunSelector {
    ChangedTest(PathBuf),
    Gap {
        canonical_gap_id: String,
        gap_ledger: PathBuf,
    },
}

#[derive(Serialize)]
struct TargetedRerunReport {
    schema_version: &'static str,
    state: &'static str,
    selector: TargetedRerunSelector,
    cache: TargetedRerunCache,
    seams: Vec<TargetedRerunSeam>,
    #[serde(skip_serializing_if = "Option::is_none")]
    route: Option<TargetedRerunRoute>,
    #[serde(skip_serializing_if = "Option::is_none")]
    limitation: Option<TargetedRerunLimitation>,
    authority_boundary: &'static str,
}

#[derive(Serialize)]
struct TargetedRerunSelector {
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    changed_test: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    canonical_gap_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    gap_ledger: Option<String>,
    selected_test_count: usize,
    direct_call_names: Vec<String>,
}

#[derive(Serialize)]
struct TargetedRerunCache {
    file_fact_status: String,
    hits: usize,
    misses: usize,
    corrupt_ignored: usize,
    stores: usize,
    store_errors: usize,
}

#[derive(Serialize)]
struct TargetedRerunSeam {
    canonical_gap_id: Option<String>,
    seam_id: String,
    file: String,
    line: usize,
    owner: String,
    static_class: String,
}

#[derive(Serialize)]
struct TargetedRerunRoute {
    verify_commands: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    receipt_command: Option<String>,
}

#[derive(Serialize)]
struct TargetedRerunLimitation {
    kind: &'static str,
    message: String,
}

pub(super) fn run(args: &[String]) -> Result<(), String> {
    if args
        .iter()
        .any(|arg| matches!(arg.as_str(), "--help" | "-h"))
    {
        help::print_rerun_help();
        return Ok(());
    }
    let options = parse_options(args)?;
    ensure_command_root(&options.root, "rerun")?;
    let (input, config) = load_root_input_and_config(&options.root)?;
    let report = match options.selector {
        RerunSelector::ChangedTest(changed_test) => {
            rerun_changed_test(&input.root, &config, &changed_test)?
        }
        RerunSelector::Gap {
            canonical_gap_id,
            gap_ledger,
        } => rerun_gap(&input.root, &config, &canonical_gap_id, &gap_ledger)?,
    };
    let rendered = if options.json {
        serde_json::to_string_pretty(&report)
            .map_err(|err| format!("serialize targeted rerun report failed: {err}"))?
    } else {
        render_human(&report)
    };
    if let Some(out) = options.out {
        let out = resolve_output_path(&input.root, &out);
        write_text_file(&out, &rendered)?;
        println!("Wrote {}", out.display());
    } else {
        println!("{rendered}");
    }
    Ok(())
}

fn parse_options(args: &[String]) -> Result<RerunOptions, String> {
    let mut root = PathBuf::from(".");
    let mut changed_test = None;
    let mut canonical_gap_id = None;
    let mut gap_ledger = None;
    let mut json = false;
    let mut out = None;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--root" => {
                index += 1;
                root = PathBuf::from(expect_value(args, index, "--root")?);
            }
            "--changed-test" => {
                index += 1;
                if changed_test.is_some() || canonical_gap_id.is_some() {
                    return Err("rerun accepts exactly one selector".to_string());
                }
                changed_test = Some(PathBuf::from(expect_value(args, index, "--changed-test")?));
            }
            "--gap" => {
                index += 1;
                if changed_test.is_some() || canonical_gap_id.is_some() {
                    return Err("rerun accepts exactly one selector".to_string());
                }
                canonical_gap_id = Some(expect_value(args, index, "--gap")?.to_string());
            }
            "--gap-ledger" => {
                index += 1;
                gap_ledger = Some(PathBuf::from(expect_value(args, index, "--gap-ledger")?));
            }
            "--json" => json = true,
            "--out" => {
                index += 1;
                out = Some(PathBuf::from(expect_value(args, index, "--out")?));
            }
            other => return Err(format!("unknown rerun argument {other:?}")),
        }
        index += 1;
    }
    let selector = match (changed_test, canonical_gap_id, gap_ledger) {
        (Some(changed_test), None, None) => RerunSelector::ChangedTest(changed_test),
        (Some(_), None, Some(_)) => {
            return Err("rerun --gap-ledger requires --gap".to_string());
        }
        (None, Some(canonical_gap_id), Some(gap_ledger)) => RerunSelector::Gap {
            canonical_gap_id,
            gap_ledger,
        },
        (None, Some(_), None) => {
            return Err("rerun --gap requires --gap-ledger <path>".to_string());
        }
        (None, None, Some(_)) => {
            return Err("rerun --gap-ledger requires --gap".to_string());
        }
        (None, None, None) => {
            return Err("rerun requires --changed-test <path> or --gap <canonical-gap-id> --gap-ledger <path>".to_string());
        }
        (Some(_), Some(_), _) => return Err("rerun accepts exactly one selector".to_string()),
    };
    Ok(RerunOptions {
        root,
        selector,
        json,
        out,
    })
}

fn rerun_changed_test(
    root: &Path,
    config: &crate::config::RiprConfig,
    changed_test: &Path,
) -> Result<TargetedRerunReport, String> {
    let changed_test = normalize_changed_test(root, changed_test)?;
    let inventory =
        inventory_changed_test_classified_seams_at_with_config(root, config, &changed_test)?;
    Ok(report(
        "current_state_only",
        TargetedRerunSelector {
            kind: "changed_test",
            changed_test: Some(display_path(&changed_test)),
            canonical_gap_id: None,
            gap_ledger: None,
            selected_test_count: inventory.selected_test_count,
            direct_call_names: inventory.direct_call_names,
        },
        cache_from(&inventory.file_fact_cache),
        seams_from(&inventory.classified),
        None,
        None,
    ))
}

fn rerun_gap(
    root: &Path,
    config: &crate::config::RiprConfig,
    canonical_gap_id: &str,
    gap_ledger: &Path,
) -> Result<TargetedRerunReport, String> {
    let gap_ledger = resolve_output_path(root, gap_ledger);
    let selector = TargetedRerunSelector {
        kind: "canonical_gap",
        changed_test: None,
        canonical_gap_id: Some(canonical_gap_id.to_string()),
        gap_ledger: Some(display_path(&gap_ledger)),
        selected_test_count: 0,
        direct_call_names: Vec::new(),
    };
    let contents = match std::fs::read_to_string(&gap_ledger) {
        Ok(contents) => contents,
        Err(err) => {
            return Ok(limited_report(
                selector,
                "canonical_gap_unresolved",
                format!("read gap ledger {} failed: {err}", gap_ledger.display()),
            ));
        }
    };
    let source = match parse_gap_record_source_json(&contents) {
        Ok(source) => source,
        Err(err) => {
            return Ok(limited_report(
                selector,
                "canonical_gap_unresolved",
                format!("parse gap ledger {} failed: {err}", gap_ledger.display()),
            ));
        }
    };
    if let Some(ledger_root) = source
        .root
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        && !same_root(root, Path::new(ledger_root))
    {
        return Ok(limited_report(
            selector,
            "stale_gap_ledger",
            format!(
                "gap ledger root `{ledger_root}` does not match rerun root `{}`",
                root.display()
            ),
        ));
    }
    let record = match resolve_gap_record(source.records, canonical_gap_id) {
        Ok(record) => record,
        Err(limitation) => {
            return Ok(limited_report(
                selector,
                limitation.kind,
                limitation.message,
            ));
        }
    };
    let Some(anchor) = record.anchor.as_ref() else {
        return Ok(limited_report(
            selector,
            "canonical_gap_unresolved",
            format!("gap ledger record `{canonical_gap_id}` has no source anchor"),
        ));
    };
    let Some(file) = anchor
        .file
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    else {
        return Ok(limited_report(
            selector,
            "canonical_gap_unresolved",
            format!("gap ledger record `{canonical_gap_id}` has no anchored file"),
        ));
    };
    let changed_files = vec![PathBuf::from(file)];
    let changed_owner_names = anchor.owner.iter().cloned().collect::<Vec<_>>();
    let inventory = inventory_diff_scoped_classified_seams_at_with_config(
        root,
        config,
        &changed_files,
        &changed_owner_names,
    )?;
    let seams = inventory
        .classified
        .iter()
        .filter(|entry| {
            canonical_gap_identity(entry).is_some_and(|identity| identity.id == canonical_gap_id)
        })
        .collect::<Vec<_>>();
    if seams.is_empty() {
        return Ok(limited_report(
            selector,
            "canonical_gap_unresolved",
            format!(
                "no current seam matched canonical gap `{canonical_gap_id}` from the supplied ledger"
            ),
        ));
    }
    Ok(report(
        "current_state_only",
        selector,
        cache_from(&inventory.file_fact_cache),
        seams_from_refs(&seams),
        Some(TargetedRerunRoute {
            verify_commands: record.verification_commands,
            receipt_command: record.receipt_command,
        }),
        None,
    ))
}

fn resolve_gap_record(
    records: Vec<GapRecord>,
    canonical_gap_id: &str,
) -> Result<GapRecord, TargetedRerunLimitation> {
    let mut matches = records
        .into_iter()
        .filter(|record| record.canonical_gap_id == canonical_gap_id)
        .collect::<Vec<_>>();
    match matches.len() {
        0 => Err(TargetedRerunLimitation {
            kind: "canonical_gap_unresolved",
            message: format!("no gap ledger record has canonical_gap_id `{canonical_gap_id}`"),
        }),
        1 => Ok(matches.remove(0)),
        count => Err(TargetedRerunLimitation {
            kind: "canonical_gap_ambiguous",
            message: format!(
                "{count} gap ledger records have canonical_gap_id `{canonical_gap_id}`"
            ),
        }),
    }
}

fn same_root(root: &Path, ledger_root: &Path) -> bool {
    let ledger_root = if ledger_root.is_absolute() {
        ledger_root.to_path_buf()
    } else {
        root.join(ledger_root)
    };
    std::fs::canonicalize(root).ok() == std::fs::canonicalize(ledger_root).ok()
}

fn report(
    state: &'static str,
    selector: TargetedRerunSelector,
    cache: TargetedRerunCache,
    seams: Vec<TargetedRerunSeam>,
    route: Option<TargetedRerunRoute>,
    limitation: Option<TargetedRerunLimitation>,
) -> TargetedRerunReport {
    TargetedRerunReport {
        schema_version: "ripr-targeted-rerun-v1",
        state,
        selector,
        cache,
        seams,
        route,
        limitation,
        authority_boundary: "static evidence only; no before snapshot was supplied, so gap movement is not inferred",
    }
}

fn limited_report(
    selector: TargetedRerunSelector,
    kind: &'static str,
    message: String,
) -> TargetedRerunReport {
    report(
        "limited",
        selector,
        TargetedRerunCache {
            file_fact_status: "not_run".to_string(),
            hits: 0,
            misses: 0,
            corrupt_ignored: 0,
            stores: 0,
            store_errors: 0,
        },
        Vec::new(),
        None,
        Some(TargetedRerunLimitation { kind, message }),
    )
}

fn cache_from(cache: &crate::analysis::seam_cache::FileFactCacheStats) -> TargetedRerunCache {
    TargetedRerunCache {
        file_fact_status: cache.status_label(),
        hits: cache.hits,
        misses: cache.misses,
        corrupt_ignored: cache.corrupt_ignored,
        stores: cache.stores,
        store_errors: cache.store_errors,
    }
}

fn seams_from(entries: &[crate::analysis::ClassifiedSeam]) -> Vec<TargetedRerunSeam> {
    entries.iter().map(seam_from).collect()
}

fn seams_from_refs(entries: &[&crate::analysis::ClassifiedSeam]) -> Vec<TargetedRerunSeam> {
    entries.iter().map(|entry| seam_from(entry)).collect()
}

fn seam_from(entry: &crate::analysis::ClassifiedSeam) -> TargetedRerunSeam {
    TargetedRerunSeam {
        canonical_gap_id: canonical_gap_identity(entry).map(|identity| identity.id),
        seam_id: entry.seam.id().as_str().to_string(),
        file: display_path(entry.seam.file()),
        line: entry.seam.display_line(),
        owner: entry.seam.owner().to_string(),
        static_class: entry.class.as_str().to_string(),
    }
}

fn normalize_changed_test(root: &Path, changed_test: &Path) -> Result<PathBuf, String> {
    if changed_test.is_absolute() {
        return changed_test
            .strip_prefix(root)
            .map(PathBuf::from)
            .map_err(|err| {
                format!(
                    "rerun changed test {} is outside root {}: {err}",
                    changed_test.display(),
                    root.display()
                )
            });
    }
    Ok(changed_test.to_path_buf())
}

fn resolve_output_path(root: &Path, out: &Path) -> PathBuf {
    if out.is_absolute() {
        out.to_path_buf()
    } else {
        root.join(out)
    }
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn write_text_file(path: &Path, rendered: &str) -> Result<(), String> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("create {} failed: {err}", parent.display()))?;
    }
    std::fs::write(path, rendered).map_err(|err| format!("write {} failed: {err}", path.display()))
}

fn render_human(report: &TargetedRerunReport) -> String {
    let mut lines = vec![
        format!("State: {}", report.state),
        format!("Selector: {}", report.selector.kind),
        format!("Selected tests: {}", report.selector.selected_test_count),
        format!(
            "Direct call names: {}",
            report.selector.direct_call_names.join(", ")
        ),
        format!(
            "File-fact cache: {} (hits {}, misses {})",
            report.cache.file_fact_status, report.cache.hits, report.cache.misses
        ),
        "Recomputed seams:".to_string(),
    ];
    if let Some(changed_test) = report.selector.changed_test.as_deref() {
        lines.push(format!("Changed test: {changed_test}"));
    }
    if let Some(canonical_gap_id) = report.selector.canonical_gap_id.as_deref() {
        lines.push(format!("Canonical gap: {canonical_gap_id}"));
    }
    if let Some(gap_ledger) = report.selector.gap_ledger.as_deref() {
        lines.push(format!("Gap ledger: {gap_ledger}"));
    }
    lines.extend(report.seams.iter().map(|seam| {
        format!(
            "  - [{}] {}:{} {} ({})",
            seam.static_class, seam.file, seam.line, seam.owner, seam.seam_id
        )
    }));
    if let Some(route) = report.route.as_ref() {
        if !route.verify_commands.is_empty() {
            lines.push(format!("Verify: {}", route.verify_commands.join(" && ")));
        }
        if let Some(receipt_command) = route.receipt_command.as_deref() {
            lines.push(format!("Receipt: {receipt_command}"));
        }
    }
    if let Some(limitation) = report.limitation.as_ref() {
        lines.push(format!(
            "Limitation ({}): {}",
            limitation.kind, limitation.message
        ));
    }
    lines.push(format!("Boundary: {}", report.authority_boundary));
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::{
        RerunSelector, TargetedRerunCache, TargetedRerunReport, TargetedRerunSeam,
        TargetedRerunSelector, parse_options, render_human,
    };
    use std::path::{Path, PathBuf};

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn rerun_requires_one_selector() {
        assert_eq!(
            parse_options(&[]),
            Err(
                "rerun requires --changed-test <path> or --gap <canonical-gap-id> --gap-ledger <path>"
                    .to_string()
            )
        );
    }

    #[test]
    fn rerun_gap_requires_explicit_ledger() {
        assert_eq!(
            parse_options(&args(&["--gap", "gap:example"])),
            Err("rerun --gap requires --gap-ledger <path>".to_string())
        );
    }

    #[test]
    fn rerun_parses_gap_selector_with_explicit_ledger() -> Result<(), String> {
        let options = parse_options(&args(&[
            "--gap",
            "gap:example",
            "--gap-ledger",
            "target/ripr/gaps.json",
        ]))?;
        if options.selector
            != (RerunSelector::Gap {
                canonical_gap_id: "gap:example".to_string(),
                gap_ledger: PathBuf::from("target/ripr/gaps.json"),
            })
        {
            return Err(format!("unexpected rerun options: {options:?}"));
        }
        Ok(())
    }

    #[test]
    fn rerun_rejects_multiple_changed_test_selectors() {
        assert_eq!(
            parse_options(&args(&[
                "--changed-test",
                "tests/first.rs",
                "--changed-test",
                "tests/second.rs",
            ])),
            Err("rerun accepts exactly one selector".to_string())
        );
    }

    #[test]
    fn rerun_parses_changed_test_json_output() -> Result<(), String> {
        let options = parse_options(&args(&[
            "--root",
            "workspace",
            "--changed-test",
            "tests/pricing.rs",
            "--json",
            "--out",
            "target/rerun.json",
        ]))?;
        if options.root.as_path() != Path::new("workspace")
            || options.selector != RerunSelector::ChangedTest(PathBuf::from("tests/pricing.rs"))
            || !options.json
            || options.out != Some(PathBuf::from("target/rerun.json"))
        {
            return Err(format!("unexpected rerun options: {options:?}"));
        }
        Ok(())
    }

    #[test]
    fn human_rerun_report_names_current_state_only_boundary() -> Result<(), String> {
        let report = TargetedRerunReport {
            schema_version: "ripr-targeted-rerun-v1",
            state: "current_state_only",
            selector: TargetedRerunSelector {
                kind: "changed_test",
                changed_test: Some("tests/pricing.rs".to_string()),
                canonical_gap_id: None,
                gap_ledger: None,
                selected_test_count: 1,
                direct_call_names: vec!["discounted_total".to_string()],
            },
            cache: TargetedRerunCache {
                file_fact_status: "warm".to_string(),
                hits: 2,
                misses: 0,
                corrupt_ignored: 0,
                stores: 0,
                store_errors: 0,
            },
            seams: vec![TargetedRerunSeam {
                canonical_gap_id: Some("gap:example".to_string()),
                seam_id: "seam:example".to_string(),
                file: "src/lib.rs".to_string(),
                line: 8,
                owner: "pricing::discounted_total".to_string(),
                static_class: "weakly_gripped".to_string(),
            }],
            route: None,
            limitation: None,
            authority_boundary: "static evidence only; no before snapshot was supplied, so gap movement is not inferred",
        };
        let rendered = render_human(&report);
        for expected in [
            "State: current_state_only",
            "File-fact cache: warm",
            "static evidence only",
        ] {
            if !rendered.contains(expected) {
                return Err(format!("missing {expected:?} from {rendered:?}"));
            }
        }
        Ok(())
    }
}
