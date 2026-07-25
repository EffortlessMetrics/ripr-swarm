from pathlib import Path

LOOP = Path("crates/ripr/src/agent/loop_commands.rs")
POLICY = Path("policy/process_allowlist.txt")
text = LOOP.read_text(encoding="utf-8")


def replace_once(old: str, new: str, label: str) -> None:
    global text
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one match, found {count}")
    text = text.replace(old, new, 1)


old_impl = '''/// Render `value` as a single bash-safe argv token for an advisory command string.
///
/// Tokens whose every character is in the safe set `[a-zA-Z0-9][./\\_:-]` pass
/// through unquoted. Any other character triggers a double-quoted form in which
/// `\\`, `"`, `$`, backtick, and `!` are backslash-escaped so no shell expansion
/// remains inside the quotes. Without the `$`/backtick/`!` escapes, a value
/// such as `$(cmd)` or `` `cmd` `` would still execute as command substitution
/// even when wrapped in double quotes (#2347).
pub(crate) fn shell_arg(value: &str) -> String {
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '/' | '\\' | '_' | '-' | ':'))
    {
        return value.to_string();
    }
    format!(
        "\\\"{}\\\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('$', "\\$")
            .replace('`', "\\`")
            .replace('!', "\\!")
    )
}
'''
new_impl = '''/// Render `value` as one Bash-safe argv token for an advisory command string.
///
/// Non-empty tokens containing only ASCII alphanumerics plus `./_:-` pass
/// through unquoted. Other values normally retain the established double-quote
/// shape, escaping the four characters Bash still interprets there: backslash,
/// double quote, `$`, and backtick. Values containing `!` use a complete
/// single-quote encoder because `\\!` inside double quotes does not round-trip
/// consistently across interactive and non-interactive Bash. Embedded single
/// quotes are represented by the standard `'\\''` splice. Empty input is `''`.
pub(crate) fn shell_arg(value: &str) -> String {
    if !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '/' | '_' | '-' | ':'))
    {
        return value.to_string();
    }
    if value.contains('!') {
        return format!("'{}'", value.replace('\'', "'\\\\''"));
    }
    format!(
        "\\\"{}\\\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('$', "\\$")
            .replace('`', "\\`")
    )
}
'''
replace_once(old_impl, new_impl, "replace partial shell encoder")

old_safe = '''    #[test]
    fn shell_arg_passes_safe_tokens_unquoted() {
        // Every character in the safe set passes through unchanged.
        assert_eq!(shell_arg("a-b_c.d/e:f\\g"), "a-b_c.d/e:f\\g");
        assert_eq!(shell_arg("origin/main"), "origin/main");
        assert_eq!(shell_arg("target/ripr/workflow"), "target/ripr/workflow");
    }
'''
new_safe = '''    #[test]
    fn shell_arg_passes_only_complete_safe_tokens_unquoted() {
        assert_eq!(shell_arg("a-b_c.d/e:f"), "a-b_c.d/e:f");
        assert_eq!(shell_arg("origin/main"), "origin/main");
        assert_eq!(shell_arg("target/ripr/workflow"), "target/ripr/workflow");
        assert_eq!(shell_arg(""), "''");
        assert_eq!(shell_arg("a\\\\b"), "\\\"a\\\\\\\\b\\\"");
    }
'''
replace_once(old_safe, new_safe, "replace unsafe safe-set test")

