//! `ripr help --json` flag inventory and JSON-flag contract (#11599).
//!
//! The catalog's `accepts_json_flag` must agree with the flag inventory parsed
//! from each command's own help body, so a script can trust one field instead
//! of probing every command.

use std::process::Command;

/// Run the real binary with no stdin, so a command that reads it sees EOF.
fn run_ripr(args: &[&str]) -> Result<std::process::Output, String> {
    Command::new(env!("CARGO_BIN_EXE_ripr"))
        .args(args)
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|error| format!("failed to run ripr {args:?}: {error}"))
}

fn catalog() -> Result<serde_json::Value, String> {
    let output = run_ripr(&["help", "--json"])?;
    if !output.status.success() {
        return Err(format!("help --json failed: {:?}", output.status));
    }
    serde_json::from_slice(&output.stdout).map_err(|error| format!("invalid JSON: {error}"))
}

fn flag_names(row: &serde_json::Value) -> Vec<String> {
    row["flags"]
        .as_array()
        .map(|flags| {
            flags
                .iter()
                .filter_map(|flag| flag["name"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn format_accepts_json(row: &serde_json::Value) -> bool {
    row["flags"].as_array().is_some_and(|flags| {
        flags.iter().any(|flag| {
            flag["name"] == "--format"
                && flag["values"]
                    .as_array()
                    .is_some_and(|values| values.iter().any(|value| value == "json"))
        })
    })
}

#[test]
fn json_flag_field_agrees_with_the_parsed_flag_inventory() -> Result<(), String> {
    let document = catalog()?;
    assert_eq!(document["schema_version"], 4);
    let commands = document["commands"].as_array().ok_or("commands missing")?;
    // `checked` counts own-body rows; `discriminating` counts those whose
    // body documents a JSON flag, the only rows where a wrong declared
    // spelling can be caught. Body-less and shared-body rows are not counted.
    let mut checked = 0;
    let mut discriminating = 0;
    for row in commands {
        if row["flags_scope"] != "own_help_body" {
            continue;
        }
        let path = row["path"].as_str().unwrap_or("?");
        let names = flag_names(row);
        // Known documentation gaps: these help bodies do not list the JSON
        // flag they accept (`help` prints no option list; `context` omits
        // `--json`), or list `--format` without a typed value set. The
        // catalog's `accepts_json_flag` is the authority there.
        if matches!(
            path,
            "help" | "context" | "outcome" | "calibrate cargo-mutants"
        ) {
            continue;
        }
        match row["accepts_json_flag"].as_str() {
            Some("--json") => assert!(
                names.iter().any(|name| name == "--json"),
                "{path} claims --json but its help documents {names:?}"
            ),
            Some("--format json") => assert!(
                format_accepts_json(row),
                "{path} claims --format json but --format does not list json"
            ),
            Some(other) => return Err(format!("{path}: unknown spelling {other}")),
            None => assert!(
                !names.iter().any(|name| name == "--json") && !format_accepts_json(row),
                "{path} documents a JSON flag but claims none"
            ),
        }
        checked += 1;
        if names.iter().any(|name| name == "--json") || format_accepts_json(row) {
            discriminating += 1;
        }
    }
    assert!(checked >= 35, "only {checked} own-body rows were checked");
    assert!(
        discriminating >= 15,
        "only {discriminating} rows had a documented JSON flag to discriminate on"
    );
    Ok(())
}

/// `writes_json_artifacts` is independent of `accepts_json_flag`: a command
/// may print JSON on stdout, write JSON files, both, or neither. Pin one real
/// row of each quadrant so the definition cannot drift to "file-only".
#[test]
fn writes_json_artifacts_is_independent_of_the_stdout_json_flag() -> Result<(), String> {
    let document = catalog()?;
    let commands = document["commands"].as_array().ok_or("commands missing")?;
    let row = |path: &str| {
        commands
            .iter()
            .find(|row| row["path"] == path)
            .ok_or_else(|| format!("{path} missing from help --json"))
    };
    // Both: stdout JSON mode and artifact files.
    for (path, spelling) in [("check", "--json"), ("pilot", "--format json")] {
        let r = row(path)?;
        assert_eq!(r["accepts_json_flag"], spelling, "{path}");
        assert_eq!(r["writes_json_artifacts"], true, "{path}");
    }
    // Every row carrying both fields is reported with both populated.
    let both = commands
        .iter()
        .filter(|r| r["accepts_json_flag"].is_string() && r["writes_json_artifacts"] == true)
        .count();
    assert!(both >= 6, "expected at least 6 rows with both, got {both}");
    // Artifacts only: writes files, accepts no stdout JSON flag.
    let artifact_only = commands
        .iter()
        .filter(|r| r["accepts_json_flag"].is_null() && r["writes_json_artifacts"] == true)
        .count();
    assert!(artifact_only > 0, "no artifact-only row is reported");
    // Stdout JSON only: prints JSON, writes no artifact files.
    let stdout_only = commands
        .iter()
        .filter(|r| r["accepts_json_flag"].is_string() && r["writes_json_artifacts"] == false)
        .count();
    assert!(stdout_only > 0, "no stdout-JSON-only row is reported");
    // Neither: the metadata is not all-true.
    let neither = commands
        .iter()
        .filter(|r| r["accepts_json_flag"].is_null() && r["writes_json_artifacts"] == false)
        .count();
    assert!(neither > 0, "no row reports neither");
    Ok(())
}

/// Body-less rows are labelled so an empty `flags` list is not read as "no
/// flags documented".
#[test]
fn rows_without_a_help_body_are_scoped_no_help_body() -> Result<(), String> {
    let document = catalog()?;
    let commands = document["commands"].as_array().ok_or("commands missing")?;
    let agent = commands
        .iter()
        .find(|row| row["path"] == "agent")
        .ok_or("agent row missing")?;
    assert_eq!(agent["flags_scope"], "no_help_body");
    assert_eq!(agent["flags"], serde_json::json!([]));
    let no_body = commands
        .iter()
        .filter(|row| row["flags_scope"] == "no_help_body")
        .count();
    assert!(no_body >= 10, "expected body-less rows, got {no_body}");
    Ok(())
}

/// #14086: `gate check` and `gate evaluate` print the gate help, so their rows
/// carry its flags. The body documents both subcommands, so all three gate
/// rows say `shared_help_body` rather than claiming the union as their own.
#[test]
fn gate_rows_carry_the_shared_gate_flags() -> Result<(), String> {
    let document = catalog()?;
    let commands = document["commands"].as_array().ok_or("commands missing")?;
    for path in ["gate", "gate check", "gate evaluate"] {
        let row = commands
            .iter()
            .find(|row| row["path"] == path)
            .ok_or_else(|| format!("{path} missing from help --json"))?;
        assert_eq!(row["flags_scope"], "shared_help_body", "{path}");
        let flags = row["flags"].as_array().ok_or("flags missing")?;
        for name in ["--base", "--mode", "--root", "--pr-guidance", "--out-md"] {
            let flag = flags
                .iter()
                .find(|flag| flag["name"] == name)
                .ok_or_else(|| format!("{path} lost {name}: {:?}", flag_names(row)))?;
            assert_eq!(flag["takes_value"], true, "{path} {name}");
        }
    }
    Ok(())
}

/// A row that claims its help body as its own lists the flags that command
/// accepts. Run each one through the real parser, followed by a token no
/// command accepts: the parser reads left to right and refuses the first
/// argument it does not know, so the refusal names the sentinel, never the
/// documented flag. The sentinel also ends every probe at argument parsing,
/// before any analysis or write.
///
/// `gate` used to fail this: its row listed `gate check`'s `--base` and
/// `--head`, which the bare command rejects (#14086).
#[test]
fn every_flag_an_own_body_row_lists_is_accepted_by_the_parser() -> Result<(), String> {
    const SENTINEL: &str = "--zz-not-a-ripr-flag";
    // `check --all` is the help modifier (`ripr check --help --all`), listed
    // among check's options but meaningful only next to `--help`.
    const HELP_ONLY: &[(&str, &str)] = &[("check", "--all")];
    // Listed in the help body as rejected, with the reason.
    const DOCUMENTED_AS_REJECTED: &[(&str, &str)] =
        &[("feedback record", "--file"), ("feedback record", "--line")];
    const DUMMY: &str = "zzdummy";
    let document = catalog()?;
    let commands = document["commands"].as_array().ok_or("commands missing")?;
    let mut probed = 0;
    let mut reached_sentinel = 0;
    let mut rejected: Vec<String> = Vec::new();
    for row in commands {
        if row["flags_scope"] != "own_help_body" {
            continue;
        }
        let path = row["path"].as_str().ok_or("path missing")?;
        for flag in row["flags"].as_array().ok_or("flags missing")? {
            let name = flag["name"].as_str().ok_or("flag name missing")?;
            if HELP_ONLY.contains(&(path, name)) || DOCUMENTED_AS_REJECTED.contains(&(path, name)) {
                continue;
            }
            let mut args: Vec<&str> = path.split(' ').collect();
            args.push(name);
            if flag["takes_value"] == true {
                args.push(flag["values"][0].as_str().unwrap_or(DUMMY));
            }
            args.push(SENTINEL);
            let output = run_ripr(&args)?;
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains(&format!("unknown {path} argument {name:?}")) {
                rejected.push(format!("ripr {}: {}", args.join(" "), stderr.trim()));
            }
            if stderr.contains(&format!("unknown {path} argument {SENTINEL:?}")) {
                reached_sentinel += 1;
            } else if !(output.status.success()
                || stderr.contains(&format!("{DUMMY:?}"))
                || stderr.contains(name))
            {
                // A probe that stops before the sentinel must show the flag
                // was read: the run succeeded (`--help`, `--version`), or the
                // refusal is about the flag's value, quoting the dummy or
                // naming the flag. A refusal that mentions neither did not
                // come from reading this flag.
                rejected.push(format!("ripr {}: {}", args.join(" "), stderr.trim()));
            }
            probed += 1;
        }
    }
    assert!(
        rejected.is_empty(),
        "help documents flags the parser rejects:\n{}",
        rejected.join("\n")
    );
    // Guards the stimulus, not the surface: a short count means the probe
    // stopped reaching the parsers. Some flags end the run before the
    // sentinel is read (`--help`, an eagerly validated value); each of
    // those is checked above, so the sentinel floor sits below the probe
    // floor.
    assert!(probed >= 300, "only {probed} documented flags were probed");
    assert!(
        reached_sentinel >= 250,
        "only {reached_sentinel} of {probed} probes were refused at the sentinel"
    );
    Ok(())
}
