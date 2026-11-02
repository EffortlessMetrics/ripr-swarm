//! Replay `edited_producer` Perl verdict-corpus packets from a named producer base
//! case plus the machine-readable tail of `perl_facts.edits` (RIPR-SPEC-0245, #9062).

use super::verdict_corpus::{Case, PacketProvenance, PerlFacts};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

const EDIT_PREFIX: &str = "Started from the producer packet of ";

pub(crate) fn edited_producer_packet_violations(
    case_id: &str,
    facts: &PerlFacts,
    dir: &Path,
    cases_by_id: &BTreeMap<&str, &Case>,
) -> Vec<String> {
    let mut violations = Vec::new();
    let Some(edits) = facts
        .edits
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    else {
        return violations;
    };
    let Ok((base_case_id, ops)) = parse_edit_provenance(edits) else {
        violations.push(format!(
            "case `{case_id}` `perl_facts.edits` is not a replayable edited_producer derivation: {}",
            parse_edit_provenance(edits).err().unwrap_or_default()
        ));
        return violations;
    };
    let Some(base_case) = cases_by_id.get(base_case_id.as_str()) else {
        violations.push(format!(
            "case `{case_id}` edits name unknown base producer case `{base_case_id}`"
        ));
        return violations;
    };
    let Some(base_facts) = &base_case.perl_facts else {
        violations.push(format!(
            "case `{case_id}` edits name base `{base_case_id}` which has no `perl_facts`"
        ));
        return violations;
    };
    if base_facts.provenance != PacketProvenance::Producer {
        violations.push(format!(
            "case `{case_id}` edits must start from a producer packet; `{base_case_id}` is `{}`",
            base_facts.provenance.as_str()
        ));
        return violations;
    }
    let base_path = dir.join(&base_facts.packet);
    let committed_path = dir.join(&facts.packet);
    let Ok(base_text) = fs::read_to_string(&base_path) else {
        violations.push(format!(
            "case `{case_id}` base packet `{}` is unreadable",
            base_facts.packet
        ));
        return violations;
    };
    let Ok(committed_text) = fs::read_to_string(&committed_path) else {
        return violations;
    };
    let Ok(mut base_packet) = serde_json::from_str::<Value>(&base_text) else {
        violations.push(format!(
            "case `{case_id}` base packet `{}` is not valid JSON",
            base_facts.packet
        ));
        return violations;
    };
    let Ok(committed_packet) = serde_json::from_str::<Value>(&committed_text) else {
        violations.push(format!(
            "case `{case_id}` packet `{}` is not valid JSON",
            facts.packet
        ));
        return violations;
    };
    if let Err(err) = apply_ops(&mut base_packet, &ops) {
        violations.push(format!(
            "case `{case_id}` could not replay edits onto `{base_case_id}`: {err}"
        ));
        return violations;
    }
    let rebuilt_fingerprint = recompute_packet_fingerprint(&base_packet);
    if let Some(obj) = base_packet.as_object_mut() {
        obj.insert(
            "packet_fingerprint".to_string(),
            Value::String(rebuilt_fingerprint.clone()),
        );
    }
    if canonical_packet_json(&base_packet) != canonical_packet_json(&committed_packet) {
        violations.push(format!(
            "case `{case_id}` packet `{}` does not match `{base_case_id}` plus its stated edits; regenerate the edited packet or fix `perl_facts.edits`",
            facts.packet
        ));
    }
    match committed_packet
        .get("packet_fingerprint")
        .and_then(Value::as_str)
    {
        Some(fp) if fp == rebuilt_fingerprint => {}
        Some(_) => violations.push(format!(
            "case `{case_id}` packet `{}` has a stale `packet_fingerprint` for its edited facts",
            facts.packet
        )),
        None => violations.push(format!(
            "case `{case_id}` packet `{}` has no `packet_fingerprint`",
            facts.packet
        )),
    }
    violations
}

