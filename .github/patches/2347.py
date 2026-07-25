from pathlib import Path

LOOP = Path("crates/ripr/src/agent/loop_commands.rs")
POLICY = Path("policy/process_allowlist.txt")


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one match, found {count}")
    return text.replace(old, new, 1)


text = LOOP.read_text(encoding="utf-8")
old_impl = r'''pub(crate) fn shell_arg(value: &str) -> String {
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '/' | '\\' | '_' | '-' | ':'))
    {
        return value.to_string();
    }
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}
'''
new_impl = r'''/// Render `value` as one complete Bash argv token for advisory command text.
///
/// Non-empty `[A-Za-z0-9./_:-]` values remain readable without quotes. Every
/// other value uses Bash's single-quote form, with embedded single quotes
/// represented by the standard `'\''` splice. Single quotes are the only Bash
/// quoting form with no interpretation inside, so `$`, backticks, `!`,
/// backslashes, newlines, redirect characters, and empty values all round-trip
/// without expansion or token loss.
pub(crate) fn shell_arg(value: &str) -> String {
    if !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '/' | '_' | '-' | ':'))
    {
        return value.to_string();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}
'''
text = replace_once(text, old_impl, new_impl, "shell_arg implementation")

replacements = {
    'assert_eq!(shell_arg("repo root"), "\\\"repo root\\\"");':
        'assert_eq!(shell_arg("repo root"), "\'repo root\'");',
    '"ripr check --root . --mode \\\"draft mode\\\" --format agent-seam-packets-json > target/ripr/workflow/packets.json"':
        '"ripr check --root . --mode \'draft mode\' --format agent-seam-packets-json > target/ripr/workflow/packets.json"',
    '"ripr agent start --root \\\"repo root\\\" --seam-id \\\"seam a\\\" --out \\\"target/ripr/work flow\\\""':
        '"ripr agent start --root \'repo root\' --seam-id \'seam a\' --out \'target/ripr/work flow\'"',
    '"ripr check --root \\\"repo root\\\" --mode draft --format repo-exposure-json > \\\"target/ripr/work flow/before.json\\\""':
        '"ripr check --root \'repo root\' --mode draft --format repo-exposure-json > \'target/ripr/work flow/before.json\'"',
    '"ripr check --root \\\"repo root\\\" --base \\\"origin/main with space\\\" --mode draft --format repo-exposure-json > \\\"target/ripr/work flow/before.json\\\""':
        '"ripr check --root \'repo root\' --base \'origin/main with space\' --mode draft --format repo-exposure-json > \'target/ripr/work flow/before.json\'"',
    '"ripr agent verify --root \\\"repo root\\\" --before \\\"target/ripr/work flow/before.json\\\" --after \\\"target/ripr/work flow/after.json\\\" --json > \\\"target/ripr/work flow/verify.json\\\""':
        '"ripr agent verify --root \'repo root\' --before \'target/ripr/work flow/before.json\' --after \'target/ripr/work flow/after.json\' --json > \'target/ripr/work flow/verify.json\'"',
    '"ripr agent receipt --root \\\"repo root\\\" --verify-json \\\"target/ripr/work flow/verify.json\\\" --seam-id \\\"seam a\\\" --json --out \\\"target/ripr/work flow/receipt.json\\\""':
        '"ripr agent receipt --root \'repo root\' --verify-json \'target/ripr/work flow/verify.json\' --seam-id \'seam a\' --json --out \'target/ripr/work flow/receipt.json\'"',
}
for old, new in replacements.items():
    text = replace_once(text, old, new, f"loop command expectation {old[:40]}")

module_end = text.rfind("\n}")
if module_end < 0:
    raise SystemExit("loop_commands test module end not found")
