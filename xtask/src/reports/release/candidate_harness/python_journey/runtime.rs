//! Real interpreter observations through the existing bounded process owner.
use super::*;
use std::time::Duration;

pub(super) fn command(cwd: &Path, script: &str) -> Result<CommandResult, String> {
    let script = format!("unset -f python pytest\n{script}");
    let output = crate::run::capture_bytes_in_dir_with_env_budget(
        Path::new("bash"),
        &["--noprofile".into(), "--norc".into(), "-c".into(), script],
        cwd,
        (
            &[
                ("PYTEST_DISABLE_PLUGIN_AUTOLOAD", "1"),
                ("PYTHONDONTWRITEBYTECODE", "1"),
                ("BASH_ENV", ""),
            ],
            &[
                "PYTHONPATH",
                "PYTHONHOME",
                "PYTEST_ADDOPTS",
                "PYTEST_PLUGINS",
                "PYTHONSTARTUP",
                "PYTHONINSPECT",
            ],
        ),
        crate::run::ByteCaptureBudget {
            timeout: Duration::from_secs(30),
            stdout_bytes: 2 * 1024 * 1024,
            stderr_bytes: 2 * 1024 * 1024,
        },
        "Python journey real runtime",
    )?;
    if output.timed_out {
        return Err("Python runtime deadline exceeded".into());
    }
    let status = output
        .status
        .ok_or("Python runtime has no terminal status")?;
    Ok(CommandResult {
        status: status.code(),
        success: status.success(),
        stdout: String::from_utf8(output.stdout).map_err(|e| e.to_string())?,
        stderr: String::from_utf8(output.stderr).map_err(|e| e.to_string())?,
    })
}

pub(super) fn record(
    evidence: &Path,
    cwd: &Path,
    script: &str,
    label: &str,
) -> Result<CommandResult, String> {
    let result = command(cwd, script)?;
    write_json(
        &evidence.join(format!("runtime-{label}.json")),
        &json!({
        "literal_script":script,"cwd":cwd,"exit":result.status,"stdout":result.stdout,"stderr":result.stderr,
        "environment":"caller-supplied PATH; Python path overrides removed; plugin autoload and bytecode disabled"}),
    )?;
    Ok(result)
}

pub(super) fn identity(evidence: &Path, cwd: &Path) -> Result<Value, String> {
    let observed = record(
        evidence,
        cwd,
        "python -c 'import json,sys,pytest,shutil; p=shutil.which(\"pytest\"); print(json.dumps(dict(executable=sys.executable,prefix=sys.prefix,pytest=pytest.__version__,pytest_file=pytest.__file__,console_script=p,console_shebang=open(p).readline(4096).rstrip() if p else None)))'",
        "identity",
    )?;
    require_success(
        &observed,
        "Python/pytest prerequisite: activate a project environment with pytest before starting this consumer",
    )?;
    let identity: Value = serde_json::from_str(&observed.stdout).map_err(|e| e.to_string())?;
    same_environment(&identity)?;
    let console = record(evidence, cwd, "pytest --version", "console-version")?;
    require_success(&console, "pytest console prerequisite")?;
    if console.stdout.trim()
        != format!(
            "pytest {}",
            identity["pytest"]
                .as_str()
                .ok_or("pytest identity missing")?
        )
    {
        return Err("Python module and console pytest versions differ".into());
    }
    Ok(identity)
}

pub(super) fn same_environment(identity: &Value) -> Result<(), String> {
    let parent = |key: &str| -> Result<PathBuf, String> {
        Path::new(
            identity[key]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| format!("missing runtime {key}"))?,
        )
        .parent()
        .ok_or("runtime entry lacks parent")?
        .canonicalize()
        .map_err(|e| e.to_string())
    };
    if parent("executable")? != parent("console_script")? {
        return Err("Python and pytest console select different environment directories".into());
    }
    if identity["pytest_file"].as_str().is_none_or(str::is_empty) {
        return Err("pytest module identity missing".into());
    }
    Ok(())
}

pub(super) fn passed(result: &CommandResult, count: usize, label: &str) -> Result<(), String> {
    require_success(result, label)?;
    let words = result.stdout.split_whitespace().collect::<Vec<_>>();
    let passed_counts = words
        .windows(2)
        .filter(|pair| pair[1] == "passed")
        .map(|pair| pair[0].parse::<usize>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    if passed_counts != [count] || count == 0 || result.stdout.contains("no tests ran") {
        return Err(format!(
            "{label} has no expected nonzero test denominator: {}",
            result.stdout
        ));
    }
    Ok(())
}

pub(super) fn verify_action(check: &str, root: &Path) -> Result<String, String> {
    let action = check
        .lines()
        .find_map(|line| line.trim().strip_prefix("Verify command (Bash): "))
        .and_then(|line| line.strip_suffix(" (high)"))
        .ok_or("check lacks executable Python verify command")?;
    let inner = action
        .strip_prefix('(')
        .and_then(|s| s.strip_suffix(')'))
        .ok_or("unsupported Python verify shell shape")?;
    let words = printed_words::printed_words(inner)?;
    if words
        != [
            "cd",
            "-P",
            "--",
            root.to_str().ok_or("Python root UTF-8")?,
            "&&",
            "python",
            "-m",
            "pytest",
            "tests/test_pricing.py::test_above_boundary_gets_discount",
        ]
    {
        return Err(format!(
            "unexpected or wrong-root Python verification action: {action}"
        ));
    }
    Ok(action.to_string())
}
