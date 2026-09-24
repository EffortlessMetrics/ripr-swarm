use std::error::Error;
use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[path = "common/mod.rs"]
mod common;

use common::fixture_git::{fixture_git_ok, fixture_git_output};

#[test]
fn generated_workflow_batches_compact_review_comments() -> Result<(), Box<dyn Error>> {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let root = std::env::temp_dir().join(format!(
        "ripr-generated-review-workflow-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&root)?;

    let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
        .args(["init", "--root"])
        .arg(&root)
        .args(["--ci", "github"])
        .output()?;
    assert!(
        output.status.success(),
        "ripr init failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let workflow = fs::read_to_string(root.join(".github/workflows/ripr.yml"))?;
    let review_endpoint = "gh api --method POST \"repos/${{ github.repository }}/pulls/${{ github.event.pull_request.number }}/reviews\"";
    let legacy_create_endpoint = "gh api --method POST \"repos/${{ github.repository }}/pulls/${{ github.event.pull_request.number }}/comments\"";
    let update_endpoint =
        "gh api --method PATCH \"repos/${{ github.repository }}/pulls/comments/$comment_id\"";
    assert_eq!(workflow.matches(review_endpoint).count(), 1);
    assert!(!workflow.contains(legacy_create_endpoint));
    assert!(workflow.contains(update_endpoint));
    assert!(workflow.contains("event: \"COMMENT\""));
    assert!(workflow.contains("comments: ["));
    assert!(workflow.contains("<details><summary>Full RIPR repair card</summary>"));
    assert!(workflow.contains("presentation=compact-v1"));
    // #3906: a card that carries the repair start leads the compact comment
    // with it; only cards without one fall back to the Verify line.
    assert!(
        workflow.contains(r#"captured("\nStart the repair:\n`(?<value>[^`]+)`"; "")) as $start"#)
    );
    assert!(workflow.contains(
        r#"(if $start then "Start the repair: `\($start)`" else "Verify: `\($verify)`" end) as $next"#
    ));
    assert!(workflow.contains("__ripr_legacy_presentation__"));
    assert!(workflow.contains("__ripr_compact_presentation_unreadable__"));
    assert!(workflow.contains("additional recommendation"));
    assert!(workflow.contains("target/ripr/review/comments.json"));
    assert!(workflow.contains("target/ripr/review/comments.md"));
    assert!(workflow.contains("Created one RIPR review with $create_count inline comment(s)."));

    fs::remove_dir_all(root)?;
    Ok(())
}

/// #4005: the generated "Capture pull request diff" step must use the pinned
/// diff contract (same presentation pins as the production loaders), resolve
/// and retain the exact base/head identities, record the patch digest, and
/// fail closed instead of handing RIPR an ambient-presentation patch.
#[test]
fn generated_capture_step_uses_pinned_diff_contract() -> Result<(), Box<dyn Error>> {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let root = std::env::temp_dir().join(format!(
        "ripr-generated-capture-contract-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&root)?;

    let output = run_ripr_init(&root)?;
    assert!(
        output.status.success(),
        "ripr init failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let workflow = fs::read_to_string(root.join(".github/workflows/ripr.yml"))?;
    let step = capture_pull_request_diff_step(&workflow)
        .ok_or("generated workflow has no 'Capture pull request diff' step")?;
    for pin in [
        "--no-ext-diff",
        "--no-textconv",
        "--no-color",
        "--unified=3",
        "--inter-hunk-context=0",
        "core.quotePath",
        "rev-parse",
        "sha256sum",
        "mktemp",
    ] {
        assert!(
            step.contains(pin),
            "capture step must pin {pin}; got:\n{step}"
        );
    }

    fs::remove_dir_all(root)?;
    Ok(())
}

/// #4005: docs/CI.md documents the same capture step the generator emits.
/// The doc copy and the template copy drifted before (unpinned `git diff
/// --binary` in both); the capture block must stay byte-identical so the
/// documented recipe cannot promise a different patch than `ripr init` ships.
#[test]
fn docs_ci_capture_step_matches_generated_template() -> Result<(), Box<dyn Error>> {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let root = std::env::temp_dir().join(format!(
        "ripr-generated-capture-doc-sync-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&root)?;

    let output = run_ripr_init(&root)?;
    assert!(
        output.status.success(),
        "ripr init failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let workflow = fs::read_to_string(root.join(".github/workflows/ripr.yml"))?;
    let generated = capture_pull_request_diff_step(&workflow)
        .ok_or("generated workflow has no 'Capture pull request diff' step")?;
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let doc = fs::read_to_string(manifest_dir.join("../../docs/CI.md"))?;
    let documented = capture_pull_request_diff_step(&doc)
        .ok_or("docs/CI.md has no 'Capture pull request diff' step")?;
    assert_eq!(
        generated, documented,
        "docs/CI.md capture step drifted from the generated template"
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

/// #4005: the pinned capture flags must retain a source edit that a
/// configured textconv driver hides. Control first: the pre-repair recipe
/// (`diff --binary` with ambient presentation) yields an empty patch, which
/// proves the fixture hides the edit. Then the repaired flag set — the same
/// pins the generated template now carries — must retain the edit with its
/// three-line context and no color bytes.
#[test]
fn pinned_capture_flags_retain_edit_hidden_by_textconv() -> Result<(), Box<dyn Error>> {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let root = std::env::temp_dir().join(format!(
        "ripr-generated-capture-textconv-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(root.join("src"))?;
    fixture_git_ok(&root, &["init", "--initial-branch=main"])?;
    fixture_git_ok(
        &root,
        &["config", "--local", "user.name", "Capture Contract"],
    )?;
    fixture_git_ok(
        &root,
        &["config", "--local", "user.email", "capture@example.com"],
    )?;
    fixture_git_ok(&root, &["config", "--local", "commit.gpgsign", "false"])?;
    fs::write(root.join(".gitattributes"), "src/lib.rs diff=audit\n")?;
    fs::write(
        root.join("src/lib.rs"),
        "pub const A: u32 = 1;\npub const B: u32 = 2;\npub const C: u32 = 3;\npub const VALUE: u32 = 1;\npub const D: u32 = 4;\npub const E: u32 = 5;\npub const F: u32 = 6;\n",
    )?;
    fixture_git_ok(&root, &["add", "."])?;
    fixture_git_ok(&root, &["commit", "--quiet", "-m", "base"])?;
    fixture_git_ok(&root, &["tag", "capture-base"])?;
    fs::write(
        root.join("src/lib.rs"),
        "pub const A: u32 = 1;\npub const B: u32 = 2;\npub const C: u32 = 3;\npub const VALUE: u32 = 2;\npub const D: u32 = 4;\npub const E: u32 = 5;\npub const F: u32 = 6;\n",
    )?;
    fixture_git_ok(&root, &["add", "src/lib.rs"])?;
    fixture_git_ok(&root, &["commit", "--quiet", "-m", "edit"])?;
    // Git itself is the constant-output helper on Unix and Windows; no
    // shell script or executable bit needed.
    fixture_git_ok(
        &root,
        &["config", "--local", "diff.audit.textconv", "git --version"],
    )?;
    let range = "capture-base...HEAD";
    let hidden = fixture_git_output(&root, &["diff", "--binary", range])?;
    assert!(
        hidden.trim().is_empty(),
        "the constant textconv must hide the source edit"
    );
    // Repaired recipe: the presentation pins the generated "Capture pull
    // request diff" step now carries (init.rs template + docs/CI.md).
    let retained = fixture_git_output(
        &root,
        &[
            "-c",
            "core.quotePath=true",
            "diff",
            "--binary",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            "--unified=3",
            "--inter-hunk-context=0",
            range,
        ],
    )?;
    assert!(
        retained.contains("pub const VALUE"),
        "pinned capture must retain the source edit despite textconv"
    );
    assert!(
        retained.contains("pub const C: u32 = 3;") && retained.contains("pub const D: u32 = 4;"),
        "pinned capture must keep context around the edit"
    );
    assert!(
        !retained.contains('\u{1b}'),
        "pinned capture must not contain color"
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

/// #4005: execute the generated capture step itself in a disposable
/// repository. Unix-only: the generated workflow declares `runs-on:
/// ubuntu-latest`, so `sh` plus coreutils/`jq` are the faithful runner.
/// The `${{ github.base_ref }}` expression is substituted with a real local
/// branch (there is no origin in a fixture); everything else runs verbatim,
/// so this fails when the template stops being executable shell.
#[cfg(unix)]
#[test]
fn generated_capture_step_runs_end_to_end() -> Result<(), Box<dyn Error>> {
    // Runner tools: the generated workflow targets ubuntu-latest, where
    // bash, coreutils, and jq exist. On minimal local Unix environments
    // without them, skip loudly instead of failing the suite.
    let tools = run_sh(
        "command -v bash >/dev/null && command -v sha256sum >/dev/null && command -v jq >/dev/null",
        std::env::temp_dir().as_path(),
    )?;
    if !tools.status.success() {
        eprintln!(
            "skipping generated_capture_step_runs_end_to_end: bash, sha256sum, or jq not available"
        );
        return Ok(());
    }
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let init_root = std::env::temp_dir().join(format!(
        "ripr-generated-capture-exec-init-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&init_root)?;
    let output = run_ripr_init(&init_root)?;
    assert!(
        output.status.success(),
        "ripr init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let workflow = fs::read_to_string(init_root.join(".github/workflows/ripr.yml"))?;
    let step = capture_pull_request_diff_step(&workflow)
        .ok_or("generated workflow has no 'Capture pull request diff' step")?;
    let body: Vec<String> = step
        .lines()
        .skip_while(|line| !line.trim_start_matches(' ').starts_with("run: |"))
        .skip(1)
        .map(|line| {
            line.strip_prefix("          ")
                .ok_or("capture run-block line lost its 10-space indent")
                .map(str::to_string)
        })
        .collect::<Result<Vec<String>, &str>>()
        .map_err(|err| err.to_string())?;
    assert!(
        !body.is_empty(),
        "capture step has no shell body to execute"
    );

    let repo = std::env::temp_dir().join(format!(
        "ripr-generated-capture-exec-repo-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&repo)?;
    fixture_git_ok(&repo, &["init", "--initial-branch=main"])?;
    fixture_git_ok(&repo, &["config", "--local", "user.name", "Capture Exec"])?;
    fixture_git_ok(
        &repo,
        &[
            "config",
            "--local",
            "user.email",
            "capture-exec@example.com",
        ],
    )?;
    fixture_git_ok(&repo, &["config", "--local", "commit.gpgsign", "false"])?;
    fs::write(repo.join("probe.txt"), "before\n")?;
    fixture_git_ok(&repo, &["add", "."])?;
    fixture_git_ok(&repo, &["commit", "--quiet", "-m", "base"])?;
    fixture_git_ok(&repo, &["checkout", "--quiet", "-b", "feature"])?;
    fs::write(repo.join("probe.txt"), "after\n")?;
    fixture_git_ok(&repo, &["add", "probe.txt"])?;
    fixture_git_ok(&repo, &["commit", "--quiet", "-m", "edit"])?;
    let base_sha = fixture_git_output(&repo, &["rev-parse", "--verify", "main^{commit}"])?;
    let head_sha = fixture_git_output(&repo, &["rev-parse", "--verify", "HEAD^{commit}"])?;

    // Positive: the step resolves main, captures the edit, and retains a
    // receipt whose identities and byte count match the run.
    let script = body
        .join("\n")
        .replace("origin/${{ github.base_ref }}", "main");
    let run = run_sh(&script, &repo)?;
    assert!(
        run.status.success(),
        "capture step failed: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    let patch = fs::read(repo.join("target/ripr/reports/pr.diff"))?;
    assert!(
        String::from_utf8_lossy(&patch).contains("after"),
        "captured patch must contain the feature-branch edit"
    );
    let receipt: serde_json::Value = serde_json::from_str(&fs::read_to_string(
        repo.join("target/ripr/reports/pr-diff.receipt.json"),
    )?)?;
    let field = |name: &str| {
        receipt
            .get(name)
            .ok_or(format!("receipt is missing field {name:?}"))
    };
    let as_str = |name: &str| {
        field(name)?
            .as_str()
            .ok_or(format!("receipt field {name:?} is not a string"))
    };
    assert_eq!(
        as_str("tool")?,
        "ripr",
        "receipt must identify its producer"
    );
    assert_eq!(
        as_str("kind")?,
        "pr-diff-receipt",
        "receipt must identify its kind"
    );
    assert_eq!(
        as_str("base_ref")?,
        "main",
        "receipt must retain the requested base ref"
    );
    assert_eq!(
        as_str("base_sha")?,
        base_sha.trim(),
        "receipt base_sha must equal the resolved base commit"
    );
    assert_eq!(
        as_str("head_sha")?,
        head_sha.trim(),
        "receipt head_sha must equal the resolved head commit"
    );
    assert_eq!(
        field("byte_count")?
            .as_u64()
            .ok_or("receipt field \"byte_count\" is not a number")?,
        patch.len() as u64,
        "receipt byte count must match the patch"
    );
    let expected_digest = {
        use sha2::{Digest, Sha256};
        Sha256::digest(&patch)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    };
    assert_eq!(
        as_str("sha256")?,
        expected_digest.as_str(),
        "receipt digest must match the patch bytes"
    );

    // Negative: an unresolvable base fails closed with the named error
    // instead of handing RIPR an absent patch.
    let missing = body
        .join("\n")
        .replace("origin/${{ github.base_ref }}", "nonexistent-base-branch");
    let run = run_sh(&missing, &repo)?;
    assert!(
        !run.status.success(),
        "capture step must fail when the base cannot be resolved"
    );
    assert!(
        String::from_utf8_lossy(&run.stderr).contains("cannot resolve base ref"),
        "missing base must name the failure; got:\n{}",
        String::from_utf8_lossy(&run.stderr)
    );

    // Empty range: base == HEAD is an honest zero-change run — the step
    // succeeds with an empty patch and a zero byte count (exercises the
    // mktemp zero-change proof path with zero NUL bytes).
    let empty = body
        .join("\n")
        .replace("origin/${{ github.base_ref }}", "feature");
    let run = run_sh(&empty, &repo)?;
    assert!(
        run.status.success(),
        "capture step must accept a real zero-change range; got:\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let empty_patch = fs::read(repo.join("target/ripr/reports/pr.diff"))?;
    assert!(
        empty_patch.is_empty(),
        "zero-change range must capture an empty patch"
    );
    let empty_receipt: serde_json::Value = serde_json::from_str(&fs::read_to_string(
        repo.join("target/ripr/reports/pr-diff.receipt.json"),
    )?)?;
    assert_eq!(
        empty_receipt
            .get("byte_count")
            .and_then(serde_json::Value::as_u64),
        Some(0),
        "zero-change receipt must record a zero byte count"
    );

    fs::remove_dir_all(init_root)?;
    fs::remove_dir_all(repo)?;
    Ok(())
}

/// One spawn site for the built-binary `ripr init` invocations below
/// (process-policy bound).
fn run_ripr_init(root: &std::path::Path) -> Result<std::process::Output, Box<dyn Error>> {
    Ok(Command::new(env!("CARGO_BIN_EXE_ripr"))
        .args(["init", "--root"])
        .arg(root)
        .args(["--ci", "github"])
        .output()?)
}

/// One spawn site for executing extracted capture-step shell (process-policy
/// bound). GitHub Actions runs a `run:` step without `shell:` as `bash -e`
/// on ubuntu-latest, so the helper mirrors that invocation instead of a
/// bare `sh -c`, which would not enable errexit. Unix-only like its single
/// caller, so Windows builds never see a dead helper.
#[cfg(unix)]
fn run_sh(script: &str, cwd: &std::path::Path) -> Result<std::process::Output, Box<dyn Error>> {
    Ok(Command::new("bash")
        .args(["-e", "-c", script])
        .current_dir(cwd)
        .output()?)
}

/// Extract the "Capture pull request diff" step block: from its `- name:`
/// line through (excluding) the next step's `- name:` line.
fn capture_pull_request_diff_step(text: &str) -> Option<String> {
    let marker = "- name: Capture pull request diff";
    let start = text.find(marker)?;
    let rest = &text[start..];
    let end = rest[marker.len()..]
        .find("\n      - name: ")
        .map(|offset| marker.len() + offset)
        .unwrap_or(rest.len());
    Some(rest[..end].trim_end().to_string())
}
