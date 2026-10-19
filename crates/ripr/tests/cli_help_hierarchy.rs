//! Rendered command help contract, portable in the published source package.
//! Repository guide audits live in the unpublished xtask package.

use std::process::Command;

#[path = "common/help_contract.rs"]
mod help_contract;
use help_contract::{assert_contains, normalized, undocumented_doc_flags};

fn run_ripr(args: &[&str]) -> Result<std::process::Output, String> {
    run_ripr_in(args, None)
}

fn run_ripr_in(
    args: &[&str],
    root: Option<&std::path::Path>,
) -> Result<std::process::Output, String> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ripr"));
    command.args(args);
    if let Some(root) = root {
        command.current_dir(root);
    }
    command
        .output()
        .map_err(|error| format!("failed to run ripr {args:?}: {error}"))
}

#[test]
fn init_ci_provider_errors_recover_near_misses() -> Result<(), String> {
    for (value, suggestion) in [
        ("githb", " Did you mean `github`?"),
        ("githbu", " Did you mean `github`?"),
        ("gitlab", " Did you mean `github`?"),
        ("gitlab-ci", ""),
        ("zzzzzzzzzzzz", ""),
    ] {
        let output = run_ripr(&["init", "--ci", value, "--dry-run"])?;
        assert_eq!(output.status.code(), Some(2), "provider {value:?}");
        assert!(output.stdout.is_empty(), "provider {value:?}: {output:?}");
        assert_eq!(
            String::from_utf8_lossy(&output.stderr).replace("\r\n", "\n"),
            format!("ripr: unknown init --ci provider {value:?}.{suggestion} Accepted: github.\n"),
            "provider {value:?}"
        );
    }
    let missing = run_ripr(&["init", "--ci"])?;
    assert_eq!(missing.status.code(), Some(2));
    assert!(missing.stdout.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&missing.stderr).replace("\r\n", "\n"),
        "ripr: missing value for --ci\n"
    );
    Ok(())
}

#[test]
fn init_ci_provider_valid_preview_keeps_workflow_contract() -> Result<(), String> {
    let output = run_ripr(&["init", "--ci", "github", "--dry-run", "--force"])?;
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(".github/workflows/ripr.yml"), "{stdout}");
    assert!(stdout.contains("continue-on-error: true"), "{stdout}");
    assert!(
        stdout.contains("Rerun without --dry-run to apply."),
        "{stdout}"
    );
    Ok(())
}

const BARE_HELP_GROUPS: &[&str] = &[
    "cache",
    "baseline",
    "policy",
    "reports",
    "calibrate",
    "zero",
    "assistant-loop",
    "config",
    "pr-ledger",
    "pr-review",
    "coverage-grip",
    "gate",
];

/// Std-only machine-discovery parser for the portable `rustc --test` path.
/// `xtask/tests/ripr_public_guide_contract.rs` compiles this file without crate
/// dependencies, so the same-row check cannot import `serde_json`.
mod discovery_rows {
    use std::collections::BTreeMap;

