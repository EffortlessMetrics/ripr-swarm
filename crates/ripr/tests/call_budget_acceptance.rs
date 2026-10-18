//! Independent #8600 real-extractor oracle; no synthetic CallFact construction.
//! Run with --nocapture. Set RIPR_8600_REQUIRE_TYPED=1 for future acceptance.
//! Retains unique input/cache/raw-output directories for diagnosis; no cleanup.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Clone)]
struct Case {
    name: String,
    line: String,
    declaration_inline: bool,
    extra: String,
    slice: bool,
    expected_names: BTreeSet<String>,
    requested_names: usize,
    requires_limit: bool,
    independent_positive: bool,
}

fn names(line: &str, inline: bool) -> BTreeSet<String> {
    // Fixture grammar is deliberately tiny and independently enumerated.
    // Production facts sort names after retaining a scan-order prefix.
    let mut ordered = if inline {
        vec!["checks".to_string()]
    } else {
        vec![]
    };
    for statement in line.split(';') {
        let statement = statement.trim();
        let name = if statement.starts_with("let _ = weight(") {
            "weight"
        } else if statement.starts_with('h') {
            statement
                .split('(')
                .next()
                .expect("oracle setup or evidence shape")
        } else {
            continue;
        };
        if !ordered.iter().any(|old| old == name) {
            ordered.push(name.to_string());
        }
    }
    ordered.into_iter().take(256).collect()
}

fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for inline in [false, true] {
        for total in [255, 256, 257] {
            for first in [true, false] {
                // total includes the declaration when it shares this line.
                let fillers = total - 1 - usize::from(inline);
                let noise = (0..fillers).map(|i| format!("h{i}();")).collect::<String>();
                let line = if first {
                    format!("let _ = weight(2);{noise}")
                } else {
                    format!("{noise}let _ = weight(2);")
                };
                cases.push(Case {
                    name: format!(
                        "reach_{total}_{}_{}",
                        if first { "first" } else { "last" },
                        if inline { "inline" } else { "body" }
                    ),
                    expected_names: names(&line, inline),
                    line,
                    declaration_inline: inline,
                    extra: String::new(),
                    slice: false,
                    requested_names: total,
                    requires_limit: false,
                    independent_positive: false,
                });
            }
        }
    }
    let positive_line = (0..257).map(|i| format!("h{i}();")).collect::<String>();
    cases.push(Case {
        name: "numeric_independent_positive".into(),
        expected_names: names(&positive_line, false),
        line: positive_line,
        declaration_inline: false,
        extra: "let _ = weight(2);\n}\n#[test] fn positive() { assert_eq!(weight(2), 18);\n".into(),
        slice: false,
        requested_names: 257,
        requires_limit: false,
        independent_positive: true,
    });
    let line = format!("{}let _ = weight(2);", "h0();".repeat(2000));
    cases.push(Case {
        name: "repeated_name_dedup".into(),
        expected_names: names(&line, true),
        line,
        declaration_inline: true,
        extra: String::new(),
        slice: false,
        requested_names: 3,
        requires_limit: false,
        independent_positive: false,
    });
    let noise = (0..257).map(|i| format!("h{i}();")).collect::<String>();
    for (name, line, extra, requires_limit, positive) in [
        (
            "slice_complete",
            "let samples: [&[u64]; 2] = [&[5], &[6]]; for s in samples { let _ = weight(s); }"
                .to_string(),
            String::new(),
            false,
            false,
        ),
        (
            "slice_saturated_same_test",
            noise.clone(),
            "let samples: [&[u64]; 2] = [&[5], &[6]]; for s in samples { let _ = weight(s); }\n"
                .into(),
            true,
            false,
        ),
        (
            "slice_unrelated_line",
            "let samples: [&[u64]; 2] = [&[5], &[6]]; for s in samples { let _ = weight(s); }"
                .to_string(),
            format!("}}\n#[test] fn unrelated() {{\n{noise}\n"),
            false,
            false,
        ),
        (
            "slice_independent_positive",
            noise.clone(),
            "}\n#[test] fn positive() { assert_eq!(weight(&[]), 0);\n".into(),
            false,
            true,
        ),
    ] {
        // The positive case also keeps a dependent table call in checks.
        let extra = if positive {
            format!(
                "let samples: [&[u64]; 2] = [&[5], &[6]]; for s in samples {{ let _ = weight(s); }}\n{extra}"
            )
        } else {
            extra
        };
        cases.push(Case {
            name: name.into(),
            expected_names: if line.starts_with("let samples") {
                BTreeSet::from(["weight".into()])
            } else {
                names(&line, false)
            },
            line,
            declaration_inline: false,
            extra,
            slice: true,
            requested_names: if requires_limit || positive { 257 } else { 1 },
            requires_limit,
            independent_positive: positive,
        });
    }
    // Output boundary twins use a distinct-name noise line and a separate
    // table call. Even the 255 twin may be unknown for the independent eager-
    // table rule; only >=256 is eligible for the future budget reason.
    let template = cases
        .iter()
        .find(|c| c.name == "slice_saturated_same_test")
        .expect("oracle setup or evidence shape")
        .clone();
    for total in [255, 256] {
        let mut case = template.clone();
        case.name = format!("slice_noise_{total}");
        case.line = (0..total).map(|i| format!("h{i}();")).collect();
        case.expected_names = names(&case.line, false);
        case.requested_names = total;
        case.requires_limit = total >= 256;
        cases.push(case);
    }
    cases
}

