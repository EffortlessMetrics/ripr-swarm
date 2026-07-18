use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

const INVENTORY: &str = "fixtures/active-goal-authority-audit/consumers.toml";
const OCCURRENCE_INVENTORY: &str = "fixtures/active-goal-authority-audit/occurrences.json";
const ISSUE_CONTRACTS: &str = "fixtures/active-goal-authority-audit/issue-contracts.json";
const MARKERS: [&str; 4] = [
    ".ripr/goals/active.toml",
    "active_goal",
    "active goal",
    "current goal",
];
const CLASSIFICATIONS: [&str; 8] = [
    "historical_campaign_evidence",
    "stable_campaign_definition",
    "explicit_campaign_filter",
    "live_selection_or_authorization_remove",
    "compatibility_reader_only",
    "derived_current_view",
    "obsolete",
    "unknown_needs_decision",
];

#[derive(Clone, Debug, Default)]
struct Rule {
    id: String,
    selector: String,
    classification: String,
    owner: String,
    dependent_issue: String,
    compatibility_period: String,
    current_behavior: String,
    fields: String,
    authority: String,
    target_behavior: String,
    positive_proof: String,
    negative_proof: String,
}

#[derive(Clone, Debug, Default)]
struct OccurrenceRule {
    path: String,
    anchor: String,
    marker_kind: String,
    normalized_marker_hash: String,
    consumer_id: String,
}

#[derive(Debug)]
struct Audit {
    rows: Vec<AuditRow>,
    unclassified: Vec<String>,
    unclassified_occurrences: Vec<Occurrence>,
    unused_rules: Vec<String>,
    contradictions: Vec<String>,
    semantic_digest: String,
    framework_blockers: Vec<String>,
    occurrence_inventory_ready: bool,
    stale_occurrence_rows: Vec<String>,
}

