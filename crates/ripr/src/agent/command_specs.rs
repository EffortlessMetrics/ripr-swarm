//! Producer-owned typed descriptions for the canonical agent routes.

use super::loop_commands::{agent_receipt_command, agent_verify_command};
use crate::domain::{
    CancellationPolicy, CommandAuthorityBoundary, CommandCostClass, CommandExecutionMode,
    CommandPlatform, CommandRole, CommandSpec, EnvironmentPolicy, ExpectedResultParser,
    NetworkPolicy, StdinPolicy,
};

pub(crate) fn agent_verify_command_spec(
    root: &str,
    before_path: &str,
    after_path: &str,
    out_path: Option<&str>,
) -> CommandSpec {
    let display = agent_verify_command(root, before_path, after_path, out_path);
    let expected_writes = out_path.into_iter().map(ToOwned::to_owned).collect();
    command_spec(
        "ripr:agent:verify",
        CommandRole::Verify,
        if out_path.is_some() {
            CommandExecutionMode::ShellRequired
        } else {
            CommandExecutionMode::Direct
        },
        vec![
            "agent".to_string(),
            "verify".to_string(),
            "--root".to_string(),
            root.to_string(),
            "--before".to_string(),
            before_path.to_string(),
            "--after".to_string(),
            after_path.to_string(),
            "--json".to_string(),
        ],
        expected_writes,
        display,
    )
}

pub(crate) fn agent_receipt_command_spec(
    root: &str,
    verify_json: &str,
    seam_id: &str,
    out_path: Option<&str>,
) -> CommandSpec {
    let display = agent_receipt_command(root, verify_json, seam_id, out_path);
    let mut args = vec![
        "agent".to_string(),
        "receipt".to_string(),
        "--root".to_string(),
        root.to_string(),
        "--verify-json".to_string(),
        verify_json.to_string(),
        "--seam-id".to_string(),
        seam_id.to_string(),
        "--json".to_string(),
    ];
    let expected_writes = out_path
        .map(|path| {
            args.extend(["--out".to_string(), path.to_string()]);
            vec![path.to_string()]
        })
        .unwrap_or_default();
    command_spec(
        "ripr:agent:receipt",
        CommandRole::Receipt,
        CommandExecutionMode::Direct,
        args,
        expected_writes,
        display,
    )
}

/// Recover a typed spec only for canonical agent routes. Arbitrary
/// user-supplied test commands remain legacy advisory text until a producer
/// supplies their executable and argument boundary.
pub(crate) fn agent_command_spec_from_display(command: &str) -> Option<CommandSpec> {
    let words = shell_words(command)?;
    if words.iter().any(|word| {
        word.chars()
            .any(|character| matches!(character, ';' | '&' | '|' | '<' | '`' | '$'))
    }) {
        return None;
    }
    if words.first().map(String::as_str) != Some("ripr")
        || words.get(1).map(String::as_str) != Some("agent")
    {
        return None;
    }
    match words.get(2).map(String::as_str) {
        Some("verify") => {
            let redirect = words.iter().position(|word| word == ">");
            let (args, execution_mode, expected_writes) = match redirect {
                Some(redirect) => {
                    if redirect + 2 != words.len() {
                        return None;
                    }
                    let out_path = words.get(redirect + 1)?.as_str();
                    (
                        words.get(1..redirect)?.to_vec(),
                        CommandExecutionMode::ShellRequired,
                        vec![out_path.to_string()],
                    )
                }
                None => (
                    words.get(1..)?.to_vec(),
                    CommandExecutionMode::Direct,
                    Vec::new(),
                ),
            };
            if args.is_empty() {
                return None;
            }
            Some(command_spec(
                "ripr:agent:verify",
                CommandRole::Verify,
                execution_mode,
                args,
                expected_writes,
                command.to_string(),
            ))
        }
        Some("receipt") => {
            let args = words.get(1..)?.to_vec();
            if args.is_empty() || args.iter().any(|arg| arg == ">") {
                return None;
            }
            let mut expected_writes = Vec::new();
            let mut out_seen = false;
            for (index, arg) in args.iter().enumerate() {
                if arg != "--out" {
                    continue;
                }
                if out_seen {
                    return None;
                }
                let out_path = args.get(index + 1)?;
                if out_path.is_empty() || out_path == ">" {
                    return None;
                }
                expected_writes.push(out_path.clone());
                out_seen = true;
            }
            Some(command_spec(
                "ripr:agent:receipt",
                CommandRole::Receipt,
                CommandExecutionMode::Direct,
                args,
                expected_writes,
                command.to_string(),
            ))
        }
        _ => None,
    }
}

