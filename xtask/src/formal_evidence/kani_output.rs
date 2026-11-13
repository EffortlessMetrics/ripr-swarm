//! Single pure Kani output interpretation shared by live collection and
//! retained evidence replay. No process or authenticity claim is made here.
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
    pub fn label(&self) -> &'static str {
        match self {
            Self::Verified => "VERIFIED",
            Self::Failed(_) => "FAILED",
            Self::NotEstablished(_) => "NOT-ESTABLISHED",
        }
    }

    pub fn detail(&self) -> &str {
        match self {
            Self::Verified => "within the harness bounds",
            Self::Failed(detail) | Self::NotEstablished(detail) => detail,
        }
    }
}

/// Classifies one harness result block from Kani's output.
/// Only an explicit success verdict with positive evidence that every required
/// `cover!` was satisfied is `Verified`; missing evidence is never a pass.
pub fn classify_output(output: &str) -> HarnessStatus {
    // Kani prints `VERIFICATION:- FAILED` for a solver timeout too; that is no
    // counterexample, so it is checked first and stays `NOT-ESTABLISHED`.
    if output.contains("CBMC timed out") {
        return HarnessStatus::NotEstablished(
            "CBMC exceeded the per-harness timeout; no verdict".to_string(),
        );
    }
    let verdicts: Vec<_> = output
        .lines()
        .filter(|line| line.starts_with("VERIFICATION:-"))
        .collect();
    if verdicts.len() != 1 {
        return HarnessStatus::NotEstablished("native verdict is absent or ambiguous".to_string());
    }
    if verdicts[0] == "VERIFICATION:- FAILED" {
        return HarnessStatus::Failed(
            "native verification failed; inspect the reported properties".to_string(),
        );
    }
    if verdicts[0] == "VERIFICATION:- SUCCESSFUL"
        || verdicts[0] == "VERIFICATION:- SUCCESSFUL (encountered one or more panics as expected)"
    {
        return classify_cover_evidence(output);
    }

    HarnessStatus::NotEstablished(
        "no successful verification verdict in the kani output".to_string(),
    )
}

struct CoverSummary {
    satisfied: u64,
    total: u64,
    undetermined: u64,
}

/// Kani 0.68.0 emits one positive-total summary per harness. Its optional
/// suffix lists undetermined then unreachable counts; unsatisfiable covers
/// contribute to the total without a suffix. Parse that format fail-closed.
fn parse_cover_summary(line: &str) -> Option<CoverSummary> {
    fn count(text: &str) -> Option<u64> {
        if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        text.parse().ok()
    }

    let body = line.trim().strip_prefix("** ")?;
    let (counts, suffix) = body.split_once(" cover properties satisfied")?;
    let mut words = counts.split_whitespace();
    let satisfied = count(words.next()?)?;
    if words.next()? != "of" {
        return None;
    }
    let total = count(words.next()?)?;
    if words.next().is_some() || total == 0 || satisfied > total {
        return None;
    }

    let mut undetermined = 0;
    let mut unreachable = 0;
    if !suffix.is_empty() {
        let details = suffix.strip_prefix(" (")?.strip_suffix(')')?;
        for detail in details.split(',') {
            if let Some(value) = detail.strip_suffix(" undetermined") {
                // The native renderer emits each nonzero count at most once,
                // and emits undetermined before unreachable.
                if undetermined != 0 || unreachable != 0 {
                    return None;
                }
                undetermined = count(value)?;
                if undetermined == 0 {
                    return None;
                }
            } else if let Some(value) = detail.strip_suffix(" unreachable") {
                if unreachable != 0 {
                    return None;
                }
                unreachable = count(value)?;
                if unreachable == 0 {
                    return None;
                }
            } else {
                return None;
            }
        }
    }
    if undetermined.checked_add(unreachable)? > total - satisfied {
        return None;
    }
    Some(CoverSummary {
        satisfied,
        total,
        undetermined,
    })
}

