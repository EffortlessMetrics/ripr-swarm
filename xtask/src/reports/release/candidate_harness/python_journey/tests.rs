use super::*;

#[test]
fn wrong_environment_and_wrong_root_fail_closed() -> Result<(), String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let temporary = std::env::temp_dir();
    fs::create_dir_all(&temporary).map_err(|e| e.to_string())?;
    let scratch = temporary.join(format!("python-env-control-{}-{stamp}", std::process::id()));
    fs::create_dir(&scratch).map_err(|e| e.to_string())?;
    fs::create_dir(scratch.join("selected")).map_err(|e| e.to_string())?;
    fs::create_dir(scratch.join("foreign")).map_err(|e| e.to_string())?;
    let identity = json!({"executable":scratch.join("selected/python"),"console_script":scratch.join("foreign/pytest"),"pytest_file":"fixture"});
    let result = runtime::same_environment(&identity);
    fs::remove_dir_all(&scratch).map_err(|e| e.to_string())?;
    if !result.is_err_and(|e| e.contains("different environment")) {
        return Err("foreign console accepted".into());
    }
    let root = Path::new("/selected é");
    let good = "Verify command (Bash): (cd -P -- '/selected é' && python -m pytest tests/test_pricing.py::test_above_boundary_gets_discount) (high)";
    runtime::verify_action(good, root)?;
    if runtime::verify_action(&good.replace("/selected é", "/decoy"), root).is_ok() {
        return Err("decoy root accepted".into());
    }
    if runtime::passed(
        &CommandResult {
            status: Some(0),
            success: true,
            stdout: "no tests ran".into(),
            stderr: String::new(),
        },
        1,
        "empty control",
    )
    .is_ok()
    {
        return Err("empty runtime was promoted".into());
    }
    if runtime::passed(
        &CommandResult {
            status: Some(0),
            success: true,
            stdout: "11 passed in 0.01s".into(),
            stderr: String::new(),
        },
        1,
        "wrong denominator",
    )
    .is_ok()
    {
        return Err("eleven tests were accepted as the literal one-test selector".into());
    }
    Ok(())
}

/// Caller supplies a workspace-built executable and an activated isolated
/// Python environment. This never constructs InstalledCandidate or installed
/// acceptance, even though it exercises the same public journey implementation.
#[test]
#[ignore = "requires explicit workspace binary and real isolated pytest environment"]
fn workspace_real_python_protocol_preparation() -> Result<(), String> {
    workspace_preparation()
}

#[test]
#[ignore = "requires explicit workspace binary and a Python PATH without pytest"]
fn workspace_missing_environment_refuses_before_edit() -> Result<(), String> {
    let result = workspace_preparation();
    let error = match result {
        Ok(()) => {
            return Err("missing environment unexpectedly completed the Python journey".into());
        }
        Err(error) => error,
    };
    let evidence = PathBuf::from(
        std::env::var_os("RIPR_PYTHON_PREPARATION_EVIDENCE").ok_or("missing evidence path")?,
    );
    let selected = evidence.join("selected ' é");
    if !error.contains("activate a project environment with pytest")
        || selected.join("target/ripr/repair-attempts").exists()
        || !error.contains("No module named 'pytest'")
    {
        return Err(format!(
            "missing environment did not stop before edit with its prerequisite: {error}"
        ));
    }
    write_json(
        &evidence.join("missing-environment-result.json"),
        &json!({"scope":"workspace_preparation_negative_control",
        "error":error,"attempt_started":false,"test_edit":"not_run","project_verification":"not_run","receipt":"not_run"}),
    )
}

