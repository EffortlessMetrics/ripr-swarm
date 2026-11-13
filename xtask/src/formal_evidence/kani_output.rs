//! Pure Kani output interpretation shared by repository tooling.
//! This #14472 precursor preserves the existing lane behavior; native observation
//! capture and retained-byte replay remain separate work under #14545.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Harness {
    pub file: String,
    pub name: String,
}

/// Finds `#[kani::proof]` harnesses in `(path, source)` pairs: the attribute
/// line followed (after any further attributes or doc comments) by `fn name`.
pub fn discover_harnesses(files: &[(String, String)]) -> Vec<Harness> {
    let mut found = Vec::new();
    for (file, text) in files {
        let mut pending = false;
        let mut in_raw_string = false;
        for line in text.lines() {
            let trimmed = line.trim();
            // Text inside a `r#"..."#` literal is fixture data (for example the
            // test-style fixtures that mention `#[kani::proof]`), not a harness.
            if in_raw_string {
                if trimmed.contains("\"#") {
                    in_raw_string = false;
                }
                continue;
            }
            if let Some(open) = trimmed.find("r#\"")
                && !trimmed[open + 3..].contains("\"#")
            {
                in_raw_string = true;
                pending = false;
                continue;
            }
            if trimmed.starts_with("#[kani::proof") {
                pending = true;
                continue;
            }
            if !pending {
                continue;
            }
            if trimmed.starts_with("#[") || trimmed.starts_with("//") || trimmed.is_empty() {
                continue;
            }
            pending = false;
            if let Some(name) = function_name(trimmed) {
                found.push(Harness {
                    file: file.clone(),
                    name,
                });
            }
        }
    }
    found.sort_by(|left, right| (&left.file, &left.name).cmp(&(&right.file, &right.name)));
    found
}

fn function_name(line: &str) -> Option<String> {
    let rest = line.split("fn ").nth(1)?;
    let name: String = rest
        .chars()
        .take_while(|ch| ch.is_alphanumeric() || *ch == '_')
        .collect();
    if name.is_empty() { None } else { Some(name) }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HarnessStatus {
    Verified,
    Failed(String),
    NotEstablished(String),
}

impl HarnessStatus {
    fn label(&self) -> &'static str {
        match self {
            Self::Verified => "VERIFIED",
            Self::Failed(_) => "FAILED",
            Self::NotEstablished(_) => "NOT-ESTABLISHED",
        }
    }

    fn detail(&self) -> &str {
        match self {
            Self::Verified => "within the harness bounds",
            Self::Failed(detail) | Self::NotEstablished(detail) => detail,
        }
    }
}

/// Classifies one harness result block from Kani's output.
/// Only an explicit success verdict with every `cover!` satisfied is
/// `Verified`; a missing verdict is never a pass.
pub fn classify_output(output: &str) -> HarnessStatus {
    // Kani prints `VERIFICATION:- FAILED` for a solver timeout too; that is no
    // counterexample, so it is checked first and stays `NOT-ESTABLISHED`.
    if output.contains("CBMC timed out") {
        return HarnessStatus::NotEstablished(
            "CBMC exceeded the per-harness timeout; no verdict".to_string(),
        );
    }
    if output.contains("VERIFICATION:- FAILED") {
        return HarnessStatus::Failed("verification failed (counterexample)".to_string());
    }
    if output.contains("No proof harnesses") {
        return HarnessStatus::NotEstablished(
            "kani found no proof harness with this name".to_string(),
        );
    }
    if output.contains("VERIFICATION:- SUCCESSFUL") {
        if output.contains("Status: UNSATISFIABLE") || cover_shortfall(output) {
            return HarnessStatus::Failed(
                "a cover! property is unreachable, so the proof is vacuous".to_string(),
            );
        }
        return HarnessStatus::Verified;
    }
    HarnessStatus::NotEstablished(
        "no successful verification verdict in the kani output".to_string(),
    )
}

/// True when kani's summary reports fewer satisfied covers than declared
/// (`** 1 of 2 cover properties satisfied`).
fn cover_shortfall(output: &str) -> bool {
    output.lines().any(|line| {
        let Some((head, _)) = line.split_once(" cover properties satisfied") else {
            return false;
        };
        let words: Vec<&str> = head.split_whitespace().collect();
        let Some(split) = words.iter().position(|word| *word == "of") else {
            return false;
        };
        let satisfied = split
            .checked_sub(1)
            .and_then(|index| words.get(index))
            .and_then(|word| word.parse::<u32>().ok());
        let declared = words
            .get(split + 1)
            .and_then(|word| word.parse::<u32>().ok());
        matches!((satisfied, declared), (Some(done), Some(total)) if done < total)
    })
}