    struct JsonRead<'a> {
        bytes: &'a [u8],
        i: usize,
    }

    impl<'a> JsonRead<'a> {
        fn new(src: &'a str) -> Self {
            Self {
                bytes: src.as_bytes(),
                i: 0,
            }
        }

        fn peek(&self) -> Option<u8> {
            self.bytes.get(self.i).copied()
        }

        fn bump(&mut self) -> Result<u8, String> {
            let byte = self
                .peek()
                .ok_or_else(|| "unexpected end of JSON".to_string())?;
            self.i += 1;
            Ok(byte)
        }

        fn skip_ws(&mut self) {
            while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
                self.i += 1;
            }
        }

        fn require_byte(&mut self, wanted: u8) -> Result<(), String> {
            self.skip_ws();
            let got = self.bump()?;
            if got == wanted {
                Ok(())
            } else {
                Err(format!(
                    "required {:?}, got {:?}",
                    wanted as char, got as char
                ))
            }
        }

        fn finish(mut self) -> Result<(), String> {
            self.skip_ws();
            if self.i == self.bytes.len() {
                Ok(())
            } else {
                Err("trailing JSON after the discovery document".to_string())
            }
        }

        fn parse_string(&mut self) -> Result<String, String> {
            if self.bump()? != b'"' {
                return Err("expected a JSON string".to_string());
            }
            let mut out = Vec::new();
            loop {
                let byte = self.bump()?;
                match byte {
                    b'"' => {
                        return String::from_utf8(out)
                            .map_err(|error| format!("non-UTF-8 JSON string: {error}"));
                    }
                    b'\\' => {
                        let escaped = self.bump()?;
                        match escaped {
                            b'"' | b'\\' | b'/' => out.push(escaped),
                            b'b' => out.push(0x08),
                            b'f' => out.push(0x0c),
                            b'n' => out.push(b'\n'),
                            b'r' => out.push(b'\r'),
                            b't' => out.push(b'\t'),
                            b'u' => {
                                let ch = self.parse_hex_escape()?;
                                let mut buf = [0; 4];
                                out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                            }
                            other => {
                                return Err(format!("unknown string escape \\{}", other as char));
                            }
                        }
                    }
                    0x00..=0x1f => {
                        return Err("unescaped control character in JSON string".to_string());
                    }
                    _ => out.push(byte),
                }
            }
        }

        fn parse_hex_escape(&mut self) -> Result<char, String> {
            let mut value = 0u32;
            for _ in 0..4 {
                let digit = self.bump()?;
                let nibble = match digit {
                    b'0'..=b'9' => u32::from(digit - b'0'),
                    b'a'..=b'f' => u32::from(digit - b'a' + 10),
                    b'A'..=b'F' => u32::from(digit - b'A' + 10),
                    _ => return Err("invalid \\u escape".to_string()),
                };
                value = (value << 4) | nibble;
            }
            char::from_u32(value).ok_or_else(|| format!("invalid unicode escape {value:04x}"))
        }

        fn skip_string(&mut self) -> Result<(), String> {
            if self.bump()? != b'"' {
                return Err("expected a JSON string".to_string());
            }
            loop {
                match self.bump()? {
                    b'"' => return Ok(()),
                    b'\\' => {
                        let escaped = self.bump()?;
                        if escaped == b'u' {
                            for _ in 0..4 {
                                let digit = self.bump()?;
                                if !digit.is_ascii_hexdigit() {
                                    return Err("invalid \\u escape".to_string());
                                }
                            }
                        } else if !matches!(
                            escaped,
                            b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't'
                        ) {
                            return Err(format!("unknown string escape \\{}", escaped as char));
                        }
                    }
                    0x00..=0x1f => {
                        return Err("unescaped control character in JSON string".to_string());
                    }
                    _ => {}
                }
            }
        }

        fn skip_literal(&mut self, literal: &[u8]) -> Result<(), String> {
            for expected in literal {
                let got = self.bump()?;
                if got != *expected {
                    return Err(format!(
                        "expected literal {}, got {:?}",
                        String::from_utf8_lossy(literal),
                        got as char
                    ));
                }
            }
            Ok(())
        }

        fn skip_number(&mut self) -> Result<(), String> {
            let start = self.i;
            if self.peek() == Some(b'-') {
                self.i += 1;
            }
            while matches!(
                self.peek(),
                Some(b'0'..=b'9' | b'.' | b'e' | b'E' | b'+' | b'-')
            ) {
                self.i += 1;
            }
            if self.i == start {
                return Err("expected a JSON number".to_string());
            }
            Ok(())
        }

        fn skip_value(&mut self) -> Result<(), String> {
            self.skip_ws();
            match self.peek() {
                Some(b'"') => self.skip_string(),
                Some(b'{') => self.skip_object(),
                Some(b'[') => self.skip_array(),
                Some(b't') => self.skip_literal(b"true"),
                Some(b'f') => self.skip_literal(b"false"),
                Some(b'n') => self.skip_literal(b"null"),
                Some(b'-' | b'0'..=b'9') => self.skip_number(),
                Some(other) => Err(format!(
                    "unexpected JSON value starting with {:?}",
                    other as char
                )),
                None => Err("unexpected end of JSON".to_string()),
            }
        }

        fn skip_object(&mut self) -> Result<(), String> {
            self.parse_object(|_, reader| reader.skip_value())
        }

        fn skip_array(&mut self) -> Result<(), String> {
            self.require_byte(b'[')?;
            self.skip_ws();
            if self.peek() == Some(b']') {
                self.i += 1;
                return Ok(());
            }
            loop {
                self.skip_value()?;
                self.skip_ws();
                match self.peek() {
                    Some(b']') => {
                        self.i += 1;
                        return Ok(());
                    }
                    Some(b',') => {
                        self.i += 1;
                        self.skip_ws();
                        if self.peek() == Some(b']') {
                            return Err("trailing comma in JSON array".to_string());
                        }
                    }
                    _ => return Err("expected comma or ] in JSON array".to_string()),
                }
            }
        }

        fn parse_object(
            &mut self,
            mut on_field: impl FnMut(&str, &mut Self) -> Result<(), String>,
        ) -> Result<(), String> {
            self.require_byte(b'{')?;
            self.skip_ws();
            if self.peek() == Some(b'}') {
                self.i += 1;
                return Ok(());
            }
            loop {
                self.skip_ws();
                let key = self.parse_string()?;
                self.require_byte(b':')?;
                self.skip_ws();
                on_field(&key, self)?;
                self.skip_ws();
                match self.peek() {
                    Some(b'}') => {
                        self.i += 1;
                        return Ok(());
                    }
                    Some(b',') => {
                        self.i += 1;
                        self.skip_ws();
                        if self.peek() == Some(b'}') {
                            return Err("trailing comma in JSON object".to_string());
                        }
                    }
                    _ => return Err("expected comma or } in JSON object".to_string()),
                }
            }
        }

        fn parse_command_row(&mut self) -> Result<(String, String), String> {
            let mut path = None;
            let mut example = None;
            self.parse_object(|key, reader| match key {
                "path" => {
                    if path.is_some() {
                        return Err("command row has a duplicate path field".to_string());
                    }
                    path = Some(reader.parse_string()?);
                    Ok(())
                }
                "example" => {
                    if example.is_some() {
                        return Err("command row has a duplicate example field".to_string());
                    }
                    example = Some(reader.parse_string()?);
                    Ok(())
                }
                _ => reader.skip_value(),
            })?;
            match (path, example) {
                (Some(path), Some(example)) => Ok((path, example)),
                (None, _) => Err("command row missing string path".to_string()),
                (_, None) => Err("command row missing string example".to_string()),
            }
        }

        fn parse_command_rows(&mut self) -> Result<Vec<(String, String)>, String> {
            self.require_byte(b'[')?;
            self.skip_ws();
            if self.peek() == Some(b']') {
                return Err("commands array is empty".to_string());
            }
            let mut rows = Vec::new();
            loop {
                self.skip_ws();
                rows.push(self.parse_command_row()?);
                self.skip_ws();
                match self.peek() {
                    Some(b']') => {
                        self.i += 1;
                        return Ok(rows);
                    }
                    Some(b',') => {
                        self.i += 1;
                        self.skip_ws();
                        if self.peek() == Some(b']') {
                            return Err("trailing comma in JSON array".to_string());
                        }
                    }
                    _ => return Err("expected comma or ] in commands array".to_string()),
                }
            }
        }
    }

    pub(super) fn require_groups(json: &str, groups: &[&str]) -> Result<(), String> {
        let mut reader = JsonRead::new(json);
        let mut commands = None;
        reader.parse_object(|key, reader| {
            if key == "commands" {
                if commands.is_some() {
                    return Err("discovery document has a duplicate commands array".to_string());
                }
                commands = Some(reader.parse_command_rows()?);
                Ok(())
            } else {
                reader.skip_value()
            }
        })?;
        reader.finish()?;
        let rows =
            commands.ok_or_else(|| "discovery document missing commands array".to_string())?;
        let mut by_path: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for (path, example) in &rows {
            by_path
                .entry(path.as_str())
                .or_default()
                .push(example.as_str());
        }
        for &group in groups {
            let expected = format!("ripr {group}");
            match by_path.get(group).map(Vec::as_slice) {
                Some([example]) if *example == expected => {}
                Some([example]) => {
                    return Err(format!(
                        "{group} discovery row example {example:?} does not match {expected:?}"
                    ));
                }
                Some(examples) => {
                    return Err(format!(
                        "duplicate {group} discovery rows ({})",
                        examples.len()
                    ));
                }
                None => {
                    return Err(format!(
                        "missing {group} discovery row in machine discovery output"
                    ));
                }
            }
        }
        Ok(())
    }
}

fn in_help_workspace(
    test: impl FnOnce(&std::path::Path) -> Result<(), String>,
) -> Result<(), String> {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let parent = std::env::temp_dir();
    std::fs::create_dir_all(&parent).map_err(|error| error.to_string())?;
    let root = parent.join(format!("ripr-group-help-{}-{nonce}", std::process::id()));
    std::fs::create_dir(&root).map_err(|error| error.to_string())?;
    // Help must neither load configuration nor write reports, even in a
    // workspace where an executing command would reject the configuration.
    let config = "deliberately invalid TOML [";
    std::fs::write(root.join("ripr.toml"), config).map_err(|error| error.to_string())?;
    let result = test(&root).and_then(|()| {
        let entries = std::fs::read_dir(&root).map_err(|error| error.to_string())?;
        let entries = entries
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        let actual =
            std::fs::read_to_string(root.join("ripr.toml")).map_err(|error| error.to_string())?;
        if entries.len() != 1 || actual != config {
            return Err(format!("group help changed workspace contents: {root:?}"));
        }
        Ok(())
    });
    let cleanup = std::fs::remove_dir_all(&root).map_err(|error| error.to_string());
    result.and(cleanup)
}

