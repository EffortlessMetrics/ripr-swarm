use super::Mode;
use super::{
    CHANNELS, Cell, Disposition, PLATFORMS, build_receipt, parse_args, parse_cell,
    parse_dispositions, render_markdown,
};

fn works(channel: &str, platform: &str, seconds: f64) -> Cell {
    Cell {
        channel: channel.to_string(),
        platform: platform.to_string(),
        status: "works".to_string(),
        version: Some("0.10.0".to_string()),
        install_seconds: Some(seconds),
    }
}

fn full_matrix(seconds: f64) -> Vec<Cell> {
    CHANNELS
        .iter()
        .flat_map(|channel| {
            PLATFORMS
                .iter()
                .map(|platform| works(channel, platform, seconds))
        })
        .collect()
}

fn tested() -> Vec<Disposition> {
    vec![Disposition {
        channel: "archive".to_string(),
        status: "tested_elsewhere".to_string(),
        reason: "first-run-archive covers all five platforms".to_string(),
    }]
}

fn verdict(receipt: &serde_json::Value) -> String {
    receipt["r0_install"]["status"]
        .as_str()
        .unwrap_or("")
        .to_string()
}

#[test]
fn every_cell_working_within_the_ceiling_with_no_untested_channel_is_met() -> Result<(), String> {
    let receipt = build_receipt(&full_matrix(120.0), &tested(), Some("42"))?;
    assert_eq!(verdict(&receipt), "met");
    assert_eq!(receipt["cells"].as_array().map(Vec::len), Some(15));
    Ok(())
}

#[test]
fn one_slow_cell_makes_the_row_not_met_and_names_it() -> Result<(), String> {
    let mut cells = full_matrix(120.0);
    for cell in &mut cells {
        if cell.channel == "cargo" && cell.platform == "macos-latest" {
            cell.install_seconds = Some(301.0);
        }
    }
    let receipt = build_receipt(&cells, &tested(), None)?;
    assert_eq!(verdict(&receipt), "not_met");
    let reasons = receipt["r0_install"]["reasons"].to_string();
    assert!(
        reasons.contains("cargo on macos-latest took 301 s"),
        "{reasons}"
    );
    Ok(())
}

#[test]
fn a_failed_or_broken_cell_is_not_met_even_when_the_rest_are_fast() -> Result<(), String> {
    for status in ["failed", "broken"] {
        let mut cells = full_matrix(10.0);
        cells[3].status = status.to_string();
        cells[3].install_seconds = None;
        let receipt = build_receipt(&cells, &tested(), None)?;
        assert_eq!(verdict(&receipt), "not_met", "{status}");
    }
    Ok(())
}

#[test]
fn a_documented_gap_or_an_untested_channel_is_not_established_never_met() -> Result<(), String> {
    let mut cells = full_matrix(10.0);
    cells[5].status = "known_gap".to_string();
    cells[5].version = None;
    cells[5].install_seconds = None;
    let receipt = build_receipt(&cells, &tested(), None)?;
    assert_eq!(verdict(&receipt), "not_established");

    let unprobed = vec![Disposition {
        channel: "vscode_extension".to_string(),
        status: "not_tested".to_string(),
        reason: "needs a desktop host".to_string(),
    }];
    let receipt = build_receipt(&full_matrix(10.0), &unprobed, None)?;
    assert_eq!(verdict(&receipt), "not_established");
    Ok(())
}

#[test]
fn a_slow_cell_outranks_a_gap_so_the_row_reports_the_real_failure() -> Result<(), String> {
    let mut cells = full_matrix(10.0);
    cells[0].install_seconds = Some(999.0);
    cells[5].status = "known_gap".to_string();
    cells[5].version = None;
    cells[5].install_seconds = None;
    assert_eq!(verdict(&build_receipt(&cells, &tested(), None)?), "not_met");
    Ok(())
}

