// #11570 item 5 and #11395 residuals, through the real `ripr status` command.
use super::{run_ripr, unique_temp_workspace};

fn status_text(label: &str, file: &str, text: &str) -> Result<String, Box<dyn std::error::Error>> {
    status_text_in(&unique_temp_workspace(label), file, text)
}

fn status_text_in(
    root: &std::path::Path,
    file: &str,
    text: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    std::fs::create_dir_all(root)?;
    std::fs::write(root.join(file), text)?;
    let root_arg = root.display().to_string();
    let output = run_ripr(&["status", "--root", &root_arg]);
    let _ = std::fs::remove_dir_all(root);
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
    assert!(
        rendered.contains("no repair gap is known yet"),
        "{rendered}"
    );
    assert!(!rendered.contains("repair seam"), "{rendered}");
    if !cfg!(windows) {
        assert!(rendered.contains(SHORT_NOTE), "{rendered}");
        assert!(!rendered.contains("WSL bash"), "{rendered}");
    }
    Ok(())
}

const SHORT_NOTE: &str =
    "This command is written for Bash and runs unchanged in PowerShell. cmd.exe is not supported.";

/// The bug class of the #11434 paste failure: a root whose name needs shell
/// quoting. A space keeps the command identical in PowerShell (short note); an
/// apostrophe needs a different PowerShell form (full paragraph).
#[cfg(not(windows))]
#[test]
fn status_shell_note_follows_whether_the_root_needs_a_second_form()
-> Result<(), Box<dyn std::error::Error>> {
    let base = unique_temp_workspace("status-quoting");
    let manifest = "[package]\nname = \"x\"\nversion = \"0.1.0\"\n";

    let spaced = status_text_in(&base.join("with space"), "Cargo.toml", manifest)?;
    assert!(spaced.contains("with space"), "{spaced}");
    assert!(spaced.contains(SHORT_NOTE), "{spaced}");

    let quoted = status_text_in(&base.join("it's here"), "Cargo.toml", manifest)?;
    let _ = std::fs::remove_dir_all(&base);
    assert!(quoted.contains("it's here"), "{quoted}");
    assert!(!quoted.contains(SHORT_NOTE), "{quoted}");
    assert!(quoted.contains("PowerShell"), "{quoted}");
    Ok(())
}