pub struct HarnessResult {
    pub harness: Harness,
    pub status: HarnessStatus,
    pub time: String,
}

/// Whether the lane as a whole is acceptable.
pub fn lane_verdict(
    results: &[HarnessResult],
    tool_available: bool,
    allow_empty: bool,
) -> Result<(), String> {
    if !tool_available {
        return Err("NOT-ESTABLISHED: cargo-kani is not installed or not runnable".to_string());
    }
    if results.is_empty() {
        return if allow_empty {
            Ok(())
        } else {
            Err(
                "NOT-ESTABLISHED: zero kani harnesses found (pass --allow-empty to accept)"
                    .to_string(),
            )
        };
    }
    let failed = results
        .iter()
        .filter(|result| matches!(result.status, HarnessStatus::Failed(_)))
        .count();
    let unestablished = results
        .iter()
        .filter(|result| matches!(result.status, HarnessStatus::NotEstablished(_)))
        .count();
    if failed > 0 || unestablished > 0 {
        return Err(format!(
            "kani lane not clean: {failed} failed, {unestablished} not established of {} harnesses",
            results.len()
        ));
    }
    Ok(())
}

pub fn render_report(results: &[HarnessResult], note: &str) -> String {
    let mut out = String::from("# Kani lane (RIPR-SPEC-0371)\n\n");
    out.push_str("Bounded model checking of pure cores. A VERIFIED row means no violation within the harness bounds, not a full-program or runtime-mutation claim.\n\n");
    if !note.is_empty() {
        out.push_str(&format!("{note}\n\n"));
    }
    out.push_str(
        "| Harness | File | Status | Detail | Kani time |\n| --- | --- | --- | --- | ---: |\n",
    );
    for result in results {
        out.push_str(&format!(
            "| `{}` | `{}` | {} | {} | {} |\n",
            result.harness.name,
            result.harness.file,
            result.status.label(),
            result.status.detail(),
            result.time
        ));
    }
    out
}

/// Splits Kani's output into `(qualified harness name, block text)` pairs at
/// each `Checking harness <name>...` header.
pub fn split_harness_blocks(output: &str) -> Vec<(String, String)> {
    let mut blocks: Vec<(String, String)> = Vec::new();
    for line in output.lines() {
        if let Some(rest) = line.strip_prefix("Checking harness ") {
            let name = rest.trim_end().trim_end_matches("...").to_string();
            blocks.push((name, String::new()));
        } else if let Some((_, text)) = blocks.last_mut() {
            text.push_str(line);
            text.push('\n');
        }
    }
    blocks
}

fn verification_time(block: &str) -> String {
    block
        .lines()
        .find_map(|line| line.trim().strip_prefix("Verification Time:"))
        .map_or_else(|| "n/a".to_string(), |time| time.trim().to_string())
}

fn matches_harness(qualified: &str, harness: &Harness) -> bool {
    qualified == harness.name || qualified.ends_with(&format!("::{}", harness.name))
}

