//! Execute the local-wheelhouse quickstart with a synthetic script wheel.
//! Fixture processes use the existing Rust runner; Python only writes wheel data.
//! This checks interpreter/entrypoint selection, not native RIPR qualification.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const README: &str = include_str!("../../../../packaging/python/README.md");
const PAYLOAD: &str = r#"#!/bin/sh
printf "%s\n" "$0" > "$RIPR_FIXTURE_EXECUTED"
printf "selected-wheel-fixture 1.0.0\n"
"#;
const WHEEL_FIXTURE: &str = r#"
import base64, csv, hashlib, io, pathlib, sys, zipfile
wheelhouse = pathlib.Path(sys.argv[1])
wheelhouse.mkdir()
prefix = "ripr_rs-1.0.0"
entries = {
    prefix + ".data/scripts/ripr": pathlib.Path(sys.argv[2]).read_bytes(),
    prefix + ".dist-info/METADATA": b"Metadata-Version: 2.1\nName: ripr-rs\nVersion: 1.0.0\nRequires-Python: >=3.10\n\nSynthetic selection fixture, not RIPR.\n",
    prefix + ".dist-info/WHEEL": b"Wheel-Version: 1.0\nGenerator: selection-test\nRoot-Is-Purelib: false\nTag: py3-none-any\n",
}
record = io.StringIO()
writer = csv.writer(record, lineterminator="\n")
for name, data in entries.items():
    digest = base64.urlsafe_b64encode(hashlib.sha256(data).digest()).decode().rstrip("=")
    writer.writerow([name, "sha256=" + digest, len(data)])
record_name = prefix + ".dist-info/RECORD"
writer.writerow([record_name, "", ""])
entries[record_name] = record.getvalue().encode()
with zipfile.ZipFile(wheelhouse / (prefix + "-py3-none-any.whl"), "w") as archive:
    for name, data in entries.items():
        info = zipfile.ZipInfo(name)
        info.create_system = 3
        mode = 0o100755 if ".data/scripts/" in name else 0o100644
        info.external_attr = mode << 16
        archive.writestr(info, data)
print(sys.executable)
"#;

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Result<Self, String> {
        let parent = std::env::temp_dir();
        fs::create_dir_all(&parent).map_err(|error| format!("test temp root: {error}"))?;
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let path = parent.join(format!(
            "ripr-wheel-guidance-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&path).map_err(|error| format!("create {}: {error}", path.display()))?;
        Ok(Self(path))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.0) {
            eprintln!("retained test scratch {}: {error}", self.0.display());
        }
    }
}

fn path_text(path: &Path) -> Result<&str, String> {
    path.to_str()
        .ok_or_else(|| format!("fixture path is not UTF-8: {}", path.display()))
}

