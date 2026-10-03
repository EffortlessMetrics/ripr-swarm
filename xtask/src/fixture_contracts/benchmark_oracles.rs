//! Expected-behavior validity for retained benchmark controls, not static
//! discrimination or judged-panel calibration. No external tests are executed.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use serde_json::Value;

use super::retained_files::{local_path, read_json, verify_file};

#[cfg(test)]
mod tests;

const PAIRS: &[(&str, &str, &str, bool)] = &[
    ("fixed_corrected", "fixed", "corrected", true),
    ("broken_corrected", "broken", "corrected", false),
    ("fixed_weak", "fixed", "weak", true),
    ("broken_weak", "broken", "weak", true),
    ("fixed_original", "fixed", "original", false),
    ("broken_original", "broken", "original", true),
];

#[derive(Default)]
pub(super) struct Summary {
    valid: usize,
    invalid: usize,
    unreviewed: usize,
    legacy: usize,
    rejected: usize,
    declared: Vec<String>,
}

impl Summary {
    pub(super) fn observe(&mut self, root: &Path, case: &Value, violations: &mut Vec<String>) {
        let id = case["id"].as_str().unwrap_or("unknown");
        let status = match validate_case(root, case) {
            Ok(status) => status,
            Err(error) => {
                violations.push(format!("Lane 1 semantic oracle case {id}: {error}"));
                "rejected"
            }
        };
        match status {
            "valid" => self.valid += 1,
            "invalid" => self.invalid += 1,
            "unreviewed" => self.unreviewed += 1,
            _ => self.rejected += 1,
        }
        if case.get("semantic_oracle").is_none() {
            self.legacy += 1;
        } else {
            self.declared.push(format!("{id}: {status}"));
        }
    }

    pub(super) fn disclosure(self) -> crate::PolicyDisclosure {
        let mut items = vec![format!(
            "Expected-behavior validity: valid={}, invalid={}, unreviewed={} (legacy absent={}), rejected={}",
            self.valid, self.invalid, self.unreviewed, self.legacy, self.rejected
        )];
        items.extend(self.declared.iter().take(20).cloned());
        if self.declared.len() > 20 {
            items.push(format!(
                "{} additional declarations remain in corpus.json",
                self.declared.len() - 20
            ));
        }
        crate::PolicyDisclosure {
            heading: "Benchmark semantic-oracle scope".to_string(),
            intro: "Missing legacy status means unreviewed. Valid/invalid declarations require retained independent semantic review and six exact native pairings. Invalid-oracle controls may pass this fixture contract. These labels do not change static discrimination, Lane 1 scorecards, judged-panel calibration or frozen denominators.".to_string(),
            items,
        }
    }
}

pub(super) fn unavailable_disclosure() -> crate::PolicyDisclosure {
    crate::PolicyDisclosure {
        heading: "Benchmark semantic-oracle scope".to_string(),
        intro: "NOT_ESTABLISHED: the benchmark corpus could not be read as cases; no semantic-oracle counts are available.".to_string(),
        items: Vec::new(),
    }
}

fn text<'a>(value: &'a Value, field: &str) -> Result<&'a str, String> {
    value[field]
        .as_str()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("semantic oracle is missing {field}"))
}

fn retained_text(root: &Path, entry: &Value) -> Result<String, String> {
    verify_file(root, entry)?;
    fs::read_to_string(root.join(text(entry, "path")?)).map_err(|error| error.to_string())
}

fn retained_json(root: &Path, entry: &Value) -> Result<Value, String> {
    verify_file(root, entry)?;
    read_json(&root.join(text(entry, "path")?))
}

/// Absence is explicitly unreviewed. A declared label is never its own proof.
pub(super) fn validate_case(root: &Path, case: &Value) -> Result<&'static str, String> {
    let Some(oracle) = case.get("semantic_oracle") else {
        return Ok("unreviewed");
    };
    let status = text(oracle, "status")?;
    match status {
        "unreviewed" => return Ok("unreviewed"),
        "valid" | "invalid" => {}
        _ => return Err(format!("unsupported semantic oracle status {status}")),
    }
    let expected_variant = if status == "valid" {
        "corrected"
    } else {
        "original"
    };
    if text(oracle, "variant")? != expected_variant {
        return Err("semantic oracle status contradicts its test variant".to_string());
    }
    let key = retained_json(root, &oracle["answer_key"])?;
    let pairing = retained_json(root, &oracle["native_pairing"])?;
    let case_id = text(case, "id")?;
    if text(&key, "case_id")? != case_id
        || text(&pairing, "case_id")? != case_id
        || pairing["answer_key_sha256"] != oracle["answer_key"]["sha256"]
    {
        return Err("semantic oracle has a stale or wrong case/answer-key identity".to_string());
    }
    validate_answer_key(root, &key)?;
    validate_pairing(root, &key, &pairing)?;
    Ok(if status == "valid" {
        "valid"
    } else {
        "invalid"
    })
}

