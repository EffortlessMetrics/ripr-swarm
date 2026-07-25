from pathlib import Path

LOOP = Path("crates/ripr/src/agent/loop_commands.rs")
POLICY = Path("policy/process_allowlist.txt")
text = LOOP.read_text(encoding="utf-8")


def replace_region(start_marker: str, end_marker: str, replacement: str, label: str) -> None:
    global text
    start = text.find(start_marker)
    if start < 0:
        raise SystemExit(f"{label}: start marker not found")
    end = text.find(end_marker, start)
    if end < 0:
        raise SystemExit(f"{label}: end marker not found")
    if text.find(start_marker, start + 1) >= 0:
        raise SystemExit(f"{label}: start marker is not unique")
    text = text[:start] + replacement + text[end:]


new_impl = r'''/// Render `value` as one Bash-safe argv token for an advisory command string.
///
/// Non-empty tokens containing only ASCII alphanumerics plus `./_:-` pass
/// through unquoted. Other values normally retain the established double-quote
/// shape, escaping the four characters Bash still interprets there: backslash,
/// double quote, `$`, and backtick. Values containing `!` use a complete
/// single-quote encoder because `\!` inside double quotes does not round-trip
/// consistently across interactive and non-interactive Bash. Embedded single
/// quotes are represented by the standard `'\''` splice. Empty input is `''`.
pub(crate) fn shell_arg(value: &str) -> String {
    if !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '/' | '_' | '-' | ':'))
    {
        return value.to_string();
    }
    if value.contains('!') {
        return format!("'{}'", value.replace('\'', "'\\''"));
    }
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('$', "\\$")
            .replace('`', "\\`")
    )
}

'''
replace_region(
    "/// Render `value` as a single bash-safe argv token",
    "fn append_redirect",
    new_impl,
    "shell_arg implementation",
)

new_safe = r'''    #[test]
    fn shell_arg_passes_only_complete_safe_tokens_unquoted() {
        assert_eq!(shell_arg("a-b_c.d/e:f"), "a-b_c.d/e:f");
        assert_eq!(shell_arg("origin/main"), "origin/main");
        assert_eq!(shell_arg("target/ripr/workflow"), "target/ripr/workflow");
        assert_eq!(shell_arg(""), "''");
        assert_eq!(shell_arg("a\\b"), "\"a\\\\b\"");
    }

'''
replace_region(
    "    #[test]\n    fn shell_arg_passes_safe_tokens_unquoted()",
    "    #[test]\n    fn shell_arg_quotes_spaces_with_double_quotes()",
    new_safe,
    "safe-token test",
)

new_meta = r'''    #[test]
    fn shell_arg_uses_complete_quote_forms_for_bash_metacharacters() {
        assert_eq!(shell_arg("$(whoami)"), "\"\\$(whoami)\"");
        assert_eq!(shell_arg("a`b`c"), "\"a\\`b\\`c\"");
        assert_eq!(shell_arg("with!bang"), "'with!bang'");
        assert_eq!(shell_arg("a'b!c"), "'a'\\''b!c'");
        assert_eq!(shell_arg("a\"b"), "\"a\\\"b\"");
        assert_eq!(shell_arg("a\\b c"), "\"a\\\\b c\"");
        assert_eq!(
            shell_arg("gap:pr:amount>=threshold"),
            "\"gap:pr:amount>=threshold\""
        );
        assert_eq!(
            shell_arg("$(curl evil)/repo root"),
            "\"\\$(curl evil)/repo root\""
        );
    }

    #[cfg(unix)]
    #[test]
    fn shell_arg_round_trips_exact_argv_through_bash() -> Result<(), String> {
        for value in [
            "",
            "plain",
            "repo root",
            "a\\b",
            "with!bang",
            "a'b!c",
            "$HOME",
            "`id`",
            "a\"b",
            "gap:pr:amount>=threshold",
            "line\nbreak",
        ] {
            let command = format!("set -- {}; printf '%s' \"$1\"", shell_arg(value));
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
meta_start = "    #[test]\n    fn shell_arg_escapes_bash_metacharacters_in_quote_path()"
start = text.find(meta_start)
if start < 0:
    raise SystemExit("metacharacter test: start marker not found")
module_end = text.rfind("\n}")
if module_end <= start:
    raise SystemExit("metacharacter test: module end not found")
text = text[:start] + new_meta + text[module_end:]
LOOP.write_text(text, encoding="utf-8")

policy = POLICY.read_text(encoding="utf-8")
entry = 'crates/ripr/src/agent/loop_commands.rs|Command::new|1|agent-command-tests|#2347: Unix-only test executes generated advisory argv tokens through bash --noprofile --norc and proves exact round-trip for empty, whitespace, backslash, quotes, $, backtick, !, redirect-shaped ids, and newlines; test-only process surface.\n'
if entry not in policy:
    anchor = 'crates/ripr/src/agent/artifact.rs|use std::process::Command|1|repair-artifact-provenance|RIPR-SPEC-0134: artifact validation imports Command for the bounded git provenance probes described above.\n'
    if policy.count(anchor) != 1:
        raise SystemExit(f"process policy anchor: expected one match, found {policy.count(anchor)}")
    policy = policy.replace(anchor, anchor + entry, 1)
POLICY.write_text(policy, encoding="utf-8")