old_meta = '''    #[test]
    fn shell_arg_escapes_bash_metacharacters_in_quote_path() {
        // Regression for #2347: inside double quotes bash still expands $VAR,
        // $(cmd), `cmd`, and (interactively) !history. Each must be backslash-
        // escaped so the quoted token is inert when an agent copies the
        // advisory command into a shell.
        assert_eq!(shell_arg("$(whoami)"), "\\\"\\$(whoami)\\\"");
        assert_eq!(shell_arg("a`b`c"), "\\\"a\\`b\\`c\\\"");
        assert_eq!(shell_arg("with!bang"), "\\\"with\\!bang\\\"");
        // Backslash and double-quote are in the safe set, so a value made only
        // of safe characters passes through unquoted (existing behavior). The
        // escapes below only fire when a non-safe character triggers the quote
        // path; pair them with a space to exercise that path.
        assert_eq!(shell_arg("a\\b"), "a\\b");
        assert_eq!(shell_arg("a\"b"), "\\\"a\\\"b\\\"");
        assert_eq!(shell_arg("a\\b c"), "\\\"a\\\\b c\\\"");
        // A gap-id containing `>`/`=` triggers quoting so the `>` cannot be
        // parsed as a redirect; the metacharacters themselves are not special
        // inside double quotes and need no escape, but the wrapping is the fix.
        assert_eq!(
            shell_arg("gap:pr:amount>=threshold"),
            "\\\"gap:pr:amount>=threshold\\\""
        );
        // Compound: space triggers quoting, $ and backtick are escaped inside.
        assert_eq!(
            shell_arg("$(curl evil)/repo root"),
            "\\\"\\$(curl evil)/repo root\\\""
        );
    }
'''
new_meta = '''    #[test]
    fn shell_arg_uses_complete_quote_forms_for_bash_metacharacters() {
        assert_eq!(shell_arg("$(whoami)"), "\\\"\\$(whoami)\\\"");
        assert_eq!(shell_arg("a`b`c"), "\\\"a\\`b\\`c\\\"");
        assert_eq!(shell_arg("with!bang"), "'with!bang'");
        assert_eq!(shell_arg("a'b!c"), "'a'\\\\''b!c'");
        assert_eq!(shell_arg("a\"b"), "\\\"a\\\"b\\\"");
        assert_eq!(shell_arg("a\\b c"), "\\\"a\\\\b c\\\"");
        assert_eq!(
            shell_arg("gap:pr:amount>=threshold"),
            "\\\"gap:pr:amount>=threshold\\\""
        );
        assert_eq!(
            shell_arg("$(curl evil)/repo root"),
            "\\\"\\$(curl evil)/repo root\\\""
        );
    }

    #[cfg(unix)]
    #[test]
    fn shell_arg_round_trips_exact_argv_through_bash() -> Result<(), String> {
        for value in [
            "",
            "plain",
            "repo root",
            "a\\\\b",
            "with!bang",
            "a'b!c",
            "$HOME",
            "`id`",
            "a\\\"b",
            "gap:pr:amount>=threshold",
            "line\\nbreak",
        ] {
            let command = format!("set -- {}; printf '%s' \\\"$1\\\"", shell_arg(value));
            let output = std::process::Command::new("bash")
                .args(["--noprofile", "--norc", "-c", &command])
                .env("BASH_ENV", "")
                .output()
                .map_err(|err| format!("run Bash argv round-trip failed: {err}"))?;
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
'''
replace_once(old_meta, new_meta, "replace string-only metacharacter test")
LOOP.write_text(text, encoding="utf-8")

policy = POLICY.read_text(encoding="utf-8")
entry = 'crates/ripr/src/agent/loop_commands.rs|Command::new|1|agent-command-tests|#2347: Unix-only test executes generated advisory argv tokens through bash --noprofile --norc and proves exact round-trip for empty, whitespace, backslash, quotes, $, backtick, !, redirect-shaped ids, and newlines; test-only process surface.\n'
if entry not in policy:
    anchor = 'crates/ripr/src/agent/artifact.rs|use std::process::Command|1|repair-artifact-provenance|RIPR-SPEC-0134: artifact validation imports Command for the bounded git provenance probes described above.\n'
    if policy.count(anchor) != 1:
        raise SystemExit(f"process policy anchor: expected one match, found {policy.count(anchor)}")
    policy = policy.replace(anchor, anchor + entry, 1)
POLICY.write_text(policy, encoding="utf-8")