#[derive(Clone, Debug)]
struct AuditRow {
    identity: String,
    path: String,
    anchor: Option<String>,
    marker_kind: Option<String>,
    normalized_marker_hash: Option<String>,
    rule: Rule,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct Occurrence {
    path: String,
    anchor: String,
    marker_kind: String,
    normalized_marker_hash: String,
}

#[derive(Debug)]
struct Discovery {
    occurrences: Vec<Occurrence>,
    read_failures: Vec<String>,
}

impl Occurrence {
    fn identity(&self) -> String {
        format!(
            "{}|{}|{}|{}",
            self.path, self.anchor, self.marker_kind, self.normalized_marker_hash
        )
    }
}

impl OccurrenceRule {
    fn identity(&self) -> String {
        format!(
            "{}|{}|{}|{}",
            self.path, self.anchor, self.marker_kind, self.normalized_marker_hash
        )
    }
}

pub(crate) fn run() -> Result<(), String> {
    run_at(Path::new("."), Path::new("target/ripr/reports"))
}

pub(crate) fn run_at(root: &Path, reports: &Path) -> Result<(), String> {
    let audit = audit_root(root)?;
    fs::create_dir_all(reports).map_err(|err| format!("create reports directory: {err}"))?;
    fs::write(
        reports.join("active-goal-authority-audit.json"),
        render_json(&audit)?,
    )
    .map_err(|err| format!("write authority audit JSON: {err}"))?;
    fs::write(
        reports.join("active-goal-authority-audit.md"),
        render_markdown(&audit),
    )
    .map_err(|err| format!("write authority audit Markdown: {err}"))?;
    println!(
        "active-goal authority audit: {} consumers, {} blockers",
        audit.rows.len(),
        audit.unclassified.len() + audit.contradictions.len() + audit.framework_blockers.len()
    );
    if audit.unclassified.is_empty()
        && audit.contradictions.is_empty()
        && audit.framework_blockers.is_empty()
    {
        Ok(())
    } else {
        Err(format!(
            "active-goal authority audit blocked: {} unclassified discoveries, {} contradictions, {} framework blockers; inspect {}",
            audit.unclassified.len(),
            audit.contradictions.len(),
            audit.framework_blockers.len(),
            reports.join("active-goal-authority-audit.json").display()
        ))
    }
}

fn audit_root(root: &Path) -> Result<Audit, String> {
    let (rules, inline_occurrence_rules) = parse_inventory(
        &fs::read_to_string(root.join(INVENTORY))
            .map_err(|err| format!("read {INVENTORY}: {err}"))?,
    )?;
    if !inline_occurrence_rules.is_empty() {
        return Err(format!(
            "{INVENTORY} must keep occurrence rows in {OCCURRENCE_INVENTORY}"
        ));
    }
    let occurrence_rules = parse_occurrence_inventory(
        &fs::read_to_string(root.join(OCCURRENCE_INVENTORY))
            .map_err(|err| format!("read {OCCURRENCE_INVENTORY}: {err}"))?,
    )?;
    let rules_by_id: BTreeMap<&str, &Rule> =
        rules.iter().map(|rule| (rule.id.as_str(), rule)).collect();
    let issue_rows = issue_contract_rows(root)?;
    let discovery = discover(root)?;
    let mut occurrence_identities = discovery
        .occurrences
        .iter()
        .map(Occurrence::identity)
        .collect::<Vec<_>>();
    occurrence_identities.sort();
    let mut duplicate_occurrences = Vec::new();
    for pair in occurrence_identities.windows(2) {
        if pair[0] == pair[1] {
            duplicate_occurrences.push(format!("duplicate exact occurrence identity {}", pair[0]));
        }
    }
    let reviewed_by_identity: BTreeMap<String, &OccurrenceRule> = occurrence_rules
        .iter()
        .map(|row| (row.identity(), row))
        .collect();
    let mut rows = Vec::new();
    let mut used = BTreeSet::new();
    let mut used_occurrences = BTreeSet::new();
    let mut unclassified = Vec::new();
    let mut unclassified_occurrences = Vec::new();
    let mut contradictions = duplicate_occurrences;
    contradictions.extend(discovery.read_failures);
    for occurrence in discovery.occurrences {
        let identity = occurrence.identity();
        let Some(reviewed) = reviewed_by_identity.get(&identity) else {
            unclassified.push(identity);
            unclassified_occurrences.push(occurrence);
            continue;
        };
        used_occurrences.insert(identity.clone());
        let Some(rule) = rules_by_id.get(reviewed.consumer_id.as_str()) else {
            contradictions.push(format!(
                "{identity} references unknown consumer {}",
                reviewed.consumer_id
            ));
            continue;
        };
        used.insert(rule.id.clone());
        if rule.classification == "unknown_needs_decision" {
            contradictions.push(format!("{identity} remains unknown under {}", rule.id));
        }
        rows.push(AuditRow {
            identity,
            path: occurrence.path,
            anchor: Some(occurrence.anchor),
            marker_kind: Some(occurrence.marker_kind),
            normalized_marker_hash: Some(occurrence.normalized_marker_hash),
            rule: (*rule).clone(),
        });
    }
    rows.extend(issue_rows);
    rows.sort_by(|left, right| left.identity.cmp(&right.identity));
    unclassified.sort();
    unclassified_occurrences.sort();
    contradictions.sort();
    let stale_occurrence_rows: Vec<String> = occurrence_rules
        .iter()
        .map(OccurrenceRule::identity)
        .filter(|identity| !used_occurrences.contains(identity))
        .collect();
    let unused_rules: Vec<String> = rules
        .iter()
        .filter(|rule| !used.contains(&rule.id))
        .map(|rule| rule.id.clone())
        .collect();
    let occurrence_inventory_ready =
        unclassified.is_empty() && contradictions.is_empty() && stale_occurrence_rows.is_empty();
    let mut framework_blockers = Vec::new();
    if !occurrence_inventory_ready {
        framework_blockers.push("occurrence_inventory_not_proven: every discovered live occurrence must match one reviewed (path, anchor, marker_kind, normalized_marker_hash) row, with no stale or duplicate identities".to_string());
    }
    framework_blockers.push("issue_snapshots_not_proven: current individual snapshots are required for #1631-#1639, #1643-#1650, #1692, #1697, and #1701; range aggregates are not migration evidence".to_string());
    let mut semantic = semantic_text(
        &rows,
        &unclassified,
        &unused_rules,
        &stale_occurrence_rows,
        &contradictions,
    );
    for blocker in &framework_blockers {
        semantic.push_str(&format!("framework_blocker|{blocker}\n"));
    }
    let semantic_digest = format!("{:x}", Sha256::digest(semantic.as_bytes()));
    Ok(Audit {
        rows,
        unclassified,
        unclassified_occurrences,
        unused_rules,
        contradictions,
        semantic_digest,
        framework_blockers,
        occurrence_inventory_ready,
        stale_occurrence_rows,
    })
}

fn issue_contract_rows(root: &Path) -> Result<Vec<AuditRow>, String> {
    let text = fs::read_to_string(root.join(ISSUE_CONTRACTS))
        .map_err(|err| format!("read {ISSUE_CONTRACTS}: {err}"))?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|err| format!("parse {ISSUE_CONTRACTS}: {err}"))?;
    let contracts = value
        .get("contracts")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| format!("{ISSUE_CONTRACTS} must contain contracts[]"))?;
    if contracts.is_empty() {
        return Err(format!(
            "{ISSUE_CONTRACTS} must capture at least one issue contract"
        ));
    }
    let mut rows = Vec::new();
    let mut seen_issues = BTreeSet::new();
    for contract in contracts {
        for field in [
            "issue",
            "classification",
            "owner",
            "dependent_issue",
            "compatibility_period",
            "current_behavior",
            "fields",
            "authority",
            "target_behavior",
            "positive_proof",
            "negative_proof",
        ] {
            if contract
                .get(field)
                .and_then(serde_json::Value::as_str)
                .is_none_or(str::is_empty)
            {
                return Err(format!("issue contract is missing non-empty {field}"));
            }
        }
        let get = |field: &str| {
            contract
                .get(field)
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let issue = get("issue");
        if !seen_issues.insert(issue.clone()) {
            return Err(format!("duplicate issue contract identity {issue}"));
        }
        let rule = validate_rule(Rule {
            id: format!("issue-contract-{issue}"),
            selector: format!("github:{issue}"),
            classification: get("classification"),
            owner: get("owner"),
            dependent_issue: get("dependent_issue"),
            compatibility_period: get("compatibility_period"),
            current_behavior: get("current_behavior"),
            fields: get("fields"),
            authority: get("authority"),
            target_behavior: get("target_behavior"),
            positive_proof: get("positive_proof"),
            negative_proof: get("negative_proof"),
        })?;
        let identity = format!("github:{issue}");
        rows.push(AuditRow {
            identity: identity.clone(),
            path: identity,
            anchor: None,
            marker_kind: None,
            normalized_marker_hash: None,
            rule,
        });
    }
    Ok(rows)
}

