//! `ripr doctor` names unanalyzed source languages, but a virtualenv's shell
//! activation scripts are not repository source (#15283).

use std::fs;
use std::path::Path;
use std::process::Command;

fn write(root: &Path, relative: &str, text: &str) -> Result<(), String> {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    fs::write(path, text).map_err(|err| err.to_string())
}

fn doctor_stdout(root: &Path) -> Result<String, String> {
    let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
        .args(["doctor", "--root"])
        .arg(root)
        .output()
        .map_err(|err| err.to_string())?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[test]
fn doctor_census_skips_virtualenvs_but_still_counts_real_scripts() -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("ripr-doctor-venv-census-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    write(&dir, "pyproject.toml", "[project]\nname = \"demo\"\n")?;
    write(&dir, "src/demo.py", "def f(x):\n    return x\n")?;
    // A conventionally named venv, and a custom-named one only the marker finds.
    write(&dir, ".venv/pyvenv.cfg", "home = /usr/bin\n")?;
    write(&dir, ".venv/bin/activate.ps1", "# activate\n")?;
    write(&dir, "custom-env/pyvenv.cfg", "home = /usr/bin\n")?;
    write(&dir, "custom-env/bin/activate.ps1", "# activate\n")?;

    let venv_only = doctor_stdout(&dir)?;
    assert!(
        !venv_only.contains("Unanalyzed languages"),
        "virtualenv scripts must not be counted:\n{venv_only}"
    );

    // Discriminating positive: a real script outside any venv is still named.
    write(&dir, "scripts/deploy.ps1", "Write-Host hi\n")?;
    let with_script = doctor_stdout(&dir)?;
    assert!(
        with_script.contains("Unanalyzed languages: PowerShell (1 file(s))"),
        "a repository script must still be counted exactly once:\n{with_script}"
    );
    let _ = fs::remove_dir_all(&dir);
    Ok(())
}
