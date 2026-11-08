//! Runs the built `ripr completions` binary end to end (#7438).

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
/// turns a missing shell into a failure instead of a skip. Unix only: on
/// Windows `bash` can resolve to the WSL launcher rather than a real shell.
#[cfg(unix)]
#[test]
fn scripts_load_in_the_installed_shells() {
    let require = std::env::var("RIPR_REQUIRE_COMPLETION_SHELLS")
        .is_ok_and(|value| !value.is_empty() && value != "0");
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
        let file: std::path::PathBuf = dir.join(format!("ripr.{shell}"));
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

/// Runs an installed shell and returns its stdout. `None` means the shell is
/// not installed and `RIPR_REQUIRE_COMPLETION_SHELLS` did not require it.
#[cfg(unix)]
fn shell_stdout(program: &str, args: &[&str]) -> Option<String> {
    let require = std::env::var("RIPR_REQUIRE_COMPLETION_SHELLS")
        .is_ok_and(|value| !value.is_empty() && value != "0");
    match Command::new(program).args(args).output() {
        Ok(output) => {
            assert!(
                output.status.success(),
                "{program} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            Some(String::from_utf8_lossy(&output.stdout).into_owned())
        }
        Err(_) => {
            assert!(!require, "{program} is required but not installed");
            None
        }
    }
}

#[cfg(unix)]
fn script_file(shell: &str) -> std::path::PathBuf {
    let file = std::env::temp_dir().join(format!("ripr-offer-{}.{shell}", std::process::id()));
    assert_eq!(
        std::fs::write(&file, ripr(&["completions", shell]).stdout).map_err(|e| e.to_string()),
        Ok(())
    );
    file
}

/// `cache` offers its sub-words in second position and nothing once one is typed.
#[cfg(unix)]
#[test]
fn bash_offers_the_second_word_only_in_second_position() {
    let file = script_file("bash");
    let path = file.to_string_lossy().into_owned();
    let offer = |words: &str, cword: u8| {
        let script = format!(
            "source \"$0\"; COMP_WORDS=({words}); COMP_CWORD={cword}; _ripr; echo \"${{COMPREPLY[*]}}\""
        );
        shell_stdout("bash", &["-c", &script, &path])
    };
    if let Some(second) = offer("ripr cache ''", 2) {
        assert!(second.contains("status"), "{second}");
        assert!(second.contains("export"), "{second}");
    }
    if let Some(third) = offer("ripr cache status ''", 3) {
        assert_eq!(third.trim(), "", "{third}");
    }
    let _ = std::fs::remove_file(&file);
}

#[cfg(unix)]
#[test]
fn fish_offers_the_second_word_only_in_second_position() {
    let file = script_file("fish");
    let path = file.to_string_lossy().into_owned();
    let offer = |line: &str| {
        let script = format!("source {path}; complete -C '{line}'");
        shell_stdout("fish", &["-c", &script])
    };
    if let Some(second) = offer("ripr cache ") {
        assert!(second.contains("status"), "{second}");
        assert!(second.contains("export"), "{second}");
    }
    if let Some(third) = offer("ripr cache status ") {
        assert!(!third.contains("export"), "{third}");
        assert!(!third.contains("clear"), "{third}");
    }
    let _ = std::fs::remove_file(&file);
}
