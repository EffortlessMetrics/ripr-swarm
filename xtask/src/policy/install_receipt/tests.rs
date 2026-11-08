use super::Mode;
use super::{
    CHANNELS, Cell, Disposition, PLATFORMS, build_receipt, median, parse_args, parse_cell,
    parse_dispositions, render_markdown,
};

fn works(channel: &str, platform: &str, seconds: f64, run: &str) -> Cell {
    Cell {
        channel: channel.to_string(),
        platform: platform.to_string(),
        status: "works".to_string(),
        version: Some("ripr 0.10.0".to_string()),
        install_seconds: Some(seconds),
        run_id: run.to_string(),
    }
}

/// Every channel x platform cell working in `seconds`, once per run in `runs`.
fn matrix(seconds: f64, runs: &[&str]) -> Vec<Cell> {
    runs.iter()
        .flat_map(|run| {
            CHANNELS.iter().flat_map(move |channel| {
                PLATFORMS
                    .iter()
                    .map(move |platform| works(channel, platform, seconds, run))
            })
        })
        .collect()
}

const THREE_RUNS: [&str; 3] = ["101", "102", "103"];

fn shipped() -> Vec<Disposition> {
    vec![Disposition {
        channel: "homebrew".to_string(),
        status: "not_shipped".to_string(),
        reason: "no tap".to_string(),
    }]
}

fn verdict(receipt: &serde_json::Value) -> String {
    receipt["r0_install"]["status"]
        .as_str()
        .unwrap_or("")
        .to_string()
}

fn reasons(receipt: &serde_json::Value) -> String {
    receipt["r0_install"]["reasons"].to_string()
}

fn set(
    cells: &mut [Cell],
    channel: &str,
    platform: &str,
    run: &str,
    status: &str,
    seconds: Option<f64>,
) {
    for cell in cells.iter_mut() {
        if cell.channel == channel && cell.platform == platform && cell.run_id == run {
            cell.status = status.to_string();
            cell.install_seconds = seconds;
            if status != "works" {
                cell.version = None;
            }
        }
    }
}

#[test]
fn every_cell_working_within_its_bar_over_three_runs_is_met() -> Result<(), String> {
    let receipt = build_receipt(&matrix(120.0, &THREE_RUNS), &shipped())?;
    assert_eq!(verdict(&receipt), "met", "{}", reasons(&receipt));
    assert_eq!(receipt["cells"].as_array().map(Vec::len), Some(25));
    assert_eq!(
        receipt["source_run_ids"],
        serde_json::json!(["101", "102", "103"])
    );
    Ok(())
}

#[test]
fn a_source_build_with_fewer_than_three_samples_is_not_established() -> Result<(), String> {
    let receipt = build_receipt(&matrix(120.0, &["101"]), &shipped())?;
    assert_eq!(verdict(&receipt), "not_established");
    let text = reasons(&receipt);
    assert!(
        text.contains("cargo on macos-15-intel has 1 of the 3 samples"),
        "{text}"
    );
    assert!(
        !text.contains("pypi on"),
        "prebuilt needs one sample: {text}"
    );
    Ok(())
}

#[test]
fn the_source_build_is_judged_on_its_median_not_its_worst_sample() -> Result<(), String> {
    let mut cells = matrix(120.0, &THREE_RUNS);
    set(
        &mut cells,
        "cargo",
        "macos-15-intel",
        "101",
        "works",
        Some(408.0),
    );
    set(
        &mut cells,
        "cargo",
        "macos-15-intel",
        "102",
        "works",
        Some(728.0),
    );
    set(
        &mut cells,
        "cargo",
        "macos-15-intel",
        "103",
        "works",
        Some(1300.0),
    );
    let receipt = build_receipt(&cells, &shipped())?;
    assert_eq!(verdict(&receipt), "met", "{}", reasons(&receipt));
    let row = receipt["cells"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|row| row["channel"] == "cargo" && row["platform"] == "macos-15-intel")
        .cloned()
        .unwrap_or_default();
    assert_eq!(row["install_seconds"], 728.0);
    assert_eq!(row["build"], "source");
    assert_eq!(row["threshold_seconds"], 900.0);
    assert_eq!(row["samples"].as_array().map(Vec::len), Some(3));

    set(
        &mut cells,
        "cargo",
        "macos-15-intel",
        "101",
        "works",
        Some(950.0),
    );
    let receipt = build_receipt(&cells, &shipped())?;
    assert_eq!(verdict(&receipt), "not_met");
    assert!(
        reasons(&receipt)
            .contains("cargo on macos-15-intel: median 950 s over 3 sample(s), over 900 s"),
        "{}",
        reasons(&receipt)
    );
    Ok(())
}

