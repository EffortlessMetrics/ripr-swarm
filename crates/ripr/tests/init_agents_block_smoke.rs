//! #11575: every `ripr …` command advertised in the `ripr init --agents` block
//! is accepted by the built CLI with a documented exit, not an argv usage error.
//!
//! Commands are parsed from the generated marked section (not hard-coded), so a
//! stale block spelling fails when the CLI rejects it.

use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const CARGO_TOML: &str =
    "[package]\nname = \"agents-block-smoke\"\nversion = \"0.1.0\"\nedition = \"2024\"\n";
const LIB: &str = "pub fn dbl(x: i32) -> i32 {\n    x * 2\n}\n";
const WEAK_TEST: &str =
    "use agents_block_smoke::dbl;\n\n#[test]\nfn weak() {\n    assert!(dbl(2) > 0);\n}\n";
const CHANGED_LIB: &str = "pub fn dbl(x: i32) -> i32 {\n    x * 3\n}\n";

const BEGIN: &str = "<!-- ripr:agents:begin";
const END: &str = "<!-- ripr:agents:end -->";

struct Repo(PathBuf);

impl Repo {
    fn fresh_gap_repo() -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-init-agents-block-smoke-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(root.join("tests")).map_err(|e| e.to_string())?;
        let repo = Self(root);
        repo.write(".gitignore", "/target/\n")?;
        repo.write("Cargo.toml", CARGO_TOML)?;
        repo.write("src/lib.rs", LIB)?;
        repo.write("tests/t.rs", WEAK_TEST)?;
        repo.git(&["-c", "init.templateDir=", "init", "-q", "-b", "main"])?;
        repo.git(&["config", "user.email", "t@example.com"])?;
        repo.git(&["config", "user.name", "t"])?;
        repo.git(&["config", "commit.gpgSign", "false"])?;
        repo.git(&["add", "-A"])?;
        repo.git(&["commit", "-q", "--no-gpg-sign", "-m", "base"])?;
        repo.write("src/lib.rs", CHANGED_LIB)?;
        repo.git(&["commit", "-qam", "change"])?;
        Ok(repo)
    }

    fn write(&self, name: &str, text: &str) -> Result<(), String> {
        std::fs::write(self.0.join(name), text).map_err(|e| e.to_string())
    }

    fn git(&self, args: &[&str]) -> Result<(), String> {
        let output = Command::new("git")
            .current_dir(&self.0)
            .args(args)
            .output()
            .map_err(|e| e.to_string())?;
        if output.status.success() {
            Ok(())
        } else {
            Err(format!(
                "git {args:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            ))
        }
    }

    fn ripr(&self, argv: &[&str]) -> Result<Output, String> {
        Command::new(env!("CARGO_BIN_EXE_ripr"))
            .current_dir(&self.0)
            .args(argv)
            .output()
            .map_err(|e| e.to_string())
    }

    fn init_agents_block(&self) -> Result<String, String> {
        let output = self.ripr(&["init", "--agents", "codex"])?;
        if !output.status.success() {
            return Err(format!(
                "init --agents failed: {}",
                combined_output(&output)
            ));
        }
        let agents =
            std::fs::read_to_string(self.0.join("AGENTS.md")).map_err(|e| e.to_string())?;
        let begin = agents
            .find(BEGIN)
            .ok_or_else(|| format!("AGENTS.md missing begin marker:\n{agents}"))?;
        let end = agents[begin..]
            .find(END)
            .ok_or_else(|| format!("AGENTS.md missing end marker:\n{agents}"))?;
        Ok(agents[begin..begin + end + END.len()].to_string())
    }

    fn check_for_expansion(&self) -> Result<String, String> {
        let output = self.ripr(&["check", "--base", "HEAD~1"])?;
        Ok(combined_output(&output))
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn combined_output(output: &Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn exit_code(output: &Output) -> Option<i32> {
    output.status.code()
}

fn is_argv_usage_error(output: &Output) -> bool {
    if exit_code(output) != Some(2) || !output.stdout.is_empty() {
        return false;
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    stderr.starts_with("ripr: ")
        && (stderr.contains("unknown subcommand")
            || stderr.contains("missing value for")
            || stderr.contains("unknown ")
            || stderr.contains("Accepted:")
            || stderr.contains("Did you mean"))
}

fn parse_block_commands(section: &str) -> Vec<String> {
    let mut out = Vec::new();
    for segment in section.split('`') {
        let trimmed = segment.trim();
        if !trimmed.starts_with("ripr ") {
            continue;
        }
        let cmd = trimmed.split(" | ").next().unwrap_or(trimmed).trim();
        if cmd.is_empty() {
            continue;
        }
        // The begin marker mentions `ripr init --agents` as documentation, not a runnable argv.
        if cmd == "ripr init --agents" {
            continue;
        }
        out.push(cmd.to_string());
    }
    out
}

fn is_template(command: &str) -> bool {
    command.contains('<') || command.contains("...")
}

fn ripr_argv(command: &str) -> Vec<String> {
    command
        .strip_prefix("ripr ")
        .unwrap_or(command)
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

fn printed_lines<'a>(check: &'a str, prefix: &str) -> Vec<&'a str> {
    check
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with(prefix))
        .collect()
}

fn printed_repair_seam(check: &str) -> Option<String> {
    check.lines().find_map(|line| {
        let rest = line.trim().strip_prefix("ripr repair ")?;
        let id = rest.split_whitespace().next()?;
        (id.len() == 16 && id.chars().all(|c| c.is_ascii_hexdigit())).then(|| id.to_string())
    })
}

fn allowed_exit(command: &str, code: i32) -> bool {
    let lower = command.to_ascii_lowercase();
    if lower.starts_with("ripr check") {
        return matches!(code, 0 | 2 | 3);
    }
    if lower.starts_with("ripr repair") || lower.starts_with("ripr continue") {
        return matches!(code, 0 | 2 | 3);
    }
    if lower.starts_with("ripr status") {
        return matches!(code, 0 | 2);
    }
    if lower.starts_with("ripr init") {
        return code == 0;
    }
    matches!(code, 0 | 2 | 3)
}

fn assert_command(repo: &Repo, command: &str) -> Result<(), String> {
    let argv = ripr_argv(command);
    let output = repo.ripr(&argv.iter().map(String::as_str).collect::<Vec<_>>())?;
    let code = exit_code(&output).ok_or_else(|| format!("no exit code for `{command}`"))?;
    if is_argv_usage_error(&output) {
        return Err(format!(
            "argv usage error for `{command}`:\n{}",
            combined_output(&output)
        ));
    }
    if !allowed_exit(command, code) {
        return Err(format!(
            "unexpected exit {code} for `{command}`:\n{}",
            combined_output(&output)
        ));
    }
    Ok(())
}

fn expand_templates(check: &str, templates: &[String]) -> Vec<String> {
    let mut expanded = Vec::new();
    for template in templates {
        if template.starts_with("ripr repair ") && template.contains('<') {
            if let Some(seam) = printed_repair_seam(check) {
                expanded.push(format!("ripr repair {seam} --root ."));
            }
            continue;
        }
        if template.starts_with("ripr explain ") {
            for line in printed_lines(check, "ripr explain ") {
                expanded.push(line.to_string());
            }
            continue;
        }
        if template.starts_with("ripr context ") {
            for line in printed_lines(check, "ripr context ") {
                expanded.push(line.to_string());
            }
            continue;
        }
        if template.starts_with("ripr agent stub ") {
            for line in printed_lines(check, "ripr agent stub ") {
                expanded.push(line.to_string());
            }
            continue;
        }
        if template.contains(" | jq ")
            && let Some(ripr_part) = template.split(" | jq ").next()
        {
            expanded.push(ripr_part.trim().to_string());
        }
    }
    expanded
}

#[test]
fn init_agents_block_commands_run_with_documented_exits() -> Result<(), String> {
    let repo = Repo::fresh_gap_repo()?;
    let section = repo.init_agents_block()?;
    let parsed = parse_block_commands(&section);
    assert!(
        parsed.iter().any(|c| c.starts_with("ripr repair ")),
        "block must advertise repair; parsed: {parsed:?}"
    );
    assert!(
        parsed.iter().any(|c| c == "ripr continue"),
        "block must advertise continue; parsed: {parsed:?}"
    );

    let check = repo.check_for_expansion()?;
    let (templates, literal): (Vec<_>, Vec<_>) = parsed
        .into_iter()
        .partition(|c| is_template(c) || c.contains(" | jq "));

    let mut run: Vec<String> = literal;
    run.extend(expand_templates(&check, &templates));
    run.sort();
    run.dedup();

    for command in &run {
        if command.starts_with("cargo ") {
            continue;
        }
        assert_command(&repo, command)?;
    }

    // Guided repair path from the block: repair then continue on the fixture seam.
    let seam = printed_repair_seam(&check)
        .ok_or_else(|| format!("expected a repair start in check output:\n{check}"))?;
    assert_command(&repo, &format!("ripr repair {seam} --root ."))?;
    assert_command(&repo, "ripr continue")?;

    Ok(())
}