fn command_spec(
    command_id: &str,
    role: CommandRole,
    execution_mode: CommandExecutionMode,
    args: Vec<String>,
    expected_writes: Vec<String>,
    display: String,
) -> CommandSpec {
    CommandSpec {
        schema_version: CommandSpec::SCHEMA_VERSION.to_string(),
        command_id: command_id.to_string(),
        role,
        execution_mode,
        program: "ripr".to_string(),
        args,
        cwd: ".".to_string(),
        env_set: Vec::new(),
        env_passthrough: Vec::new(),
        environment_policy: EnvironmentPolicy::Clean,
        stdin: StdinPolicy::Null,
        timeout_ms: 120_000,
        cancellation: CancellationPolicy::Allowed,
        network_policy: NetworkPolicy::Forbidden,
        expected_result_parser: ExpectedResultParser::DeclaredJson,
        expected_exit_codes: vec![0],
        expected_writes,
        cost_class: CommandCostClass::Unknown,
        platforms: vec![
            CommandPlatform::Linux,
            CommandPlatform::Macos,
            CommandPlatform::Windows,
        ],
        display,
        authority_boundary: match role {
            CommandRole::Verify => CommandAuthorityBoundary::VerificationRouteOnly,
            CommandRole::Receipt => CommandAuthorityBoundary::ReceiptRouteOnly,
            CommandRole::Regeneration => CommandAuthorityBoundary::RegenerationRouteOnly,
            CommandRole::Inspection => CommandAuthorityBoundary::InspectionRouteOnly,
            CommandRole::TargetedRerun => CommandAuthorityBoundary::TargetedRerunRouteOnly,
        },
    }
}