fn classify_cover_evidence(output: &str) -> HarnessStatus {
    let missing = |reason: &str| HarnessStatus::NotEstablished(reason.to_string());
    let mut summaries = output
        .lines()
        .filter(|line| line.contains("cover properties satisfied"));
    let Some(line) = summaries.next() else {
        return missing("kani reported no positive cover summary for this harness");
    };
    if summaries.next().is_some() {
        return missing("kani printed multiple cover summaries for this harness");
    }
    let Some(summary) = parse_cover_summary(line) else {
        return missing("kani cover summary is malformed, inconsistent or has zero properties");
    };

    // The regular renderer's total omits raw Unknown/Error properties. Unknown
    // is printed as UNDETERMINED, so even an N-of-N summary cannot override it.
    // An ordinary assertion can legitimately be UNREACHABLE. Associate each
    // status with its regular Check header so a complete summary cannot hide
    // an explicitly unreachable cover; match the final property class only.
    let mut is_cover_property = false;
    for line in output.lines() {
        let text = line.trim();
        if let Some(check) = text.strip_prefix("Check ") {
            is_cover_property = check
                .split_once(": ")
                .is_some_and(|(_, property)| property.rsplit('.').nth(1) == Some("cover"));
            continue;
        }
        let Some(status) = text
            .strip_prefix("- Status:")
            .or_else(|| text.strip_prefix("Status:"))
        else {
            continue;
        };
        match status.trim() {
            "UNDETERMINED" | "ERROR" => {
                return missing("kani reported an undetermined or erroneous property");
            }
            "UNSATISFIABLE" if summary.satisfied == summary.total => {
                return missing("kani property status contradicts its complete cover summary");
            }
            "UNREACHABLE" if is_cover_property && summary.satisfied == summary.total => {
                return missing("kani property status contradicts its complete cover summary");
            }
            "SUCCESS" | "FAILURE" | "COVERED" | "UNCOVERED" if is_cover_property => {
                return missing("kani reported an unsupported cover property status");
            }
            // FAILURE can accompany SUCCESSFUL for an expected-panic harness.
            "SUCCESS" | "FAILURE" | "SATISFIED" | "UNSATISFIABLE" | "UNREACHABLE" | "COVERED"
            | "UNCOVERED" => {}
            _ => return missing("kani reported an unrecognized property status"),
        }
    }
    if summary.undetermined > 0 {
        return missing("kani did not determine every required cover property");
    }
    if summary.satisfied < summary.total {
        return HarnessStatus::Failed("a required cover property was not satisfied".to_string());
    }
    HarnessStatus::Verified
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

/// Render the scoped lane result without promoting native assurance.
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
            markdown_text(&result.harness.name),
            markdown_text(&result.harness.file),
            result.status.label(),
            markdown_text(result.status.detail()),
            markdown_text(&result.time)
        ));
    }
    out
}

/// Splits Kani's output into `(qualified harness name, block text)` pairs at
/// each `Checking harness <name>...` header.
pub fn split_harness_blocks(output: &str) -> Vec<(String, String)> {
    let mut blocks: Vec<(String, String)> = Vec::new();
    for raw in output.split_inclusive('\n') {
        let line = raw.trim_end_matches(['\r', '\n']);
        if let Some(rest) = line.strip_prefix("Checking harness ") {
            let name = rest.trim_end().trim_end_matches("...").to_string();
            blocks.push((name, String::new()));
        } else if let Some((_, text)) = blocks.last_mut() {
            text.push_str(raw);
        }
    }
    blocks
}

pub fn cover_observation(output: &str) -> Option<crate::formal_evidence::native::CoverObservation> {
    let mut lines = output
        .lines()
        .filter(|line| line.contains("cover properties satisfied"));
    let summary = parse_cover_summary(lines.next()?)?;
    if lines.next().is_some() {
        return None;
    }
    Some(crate::formal_evidence::native::CoverObservation {
        total: summary.total,
        satisfied: summary.satisfied,
        undetermined: summary.undetermined,
    })
}

fn verification_time(block: &str) -> String {
    block
        .lines()
        .find_map(|line| line.trim().strip_prefix("Verification Time:"))
        .map_or_else(|| "n/a".to_string(), |time| time.trim().to_string())
}