fn discover(root: &Path) -> Result<Discovery, String> {
    let mut read_failures = Vec::new();
    if root.join(".git").exists() {
        let mut paths = Vec::new();
        for normalized in tracked_paths(root)? {
            if super::should_skip_path(&normalized) || normalized == OCCURRENCE_INVENTORY {
                continue;
            }
            if !is_auditable_text_path(&normalized) {
                continue;
            }
            let path = root.join(&normalized);
            let text = match fs::read_to_string(path) {
                Ok(text) => text,
                Err(err) => {
                    read_failures.push(format!(
                        "tracked text {normalized} could not be read as UTF-8: {err}"
                    ));
                    continue;
                }
            };
            paths.extend(discover_text_occurrences(&normalized, &text)?);
        }
        paths.sort();
        read_failures.sort();
        return Ok(Discovery {
            occurrences: paths,
            read_failures,
        });
    }
    let mut paths = Vec::new();
    walk(root, root, &mut paths, &mut read_failures)?;
    paths.sort();
    read_failures.sort();
    Ok(Discovery {
        occurrences: paths,
        read_failures,
    })
}

fn tracked_paths(root: &Path) -> Result<Vec<String>, String> {
    let output = crate::run::run_output_bytes_in_dir("git", &["ls-files", "-z"], root)?;
    parse_tracked_paths(&output)
}

fn parse_tracked_paths(output: &[u8]) -> Result<Vec<String>, String> {
    output
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| {
            std::str::from_utf8(path)
                .map(str::to_string)
                .map_err(|err| format!("git ls-files path was not UTF-8: {err}"))
        })
        .collect()
}

fn walk(
    root: &Path,
    directory: &Path,
    found: &mut Vec<Occurrence>,
    read_failures: &mut Vec<String>,
) -> Result<(), String> {
    let mut entries: Vec<_> = fs::read_dir(directory)
        .map_err(|err| format!("read directory {}: {err}", directory.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("read directory entry: {err}"))?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .map_err(|err| format!("strip root: {err}"))?;
        let normalized = normalize_path(relative)?;
        if normalized == OCCURRENCE_INVENTORY {
            continue;
        }
        if path.is_dir() {
            if normalized == ".git" || normalized == "target" || normalized.starts_with(".git/") {
                continue;
            }
            walk(root, &path, found, read_failures)?;
        } else if path.is_file() {
            if !is_auditable_text_path(&normalized) {
                continue;
            }
            let text = match fs::read_to_string(&path) {
                Ok(text) => text,
                Err(err) => {
                    read_failures.push(format!(
                        "tracked text {normalized} could not be read as UTF-8: {err}"
                    ));
                    continue;
                }
            };
            found.extend(discover_text_occurrences(&normalized, &text)?);
        }
    }
    Ok(())
}

fn is_auditable_text_path(path: &str) -> bool {
    matches!(
        Path::new(path)
            .extension()
            .and_then(|value| value.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some(
            "rs" | "md"
                | "toml"
                | "json"
                | "yml"
                | "yaml"
                | "txt"
                | "lock"
                | "spec"
                | "diff"
                | "html"
                | "ts"
                | "js"
                | "py"
                | "pl"
                | "sh"
                | "ps1"
                | "xml"
                | "csv"
        )
    )
}

fn normalize_path(path: &Path) -> Result<String, String> {
    path.to_str()
        .map(|value| value.replace('\\', "/"))
        .ok_or_else(|| format!("path is not UTF-8: {}", path.display()))
}

fn discover_text_occurrences(path: &str, text: &str) -> Result<Vec<Occurrence>, String> {
    let mut occurrences = Vec::new();
    let mut anchor = "file".to_string();
    let mut structural_context = String::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(next) = syntax_anchor(path, trimmed, &mut structural_context) {
            anchor = next;
        }
        let lowered = trimmed.to_ascii_lowercase();
        let normalized_line = lowered.split_whitespace().collect::<Vec<_>>().join(" ");
        let local_discriminator = format!("{:x}", Sha256::digest(normalized_line.as_bytes()));
        for marker in MARKERS {
            for _ in lowered.match_indices(marker) {
                occurrences.push(Occurrence {
                    path: path.to_string(),
                    anchor: format!("{anchor}::{local_discriminator}"),
                    marker_kind: marker_kind(marker)?.to_string(),
                    normalized_marker_hash: format!("{:x}", Sha256::digest(marker.as_bytes())),
                });
            }
        }
    }
    let mut totals = BTreeMap::new();
    for occurrence in &occurrences {
        *totals
            .entry((
                occurrence.anchor.clone(),
                occurrence.marker_kind.clone(),
                occurrence.normalized_marker_hash.clone(),
            ))
            .or_insert(0usize) += 1;
    }
    let mut ordinals = BTreeMap::new();
    for occurrence in &mut occurrences {
        let key = (
            occurrence.anchor.clone(),
            occurrence.marker_kind.clone(),
            occurrence.normalized_marker_hash.clone(),
        );
        let ordinal = ordinals.entry(key).or_insert(0usize);
        *ordinal += 1;
        if totals
            .get(&(
                occurrence.anchor.clone(),
                occurrence.marker_kind.clone(),
                occurrence.normalized_marker_hash.clone(),
            ))
            .is_some_and(|total| *total > 1)
        {
            occurrence.anchor = format!("{}@{}", occurrence.anchor, ordinal);
        }
    }
    Ok(occurrences)
}