fn canonical_packet_json(packet: &Value) -> String {
    let mut clone = packet.clone();
    if let Some(obj) = clone.as_object_mut() {
        obj.remove("packet_fingerprint");
    }
    serde_json::to_string(&clone).unwrap_or_default()
}

fn normalize_repo_relative(path: &str) -> String {
    path.replace('\\', "/")
}

fn recompute_packet_fingerprint(packet: &Value) -> String {
    let mut hasher = Sha256::new();
    let empty = Vec::<Value>::new();

    let mut files: Vec<(&str, String)> = packet
        .get("files")
        .and_then(Value::as_array)
        .unwrap_or(&empty)
        .iter()
        .filter_map(|row| {
            Some((
                row.get("file_id")?.as_str()?,
                normalize_repo_relative(row.get("path")?.as_str()?),
            ))
        })
        .collect();
    files.sort();
    for (file_id, path) in files {
        hasher.update(b"file\0");
        hasher.update(file_id.as_bytes());
        hasher.update(b"\0");
        hasher.update(path.as_bytes());
        hasher.update(b"\0");
    }

    let mut owner_ids: Vec<&str> = packet
        .get("owners")
        .and_then(Value::as_array)
        .unwrap_or(&empty)
        .iter()
        .filter_map(|row| row.get("owner_id").and_then(Value::as_str))
        .collect();
    owner_ids.sort();
    for owner_id in owner_ids {
        hasher.update(b"owner\0");
        hasher.update(owner_id.as_bytes());
        hasher.update(b"\0");
    }

    let mut change_ids: Vec<&str> = packet
        .get("changes")
        .and_then(Value::as_array)
        .unwrap_or(&empty)
        .iter()
        .filter_map(|row| row.get("change_id").and_then(Value::as_str))
        .collect();
    change_ids.sort();
    for change_id in change_ids {
        hasher.update(b"change\0");
        hasher.update(change_id.as_bytes());
        hasher.update(b"\0");
    }

    let mut oracle_tuples: Vec<(&str, &str, &str, &str)> = packet
        .get("oracles")
        .and_then(Value::as_array)
        .unwrap_or(&empty)
        .iter()
        .filter_map(|row| {
            Some((
                row.get("oracle_id")?.as_str()?,
                row.get("target_owner_id")
                    .and_then(Value::as_str)
                    .unwrap_or(""),
                row.get("observed_sink")
                    .and_then(Value::as_str)
                    .unwrap_or(""),
                row.get("expected_expression")
                    .and_then(Value::as_str)
                    .unwrap_or(""),
            ))
        })
        .collect();
    oracle_tuples.sort();
    for (oracle_id, target, sink, expected) in oracle_tuples {
        hasher.update(b"oracle\0");
        hasher.update(oracle_id.as_bytes());
        hasher.update(b"\0");
        hasher.update(target.as_bytes());
        hasher.update(b"\0");
        hasher.update(sink.as_bytes());
        hasher.update(b"\0");
        hasher.update(expected.as_bytes());
        hasher.update(b"\0");
    }

    let mut relation_tuples: Vec<(&str, &str, &str, &str, &str)> = packet
        .get("relations")
        .and_then(Value::as_array)
        .unwrap_or(&empty)
        .iter()
        .filter_map(|row| {
            Some((
                row.get("relation_id")?.as_str()?,
                row.get("change_id")?.as_str()?,
                row.get("owner_id")?.as_str()?,
                row.get("test_id")?.as_str()?,
                row.get("oracle_id").and_then(Value::as_str).unwrap_or(""),
            ))
        })
        .collect();
    relation_tuples.sort();
    for (relation_id, change_id, owner_id, test_id, oracle_id) in relation_tuples {
        hasher.update(b"relation\0");
        hasher.update(relation_id.as_bytes());
        hasher.update(b"\0");
        hasher.update(change_id.as_bytes());
        hasher.update(b"\0");
        hasher.update(owner_id.as_bytes());
        hasher.update(b"\0");
        hasher.update(test_id.as_bytes());
        hasher.update(b"\0");
        hasher.update(oracle_id.as_bytes());
        hasher.update(b"\0");
    }

    format!("sha256:{:x}", hasher.finalize())
}

