use super::*;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

fn env(name: &str) -> Result<String, String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("missing {name}"))
}

fn git(args: &[&str]) -> Result<String, String> {
    crate::run::run_output("git", args).map(|value| value.trim().to_string())
}

fn validate_scalar_identities(
    subject: &serde_json::Value,
    run: &str,
    attempt: &str,
) -> Result<(), String> {
    for key in ["head", "base", "merge", "tree", "workflow_generation"] {
        let value = subject
            .get(key)
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("missing {key}"))?;
        if value.len() != 40
            || !value
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        {
            return Err(format!("invalid exact Git identity: {key}"));
        }
    }
    for (key, value) in [("run_id", run), ("run_attempt", attempt)] {
        if value.starts_with('0')
            || value.is_empty()
            || !value.bytes().all(|b| b.is_ascii_digit())
            || subject.get(key).and_then(serde_json::Value::as_str) != Some(value)
        {
            return Err(format!("invalid or stale {key}"));
        }
    }
    Ok(())
}

fn observed_plan() -> Result<Plan, String> {
    let subject: serde_json::Value =
        serde_json::from_str(&env("RIPR_DAG_SUBJECT")?).map_err(|e| e.to_string())?;
    validate_scalar_identities(
        &subject,
        &env("GITHUB_RUN_ID")?,
        &env("GITHUB_RUN_ATTEMPT")?,
    )?;
    let field = |key: &str| {
        subject
            .get(key)
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("missing subject {key}"))
    };
    if subject.get("version").and_then(serde_json::Value::as_u64) != Some(3)
        || field("kind")? != "merge-group"
        || field("repository")? != env("GITHUB_REPOSITORY")?
        || field("request_id")?
            != format!("{}-{}", env("GITHUB_RUN_ID")?, env("GITHUB_RUN_ATTEMPT")?)
        || field("merge")? != git(&["rev-parse", "HEAD"])?
        || field("tree")? != git(&["rev-parse", "HEAD^{tree}"])?
    {
        return Err("native caller subject/request does not match this execution".into());
    }
    let parents: Vec<_> = git(&["show", "-s", "--format=%P", "HEAD"])?
        .split_whitespace()
        .map(str::to_owned)
        .collect();
    if subject.get("parents") != Some(&serde_json::json!(parents)) {
        return Err("actual ordered parents mismatch".into());
    }
    git(&["diff", "--exit-code", "HEAD", "--"])?;
    let base = field("base")?;
    // This guard deliberately blocks broad policy/config changes, not just a
    // head-authored selector's claimed relevant files. No path-based exemption.
    git(&[
        "diff",
        "--exit-code",
        base,
        "HEAD",
        "--",
        "xtask",
        "tools/repo-policy",
        ".github",
        ".cargo",
        ".config",
        "policy",
        "Cargo.toml",
        "Cargo.lock",
        "rust-toolchain.toml",
    ])?;
    let inventory = crate::run::run_output_bytes(
        "git",
        &[
            "diff",
            "--raw",
            "--no-abbrev",
            "--no-renames",
            "-z",
            base,
            "HEAD",
            "--",
        ],
    )?;
    let mut configuration = crate::run::run_output("rustc", &["-vV"])?;
    configuration.push_str(&crate::run::run_output("cargo", &["--version"])?);
    for path in [
        "Cargo.toml",
        "Cargo.lock",
        "rust-toolchain.toml",
        ".cargo/config.toml",
        ".config/nextest.toml",
    ] {
        configuration.push_str(&git(&["rev-parse", &format!("HEAD:{path}")])?);
    }
    for key in [
        "RUSTFLAGS",
        "CARGO_ENCODED_RUSTFLAGS",
        "RUSTC_WRAPPER",
        "CARGO_BUILD_TARGET",
    ] {
        configuration.push_str(key);
        configuration.push_str(&std::env::var(key).unwrap_or_default());
        configuration.push('\0');
    }
    Ok(Plan {
        version: 1,
        subject: subject.clone(),
        // Hex is lossless for NUL-separated inventory and non-UTF8 Git paths.
        inventory: inventory.iter().map(|byte| format!("{byte:02x}")).collect(),
        configuration: format!("{:x}", Sha256::digest(configuration.as_bytes())),
        producer: git(&["rev-parse", "HEAD:.github/workflows/rust-gates.yml"])?,
        rows: rows(),
    })
}

fn read<T: serde::de::DeserializeOwned>(path: &str) -> Result<T, String> {
    serde_json::from_slice(&fs::read(path).map_err(|e| format!("{path}: {e}"))?)
        .map_err(|e| format!("{path}: {e}"))
}

fn write<T: Serialize>(path: &str, value: &T) -> Result<(), String> {
    fs::write(
        path,
        serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("{path}: {e}"))
}

fn verify(path: &str) -> Result<Plan, String> {
    let plan: Plan = read(path)?;
    if plan != observed_plan()? {
        return Err("stale or substituted admitted plan".into());
    }
    Ok(plan)
}