fn marker_kind(marker: &str) -> Result<&'static str, String> {
    match marker {
        ".ripr/goals/active.toml" => Ok("singleton_path"),
        "active_goal" => Ok("identifier"),
        "active goal" => Ok("phrase_active_goal"),
        "current goal" => Ok("phrase_current_goal"),
        other => Err(format!("unknown active-goal marker kind {other}")),
    }
}

fn syntax_anchor(path: &str, line: &str, structural_context: &mut String) -> Option<String> {
    if path.ends_with(".rs") {
        let declaration = ["pub(crate) ", "pub(super) ", "pub "]
            .into_iter()
            .find_map(|prefix| line.strip_prefix(prefix))
            .unwrap_or(line);
        for prefix in [
            "async fn ",
            "fn ",
            "const ",
            "static ",
            "struct ",
            "enum ",
            "mod ",
            "impl ",
        ] {
            if let Some(rest) = declaration.strip_prefix(prefix) {
                let name = rest
                    .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
                    .next()?;
                if !name.is_empty() {
                    return Some(format!("rust:{prefix}{name}"));
                }
            }
        }
    } else if path.ends_with(".md") && line.starts_with('#') {
        return Some(format!(
            "markdown:{}",
            line.trim_start_matches('#').trim().to_ascii_lowercase()
        ));
    } else {
        match Path::new(path).extension().and_then(|value| value.to_str()) {
            Some("toml") => {
                if line.starts_with('[') {
                    *structural_context = format!("toml:table:{}", line.to_ascii_lowercase());
                    return Some(structural_context.clone());
                }
                if let Some((key, _)) = line.split_once('=') {
                    let key = key.trim();
                    if !is_config_key(key) {
                        return None;
                    }
                    let normalized_key = key.trim_matches('"').to_ascii_lowercase();
                    if normalized_key == "id"
                        && let Some((_, value)) = line.split_once('=')
                    {
                        let identity = value.trim().trim_matches(['"', ',']).to_ascii_lowercase();
                        if !identity.is_empty() {
                            let table = structural_context
                                .split("/id:")
                                .next()
                                .unwrap_or(structural_context);
                            *structural_context = format!("{table}/id:{identity}");
                        }
                    }
                    return Some(if structural_context.is_empty() {
                        format!("toml:key:{normalized_key}")
                    } else {
                        format!("{structural_context}/key:{normalized_key}")
                    });
                }
            }
            Some("json") => {
                if let Some((key, _)) = line.split_once(':') {
                    let key = key.trim();
                    if is_config_key(key) {
                        return Some(format!(
                            "json:key:{}",
                            key.trim_matches('"').to_ascii_lowercase()
                        ));
                    }
                }
            }
            Some("yml" | "yaml") => {
                if let Some((key, _)) = line.split_once(':') {
                    let key = key.trim();
                    if is_config_key(key) {
                        return Some(format!(
                            "yaml:key:{}",
                            key.trim_matches('"').to_ascii_lowercase()
                        ));
                    }
                }
            }
            _ => {}
        }
    }
    None
}

fn is_config_key(value: &str) -> bool {
    let unquoted = value.trim_matches('"');
    !unquoted.is_empty()
        && unquoted
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
}

fn parse_inventory(text: &str) -> Result<(Vec<Rule>, Vec<OccurrenceRule>), String> {
    enum Section {
        Consumer(Rule),
        Occurrence(OccurrenceRule),
    }

    let mut rules = Vec::new();
    let mut occurrences = Vec::new();
    let mut current: Option<Section> = None;
    let finish = |current: Section,
                  rules: &mut Vec<Rule>,
                  occurrences: &mut Vec<OccurrenceRule>|
     -> Result<(), String> {
        match current {
            Section::Consumer(rule) => rules.push(validate_rule(rule)?),
            Section::Occurrence(row) => occurrences.push(validate_occurrence_rule(row)?),
        }
        Ok(())
    };
    for (index, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if matches!(line, "[[consumer]]" | "[[occurrence]]") {
            if let Some(section) = current.take() {
                finish(section, &mut rules, &mut occurrences)?;
            }
            current = Some(if line == "[[consumer]]" {
                Section::Consumer(Rule::default())
            } else {
                Section::Occurrence(OccurrenceRule::default())
            });
            continue;
        }
        let Some(section) = current.as_mut() else {
            return Err(format!(
                "inventory line {} is outside an inventory table",
                index + 1
            ));
        };
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| format!("invalid inventory line {}", index + 1))?;
        let value = value
            .trim()
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .ok_or_else(|| format!("inventory line {} must use a quoted value", index + 1))?;
        match section {
            Section::Consumer(rule) => match key.trim() {
                "id" => rule.id = value.to_string(),
                "selector" => rule.selector = value.to_string(),
                "classification" => rule.classification = value.to_string(),
                "owner" => rule.owner = value.to_string(),
                "dependent_issue" => rule.dependent_issue = value.to_string(),
                "compatibility_period" => rule.compatibility_period = value.to_string(),
                "current_behavior" => rule.current_behavior = value.to_string(),
                "fields" => rule.fields = value.to_string(),
                "authority" => rule.authority = value.to_string(),
                "target_behavior" => rule.target_behavior = value.to_string(),
                "positive_proof" => rule.positive_proof = value.to_string(),
                "negative_proof" => rule.negative_proof = value.to_string(),
                other => {
                    return Err(format!(
                        "unknown consumer key {other} on line {}",
                        index + 1
                    ));
                }
            },
            Section::Occurrence(row) => match key.trim() {
                "path" => row.path = value.to_string(),
                "anchor" => row.anchor = value.to_string(),
                "marker_kind" => row.marker_kind = value.to_string(),
                "normalized_marker_hash" => row.normalized_marker_hash = value.to_string(),
                "consumer_id" => row.consumer_id = value.to_string(),
                other => {
                    return Err(format!(
                        "unknown occurrence key {other} on line {}",
                        index + 1
                    ));
                }
            },
        }
    }
    if let Some(section) = current {
        finish(section, &mut rules, &mut occurrences)?;
    }
    if rules.is_empty() {
        return Err("consumer inventory is empty".to_string());
    }
    let mut ids = BTreeSet::new();
    for rule in &rules {
        if !ids.insert(rule.id.as_str()) {
            return Err(format!("duplicate consumer inventory id {}", rule.id));
        }
    }
    let mut occurrence_ids = BTreeSet::new();
    for row in &occurrences {
        if !occurrence_ids.insert(row.identity()) {
            return Err(format!(
                "duplicate reviewed occurrence identity {}",
                row.identity()
            ));
        }
    }
    Ok((rules, occurrences))
}