#[test]
fn bare_groups_display_their_documented_help() -> Result<(), String> {
    in_help_workspace(|root| {
        let discovery = run_ripr_in(&["help", "--json"], Some(root))?;
        if !discovery.status.success() || !discovery.stderr.is_empty() {
            return Err(format!("machine discovery failed: {discovery:?}"));
        }
        let discovery_text = String::from_utf8_lossy(&discovery.stdout);
        discovery_rows::require_groups(&discovery_text, BARE_HELP_GROUPS)?;
        let mut failures = Vec::new();
        for &group in BARE_HELP_GROUPS {
            let help = run_ripr_in(&["help", group], Some(root))?;
            if !help.status.success()
                || !help.stderr.is_empty()
                || !String::from_utf8_lossy(&help.stdout).contains("Usage:")
            {
                return Err(format!("invalid reference help for {group}: {help:?}"));
            }
            let bare = run_ripr_in(&[group], Some(root))?;
            let flag = run_ripr_in(&[group, "--help"], Some(root))?;
            for (spelling, output) in [("bare", bare), ("--help", flag)] {
                if output.status.code() != Some(0)
                    || !output.stderr.is_empty()
                    || output.stdout != help.stdout
                {
                    failures.push(format!(
                        "{group} ({spelling}) differs from group help: {output:?}"
                    ));
                }
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("\n"))
        }
    })
}

/// Independent `"path"` / `"example"` substring searches cannot notice a
/// swapped pair of discovery rows. This control keeps the same-object parser
/// honest without invoking the CLI.
#[test]
fn discovery_rows_require_path_and_example_in_the_same_object() -> Result<(), String> {
    let coupled = r#"{"commands":[{"path":"cache","nested":{"path":"policy","example":"ripr policy"},"example":"ripr cache"},{"path":"policy","example":"ripr policy"}]}"#;
    discovery_rows::require_groups(coupled, &["cache", "policy"])?;
    discovery_rows::require_groups(
        "{\n  \"commands\": [\n    { \"path\": \"cache\", \"example\": \"ripr cache\" }\n  ]\n}\n",
        &["cache"],
    )?;

    let swapped = r#"{"commands":[{"path":"cache","example":"ripr policy"},{"path":"policy","example":"ripr cache"}]}"#;
    if !(swapped.contains("\"path\":\"cache\"")
        && swapped.contains("\"example\":\"ripr cache\"")
        && swapped.contains("\"path\":\"policy\"")
        && swapped.contains("\"example\":\"ripr policy\""))
    {
        return Err("swapped-row fixture lost the independent substrings".to_string());
    }
    match discovery_rows::require_groups(swapped, &["cache", "policy"]) {
        Err(error)
            if error.contains("cache discovery row example \"ripr policy\"")
                || error.contains("policy discovery row example \"ripr cache\"") => {}
        other => {
            return Err(format!(
                "swapped discovery rows must fail the same-object check, got {other:?}"
            ));
        }
    }

    let nested_credit = r#"{"commands":[{"path":"cache","effects":{"path":"cache","example":"ripr cache"},"example":"ripr policy"}]}"#;
    match discovery_rows::require_groups(nested_credit, &["cache"]) {
        Err(error) if error.contains("cache discovery row example \"ripr policy\"") => {}
        other => {
            return Err(format!(
                "nested matching fields must not credit the parent row, got {other:?}"
            ));
        }
    }

    for (label, json, wanted) in [
        (
            "duplicate group row",
            r#"{"commands":[{"path":"cache","example":"ripr cache"},{"path":"cache","example":"ripr cache"}]}"#,
            "duplicate cache discovery rows",
        ),
        (
            "missing example",
            r#"{"commands":[{"path":"cache"}]}"#,
            "missing string example",
        ),
        (
            "missing path",
            r#"{"commands":[{"example":"ripr cache"}]}"#,
            "missing string path",
        ),
        (
            "empty commands",
            r#"{"commands":[]}"#,
            "commands array is empty",
        ),
        (
            "missing commands",
            r#"{"workflows":[]}"#,
            "missing commands array",
        ),
        (
            "truncated",
            r#"{"commands":[{"path":"cache","example":"ripr cache"}]"#,
            "expected comma or } in JSON object",
        ),
        (
            "non-string path",
            r#"{"commands":[{"path":1,"example":"ripr cache"}]}"#,
            "expected a JSON string",
        ),
        (
            "invalid skipped escape",
            r#"{"commands":[{"path":"cache","summary":"bad \q","example":"ripr cache"}]}"#,
            "unknown string escape",
        ),
    ] {
        match discovery_rows::require_groups(json, &["cache"]) {
            Err(error) if error.contains(wanted) => {}
            other => {
                return Err(format!(
                    "{label} must be rejected with {wanted:?}, got {other:?}"
                ));
            }
        }
    }
    Ok(())
}

#[test]
fn group_help_preserves_explicit_errors_and_gate_flag_alias() -> Result<(), String> {
    in_help_workspace(|root| {
        for &group in BARE_HELP_GROUPS {
            let output = run_ripr_in(&[group, "bogus"], Some(root))?;
            assert_eq!(output.status.code(), Some(2), "{group}: {output:?}");
            assert!(output.stdout.is_empty(), "{group}: {output:?}");
            assert_contains(
                group,
                &String::from_utf8_lossy(&output.stderr),
                &format!("unknown {group} subcommand"),
            )?;
        }
        for (args, error) in [
            (["zero", "status"], "zero status requires --delta"),
            (
                ["pr-ledger", "record"],
                "pr-ledger record requires at least one",
            ),
            (
                ["pr-review", "front-panel"],
                "pr-review front-panel requires at least one",
            ),
            (
                ["coverage-grip", "frontier"],
                "coverage-grip frontier requires at least one",
            ),
            (["gate", "evaluate"], "gate evaluate requires at least one"),
            (
                ["calibrate", "cargo-mutants"],
                "calibrate cargo-mutants requires --mutants-json",
            ),
            (
                ["assistant-loop", "proof"],
                "assistant-loop proof requires at least one",
            ),
        ] {
            let output = run_ripr_in(&args, Some(root))?;
            assert_eq!(output.status.code(), Some(2), "{args:?}: {output:?}");
            assert!(output.stdout.is_empty(), "{args:?}: {output:?}");
            assert_contains(args[0], &String::from_utf8_lossy(&output.stderr), error)?;
        }
        let alias = run_ripr_in(&["gate", "--pr-guidance"], Some(root))?;
        let explicit = run_ripr_in(&["gate", "evaluate", "--pr-guidance"], Some(root))?;
        assert_eq!(alias.status.code(), Some(2));
        assert!(alias.stdout.is_empty());
        assert_eq!(alias.stderr, explicit.stderr);
        assert_contains(
            "gate flag alias",
            &String::from_utf8_lossy(&alias.stderr),
            "missing value for --pr-guidance",
        )?;
        Ok(())
    })
}

