use super::*;

pub(super) fn parse_repo_exposure_static_seams(
    json: &str,
) -> Result<Vec<StaticSeamRecord>, String> {
    let value: Value = serde_json::from_str(json)
        .map_err(|err| format!("failed to parse repo exposure JSON: {err}"))?;
    if let Some(seams) = value.get("seams").and_then(Value::as_array) {
        return parse_repo_exposure_seams(seams);
    }
    if let Some(findings) = value.get("findings").and_then(Value::as_array) {
        return parse_check_output_findings(findings);
    }

    Err("static snapshot JSON is missing repo-exposure `seams` array or check-output `findings` array".to_string())
}

fn parse_repo_exposure_seams(seams: &[Value]) -> Result<Vec<StaticSeamRecord>, String> {
    let mut records = Vec::new();
    for seam in seams {
        let evidence_record = seam
            .get("evidence_record")
            .filter(|value| value.is_object());
        let location = evidence_record
            .and_then(|record| record.get("location"))
            .filter(|value| value.is_object());
        let seam_id = optional_json_string(evidence_record, "seam_id")
            .or_else(|| optional_json_string(Some(seam), "seam_id"))
            .ok_or_else(|| "repo exposure seam is missing string field `seam_id`".to_string())?;
        let seam_kind = optional_json_string(evidence_record, "seam_kind")
            .or_else(|| optional_json_string(Some(seam), "kind"))
            .ok_or_else(|| "repo exposure seam is missing string field `kind`".to_string())?;
        let file = optional_json_string(location, "file")
            .or_else(|| optional_json_string(Some(seam), "file"))
            .map(|path| normalize_report_path(&path))
            .ok_or_else(|| "repo exposure seam is missing string field `file`".to_string())?;
        let line = optional_json_usize(location, "line")
            .or_else(|| optional_json_usize(Some(seam), "line"))
            .ok_or_else(|| "repo exposure seam is missing numeric field `line`".to_string())?;
        let seam_grip_class = optional_json_string(evidence_record, "grip_class")
            .or_else(|| optional_json_string(Some(seam), "grip_class"))
            .ok_or_else(|| "repo exposure seam is missing string field `grip_class`".to_string())?;
        let oracle_source = match evidence_record {
            Some(record) if record.get("related_tests").is_some() => record,
            _ => seam,
        };
        let (oracle_kind, oracle_strength) = strongest_related_oracle(oracle_source);
        records.push(StaticSeamRecord {
            seam_id,
            seam_kind,
            file,
            line,
            seam_grip_class,
            oracle_kind,
            oracle_strength,
            observed_values: evidence_record_values_or_legacy(
                evidence_record,
                seam,
                "observed_values",
                observed_value_strings,
            ),
            missing_discriminators: evidence_record_values_or_legacy(
                evidence_record,
                seam,
                "missing_discriminators",
                missing_discriminator_strings,
            ),
            evidence_source: if evidence_record.is_some() {
                "evidence_record".to_string()
            } else {
                "legacy_fields".to_string()
            },
            evidence_path: evidence_path_stages(evidence_record),
            related_tests_total: related_tests_total(evidence_record, seam),
        });
    }
    Ok(records)
}

fn parse_check_output_findings(findings: &[Value]) -> Result<Vec<StaticSeamRecord>, String> {
    let mut records = Vec::new();
    for finding in findings {
        let Some(record) = static_seam_record_from_check_finding(finding) else {
            continue;
        };
        records.push(record);
    }
    Ok(records)
}

