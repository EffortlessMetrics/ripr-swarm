//! Exercise the selected project interpreter through an isolated install layout.

use std::fs;
use std::path::Path;

use super::identity::sha256_file;
use super::isolation::{require_clean_consumer_path, require_project_python_idle};
use super::oracles::require_explicit_rust_only_is_limited_not_clean;
use super::support::{
    TempRoot, copy_tree, require_success, run_bash_script, stdout_text, workspace_root,
    write_executable,
};

const BASE_PATH: &str = "/usr/bin:/bin";
const NODE: &str = "tests/test_pricing.py::test_calculate_discount_smoke";
const PROJECT_PYTHON: &str = r#"#!/bin/sh
[ "$(pwd -P)" = "$EXPECTED_PROJECT" ] || exit 71
[ "$#" -eq 3 ] && [ "$1" = -m ] && [ "$2" = pytest ] && [ "$3" = "$EXPECTED_NODE" ] || exit 72
[ -f "${3%%::*}" ] || exit 73
printf '%s\n' "$3" > "$PROJECT_PYTHON_RAN"
"#;

#[test]
fn installed_check_prints_a_verify_command_for_the_selected_project_environment()
-> Result<(), String> {
    // Cargo sets TMPDIR inside the build tree. Run the consumer outside it.
    let temp = TempRoot::new_in(Path::new("/tmp"), "project-environment")?;
    let root = temp
        .path
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let project = root.join("selected ' project");
    let foreign = root.join("foreign project");
    let tool_bin = root.join("tool/bin");
    let ambient_bin = root.join("ambient/bin");
    for dir in [&tool_bin, &ambient_bin] {
        fs::create_dir_all(dir).map_err(|error| format!("create {}: {error}", dir.display()))?;
    }
    let fixture = workspace_root().join("fixtures/python/basic");
    copy_tree(&fixture, &project)?;
    copy_tree(&fixture, &foreign)?;
    fs::write(
        project.join("ripr.toml"),
        "[languages]\nenabled = [\"python\"]\n",
    )
    .map_err(|error| format!("write selected project config: {error}"))?;
    fs::write(
        foreign.join("ripr.toml"),
        "[languages]\nenabled = [\"rust\"]\n",
    )
    .map_err(|error| format!("write foreign project config: {error}"))?;

    // This is an install-layout fixture using the source-built test binary.
    let built = Path::new(env!("CARGO_BIN_EXE_ripr"));
    let installed = tool_bin.join("ripr");
    fs::copy(built, &installed).map_err(|error| format!("stage ripr: {error}"))?;
    let installed_sha = sha256_file(&installed)?;
    assert_eq!(installed_sha, sha256_file(built)?);

    let venv = project.join(".venv");
    write_executable(&venv.join("bin/python"), PROJECT_PYTHON)?;
    fs::write(venv.join("pyvenv.cfg"), "version = 3.13.0\n")
        .map_err(|error| format!("write project venv metadata: {error}"))?;
    let pytest = venv.join("lib/python3.13/site-packages/pytest");
    fs::create_dir_all(&pytest).map_err(|error| format!("create pytest stand-in: {error}"))?;
    fs::write(pytest.join("__init__.py"), "")
        .map_err(|error| format!("write pytest stand-in: {error}"))?;

    write_decoy(&ambient_bin.join("ripr"), "ambient ripr", 74)?;
    write_decoy(&ambient_bin.join("python"), "ambient python", 75)?;
    write_decoy(&tool_bin.join("python"), "tool python", 76)?;
    write_decoy(
        &foreign.join(".venv/bin/python"),
        "foreign project python",
        77,
    )?;
    require_clean_consumer_path(BASE_PATH, Some(&project))?;
    let tool_path = format!(
        "{}:{}:{BASE_PATH}",
        tool_bin.display(),
        ambient_bin.display()
    );
    let ambient_path = format!(
        "{}:{}:{BASE_PATH}",
        ambient_bin.display(),
        tool_bin.display()
    );
    let sentinel = root.join("project-python-ran");
    let env = [
        ("HOME", utf8(&root)?),
        ("EXPECTED_PROJECT", utf8(&project)?),
        ("EXPECTED_NODE", NODE),
        ("PROJECT_PYTHON_RAN", utf8(&sentinel)?),
    ];
    let run = |script: &str, path: &str| {
        run_bash_script(&root.join("command.sh"), script, &foreign, path, &env)
    };

    let check = r#"ripr check --root "$EXPECTED_PROJECT" --diff "$EXPECTED_PROJECT/diff.patch""#;
    let checked = run(check, &tool_path)?;
    require_success(&checked, "installed Python check from the foreign project")?;
    let report = stdout_text(&checked);
    let action = report
        .lines()
        .find_map(|line| line.trim().strip_prefix("Verify command (Bash): "))
        .and_then(|line| line.strip_suffix(" (high)"))
        .ok_or_else(|| format!("missing high-confidence project verify action:\n{report}"))?;
    let relative = format!(".venv/bin/python -m pytest {NODE}");
    let body = action
        .strip_suffix(')')
        .and_then(|line| line.rsplit_once(" && "))
        .map(|(_, command)| command)
        .ok_or_else(|| format!("missing rooted verify command: {action}"))?;
    assert_eq!(
        body, relative,
        "verify command must name the project interpreter and test"
    );

    let foreign_check = run(
        r#"ripr check --root "$PWD" --diff "$PWD/diff.patch" --json"#,
        &tool_path,
    )?;
    require_success(&foreign_check, "installed check against the foreign config")?;
    let foreign_report = serde_json::from_slice(&foreign_check.stdout)
        .map_err(|error| format!("foreign check did not return a JSON report: {error}"))?;
    require_explicit_rust_only_is_limited_not_clean(&foreign_report)?;
    require_project_python_idle(&sentinel)?;

    let path_python = action.replacen(body, &format!("python -m pytest {NODE}"), 1);
    for (command, path, code, label) in [
        (check, &ambient_path, 74, "ambient ripr"),
        (path_python.as_str(), &ambient_path, 75, "ambient python"),
        (path_python.as_str(), &tool_path, 76, "tool python"),
        (body, &tool_path, 77, "foreign project python"),
    ] {
        let rejected = run(command, path)?;
        assert_eq!(rejected.status.code(), Some(code), "{label}: {rejected:?}");
        assert_eq!(stdout_text(&rejected).trim(), label);
    }
    require_project_python_idle(&sentinel)?;

    let verified = run(action, &ambient_path)?;
    require_success(&verified, "replay the printed project verify action")?;
    let selected = fs::read_to_string(&sentinel).map_err(|error| {
        format!("project interpreter did not record the test selector: {error}")
    })?;
    assert_eq!(selected.trim_end(), NODE);
    eprintln!(
        "source-built install-layout payload sha256={installed_sha}; action={action}; selected={NODE}; controls=74,75,76,77"
    );
    Ok(())
}

fn utf8(path: &Path) -> Result<&str, String> {
    path.to_str()
        .ok_or_else(|| format!("path is not UTF-8: {path:?}"))
}

fn write_decoy(path: &Path, label: &str, code: u8) -> Result<(), String> {
    write_executable(
        path,
        &format!("#!/bin/sh\nprintf '%s\\n' '{label}'\nexit {code}\n"),
    )
}