fn run_fixture(
    program: &Path,
    args: &[String],
    cwd: &Path,
    envs: &[(&str, &str)],
    deadline: Instant,
) -> Result<String, String> {
    let timeout = deadline
        .saturating_duration_since(Instant::now())
        .min(Duration::from_secs(30));
    let output = crate::run::capture_bytes_in_dir_with_timeout(
        program,
        args,
        cwd,
        envs,
        &[
            "BASH_ENV",
            "ENV",
            "PIP_TARGET",
            "PIP_PREFIX",
            "PIP_ROOT",
            "PIP_USER",
            "PIP_PYTHON",
            "PIP_LOG",
            "PYTHONHOME",
            "PYTHONPATH",
        ],
        timeout,
        "local wheelhouse README fixture",
    )?;
    if output.timed_out || !output.status.is_some_and(|status| status.success()) {
        return Err(format!(
            "fixture command failed: program={} args={args:?} stdout={} stderr={}",
            program.display(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    String::from_utf8(output.stdout).map_err(|error| format!("fixture stdout: {error}"))
}

fn exercise_recipe(
    recipe: &str,
    cwd: &Path,
    tool_name: &str,
    wheelhouse_name: &str,
    deadline: Instant,
) -> Result<(), String> {
    fs::create_dir(cwd).map_err(|error| format!("create fixture cwd: {error}"))?;
    let payload = cwd.join("expected-payload");
    fs::write(&payload, PAYLOAD).map_err(|error| format!("write fixture payload: {error}"))?;
    let decoys = cwd.join("ambient tools");
    fs::create_dir(&decoys).map_err(|error| format!("create decoy directory: {error}"))?;
    let decoy = decoys.join("ripr");
    fs::write(
        &decoy,
        "#!/bin/sh\nprintf 'ambient\\n' > \"$RIPR_DECOY_EXECUTED\"\nprintf 'ambient-unrelated-ripr\\n'\n",
    )
    .map_err(|error| format!("write decoy: {error}"))?;
    fs::set_permissions(&decoy, fs::Permissions::from_mode(0o755))
        .map_err(|error| format!("chmod decoy: {error}"))?;

    let wheelhouse = cwd.join(wheelhouse_name);
    let selected = cwd.join("selected python ü");
    let selected_python = selected.join("bin/python");
    let decoy_marker = cwd.join("ambient-executed");
    let installed_marker = cwd.join("installed-executed");
    let cache = cwd.join("pip-cache");
    let path = format!("{}:/usr/bin:/bin", path_text(&decoys)?);
    let fixture_env = [
        ("PATH", path.as_str()),
        ("HOME", path_text(cwd)?),
        ("PIP_NO_INDEX", "1"),
        ("PIP_DISABLE_PIP_VERSION_CHECK", "1"),
        ("PIP_CONFIG_FILE", "/dev/null"),
        ("PIP_CACHE_DIR", path_text(&cache)?),
        ("RIPR_FIXTURE_EXECUTED", path_text(&installed_marker)?),
        ("RIPR_DECOY_EXECUTED", path_text(&decoy_marker)?),
        ("PYTHONDONTWRITEBYTECODE", "1"),
        ("LC_ALL", "C.UTF-8"),
    ];
    // Resolve the host interpreter before giving the recipe its controlled PATH.
    let mut bootstrap_env = fixture_env.to_vec();
    bootstrap_env.retain(|(name, _)| *name != "PATH");
    let python = run_fixture(
        Path::new("python3"),
        &[
            "-c".to_string(),
            WHEEL_FIXTURE.to_string(),
            path_text(&wheelhouse)?.to_string(),
            path_text(&payload)?.to_string(),
        ],
        cwd,
        &bootstrap_env,
        deadline,
    )?;
    let python = python.trim();
    run_fixture(
        Path::new(python),
        &[
            "-m".to_string(),
            "venv".to_string(),
            path_text(&selected)?.to_string(),
        ],
        cwd,
        &fixture_env,
        deadline,
    )?;
    // This selected interpreter is deliberately unactivated for the old recipe.
    let mut recipe_env = fixture_env.to_vec();
    recipe_env.push(("SELECTED_PYTHON", path_text(&selected_python)?));
    recipe_env.push(("BOOTSTRAP_PYTHON", python));
    let case_recipe = recipe
        .replace(".ripr-tool", tool_name)
        .replace("./wheelhouse", &format!("./{wheelhouse_name}"));
    let script = format!(
        "set -eu\npython() {{ \"$SELECTED_PYTHON\" \"$@\"; }}\npython3() {{ \"$BOOTSTRAP_PYTHON\" \"$@\"; }}\n{case_recipe}"
    );
    let stdout = run_fixture(
        Path::new("/bin/bash"),
        &[
            "--noprofile".to_string(),
            "--norc".to_string(),
            "-c".to_string(),
            script,
        ],
        cwd,
        &recipe_env,
        deadline,
    )?;
    let installed = cwd.join(tool_name).join("bin/ripr");
    assert!(
        installed.is_file() || selected.join("bin/ripr").is_file(),
        "wheel installation did not run"
    );
    assert!(
        !decoy_marker.exists(),
        "recipe executed ambient ripr: {stdout}"
    );
    assert_eq!(stdout.lines().last(), Some("selected-wheel-fixture 1.0.0"));
    assert_eq!(
        fs::read(&installed).map_err(|error| format!("read installed fixture: {error}"))?,
        PAYLOAD.as_bytes()
    );
    let invoked = fs::read_to_string(&installed_marker)
        .map_err(|error| format!("read invocation marker: {error}"))?;
    assert_eq!(
        fs::canonicalize(cwd.join(invoked.trim()))
            .map_err(|error| format!("canonicalize invoked fixture: {error}"))?,
        fs::canonicalize(&installed)
            .map_err(|error| format!("canonicalize installed fixture: {error}"))?
    );
    let followup = case_recipe
        .trim()
        .lines()
        .last()
        .ok_or_else(|| "empty recipe follow-up".to_string())?;
    let stdout = run_fixture(
        Path::new("/bin/bash"),
        &[
            "--noprofile".to_string(),
            "--norc".to_string(),
            "-c".to_string(),
            followup.to_string(),
        ],
        cwd,
        &fixture_env,
        deadline,
    )?;
    assert_eq!(stdout, "selected-wheel-fixture 1.0.0\n");
    assert!(!decoy_marker.exists());
    Ok(())
}

#[test]
fn local_wheelhouse_recipe_selects_its_installed_executable() -> Result<(), String> {
    let section = README
        .split_once("For a reviewed local wheelhouse during qualification:")
        .ok_or_else(|| "missing local wheelhouse section".to_string())?
        .1;
    let recipe = section
        .split_once("```console\n")
        .and_then(|(_, after)| after.split_once("```"))
        .map(|(recipe, _)| recipe)
        .ok_or_else(|| "missing POSIX wheelhouse recipe".to_string())?;
    assert!(
        recipe.trim().lines().count() >= 2,
        "empty wheelhouse recipe"
    );
    let scratch = Scratch::new()?;
    let deadline = Instant::now() + Duration::from_mins(2);
    for (label, tool_name, wheelhouse_name) in [
        ("plain", ".ripr-tool", "wheelhouse"),
        (
            "consumer tools ü with spaces",
            "tool environment ü",
            "reviewed wheels ü",
        ),
    ] {
        exercise_recipe(
            recipe,
            &scratch.0.join(label),
            tool_name,
            wheelhouse_name,
            deadline,
        )?;
    }
    Ok(())
}