fn static_seam_record_from_check_finding(finding: &Value) -> Option<StaticSeamRecord> {
    let canonical_gap_id = string_at_path(
        finding,
        &[
            &["canonical_gap_id"],
            &["canonical_gap", "id"],
            &["python_repair_card", "canonical_gap_id"],
        ],
    )?;
    let canonical_gap = finding
        .get("canonical_gap")
        .filter(|value| value.is_object());
    let seam_kind = string_at_path(
        finding,
        &[
            &["canonical_gap", "behavior_kind"],
            &["probe", "family"],
            &["python_repair_card", "source"],
        ],
    )
    .unwrap_or("unknown");
    let file = string_at_path(
        finding,
        &[
            &["canonical_gap", "file"],
            &["probe", "file"],
            &["python_repair_card", "suggested_location", "source_file"],
        ],
    )
    .map(normalize_report_path)
    .unwrap_or_else(|| "unknown".to_string());
    let line = usize_at_path(finding, &[&["probe", "line"]]).unwrap_or(0);
    let classification =
        string_at_path(finding, &[&["classification"]]).unwrap_or("static_unknown");
    let (oracle_kind, oracle_strength) = strongest_related_oracle(finding);
    let evidence_path = check_output_ripr_stages(finding);
    let related_tests_total = related_tests_total(None, finding);
    let observed_values = observed_value_strings(finding);
    let missing_discriminators = missing_discriminator_strings(finding);

    Some(StaticSeamRecord {
        seam_id: canonical_gap_id.to_string(),
        seam_kind: canonical_gap
            .and_then(|gap| optional_json_string(Some(gap), "behavior_kind"))
            .unwrap_or_else(|| seam_kind.to_string()),
        file,
        line,
        seam_grip_class: grip_class_from_check_classification(classification).to_string(),
        oracle_kind,
        oracle_strength,
        observed_values,
        missing_discriminators,
        evidence_source: "check_output_finding".to_string(),
        evidence_path,
        related_tests_total,
    })
}

fn optional_json_string(value: Option<&Value>, key: &str) -> Option<String> {
    value?.get(key).and_then(json_scalar_as_string)
}

fn optional_json_usize(value: Option<&Value>, key: &str) -> Option<usize> {
    value?.get(key).and_then(json_scalar_as_usize)
}

fn string_at_path<'a>(value: &'a Value, paths: &[&[&str]]) -> Option<&'a str> {
    paths
        .iter()
        .find_map(|path| path_value(value, path).and_then(Value::as_str))
}

fn usize_at_path(value: &Value, paths: &[&[&str]]) -> Option<usize> {
    paths
        .iter()
        .find_map(|path| path_value(value, path).and_then(json_scalar_as_usize))
}

fn path_value<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut cursor = value;
    for segment in path {
        cursor = cursor.get(*segment)?;
    }
    Some(cursor)
}

fn strongest_related_oracle(seam: &Value) -> (String, String) {
    let mut best_kind = "unknown".to_string();
    let mut best_strength = "unknown".to_string();
    let mut best_rank = 0;

    if let Some(related) = seam.get("related_tests").and_then(Value::as_array) {
        for test in related {
            let strength = test
                .get("oracle_strength")
                .and_then(Value::as_str)
                .map_or("unknown", |strength| strength);
            let rank = oracle_strength_rank(strength);
            if rank > best_rank {
                best_rank = rank;
                best_strength = strength.to_string();
                best_kind = test
                    .get("oracle_kind")
                    .and_then(Value::as_str)
                    .map_or("unknown", |kind| kind)
                    .to_string();
            }
        }
    }

    (best_kind, best_strength)
}

pub(super) fn oracle_strength_rank(strength: &str) -> u8 {
    match strength {
        "strong" => 5,
        "medium" => 4,
        "weak" => 3,
        "smoke" => 2,
        "none" => 1,
        _ => 0,
    }
}

fn evidence_record_values_or_legacy(
    evidence_record: Option<&Value>,
    seam: &Value,
    key: &str,
    parser: fn(&Value) -> Vec<String>,
) -> Vec<String> {
    if let Some(record) = evidence_record.filter(|record| record.get(key).is_some()) {
        parser(record)
    } else {
        parser(seam)
    }
}

fn observed_value_strings(seam: &Value) -> Vec<String> {
    match seam.get("observed_values").and_then(Value::as_array) {
        Some(items) => items
            .iter()
            .filter_map(|item| {
                json_scalar_as_string(item)
                    .or_else(|| item.get("value").and_then(json_scalar_as_string))
            })
            .collect::<Vec<_>>(),
        None => Vec::new(),
    }
}

