//! Runs the built `ripr completions` binary end to end (#7438).

use std::path::PathBuf;
use std::process::Command;

struct Run {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

fn ripr(args: &[&str]) -> Run {
    match Command::new(env!("CARGO_BIN_EXE_ripr")).args(args).output() {
        Ok(output) => Run {
            code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        },
        Err(err) => Run {
            code: None,
            stdout: String::new(),
            stderr: format!("spawn failed: {err}"),
        },
    }
}

#[test]
fn each_shell_prints_a_script_and_exits_zero() {
    let markers = [
        ("bash", "complete -F _ripr ripr"),
        ("zsh", "compdef _ripr ripr"),
        ("fish", "complete -c ripr"),
        (
            "powershell",
            "Register-ArgumentCompleter -Native -CommandName ripr",
        ),
    ];
    for (shell, marker) in markers {
        let run = ripr(&["completions", shell]);
        assert_eq!(run.code, Some(0), "{shell}: {}", run.stderr);
        assert!(run.stdout.contains(marker), "{shell}: {}", run.stdout);
        assert!(run.stdout.contains("check"), "{shell} omits check");
        assert!(
            run.stdout.contains("completions"),
            "{shell} omits completions"
        );
        assert!(run.stderr.is_empty(), "{shell}: {}", run.stderr);
    }
}

#[test]
fn missing_shell_is_refused_with_the_accepted_names() {
    let run = ripr(&["completions"]);
    assert_eq!(run.code, Some(2), "{}", run.stderr);
    assert!(run.stdout.is_empty(), "{}", run.stdout);
    assert!(run.stderr.contains("needs a shell name"), "{}", run.stderr);
    assert!(
        run.stderr.contains("bash, zsh, fish, powershell"),
        "{}",
        run.stderr
    );
}

#[test]
fn unknown_shell_is_refused_with_a_suggestion() {
    let run = ripr(&["completions", "tcsh"]);
    assert_eq!(run.code, Some(2), "{}", run.stderr);
    assert!(run.stdout.is_empty(), "{}", run.stdout);
    assert!(run.stderr.contains("tcsh"), "{}", run.stderr);
    assert!(
        run.stderr.contains("Accepted: bash, zsh, fish, powershell"),
        "{}",
        run.stderr
    );
}

/// Syntax-checks and, for bash, sources each script in the shells installed on
/// this machine. `RIPR_REQUIRE_COMPLETION_SHELLS=1` (set by the hosted lane)
/// turns a missing shell into a failure instead of a skip.
#[test]
fn scripts_load_in_the_installed_shells() {
    let require = std::env::var_os("RIPR_REQUIRE_COMPLETION_SHELLS").is_some();
    let dir = std::env::temp_dir().join(format!("ripr-completions-{}", std::process::id()));
    assert_eq!(
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string()),
        Ok(())
    );
    let checks: [(&str, &str, &[&str]); 4] = [
        ("bash", "bash", &["-n"]),
        ("bash", "bash", &["-c", "source \"$0\" && complete -p ripr"]),
        ("zsh", "zsh", &["-n"]),
        ("fish", "fish", &["--no-execute"]),
    ];
    for (shell, program, flags) in checks {
        let script = ripr(&["completions", shell]).stdout;
        let file: PathBuf = dir.join(format!("ripr.{shell}"));
        assert_eq!(
            std::fs::write(&file, script).map_err(|e| e.to_string()),
            Ok(())
        );
        let mut command = Command::new(program);
        command.args(flags);
        command.arg(&file);
        match command.output() {
            Ok(output) => assert!(
                output.status.success(),
                "{program} {flags:?} rejected the {shell} script: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
            Err(_) => assert!(!require, "{program} is required but not installed"),
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}