fn validate_answer_key(root: &Path, key: &Value) -> Result<(), String> {
    for field in ["claim", "test_id", "package", "test_source_path"] {
        let _ = text(key, field)?;
    }
    if !local_path(text(key, "test_source_path")?) {
        return Err("semantic oracle test source must be a contained relative path".to_string());
    }
    let review = &key["independent_review"];
    if text(review, "disposition")? != "accepted" {
        return Err(
            "semantic oracle expected behavior has not been independently accepted".to_string(),
        );
    }
    let _ = text(review, "reviewer")?;
    let _ = text(review, "rationale")?;
    let basis = key["basis"]
        .as_array()
        .filter(|basis| !basis.is_empty())
        .ok_or_else(|| "semantic oracle needs independent semantic basis".to_string())?;
    for item in basis {
        if !text(item, "url")?.starts_with("https://") {
            return Err("semantic oracle basis must identify its source URL".to_string());
        }
        if retained_text(root, &item["artifact"])?.trim().is_empty() {
            return Err("semantic oracle basis artifact is empty".to_string());
        }
        for (group, names) in [
            ("sources", &["fixed", "broken"][..]),
            ("tests", &["corrected", "original", "weak"][..]),
        ] {
            if names
                .iter()
                .any(|name| item["artifact"]["sha256"] == key[group][*name]["sha256"])
            {
                return Err(
                    "semantic oracle basis cannot be the implementation or test under review"
                        .to_string(),
                );
            }
        }
    }
    let fixed = retained_text(root, &key["sources"]["fixed"])?;
    let broken = retained_text(root, &key["sources"]["broken"])?;
    if fixed == broken {
        return Err("semantic oracle fixed and broken sources must differ".to_string());
    }
    let corrected = retained_text(root, &key["tests"]["corrected"])?;
    let original = retained_text(root, &key["tests"]["original"])?;
    let weak = retained_text(root, &key["tests"]["weak"])?;
    let assertions = key["boundary_assertions"]
        .as_array()
        .filter(|assertions| assertions.len() == 2)
        .ok_or_else(|| "semantic oracle needs the two exact boundary assertions".to_string())?;
    let mut reduced = corrected.clone();
    let mut seen = BTreeSet::new();
    for assertion in assertions {
        let positive = text(assertion, "corrected")?;
        let negative = text(assertion, "original")?;
        if !seen.insert(positive)
            || !positive.starts_with("assert!(")
            || !positive.ends_with(");")
            || negative != positive.replacen("assert!(", "assert!(!", 1)
            || corrected
                .lines()
                .filter(|line| line.trim() == positive)
                .count()
                != 1
            || original
                .lines()
                .filter(|line| line.trim() == negative)
                .count()
                != 1
        {
            return Err(
                "semantic oracle boundary assertion identity or polarity differs".to_string(),
            );
        }
        reduced = reduced
            .split_inclusive('\n')
            .filter(|line| line.trim() != positive)
            .collect();
    }
    if reduced != weak {
        return Err(
            "semantic oracle weak test must remove only the two boundary assertions".to_string(),
        );
    }
    for (variant, source) in [
        ("broken_corrected", &corrected),
        ("fixed_original", &original),
    ] {
        let line = key["failure_lines"][variant]
            .as_u64()
            .and_then(|line| usize::try_from(line).ok())
            .filter(|line| *line > 0)
            .and_then(|line| source.lines().nth(line - 1))
            .map(str::trim);
        let field = if variant == "broken_corrected" {
            "corrected"
        } else {
            "original"
        };
        if !assertions
            .iter()
            .any(|assertion| assertion[field].as_str() == line)
        {
            return Err(format!(
                "{variant}: intended failure line is not a named boundary assertion"
            ));
        }
    }
    Ok(())
}

fn validate_pairing(root: &Path, key: &Value, pairing: &Value) -> Result<(), String> {
    verify_file(root, &pairing["lock"])?;
    let rows = pairing["observations"]
        .as_array()
        .filter(|rows| rows.len() == PAIRS.len())
        .ok_or_else(|| "semantic oracle needs all six actual native observations".to_string())?;
    let mut seen = BTreeSet::new();
    for row in rows {
        let variant = text(row, "variant")?;
        let (_, source, test, passes) = PAIRS
            .iter()
            .find(|(name, _, _, _)| *name == variant)
            .ok_or_else(|| format!("unknown semantic oracle pairing {variant}"))?;
        if !seen.insert(variant) {
            return Err(format!("duplicate semantic oracle pairing {variant}"));
        }
        for (field, expected) in [
            ("source_sha256", &key["sources"][*source]["sha256"]),
            ("test_sha256", &key["tests"][*test]["sha256"]),
            ("lock_sha256", &pairing["lock"]["sha256"]),
        ] {
            if &row[field] != expected || &row[format!("{field}_after")] != expected {
                return Err(format!("{variant}: stale {field} input fence"));
            }
        }
        validate_observation(root, key, row, *passes)?;
    }
    Ok(())
}