#[test]
fn a_missing_duplicate_or_unknown_cell_is_refused() {
    let mut cells = full_matrix(10.0);
    cells.pop();
    let err = build_receipt(&cells, &tested(), None)
        .err()
        .unwrap_or_default();
    assert!(
        err.contains("missing matrix cells: npm on windows-latest"),
        "{err}"
    );

    let mut cells = full_matrix(10.0);
    cells.push(works("cargo", "ubuntu-latest", 5.0));
    let err = build_receipt(&cells, &tested(), None)
        .err()
        .unwrap_or_default();
    assert!(
        err.contains("duplicate cell cargo on ubuntu-latest"),
        "{err}"
    );

    let mut cells = full_matrix(10.0);
    cells[0].platform = "solaris".to_string();
    let err = build_receipt(&cells, &tested(), None)
        .err()
        .unwrap_or_default();
    assert!(err.contains("unknown platform `solaris`"), "{err}");
}

#[test]
fn cell_files_are_validated_one_by_one() -> Result<(), String> {
    let ok = r#"{"channel":"cargo","os":"macos-latest","status":"works","version":"0.10.0","install_seconds":142.5}"#;
    let cell = parse_cell(ok, "ok.json")?;
    assert_eq!(cell.install_seconds, Some(142.5));
    assert_eq!(cell.platform, "macos-latest");

    let bad_status = ok.replace("works", "fine");
    let err = parse_cell(&bad_status, "x.json").err().unwrap_or_default();
    assert!(err.contains("unknown status `fine`"), "{err}");

    let no_seconds =
        r#"{"channel":"cargo","os":"macos-latest","status":"works","version":"0.10.0"}"#;
    let err = parse_cell(no_seconds, "x.json").err().unwrap_or_default();
    assert!(
        err.contains("must record its version and install seconds"),
        "{err}"
    );

    let negative = ok.replace("142.5", "-1");
    let err = parse_cell(&negative, "x.json").err().unwrap_or_default();
    assert!(err.contains("non-negative"), "{err}");

    let gap = r#"{"channel":"pypi","os":"macos-latest","status":"known_gap"}"#;
    assert_eq!(parse_cell(gap, "gap.json")?.version, None);
    Ok(())
}