fn record(
    plan: Plan,
    branch: Branch,
    steps: &serde_json::Value,
    elapsed_seconds: u64,
) -> Result<BranchResult, String> {
    for row in &plan.rows {
        let outcome = steps
            .get(&row.step)
            .and_then(|value| value.get("outcome"))
            .and_then(serde_json::Value::as_str);
        if row.branch != branch && outcome.is_some_and(|value| value != "skipped") {
            return Err(format!("unselected obligation executed: {}", row.id));
        }
    }
    let rows = plan
        .rows
        .iter()
        .filter(|row| row.branch == branch)
        .map(|row| ResultRow {
            id: row.id.clone(),
            step: row.step.clone(),
            outcome: steps
                .get(&row.step)
                .and_then(|value| value.get("outcome"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("missing")
                .into(),
        })
        .collect();
    Ok(BranchResult {
        plan,
        branch,
        rows,
        elapsed_seconds,
    })
}

pub(crate) fn run(args: &[String]) -> Result<(), String> {
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["project"] => projection::project(false),
        ["project", "--check"] => projection::project(true),
        ["plan", path] => {
            projection::project(true)?;
            write(path, &observed_plan()?)?;
            println!("{}", serde_json::json!({"include": Branch::ALL.map(|branch| serde_json::json!({"branch": branch.name()}))}));
            Ok(())
        }
        ["verify", path] => verify(path).map(|_| ()),
        ["record", path, branch, output, start] => {
            let plan = verify(path)?;
            let steps = serde_json::from_str(&env("RIPR_DAG_STEPS")?).map_err(|e| e.to_string())?;
            let now = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| e.to_string())?.as_secs();
            let start: u64 = start.parse().map_err(|e| format!("invalid start: {e}"))?;
            let elapsed = now.checked_sub(start).ok_or("future branch start")?;
            write(output, &record(plan, Branch::parse(branch)?, &steps, elapsed)?)
        }
        ["finalize", path, directory] => {
            let plan = verify(path)?;
            let entries = fs::read_dir(directory).map_err(|e| e.to_string())?.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?;
            if entries.len() != Branch::ALL.len() { return Err("missing or unexpected branch artifacts".into()); }
            let mut results = Vec::new();
            for branch in Branch::ALL {
                let artifact = Path::new(directory).join(format!("rust-dag-result-{}-{}-{}", branch.name(), env("GITHUB_RUN_ID")?, env("GITHUB_RUN_ATTEMPT")?));
                let files = fs::read_dir(&artifact).map_err(|e| e.to_string())?.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?;
                if files.len() != 1 { return Err("missing or extra files in branch artifact".into()); }
                let path = artifact.join(format!("result-{}.json", branch.name()));
                results.push(read::<BranchResult>(path.to_str().ok_or("invalid result path")?)?);
            }
            reconcile(&plan, &results)?;
            let occupancy = results.iter().try_fold(0u64, |sum, result| sum.checked_add(result.elapsed_seconds).ok_or("branch timing overflow"))?;
            let longest = results.iter().map(|result| result.elapsed_seconds).max().ok_or("empty timing denominator")?;
            println!("{}", serde_json::json!({"version":1,"status":"passed","subject":plan.subject,"obligations":plan.rows.len(),"branches":results.len(),"summed_branch_seconds":occupancy,"longest_branch_seconds":longest,"claim":"branch durations include local setup; admission, finalization, queue delays and complete hosted latency require separate run timing"}));
            Ok(())
        }
        _ => Err("usage: cargo xtask product-gate-dag project [--check] | plan <file> | verify <file> | record <plan> <branch> <output> <start-epoch> | finalize <plan> <result-directory>".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn git_options_and_path_like_run_ids_are_refused_before_observation() -> Result<(), String> {
        let mut subject = serde_json::json!({"head":"a".repeat(40),"base":"b".repeat(40),"merge":"c".repeat(40),"tree":"d".repeat(40),"workflow_generation":"e".repeat(40),"run_id":"42","run_attempt":"1"});
        validate_scalar_identities(&subject, "42", "1")?;
        subject["base"] = "--output=untrusted".into();
        assert!(validate_scalar_identities(&subject, "42", "1").is_err());
        subject["base"] = "b".repeat(40).into();
        assert!(validate_scalar_identities(&subject, "../42", "1").is_err());
        assert!(validate_scalar_identities(&subject, "42", "2").is_err());
        Ok(())
    }
    #[test]
    fn record_uses_native_outcome_not_successful_continue_on_error_conclusion() -> Result<(), String>
    {
        let plan = super::super::tests::plan();
        let steps = serde_json::json!({"clippy":{"outcome":"failure","conclusion":"success"}});
        let result = record(plan, Branch::Clippy, &steps, 1)?;
        assert_eq!(result.rows[0].outcome, "failure");
        Ok(())
    }
    #[test]
    fn record_refuses_extra_execution_and_retains_missing_results() -> Result<(), String> {
        let plan = super::super::tests::plan();
        assert!(
            record(
                plan.clone(),
                Branch::Clippy,
                &serde_json::json!({"goldens":{"outcome":"success"}}),
                0
            )
            .is_err()
        );
        assert_eq!(
            record(plan, Branch::Clippy, &serde_json::json!({}), 0)?.rows[0].outcome,
            "missing"
        );
        Ok(())
    }
}