fn rendered_help(args: &[&str]) -> Result<String, String> {
    let output = run_ripr(args)?;
    if !output.status.success() {
        return Err(format!(
            "ripr {args:?} failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    String::from_utf8(output.stdout)
        .map_err(|error| format!("ripr {args:?} emitted non-UTF-8 stdout: {error}"))
}

#[test]
fn workflow_help_lists_the_five_governed_identities() -> Result<(), String> {
    let stdout = rendered_help(&["help", "workflow"])?;
    for id in [
        "inspect-change",
        "guided-adoption",
        "repair-gap",
        "compose-pr-evidence",
        "adopt-ci",
    ] {
        assert_contains("`ripr help workflow`", &stdout, id)?;
    }
    assert_contains(
        "`ripr help workflow`",
        &stdout,
        "Run `ripr help workflow <name>` for one workflow's steps",
    )?;
    // The listing is guidance only: it must not silently execute anything.
    assert_contains(
        "`ripr help workflow`",
        &stdout,
        "Nothing on this screen runs a command",
    )
}

#[test]
fn workflow_help_renders_one_workflow_with_bounded_sections() -> Result<(), String> {
    let stdout = rendered_help(&["help", "workflow", "repair-gap"])?;
    for section in [
        "Workflow: repair-gap",
        "Purpose:",
        "Applies when:",
        "Commands (ordered):",
        "Result families:",
        "Artifacts:",
        "Recovery:",
        "Stop conditions:",
        "Limitations:",
    ] {
        assert_contains("`ripr help workflow repair-gap`", &stdout, section)?;
    }
    // Aliases resolve on the render path.
    assert_contains(
        "`ripr help workflow repair-gap`",
        &rendered_help(&["help", "workflow", "adoption"])?,
        "Workflow: guided-adoption",
    )?;
    Ok(())
}

#[test]
fn unknown_workflow_fails_with_a_family_distinct_from_unknown_command() -> Result<(), String> {
    let output = run_ripr(&["help", "workflow", "repar-gap"])?;
    if output.status.success() {
        return Err(format!(
            "unknown workflow exited 0\nstdout:\n{}",
            String::from_utf8_lossy(&output.stdout)
        ));
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stderr.contains("unknown workflow") {
        return Err(format!(
            "unknown workflow error missing its family marker:\n{stderr}"
        ));
    }
    if stderr.contains("unknown command") {
        return Err(format!(
            "unknown workflow error leaked the unknown-command family:\n{stderr}"
        ));
    }
    // Suggestions, when present, must come from workflow identities only and
    // must not suggest a command spelling for a workflow typo.
    if stderr.contains("repair-loop") || stderr.contains("ripr agent") {
        return Err(format!(
            "unknown workflow error suggested command machinery instead of a workflow:\n{stderr}"
        ));
    }
    Ok(())
}

#[test]
fn default_help_keeps_the_task_roles_distinct() -> Result<(), String> {
    for args in [["--help"].as_slice(), ["help"].as_slice()] {
        let stdout = normalized(&rendered_help(args)?);
        for needle in [
            "Diagnose setup ripr doctor",
            "Inspect one change ripr check",
            "Guided repo adoption ripr pilot --root .",
            "Repair one named gap ripr repair",
            "Compose PR evidence ripr first-pr",
            "Adopt advisory CI ripr init --ci github",
            "ripr help --all",
            "ripr help --json",
        ] {
            assert_contains(
                "default help (`ripr --help` / `ripr help`)",
                &stdout,
                needle,
            )?;
        }
    }
    Ok(())
}

#[test]
fn exhaustive_help_keeps_the_same_roles_and_boundaries() -> Result<(), String> {
    let stdout = normalized(&rendered_help(&["help", "--all"])?);
    for needle in [
        "Diagnose setup ripr doctor",
        "Inspect one change ripr check",
        "Guided repo adoption ripr pilot --root .",
        "Repair one named gap ripr repair [<item>] | ripr continue | ripr status",
        "Compose PR evidence ripr first-pr --root . --base BASE --head HEAD",
        "Adopt advisory CI ripr init --ci github",
        "`ripr check` is the ordinary first-value analysis; `ripr pilot` is the guided repo-adoption workflow.",
        "`ripr first-pr` and `ripr start-here` compose `target/ripr/reports/start-here.{json,md}` from existing artifacts; they do not run analysis or repair a gap.",
        "ripr help --json",
        "this route accepts no other arguments",
        "except on ripr help --json",
    ] {
        assert_contains("exhaustive help (`ripr help --all`)", &stdout, needle)?;
    }
    Ok(())
}

/// Non-public rows are visibly distinct in the exhaustive reference
/// (issue #4823): advanced rows carry an `[advanced]` line marker and the
/// compatibility alias carries `[compatibility]`, so a reader can tell the
/// primary surface from control and legacy commands without opening each
/// help page. The projection is validated against the typed tables in
/// `command_metadata::tests::human_surfaces_agree_with_the_typed_tables`.
#[test]
fn exhaustive_help_marks_non_public_rows_visibly() -> Result<(), String> {
    let rendered = rendered_help(&["help", "--all"])?;
    let stdout = normalized(&rendered);
    for needle in [
        "ripr agent repair --root . --seam-id ID --phase before [advanced]",
        "ripr agent repair --root . (--attempt ID | --seam-id ID) --phase after [advanced]",
        "ripr agent repair --root . --attempt ID --phase verify --verify-authorized --verify-authority ID [advanced]",
        "ripr agent status --root . [--json] [advanced]",
        "ripr agent start --root . --seam-id ID [--out target/ripr/workflow] [advanced]",
        "ripr agent brief --root . (--diff PATH|--base REV|--files PATHS|--seam-id ID) --json [advanced]",
        "ripr agent packet --root . (--seam-id ID | --gap-ledger PATH --gap-id ID) --json [advanced]",
        "ripr agent card --root . --seam-id ID [--json] [advanced]",
        "ripr agent verify --root . --before before.json --after after.json --json [advanced]",
        "ripr agent verify-execute --root . --packet packet.json --result-json result.json --authorize --json [advanced]",
        "ripr agent receipt --root . --verify-json agent-verify.json --seam-id ID --json [advanced]",
        "ripr agent review-summary --root . [--json] [advanced]",
        "ripr start-here [same options as first-pr] [compatibility]",
    ] {
        assert_contains("exhaustive help (`ripr help --all`)", &stdout, needle)?;
    }
    // The primary public rows stay unmarked. Inspect each original rendered
    // row: a class marker anywhere on the row (not just adjacent to the
    // route) is a leak, and the canonical route must still be present.
    let rendered_rows: Vec<String> = rendered.lines().map(normalized).collect();
    for needle in [
        "ripr check [--base REV] [--worktree] [--diff PATH] [--mode draft] [--format FORMAT]",
        "ripr repair [<item>] [--root PATH]",
        "ripr continue [--attempt ID] [--root PATH]",
        "ripr status [--attempt ID] [--root PATH] [--json]",
    ] {
        let mut found = false;
        for row in &rendered_rows {
            if row.contains(needle) {
                found = true;
                for marker in ["[advanced]", "[compatibility]"] {
                    if row.contains(marker) {
                        return Err(format!(
                            "public row unexpectedly carries a class marker: {needle} {marker}"
                        ));
                    }
                }
            }
        }
        if !found {
            return Err(format!(
                "exhaustive help (`ripr help --all`) lost the canonical route `{needle}`"
            ));
        }
    }
    Ok(())
}

/// The help screens are the exhaustive reference, so a line on one is a claim
/// about what the command accepts. Each needle below was wrong against source
/// until this test existed: the repair rows printed `--phase verify` forms that
/// always fail (the verify phase refuses without `--verify-authorized` and
/// `--verify-authority`, and takes only `--attempt`), two report rows named
/// default outputs the producers never write, and four rows omitted selectors
/// the parsers accept. The negative asserts are the discriminators — they fail
/// on the exact prior wording.
#[test]
fn help_screens_state_the_surfaces_the_parsers_accept() -> Result<(), String> {
    // The default screen is a bounded first screen with a line budget
    // (`help_leads_with_a_bounded_first_screen_that_routes_onward`), so the
    // verify phase does not belong on it at all: it is reachable only for
    // trust-bound Python attempts and needs an authorization pair that would
    // not fit. Its repair teaser is before -> edit -> after, the ordinary Rust
    // path, and the full invocation lives in `help --all` and
    // `agent repair --help`.
    let default_help = normalized(&rendered_help(&["--help"])?);
    for needle in ["ripr repair [<item>]", "ripr continue"] {
        assert_contains("default help (`ripr --help`)", &default_help, needle)?;
    }
    if default_help.contains("ripr agent repair --attempt ID --phase verify") {
        return Err(
            "the bounded first screen must not print a verify invocation it cannot make runnable"
                .to_string(),
        );
    }

    let all = normalized(&rendered_help(&["help", "--all"])?);
    for needle in [
        // The verify phase takes `--attempt`, never `--seam-id`, and refuses
        // without both authorization signals.
        "ripr agent repair --root . --attempt ID --phase verify --verify-authorized --verify-authority ID",
        "ripr agent repair --root . (--attempt ID | --seam-id ID) --phase after",
        // Producer defaults, from typescript_limitations.rs and
        // typescript_false_actionable.rs.
        "target/ripr/reports/typescript-limitations.json",
        "target/ripr/reports/typescript-false-actionable-audit.json",
        // `gate evaluate --help` lists four modes; this screen listed two.
        "--mode visible-only|acknowledgeable|baseline-check|calibrated-gate",
        // Selectors the parsers accept that this screen did not name.
        "ripr reports gap-ledger (--records PATH | --repo-exposure PATH | --check-output PATH)",
        "ripr agent packet --root . (--seam-id ID | --gap-ledger PATH --gap-id ID) --json",
        "--gap CANONICAL_GAP_ID --gap-ledger PATH",
        // `ripr mcp --help` states `--stdio` is optional and the default.
        "ripr mcp [--stdio] [--root PATH]",
    ] {
        assert_contains("exhaustive help (`ripr help --all`)", &all, needle)?;
    }

    // Discriminators. The `Repair one named gap` role line is the task-first
    // alternation `repair | continue | status`: it names the ordinary
    // commands rather than the internal phase vocabulary, and
    // `default_help_keeps_the_task_roles_distinct` pins it as role
    // vocabulary.
    for (surface, text, stale) in [
        (
            "exhaustive help",
            &all,
            "ripr agent repair --root . --seam-id ID --phase before|after|verify",
        ),
        (
            "exhaustive help",
            &all,
            "target/ripr/reports/ts-limitations.json",
        ),
        (
            "exhaustive help",
            &all,
            "target/ripr/reports/ts-false-actionable.json",
        ),
        (
            "exhaustive help",
            &all,
            "--mode visible-only|acknowledgeable]",
        ),
        ("exhaustive help", &all, "ripr mcp --stdio [--root PATH]"),
    ] {
        if text.contains(stale) {
            return Err(format!("{surface} still prints the stale form `{stale}`"));
        }
    }
    Ok(())
}

/// #7450: `--help` must name how to produce the cargo-mutants artifacts the
/// importer consumes. The option one-liner naming `outcomes.json` is not
/// enough; a reader still cannot install, run, or point at `mutants.out`.
#[test]
fn calibrate_help_names_how_to_produce_the_imported_artifacts() -> Result<(), String> {
    let help = normalized(&rendered_help(&["calibrate", "cargo-mutants", "--help"])?);
    for needle in [
        "Producing the input:",
        "cargo install cargo-mutants --locked",
        "cargo mutants",
        "mutants.out",
        "27.1.0",
        "RIPR-SPEC-0006",
        "fixtures/boundary_gap/calibration/runtime-mutants.json",
    ] {
        assert_contains(
            "calibrate help (`ripr calibrate cargo-mutants --help`)",
            &help,
            needle,
        )?;
    }
    if help.contains("It does not run mutation testing") {
        return Ok(());
    }
    Err("calibrate help producer recipe must keep the existing non-runner bound".to_string())
}

/// #7451: `--help` must say that a mutants.json-only import counts
/// unexecuted candidates in matched_total. Naming mutants.json as a valid
/// path is not the denominator caveat.
#[test]
fn calibrate_help_discloses_mutants_json_only_denominator() -> Result<(), String> {
    let help = normalized(&rendered_help(&["calibrate", "cargo-mutants", "--help"])?);
    for needle in [
        "mutants.json-only import counts unexecuted candidates in matched_total",
        "no runtime observation",
        "no_runtime_data",
        "Pass outcomes.json",
    ] {
        assert_contains(
            "calibrate help (`ripr calibrate cargo-mutants --help`)",
            &help,
            needle,
        )?;
    }
    Ok(())
}

/// #7202 drift guard. `ripr outcome --help` must describe the canonical-gap
/// refusal as it is enforced: Rust `ripr check --json` findings carry
/// `canonical_gap_id` and are compared by it, and the refusal keys on
/// findings that lack one. The help once claimed Rust check JSON "is refused
/// rather than compared" outright, sending agents down a needless
/// `repo-exposure-json` detour for a path that already worked.
#[test]
fn outcome_help_matches_the_enforced_canonical_gap_contract() -> Result<(), String> {
    let help = normalized(&rendered_help(&["outcome", "--help"])?);
    assert_contains(
        "outcome help (`ripr outcome --help`)",
        &help,
        "Rust `ripr check --json` findings carry canonical_gap_id and are compared by it",
    )?;
    assert_contains(
        "outcome help (`ripr outcome --help`)",
        &help,
        "Check output whose findings carry no canonical_gap_id is refused rather than compared",
    )?;
    if help.contains("no canonical_gap_id (Rust") {
        return Err(
            "outcome help claims Rust check JSON findings lack canonical_gap_id; they carry \
             it and are compared by it (#7202)"
                .to_string(),
        );
    }
    Ok(())
}

#[test]
fn agent_help_makes_repair_primary_without_removing_control_surfaces() -> Result<(), String> {
    for args in [["agent", "--help"].as_slice(), ["agent"].as_slice()] {
        let stdout = rendered_help(args)?;
        let collapsed = normalized(&stdout);
        assert_contains("agent help", &collapsed, "Primary workflow:")?;

        assert_contains(
            "agent help",
            &collapsed,
            "repair Run the before/edit/after repair transaction and its verification phase for one seam.",
        )?;
        assert_contains(
            "agent help",
            &collapsed,
            "status Report existing agent-loop artifacts and the exact next command.",
        )?;
        let advanced = stdout
            .split("Advanced and compatibility workflows:")
            .nth(1)
            .ok_or_else(|| {
                "agent help lost the explicit advanced/compatibility boundary".to_string()
            })?;
        // Match each advanced command as the first token of a listed entry, so
        // a dropped entry cannot hide behind a substring of another command's
        // name or description (`verify` inside `verify-execute`, and so on).
        for command in [
            "start",
            "brief",
            "packet",
            "verify",
            "verify-execute",
            "receipt",
            "review-summary",
        ] {
            let listed = advanced
                .lines()
                .any(|line| line.split_whitespace().next() == Some(command));
            if !listed {
                return Err(format!(
                    "agent help lost the advanced command entry `{command}`"
                ));
            }
        }
    }
    Ok(())
}

/// The primary repair transaction has its own help surface; it must keep the
/// before/edit/after sequence and the explicit limits (no test generation, no
/// mutation execution, no merge authority) that keep the route advisory.
#[test]
fn agent_repair_help_names_the_primary_transaction_and_its_limits() -> Result<(), String> {
    let stdout = normalized(&rendered_help(&["agent", "repair", "--help"])?);
    for needle in [
        "Run the before/edit/after repair transaction and its verification phase for one named gap.",
        "ripr agent repair --seam-id ID --phase before",
        "# edit one focused test outside RIPR",
        "ripr agent repair --attempt ID --phase after",
        "--verify-authorized",
        "--verify-authority ID",
        "--verify-rollback",
        "ripr agent repair [--root PATH] [--store PATH] --seam-id ID --phase before",
        "ripr agent repair [--root PATH] [--store PATH] (--attempt ID|--seam-id ID) --phase after",
        "ripr agent repair [--root PATH] [--store PATH] --attempt ID --phase verify",
        "--store PATH Explicit repair-attempt store, resolved against --root.",
        "The repair command does not generate or apply tests, execute mutation testing, or declare the repository safe to merge.",
        "Inline `#[cfg(test)]` modules don't qualify their file: a repository whose only tests are inline in non-test-surface files is permanently out of repair scope",
    ] {
        assert_contains(
            "agent repair help (`ripr agent repair --help`)",
            &stdout,
            needle,
        )?;
    }
    if stdout.contains("ripr agent repair [--root PATH] --attempt ID --phase verify") {
        return Err(
            "agent repair help still prints verify usage without optional [--store PATH]"
                .to_string(),
        );
    }
    Ok(())
}

#[test]
fn agent_status_help_names_the_selected_store_and_exact_attempt_selection() -> Result<(), String> {
    let stdout = normalized(&rendered_help(&["agent", "status", "--help"])?);
    for needle in [
        "Usage: ripr agent status [--root PATH] [--store PATH] [--attempt ID] [--json] [--out PATH]",
        "--store PATH Explicit repair-attempt store, resolved against --root.",
        "Missing explicit stores do not fall back to the default.",
        "--attempt ID Select exactly one repair attempt by ID and report its",
        "corrupt_or_unavailable result, never as another attempt's state.",
    ] {
        assert_contains(
            "agent status help (`ripr agent status --help`)",
            &stdout,
            needle,
        )?;
    }
    Ok(())
}

/// The `help` index must route a subcommand to that subcommand's own help
/// (#4378): `ripr help agent repair` printed the parent `agent` overview
/// because the injected `--help` preceded `repair`. An unknown subcommand
/// errors instead of printing unrelated help.
#[test]
fn help_index_routes_agent_repair_to_repair_help() -> Result<(), String> {
    let via_index = rendered_help(&["help", "agent", "repair"])?;
    let direct = rendered_help(&["agent", "repair", "--help"])?;
    if via_index != direct {
        return Err(format!(
            "`ripr help agent repair` must print the same help as `ripr agent repair --help`\n\
             help index:\n{via_index}\ndirect:\n{direct}"
        ));
    }
    let unknown = run_ripr(&["help", "agent", "no-such-subcommand"])?;
    if unknown.status.success() {
        return Err(format!(
            "`ripr help agent no-such-subcommand` must fail, got stdout:\n{}",
            String::from_utf8_lossy(&unknown.stdout)
        ));
    }
    assert_contains(
        "unknown agent subcommand via help index",
        &String::from_utf8_lossy(&unknown.stderr),
        "unknown agent subcommand",
    )
}

/// `check` and `context` must say what each `--mode` value means and where
/// the full table lives (#4378), not only list the bare names.
#[test]
fn mode_help_names_index_scope_and_points_at_the_mode_table() -> Result<(), String> {
    for command in ["check", "context"] {
        let help = normalized(&rendered_help(&[command, "--help", "--all"])?);
        for needle in [
            "instant (changed files only, cheapest)",
            "deep and ready (whole workspace, slowest)",
            "See docs/CONFIGURATION.md \"Analysis modes\"",
        ] {
            assert_contains(&format!("{command} --help --mode line"), &help, needle)?;
        }
    }
    Ok(())
}

/// `check --help` must teach the loader's real default-base resolution order
/// (#3885), not the old `origin/main` shorthand. `diff` resolves an omitted
/// base through the same authority (#3952), so its help states the same order.
#[test]
fn check_and_diff_help_state_the_real_base_default() -> Result<(), String> {
    let check = normalized(&rendered_help(&["check", "--help", "--all"])?);
    for needle in [
        "the local origin/HEAD ref, then origin/main, origin/master, main, and master in order",
        "when none of those resolves, the analysis does not run",
    ] {
        assert_contains("check help (`ripr check --help`)", &check, needle)?;
    }
    if check.contains("Defaults to origin/main") {
        return Err(
            "check help still teaches the origin/main default instead of the resolution order"
                .to_string(),
        );
    }
    let diff = normalized(&rendered_help(&["diff", "--help"])?);
    assert_contains(
        "diff help (`ripr diff --help`)",
        &diff,
        "the local origin/HEAD ref, then origin/main, origin/master, main, and master",
    )?;
    if diff.contains("Defaults to origin/main") {
        return Err("diff help still teaches the origin/main default".to_string());
    }
    Ok(())
}

/// #5211 direction B: `check --help` keeps every format but chooses one
/// per task, so a newcomer maps their job to a format without re-reading
/// the group list. Each task below must keep exactly its recommended path.
#[test]
fn check_help_chooses_one_format_per_task() -> Result<(), String> {
    let check = normalized(&rendered_help(&["check", "--help", "--all"])?);
    for needle in [
        "Choose by task:",
        "eye review -> human",
        "every finding with drill-in commands -> human-full",
        "machine consumer (jq, CI scripts) -> json",
        "file annotations in Actions logs -> github",
        "code scanning upload -> sarif",
        "README badge -> repo-badge-shields (repo ledger)",
        "PR/CI status badge -> badge-shields (diff)",
        "whole-repo inventory -> repo-exposure-json",
        "agent repair evidence -> agent-seam-packets-json",
    ] {
        assert_contains("check help (`ripr check --help`)", &check, needle)?;
    }
    // The chooser orients before the group list it summarizes.
    let chooser = check
        .find("Choose by task:")
        .ok_or("chooser missing from check help")?;
    let groups = check
        .find("Analysis (diff-scoped):")
        .ok_or("format groups missing from check help")?;
    if chooser > groups {
        return Err("the task chooser must precede the format groups".to_string());
    }
    // BADGE_ADOPTION.md rule 1: README badges are repo-scoped. The README
    // mapping must name only the repo format; the diff format belongs to
    // the PR/CI mapping.
    let readme = check
        .find("README badge ->")
        .ok_or("README mapping missing from the chooser")?;
    let pr = check
        .find("PR/CI status badge ->")
        .ok_or("PR/CI mapping missing from the chooser")?;
    if readme > pr {
        return Err("the README mapping must precede the PR/CI mapping".to_string());
    }
    let readme_span = &check[readme..pr];
    if !readme_span.contains("repo-badge-shields") {
        return Err("the README mapping must recommend repo-badge-shields".to_string());
    }
    if readme_span.contains("badge-shields (diff)") {
        return Err("the README mapping must not include the diff format".to_string());
    }
    Ok(())
}

#[test]
fn doc_flag_guard_rejects_removed_flags_and_accepts_prose() -> Result<(), String> {
    let mut cache = std::collections::BTreeMap::new();
    let bad = "Run `ripr check --no-such-flag` first.\n";
    let drift = undocumented_doc_flags(&[("bad.md", bad)], &mut cache, &run_ripr)?;
    if drift.len() != 1 || !drift[0].contains("bad.md:1 `ripr check --no-such-flag`") {
        return Err(format!("guard missed a removed check flag: {drift:?}"));
    }
    let sub =
        "```bash\nripr agent repair --root . \\\n  --seam-id ID --phase before --bogus\n```\n";
    let drift = undocumented_doc_flags(&[("sub.md", sub)], &mut cache, &run_ripr)?;
    if drift.len() != 1 || !drift[0].contains("`ripr agent repair --bogus`") {
        return Err(format!(
            "guard missed a continued-line subcommand flag: {drift:?}"
        ));
    }
    let quoted = "Run `ripr check --diff \"a b.patch\" --bogus`.\n";
    let drift = undocumented_doc_flags(&[("quoted.md", quoted)], &mut cache, &run_ripr)?;
    if drift.len() != 1 || !drift[0].contains("`ripr check --bogus`") {
        return Err(format!(
            "guard missed a flag after a quoted value: {drift:?}"
        ));
    }
    let good = "Use ripr check with --json, or `ripr pilot --root .`. The ripr is static; \
                cargo-ripr --x and ripr-swarm --y are not invocations.\n";
    let drift = undocumented_doc_flags(&[("good.md", good)], &mut cache, &run_ripr)?;
    if !drift.is_empty() {
        return Err(format!("guard rejected valid prose: {drift:?}"));
    }
    Ok(())
}

/// The exhaustive `check` reference must keep every option, output format and
/// environment variable by name, so a removed entry fails even when the help
/// stays long.
const CHECK_REFERENCE_OPTIONS: &[&str] = &[
    "--root",
    "--base",
    "--diff",
    "--candidate-tree",
    "--candidate-base",
    "--worktree",
    "--committed",
    "--mode",
    "--format",
    "--gap-ledger",
    "--json",
    "--no-unchanged-tests",
    "--perl-facts",
    "--suppression-policy",
    "--write-artifact",
    "--git-timeout",
    "--quiet",
    "--all",
];
const CHECK_REFERENCE_FORMATS: &[&str] = &[
    "human",
    "human-full",
    "json",
    "github",
    "sarif",
    "badge-json",
    "badge-shields",
    "badge-plus-json",
    "badge-plus-shields",
    "repo-seams-json",
    "repo-seams-md",
    "repo-exposure-json",
    "repo-exposure-summary-json",
    "repo-exposure-md",
    "repo-sarif",
    "repo-badge-json",
    "repo-badge-shields",
    "repo-badge-plus-json",
    "repo-badge-plus-shields",
    "agent-seam-packets-json",
];
const CHECK_REFERENCE_ENVIRONMENT: &[&str] = &[
    "RIPR_MAX_DIFF_CHANGED_RUST_LINES",
    "RIPR_MAX_DIFF_INDEX_FILES",
    "RIPR_DIFF_NARROW_INDEX_FILES",
    "RIPR_DIFF_DEPENDENT_SCOPE",
    "RIPR_PARTIAL_DIFF_FILE_BUDGET",
    "RIPR_PARTIAL_DIFF_LINE_BUDGET",
    "RIPR_GIT_TIMEOUT",
    "RIPR_REPO_EXPOSURE_LATENCY_TRACE",
    "RIPR_ALLOW_REPO_PERL_EXECUTABLE",
];

/// #8158 item 2: `check --help` is one screen that still answers the first
/// questions (which base, which scope, which format, what the exit code
/// means) and points at the exhaustive reference, which stays reachable and
/// complete.
#[test]
fn check_help_is_one_screen_and_all_is_the_full_reference() -> Result<(), String> {
    let short = rendered_help(&["check", "--help"])?;
    let lines = short.lines().count();
    if lines > 35 {
        return Err(format!(
            "`ripr check --help` must fit one screen (<= 35 lines), got {lines}"
        ));
    }
    let short = normalized(&short);
    for needle in [
        "--base REV",
        "origin/HEAD ref, then origin/main",
        "--diff PATH",
        "--worktree",
        "already the",
        "--committed",
        "committed history only",
        "--format FORMAT",
        "--json",
        "--mode MODE",
        "instant, draft (default), fast, deep, or ready",
        "--root PATH",
        "--quiet",
        "Unknown totals show no percentage or ETA",
        "progress makes no speed claim",
        // The completed-run and gate-blocked exit contract, sentence by sentence.
        "Exit status: 0 for a completed run, including `exposed` and `weakly_exposed` findings.",
        "`unsupported_input` reports exit 2, as do usage errors.",
        "run `ripr gate evaluate` (exit 3 on `blocked`)",
        "ripr check --help --all",
    ] {
        assert_contains("`ripr check --help`", &short, needle)?;
    }
    if short.contains("3 for a completed run") {
        return Err("short check help must not give a completed run exit 3".to_string());
    }
    // Reference-only detail stays behind `--all`.
    let deep: Vec<&str> = CHECK_REFERENCE_ENVIRONMENT
        .iter()
        .copied()
        .chain([
            "--candidate-tree",
            "--suppression-policy",
            "--write-artifact",
        ])
        .chain(["--git-timeout", "--gap-ledger", "badge-shields"])
        .collect();
    for item in deep {
        if short.contains(item) {
            return Err(format!("short check help leaked reference detail {item:?}"));
        }
    }
    let full_text = rendered_help(&["check", "--help", "--all"])?;
    if full_text.lines().count() <= 150 {
        return Err("`--help --all` must be the exhaustive reference (> 150 lines)".to_string());
    }
    let full = normalized(&full_text);
    // Options and environment variables must be DEFINED (an entry at the start
    // of a line), not merely mentioned in another entry's prose.
    for name in CHECK_REFERENCE_OPTIONS
        .iter()
        .chain(CHECK_REFERENCE_ENVIRONMENT)
    {
        let defined = full_text.lines().any(|line| {
            line.trim_start()
                .strip_prefix(name)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
        });
        if !defined {
            return Err(format!(
                "`ripr check --help --all` has no entry defining `{name}`"
            ));
        }
    }
    for name in CHECK_REFERENCE_FORMATS {
        assert_contains("`ripr check --help --all`", &full, name)?;
    }
    // Every spelling of "help for check" agrees, and `--all` order is free.
    let help_check_all = rendered_help(&["help", "check", "--all"])?;
    let all_first = rendered_help(&["check", "--all", "--help"])?;
    if help_check_all != full_text || all_first != full_text {
        return Err(
            "`help check --all` and `check --all --help` must equal `check --help --all`"
                .to_string(),
        );
    }
    let summary = rendered_help(&["check", "--help"])?;
    if rendered_help(&["help", "check"])? != summary || rendered_help(&["check", "-h"])? != summary
    {
        return Err("`help check` and `check -h` must equal `check --help`".to_string());
    }
    // `--all` is not a general check flag: without `--help` it is an unknown
    // argument (exit 2), in the same family as any other typo.
    let bare = run_ripr(&["check", "--all"])?;
    if bare.status.code() != Some(2) {
        return Err(format!(
            "`ripr check --all` must exit 2, got {:?}",
            bare.status.code()
        ));
    }
    assert_contains(
        "`ripr check --all` stderr",
        &String::from_utf8_lossy(&bare.stderr),
        "unknown check argument \"--all\"",
    )?;
    // A mistyped advanced flag is suggested from the exhaustive reference and
    // points at it, because the summary omits that flag.
    let typo = run_ripr(&["check", "--candidate-tre"])?;
    if typo.status.code() != Some(2) {
        return Err(format!(
            "a mistyped advanced flag must exit 2, got {:?}",
            typo.status.code()
        ));
    }
    let typo_stderr = String::from_utf8_lossy(&typo.stderr).into_owned();
    assert_contains(
        "typo stderr",
        &typo_stderr,
        "Did you mean `--candidate-tree`?",
    )?;
    assert_contains(
        "typo stderr",
        &typo_stderr,
        "Run `ripr check --help --all`.",
    )?;
    Ok(())
}

/// A value consumed by an option is never the `--all` flag: `--diff --all
/// --help` names a diff file called `--all` and wants the default summary,
/// while a real trailing `--all` still selects the reference.
#[test]
fn check_help_all_is_a_parsed_flag_not_a_raw_scan() -> Result<(), String> {
    let short = rendered_help(&["check", "--help"])?;
    if rendered_help(&["check", "--diff", "--all", "--help"])? != short {
        return Err("`--diff --all --help` must print the one-screen summary".to_string());
    }
    let full = rendered_help(&["check", "--help", "--all"])?;
    if rendered_help(&["check", "--diff", "d.patch", "--help", "--all"])? != full {
        return Err("a real `--all` after other options must select the reference".to_string());
    }
    assert_contains(
        "`ripr check --help`",
        &normalized(&short),
        "walks up to the enclosing workspace or package",
    )
}

/// Help wins over later option values: a missing or invalid value after
/// `--help` must not turn the help request into a usage error.
#[test]
fn check_help_wins_over_later_option_values() -> Result<(), String> {
    let short = rendered_help(&["check", "--help"])?;
    for args in [
        &["check", "--help", "--format"][..],
        &["check", "--help", "--mode", "invalid"][..],
        &["check", "--bogus", "--help"][..],
        // Enumerated values never look like flags, so these are help requests.
        &["check", "--mode", "--help"][..],
        &["check", "--format", "-h"][..],
    ] {
        if rendered_help(args)? != short {
            return Err(format!("{args:?} must print the one-screen summary"));
        }
    }
    // A free-form value may be flag-shaped: `--diff --help` names a file called
    // `--help`, so it is not a help request.
    let diff = run_ripr(&["check", "--diff", "--help"])?;
    if diff.status.success() || String::from_utf8_lossy(&diff.stdout).contains("Common options:") {
        return Err("`check --diff --help` must treat `--help` as a diff path".to_string());
    }
    Ok(())
}
