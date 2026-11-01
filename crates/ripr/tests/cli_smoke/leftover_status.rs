// #11570 item 5 and #11395 residuals, through the real `ripr status` command.
use super::{run_ripr, unique_temp_workspace};

fn status_text(label: &str, file: &str, text: &str) -> Result<String, Box<dyn std::error::Error>> {
    let root = unique_temp_workspace(label);
    std::fs::create_dir_all(&root)?;
    std::fs::write(root.join(file), text)?;
    let root_arg = root.display().to_string();
    let output = run_ripr(&["status", "--root", &root_arg]);
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[test]
fn status_keeps_the_pilot_route_for_python_only_and_typescript_only_roots()
-> Result<(), Box<dyn std::error::Error>> {
    for (label, file, text) in [
        ("status-py-only", "app.py", "def f(a):\n    return a > 5\n"),
        (
            "status-ts-only",
            "app.ts",
            "export const f = (a: number) => a > 5;\n",
        ),
    ] {
        let rendered = status_text(label, file, text)?;
        assert!(
            rendered.contains("ripr pilot --root"),
            "{label}: {rendered}"
        );
        assert!(
            !rendered.contains("nothing to repair yet"),
            "{label}: {rendered}"
        );
    }
    Ok(())
}

#[test]
fn status_pilot_step_is_plain_and_uses_the_short_shell_note()
-> Result<(), Box<dyn std::error::Error>> {
    let rendered = status_text(
        "status-cargo-root",
        "Cargo.toml",
        "[package]\nname = \"x\"\nversion = \"0.1.0\"\n",
    )?;
    assert!(rendered.contains("no gap is selected yet"), "{rendered}");
    assert!(!rendered.contains("repair seam"), "{rendered}");
    if !cfg!(windows) {
        assert!(
            rendered.contains("runs unchanged in PowerShell"),
            "{rendered}"
        );
        assert!(!rendered.contains("WSL bash"), "{rendered}");
    }
    Ok(())
}
