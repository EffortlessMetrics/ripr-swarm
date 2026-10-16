//! #8193: keep JSON dependencies outside the bare-rustc portable help audit.

use serde_json::{Value, json};

const GROUPS: &[&str] = &[
    "cache",
    "baseline",
    "policy",
    "reports",
    "calibrate",
    "zero",
    "assistant-loop",
    "config",
    "pr-ledger",
    "pr-review",
    "coverage-grip",
    "gate",
];

pub(super) fn assert_bare_group_rows(bytes: &[u8]) -> Result<(), String> {
    let document: Value = serde_json::from_slice(bytes)
        .map_err(|error| format!("invalid help discovery JSON: {error}"))?;
    let rows = document
        .get("commands")
        .and_then(Value::as_array)
        .ok_or("help discovery commands must be an array")?;
    if rows.iter().any(|row| !row.is_object()) {
        return Err("help discovery commands must contain objects".to_string());
    }
    for &group in GROUPS {
        let id = format!("cmd:{group}");
        let matching: Vec<_> = rows
            .iter()
            .filter(|row| row["path"] == group || row["id"] == id)
            .collect();
        if matching.len() != 1 {
            return Err(format!(
                "expected exactly one {group} discovery row, found {}",
                matching.len()
            ));
        }
        let row = matching[0];
        if row["path"] != group || row["id"] != id || row["example"] != format!("ripr {group}") {
            return Err(format!(
                "wrong identity/example on {group} discovery row: {row}"
            ));
        }
    }
    Ok(())
}

fn document() -> Value {
    json!({"commands": GROUPS.iter().map(|group| json!({
        "id": format!("cmd:{group}"), "path": group, "example": format!("ripr {group}")
    })).collect::<Vec<_>>()})
}

fn check(document: &Value) -> Result<(), String> {
    assert_bare_group_rows(&serde_json::to_vec(document).map_err(|error| error.to_string())?)
}

#[test]
fn reordered_rows_and_fields_keep_their_own_examples() -> Result<(), String> {
    let mut document = document();
    let rows = document["commands"]
        .as_array_mut()
        .ok_or("fixture commands")?;
    rows.reverse();
    check(&document)?;
    // Whitespace and object-field order are independent of row identity.
    let rows = GROUPS
        .iter()
        .rev()
        .map(|group| format!(r#"{{"example":"ripr {group}","path":"{group}","id":"cmd:{group}"}}"#))
        .collect::<Vec<_>>()
        .join(",\n");
    assert_bare_group_rows(format!("{{\"commands\": [\n{rows}\n]}}").as_bytes())
}

#[test]
fn swapped_examples_are_rejected_even_when_both_global_needles_remain() -> Result<(), String> {
    let mut document = document();
    document["commands"][0]["example"] = json!("ripr policy");
    document["commands"][2]["example"] = json!("ripr cache");
    let text = serde_json::to_string(&document).map_err(|error| error.to_string())?;
    for group in ["cache", "policy"] {
        assert!(text.contains(&format!(r#""path":"{group}""#)));
        assert!(text.contains(&format!(r#""example":"ripr {group}""#)));
    }
    assert!(assert_bare_group_rows(text.as_bytes()).is_err());
    Ok(())
}

#[test]
fn discovery_rows_reject_wrong_identity_duplicates_missing_and_decoys() -> Result<(), String> {
    let mut wrong_id = document();
    wrong_id["commands"][0]["id"] = json!("cmd:policy");
    let mut duplicate = document();
    let cache = duplicate["commands"][0].clone();
    duplicate["commands"]
        .as_array_mut()
        .ok_or("fixture commands")?
        .push(cache);
    let mut missing = document();
    let cache = missing["commands"]
        .as_array_mut()
        .ok_or("fixture commands")?
        .remove(0);
    // A correct row in another section cannot rescue the missing command.
    missing["workflows"] = json!([cache]);
    let mut nested = document();
    nested["commands"][0] = json!({"path":"cache", "id":"cmd:cache",
        "decoy":{"example":"ripr cache"}});
    let mut null = document();
    null["commands"][0]["example"] = Value::Null;
    for (name, document) in [
        ("wrong id", wrong_id),
        ("duplicate", duplicate),
        ("missing command", missing),
        ("nested example", nested),
        ("null example", null),
    ] {
        assert!(check(&document).is_err(), "accepted {name}");
    }
    Ok(())
}

#[test]
fn discovery_rows_reject_invalid_documents() -> Result<(), String> {
    let valid = serde_json::to_vec(&document()).map_err(|error| error.to_string())?;
    assert!(assert_bare_group_rows(&valid[..valid.len() - 1]).is_err());
    for text in [
        "",
        "null",
        "{}",
        r#"{"commands":{}}"#,
        r#"{"commands":[]}"#,
        "not JSON",
    ] {
        assert!(
            assert_bare_group_rows(text.as_bytes()).is_err(),
            "accepted {text}"
        );
    }
    Ok(())
}
