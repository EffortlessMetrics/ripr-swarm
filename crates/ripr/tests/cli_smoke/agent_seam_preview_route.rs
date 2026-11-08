//! #13791: when the selected scope's changed code is TypeScript/Python
//! preview, `ripr agent brief`, `ripr agent card --seam-id` and
//! `ripr agent packet --seam-id` say so in one line and name the working
//! route (`ripr check --format agent`, then the gap-bound start `check`
//! prints). A preview finding id is told apart from a truly unknown id,
//! and exit codes and JSON statuses stay what they were.
use super::{ignore_remove_dir_all, run_command, run_git, unique_temp_workspace};
use serde_json::Value;
use std::path::Path;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

const COMMITTER: [&str; 4] = [
    "-c",
    "user.name=RIPR test",
    "-c",
    "user.email=ripr@example.invalid",
];

/// A committed `main` holding `base`, then a `feature` branch whose only
/// commit writes `head`. `check --base main` sees exactly that change.
fn branch_fixture(
    label: &str,
    base: &[(&str, &str)],
    head: &[(&str, &str)],
) -> TestResult<std::path::PathBuf> {
    let root = unique_temp_workspace(label);
    std::fs::create_dir_all(&root)?;
    let write = |files: &[(&str, &str)]| -> TestResult {
        for (path, text) in files {
            let target = root.join(path);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(target, text)?;
        }
        Ok(())
    };
    run_git(&root, &["init", "-q", "-b", "main"])?;
    run_git(&root, &["config", "core.autocrlf", "false"])?;
    write(base)?;
    run_git(&root, &["add", "-A"])?;
    run_git(
        &root,
        &[&COMMITTER[..], &["commit", "-qm", "base"]].concat(),
    )?;
    run_git(&root, &["checkout", "-q", "-b", "feature"])?;
    write(head)?;
    run_git(&root, &["add", "-A"])?;
    run_git(
        &root,
        &[&COMMITTER[..], &["commit", "-qm", "change"]].concat(),
    )?;
    Ok(root)
}

fn ripr(root: &Path, args: &[&str]) -> TestResult<(Option<i32>, String, String)> {
    let output = run_command(env!("CARGO_BIN_EXE_ripr"), Some(root), args)?;
    Ok((
        output.status.code(),
        String::from_utf8(output.stdout)?,
        String::from_utf8(output.stderr)?,
    ))
}

/// The first `probe:` finding id and `gap:` start id `check --format agent`
/// prints. The fixture must produce a finding, so a missing id is a
/// fixture failure, not a pass.
fn check_ids(root: &Path) -> TestResult<(String, Option<String>)> {
    let (code, stdout, stderr) = ripr(
        root,
        &[
            "check", "--root", ".", "--base", "main", "--format", "agent",
        ],
    )?;
    assert_eq!(code, Some(0), "check failed: {stdout}{stderr}");
    let token = |prefix: &str| {
        stdout
            .split(['|', ' ', '\n'])
            .find_map(|token| {
                token
                    .strip_prefix("id=")
                    .unwrap_or(token)
                    .strip_prefix(prefix)
            })
            .map(|rest| format!("{prefix}{rest}"))
    };
    let probe = token("probe:").ok_or_else(|| format!("check printed no finding id:\n{stdout}"))?;
    Ok((probe, token("gap:")))
}

fn assert_brief_names_preview_route(root: &Path, language: &str) -> TestResult {
    let (code, stdout, stderr) = ripr(
        root,
        &["agent", "brief", "--root", ".", "--base", "main", "--json"],
    )?;
    assert_eq!(code, Some(0), "brief failed: {stderr}");
    let brief: Value = serde_json::from_str(&stdout)?;
    assert_eq!(brief["top_seams"], Value::Array(Vec::new()), "{stdout}");
    let warnings = brief["warnings"]
        .as_array()
        .ok_or("brief has no warnings")?;
    let scope = warnings
        .iter()
        .filter_map(Value::as_str)
        .find(|warning| warning.starts_with("No seams matched the requested scope"))
        .ok_or_else(|| format!("brief has no empty-scope warning: {stdout}"))?;
    assert!(
        scope.contains(&format!("the changed code is {language} preview code"))
            && scope.contains("--base main --format agent`")
            && scope
                .contains("`ripr agent repair --gap-id <gap> --base main --phase before` start")
            && scope.contains("covers Rust files only")
            && !scope.contains("under the current config")
            && !scope.contains('\n'),
        "brief must name the preview route in one line: {scope}"
    );
    Ok(())
}

