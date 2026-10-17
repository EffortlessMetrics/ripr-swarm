//! RIPR-SPEC-0147 / #8424: a legal Unix source name rejected by the portable
//! limitation-path DTO must retain the parse failure, not disable the adapter.
#![cfg(all(
    unix,
    feature = "lang-rust",
    feature = "lang-python",
    feature = "lang-typescript"
))]

use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

struct Language {
    name: &'static str,
    extension: &'static str,
    header: &'static str,
    old_line: &'static str,
    new_line: &'static str,
    footer: &'static str,
    broken: &'static str,
    limitation: &'static str,
    recovery: &'static str,
}

const LANGUAGES: [Language; 3] = [
    Language {
        name: "rust",
        extension: "rs",
        header: "pub fn calc(amount: u32, threshold: u32) -> bool {",
        old_line: "    amount > threshold",
        new_line: "    amount >= threshold",
        footer: "}",
        broken: "pub fn broken( {\n",
        limitation: "producer_failure",
        recovery: "inspect_failure",
    },
    Language {
        name: "python",
        extension: "py",
        header: "def calc(amount, threshold):",
        old_line: "    return amount > threshold",
        new_line: "    return amount >= threshold",
        footer: "",
        broken: "def broken(:\n",
        limitation: "producer_failure",
        recovery: "inspect_failure",
    },
    Language {
        name: "typescript",
        extension: "ts",
        header: "export function calc(amount: number, threshold: number): boolean {",
        old_line: "    return amount > threshold;",
        new_line: "    return amount >= threshold;",
        footer: "}",
        broken: "export function broken( {\n",
        limitation: "language_scope_unsupported",
        recovery: "retry",
    },
];

struct Fixture(PathBuf);