#[derive(Clone, Debug)]
enum EditOp {
    SectionSet {
        section: &'static str,
        predicate: Map<String, Value>,
        values: Map<String, Value>,
    },
    SectionRemove {
        section: &'static str,
        predicate: Map<String, Value>,
    },
    Append {
        section: &'static str,
        value: Value,
    },
    TopLevel(Map<String, Value>),
    PacketStatusComplete,
    ReplaceText {
        old: String,
        new: String,
    },
}

fn parse_edit_provenance(edits: &str) -> Result<(String, Vec<EditOp>), String> {
    let core = edits.split(". The retained").next().unwrap_or(edits).trim();
    let rest = core
        .strip_prefix(EDIT_PREFIX)
        .ok_or_else(|| "edits must start with `Started from the producer packet of`".to_string())?;
    let (base_case_id, tail) = rest
        .split_once(", then ")
        .ok_or_else(|| "edits must continue with `, then ` after the base case id".to_string())?;
    Ok((base_case_id.trim().to_string(), parse_edit_ops(tail)?))
}

fn parse_edit_ops(tail: &str) -> Result<Vec<EditOp>, String> {
    let mut ops = Vec::new();
    let mut rest = tail.trim();
    while !rest.is_empty() {
        if rest.starts_with("the packet fingerprint") {
            break;
        }
        if let Some(r) = rest.strip_prefix("in oracles where ") {
            rest = r;
            let (pred, pos) = read_json_object(rest)?;
            rest = rest[pos..].trim_start();
            rest = rest
                .strip_prefix("set ")
                .ok_or_else(|| "expected `set` after oracle predicate".to_string())?;
            let (vals, pos) = read_json_object(rest)?;
            rest = rest[pos..]
                .trim_start()
                .trim_start_matches(';')
                .trim_start();
            ops.push(EditOp::SectionSet {
                section: "oracles",
                predicate: pred,
                values: vals,
            });
            continue;
        }
        if let Some(r) = rest.strip_prefix("in relations remove where ") {
            rest = r;
            let (pred, pos) = read_json_object(rest)?;
            rest = rest[pos..]
                .trim_start()
                .trim_start_matches(';')
                .trim_start();
            ops.push(EditOp::SectionRemove {
                section: "relations",
                predicate: pred,
            });
            continue;
        }
        if let Some(r) = rest.strip_prefix("in relations where ") {
            rest = r;
            let (pred, pos) = read_json_object(rest)?;
            rest = rest[pos..].trim_start();
            rest = rest
                .strip_prefix("set ")
                .ok_or_else(|| "expected `set` after relation predicate".to_string())?;
            let (vals, pos) = read_json_object(rest)?;
            rest = rest[pos..]
                .trim_start()
                .trim_start_matches(';')
                .trim_start();
            ops.push(EditOp::SectionSet {
                section: "relations",
                predicate: pred,
                values: vals,
            });
            continue;
        }
        if let Some(r) = rest.strip_prefix("in changes where ") {
            rest = r;
            let (pred, pos) = read_json_object(rest)?;
            rest = rest[pos..].trim_start();
            rest = rest
                .strip_prefix("set ")
                .ok_or_else(|| "expected `set` after change predicate".to_string())?;
            let (vals, pos) = read_json_object(rest)?;
            rest = rest[pos..]
                .trim_start()
                .trim_start_matches(';')
                .trim_start();
            ops.push(EditOp::SectionSet {
                section: "changes",
                predicate: pred,
                values: vals,
            });
            continue;
        }
        if let Some(r) = rest.strip_prefix("in tests where ") {
            rest = r;
            let (pred, pos) = read_json_object(rest)?;
            rest = rest[pos..].trim_start();
            rest = rest
                .strip_prefix("set ")
                .ok_or_else(|| "expected `set` after test predicate".to_string())?;
            let (vals, pos) = read_json_object(rest)?;
            rest = rest[pos..]
                .trim_start()
                .trim_start_matches(';')
                .trim_start();
            ops.push(EditOp::SectionSet {
                section: "tests",
                predicate: pred,
                values: vals,
            });
            continue;
        }
        if let Some(r) = rest.strip_prefix("in owners where ") {
            rest = r;
            let (pred, pos) = read_json_object(rest)?;
            rest = rest[pos..].trim_start();
            rest = rest
                .strip_prefix("set ")
                .ok_or_else(|| "expected `set` after owner predicate".to_string())?;
            let (vals, pos) = read_json_object(rest)?;
            rest = rest[pos..]
                .trim_start()
                .trim_start_matches(';')
                .trim_start();
            ops.push(EditOp::SectionSet {
                section: "owners",
                predicate: pred,
                values: vals,
            });
            continue;
        }
        if let Some(r) = rest.strip_prefix("append to dynamic_boundaries: ") {
            rest = r;
            let (val, pos) = read_json_object(rest)?;
            rest = rest[pos..]
                .trim_start()
                .trim_start_matches(';')
                .trim_start();
            ops.push(EditOp::Append {
                section: "dynamic_boundaries",
                value: Value::Object(val),
            });
            continue;
        }
        if let Some(r) = rest.strip_prefix("append to limitations: ") {
            rest = r;
            let (val, pos) = read_json_object(rest)?;
            rest = rest[pos..]
                .trim_start()
                .trim_start_matches(';')
                .trim_start();
            ops.push(EditOp::Append {
                section: "limitations",
                value: Value::Object(val),
            });
            continue;
        }
        if let Some(r) = rest.strip_prefix("append to relations: ") {
            rest = r;
            let (val, pos) = read_json_object(rest)?;
            rest = rest[pos..]
                .trim_start()
                .trim_start_matches(';')
                .trim_start();
            ops.push(EditOp::Append {
                section: "relations",
                value: Value::Object(val),
            });
            continue;
        }
        if let Some(r) = rest.strip_prefix("set top-level ") {
            rest = r;
            let (val, pos) = read_json_object(rest)?;
            rest = rest[pos..]
                .trim_start()
                .trim_start_matches(';')
                .trim_start();
            ops.push(EditOp::TopLevel(val));
            continue;
        }
        if rest.starts_with("packet_status set to complete") {
            ops.push(EditOp::PacketStatusComplete);
            rest = rest
                .split_once(';')
                .map_or("", |(_, tail)| tail)
                .trim_start();
            continue;
        }
        if let Some(r) = rest.strip_prefix("replace the text `") {
            let Some((old, tail)) = r.split_once("` with `") else {
                return Err("malformed replace-the-text edit".to_string());
            };
            let Some((new, tail)) = tail.split_once("` in every fact") else {
                return Err("malformed replace-the-text edit".to_string());
            };
            ops.push(EditOp::ReplaceText {
                old: old.to_string(),
                new: new.to_string(),
            });
            rest = tail.trim_start().trim_start_matches(';').trim_start();
            continue;
        }
        return Err(format!(
            "unrecognized edit step: {}",
            &rest[..rest.len().min(80)]
        ));
    }
    Ok(ops)
}

