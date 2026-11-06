//! Caller-declared test status and static producer receipts are distinct.
use super::*;

pub(super) struct Advertised {
    args: Vec<String>,
    out: PathBuf,
    gap: String,
    literal: String,
}

pub(super) fn first_pr(
    root: &Path,
    foreign: &Path,
    evidence: &Path,
    base: &str,
    gap: &str,
    commands: &mut Vec<Value>,
    run: &mut impl FnMut(&[String], &Path, &str) -> Result<CommandResult, String>,
) -> Result<Advertised, String> {
    let root_arg = root.to_str().ok_or("Python root UTF-8")?;
    let packet = invoke(
        run,
        foreign,
        evidence,
        commands,
        [
            "first-pr",
            "--root",
            root_arg,
            "--base",
            base,
            "--run-chain",
        ]
        .map(str::to_string)
        .to_vec(),
        "Python first-pr receipt advertisement",
    )?;
    require_success(&packet, "Python first-pr")?;
    let line = |prefix: &str| {
        packet
            .stdout
            .lines()
            .find_map(|line| line.strip_prefix(prefix))
            .ok_or_else(|| format!("Python first-pr lacks {prefix}"))
    };
    let literal = line("Receipt after verify: `")?
        .strip_suffix('`')
        .ok_or("receipt command closing quote")?
        .to_string();
    let path = line("Receipt path: `")?
        .strip_suffix('`')
        .ok_or("receipt path closing quote")?;
    let status = line("Receipt status: ")?;
    for state in ["not_run", "failed", "passed"] {
        if !status.contains(&format!("--status {state}")) {
            return Err(format!("Python first-pr lacks {state} mapping"));
        }
    }
    let inner = literal
        .strip_prefix('(')
        .and_then(|s| s.strip_suffix(')'))
        .ok_or("Python first-pr receipt shell shape")?;
    let words = printed_words::printed_words(inner)?;
    if words.get(..7)
        != Some(
            ["cd", "-P", "--", root_arg, "&&", "ripr", "receipt"]
                .map(str::to_string)
                .as_slice(),
        )
        || words.get(7).map(String::as_str) != Some("write")
    {
        return Err("Python first-pr receipt escaped the selected root".into());
    }
    let args = words[6..].to_vec();
    if value(&args, "--gap")? != gap
        || value(&args, "--status")? != "not_run"
        || value(&args, "--out")? != path
        || value(&args, "--verify-command")?
            != "python -m pytest tests/test_pricing.py::test_above_boundary_gets_discount"
    {
        return Err("Python receipt path/command/subject parity differs".into());
    }
    Ok(Advertised {
        args,
        out: relative_path(root, path)?,
        gap: gap.into(),
        literal,
    })
}

pub(super) fn caller_statuses(
    route: &Advertised,
    root: &Path,
    evidence: &Path,
    commands: &mut Vec<Value>,
    run: &mut impl FnMut(&[String], &Path, &str) -> Result<CommandResult, String>,
    states: &[&str],
) -> Result<Value, String> {
    let mut rows = Vec::new();
    for &state in states {
        let mut args = route.args.clone();
        let position = args
            .iter()
            .position(|s| s == "--status")
            .ok_or("receipt status flag lost")?;
        args[position + 1] = state.to_string();
        let result = invoke(
            run,
            root,
            evidence,
            commands,
            args,
            "Python advertised caller-status receipt",
        )?;
        require_success(&result, "caller-status receipt")?;
        let bytes = plain_bytes(&route.out)?;
        let receipt: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        if receipt["verify_status"] != state || receipt["canonical_gap_id"] != route.gap {
            return Err("Python caller status/subject changed".into());
        }
        for field in ["bound_gap", "closed", "movement", "outcome"] {
            if receipt.get(field).is_some() {
                return Err("caller receipt acquired static authority".into());
            }
        }
        let retained = evidence.join(format!("caller-receipt-{state}.json"));
        fs::write(&retained, &bytes).map_err(|e| e.to_string())?;
        rows.push(
            json!({"status":state,"artifact":artifact(&retained)?,"producer_path":route.out}),
        );
    }
    Ok(
        json!({"literal_advertisement":route.literal,"path_parity":true,"states":rows,
        "authority":"caller-declared project outcome; not authenticated process evidence or static closure"}),
    )
}

pub(super) fn static_refusals(
    root: &Path,
    foreign: &Path,
    evidence: &Path,
    gap: &str,
    attempt: &str,
    commands: &mut Vec<Value>,
    run: &mut impl FnMut(&[String], &Path, &str) -> Result<CommandResult, String>,
) -> Result<Value, String> {
    let verify = root.join("target/ripr/workflow/agent-verify.json");
    let original = plain_bytes(&verify)?;
    let receipt = root.join("target/ripr/reports/agent-receipt.json");
    let receipt_before = artifact(&receipt)?;
    let mut rows = Vec::new();
    for case in ["positive", "missing", "tampered"] {
        if case == "missing" {
            fs::remove_file(&verify).map_err(|e| e.to_string())?;
        }
        if case == "tampered" {
            let mut value: Value = serde_json::from_slice(&original).map_err(|e| e.to_string())?;
            value["bound_gap"]["before_classification"] = json!("exposed");
            write_json(&verify, &value)?;
        }
        let out = format!("target/ripr/reports/python-{case}-control.json");
        let args = [
            "agent",
            "receipt",
            "--root",
            root.to_str().ok_or("root UTF-8")?,
            "--verify-json",
            verify.to_str().ok_or("verify UTF-8")?,
            "--seam-id",
            gap,
            "--attempt",
            attempt,
            "--json",
            "--out",
            &out,
        ]
        .map(str::to_string)
        .to_vec();
        let result = invoke(
            run,
            foreign,
            evidence,
            commands,
            args,
            "Python static receipt artifact control",
        );
        // Restore exact producer bytes even after a process or parsing error.
        fs::write(&verify, &original).map_err(|e| e.to_string())?;
        let result = result?;
        if case == "positive" {
            require_success(&result, "positive canonical static receipt")?;
            let produced = read_json(&root.join(&out))?;
            if produced["bound_gap"]["closed"] != true
                || produced["verification"]["status"] != "verification_not_run"
                || produced["verification"]["commands_run"] != json!([])
            {
                return Err(
                    "positive static receipt control lost closure or promoted execution".into(),
                );
            }
        } else if result.success
            || root.join(&out).exists()
            || case == "missing" && !result.stderr.contains("agent-verify.json")
            || case == "tampered" && !result.stderr.contains("not_canonical")
        {
            return Err(format!(
                "Python {case} verify control failed to refuse without output: {}",
                result.stderr
            ));
        }
        rows.push(
            json!({"case":case,"exit":result.status,"output_exists":root.join(&out).exists()}),
        );
    }
    if artifact(&receipt)? != receipt_before {
        return Err("refused controls changed authoritative static receipt".into());
    }
    Ok(json!(rows))
}

fn value(words: &[String], name: &str) -> Result<String, String> {
    let index = words
        .iter()
        .position(|s| s == name)
        .ok_or_else(|| format!("receipt lacks {name}"))?;
    if words.iter().filter(|s| s.as_str() == name).count() != 1 {
        return Err(format!("duplicate receipt {name}"));
    }
    words
        .get(index + 1)
        .cloned()
        .ok_or_else(|| format!("receipt lacks {name} value"))
}