/// Kani's pinned renderer emits decimal seconds. Preserve the original string
/// separately and require one bounded, nonnegative duration for evidence rows.
pub fn verification_duration_ms(block: &str) -> Option<u64> {
    let mut times = block
        .lines()
        .filter_map(|line| line.trim().strip_prefix("Verification Time:"));
    let seconds = times.next()?.trim().strip_suffix('s')?;
    if times.next().is_some() {
        return None;
    }
    let (whole, fraction) = seconds.split_once('.').unwrap_or((seconds, ""));
    if whole.is_empty()
        || !whole.bytes().all(|v| v.is_ascii_digit())
        || !fraction.bytes().all(|v| v.is_ascii_digit())
        || fraction.len() > 9
    {
        return None;
    }
    let millis = whole.parse::<u64>().ok()?.checked_mul(1000)?;
    let fractional = format!("{fraction:0<3}").get(..3)?.parse::<u64>().ok()?;
    millis.checked_add(fractional)
}

/// Typed native meaning, shared by live capture and raw replay. Process state
/// and semantic cover admission are separate inputs to the registered join.
pub fn native_verdict(block: &str) -> crate::formal_evidence::Verdict {
    use crate::formal_evidence::Verdict;
    if block.contains("CBMC timed out") {
        return Verdict::Timeout;
    }
    if block.lines().any(|line| {
        line == "VERIFICATION:- SUCCESSFUL (encountered one or more panics as expected)"
    }) {
        return Verdict::Unsupported;
    }
    match classify_output(block) {
        HarnessStatus::Verified => match native_properties(block) {
            Ok(properties)
                if properties.iter().all(|property| {
                    matches!(
                        property.status.as_str(),
                        "SUCCESS" | "SATISFIED" | "UNREACHABLE"
                    )
                }) =>
            {
                Verdict::Verified
            }
            _ => Verdict::Unknown,
        },
        HarnessStatus::Failed(_) if block.lines().any(|line| line == "VERIFICATION:- FAILED") => {
            match native_properties(block) {
                Ok(properties)
                    if crate::formal_evidence::native::has_counterexample_properties(
                        &properties,
                    ) =>
                {
                    Verdict::Counterexample
                }
                _ => Verdict::Unknown,
            }
        }
        HarnessStatus::Failed(_) => match cover_properties(block) {
            Ok(properties)
                if properties
                    .iter()
                    .any(|p| matches!(p.status.as_str(), "UNSATISFIABLE" | "UNREACHABLE")) =>
            {
                Verdict::UnreachableCover
            }
            _ => Verdict::Unknown,
        },
        HarnessStatus::NotEstablished(_) => Verdict::Unknown,
    }
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

fn markdown_text(text: &str) -> String {
    text.replace(['\n', '\r', '\t'], " ")
        .replace('|', "\\|")
        .replace('`', "&#96;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Read every native property once, retaining its exact identity, status and
/// location. Assertion and cover consumers share this parser and raw block.
pub fn native_properties(
    output: &str,
) -> Result<Vec<crate::formal_evidence::native::NativeProperty>, String> {
    use crate::formal_evidence::native::NativeProperty;
    let mut properties = Vec::new();
    let mut current: Option<NativeProperty> = None;
    let mut numbers = std::collections::BTreeSet::new();
    let mut description_open = false;
    for raw_line in output.lines() {
        let line = raw_line.trim();
        if description_open {
            if line.starts_with("Check ")
                || line.starts_with("- Status:")
                || line.starts_with("- Description:")
                || line.starts_with("- Location:")
                || line.starts_with("VERIFICATION:-")
            {
                return Err(
                    "native description is unterminated or has an ambiguous metadata continuation"
                        .to_string(),
                );
            }
            let description = current
                .as_mut()
                .and_then(|property| property.description.as_mut())
                .ok_or("native description owner absent")?;
            description.push('\n');
            description.push_str(raw_line);
            description_open = !raw_line.ends_with('"');
            continue;
        }
        if let Some(check) = line.strip_prefix("Check ") {
            if let Some(property) = current.take() {
                properties.push(property);
            }
            let (number, name) = check
                .split_once(": ")
                .ok_or("malformed native property header")?;
            let number = number.parse::<u64>().map_err(|e| e.to_string())?;
            if number == 0 || !numbers.insert(number) || name.is_empty() {
                return Err("native property number or identity is invalid".to_string());
            }
            current = Some(NativeProperty {
                name: name.to_string(),
                status: String::new(),
                description: None,
                location: None,
            });
        } else if let Some(property) = current.as_mut() {
            if let Some(status) = line.strip_prefix("- Status:") {
                if !property.status.is_empty() {
                    return Err("duplicate native property status".to_string());
                }
                property.status = status.trim().to_string();
            } else if let Some(description) = line.strip_prefix("- Description:") {
                if property.description.is_some() {
                    return Err("duplicate native property description".to_string());
                }
                let description = description.trim();
                if !description.starts_with('"') {
                    return Err(
                        "native description is outside the pinned quoted format".to_string()
                    );
                }
                description_open = description.len() == 1 || !description.ends_with('"');
                property.description = Some(description.to_string());
            } else if let Some(location) = line.strip_prefix("- Location:") {
                if property.location.is_some() {
                    return Err("duplicate native property location".to_string());
                }
                property.location = Some(location.trim().to_string());
            } else if !line.is_empty()
                && property.description.is_some()
                && property.location.is_none()
            {
                return Err(
                    "unsupported native description continuation before its location".to_string(),
                );
            }
        }
    }
    if description_open {
        return Err("native property description is unterminated".to_string());
    }
    if let Some(property) = current {
        properties.push(property);
    }
    let mut names = std::collections::BTreeSet::new();
    for property in &properties {
        if !names.insert(&property.name)
            || !matches!(
                property.status.as_str(),
                "SUCCESS"
                    | "FAILURE"
                    | "SATISFIED"
                    | "UNSATISFIABLE"
                    | "UNREACHABLE"
                    | "UNDETERMINED"
                    | "ERROR"
            )
        {
            return Err("native property identity/status is duplicate or unsupported".to_string());
        }
    }
    Ok(properties)
}

pub fn cover_properties(
    output: &str,
) -> Result<Vec<crate::formal_evidence::native::CoverProperty>, String> {
    cover_property_projection(&native_properties(output)?)
}

/// Derive covers from the same uniquely attributed native property set retained
/// by the observation. Ambiguous blocks must never lend either projection.
pub fn cover_property_projection(
    properties: &[crate::formal_evidence::native::NativeProperty],
) -> Result<Vec<crate::formal_evidence::native::CoverProperty>, String> {
    let properties: Vec<_> = properties
        .iter()
        .filter(|property| property.name.rsplit('.').nth(1) == Some("cover"))
        .cloned()
        .collect();
    if properties.iter().any(|property| {
        !matches!(
            property.status.as_str(),
            "SATISFIED" | "UNSATISFIABLE" | "UNREACHABLE" | "UNDETERMINED" | "ERROR"
        )
    }) {
        return Err("native cover property has an unsupported status".to_string());
    }
    Ok(properties)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_fields_cannot_add_markdown_rows_or_markup() {
        let row = HarnessResult {
            harness: Harness {
                file: "a|b.rs".into(),
                name: "<x>`\n".into(),
            },
            status: HarnessStatus::NotEstablished("raw\r|<detail>".into()),
            time: "1\t|s".into(),
        };
        assert_eq!(
            render_report(&[row], "").lines().last(),
            Some(
                "| `&lt;x&gt;&#96; ` | `a\\|b.rs` | NOT-ESTABLISHED | raw \\|&lt;detail&gt; | 1 \\|s |"
            )
        );
    }

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
    fn classification_requires_positive_complete_cover_evidence() {
        for summary in [
            "",
            " ** 0 of 0 cover properties satisfied\n",
            " ** invalid of 1 cover properties satisfied\n",
            " ** 1 of invalid cover properties satisfied\n",
            " ** 2 of 1 cover properties satisfied\n",
            " ** 1 / 1 cover properties satisfied\n",
            " ** +1 of 1 cover properties satisfied\n",
            " ** 18446744073709551616 of 18446744073709551616 cover properties satisfied\n",
            " ** 1 of 1 cover properties satisfied (1 undetermined)\n",
            " ** 1 of 1 cover properties satisfied (1 unreachable)\n",
            " ** 1 of 1 cover properties satisfied (0 undetermined)\n",
            " ** 1 of 1 cover properties satisfied (unknown)\n",
            " ** 1 of 1 cover properties satisfied unexpectedly\n",
            " ** 0 of 2 cover properties satisfied (1 unreachable,1 undetermined)\n",
            " ** 0 of 2 cover properties satisfied (1 unreachable,1 unreachable)\n",
            " ** 0 of 2 cover properties satisfied (1 undetermined,1 undetermined)\n",
            " ** 0 of 18446744073709551615 cover properties satisfied (18446744073709551615 undetermined,1 unreachable)\n",
            " ** 1 of 1 cover properties satisfied\n ** 1 of 1 cover properties satisfied\n",
            " ** 1 of 1 cover properties satisfied\n ** 0 of 1 cover properties satisfied\n",
            " ** invalid of 1 cover properties satisfied\n ** 1 of 1 cover properties satisfied\n",
        ] {
            let output = format!("{summary}VERIFICATION:- SUCCESSFUL\n");
            assert!(
                matches!(classify_output(&output), HarnessStatus::NotEstablished(_)),
                "missing or ambiguous cover evidence was accepted: {output:?}",
            );
        }
    }

    #[test]
    fn undetermined_properties_never_become_verified() {
        for evidence in [
            " ** 1 of 2 cover properties satisfied (1 undetermined)\n",
            " ** 1 of 3 cover properties satisfied (1 undetermined,1 unreachable)\n",
            "Check 1: proof.cover.1\n\t - Status: UNDETERMINED\n ** 1 of 1 cover properties satisfied\n",
            "Check 1: proof.cover.1\n\t - Status: ERROR\n ** 1 of 1 cover properties satisfied\n",
            "Check 1: proof.cover.1\n\t - Status: UNRECOGNIZED\n ** 1 of 1 cover properties satisfied\n",
        ] {
            let output = format!("{evidence}VERIFICATION:- SUCCESSFUL\n");
            assert!(
                matches!(classify_output(&output), HarnessStatus::NotEstablished(_)),
                "undetermined property evidence was accepted: {output:?}",
            );
        }
    }

    #[test]
    fn complete_cover_evidence_allows_unreachable_ordinary_assertions() {
        for properties in [
            "Check 1: proof.assertion.1\n\t - Status: UNREACHABLE\nCheck 2: proof.cover.1\n\t - Status: SATISFIED\n",
            "Check 1: proof.cover.1\n\t - Status: SATISFIED\nCheck 2: proof.assertion.1\n\t - Status: UNREACHABLE\n",
            "Check 1: proof.cover.1\n\t - Status: SATISFIED\nCheck 2: proof.cover.1.assertion.2\n\t - Status: UNREACHABLE\n",
        ] {
            let output = format!(
                "{properties} ** 1 of 1 cover properties satisfied\nVERIFICATION:- SUCCESSFUL\n"
            );
            assert_eq!(classify_output(&output), HarnessStatus::Verified);
        }
    }

    #[test]
    fn complete_cover_evidence_preserves_expected_panic_success() {
        let output = "Check 1: proof.assertion.1\n\t - Status: FAILURE\nCheck 2: proof.cover.1\n\t - Status: SATISFIED\n ** 1 of 1 cover properties satisfied\nVERIFICATION:- SUCCESSFUL (encountered one or more panics as expected)\n";
        assert_eq!(classify_output(output), HarnessStatus::Verified);
    }

    #[test]
    fn cover_property_and_summary_must_not_contradict() {
        for property in ["proof.cover.1", "module::proof.cover.2", "cover.1"] {
            for status in [
                "UNSATISFIABLE",
                "UNREACHABLE",
                "SUCCESS",
                "FAILURE",
                "COVERED",
                "UNCOVERED",
            ] {
                let output = format!(
                    "Check 1: {property}\n\t - Status: {status}\n ** 1 of 1 cover properties satisfied\nVERIFICATION:- SUCCESSFUL\n"
                );
                assert!(
                    matches!(classify_output(&output), HarnessStatus::NotEstablished(_)),
                    "a cover status cannot be overridden by a complete summary: {output:?}",
                );
            }
        }
    }

    #[test]
    fn pinned_native_cover_captures_keep_distinct_lane_dispositions() {
        // Unmodified output captured from the pinned native Kani 0.68.0. This
        // test replays bytes, not the verifier; see the fixture provenance.
        for (name, stdout, stderr, expected) in [
            (
                "cover_pass_compact",
                include_str!(
                    "../../tests/fixtures/kani-cover-evidence/cover-pass-compact.stdout.txt"
                ),
                include_str!(
                    "../../tests/fixtures/kani-cover-evidence/cover-pass-compact.stderr.txt"
                ),
                "VERIFIED",
            ),
            (
                "cover_overconstrained",
                include_str!("../../tests/fixtures/kani-cover-evidence/cover-fail.stdout.txt"),
                include_str!("../../tests/fixtures/kani-cover-evidence/cover-fail.stderr.txt"),
                "FAILED",
            ),
            (
                "cover_unreachable",
                include_str!(
                    "../../tests/fixtures/kani-cover-evidence/cover-unreachable.stdout.txt"
                ),
                include_str!(
                    "../../tests/fixtures/kani-cover-evidence/cover-unreachable.stderr.txt"
                ),
                "FAILED",
            ),
            (
                "cover_undetermined_compact",
                include_str!(
                    "../../tests/fixtures/kani-cover-evidence/cover-undetermined-compact.stdout.txt"
                ),
                include_str!(
                    "../../tests/fixtures/kani-cover-evidence/cover-undetermined-compact.stderr.txt"
                ),
                // This native run fails verification before cover admission;
                // success-looking undetermined output has separate controls.
                "FAILED",
            ),
            (
                "no_cover_control",
                include_str!("../../tests/fixtures/kani-cover-evidence/no-cover.stdout.txt"),
                include_str!("../../tests/fixtures/kani-cover-evidence/no-cover.stderr.txt"),
                "NOT-ESTABLISHED",
            ),
        ] {
            let output = format!("{stdout}\n{stderr}");
            let harnesses = [Harness {
                file: "native-fixture.rs".to_string(),
                name: name.to_string(),
            }];
            assert_eq!(split_harness_blocks(&output).len(), 1, "{name}");
            let rows = results_from_output(&harnesses, &output);
            assert_eq!(rows.len(), 1, "{name}");
            assert_eq!(rows[0].status.label(), expected, "{name}");
        }
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
    fn unsatisfied_cover_fails_the_reachability_policy() {
        for output in [
            " ** 1 of 2 cover properties satisfied (1 unreachable)\nVERIFICATION:- SUCCESSFUL\n",
            "Check 1: proof.cover.1\n\t - Status: UNSATISFIABLE\n ** 0 of 1 cover properties satisfied\nVERIFICATION:- SUCCESSFUL\n",
            " ** 1 of 2 cover properties satisfied\nVERIFICATION:- SUCCESSFUL\n",
        ] {
            assert_eq!(
                classify_output(output),
                HarnessStatus::Failed("a required cover property was not satisfied".to_string()),
            );
        }
    }

    #[test]
    fn blocks_join_to_harnesses_and_a_missing_block_is_not_established() {
        let output = "Checking harness m::one...\n ** 1 of 1 cover properties satisfied\nVERIFICATION:- SUCCESSFUL\nVerification Time: 0.5s\nChecking harness m::two...\nVERIFICATION:- FAILED\n";
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
        let success = "Checking harness a::same...\n ** 1 of 1 cover properties satisfied\nVERIFICATION:- SUCCESSFUL\nVerification Time: 0.5s\n";
        let failure = "Checking harness b::same...\nVERIFICATION:- FAILED\n";
        let unique = "Checking harness m::unique...\n ** 1 of 1 cover properties satisfied\nVERIFICATION:- SUCCESSFUL\n";
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
        let success = "Checking harness a::same...\n ** 1 of 1 cover properties satisfied\nVERIFICATION:- SUCCESSFUL\nVerification Time: 0.5s\n";
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
                "Checking harness {selector}...\n ** 1 of 1 cover properties satisfied\nVERIFICATION:- SUCCESSFUL\nVerification Time: 0.5s\n"
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
            let output = format!(
                "Checking harness {selector}...\n ** 1 of 1 cover properties satisfied\nVERIFICATION:- SUCCESSFUL\n"
            );
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