fn shell_words(command: &str) -> Option<Vec<String>> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    let mut escaped = false;
    for character in command.chars() {
        if escaped {
            current.push(character);
            escaped = false;
            continue;
        }
        match (quote, character) {
            // #3231: inside single quotes a backslash is literal — the
            // renderer (`loop_commands::shell_arg`) documents bash
            // single-quote semantics, and the recovery parser must agree or
            // a single-quoted Windows-like token cannot round-trip. Outside
            // single quotes the backslash keeps its escape meaning (unquoted
            // and double-quoted behavior).
            (Some('\''), '\\') => current.push(character),
            (_, '\\') => escaped = true,
            (Some(active), value) if value == active => quote = None,
            (Some(_), ';' | '&' | '|' | '<' | '>' | '`' | '$') => {
                return None;
            }
            (Some(_), value) => current.push(value),
            (None, '\'' | '"') => quote = Some(character),
            (None, value) if value.is_whitespace() => {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
            }
            (None, '>') => {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
                words.push(">".to_string());
            }
            (None, value) => current.push(value),
        }
    }
    if escaped || quote.is_some() {
        return None;
    }
    if !current.is_empty() {
        words.push(current);
    }
    Some(words)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::loop_commands::{
        WORKFLOW_AFTER_SNAPSHOT_ARTIFACT, WORKFLOW_AGENT_RECEIPT_ARTIFACT,
        WORKFLOW_AGENT_VERIFY_ARTIFACT, WORKFLOW_BEFORE_SNAPSHOT_ARTIFACT,
    };

    #[test]
    fn typed_agent_routes_preserve_argv_and_disclose_shell_redirection() -> Result<(), String> {
        let verify = agent_verify_command_spec(
            ".",
            WORKFLOW_BEFORE_SNAPSHOT_ARTIFACT,
            WORKFLOW_AFTER_SNAPSHOT_ARTIFACT,
            Some(WORKFLOW_AGENT_VERIFY_ARTIFACT),
        );
        if verify.execution_mode != CommandExecutionMode::ShellRequired {
            return Err("verify redirect was not marked shell_required".to_string());
        }
        if verify.args.get(0..2) != Some(["agent".to_string(), "verify".to_string()].as_slice()) {
            return Err(format!("unexpected verify argv: {:?}", verify.args));
        }
        verify.validate().map_err(|err| err.to_string())?;

        let receipt = agent_receipt_command_spec(
            ".",
            WORKFLOW_AGENT_VERIFY_ARTIFACT,
            "seam-a",
            Some(WORKFLOW_AGENT_RECEIPT_ARTIFACT),
        );
        if receipt.execution_mode != CommandExecutionMode::Direct {
            return Err("receipt --out route was not marked direct".to_string());
        }
        if !receipt
            .args
            .windows(2)
            .any(|pair| pair == ["--out", WORKFLOW_AGENT_RECEIPT_ARTIFACT])
        {
            return Err(format!("receipt argv omitted --out: {:?}", receipt.args));
        }
        receipt.validate().map_err(|err| err.to_string())
    }

    /// #3231: inside single quotes a backslash is literal, so a
    /// single-quoted Windows-like token round-trips byte-for-byte through
    /// the bounded recovery parser — the renderer (`shell_arg`) and the
    /// parser must agree on bash single-quote semantics. The token is built
    /// at runtime so no literal drive-letter path sits in the source tree.
    #[test]
    fn single_quoted_backslash_tokens_round_trip_through_recovery() -> Result<(), String> {
        let windows_root = format!("C:{}ws{}repo", '\\', '\\');
        let display = format!(
            "ripr agent verify --root '{windows_root}' --before before.json --after after.json --json"
        );
        let spec =
            agent_command_spec_from_display(&display).ok_or("quoted route was not recoverable")?;
        if !spec
            .args
            .windows(2)
            .any(|pair| pair == ["--root", windows_root.as_str()])
        {
            return Err(format!(
                "single-quoted Windows-like token lost its backslashes: {:?}",
                spec.args
            ));
        }
        // Renderer agreement: the same token rendered by `shell_arg` (inside
        // `agent_verify_command`) recovers to the identical argv entry.
        let rendered = crate::agent::loop_commands::agent_verify_command(
            &windows_root,
            "before.json",
            "after.json",
            None,
        );
        if !rendered.contains(&format!("'{windows_root}'")) {
            return Err(format!("renderer left the token unquoted: {rendered}"));
        }
        let round_tripped = agent_command_spec_from_display(&rendered)
            .ok_or("rendered route was not recoverable")?;
        if !round_tripped
            .args
            .windows(2)
            .any(|pair| pair == ["--root", windows_root.as_str()])
        {
            return Err(format!(
                "rendered token did not round-trip byte-for-byte: {:?}",
                round_tripped.args
            ));
        }
        Ok(())
    }

    /// #3231: outside single quotes the backslash keeps its escape meaning —
    /// unquoted and double-quoted tokens consume it as an escape, matching
    /// the pre-fix behavior the canonical display tokens rely on.
    #[test]
    fn backslash_outside_single_quotes_keeps_escape_semantics() -> Result<(), String> {
        if shell_words(r"a\b c") != Some(vec!["ab".to_string(), "c".to_string()]) {
            return Err("unquoted backslash must escape the next character".to_string());
        }
        if shell_words(r#""a\b" c"#) != Some(vec!["ab".to_string(), "c".to_string()]) {
            return Err("double-quoted backslash must escape the next character".to_string());
        }
        if shell_words(r"'a\b' c") != Some(vec!["a\\b".to_string(), "c".to_string()]) {
            return Err("single-quoted backslash must stay literal".to_string());
        }
        Ok(())
    }

    /// #3231 drift test: producer-owned typed routes carry the semantic
    /// Windows-like token directly in `args[]`, built from semantic inputs —
    /// they never parse their own display text through `shell_words`. A drift
    /// here would re-establish display text as argv authority (#1906).
    #[test]
    fn producer_owned_routes_preserve_windows_argv_without_display_parsing() -> Result<(), String> {
        let windows_root = format!("C:{}ws{}repo", '\\', '\\');
        let verify = agent_verify_command_spec(&windows_root, "before.json", "after.json", None);
        if !verify
            .args
            .windows(2)
            .any(|pair| pair == ["--root", windows_root.as_str()])
        {
            return Err(format!(
                "verify spec lost the semantic Windows token in args: {:?}",
                verify.args
            ));
        }
        let receipt = agent_receipt_command_spec(&windows_root, "verify.json", "seam-a", None);
        if !receipt
            .args
            .windows(2)
            .any(|pair| pair == ["--root", windows_root.as_str()])
        {
            return Err(format!(
                "receipt spec lost the semantic Windows token in args: {:?}",
                receipt.args
            ));
        }
        Ok(())
    }

    #[test]
    fn typed_route_recovery_rejects_arbitrary_test_commands() -> Result<(), String> {
        if agent_command_spec_from_display("cargo test pricing").is_some() {
            return Err("arbitrary test command was promoted without a producer".to_string());
        }
        if agent_command_spec_from_display(
            "ripr agent verify --root . --before before.json --after after.json --json && whoami",
        )
        .is_some()
        {
            return Err("compound shell command crossed the typed route boundary".to_string());
        }
        if agent_command_spec_from_display(
            "ripr agent verify --root . --before before.json --after after.json --json || whoami",
        )
        .is_some()
        {
            return Err(
                "alternate compound shell command crossed the typed route boundary".to_string(),
            );
        }
        if agent_command_spec_from_display(
            "ripr agent verify --root . --before before.json --after after.json --json \">\" verify.json",
        )
        .is_some()
        {
            return Err("quoted redirection operator crossed the typed route boundary".to_string());
        }
        let verify = agent_command_spec_from_display(
            "ripr agent verify --root . --before before.json --after after.json --json > verify.json",
        )
        .ok_or_else(|| "canonical verify route was not recoverable".to_string())?;
        if verify.execution_mode != CommandExecutionMode::ShellRequired {
            return Err("recovered verify redirect was not shell_required".to_string());
        }
        if verify.args.get(0..2) != Some(["agent".to_string(), "verify".to_string()].as_slice()) {
            return Err(format!(
                "recovered verify argv omitted route words: {:?}",
                verify.args
            ));
        }
        verify.validate().map_err(|err| err.to_string())?;

        let receipt = agent_command_spec_from_display(
            "ripr agent receipt --root . --verify-json verify.json --seam-id seam-a --json --out receipt.json",
        )
        .ok_or_else(|| "canonical receipt route was not recoverable".to_string())?;
        if receipt.args.get(0..2) != Some(["agent".to_string(), "receipt".to_string()].as_slice()) {
            return Err(format!(
                "recovered receipt argv omitted route words: {:?}",
                receipt.args
            ));
        }
        if receipt.expected_writes != ["receipt.json".to_string()] {
            return Err(format!(
                "recovered receipt output path was not preserved: {:?}",
                receipt.expected_writes
            ));
        }
        if agent_command_spec_from_display(
            "ripr agent receipt --root . --verify-json verify.json --seam-id seam-a --json --out",
        )
        .is_some()
        {
            return Err("receipt route accepted a missing --out path".to_string());
        }
        receipt.validate().map_err(|err| err.to_string())
    }
}
