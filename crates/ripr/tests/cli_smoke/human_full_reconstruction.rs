//! Independent text reconstruction: definitions must resolve to exactly their
//! visible users, in order. The expected record is the frozen legacy snapshot,
//! not a production sharing key or a set of selected wording assertions.

use std::collections::{BTreeMap, BTreeSet};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub(super) fn reconstruct(report: &str) -> Result<String> {
    let mut catalogue = BTreeMap::<String, (Vec<usize>, String)>::new();
    let mut users = BTreeMap::<String, BTreeSet<usize>>::new();
    let mut last_definition = None;
    let mut in_catalogue = false;
    let mut finding = None;
    let mut next_step = false;
    let mut drill_in = false;
    let mut out = String::new();
    for line in report.split_inclusive('\n') {
        if line == "Shared evidence and guidance\n" {
            in_catalogue = true;
            continue;
        }
        if in_catalogue {
            if line == "\n" {
                in_catalogue = false;
                continue;
            }
            if let Some((label, remainder)) = line
                .strip_prefix("  ")
                .and_then(|line| line.split_once(" [findings "))
            {
                assert!(label.starts_with('E') || label.starts_with('N'));
                let (members, payload) = remainder.split_once("]: ").ok_or("invalid definition")?;
                let members = members
                    .split(", ")
                    .map(str::parse)
                    .collect::<std::result::Result<Vec<usize>, _>>()?;
                assert!(members.len() > 1);
                let prefix = if label.starts_with('E') { "  - " } else { "  " };
                assert!(
                    catalogue
                        .insert(label.to_string(), (members, format!("{prefix}{payload}")))
                        .is_none(),
                    "duplicate definition"
                );
                last_definition = Some(label.to_string());
            } else {
                assert!(line.starts_with("    "), "invalid catalogue continuation");
                let key = last_definition.as_ref().ok_or("orphan continuation")?;
                catalogue
                    .get_mut(key)
                    .ok_or("missing definition")?
                    .1
                    .push_str(line);
            }
            continue;
        }
        if let Some((ordinal, header)) = line
            .strip_prefix("Finding ")
            .and_then(|line| line.split_once(": "))
        {
            let ordinal: usize = ordinal.parse()?;
            assert_eq!(ordinal, finding.unwrap_or(0) + 1, "finding order");
            finding = Some(ordinal);
            drill_in = false;
            next_step = false;
            out.push_str(header);
            continue;
        }
        if line == "Drill in:\n" {
            drill_in = true;
            continue;
        }
        if drill_in {
            if line == "\n" {
                drill_in = false;
                out.push_str(line);
            } else {
                assert!(line.starts_with("  "), "malformed drill-in block");
            }
            continue;
        }
        if let Some((labels, guidance)) = line
            .strip_prefix("  shared evidence: ")
            .map(|text| (text, false))
            .or_else(|| {
                line.strip_prefix("  shared guidance: ")
                    .map(|text| (text, true))
            })
        {
            let ordinal = finding.ok_or("reference outside finding")?;
            for label in labels.trim_end_matches('\n').split(", ") {
                assert_eq!(label.starts_with('N'), guidance, "reference kind");
                let (members, payload) = catalogue.get(label).ok_or("unresolved reference")?;
                assert!(
                    members.contains(&ordinal),
                    "reference outside declared owner"
                );
                users.entry(label.to_string()).or_default().insert(ordinal);
                out.push_str(payload);
            }
            continue;
        }
        if line == "Next step\n" {
            next_step = true;
        } else if line == "\n" {
            next_step = false;
        }
        if next_step && let Some(text) = line.strip_prefix("  - ") {
            out.push_str("  ");
            out.push_str(text);
        } else {
            out.push_str(line);
        }
    }
    for (label, (members, _)) in &catalogue {
        assert_eq!(
            users.get(label).ok_or("unused definition")?,
            &members.iter().copied().collect(),
            "exact visible users"
        );
    }
    Ok(out)
}

pub(super) fn definition_members(report: &str, text: &str) -> Result<Vec<Vec<usize>>> {
    let mut result = Vec::new();
    for line in report.lines() {
        let Some((_, remaining)) = line
            .strip_prefix("  ")
            .and_then(|line| line.split_once(" [findings "))
        else {
            continue;
        };
        let Some((members, payload)) = remaining.split_once("]: ") else {
            continue;
        };
        if payload.starts_with(text) {
            result.push(
                members
                    .split(", ")
                    .map(str::parse)
                    .collect::<std::result::Result<Vec<usize>, _>>()?,
            );
        }
    }
    Ok(result)
}

/// Empty section separators are the only CLI spacing change accepted by this
/// exhaustive content oracle. Every other byte and ordered line is retained.
pub(super) fn content_lines(report: &str) -> String {
    report
        .split_inclusive('\n')
        .filter(|line| *line != "\n")
        .collect()
}