fn validate_observation(root: &Path, key: &Value, row: &Value, passes: bool) -> Result<(), String> {
    let variant = text(row, "variant")?;
    let test_id = text(key, "test_id")?;
    if row["phase"].as_str() != Some("completed")
        || row["timed_out"].as_bool() != Some(false)
        || row["compile_failed"].as_bool() != Some(false)
        || row["process_failed"].as_bool() != Some(false)
    {
        return Err(format!(
            "{variant}: unavailable or failed instrument is not native pairing"
        ));
    }
    let failures = u64::from(!passes);
    for (field, expected) in [
        ("intended", 1),
        ("discovered", 1),
        ("selected", 1),
        ("executed", 1),
        ("passed", u64::from(passes)),
        ("failed", failures),
        ("ignored", 0),
    ] {
        if row["counts"][field].as_u64() != Some(expected) {
            return Err(format!("{variant}: {field} must be {expected}"));
        }
    }
    if row["test_ids"] != serde_json::json!([test_id])
        || row["exit_code"].as_i64() != Some(if passes { 0 } else { 101 })
    {
        return Err(format!("{variant}: wrong native test identity or exit"));
    }
    let runner = &row["runner"];
    for field in [
        "version",
        "compiler_version",
        "cwd",
        "target_dir",
        "build_dir",
    ] {
        let _ = text(runner, field)?;
    }
    for field in ["artifact_sha256", "executable_sha256"] {
        let digest = text(runner, field)?;
        if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(format!("{variant}: missing exact native {field}"));
        }
    }
    let executable = text(runner, "executable")?;
    if !executable.starts_with('/') {
        return Err(format!(
            "{variant}: the retained Linux runner must use an absolute executable"
        ));
    }
    let expected_argv = serde_json::json!([
        executable,
        "test",
        "--locked",
        "--offline",
        "--manifest-path",
        "Cargo.toml",
        "-p",
        text(key, "package")?,
        "--lib",
        test_id,
        "--",
        "--exact"
    ]);
    if runner["argv"] != expected_argv {
        return Err(format!(
            "{variant}: wrong package, selector or managed command"
        ));
    }
    let stdout = retained_text(root, &row["stdout"])?;
    let _ = retained_text(root, &row["stderr"])?;
    let verdict = if passes { "ok" } else { "FAILED" };
    let result = format!(
        "test result: {verdict}. {} passed; {failures} failed; 0 ignored; 0 measured; ",
        u64::from(passes)
    );
    let test_row = format!("test {test_id} ... {verdict}");
    if stdout
        .lines()
        .filter(|line| *line == "running 1 test")
        .count()
        != 1
        || stdout.lines().filter(|line| *line == test_row).count() != 1
        || stdout
            .lines()
            .filter(|line| line.starts_with("test ") && !line.starts_with("test result:"))
            .count()
            != 1
        || stdout
            .lines()
            .filter(|line| line.starts_with("test result:"))
            .count()
            != 1
        || !stdout.lines().any(|line| {
            line.strip_prefix(&result)
                .and_then(|tail| tail.split_once(" filtered out; finished in "))
                .is_some_and(|(filtered, elapsed)| {
                    filtered.parse::<u64>().is_ok()
                        && elapsed.strip_suffix('s').is_some_and(|seconds| {
                            seconds
                                .parse::<f64>()
                                .is_ok_and(|seconds| seconds.is_finite() && seconds >= 0.0)
                        })
                })
        })
    {
        return Err(format!(
            "{variant}: native output does not establish the exact nonempty subject"
        ));
    }
    if !passes {
        let test = if variant == "broken_corrected" {
            "corrected"
        } else {
            "original"
        };
        let test_source = retained_text(root, &key["tests"][test])?;
        let line = key["failure_lines"][variant].as_u64().unwrap_or_default();
        let assertion = usize::try_from(line)
            .ok()
            .and_then(|line| line.checked_sub(1))
            .and_then(|line| test_source.lines().nth(line))
            .map(str::trim)
            .and_then(|line| line.strip_prefix("assert!("))
            .and_then(|line| line.strip_suffix(");"))
            .ok_or_else(|| format!("{variant}: intended assertion is unavailable"))?;
        let location = format!("{}:{line}:", text(key, "test_source_path")?);
        if !stdout.contains(&location)
            || !stdout.contains(&format!("assertion failed: {assertion}"))
            || stdout.matches("assertion failed:").count() != 1
        {
            return Err(format!(
                "{variant}: missing intended boundary assertion failure"
            ));
        }
    }
    Ok(())
}