additional_tests = r'''

    #[test]
    fn shell_arg_is_total_for_empty_backslash_quotes_and_metacharacters() {
        assert_eq!(shell_arg(""), "''");
        assert_eq!(shell_arg("plain/path:one-two"), "plain/path:one-two");
        assert_eq!(shell_arg("a\\b"), "'a\\b'");
        assert_eq!(shell_arg("a'b"), "'a'\\''b'");
        assert_eq!(shell_arg("$HOME"), "'$HOME'");
        assert_eq!(shell_arg("`id`"), "'`id`'");
        assert_eq!(shell_arg("with!bang"), "'with!bang'");
        assert_eq!(
            shell_arg("gap:pr:amount>=threshold"),
            "'gap:pr:amount>=threshold'"
        );
    }

    #[cfg(unix)]
    fn run_bash(script: &str, cwd: Option<&std::path::Path>) -> Result<std::process::Output, String> {
        let mut command = std::process::Command::new("bash");
        command
            .args(["--noprofile", "--norc", "-c", script])
            .env_remove("BASH_ENV")
            .env_remove("ENV");
        if let Some(cwd) = cwd {
            command.current_dir(cwd);
        }
        command
            .output()
            .map_err(|err| format!("run Bash argv proof failed: {err}"))
    }

    #[cfg(unix)]
    #[test]
    fn shell_arg_round_trips_hostile_values_through_real_bash() -> Result<(), String> {
        for value in [
            "",
            "plain",
            "repo root",
            "a\\b",
            "a'b",
            "a\"b",
            "$HOME",
            "$(printf injected)",
            "`printf injected`",
            "with!bang",
            "gap:pr:amount>=threshold",
            "line\nbreak",
        ] {
            let script = format!("set -- {}; printf '%s' \"$1\"", shell_arg(value));
            let output = run_bash(&script, None)?;
            if !output.status.success() {
                return Err(format!(
                    "Bash rejected shell_arg({value:?}): {}",
                    String::from_utf8_lossy(&output.stderr)
                ));
            }
            if output.stdout != value.as_bytes() {
                return Err(format!(
                    "shell_arg did not round-trip: input={value:?} output={:?} token={}",
                    String::from_utf8_lossy(&output.stdout),
                    shell_arg(value)
                ));
            }
        }
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn redirect_shaped_gap_id_cannot_open_a_second_redirect() -> Result<(), String> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|err| format!("system clock before UNIX_EPOCH: {err}"))?
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ripr-shell-arg-redirect-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root)
            .map_err(|err| format!("create redirect proof root failed: {err}"))?;
        let value = "gap:pr:amount>=threshold";
        let script = format!("printf '%s' {} > result.txt", shell_arg(value));
        let output = run_bash(&script, Some(&root))?;
        if !output.status.success() {
            return Err(format!(
                "Bash rejected redirect proof: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        let actual = std::fs::read(root.join("result.txt"))
            .map_err(|err| format!("read redirect proof result failed: {err}"))?;
        if actual != value.as_bytes() {
            return Err(format!(
                "redirect proof changed the gap id: {:?}",
                String::from_utf8_lossy(&actual)
            ));
        }
        if root.join("threshold").exists() {
            return Err("redirect-shaped gap id created an unintended `threshold` file".to_string());
        }
        std::fs::remove_dir_all(&root)
            .map_err(|err| format!("remove redirect proof root failed: {err}"))?;
        Ok(())
    }
'''
text = text[:module_end] + additional_tests + text[module_end:]
LOOP.write_text(text, encoding="utf-8")

policy = POLICY.read_text(encoding="utf-8")
entry = "crates/ripr/src/agent/loop_commands.rs|Command::new|1|agent-command-tests|#2347: Unix-only tests execute generated advisory argv tokens through Bash and prove exact round-trip plus redirect confinement; test-only process surface.\n"
if entry not in policy:
    anchor = "crates/ripr/src/agent/artifact.rs|use std::process::Command|1|repair-artifact-provenance|RIPR-SPEC-0134: artifact validation imports Command for the bounded git provenance probes described above.\n"
    policy = replace_once(policy, anchor, anchor + entry, "process policy anchor")
POLICY.write_text(policy, encoding="utf-8")