fn missing_discriminator_strings(seam: &Value) -> Vec<String> {
    match seam.get("missing_discriminators").and_then(Value::as_array) {
        Some(items) => items
            .iter()
            .filter_map(|item| {
                if let Some(value) = json_scalar_as_string(item) {
                    return Some(value);
                }
                let value = item.get("value").and_then(json_scalar_as_string)?;
                match item.get("reason").and_then(json_scalar_as_string) {
                    Some(reason) if !reason.is_empty() => Some(format!("{value} ({reason})")),
                    _ => Some(value),
                }
            })
            .collect::<Vec<_>>(),
        None => Vec::new(),
    }
}

fn related_tests_total(evidence_record: Option<&Value>, seam: &Value) -> usize {
    let source = match evidence_record {
        Some(record) if record.get("related_tests_total").is_some() => record,
        _ => seam,
    };
    if let Some(total) = source
        .get("related_tests_total")
        .and_then(json_scalar_as_usize)
    {
        return total;
    }
    match source.get("related_tests").and_then(Value::as_array) {
        Some(related_tests) => related_tests.len(),
        None => 0,
    }
}

fn evidence_path_stages(evidence_record: Option<&Value>) -> BTreeMap<String, StaticEvidenceStage> {
    let mut stages = BTreeMap::new();
    let Some(path) = evidence_record
        .and_then(|record| record.get("evidence_path"))
        .and_then(Value::as_object)
    else {
        return stages;
    };
    for stage in EVIDENCE_STAGES {
        let Some(value) = path.get(*stage) else {
            continue;
        };
        stages.insert(
            (*stage).to_string(),
            StaticEvidenceStage {
                state: optional_json_string_or_empty(Some(value), "state"),
                confidence: optional_json_string_or_empty(Some(value), "confidence"),
                summary: optional_json_string_or_empty(Some(value), "summary"),
            },
        );
    }
    stages
}

fn check_output_ripr_stages(finding: &Value) -> BTreeMap<String, StaticEvidenceStage> {
    let mut stages = BTreeMap::new();
    let Some(ripr) = finding.get("ripr").and_then(Value::as_object) else {
        return stages;
    };
    for (stage, source_stage) in [
        ("reach", "reach"),
        ("activate", "infect"),
        ("propagate", "propagate"),
        ("observe", "observe"),
        ("discriminate", "discriminate"),
    ] {
        let Some(value) = ripr.get(source_stage) else {
            continue;
        };
        stages.insert(
            stage.to_string(),
            StaticEvidenceStage {
                state: optional_json_string_or_empty(Some(value), "state"),
                confidence: optional_json_string_or_empty(Some(value), "confidence"),
                summary: optional_json_string_or_empty(Some(value), "summary"),
            },
        );
    }
    stages
}

fn grip_class_from_check_classification(classification: &str) -> &'static str {
    match classification {
        "exposed" => "strongly_gripped",
        "weakly_exposed" => "weakly_gripped",
        "reachable_unrevealed" => "reachable_unrevealed",
        "no_static_path" => "ungripped",
        "infection_unknown" => "activation_unknown",
        "propagation_unknown" => "propagation_unknown",
        "observation_unknown" => "observation_unknown",
        "discrimination_unknown" => "discrimination_unknown",
        "static_unknown" => "opaque",
        _ => "opaque",
    }
}

fn optional_json_string_or_empty(value: Option<&Value>, key: &str) -> String {
    let mut text = String::new();
    if let Some(value) = optional_json_string(value, key) {
        text = value;
    }
    text
}

fn json_scalar_as_string(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

fn json_scalar_as_usize(value: &Value) -> Option<usize> {
    match value {
        Value::Number(number) => number
            .as_u64()
            .and_then(|value| usize::try_from(value).ok()),
        Value::String(text) => text.trim().parse::<usize>().ok(),
        _ => None,
    }
}