fn assert_seam_surfaces_name_preview_route(
    root: &Path,
    language: &str,
    id: &str,
    family: &str,
) -> TestResult {
    let (code, stdout, stderr) = ripr(root, &["agent", "card", "--seam-id", id, "--root", "."])?;
    assert_eq!(code, Some(3), "card exit: {stdout}{stderr}");
    for needle in [
        format!("is shaped like a {language} preview {family} from ripr check, not a seam ID"),
        "--format agent`".to_string(),
        "`ripr agent repair --gap-id <gap> --base <base> --phase before` start".to_string(),
        "covers Rust files only".to_string(),
    ] {
        assert!(stderr.contains(&needle), "card misses `{needle}`: {stderr}");
    }
    assert!(!stderr.contains("names no current seam"), "{stderr}");

    let (code, stdout, stderr) = ripr(
        root,
        &["agent", "packet", "--seam-id", id, "--root", ".", "--json"],
    )?;
    assert_eq!(code, Some(3), "packet exit: {stdout}{stderr}");
    let packet: Value = serde_json::from_str(&stdout)?;
    // The machine status is unchanged; the message and remedy carry the
    // family and the working route.
    assert_eq!(packet["analysis_scope"]["run_status"], "seam_id_unknown");
    let limitation = &packet["run_limitations"][0];
    let message = limitation["message"]
        .as_str()
        .ok_or("packet has no message")?;
    assert!(
        message.contains(&format!(
            "is shaped like a {language} preview {family} from ripr check, not a seam ID"
        )) && !message.contains("was not found"),
        "packet must name the id family: {message}"
    );
    let command = limitation["remedy"]["command"]
        .as_str()
        .ok_or("packet remedy has no command")?;
    assert!(
        command.starts_with("ripr check --root ") && command.ends_with(" --format agent"),
        "packet remedy must be the preview route: {command}"
    );
    Ok(())
}

/// Negative controls in the same preview repository: a truly unknown seam
/// id and a stale Rust-shaped finding id keep their prior wording and never
/// claim the preview route.
fn assert_unknown_ids_unchanged(root: &Path) -> TestResult {
    let (code, stdout, _) = ripr(
        root,
        &[
            "agent",
            "packet",
            "--seam-id",
            "seam:definitely-not-real-13791",
            "--root",
            ".",
            "--json",
        ],
    )?;
    assert_eq!(code, Some(3));
    let packet: Value = serde_json::from_str(&stdout)?;
    assert_eq!(packet["analysis_scope"]["run_status"], "seam_id_unknown");
    let message = packet["run_limitations"][0]["message"]
        .as_str()
        .unwrap_or_default();
    assert!(
        message.contains("was not found")
            && !message.contains("preview code")
            && !message.contains("--format agent"),
        "unknown id: {message}"
    );

    let (code, _, stderr) = ripr(
        root,
        &[
            "agent",
            "card",
            "--seam-id",
            "probe:src_lib.rs:12:predicate",
            "--root",
            ".",
        ],
    )?;
    assert_eq!(code, Some(3));
    assert!(
        stderr.contains("stale or maps to no current seam")
            && !stderr.contains("preview code")
            && !stderr.contains("--format agent"),
        "stale Rust finding id: {stderr}"
    );
    Ok(())
}

#[cfg(feature = "lang-typescript")]
#[test]
fn typescript_preview_scope_names_the_working_route() -> TestResult {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/ts_repair_packet_complete/input");
    let read = |relative: &str| std::fs::read_to_string(fixture.join(relative));
    let head = read("src/discount.ts")?;
    let base = head.replace("amount >= threshold", "amount > threshold");
    assert_ne!(base, head, "fixture source must carry the boundary change");
    let (package, toml, test) = (
        read("package.json")?,
        read("ripr.toml")?,
        read("tests/discount.test.ts")?,
    );
    let root = branch_fixture(
        "agent-seam-preview-ts",
        &[
            ("package.json", &package),
            ("ripr.toml", &toml),
            ("tests/discount.test.ts", &test),
            ("src/discount.ts", &base),
            (".gitignore", "/target/\n.ripr/\n"),
        ],
        &[("src/discount.ts", &head)],
    )?;
    let (probe, gap) = check_ids(&root)?;
    assert!(probe.contains("typescript_preview"), "{probe}");
    assert_brief_names_preview_route(&root, "TypeScript")?;
    assert_seam_surfaces_name_preview_route(&root, "TypeScript", &probe, "finding ID")?;
    if let Some(gap) = gap {
        assert_seam_surfaces_name_preview_route(&root, "TypeScript", &gap, "canonical gap ID")?;
    }
    assert_unknown_ids_unchanged(&root)?;
    ignore_remove_dir_all(&root);
    Ok(())
}

#[cfg(feature = "lang-python")]
#[test]
fn python_preview_scope_names_the_working_route() -> TestResult {
    let source = |op: &str| {
        format!(
            "def discounted_total(amount, limit):\n    if amount {op} limit:\n        return amount - 10\n    return amount\n"
        )
    };
    let (base, head) = (source(">="), source(">"));
    let root = branch_fixture(
        "agent-seam-preview-py",
        &[
            (
                "pyproject.toml",
                "[project]\nname = \"fx\"\nversion = \"0.0.0\"\n",
            ),
            ("ripr.toml", "[languages]\nenabled = [\"python\"]\n"),
            ("pricing.py", &base),
            (
                "test_p.py",
                "from pricing import discounted_total\ndef test_boundary():\n    assert discounted_total(100, 100) > 0\n",
            ),
            (".gitignore", ".ripr/\n__pycache__/\n/target/\n"),
        ],
        &[("pricing.py", &head)],
    )?;
    let (probe, _) = check_ids(&root)?;
    assert!(probe.contains("python_preview"), "{probe}");
    assert_brief_names_preview_route(&root, "Python")?;
    assert_seam_surfaces_name_preview_route(&root, "Python", &probe, "finding ID")?;
    assert_unknown_ids_unchanged(&root)?;
    ignore_remove_dir_all(&root);
    Ok(())
}