fn workspace_preparation() -> Result<(), String> {
    let binary = PathBuf::from(
        std::env::var_os("RIPR_PYTHON_PREPARATION_BINARY")
            .ok_or("set RIPR_PYTHON_PREPARATION_BINARY")?,
    );
    if !binary.is_absolute() {
        return Err("workspace preparation binary must be absolute".into());
    }
    let binary_before = workspace_binary_identity(&binary)?;
    let evidence = PathBuf::from(
        std::env::var_os("RIPR_PYTHON_PREPARATION_EVIDENCE")
            .ok_or("set RIPR_PYTHON_PREPARATION_EVIDENCE to a fresh directory")?,
    );
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../fixtures/blind_journey_installed_python");
    let manifest = plain_bytes(&fixture.join("manifest.json"))?;
    let git = |root: &Path, args: &[&str], date: Option<&str>| {
        let args = args.iter().map(|s| (*s).to_string()).collect::<Vec<_>>();
        let mut environment = vec![
            ("GIT_CONFIG_NOSYSTEM", "1"),
            ("GIT_CONFIG_GLOBAL", "/dev/null"),
            ("GIT_AUTHOR_NAME", "ripr fixture"),
            ("GIT_COMMITTER_NAME", "ripr fixture"),
            ("GIT_AUTHOR_EMAIL", "fixture@example.invalid"),
            ("GIT_COMMITTER_EMAIL", "fixture@example.invalid"),
        ];
        if let Some(date) = date {
            environment.extend([("GIT_AUTHOR_DATE", date), ("GIT_COMMITTER_DATE", date)]);
        }
        let output = crate::run::capture_bytes_in_dir_with_timeout(
            Path::new("git"),
            &args,
            root,
            &environment,
            &[
                "GIT_INDEX_FILE",
                "GIT_WORK_TREE",
                "GIT_DIR",
                "GIT_OBJECT_DIRECTORY",
                "GIT_ALTERNATE_OBJECT_DIRECTORIES",
            ],
            std::time::Duration::from_secs(30),
            "Python workspace fixture Git",
        )?;
        command_result(output)
    };
    let result = journey(
        &evidence,
        &manifest,
        |snapshot, path| plain_bytes(&fixture.join("repository").join(snapshot).join(path)),
        git,
        |args, cwd, context| {
            command_result(crate::run::capture_bytes_in_dir_with_timeout(
                &binary,
                args,
                cwd,
                &[],
                &[],
                std::time::Duration::from_secs(30),
                context,
            )?)
        },
    )?;
    let mut result = result;
    result["proof_plane"] = json!("workspace_real_runtime_protocol_preparation");
    let binary_after = workspace_binary_identity(&binary)?;
    if binary_before != binary_after {
        return Err("workspace preparation executable changed during replay".into());
    }
    result["workspace_binary"] = binary_before;
    write_json(&evidence.join("workspace-preparation-result.json"), &result)?;
    eprintln!(
        "Python workspace preparation evidence: {}",
        evidence.display()
    );
    Ok(())
}

fn workspace_binary_identity(path: &Path) -> Result<Value, String> {
    use std::io::Read;
    let meta = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !meta.is_file() || meta.len() == 0 || meta.len() > 512 * 1024 * 1024 {
        return Err("workspace executable is not an ordinary bounded binary".into());
    }
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut count = 0_u64;
    loop {
        let read = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        count += read as u64;
        if count > meta.len() {
            return Err("workspace executable grew during identity observation".into());
        }
        hash.update(&buffer[..read]);
    }
    if count != meta.len() {
        return Err("workspace executable shortened during identity observation".into());
    }
    Ok(
        json!({"path":path,"bytes":count,"sha256":format!("{:x}",hash.finalize()),"authority":"workspace observation only"}),
    )
}

fn command_result(output: crate::run::TimedBytesOutput) -> Result<CommandResult, String> {
    if output.timed_out {
        return Err("workspace preparation deadline exceeded".into());
    }
    let status = output
        .status
        .ok_or("workspace preparation terminal status absent")?;
    Ok(CommandResult {
        status: status.code(),
        success: status.success(),
        stdout: String::from_utf8(output.stdout).map_err(|e| e.to_string())?,
        stderr: String::from_utf8(output.stderr).map_err(|e| e.to_string())?,
    })
}