#[test]
fn a_prebuilt_channel_keeps_the_300_second_bar() -> Result<(), String> {
    let mut cells = matrix(120.0, &THREE_RUNS);
    for run in THREE_RUNS {
        set(
            &mut cells,
            "archive",
            "windows-latest",
            run,
            "works",
            Some(301.0),
        );
    }
    let receipt = build_receipt(&cells, &shipped())?;
    assert_eq!(verdict(&receipt), "not_met");
    assert!(
        reasons(&receipt)
            .contains("archive on windows-latest: median 301 s over 3 sample(s), over 300 s"),
        "{}",
        reasons(&receipt)
    );
    Ok(())
}

#[test]
fn one_failed_or_broken_sample_fails_the_cell_and_names_its_run() -> Result<(), String> {
    for status in ["failed", "broken"] {
        let mut cells = matrix(10.0, &THREE_RUNS);
        set(&mut cells, "npm", "ubuntu-latest", "102", status, None);
        let receipt = build_receipt(&cells, &shipped())?;
        assert_eq!(verdict(&receipt), "not_met", "{status}");
        assert!(
            reasons(&receipt).contains(&format!("npm on ubuntu-latest: {status} in run 102")),
            "{}",
            reasons(&receipt)
        );
    }
    Ok(())
}

#[test]
fn a_gap_in_every_run_or_an_untested_channel_is_not_established() -> Result<(), String> {
    let mut cells = matrix(10.0, &THREE_RUNS);
    for run in THREE_RUNS {
        set(
            &mut cells,
            "binstall",
            "macos-latest",
            run,
            "known_gap",
            None,
        );
    }
    let receipt = build_receipt(&cells, &shipped())?;
    assert_eq!(verdict(&receipt), "not_established");
    assert!(reasons(&receipt).contains("binstall on macos-latest is an unavailable platform"));

    let mut closing = matrix(10.0, &THREE_RUNS);
    set(
        &mut closing,
        "binstall",
        "macos-latest",
        "101",
        "known_gap",
        None,
    );
    assert_eq!(verdict(&build_receipt(&closing, &shipped())?), "met");

    let unprobed = vec![Disposition {
        channel: "vscode_extension".to_string(),
        status: "not_tested".to_string(),
        reason: "needs a desktop host".to_string(),
    }];
    let receipt = build_receipt(&matrix(10.0, &THREE_RUNS), &unprobed)?;
    assert_eq!(verdict(&receipt), "not_established");
    Ok(())
}

#[test]
fn a_slow_cell_outranks_a_gap_so_the_row_reports_the_real_failure() -> Result<(), String> {
    let mut cells = matrix(10.0, &["101"]);
    set(
        &mut cells,
        "pypi",
        "ubuntu-latest",
        "101",
        "works",
        Some(999.0),
    );
    set(&mut cells, "npm", "macos-latest", "101", "known_gap", None);
    assert_eq!(verdict(&build_receipt(&cells, &shipped())?), "not_met");
    Ok(())
}

#[test]
fn a_missing_cell_a_repeat_within_one_run_or_an_unknown_cell_is_refused() -> Result<(), String> {
    let mut cells = matrix(10.0, &["101"]);
    cells.retain(|cell| !(cell.channel == "archive" && cell.platform == "windows-latest"));
    let err = build_receipt(&cells, &shipped()).err().unwrap_or_default();
    assert!(
        err.contains("missing matrix cells: archive on windows-latest"),
        "{err}"
    );

    let mut cells = matrix(10.0, &["101"]);
    cells.push(works("cargo", "ubuntu-latest", 5.0, "101"));
    let err = build_receipt(&cells, &shipped()).err().unwrap_or_default();
    assert!(
        err.contains("duplicate cell cargo on ubuntu-latest in run 101"),
        "{err}"
    );

    let mut cells = matrix(10.0, &["101"]);
    cells.push(works("cargo", "ubuntu-latest", 5.0, "102"));
    build_receipt(&cells, &shipped())?;

    let mut cells = matrix(10.0, &["101"]);
    if let Some(cell) = cells.first_mut() {
        cell.platform = "solaris".to_string();
    }
    let err = build_receipt(&cells, &shipped()).err().unwrap_or_default();
    assert!(err.contains("unknown platform `solaris`"), "{err}");

    let mut cells = matrix(10.0, &["101"]);
    if let Some(cell) = cells.first_mut() {
        cell.run_id = "abc".to_string();
    }
    let err = build_receipt(&cells, &shipped()).err().unwrap_or_default();
    assert!(err.contains("not a numeric run id"), "{err}");
    Ok(())
}

