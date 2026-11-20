//! #13599: `ripr check` human, `--format agent` and `--format json` agree on
//! whether `ripr agent repair` can start a Python or TypeScript gap.
//!
//! The compact agent format used to print "offers no guided repair route" and
//! `gaps=0` for a finding the human screen and the JSON `finding_routes`
//! started with `ripr agent repair --gap-id ...`. All three now read the one
//! validated gap-bound route decision.

#![cfg(any(feature = "lang-python", feature = "lang-typescript"))]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

#[path = "common/mod.rs"]
mod common;

use common::fixture_git::fixture_git_ok;

struct TempDirGuard(PathBuf);

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        if let Ok(()) = std::fs::remove_dir_all(&self.0) {}
    }
}

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|error| format!("create {}: {error}", to.display()))?;
    for entry in
        std::fs::read_dir(from).map_err(|error| format!("read {}: {error}", from.display()))?
    {
        let entry = entry.map_err(|error| format!("entry: {error}"))?;
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)
                .map_err(|error| format!("copy {}: {error}", entry.path().display()))?;
        }
    }
    Ok(())
}

fn materialize(
    fixture: &str,
    label: &str,
    ripr_toml: Option<&str>,
) -> Result<(TempDirGuard, PathBuf), String> {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(fixture)
        .join("repository");
    let launch = std::env::temp_dir().join(format!(
        "ripr-agent-route-{label}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let guard = TempDirGuard(launch.clone());
    let root = launch.join("repo");
    copy_tree(&source.join("base"), &root)?;
    fixture_git_ok(&root, &["init", "-q", "-b", "main"])?;
    fixture_git_ok(&root, &["config", "core.autocrlf", "false"])?;
    fixture_git_ok(&root, &["config", "commit.gpgsign", "false"])?;
    fixture_git_ok(&root, &["config", "user.name", "fixture"])?;
    fixture_git_ok(&root, &["config", "user.email", "fixture@example.invalid"])?;
    fixture_git_ok(&root, &["add", "-A"])?;
    fixture_git_ok(&root, &["commit", "-q", "-m", "base"])?;
    copy_tree(&source.join("head"), &root)?;
    fixture_git_ok(&root, &["add", "-A"])?;
    fixture_git_ok(&root, &["commit", "-q", "-m", "head"])?;
    if let Some(text) = ripr_toml {
        std::fs::write(root.join("ripr.toml"), text)
            .map_err(|error| format!("ripr.toml: {error}"))?;
    }
    Ok((guard, root))
}

fn check(root: &Path, base: &str, format: &str) -> Result<String, String> {
    let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
        .current_dir(root)
        .args(["check", "--quiet", "--base", base, "--format", format])
        .output()
        .map_err(|error| format!("spawn ripr: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "check --format {format} failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Whether some line of `text` is a `ripr agent repair` start for `gap`.
fn names_start_for(text: &str, gap: &str) -> bool {
    text.lines().any(|line| {
        line.contains("ripr agent repair --gap-id") && line.contains(gap) && line.contains("--base")
    })
}

fn agree_on_startable_gap(
    fixture: &str,
    label: &str,
    ripr_toml: Option<&str>,
) -> Result<(), String> {
    let (_guard, root) = materialize(fixture, label, ripr_toml)?;
    let human = check(&root, "HEAD~1", "human")?;
    let agent = check(&root, "HEAD~1", "agent")?;
    let json = check(&root, "HEAD~1", "json")?;
    let document: serde_json::Value =
        serde_json::from_str(&json).map_err(|error| format!("check JSON: {error}"))?;

    // JSON: the finding route says ready and names the command and gap.
    let repair = &document["finding_routes"][0]["repair"];
    if repair["status"] != "ready" {
        return Err(format!("expected a ready repair route:\n{json}"));
    }
    let gap = repair["canonical_gap_id"]
        .as_str()
        .ok_or_else(|| "repair route names no gap id".to_string())?;
    let json_command = repair["command"]
        .as_str()
        .ok_or_else(|| "ready repair route names no command".to_string())?;

    // Human recommends the same start.
    if !names_start_for(&human, gap) {
        return Err(format!("human check names no start for `{gap}`:\n{human}"));
    }

    // The JSON canonical action carries the same start; its class stays
    // non-executable because naming a route does not run it.
    let action = &document["canonical_next_action"];
    if action["stop"]["repair_route"].as_str() != Some(json_command) {
        return Err(format!(
            "canonical_next_action.stop.repair_route differs from finding_routes:\n{json}"
        ));
    }

    // Agent: no denial, the same command under next=, and the header says a
    // repair can start beside gaps=.
    if agent.contains("no guided repair route") || agent.contains("stop=") {
        return Err(format!("agent output denies a startable repair:\n{agent}"));
    }
    if !agent
        .lines()
        .next()
        .is_some_and(|header| header.contains(" repairable=1"))
    {
        return Err(format!("agent header omits repairable=1:\n{agent}"));
    }
    // #13803: the listed preview gap is counted beside `gaps=0`, so the
    // header never claims zero gaps alone next to a listed gap record.
    if !agent
        .lines()
        .next()
        .is_some_and(|header| header.contains(" gaps=0 unconfirmed_gaps=1 "))
    {
        return Err(format!(
            "agent header does not count the listed preview gap:\n{agent}"
        ));
    }
    let next = agent
        .lines()
        .find_map(|line| line.strip_prefix("next="))
        .ok_or_else(|| format!("agent output names no next= command:\n{agent}"))?;
    if next != json_command {
        return Err(format!(
            "agent next= `{next}` differs from JSON `{json_command}`:\n{agent}"
        ));
    }
    if !agent.contains("\nopen=") {
        return Err(format!("agent output lacks an open= terminal:\n{agent}"));
    }
    Ok(())
}

#[cfg(feature = "lang-python")]
#[test]
fn python_human_agent_and_json_agree_the_gap_can_start() -> Result<(), String> {
    agree_on_startable_gap("blind_journey_installed_python", "py", None)
}

#[cfg(feature = "lang-typescript")]
#[test]
fn typescript_human_agent_and_json_agree_the_gap_can_start() -> Result<(), String> {
    agree_on_startable_gap(
        "blind_journey_installed_typescript",
        "ts",
        Some("[languages]\nenabled = [\"typescript\"]\n"),
    )
}

/// Negative control: with the tests removed no route is offered, so the agent
/// format must not invent a start.
#[cfg(feature = "lang-python")]
#[test]
fn python_without_a_route_does_not_get_an_agent_repair_start() -> Result<(), String> {
    let (_guard, root) = materialize("blind_journey_installed_python", "pyn", None)?;
    fixture_git_ok(&root, &["rm", "-q", "-r", "tests"])?;
    fixture_git_ok(&root, &["commit", "-q", "-m", "drop the tests"])?;
    let agent = check(&root, "HEAD~2", "agent")?;
    // The finding must still be reported, so the control cannot pass vacuously.
    // #13594: a no_static_path finding prints no `missing=-` record, so its id
    // is carried by the `next=ripr explain` drill-in instead.
    if !agent.contains("findings=1")
        || !agent
            .lines()
            .any(|line| line.starts_with("next=ripr explain ") && line.contains("probe:"))
    {
        return Err(format!("control lost its finding:\n{agent}"));
    }
    if agent.contains("repairable=") || agent.contains("ripr agent repair") {
        return Err(format!("agent output invented a repair start:\n{agent}"));
    }
    Ok(())
}
