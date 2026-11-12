//! Native review-comments → gate → baseline parser-declined control.
#![cfg(feature = "lang-rust")]
use std::path::{Path, PathBuf};
use std::process::Command;

#[path = "common/mod.rs"]
mod common;
use common::fixture_git::{fixture_git_ok, fixture_git_output};

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn run(root: &Path, args: &[&str]) -> Result<(), String> {
    let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
        .current_dir(root)
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "ripr {args:?}: {:?}\n{}\n{}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}
fn read(root: &Path, name: &str) -> Result<serde_json::Value, String> {
    serde_json::from_slice(&std::fs::read(root.join(name)).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}
fn write(root: &Path, path: &str, contents: &str) -> Result<(), String> {
    let path = root.join(path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, contents).map_err(|e| e.to_string())
}
fn review_and_gate(root: &Path, base: &str, label: &str) -> Result<(), String> {
    let review = format!("{label}-review.json");
    let gate = format!("{label}-gate.json");
    let markdown = format!("{label}-gate.md");
    run(
        root,
        &[
            "review-comments",
            "--root",
            ".",
            "--base",
            base,
            "--head",
            "HEAD",
            "--out",
            &review,
        ],
    )?;
    run(
        root,
        &[
            "gate",
            "evaluate",
            "--root",
            ".",
            "--pr-guidance",
            &review,
            "--mode",
            "acknowledgeable",
            "--out",
            &gate,
            "--out-md",
            &markdown,
        ],
    )
}

fn run_status(root: &Path, args: &[&str]) -> Result<std::process::Output, String> {
    Command::new(env!("CARGO_BIN_EXE_ripr"))
        .current_dir(root)
        .args(args)
        .output()
        .map_err(|e| e.to_string())
}
fn init_fixture(root: &Path) -> Result<String, String> {
    fixture_git_ok(root, &["init", "-q", "-b", "main"])?;
    fixture_git_ok(root, &["config", "user.name", "fixture"])?;
    fixture_git_ok(root, &["config", "user.email", "fixture@example.invalid"])?;
    fixture_git_ok(root, &["config", "commit.gpgsign", "false"])?;
    write(
        root,
        "Cargo.toml",
        "[package]\nname=\"parser-proof\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    )?;
    write(
        root,
        "src/lib.rs",
        "pub fn early(n: i32) -> bool { n > 10 }\n",
    )?;
    write(
        root,
        "tests/boundary.rs",
        "#[test] fn boundary() { assert!(parser_proof::early(10)); }\n",
    )?;
    fixture_git_ok(root, &["add", "."])?;
    fixture_git_ok(root, &["commit", "-qm", "base"])?;
    Ok(fixture_git_output(root, &["rev-parse", "HEAD"])?
        .trim()
        .to_owned())
}
fn temp_fixture(label: &str) -> Result<PathBuf, String> {
    let root = std::env::temp_dir().join(format!(
        "ripr-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos()
    ));
    std::fs::create_dir(&root).map_err(|e| e.to_string())?;
    Ok(root)
}

/// #14485: `calibrated-gate` exists to stop a merge, so a changed file that
/// was never parsed (nothing judged for it) blocks there with exit 3, while
/// the same mode on a diff whose files all parsed is unchanged.
#[test]
fn calibrated_gate_blocks_a_changed_file_that_was_not_parsed() -> Result<(), String> {
    let root = temp_fixture("calibrated-declined")?;
    let _fixture = Fixture(root.clone());
    let base = init_fixture(&root)?;

    // Control: a parsed change with nothing to find is not blocked.
    write(&root, "src/lib.rs", "pub fn unrelated() {}\n")?;
    fixture_git_ok(&root, &["add", "src/lib.rs"])?;
    fixture_git_ok(&root, &["commit", "-qm", "parsed"])?;
    let gate_args = |review: &str, out: &'static str, md: &'static str| -> Vec<String> {
        [
            "gate",
            "evaluate",
            "--root",
            ".",
            "--pr-guidance",
            review,
            "--mode",
            "calibrated-gate",
            "--baseline",
            "baseline.json",
            "--out",
            out,
            "--out-md",
            md,
        ]
        .map(String::from)
        .to_vec()
    };
    run(
        &root,
        &[
            "review-comments",
            "--root",
            ".",
            "--base",
            &base,
            "--head",
            "HEAD",
            "--out",
            "parsed-review.json",
        ],
    )?;
    // calibrated-gate requires an explicit baseline artifact; an empty one
    // taken from the clean parsed diff keeps the declined-file result free of
    // baseline effects.
    review_and_gate(&root, &base, "parsed-ack")?;
    run(
        &root,
        &[
            "baseline",
            "create",
            "--from",
            "parsed-ack-gate.json",
            "--out",
            "baseline.json",
        ],
    )?;
    let parsed_args = gate_args("parsed-review.json", "parsed-gate.json", "parsed-gate.md");
    let parsed = run_status(
        &root,
        &parsed_args.iter().map(String::as_str).collect::<Vec<_>>(),
    )?;
    let parsed_gate = read(&root, "parsed-gate.json")?;
    assert_eq!(parsed.status.code(), Some(0), "{parsed_gate}");
    assert_eq!(parsed_gate["status"], "pass", "{parsed_gate}");

    // A file the parser declines: nothing is judged, so the mode blocks.
    let declined = format!(
        "pub fn early(n: i32) -> bool {{ n > 10 }}\npub fn deep() {{ {}{} }}\n",
        "{".repeat(400),
        "}".repeat(400)
    );
    write(&root, "src/lib.rs", &declined)?;
    fixture_git_ok(&root, &["add", "src/lib.rs"])?;
    fixture_git_ok(&root, &["commit", "-qm", "declined"])?;
    run(
        &root,
        &[
            "review-comments",
            "--root",
            ".",
            "--base",
            &base,
            "--head",
            "HEAD",
            "--out",
            "declined-review.json",
        ],
    )?;
    let declined_args = gate_args(
        "declined-review.json",
        "declined-gate.json",
        "declined-gate.md",
    );
    let blocked = run_status(
        &root,
        &declined_args.iter().map(String::as_str).collect::<Vec<_>>(),
    )?;
    let blocked_gate = read(&root, "declined-gate.json")?;
    assert_eq!(blocked_gate["summary"]["evaluated"], 0, "{blocked_gate}");
    assert_eq!(blocked_gate["status"], "blocked", "{blocked_gate}");
    assert_eq!(blocked.status.code(), Some(3), "{blocked_gate}");
    let warnings = blocked_gate["warnings"].to_string();
    assert!(
        warnings.contains("calibrated-gate blocks while a changed file was not parsed")
            && warnings.contains("parser_declined_files"),
        "{warnings}"
    );
    let markdown =
        std::fs::read_to_string(root.join("declined-gate.md")).map_err(|e| e.to_string())?;
    assert!(markdown.contains("Decision: blocked"), "{markdown}");
    Ok(())
}

#[test]
fn parser_decline_keeps_native_baseline_debt() -> Result<(), String> {
    let root = std::env::temp_dir().join(format!(
        "ripr-parser-declined-native-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos()
    ));
    std::fs::create_dir(&root).map_err(|e| e.to_string())?;
    let _fixture = Fixture(root.clone());
    fixture_git_ok(&root, &["init", "-q", "-b", "main"])?;
    fixture_git_ok(&root, &["config", "user.name", "fixture"])?;
    fixture_git_ok(&root, &["config", "user.email", "fixture@example.invalid"])?;
    fixture_git_ok(&root, &["config", "commit.gpgsign", "false"])?;
    write(
        &root,
        "Cargo.toml",
        "[package]\nname=\"parser-proof\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    )?;
    write(
        &root,
        "src/lib.rs",
        "pub fn early(n: i32) -> bool { n > 10 }\n",
    )?;
    write(
        &root,
        "tests/boundary.rs",
        "#[test] fn boundary() { assert!(parser_proof::early(10)); }\n",
    )?;
    fixture_git_ok(&root, &["add", "."])?;
    fixture_git_ok(&root, &["commit", "-qm", "base"])?;
    let base = fixture_git_output(&root, &["rev-parse", "HEAD"])?
        .trim()
        .to_owned();
    let shallow = "pub fn early(n: i32) -> bool { n >= 10 }\n";
    write(&root, "src/lib.rs", shallow)?;
    fixture_git_ok(&root, &["add", "src/lib.rs"])?;
    fixture_git_ok(&root, &["commit", "-qm", "shallow"])?;
    review_and_gate(&root, &base, "shallow")?;
    run(
        &root,
        &[
            "baseline",
            "create",
            "--from",
            "shallow-gate.json",
            "--out",
            "baseline.json",
        ],
    )?;
    let baseline = read(&root, "baseline.json")?;
    let identities = baseline["entries"]
        .as_array()
        .ok_or("missing baseline entries")?
        .iter()
        .map(|e| e["identity"].clone())
        .collect::<Vec<_>>();
    if identities.is_empty() {
        return Err("fixture captured no baseline debt".into());
    }
    write(&root, "src/lib.rs", "pub fn unrelated() {}\n")?;
    fixture_git_ok(&root, &["add", "src/lib.rs"])?;
    fixture_git_ok(&root, &["commit", "-qm", "fixed"])?;
    review_and_gate(&root, &base, "fixed")?;
    // #14082 control: every changed file parsed and nothing was found.
    let fixed_gate = read(&root, "fixed-gate.json")?;
    assert_eq!(fixed_gate["status"], "pass", "{fixed_gate}");
    assert_eq!(fixed_gate["warnings"], serde_json::json!([]));
    run(
        &root,
        &[
            "baseline",
            "diff",
            "--baseline",
            "baseline.json",
            "--current",
            "fixed-gate.json",
            "--out",
            "fixed-delta.json",
            "--out-md",
            "fixed-delta.md",
        ],
    )?;
    let fixed = read(&root, "fixed-delta.json")?;
    assert_eq!(
        fixed["delta"]["resolved"],
        identities.len() as u64,
        "{fixed}"
    );
    run(
        &root,
        &[
            "baseline",
            "update",
            "--baseline",
            "baseline.json",
            "--current",
            "fixed-gate.json",
            "--remove-resolved",
            "--out",
            "fixed-update.json",
        ],
    )?;
    assert_eq!(
        read(&root, "fixed-update.json")?["entries"]
            .as_array()
            .map(Vec::len),
        Some(0)
    );
    let declined = format!(
        "{shallow}pub fn deep() {{ {}{} }}\n",
        "{".repeat(400),
        "}".repeat(400)
    );
    write(&root, "src/lib.rs", &declined)?;
    fixture_git_ok(&root, &["add", "src/lib.rs"])?;
    fixture_git_ok(&root, &["commit", "-qm", "declined"])?;
    review_and_gate(&root, &base, "declined")?;
    // #14082: nothing evaluated because the file was not parsed is not `pass`.
    let declined_gate = read(&root, "declined-gate.json")?;
    assert_eq!(declined_gate["summary"]["evaluated"], 0, "{declined_gate}");
    assert_eq!(declined_gate["status"], "advisory", "{declined_gate}");
    assert!(
        declined_gate["warnings"][0]
            .as_str()
            .is_some_and(|w| w.contains("parser_declined_files") && w.contains("`src/lib.rs`")),
        "{declined_gate}"
    );
    let declined_markdown =
        std::fs::read_to_string(root.join("declined-gate.md")).map_err(|e| e.to_string())?;
    assert!(
        declined_markdown.contains("Decision: advisory")
            && declined_markdown.contains("## Warnings")
            && declined_markdown.contains("src/lib.rs"),
        "{declined_markdown}"
    );
    run(
        &root,
        &[
            "baseline",
            "diff",
            "--baseline",
            "baseline.json",
            "--current",
            "declined-gate.json",
            "--out",
            "declined-delta.json",
            "--out-md",
            "declined-delta.md",
        ],
    )?;
    let delta = read(&root, "declined-delta.json")?;
    assert_eq!(
        delta["analysis_scope"]["parser_declined_files"],
        serde_json::json!(["src/lib.rs"])
    );
    assert_eq!(delta["delta"]["resolved"], 0);
    assert_eq!(
        delta["delta"]["stale_baseline_entry"],
        identities.len() as u64
    );
    let items = delta["items"].as_array().ok_or("missing delta items")?;
    assert_eq!(items.len(), identities.len());
    assert!(items.iter().all(|item| {
        item["not_evaluated_reason"]
            .as_str()
            .is_some_and(|s| s.contains("not parsed"))
    }));
    run(
        &root,
        &[
            "baseline",
            "update",
            "--baseline",
            "baseline.json",
            "--current",
            "declined-gate.json",
            "--remove-resolved",
            "--out",
            "declined-update.json",
        ],
    )?;
    let updated = read(&root, "declined-update.json")?;
    let retained = updated["entries"]
        .as_array()
        .ok_or("missing retained entries")?
        .iter()
        .map(|e| e["identity"].clone())
        .collect::<Vec<_>>();
    assert_eq!(identities, retained);
    Ok(())
}

/// #14082: seams withheld as static limitations never become gate candidates,
/// so a decision that evaluated none of them must not read as `pass`.
#[test]
fn static_limited_seams_never_read_as_gate_pass() -> Result<(), String> {
    let root = std::env::temp_dir().join(format!(
        "ripr-static-limited-gate-native-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos()
    ));
    std::fs::create_dir(&root).map_err(|e| e.to_string())?;
    let _fixture = Fixture(root.clone());
    fixture_git_ok(&root, &["init", "-q", "-b", "main"])?;
    fixture_git_ok(&root, &["config", "user.name", "fixture"])?;
    fixture_git_ok(&root, &["config", "user.email", "fixture@example.invalid"])?;
    fixture_git_ok(&root, &["config", "commit.gpgsign", "false"])?;
    write(
        &root,
        "Cargo.toml",
        "[package]\nname=\"limited-proof\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    )?;
    let source = |operator: &str| {
        (0..4)
            .map(|i| {
                format!(
                    "pub fn f{i}(n: i32) -> bool {{ n {operator} {} }}\n",
                    i + 10
                )
            })
            .collect::<String>()
    };
    write(&root, "src/lib.rs", &source(">"))?;
    // Each owner is called, and nothing asserts on the result.
    write(
        &root,
        "tests/smoke.rs",
        &(0..4)
            .map(|i| format!("#[test] fn t{i}() {{ let _ = limited_proof::f{i}(1); }}\n"))
            .collect::<String>(),
    )?;
    fixture_git_ok(&root, &["add", "."])?;
    fixture_git_ok(&root, &["commit", "-qm", "base"])?;
    let base = fixture_git_output(&root, &["rev-parse", "HEAD"])?
        .trim()
        .to_owned();
    write(&root, "src/lib.rs", &source(">="))?;
    fixture_git_ok(&root, &["add", "src/lib.rs"])?;
    fixture_git_ok(&root, &["commit", "-qm", "boundaries"])?;
    review_and_gate(&root, &base, "limited")?;

    // The fixture must really be the withheld case: the producer dropped
    // every changed seam as a static limitation and offered no candidate.
    let review = read(&root, "limited-review.json")?;
    let withheld = review["analysis_scope"]["static_limited_omitted_seams"]
        .as_u64()
        .ok_or_else(|| format!("no static-limited omission disclosed: {review}"))?;
    assert!(withheld >= 4, "{review}");
    assert_eq!(
        review["analysis_scope"]["omitted_by_policy"], withheld,
        "{review}"
    );

    let gate = read(&root, "limited-gate.json")?;
    assert_eq!(gate["summary"]["evaluated"], 0, "{gate}");
    assert_eq!(gate["status"], "advisory", "{gate}");
    assert_eq!(
        gate["analysis_scope"]["static_limited_omitted_seams"], withheld,
        "{gate}"
    );
    let expected = format!("static_limited_omitted_seams: {withheld} seam(s)");
    assert!(
        gate["warnings"][0]
            .as_str()
            .is_some_and(|w| w.contains(&expected)),
        "{gate}"
    );
    let markdown =
        std::fs::read_to_string(root.join("limited-gate.md")).map_err(|e| e.to_string())?;
    assert!(
        markdown.contains("Decision: advisory") && markdown.contains("## Warnings"),
        "{markdown}"
    );
    Ok(())
}