#[test]
fn the_median_of_an_even_set_averages_the_middle_pair() {
    assert_eq!(median(&[]), None);
    assert_eq!(median(&[5.0]), Some(5.0));
    assert_eq!(median(&[900.0, 1.0, 50.0]), Some(50.0));
    assert_eq!(median(&[1.0, 3.0, 100.0, 5.0]), Some(4.0));
}

#[test]
fn cell_files_are_validated_one_by_one() -> Result<(), String> {
    let ok = r#"{"channel":"archive","os":"macos-latest","status":"works","version":"ripr 0.10.0","install_seconds":1.6}"#;
    let cell = parse_cell(ok, "ok.json", "7")?;
    assert_eq!(cell.install_seconds, Some(1.6));
    assert_eq!(cell.platform, "macos-latest");
    assert_eq!(cell.run_id, "7");

    let bad_status = ok.replace("works", "fine");
    let err = parse_cell(&bad_status, "x.json", "7")
        .err()
        .unwrap_or_default();
    assert!(err.contains("unknown status `fine`"), "{err}");

    let no_seconds =
        r#"{"channel":"cargo","os":"macos-latest","status":"works","version":"ripr 0.10.0"}"#;
    let err = parse_cell(no_seconds, "x.json", "7")
        .err()
        .unwrap_or_default();
    assert!(
        err.contains("must record its version and install seconds"),
        "{err}"
    );

    let negative = ok.replace("1.6", "-1");
    let err = parse_cell(&negative, "x.json", "7")
        .err()
        .unwrap_or_default();
    assert!(err.contains("non-negative"), "{err}");

    let gap = r#"{"channel":"binstall","os":"macos-latest","status":"known_gap"}"#;
    assert_eq!(parse_cell(gap, "gap.json", "7")?.version, None);
    Ok(())
}