fn parse_occurrence_inventory(text: &str) -> Result<Vec<OccurrenceRule>, String> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|err| format!("parse {OCCURRENCE_INVENTORY}: {err}"))?;
    let rows = value
        .as_array()
        .ok_or_else(|| format!("{OCCURRENCE_INVENTORY} must contain a JSON array"))?;
    let mut parsed = Vec::new();
    let mut identities = BTreeSet::new();
    for (index, value) in rows.iter().enumerate() {
        let fields = value.as_array().ok_or_else(|| {
            format!("{OCCURRENCE_INVENTORY} row {index} must be a five-string array")
        })?;
        if fields.len() != 5 {
            return Err(format!(
                "{OCCURRENCE_INVENTORY} row {index} must contain five strings"
            ));
        }
        let get = |field: usize| {
            fields
                .get(field)
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
                .ok_or_else(|| {
                    format!("{OCCURRENCE_INVENTORY} row {index} field {field} must be a string")
                })
        };
        let row = validate_occurrence_rule(OccurrenceRule {
            path: get(0)?,
            anchor: get(1)?,
            marker_kind: get(2)?,
            normalized_marker_hash: get(3)?,
            consumer_id: get(4)?,
        })?;
        if !identities.insert(row.identity()) {
            return Err(format!(
                "duplicate reviewed occurrence identity {}",
                row.identity()
            ));
        }
        parsed.push(row);
    }
    Ok(parsed)
}

fn validate_occurrence_rule(row: OccurrenceRule) -> Result<OccurrenceRule, String> {
    if row.path.is_empty()
        || row.anchor.is_empty()
        || row.marker_kind.is_empty()
        || row.normalized_marker_hash.is_empty()
        || row.consumer_id.is_empty()
    {
        return Err("occurrence inventory row has empty required fields".to_string());
    }
    if ![
        "singleton_path",
        "identifier",
        "phrase_active_goal",
        "phrase_current_goal",
    ]
    .contains(&row.marker_kind.as_str())
    {
        return Err(format!(
            "unknown occurrence marker kind {}",
            row.marker_kind
        ));
    }
    if row.normalized_marker_hash.len() != 64
        || !row
            .normalized_marker_hash
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(format!(
            "occurrence {} must use a full SHA-256 marker hash",
            row.path
        ));
    }
    Ok(row)
}

fn validate_rule(rule: Rule) -> Result<Rule, String> {
    let required = [
        &rule.id,
        &rule.selector,
        &rule.classification,
        &rule.owner,
        &rule.dependent_issue,
        &rule.compatibility_period,
        &rule.current_behavior,
        &rule.fields,
        &rule.authority,
        &rule.target_behavior,
        &rule.positive_proof,
        &rule.negative_proof,
    ];
    if required.iter().any(|value| value.is_empty()) {
        return Err(format!(
            "inventory rule {} has empty required fields",
            rule.id
        ));
    }
    if !CLASSIFICATIONS.contains(&rule.classification.as_str()) {
        return Err(format!(
            "inventory rule {} has unknown classification {}",
            rule.id, rule.classification
        ));
    }
    Ok(rule)
}

fn semantic_text(
    rows: &[AuditRow],
    unclassified: &[String],
    unused: &[String],
    stale_occurrences: &[String],
    contradictions: &[String],
) -> String {
    let mut text = String::new();
    for row in rows {
        let rule = &row.rule;
        text.push_str(&format!(
            "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}\n",
            row.identity,
            rule.id,
            rule.classification,
            rule.owner,
            rule.dependent_issue,
            rule.compatibility_period,
            rule.current_behavior,
            rule.fields,
            rule.authority,
            rule.target_behavior,
            rule.positive_proof,
            rule.negative_proof
        ));
    }
    for value in unclassified {
        text.push_str(&format!("unclassified|{value}\n"));
    }
    for value in unused {
        text.push_str(&format!("unused|{value}\n"));
    }
    for value in stale_occurrences {
        text.push_str(&format!("stale_occurrence|{value}\n"));
    }
    for value in contradictions {
        text.push_str(&format!("contradiction|{value}\n"));
    }
    text
}

