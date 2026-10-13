//! Execute the local-wheelhouse quickstart with a synthetic script wheel.
//! This checks interpreter/entrypoint selection, not native RIPR qualification.

use std::time::Duration;

const RECIPE_PROBE: &str = r#"
import base64, csv, hashlib, io, os, pathlib, shlex, subprocess, sys, tempfile, zipfile

readme = pathlib.Path(sys.argv[1]).read_text(encoding="utf-8")
section = readme.split("For a reviewed local wheelhouse during qualification:", 1)[1]
recipe = section.split("```console\n", 1)[1].split("```", 1)[0]
assert len(recipe.strip().splitlines()) >= 2, "empty wheelhouse recipe"

def run(argv, cwd, env):
    result = subprocess.run(argv, cwd=cwd, env=env, capture_output=True, text=True, timeout=30)
    assert result.returncode == 0, (argv, result.returncode, result.stdout, result.stderr)
    return result

def make_wheel(wheelhouse):
    wheelhouse.mkdir()
    prefix = "ripr_rs-1.0.0"
    script = b'#!/bin/sh\nprintf "%s\\n" "$0" > "$RIPR_FIXTURE_EXECUTED"\nprintf "selected-wheel-fixture 1.0.0\\n"\n'
    entries = {
        prefix + ".data/scripts/ripr": script,
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
    return script

with tempfile.TemporaryDirectory(prefix="ripr-wheel-guidance-") as temporary:
    root = pathlib.Path(temporary)
    for label, tool_name, wheelhouse_name in [
        ("plain", ".ripr-tool", "wheelhouse"),
        ("consumer tools ü with spaces", "tool environment ü", "reviewed wheels ü"),
    ]:
        cwd = root / label
        cwd.mkdir()
        payload = make_wheel(cwd / wheelhouse_name)
        # Exercise the guide's quoted path-replacement advice as well as its literal recipe.
        case_recipe = recipe.replace(".ripr-tool", tool_name).replace("./wheelhouse", "./" + wheelhouse_name)
        decoys = cwd / "ambient tools"
        decoys.mkdir()
        decoy_marker = cwd / "ambient-executed"
        installed_marker = cwd / "installed-executed"
        decoy = decoys / "ripr"
        decoy.write_text('#!/bin/sh\nprintf "ambient\\n" > "$RIPR_DECOY_EXECUTED"\nprintf "ambient-unrelated-ripr\\n"\n')
        decoy.chmod(0o755)
        env = {
            "PATH": str(decoys) + ":/usr/bin:/bin",
            "HOME": str(cwd),
            "PIP_NO_INDEX": "1",
            "PIP_DISABLE_PIP_VERSION_CHECK": "1",
            "PIP_CONFIG_FILE": os.devnull,
            "PIP_CACHE_DIR": str(cwd / "pip-cache"),
            "RIPR_FIXTURE_EXECUTED": str(installed_marker),
            "RIPR_DECOY_EXECUTED": str(decoy_marker),
            "LC_ALL": "C.UTF-8",
        }
        # Model a selected but unactivated interpreter for the old recipe.
        selected = cwd / "selected python ü"
        run([sys.executable, "-m", "venv", str(selected)], cwd, env)
        preamble = (
            "set -eu\npython() { " + shlex.quote(str(selected / "bin/python")) + ' "$@"; }\n'
            "python3() { " + shlex.quote(sys.executable) + ' "$@"; }\n'
        )
        result = run(["/bin/bash", "--noprofile", "--norc", "-c", preamble + case_recipe], cwd, env)
        installed = cwd / tool_name / "bin/ripr"
        assert installed.is_file() or (selected / "bin/ripr").is_file(), "wheel installation did not run"
        assert not decoy_marker.exists(), ("recipe executed ambient ripr", result.stdout)
        assert result.stdout.splitlines()[-1] == "selected-wheel-fixture 1.0.0", result.stdout
        assert installed.read_bytes() == payload, "installed fixture bytes changed"
        assert (cwd / installed_marker.read_text().strip()).resolve() == installed.resolve()
        # A new shell still selects the explicit tool path without activation.
        followup = run(["/bin/bash", "--noprofile", "--norc", "-c", case_recipe.strip().splitlines()[-1]], cwd, env)
        assert followup.stdout == "selected-wheel-fixture 1.0.0\n"
        assert not decoy_marker.exists()
        print("PASS:", label, "offline documented recipe and fresh-shell selection")
print("Python:", sys.version)
"#;

#[test]
fn local_wheelhouse_recipe_selects_its_installed_executable() -> Result<(), String> {
    std::fs::create_dir_all(std::env::temp_dir())
        .map_err(|error| format!("create test temp root: {error}"))?;
    let readme = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../packaging/python/README.md");
    let output = crate::run::capture_output_with_timeout(
        "python3",
        &[
            "-c".to_string(),
            RECIPE_PROBE.to_string(),
            readme.to_string_lossy().into_owned(),
        ],
        &[("PYTHONDONTWRITEBYTECODE", "1")],
        Duration::from_secs(120),
        "local wheelhouse README recipe",
    )?;
    if !output.status.is_some_and(|status| status.success()) {
        return Err(format!(
            "wheelhouse recipe failed: stdout={} stderr={}",
            output.stdout, output.stderr
        ));
    }
    Ok(())
}