#[test]
fn dispositions_cover_only_channels_no_workflow_installs() -> Result<(), String> {
    let good =
        r#"{"channels":[{"channel":"homebrew","status":"not_shipped","reason":"no tap yet"}]}"#;
    assert_eq!(parse_dispositions(good, "d.json")?.len(), 1);
    let doubled = good.replace(
        "]}",
        ",{\"channel\":\"homebrew\",\"status\":\"not_shipped\",\"reason\":\"again\"}]}",
    );
    assert!(parse_dispositions(&doubled, "d.json").err().is_some());
    for installed in ["cargo", "archive", "binstall"] {
        let named = good.replace("homebrew", installed);
        assert!(
            parse_dispositions(&named, "d.json").err().is_some(),
            "{installed} is installed by a workflow"
        );
    }
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
fn markdown_names_the_verdict_the_runs_the_bars_and_each_sample() -> Result<(), String> {
    let unprobed = vec![Disposition {
        channel: "ci_init".to_string(),
        status: "not_tested".to_string(),
        reason: "no generated workflow run".to_string(),
    }];
    let receipt = build_receipt(&matrix(30.0, &THREE_RUNS), &unprobed)?;
    let md = render_markdown(&receipt);
    assert!(
        md.contains("**not_established** (runs 101, 102, 103)"),
        "{md}"
    );
    assert!(md.contains("against 300 s"), "{md}");
    assert!(md.contains("at least 3 samples against 900 s"), "{md}");
    assert!(
        md.contains("| cargo | source | macos-15-intel | works | ripr 0.10.0 | 30 | 30, 30, 30 |"),
        "{md}"
    );
    assert!(md.contains("| archive | prebuilt |"), "{md}");
    assert!(md.contains("channel ci_init is not tested"), "{md}");
    Ok(())
}

#[test]
fn arguments_pair_each_cells_directory_with_its_run() {
    let strings = |items: &[&str]| items.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert_eq!(parse_args(&[]), Ok(Mode::Check));
    assert_eq!(parse_args(&strings(&["--check"])), Ok(Mode::Check));
    assert_eq!(
        parse_args(&strings(&[
            "--cells", "a", "--run-id", "12", "--cells", "b", "--run-id", "13", "--write"
        ])),
        Ok(Mode::Build {
            runs: vec![
                ("a".into(), "12".to_string()),
                ("b".into(), "13".to_string())
            ],
            write: true,
        })
    );
    assert!(parse_args(&strings(&["--write"])).err().is_some());
    let err = parse_args(&strings(&["--cells", "a"]))
        .err()
        .unwrap_or_default();
    assert!(err.contains("each --cells needs its own --run-id"), "{err}");
    let err = parse_args(&strings(&[
        "--cells", "a", "--run-id", "12", "--cells", "b", "--run-id", "12",
    ]))
    .err()
    .unwrap_or_default();
    assert!(err.contains("run 12 given twice"), "{err}");
    assert!(
        parse_args(&strings(&["--cells", "d", "--run-id", "x"]))
            .err()
            .is_some()
    );
    assert!(
        parse_args(&strings(&["--cells", "d", "--run-id", "1", "--bogus"]))
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
    let receipt = build_receipt(&matrix(seconds, &THREE_RUNS), &dispositions)?;
    write_receipt(&root, &receipt)?;
    std::fs::write(root.join(super::RECEIPT_MD), render_markdown(&receipt))
        .map_err(|error| error.to_string())?;
    Ok(root)
}

fn write_receipt(root: &std::path::Path, receipt: &serde_json::Value) -> Result<(), String> {
    std::fs::write(
        root.join(super::RECEIPT),
        serde_json::to_string_pretty(receipt).map_err(|e| e.to_string())?,
    )
    .map_err(|error| error.to_string())
}

/// Change the checked-in receipt JSON in place.
fn tamper(
    root: &std::path::Path,
    change: impl FnOnce(&mut serde_json::Value),
) -> Result<(), String> {
    let text =
        std::fs::read_to_string(root.join(super::RECEIPT)).map_err(|error| error.to_string())?;
    let mut value: serde_json::Value =
        serde_json::from_str(&text).map_err(|error| error.to_string())?;
    change(&mut value);
    write_receipt(root, &value)
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
fn check_accepts_a_receipt_its_samples_support() -> Result<(), String> {
    let root = receipt_tree("ok", 10.0)?;
    let result = super::check_receipt(&root);
    let _ = std::fs::remove_dir_all(&root);
    result
}

#[test]
fn check_refuses_a_verdict_its_samples_do_not_support() -> Result<(), String> {
    let root = receipt_tree("verdict", 1000.0)?;
    tamper(&root, |value| {
        value["r0_install"]["status"] = serde_json::json!("met");
    })?;
    let result = super::check_receipt(&root);
    let _ = std::fs::remove_dir_all(&root);
    assert!(result.err().is_some_and(|e| e.contains("r0_install")));
    Ok(())
}

#[test]
fn check_refuses_an_edited_bar_median_run_list_markdown_or_stale_dispositions() -> Result<(), String>
{
    type Tamper = fn(&mut serde_json::Value);
    let tampers: [(&str, Tamper); 4] = [
        ("bar", |value| {
            value["thresholds"]["source_build_seconds"] = serde_json::json!(9999.0);
        }),
        ("median", |value| {
            value["cells"][0]["install_seconds"] = serde_json::json!(1.0);
        }),
        ("runs", |value| {
            value["source_run_ids"] = serde_json::json!(["101"]);
        }),
        ("build", |value| {
            value["cells"][0]["build"] = serde_json::json!("source");
        }),
    ];
    for (name, change) in tampers {
        let root = receipt_tree(name, 10.0)?;
        tamper(&root, change)?;
        let result = super::check_receipt(&root);
        let _ = std::fs::remove_dir_all(&root);
        assert!(
            result.err().is_some_and(|e| e.contains("does not match")),
            "{name} edit was not refused"
        );
    }
    for (name, file, from, to, expect) in [
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
    let root = receipt_tree("no-run", 10.0)?;
    tamper(&root, |value| {
        value["cells"][0]["samples"][0]["run_id"] = serde_json::Value::Null;
    })?;
    let result = super::check_receipt(&root);
    let _ = std::fs::remove_dir_all(&root);
    assert!(result.err().is_some_and(|e| e.contains("no run_id")));
    Ok(())
}

#[test]
fn dispositions_file_must_account_for_every_other_channel() -> Result<(), String> {
    let root = receipt_tree("missing", 10.0)?;
    edit(&root, super::DISPOSITIONS, "\"homebrew\"", "\"ci_init\"")?;
    let result = super::read_dispositions(&root.join(super::DISPOSITIONS));
    let _ = std::fs::remove_dir_all(&root);
    assert!(result.err().is_some());
    Ok(())
}

#[test]
fn runs_are_ordered_numerically_so_the_latest_run_names_the_version() -> Result<(), String> {
    let mut cells = matrix(120.0, &["9", "10", "11"]);
    for cell in cells.iter_mut().filter(|cell| cell.run_id == "11") {
        cell.version = Some("ripr 0.11.0".to_string());
    }
    let receipt = build_receipt(&cells, &shipped())?;
    assert_eq!(
        receipt["source_run_ids"],
        serde_json::json!(["9", "10", "11"])
    );
    assert_eq!(receipt["cells"][0]["version"], "ripr 0.11.0");
    assert_eq!(receipt["cells"][0]["samples"][0]["run_id"], "9");
    Ok(())
}