fn render_json(audit: &Audit) -> Result<String, String> {
    let rows: Vec<_> = audit.rows.iter().map(|row| serde_json::json!({
        "identity": row.identity, "path": row.path, "anchor": row.anchor,
        "marker_kind": row.marker_kind, "normalized_marker_hash": row.normalized_marker_hash,
        "inventory_id": row.rule.id, "classification": row.rule.classification,
        "owner": row.rule.owner, "dependent_issue": row.rule.dependent_issue,
        "compatibility_period": row.rule.compatibility_period, "current_behavior": row.rule.current_behavior,
        "fields_consumed": row.rule.fields, "authority_effect": row.rule.authority,
        "target_behavior": row.rule.target_behavior, "positive_proof": row.rule.positive_proof,
        "negative_proof": row.rule.negative_proof
    })).collect();
    let unclassified_occurrences: Vec<_> = audit
        .unclassified_occurrences
        .iter()
        .map(|row| {
            serde_json::json!({
                "path": row.path, "anchor": row.anchor, "marker_kind": row.marker_kind,
                "normalized_marker_hash": row.normalized_marker_hash
            })
        })
        .collect();
    serde_json::to_string_pretty(&serde_json::json!({
        "schema_version":"0.2", "semantic_digest":audit.semantic_digest,
        "migration_ready": audit.unclassified.is_empty() && audit.contradictions.is_empty() && audit.framework_blockers.is_empty(),
        "consumers":rows, "unclassified_discoveries":audit.unclassified,
        "unclassified_occurrences": unclassified_occurrences,
        "unused_inventory_rows":audit.unused_rules, "contradictions":audit.contradictions,
        "framework_blockers": audit.framework_blockers,
        "occurrence_inventory_ready": audit.occurrence_inventory_ready,
        "stale_occurrence_rows": audit.stale_occurrence_rows,
        "blockers": audit.unclassified.len() + audit.contradictions.len() + audit.framework_blockers.len(),
        "non_claims":["live_work_selection","mutation_authority","campaign_priority","github_state_freshness"]
    }))
    .map(|text| text + "\n")
    .map_err(|err| format!("serialize authority audit JSON: {err}"))
}