fn source(case: &Case) -> String {
    let owner = if case.slice {
        "pub fn weight(samples: &[u64]) -> u64 {\n    if samples.len() == 0 { return 0; }\n    samples[0]\n}\n"
    } else {
        "pub fn weight(base: u32) -> u32 {\n    9 * base\n}\n"
    };
    let helpers = (0..257)
        .map(|i| format!("fn h{i}() {{}}\n"))
        .collect::<String>();
    format!(
        "{owner}{helpers}#[cfg(test)] mod tests {{\nuse super::*;\n#[test]\nfn checks() {{{}{}\n{}\n}}\n}}\n",
        if case.declaration_inline { "" } else { "\n" },
        case.line,
        case.extra
    )
}

fn run(binary: &Path, root: &Path, cache: &Path, format: &str, label: &str) -> Value {
    let mut command = Command::new(binary);
    command
        .args(["check", "--root"])
        .arg(root)
        .args(["--mode", "ready", "--format", format, "--quiet"])
        .env("RIPR_CACHE_DIR", cache);
    if format == "json" {
        command.arg("--diff").arg(root.join("diff.patch"));
    }
    let output = command.output().expect("CLI must execute");
    fs::write(root.join(format!("{label}.stdout.json")), &output.stdout)
        .expect("oracle setup or evidence shape");
    fs::write(root.join(format!("{label}.stderr.txt")), &output.stderr)
        .expect("oracle setup or evidence shape");
    assert!(
        matches!(output.status.code(), Some(0 | 1)),
        "instrument: {label}: {:?}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("recognized nonempty CLI JSON")
}

fn json_files(root: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(root).expect("production cache exists") {
        let path = entry.expect("oracle setup or evidence shape").path();
        if path.is_dir() {
            json_files(&path, files);
        } else if path.extension().is_some_and(|ext| ext == "json") {
            files.push(path);
        }
    }
}

#[test]
fn real_extraction_to_diff_and_repo_output() {
    let base = std::env::var_os("RIPR_8600_EVIDENCE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!(
                "ripr-8600-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("oracle setup or evidence shape")
                    .as_nanos()
            ))
        });
    assert!(
        !base.exists(),
        "use a fresh evidence destination: {}",
        base.display()
    );
    fs::create_dir_all(&base).expect("oracle setup or evidence shape");
    let binary = Path::new(env!("CARGO_BIN_EXE_ripr"));
    let mut rows = Vec::new();
    for case in cases() {
        let root = base.join(&case.name);
        fs::create_dir_all(root.join("src")).expect("oracle setup or evidence shape");
        let source = source(&case);
        fs::write(root.join("src/lib.rs"), &source).expect("oracle setup or evidence shape");
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname=\"budget-oracle\"\nversion=\"0.0.0\"\nedition=\"2024\"\n[workspace]\n",
        )
        .expect("oracle setup or evidence shape");
        let (old, new) = if case.slice {
            (
                "    if samples.is_empty() { return 0; }",
                "    if samples.len() == 0 { return 0; }",
            )
        } else {
            ("    base * 9", "    9 * base")
        };
        fs::write(root.join("diff.patch"), format!("diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -2,1 +2,1 @@\n-{old}\n+{new}\n")).expect("oracle setup or evidence shape");
        let mut outputs = Vec::new();
        for (format, scope) in [("json", "diff"), ("repo-exposure-json", "repo")] {
            let cache = root.join(format!("{scope}-cache"));
            assert!(!cache.exists());
            for temperature in ["cold", "warm"] {
                outputs.push(run(
                    binary,
                    &root,
                    &cache,
                    format,
                    &format!("{scope}-{temperature}"),
                ));
            }
        }
        let mut files = Vec::new();
        json_files(&root.join("repo-cache/repo-file-facts"), &mut files);
        let facts: Vec<_> = files
            .iter()
            .filter_map(|p| {
                let value: Value =
                    serde_json::from_slice(&fs::read(p).expect("oracle setup or evidence shape"))
                        .expect("oracle setup or evidence shape");
                (value["file_facts"]["source"] == source).then_some(value["file_facts"].clone())
            })
            .collect();
        assert_eq!(facts.len(), 1, "{}: exact source cache witness", case.name);
        assert_eq!(facts[0]["used_lexical_fallback"], false);
        let mut diff_files = Vec::new();
        json_files(&root.join("diff-cache/repo-file-facts"), &mut diff_files);
        let diff_facts: Vec<_> = diff_files
            .iter()
            .filter_map(|p| {
                let value: Value = serde_json::from_slice(&fs::read(p).expect("diff cache bytes"))
                    .expect("diff cache JSON");
                (value["file_facts"]["source"] == source).then_some(value["file_facts"].clone())
            })
            .collect();
        assert_eq!(
            diff_facts, facts,
            "both production paths extracted identical exact-source facts"
        );
        let tests = facts[0]["tests"]
            .as_array()
            .expect("oracle setup or evidence shape");
        let test = tests
            .iter()
            .find(|t| t["name"] == "checks")
            .expect("real test extracted");
        let line_number = source
            .lines()
            .position(|line| {
                if case.declaration_inline {
                    line.starts_with("fn checks() {")
                } else {
                    line == case.line
                }
            })
            .expect("oracle setup or evidence shape")
            + 1;
        let retained: BTreeSet<_> = test["calls"]
            .as_array()
            .expect("oracle setup or evidence shape")
            .iter()
            .filter(|c| c["line"] == line_number)
            .map(|c| {
                c["name"]
                    .as_str()
                    .expect("oracle setup or evidence shape")
                    .to_string()
            })
            .collect();
        assert_eq!(
            retained, case.expected_names,
            "{} actual extraction prefix",
            case.name
        );
        assert!(
            test["calls"]
                .as_array()
                .expect("oracle setup or evidence shape")
                .iter()
                .any(|c| c["name"] == "checks"),
            "declaration name must be accounted for"
        );
        let findings: Vec<_> = outputs[0]["findings"]
            .as_array()
            .expect("oracle setup or evidence shape")
            .iter()
            .filter(|f| f["probe"]["line"] == 2)
            .collect();
        assert_eq!(findings.len(), 1, "{} intended probe observed", case.name);
        let finding = findings[0];
        assert_eq!(
            outputs[0]["findings"], outputs[1]["findings"],
            "diff cold/warm parity"
        );
        assert_eq!(outputs[0]["analysis_outcome"]["analysis_complete"], true);
        let seams: Vec<_> = outputs[2]["seams"]
            .as_array()
            .expect("oracle setup or evidence shape")
            .iter()
            .filter(|s| {
                s["line"] == 2
                    && s["kind"]
                        == if case.slice {
                            "predicate_boundary"
                        } else {
                            "return_value"
                        }
            })
            .collect();
        assert_eq!(seams.len(), 1, "repo target seam observed");
        assert_eq!(
            outputs[2]["seams"], outputs[3]["seams"],
            "repo cold/warm parity"
        );
        assert_eq!(outputs[2]["run_status"], "complete");
        if case.independent_positive && !case.slice {
            assert_eq!(
                finding["classification"], "exposed",
                "independent complete positive preserved"
            );
            assert!(finding.get("static_limit_kind").is_none_or(Value::is_null));
        }
        if case.name == "slice_complete" || case.name == "slice_unrelated_line" {
            assert!(
                finding["activation"]["missing_discriminators"]
                    .as_array()
                    .expect("oracle setup or evidence shape")
                    .iter()
                    .any(|v| v["value"] == "samples.len() == 0")
            );
        }
        if case.requested_names < 256 {
            assert_ne!(
                finding["static_limit_kind"], "rust_call_fact_budget",
                "unrelated saturation must not leak a budget reason"
            );
        }
        if case.requires_limit {
            assert_eq!(
                seams[0]["evidence_record"]["canonical_item"]["canonical_item_kind"],
                "limitation"
            );
            assert_eq!(
                seams[0]["evidence_record"]["canonical_item"]["repair_command"],
                Value::Null
            );
            assert_ne!(seams[0]["evidence_record"]["repair_packet_ready"], true);
            assert!(matches!(
                finding["classification"].as_str(),
                Some("infection_unknown" | "static_unknown")
            ));
            assert!(
                finding["activation"]["missing_discriminators"]
                    .as_array()
                    .expect("oracle setup or evidence shape")
                    .is_empty()
            );
        }
        rows.push(json!({"case":case.name,"source_sha256":format!("{:x}", Sha256::digest(source.as_bytes())),"retained_names":retained,"requested_names_on_line":case.requested_names,
            "retained_names_on_line":retained.len(),"owner_retained_on_line":retained.contains("weight"),
            "diff_class":finding["classification"],"static_limit_kind":finding["static_limit_kind"],
            "diff_reach":finding["ripr"]["reach"],"repo_related_tests":seams[0]["related_tests"],"repo_static_limitations":seams[0]["evidence_record"]["static_limitations"],"repo_canonical_item":seams[0]["evidence_record"]["canonical_item"],"repo_grip":seams[0]["grip_class"],"repo_evidence":seams[0]["evidence"],
            "requires_future_limit":case.requires_limit,"independent_positive":case.independent_positive,"canonical_gap_id":finding["canonical_gap_id"],
            "missing_discriminators":finding["activation"]["missing_discriminators"]}));
    }
    fs::write(
        base.join("observations.json"),
        serde_json::to_vec_pretty(&rows).expect("oracle setup or evidence shape"),
    )
    .expect("oracle setup or evidence shape");
    println!("#8600 evidence: {}", base.display());
    for row in &rows {
        println!("{row}");
    }
    // Explicit opt-in acceptance, deliberately red on frozen current source.
    // Never describe the characterization run as typed-output acceptance.
    if std::env::var_os("RIPR_8600_REQUIRE_TYPED").is_some() {
        let failures: Vec<_> = rows
            .iter()
            .filter(|r| {
                r["requires_future_limit"] == true
                    && (r["static_limit_kind"] != "rust_call_fact_budget"
                        || !r["repo_static_limitations"]
                            .to_string()
                            .contains("rust_call_fact_budget"))
            })
            .map(|r| r["case"].clone())
            .collect();
        let lost_positive: Vec<_> = rows
            .iter()
            .filter(|r| r["independent_positive"] == true && r["diff_class"] != "exposed")
            .map(|r| r["case"].clone())
            .collect();
        assert!(
            failures.is_empty() && lost_positive.is_empty(),
            "future typed-output acceptance missing: {failures:?}; positive preservation missing: {lost_positive:?}"
        );
    }
}