fn read_json_object(text: &str) -> Result<(Map<String, Value>, usize), String> {
    let start = text
        .find('{')
        .ok_or_else(|| "expected `{` starting a JSON object".to_string())?;
    let mut depth = 0usize;
    for (offset, ch) in text[start..].char_indices() {
        if ch == '{' {
            depth += 1;
        } else if ch == '}' {
            depth -= 1;
            if depth == 0 {
                let end = start + offset + 1;
                let value: Value = serde_json::from_str(&text[start..end])
                    .map_err(|err| format!("invalid JSON object in edits: {err}"))?;
                let obj = value
                    .as_object()
                    .ok_or_else(|| "edit JSON must be an object".to_string())?
                    .clone();
                return Ok((obj, end));
            }
        }
    }
    Err("unterminated JSON object in edits".to_string())
}

fn change_id(packet: &Value) -> Result<String, String> {
    packet
        .get("changes")
        .and_then(Value::as_array)
        .and_then(|rows| rows.first())
        .and_then(|row| row.get("change_id"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| "base packet has no change_id".to_string())
}

fn change_owner_id(packet: &Value) -> Option<String> {
    packet
        .get("changes")
        .and_then(Value::as_array)
        .and_then(|rows| rows.first())
        .and_then(|row| row.get("owner_id"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn substitute_value(value: Value, packet: &Value) -> Result<Value, String> {
    let change_id = change_id(packet)?;
    let change_owner = change_owner_id(packet);
    match value {
        Value::String(text) => {
            if text == "$CHANGE_ID" {
                return Ok(Value::String(change_id));
            }
            if text == "$CHANGE_OWNER" {
                return Ok(Value::String(change_owner.ok_or_else(|| {
                    "packet has no change owner for $CHANGE_OWNER".to_string()
                })?));
            }
            let mut out = text.replace("$CHANGE_ID", &change_id);
            if let Some(owner) = &change_owner {
                out = out.replace("$CHANGE_OWNER", owner);
            } else if out.contains("$CHANGE_OWNER") {
                return Err("packet has no change owner for $CHANGE_OWNER".to_string());
            }
            Ok(Value::String(out))
        }
        Value::Array(items) => items
            .into_iter()
            .map(|item| substitute_value(item, packet))
            .collect::<Result<_, _>>()
            .map(Value::Array),
        Value::Object(map) => map
            .into_iter()
            .map(|(key, val)| substitute_value(val, packet).map(|v| (key, v)))
            .collect::<Result<_, _>>()
            .map(Value::Object),
        other => Ok(other),
    }
}

fn substitute_map(map: Map<String, Value>, packet: &Value) -> Result<Map<String, Value>, String> {
    substitute_value(Value::Object(map), packet)?
        .as_object()
        .cloned()
        .ok_or_else(|| "substitution must yield an object".to_string())
}

fn row_matches(row: &Value, predicate: &Map<String, Value>) -> bool {
    predicate
        .iter()
        .all(|(key, expected)| row.get(key) == Some(expected))
}

fn apply_ops(packet: &mut Value, ops: &[EditOp]) -> Result<(), String> {
    for op in ops {
        match op {
            EditOp::SectionSet {
                section,
                predicate,
                values,
            } => {
                let predicate = substitute_map(predicate.clone(), packet)?;
                let values = substitute_map(values.clone(), packet)?;
                let Some(rows) = packet.get_mut(*section).and_then(Value::as_array_mut) else {
                    return Err(format!("packet has no `{section}` array"));
                };
                let mut matched = false;
                for row in rows.iter_mut() {
                    if row_matches(row, &predicate) {
                        matched = true;
                        let Some(obj) = row.as_object_mut() else {
                            continue;
                        };
                        for (key, value) in &values {
                            if key == "observed_sink"
                                && value.as_object().is_some_and(|m| {
                                    m.get("strip_spaces") == Some(&Value::Bool(true))
                                })
                            {
                                if let Some(Value::String(sink)) = obj.get("observed_sink") {
                                    obj.insert(
                                        "observed_sink".to_string(),
                                        Value::String(sink.replace(' ', "")),
                                    );
                                } else {
                                    obj.insert(key.clone(), value.clone());
                                }
                            } else {
                                obj.insert(key.clone(), value.clone());
                            }
                        }
                    }
                }
                if !matched {
                    return Err(format!(
                        "no `{section}` row matched predicate {}",
                        serde_json::to_string(&predicate).unwrap_or_default()
                    ));
                }
            }
            EditOp::SectionRemove { section, predicate } => {
                let predicate = substitute_map(predicate.clone(), packet)?;
                let Some(rows) = packet.get_mut(*section).and_then(Value::as_array_mut) else {
                    return Err(format!("packet has no `{section}` array"));
                };
                let before = rows.len();
                rows.retain(|row| !row_matches(row, &predicate));
                if rows.len() == before {
                    return Err(format!(
                        "remove matched no `{section}` row for predicate {}",
                        serde_json::to_string(&predicate).unwrap_or_default()
                    ));
                }
            }
            EditOp::Append { section, value } => {
                let value = substitute_value(value.clone(), packet)?;
                packet
                    .as_object_mut()
                    .ok_or_else(|| "packet must be an object".to_string())?
                    .entry(section.to_string())
                    .or_insert_with(|| Value::Array(Vec::new()))
                    .as_array_mut()
                    .ok_or_else(|| format!("`{section}` must be an array"))?
                    .push(value);
            }
            EditOp::TopLevel(values) => {
                let values = substitute_map(values.clone(), packet)?;
                let Some(obj) = packet.as_object_mut() else {
                    return Err("packet must be an object".to_string());
                };
                for (key, value) in values {
                    obj.insert(key, value);
                }
            }
            EditOp::PacketStatusComplete => {
                packet
                    .as_object_mut()
                    .ok_or_else(|| "packet must be an object".to_string())?
                    .insert(
                        "packet_status".to_string(),
                        Value::String("complete".to_string()),
                    );
            }
            EditOp::ReplaceText { old, new } => {
                *packet = substitute_text_in_value(packet.clone(), old, new);
            }
        }
    }
    Ok(())
}

fn substitute_text_in_value(value: Value, old: &str, new: &str) -> Value {
    match value {
        Value::String(text) => Value::String(text.replace(old, new)),
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .map(|item| substitute_text_in_value(item, old, new))
                .collect(),
        ),
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, val)| (key, substitute_text_in_value(val, old, new)))
                .collect(),
        ),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reports::verdict_corpus::load_corpus;

    #[test]
    fn committed_perl_edited_producer_packets_replay_from_base_cases() -> Result<(), String> {
        let dir = crate::dogfood::repo_rooted_fixture_path("fixtures/perl-verdict-corpus");
        let corpus = load_corpus(&dir)?;
        let index: BTreeMap<&str, &Case> = corpus
            .cases
            .iter()
            .map(|case| (case.case_id.as_str(), case))
            .collect();
        for case in &corpus.cases {
            let Some(facts) = &case.perl_facts else {
                continue;
            };
            if facts.provenance != PacketProvenance::EditedProducer {
                continue;
            }
            let violations = edited_producer_packet_violations(&case.case_id, facts, &dir, &index);
            assert!(violations.is_empty(), "{}", violations.join("\n"));
        }
        Ok(())
    }

    #[test]
    fn stale_edited_producer_packet_fails_edit_replay() -> Result<(), String> {
        let dir = crate::dogfood::repo_rooted_fixture_path("fixtures/perl-verdict-corpus");
        let corpus = load_corpus(&dir)?;
        let index: BTreeMap<&str, &Case> = corpus
            .cases
            .iter()
            .map(|case| (case.case_id.as_str(), case))
            .collect();
        let case = corpus
            .cases
            .iter()
            .find(|case| case.case_id == "perl-0235-ex01-sink-unconfirmed")
            .ok_or_else(|| "fixture case missing".to_string())?;
        let facts = case
            .perl_facts
            .as_ref()
            .ok_or_else(|| "missing perl_facts".to_string())?;
        let packet_path = dir.join(&facts.packet);
        let mut packet: Value =
            serde_json::from_str(&fs::read_to_string(&packet_path).map_err(|err| err.to_string())?)
                .map_err(|err| err.to_string())?;
        let Some(oracles) = packet.get_mut("oracles").and_then(Value::as_array_mut) else {
            return Err("fixture packet has no oracles".to_string());
        };
        for oracle in oracles {
            if oracle.get("oracle_id").and_then(Value::as_str)
                == Some("oracle:t/qualified.t:is:130-198")
            {
                oracle
                    .as_object_mut()
                    .ok_or_else(|| "oracle not object".to_string())?
                    .insert(
                        "observed_sink".to_string(),
                        Value::String("stale".to_string()),
                    );
                break;
            }
        }
        let stale_text =
            serde_json::to_string_pretty(&packet).map_err(|err| err.to_string())? + "\n";
        let temp_root = std::env::temp_dir().join("ripr-verdict-corpus-edit-replay-test");
        let _ = fs::remove_dir_all(&temp_root);
        let temp_dir = temp_root.join("perl-verdict-corpus");
        copy_dir_all(&dir, &temp_dir)?;
        fs::write(temp_dir.join(&facts.packet), stale_text).map_err(|err| err.to_string())?;
        let violations = edited_producer_packet_violations(&case.case_id, facts, &temp_dir, &index);
        assert!(
            violations
                .iter()
                .any(|v| v.contains("does not match") || v.contains("stale")),
            "{violations:?}"
        );
        Ok(())
    }

    #[test]
    fn producer_pin_move_without_edited_packet_refresh_fails_replay() -> Result<(), String> {
        let dir = crate::dogfood::repo_rooted_fixture_path("fixtures/perl-verdict-corpus");
        let corpus = load_corpus(&dir)?;
        let index: BTreeMap<&str, &Case> = corpus
            .cases
            .iter()
            .map(|case| (case.case_id.as_str(), case))
            .collect();
        let case = corpus
            .cases
            .iter()
            .find(|case| case.case_id == "perl-0235-ex01-sink-unconfirmed")
            .ok_or_else(|| "fixture case missing".to_string())?;
        let facts = case
            .perl_facts
            .as_ref()
            .ok_or_else(|| "missing perl_facts".to_string())?;
        let edits = facts
            .edits
            .as_deref()
            .ok_or_else(|| "missing edits".to_string())?;
        let (base_case_id, _) = parse_edit_provenance(edits)?;
        let base_case = index
            .get(base_case_id.as_str())
            .ok_or_else(|| "missing base case".to_string())?;
        let base_facts = base_case
            .perl_facts
            .as_ref()
            .ok_or_else(|| "missing base perl_facts".to_string())?;
        let temp_root = std::env::temp_dir().join("ripr-verdict-corpus-pin-move-test");
        let _ = fs::remove_dir_all(&temp_root);
        let temp_dir = temp_root.join("perl-verdict-corpus");
        copy_dir_all(&dir, &temp_dir)?;
        let base_path = temp_dir.join(&base_facts.packet);
        let mut base_packet: Value =
            serde_json::from_str(&fs::read_to_string(&base_path).map_err(|err| err.to_string())?)
                .map_err(|err| err.to_string())?;
        let Some(limitations) = base_packet
            .get_mut("limitations")
            .and_then(Value::as_array_mut)
        else {
            return Err("base packet has no limitations".to_string());
        };
        limitations.push(Value::Object(Map::from_iter([
            (
                "limitation_id".to_string(),
                Value::String("simulated-post-pin-drift".to_string()),
            ),
            (
                "kind".to_string(),
                Value::String("simulated_producer_pin_refresh".to_string()),
            ),
            ("message".to_string(), Value::String("drift".to_string())),
            ("evidence_refs".to_string(), Value::Array(Vec::new())),
        ])));
        fs::write(
            &base_path,
            serde_json::to_string_pretty(&base_packet).map_err(|err| err.to_string())? + "\n",
        )
        .map_err(|err| err.to_string())?;
        let violations = edited_producer_packet_violations(&case.case_id, facts, &temp_dir, &index);
        assert!(
            violations.iter().any(|v| v.contains("does not match")),
            "{violations:?}"
        );
        Ok(())
    }

    fn copy_dir_all(from: &Path, to: &Path) -> Result<(), String> {
        fs::create_dir_all(to).map_err(|err| err.to_string())?;
        for entry in fs::read_dir(from).map_err(|err| err.to_string())? {
            let entry = entry.map_err(|err| err.to_string())?;
            let dest = to.join(entry.file_name());
            if entry.path().is_dir() {
                copy_dir_all(&entry.path(), &dest)?;
            } else {
                fs::copy(entry.path(), dest).map_err(|err| err.to_string())?;
            }
        }
        Ok(())
    }
}