impl Fixture {
    fn create(language: &Language, relative: &str, broken: bool) -> TestResult<Self> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let parent = std::env::temp_dir();
        fs::create_dir_all(&parent)?;
        let root = parent.join(format!(
            "ripr-parse-path-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root)?;
        let fixture = Self(root);
        let source = fixture.0.join(relative);
        fs::create_dir_all(source.parent().ok_or("source parent missing")?)?;
        fs::write(
            &source,
            format!(
                "{}\n{}\n{}\n{}",
                language.header,
                language.new_line,
                language.footer,
                if broken { language.broken } else { "" },
            ),
        )?;
        fs::write(
            fixture.0.join("ripr.toml"),
            format!("[languages]\nenabled = [\"{}\"]\n", language.name),
        )?;
        match language.name {
            "rust" => fs::write(
                fixture.0.join("Cargo.toml"),
                format!(
                    "[package]\nname = \"parse_path\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[lib]\npath = \"{relative}\"\n"
                ),
            )?,
            "python" => fs::write(
                fixture.0.join("pyproject.toml"),
                "[project]\nname = \"parse-path\"\nversion = \"0.1.0\"\n",
            )?,
            _ => fs::write(
                fixture.0.join("package.json"),
                "{\"name\":\"parse-path\",\"devDependencies\":{\"vitest\":\"^2.0.0\"}}\n",
            )?,
        }
        // Only the changed boundary is in the supplied diff. The appended
        // malformed declaration in the live source must still be disclosed.
        fs::write(
            fixture.0.join("change.diff"),
            format!(
                "diff --git a/{relative} b/{relative}\n--- a/{relative}\n+++ b/{relative}\n@@ -2 +2 @@\n-{}\n+{}\n",
                language.old_line, language.new_line,
            ),
        )?;
        Ok(fixture)
    }

    fn check(&self) -> TestResult<(Output, Value)> {
        let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
            .args(["check", "--json", "--quiet", "--root"])
            .arg(&self.0)
            .arg("--diff")
            .arg(self.0.join("change.diff"))
            .output()?;
        let json = serde_json::from_slice(&output.stdout).map_err(|error| {
            format!(
                "normal check JSON missing: {error}; status={}; stderr={}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            )
        })?;
        Ok((output, json))
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn syntax_errors_keep_partial_json_for_all_path_spellings() -> TestResult {
    for language in &LANGUAGES {
        for prefix in ["src/", "C:", "src/C:"] {
            let relative = format!("{prefix}calc.{}", language.extension);
            let fixture = Fixture::create(language, &relative, true)?;
            let (output, json) = fixture.check()?;
            let label = format!("{} / {relative}", language.name);
            let outcome = &json["analysis_outcome"]["outcome"];
            assert_eq!(
                json["analysis_outcome"]["analysis_complete"], false,
                "{label}"
            );
            assert_eq!(outcome["kind"], "partial_with_limitations", "{label}");
            assert_eq!(outcome["counts"]["changed_file_count"], 1, "{label}");
            let limitations = outcome["limitations"].as_array().ok_or("no limitations")?;
            assert_eq!(limitations.len(), 1, "{label}: {outcome}");
            let limitation = &limitations[0];
            assert_eq!(
                limitation["kind"], language.limitation,
                "{label}: {outcome}"
            );
            assert_eq!(limitation["producer_stage"], "language_adapter", "{label}");
            assert_eq!(limitation["recovery"]["kind"], language.recovery, "{label}");
            let recovery = limitation["recovery"]["detail"]
                .as_str()
                .ok_or("no recovery detail")?;
            assert!(
                recovery.contains("Fix") && recovery.contains("parse"),
                "{label}: {recovery}"
            );
            assert_eq!(
                limitation["affected_items"],
                if language.name == "rust" {
                    Value::Null
                } else {
                    Value::from(1)
                },
                "{label}"
            );
            let detail = limitation["bounded_detail"].as_str().ok_or("no detail")?;
            assert!(detail.contains("parser"), "{label}: {detail}");
            if prefix == "C:" {
                assert!(limitation["path"].is_null(), "{label}: {limitation}");
                assert!(detail.contains(&relative), "{label}: {detail}");
                assert!(detail.chars().count() <= 512, "{label}: {detail}");
            } else {
                assert_eq!(limitation["path"], relative, "{label}");
            }
            let findings = json["findings"].as_array().ok_or("no findings array")?;
            assert!(
                findings.iter().all(|finding| finding["classification"]
                    .as_str()
                    .is_some_and(|class| class != "exposed")),
                "{label}"
            );
            if language.name == "typescript" {
                assert_eq!(findings.len(), 1, "{label}");
                let finding = &findings[0];
                assert_eq!(finding["classification"], "static_unknown", "{label}");
                assert_eq!(
                    finding["static_limit_kind"], "unsupported_syntax",
                    "{label}"
                );
                assert_eq!(finding["probe"]["file"], relative, "{label}");
                assert_eq!(finding["probe"]["line"], 2, "{label}");
                assert_eq!(finding["confidence"], serde_json::json!(0.2), "{label}");
                assert_eq!(
                    finding["preview_actionability"]["repair_packet_ready"], false,
                    "{label}"
                );
                assert_eq!(
                    finding["typescript_preview_card"]["repair_packet_ready"], false,
                    "{label}"
                );
                assert_eq!(
                    json["summary"]["changed_files_by_language"],
                    serde_json::json!([{"language":"typescript","files":1}]),
                    "{label}"
                );
                assert!(json.get("language_runs").is_none(), "{label}: {json}");
                assert_eq!(json["preview_languages"][0]["enabled"], true, "{label}");
                assert_eq!(json["preview_languages"][0]["analyzed"], true, "{label}");
                for stage in ["reach", "infect", "propagate", "observe", "discriminate"] {
                    assert_eq!(
                        finding["ripr"][stage]["state"], "unknown",
                        "{label}/{stage}"
                    );
                    assert_eq!(
                        finding["ripr"][stage]["confidence"], "low",
                        "{label}/{stage}"
                    );
                }
            }
            assert_eq!(
                output.status.code(),
                Some(0),
                "{label}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
    Ok(())
}

#[test]
fn long_nonportable_source_path_keeps_bounded_parser_cause() -> TestResult {
    let relative = format!(
        "C:{}/{}/{}/calc.ts",
        "a".repeat(200),
        "b".repeat(200),
        "c".repeat(200)
    );
    let fixture = Fixture::create(&LANGUAGES[2], &relative, true)?;
    let (output, json) = fixture.check()?;
    let outcome = &json["analysis_outcome"]["outcome"];
    assert_eq!(outcome["kind"], "partial_with_limitations");
    let limitation = &outcome["limitations"][0];
    assert_eq!(limitation["kind"], "language_scope_unsupported");
    assert!(limitation["path"].is_null());
    let detail = limitation["bounded_detail"].as_str().ok_or("no detail")?;
    assert_eq!(detail.chars().count(), 512);
    assert!(detail.starts_with("1 parser error(s):"), "{detail}");
    assert!(detail.contains("; source: C:"), "{detail}");
    assert_eq!(json["findings"][0]["classification"], "static_unknown");
    assert_eq!(json["findings"][0]["probe"]["file"], relative);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

#[test]
fn valid_files_do_not_acquire_parse_limitations() -> TestResult {
    for language in &LANGUAGES {
        for prefix in ["src/", "C:", "src/C:"] {
            let relative = format!("{prefix}calc.{}", language.extension);
            let fixture = Fixture::create(language, &relative, false)?;
            let (output, json) = fixture.check()?;
            let label = format!("{} / {relative}", language.name);
            assert_eq!(
                json["analysis_outcome"]["analysis_complete"], true,
                "{label}"
            );
            let outcome = &json["analysis_outcome"]["outcome"];
            assert_eq!(outcome["kind"], "complete_with_findings", "{label}");
            assert_eq!(outcome["counts"]["changed_file_count"], 1, "{label}");
            assert_eq!(outcome["limitations"], serde_json::json!([]), "{label}");
            assert!(
                !json["findings"]
                    .as_array()
                    .ok_or("no findings array")?
                    .is_empty(),
                "{label}"
            );
            assert_eq!(
                output.status.code(),
                Some(0),
                "{label}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
    Ok(())
}