fn render_markdown(audit: &Audit) -> String {
    let ready = audit.unclassified.is_empty()
        && audit.contradictions.is_empty()
        && audit.framework_blockers.is_empty();
    let mut out = format!(
        "# Active-goal authority audit\n\nSchema: `0.2`\n\nSemantic digest: `{}`\n\nOccurrence inventory ready: `{}`\n\nMigration ready: `{ready}`\n\nThis report inventories tracked singleton consumers. It does not select work, authorize mutation, rank campaigns, or read live GitHub state.\n\n## Consumers\n\n| Identity | Classification | Owner | Follow-up | Authority effect |\n| --- | --- | --- | --- | --- |\n",
        audit.semantic_digest, audit.occurrence_inventory_ready
    );
    for row in &audit.rows {
        let rule = &row.rule;
        out.push_str(&format!(
            "| `{}` | `{}` | `{}` | `{}` | {} |\n",
            row.identity, rule.classification, rule.owner, rule.dependent_issue, rule.authority
        ));
    }
    out.push_str("\n## Blockers\n\n");
    if ready {
        out.push_str("None.\n");
    }
    for value in &audit.unclassified {
        out.push_str(&format!("- unclassified: `{value}`\n"));
    }
    for value in &audit.contradictions {
        out.push_str(&format!("- contradiction: {value}\n"));
    }
    for value in &audit.framework_blockers {
        out.push_str(&format!("- framework: {value}\n"));
    }
    for value in &audit.stale_occurrence_rows {
        out.push_str(&format!("- stale occurrence row: `{value}`\n"));
    }
    out.push_str("\n## Unused inventory rows\n\n");
    if audit.unused_rules.is_empty() {
        out.push_str("None.\n");
    } else {
        for value in &audit.unused_rules {
            out.push_str(&format!("- `{value}`\n"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{
        audit_root, discover, issue_contract_rows, parse_inventory, parse_occurrence_inventory,
        parse_tracked_paths, run_at,
    };
    use std::fs;
    use std::path::Path;

    #[test]
    fn nul_delimited_tracked_paths_preserve_newlines() -> Result<(), String> {
        let paths = parse_tracked_paths(b"plain.rs\0line\nbreak.rs\0")?;
        if paths != ["plain.rs", "line\nbreak.rs"] {
            return Err(format!("tracked paths changed identity: {paths:?}"));
        }
        Ok(())
    }

    #[test]
    fn tracked_paths_reject_non_utf8_identity() -> Result<(), String> {
        if parse_tracked_paths(b"valid.rs\0invalid-\xff.rs\0").is_ok() {
            return Err("non-UTF-8 tracked path was accepted".to_string());
        }
        Ok(())
    }

    #[test]
    fn fallback_paths_reject_non_utf8_identity() -> Result<(), String> {
        #[cfg(unix)]
        let path = {
            use std::ffi::OsString;
            use std::os::unix::ffi::OsStringExt;
            std::path::PathBuf::from(OsString::from_vec(vec![0xff]))
        };
        #[cfg(windows)]
        let path = {
            use std::ffi::OsString;
            use std::os::windows::ffi::OsStringExt;
            std::path::PathBuf::from(OsString::from_wide(&[0xd800]))
        };
        if super::normalize_path(&path).is_ok() {
            return Err("non-UTF-8 fallback path was accepted".to_string());
        }
        Ok(())
    }

    #[test]
    fn repository_inventory_is_complete_and_deterministic() -> Result<(), String> {
        let first = audit_root(Path::new("../"))?;
        if !first.unclassified.is_empty() {
            return Err(format!("unclassified: {:?}", first.unclassified));
        }
        if !first.contradictions.is_empty() {
            return Err(format!("contradictions: {:?}", first.contradictions));
        }
        if !first.occurrence_inventory_ready {
            return Err("reviewed occurrence inventory is not ready".to_string());
        }
        if first.framework_blockers.len() != 1
            || !first.framework_blockers[0].starts_with("issue_snapshots_not_proven:")
        {
            return Err(format!(
                "occurrence review removed or added the wrong blockers: {:?}",
                first.framework_blockers
            ));
        }
        let fixture = Path::new("../fixtures/active-goal-authority-audit/historical-reference");
        let fixture_first = audit_root(fixture)?;
        let fixture_second = audit_root(fixture)?;
        if fixture_first.semantic_digest != fixture_second.semantic_digest {
            return Err("equivalent inputs changed semantic digest".to_string());
        }
        Ok(())
    }

    #[test]
    fn ignored_and_untracked_residue_does_not_change_digest() -> Result<(), String> {
        let root = Path::new("../");
        let residue = root.join(format!(
            ".active-goal-audit-residue-{}/active_goal_reader.txt",
            std::process::id()
        ));
        let parent = residue
            .parent()
            .ok_or_else(|| "residue path has no parent".to_string())?;
        fs::create_dir_all(parent).map_err(|err| format!("create residue directory: {err}"))?;
        fs::write(&residue, ".ripr/goals/active.toml ready = true")
            .map_err(|err| format!("write residue: {err}"))?;
        let discovered = discover(root)?;
        if let Err(err) = fs::remove_dir_all(parent)
            && err.kind() != std::io::ErrorKind::NotFound
        {
            return Err(format!("remove residue: {err}"));
        }
        if discovered
            .occurrences
            .iter()
            .any(|occurrence| occurrence.path.contains(".active-goal-audit-residue-"))
        {
            return Err("untracked residue entered repository discovery".to_string());
        }
        Ok(())
    }

    #[test]
    fn hidden_singleton_and_legacy_ready_block_migration() -> Result<(), String> {
        let audit = audit_root(Path::new(
            "../fixtures/active-goal-authority-audit/hidden-singleton",
        ))?;
        if !audit
            .unclassified
            .iter()
            .any(|identity| identity.starts_with("hidden_reader.rs|"))
        {
            return Err("hidden_reader.rs did not block migration".to_string());
        }
        for covered in ["docs/covered.md|", "xtask/covered.rs|"] {
            if !audit
                .unclassified
                .iter()
                .any(|identity| identity.starts_with(covered))
            {
                return Err(format!("{covered} did not block exact occurrence review"));
            }
        }
        if audit
            .unclassified
            .iter()
            .any(|identity| identity.starts_with(".ripr/goals/active.toml|"))
        {
            return Err("fixture unexpectedly contains active.toml".to_string());
        }
        let source = fs::read_to_string(
            "../fixtures/active-goal-authority-audit/hidden-singleton/hidden_reader.rs",
        )
        .map_err(|err| format!("read hidden reader: {err}"))?;
        if !source.contains("const LEGACY_STATUS: &str = \"ready\";") {
            return Err("legacy ready control is missing or changed".to_string());
        }
        if source
            .replace("\"ready\"", "\"pending\"")
            .contains("const LEGACY_STATUS: &str = \"ready\";")
        {
            return Err("legacy ready control did not discriminate changed status".to_string());
        }
        Ok(())
    }

    #[test]
    fn duplicate_consumer_and_issue_identities_are_rejected() -> Result<(), String> {
        let duplicate_rule =
            include_str!("../../fixtures/active-goal-authority-audit/consumers.toml");
        let first = duplicate_rule
            .split("[[consumer]]")
            .nth(1)
            .ok_or_else(|| "inventory has no consumer".to_string())?;
        let duplicated = format!("[[consumer]]{first}[[consumer]]{first}");
        if parse_inventory(&duplicated).is_ok() {
            return Err("duplicate consumer id was accepted".to_string());
        }
        let duplicate_occurrence = r#"[
          ["a.rs","rust:fn a::hash","identifier","3916015a0f7254a0dda9b7bf3ec5d136e3f8f362572d21b66af199597b08a7de","one"],
          ["a.rs","rust:fn a::hash","identifier","3916015a0f7254a0dda9b7bf3ec5d136e3f8f362572d21b66af199597b08a7de","two"]
        ]"#;
        if parse_occurrence_inventory(duplicate_occurrence).is_ok() {
            return Err("duplicate reviewed occurrence identity was accepted".to_string());
        }
        let root = Path::new("../fixtures/active-goal-authority-audit/duplicate-issues");
        if issue_contract_rows(root).is_ok() {
            return Err("duplicate issue identity was accepted".to_string());
        }
        Ok(())
    }

    #[test]
    fn blocked_command_returns_error_after_writing_both_reports() -> Result<(), String> {
        let fixture = Path::new("../fixtures/active-goal-authority-audit/hidden-singleton");
        let reports = fixture.join("target/ripr/reports");
        let result = run_at(fixture, &reports);
        if result.is_ok() {
            return Err("blocked command unexpectedly returned success".to_string());
        }
        for name in [
            "active-goal-authority-audit.json",
            "active-goal-authority-audit.md",
        ] {
            if !reports.join(name).is_file() {
                return Err(format!("blocked command did not preserve {name}"));
            }
        }
        fs::remove_dir_all(fixture.join("target"))
            .map_err(|err| format!("remove fixture reports: {err}"))?;
        Ok(())
    }

    #[test]
    fn historical_reference_is_classified_without_authority() -> Result<(), String> {
        let audit = audit_root(Path::new(
            "../fixtures/active-goal-authority-audit/historical-reference",
        ))?;
        if !audit.unclassified.is_empty() {
            return Err(format!(
                "historical fixture unclassified: {:?}",
                audit.unclassified
            ));
        }
        if !audit
            .rows
            .iter()
            .all(|row| row.rule.classification == "historical_campaign_evidence")
        {
            return Err("historical fixture gained non-historical authority".to_string());
        }
        Ok(())
    }

    #[test]
    fn repeated_identical_markers_have_distinct_occurrence_anchors() -> Result<(), String> {
        let occurrences = super::discover_text_occurrences(
            "covered.rs",
            "fn reader() { let _ = \"active_goal active_goal\"; }",
        )?;
        let identities = occurrences
            .iter()
            .map(super::Occurrence::identity)
            .collect::<Vec<_>>();
        if identities.len() != 2 || identities[0] == identities[1] {
            return Err(format!(
                "repeated occurrence identities were not distinct: {identities:?}"
            ));
        }
        Ok(())
    }

    #[test]
    fn semantic_anchors_ignore_whitespace_and_preserve_utf8() -> Result<(), String> {
        let compact = super::discover_text_occurrences(
            "docs/a.md",
            "# Shelf 3 — Repo learnings\nThe active goal is historical.",
        )?;
        let spaced = super::discover_text_occurrences(
            "docs/a.md",
            "# Shelf 3 — Repo learnings\nThe   active goal   is historical.",
        )?;
        if compact != spaced {
            return Err(format!(
                "whitespace-only formatting changed occurrence identity: {compact:?} != {spaced:?}"
            ));
        }
        if !compact
            .first()
            .is_some_and(|row| row.anchor.contains("shelf 3 — repo learnings"))
        {
            return Err(format!("UTF-8 heading was not preserved: {compact:?}"));
        }
        Ok(())
    }

    #[test]
    fn rust_module_and_config_keys_are_structural_anchors() -> Result<(), String> {
        let module =
            super::discover_text_occurrences("xtask/src/main.rs", "mod active_goal_authority;")?;
        let config =
            super::discover_text_occurrences(".ripr/example.toml", "active_goal_required = false")?;
        if !module
            .first()
            .is_some_and(|row| row.anchor.starts_with("rust:mod active_goal_authority::"))
        {
            return Err(format!(
                "module declaration used a fallback anchor: {module:?}"
            ));
        }
        if !config
            .first()
            .is_some_and(|row| row.anchor.starts_with("toml:key:active_goal_required::"))
        {
            return Err(format!(
                "config value used a non-structural anchor: {config:?}"
            ));
        }
        Ok(())
    }

    #[test]
    fn stale_occurrences_change_semantic_identity() -> Result<(), String> {
        let first = super::semantic_text(&[], &[], &[], &["one".to_string()], &[]);
        let second = super::semantic_text(&[], &[], &[], &["two".to_string()], &[]);
        if first == second {
            return Err("stale occurrence rows did not affect semantic identity".to_string());
        }
        Ok(())
    }

    #[test]
    fn declarations_after_test_modules_do_not_inherit_test_identity() -> Result<(), String> {
        let occurrences = super::discover_text_occurrences(
            "xtask/src/example.rs",
            "mod tests { fn check() { let _ = \"active_goal\"; } }\nfn production() { let _ = \"active_goal\"; }",
        )?;
        if occurrences.len() != 2 || !occurrences[1].anchor.starts_with("rust:fn production::") {
            return Err(format!(
                "production declaration inherited test-module identity: {occurrences:?}"
            ));
        }
        Ok(())
    }

    #[test]
    fn unreadable_text_inputs_are_reported() -> Result<(), String> {
        let root = std::env::temp_dir().join(format!(
            "ripr-active-goal-unreadable-{}",
            std::process::id()
        ));
        fs::create_dir_all(&root).map_err(|err| format!("create unreadable fixture: {err}"))?;
        fs::write(root.join("broken.rs"), [0xff])
            .map_err(|err| format!("write unreadable fixture: {err}"))?;
        let discovery = discover(&root)?;
        fs::remove_dir_all(&root).map_err(|err| format!("remove unreadable fixture: {err}"))?;
        if discovery.read_failures.len() != 1 || !discovery.read_failures[0].contains("broken.rs") {
            return Err(format!(
                "unreadable tracked text was not reported: {:?}",
                discovery.read_failures
            ));
        }
        Ok(())
    }

    #[test]
    fn covered_doc_and_xtask_occurrences_change_exact_inventory() -> Result<(), String> {
        let doc = super::discover_text_occurrences(
            "docs/covered.md",
            "# Authority\nThe active goal is historical.",
        )?;
        let rust = super::discover_text_occurrences(
            "xtask/src/covered.rs",
            "fn reader() { let _ = \"active_goal\"; }",
        )?;
        if doc.len() != 1 || rust.len() != 1 || doc[0].identity() == rust[0].identity() {
            return Err("covered doc/xtask occurrence identities were not distinct".to_string());
        }
        Ok(())
    }
}