/// Joins discovered harnesses to Kani's result blocks. A missing or ambiguous
/// name-based relationship is `NOT-ESTABLISHED`, never assumed verified.
/// Source paths do not establish native module identity.
pub fn results_from_output(harnesses: &[Harness], output: &str) -> Vec<HarnessResult> {
    let blocks = split_harness_blocks(output);
    harnesses
        .iter()
        .map(|harness| {
            let duplicate_name = harnesses
                .iter()
                .filter(|other| other.name == harness.name)
                .nth(1)
                .is_some();
            let mut matching = blocks
                .iter()
                .filter(|(name, _)| matches_harness(name, harness));
            let first = matching.next();
            let (status, time) = if duplicate_name {
                (
                    HarnessStatus::NotEstablished(
                        "multiple discovered harnesses share this name; result identity is ambiguous"
                            .to_string(),
                    ),
                    "n/a".to_string(),
                )
            } else if matching.next().is_some() {
                (
                    HarnessStatus::NotEstablished(
                        "kani printed multiple matching result blocks; result identity is ambiguous"
                            .to_string(),
                    ),
                    "n/a".to_string(),
                )
            } else {
                match first {
                    Some((_, text)) => (classify_output(text), verification_time(text)),
                    None => (
                        HarnessStatus::NotEstablished(
                            "kani printed no result block for this harness".to_string(),
                        ),
                        "n/a".to_string(),
                    ),
                }
            };
            HarnessResult {
                harness: harness.clone(),
                status,
                time,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_finds_named_harnesses_past_attributes_and_docs() {
        let source = "#[kani::proof]\nfn one() {}\n\n#[kani::proof]\n#[kani::unwind(4)]\n/// doc\nfn two() {}\nfn not_a_harness() {}\n";
        let found = discover_harnesses(&[("x.rs".to_string(), source.to_string())]);
        let names: Vec<&str> = found.iter().map(|harness| harness.name.as_str()).collect();
        assert_eq!(names, vec!["one", "two"]);
    }

    #[test]
    fn discovery_ignores_harness_text_inside_raw_string_fixtures() {
        let source = "let f = r#\"\n#[kani::proof]\nfn fixture_only() {}\n\"#;\n\n#[kani::proof]\nfn real() {}\n";
        let found = discover_harnesses(&[("x.rs".to_string(), source.to_string())]);
        let names: Vec<&str> = found.iter().map(|harness| harness.name.as_str()).collect();
        assert_eq!(names, vec!["real"]);
    }

    #[test]
    fn classification_requires_an_explicit_success_verdict() {
        let ok = "** 2 of 2 cover properties satisfied\nVERIFICATION:- SUCCESSFUL\n";
        assert_eq!(classify_output(ok), HarnessStatus::Verified);
        assert!(matches!(
            classify_output("VERIFICATION:- FAILED"),
            HarnessStatus::Failed(_)
        ));
        // Absence of a verdict is never a pass, even with a zero exit.
        assert!(matches!(
            classify_output("compiled fine"),
            HarnessStatus::NotEstablished(_)
        ));
        assert!(matches!(
            classify_output("No proof harnesses (functions with #[kani::proof]) were found"),
            HarnessStatus::NotEstablished(_)
        ));
    }

    #[test]
    fn a_solver_timeout_is_not_a_counterexample() {
        // Kani reports a timed-out harness as FAILED; it has no verdict.
        let timed_out = "CBMC failed\nVERIFICATION:- FAILED\nCBMC timed out. You may want to rerun your proof with a larger timeout\n";
        assert!(matches!(
            classify_output(timed_out),
            HarnessStatus::NotEstablished(_)
        ));
        // A real counterexample without the timeout text stays FAILED.
        assert!(matches!(
            classify_output("VERIFICATION:- FAILED\n"),
            HarnessStatus::Failed(_)
        ));
    }

    #[test]
    fn unreachable_cover_makes_the_proof_vacuous() {
        let vacuous =
            "** 1 of 2 cover properties satisfied (1 unreachable)\nVERIFICATION:- SUCCESSFUL\n";
        assert!(matches!(classify_output(vacuous), HarnessStatus::Failed(_)));
        let unsat = "Status: UNSATISFIABLE\nVERIFICATION:- SUCCESSFUL\n";
        assert!(matches!(classify_output(unsat), HarnessStatus::Failed(_)));
    }

    #[test]
    fn blocks_join_to_harnesses_and_a_missing_block_is_not_established() {
        let output = "Checking harness m::one...\nVERIFICATION:- SUCCESSFUL\nVerification Time: 0.5s\nChecking harness m::two...\nVERIFICATION:- FAILED\n";
        let harness = |name: &str| Harness {
            file: "f.rs".to_string(),
            name: name.to_string(),
        };
        let harnesses = [harness("one"), harness("two"), harness("three")];
        let results = results_from_output(&harnesses, output);
        let statuses: Vec<&HarnessStatus> = results.iter().map(|r| &r.status).collect();
        assert!(matches!(statuses.first(), Some(HarnessStatus::Verified)));
        assert!(matches!(statuses.get(1), Some(HarnessStatus::Failed(_))));
        assert!(matches!(
            statuses.get(2),
            Some(HarnessStatus::NotEstablished(_))
        ));
        assert_eq!(results.first().map(|r| r.time.as_str()), Some("0.5s"));
    }

    #[test]
    fn duplicate_source_names_never_share_a_native_result() {
        let success =
            "Checking harness a::same...\nVERIFICATION:- SUCCESSFUL\nVerification Time: 0.5s\n";
        let failure = "Checking harness b::same...\nVERIFICATION:- FAILED\n";
        let unique = "Checking harness m::unique...\nVERIFICATION:- SUCCESSFUL\n";
        // Duplicate names in separate files and in one file are both ambiguous:
        // the current discovery model does not carry native module identities.
        for second_file in ["a.rs", "b.rs"] {
            let harnesses = [
                Harness {
                    file: "a.rs".to_string(),
                    name: "same".to_string(),
                },
                Harness {
                    file: second_file.to_string(),
                    name: "same".to_string(),
                },
                Harness {
                    file: "unique.rs".to_string(),
                    name: "unique".to_string(),
                },
            ];
            for output in [
                format!("{success}{unique}"),
                format!("{success}{failure}{unique}"),
                format!("{failure}{success}{unique}"),
            ] {
                let results = results_from_output(&harnesses, &output);
                assert_eq!(results.len(), 3);
                for (row, expected) in results.iter().zip(&harnesses).take(2) {
                    assert_eq!(&row.harness, expected);
                    assert_eq!(
                        row.status,
                        HarnessStatus::NotEstablished(
                            "multiple discovered harnesses share this name; result identity is ambiguous"
                                .to_string(),
                        ),
                    );
                    assert_eq!(row.time, "n/a");
                }
                assert!(matches!(
                    results.get(2).map(|row| &row.status),
                    Some(HarnessStatus::Verified)
                ));
                for allow_empty in [false, true] {
                    assert!(lane_verdict(&results, true, allow_empty).is_err());
                }
                let report = render_report(&results, "");
                assert!(report.contains("| `same` | `a.rs` | NOT-ESTABLISHED |"));
                assert!(
                    report.contains(&format!("| `same` | `{second_file}` | NOT-ESTABLISHED |"))
                );
                assert!(!report.contains("| `same` | `a.rs` | VERIFIED |"));
                assert!(report.contains("| `unique` | `unique.rs` | VERIFIED |"));
            }
        }
    }

    #[test]
    fn duplicate_native_blocks_are_not_first_match_wins() {
        let harnesses = [Harness {
            file: "one.rs".to_string(),
            name: "same".to_string(),
        }];
        let success =
            "Checking harness a::same...\nVERIFICATION:- SUCCESSFUL\nVerification Time: 0.5s\n";
        let other_failure = "Checking harness b::same...\nVERIFICATION:- FAILED\n";
        let repeated_failure = "Checking harness a::same...\nVERIFICATION:- FAILED\n";
        for output in [
            format!("{success}{other_failure}"),
            format!("{other_failure}{success}"),
            format!("{success}{repeated_failure}"),
            format!("{repeated_failure}{success}"),
            format!("{success}{success}"),
        ] {
            let results = results_from_output(&harnesses, &output);
            assert_eq!(results.len(), 1);
            assert_eq!(
                results.first().map(|row| &row.status),
                Some(&HarnessStatus::NotEstablished(
                    "kani printed multiple matching result blocks; result identity is ambiguous"
                        .to_string(),
                )),
            );
            assert_eq!(results.first().map(|row| row.time.as_str()), Some("n/a"));
            assert!(lane_verdict(&results, true, false).is_err());
        }
    }

    #[test]
    fn unique_native_join_retains_status_time_and_separator_boundary() {
        let harnesses = [Harness {
            file: "one.rs".to_string(),
            name: "same".to_string(),
        }];
        for selector in ["same", "m::same"] {
            let output = format!(
                "Checking harness {selector}...\nVERIFICATION:- SUCCESSFUL\nVerification Time: 0.5s\n"
            );
            let results = results_from_output(&harnesses, &output);
            assert_eq!(results.len(), 1);
            assert!(matches!(
                results.first().map(|row| &row.status),
                Some(HarnessStatus::Verified)
            ));
            assert_eq!(results.first().map(|row| row.time.as_str()), Some("0.5s"));
            assert_eq!(lane_verdict(&results, true, false).err(), None);
        }
        for selector in ["not_same", "m::notsame", "same::nested"] {
            let output = format!("Checking harness {selector}...\nVERIFICATION:- SUCCESSFUL\n");
            let results = results_from_output(&harnesses, &output);
            assert!(matches!(
                results.first().map(|row| &row.status),
                Some(HarnessStatus::NotEstablished(_))
            ));
            assert!(lane_verdict(&results, true, false).is_err());
        }
        let output = "Checking harness m::same...\nVERIFICATION:- FAILED\nCBMC timed out\n";
        let results = results_from_output(&harnesses, output);
        assert!(matches!(
            results.first().map(|row| &row.status),
            Some(HarnessStatus::NotEstablished(_))
        ));
        assert!(lane_verdict(&results, true, false).is_err());
    }
}