#[test]
fn dispositions_need_a_known_status_and_a_reason() -> Result<(), String> {
    let good =
        r#"{"channels":[{"channel":"homebrew","status":"not_shipped","reason":"no tap yet"}]}"#;
    assert_eq!(parse_dispositions(good, "d.json")?.len(), 1);
    let doubled = good.replace(
        "]}",
        ",{\"channel\":\"homebrew\",\"status\":\"not_shipped\",\"reason\":\"again\"}]}",
    );
    assert!(parse_dispositions(&doubled, "d.json").err().is_some());
    let matrix_name = good.replace("homebrew", "cargo");
    assert!(parse_dispositions(&matrix_name, "d.json").err().is_some());
    let bad = good.replace("not_shipped", "maybe");
    assert!(parse_dispositions(&bad, "d.json").err().is_some());
    let empty = good.replace("no tap yet", "  ");
    assert!(parse_dispositions(&empty, "d.json").err().is_some());
    assert!(
        parse_dispositions(r#"{"channels":[]}"#, "d.json")
            .err()
            .is_some()
    );
    Ok(())
}

#[test]
fn markdown_names_the_verdict_the_run_and_each_untested_channel() -> Result<(), String> {
    let unprobed = vec![Disposition {
        channel: "binstall".to_string(),
        status: "not_tested".to_string(),
        reason: "falls back to source".to_string(),
    }];
    let receipt = build_receipt(&full_matrix(30.0), &unprobed, Some("777"))?;
    let md = render_markdown(&receipt);
    assert!(md.contains("**not_established**"), "{md}");
    assert!(md.contains("matrix run 777"), "{md}");
    assert!(
        md.contains("| binstall | not_tested | falls back to source |"),
        "{md}"
    );
    assert!(md.contains("channel binstall is not tested"), "{md}");
    Ok(())
}

#[test]
fn arguments_select_check_or_build_and_reject_the_rest() {
    let strings = |items: &[&str]| items.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert_eq!(parse_args(&[]), Ok(Mode::Check));
    assert_eq!(parse_args(&strings(&["--check"])), Ok(Mode::Check));
    assert!(matches!(
        parse_args(&strings(&["--cells", "d", "--run-id", "12", "--write"])),
        Ok(Mode::Build { write: true, .. })
    ));
    assert!(parse_args(&strings(&["--write"])).err().is_some());
    assert!(
        parse_args(&strings(&["--cells", "d", "--run-id", "x"]))
            .err()
            .is_some()
    );
    assert!(
        parse_args(&strings(&["--cells", "d", "--bogus"]))
            .err()
            .is_some()
    );
}

fn all_dispositions() -> Vec<Disposition> {
    super::OTHER_CHANNELS
        .iter()
        .map(|channel| Disposition {
            channel: (*channel).to_string(),
            status: "not_shipped".to_string(),
            reason: "stated in the test".to_string(),
        })
        .collect()
}

/// Write a consistent receipt tree into a fresh temp directory.
fn receipt_tree(name: &str, seconds: f64) -> Result<std::path::PathBuf, String> {
    let root = std::env::temp_dir().join(format!(
        "ripr-install-receipt-{name}-{}",
        std::process::id()
    ));
    let dir = root.join("metrics/install");
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let dispositions = all_dispositions();
    let list: Vec<serde_json::Value> = dispositions
        .iter()
        .map(|d| serde_json::json!({"channel": d.channel, "status": d.status, "reason": d.reason}))
        .collect();
    let text = serde_json::json!({ "channels": list }).to_string();
    std::fs::write(root.join(super::DISPOSITIONS), text).map_err(|error| error.to_string())?;
    let receipt = build_receipt(&full_matrix(seconds), &dispositions, Some("42"))?;
    std::fs::write(
        root.join(super::RECEIPT),
        serde_json::to_string_pretty(&receipt).map_err(|e| e.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(root.join(super::RECEIPT_MD), render_markdown(&receipt))
        .map_err(|error| error.to_string())?;
    Ok(root)
}

fn edit(root: &std::path::Path, file: &str, from: &str, to: &str) -> Result<(), String> {
    let path = root.join(file);
    let text = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
    if !text.contains(from) {
        return Err(format!("fixture edit `{from}` not found in {file}"));
    }
    std::fs::write(&path, text.replacen(from, to, 1)).map_err(|error| error.to_string())
}

#[test]
fn check_accepts_a_receipt_its_rows_support() -> Result<(), String> {
    let root = receipt_tree("ok", 10.0)?;
    let result = super::check_receipt(&root);
    let _ = std::fs::remove_dir_all(&root);
    result
}

#[test]
fn check_refuses_a_verdict_its_rows_do_not_support() -> Result<(), String> {
    let root = receipt_tree("verdict", 400.0)?;
    edit(&root, super::RECEIPT, "\"not_established\"", "\"met\"")
        .or_else(|_| edit(&root, super::RECEIPT, "\"not_met\"", "\"met\""))?;
    let result = super::check_receipt(&root);
    let _ = std::fs::remove_dir_all(&root);
    assert!(result.err().is_some_and(|e| e.contains("r0_install")));
    Ok(())
}

#[test]
fn check_refuses_an_edited_threshold_missing_run_or_stale_markdown() -> Result<(), String> {
    for (name, file, from, to, expect) in [
        (
            "threshold",
            super::RECEIPT,
            "\"threshold_seconds\": 300.0",
            "\"threshold_seconds\": 9999.0",
            "threshold_seconds",
        ),
        (
            "run",
            super::RECEIPT,
            "\"source_run_id\": \"42\"",
            "\"source_run_id\": null",
            "source_run_id",
        ),
        (
            "markdown",
            super::RECEIPT_MD,
            "R0 install row",
            "R0 install row (edited)",
            "does not match",
        ),
        (
            "dispositions",
            super::DISPOSITIONS,
            "stated in the test",
            "changed",
            "stale",
        ),
    ] {
        let root = receipt_tree(name, 10.0)?;
        edit(&root, file, from, to)?;
        let result = super::check_receipt(&root);
        let _ = std::fs::remove_dir_all(&root);
        assert!(
            result.err().is_some_and(|e| e.contains(expect)),
            "{name} edit was not refused"
        );
    }
    Ok(())
}

#[test]
fn dispositions_file_must_account_for_every_other_channel() -> Result<(), String> {
    let root = receipt_tree("missing", 10.0)?;
    edit(&root, super::DISPOSITIONS, "\"homebrew\"", "\"archive\"")?;
    let result = super::read_dispositions(&root.join(super::DISPOSITIONS));
    let _ = std::fs::remove_dir_all(&root);
    assert!(result.err().is_some());
    Ok(())
}
