use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path};

use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};

pub(crate) const SEED_MANIFEST_PATH: &str = "fixtures/python-judged-pr-panel/manifest.json";
pub(crate) const STARTER_JUDGED_PATH: &str = "fixtures/python-judged-pr-panel/starter-judged.json";
pub(crate) const SCALED_JUDGED_PATH: &str = "fixtures/python-judged-pr-panel/scaled-judged.json";

const DIFF_ROOT_SEEDS: &str = "fixtures/python-judged-pr-panel/diffs";
const DIFF_ROOT_SWEEP: &str = "fixtures/python-eval-sweep/diffs";

const RERUN_COMMAND: &str = "cargo xtask python-judged-panel check";
const REQUIRED_DIRECTIONS: [&str; 3] = ["should_gap", "should_stay_quiet", "should_limit"];

const VALID_STATIC_LIMIT_KINDS: [&str; 10] = [
    "decorator_indirection",
    "dynamic_dispatch",
    "metaprogramming",
    "missing_import_graph",
    "mocked_module",
    "opaque_custom_assertion_helper",
    "property_based_test",
    "unresolved_pytest_fixture",
    "unsupported_syntax",
    "timeout",
];

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PythonJudgedPanelManifest {
    schema_version: String,
    kind: String,
    spec: String,
    tier: String,
    description: String,
    #[serde(default)]
    measurement_summary: Nullable<PythonJudgedPanelMeasurementSummary>,
    limits: Vec<String>,
    items: Vec<PythonJudgedPanelItem>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub(crate) struct PythonJudgedPanelMeasurementSummary {
    items_judged: u64,
    false_exposed_count: u64,
    false_actionable_count: u64,
    #[serde(default)]
    note: Nullable<String>,
    #[serde(default)]
    judged_against: Nullable<String>,
    #[serde(default)]
    updated: Nullable<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PythonJudgedPanelItem {
    id: String,
    repo: String,
    #[serde(default)]
    base: Nullable<String>,
    #[serde(default)]
    head: Nullable<String>,
    diff_path: String,
    shape: Vec<String>,
    expected_direction: String,
    anchor: PythonJudgedPanelAnchor,
    #[serde(default)]
    expected_classification: Nullable<String>,
    #[serde(default)]
    expected_static_limit_kind: Nullable<String>,
    #[serde(default)]
    actual_classification: Nullable<String>,
    #[serde(default)]
    actual_oracle_alignment: Nullable<String>,
    labels: PythonJudgedPanelLabels,
    #[serde(default)]
    judgment_source: Nullable<String>,
    #[serde(default)]
    judged_at: Nullable<String>,
    #[serde(default)]
    judged_by: Nullable<String>,
    authority_boundary: String,
    repair_packet_ready: bool,
    #[serde(default)]
    must_not_claim: Nullable<Vec<String>>,
    reason: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PythonJudgedPanelAnchor {
    #[serde(default)]
    file: Nullable<String>,
    #[serde(default)]
    line: Nullable<u64>,
    owner: String,
    boundary: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PythonJudgedPanelLabels {
    #[serde(default)]
    top_card_useful: Nullable<bool>,
    #[serde(default)]
    false_actionable: Nullable<bool>,
    #[serde(default)]
    false_exposed: Nullable<bool>,
    #[serde(default)]
    verify_command_valid: Nullable<bool>,
    #[serde(default)]
    suggested_location_valid: Nullable<bool>,
    #[serde(default)]
    packet_boundaries_safe: Nullable<bool>,
    #[serde(default)]
    limitation_quality: Nullable<String>,
}

impl PythonJudgedPanelLabels {
    fn all_explicitly_null(&self) -> bool {
        self.top_card_useful.is_null()
            && self.false_actionable.is_null()
            && self.false_exposed.is_null()
            && self.verify_command_valid.is_null()
            && self.suggested_location_valid.is_null()
            && self.packet_boundaries_safe.is_null()
            && self.limitation_quality.is_null()
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) enum Nullable<T> {
    #[default]
    Missing,
    Null,
    Value(T),
}

impl<T> Nullable<T> {
    pub(crate) fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    pub(crate) fn is_missing(&self) -> bool {
        matches!(self, Self::Missing)
    }

    pub(crate) fn value(&self) -> Option<&T> {
        match self {
            Self::Value(value) => Some(value),
            Self::Missing | Self::Null => None,
        }
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Nullable<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Option::<T>::deserialize(deserializer).map(|value| match value {
            Some(value) => Self::Value(value),
            None => Self::Null,
        })
    }
}

struct StrictJson(serde_json::Value);

impl<'de> Deserialize<'de> for StrictJson {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(StrictJsonVisitor)
    }
}

struct StrictJsonVisitor;

impl<'de> Visitor<'de> for StrictJsonVisitor {
    type Value = StrictJson;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("JSON without duplicate object keys")
    }

    fn visit_bool<E: de::Error>(self, value: bool) -> Result<Self::Value, E> {
        Ok(StrictJson(value.into()))
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Self::Value, E> {
        Ok(StrictJson(value.into()))
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
        Ok(StrictJson(value.into()))
    }

    fn visit_f64<E: de::Error>(self, value: f64) -> Result<Self::Value, E> {
        serde_json::Number::from_f64(value)
            .map(serde_json::Value::Number)
            .map(StrictJson)
            .ok_or_else(|| E::custom("non-finite JSON number"))
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
        Ok(StrictJson(value.into()))
    }

    fn visit_string<E: de::Error>(self, value: String) -> Result<Self::Value, E> {
        Ok(StrictJson(value.into()))
    }

    fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(StrictJson(serde_json::Value::Null))
    }

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(StrictJson(serde_json::Value::Null))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Self::Value, A::Error> {
        let mut values = Vec::new();
        while let Some(StrictJson(value)) = sequence.next_element()? {
            values.push(value);
        }
        Ok(StrictJson(serde_json::Value::Array(values)))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut values = serde_json::Map::new();
        while let Some(key) = map.next_key::<String>()? {
            let StrictJson(value) = map.next_value()?;
            if values.insert(key.clone(), value).is_some() {
                return Err(de::Error::custom(format!("duplicate object key `{key}`")));
            }
        }
        Ok(StrictJson(serde_json::Value::Object(values)))
    }
}

fn parse_json_without_duplicate_keys(body: &str) -> Result<serde_json::Value, serde_json::Error> {
    let mut deserializer = serde_json::Deserializer::from_str(body);
    let StrictJson(value) = StrictJson::deserialize(&mut deserializer)?;
    deserializer.end()?;
    Ok(value)
}

pub(crate) fn run(args: &[String]) -> Result<(), String> {
    match args {
        [subcommand] if subcommand == "check" => {
            let (seed, starter, scaled) = check_all(Path::new("."))?;
            println!(
                "Python judged panel manifests valid: seed={} items={} | starter={} items={} | scaled={} items={}",
                SEED_MANIFEST_PATH,
                seed.items.len(),
                STARTER_JUDGED_PATH,
                starter.items.len(),
                SCALED_JUDGED_PATH,
                scaled.items.len()
            );
            Ok(())
        }
        [] => Err(format!(
            "python-judged-panel requires `check`\nrerun: {RERUN_COMMAND}"
        )),
        _ => Err(format!(
            "unknown python-judged-panel arguments `{}`\nrerun: {RERUN_COMMAND}",
            args.join(" ")
        )),
    }
}

pub(crate) fn check_canonical() -> Result<(), String> {
    check_all(Path::new(".")).map(|_| ())
}

pub(crate) fn check_all(
    root: &Path,
) -> Result<
    (
        PythonJudgedPanelManifest,
        PythonJudgedPanelManifest,
        PythonJudgedPanelManifest,
    ),
    String,
> {
    let seed = load_and_validate_at(root, Path::new(SEED_MANIFEST_PATH))?;
    let starter = load_and_validate_at(root, Path::new(STARTER_JUDGED_PATH))?;
    let scaled = load_and_validate_at(root, Path::new(SCALED_JUDGED_PATH))?;
    Ok((seed, starter, scaled))
}

pub(crate) fn load_and_validate_at(
    root: &Path,
    manifest_path: &Path,
) -> Result<PythonJudgedPanelManifest, String> {
    let display = normalize_path(manifest_path);
    let body = fs::read_to_string(root.join(manifest_path))
        .map_err(|error| format!("read Python judged panel manifest `{display}`: {error}"))?;
    let value = parse_json_without_duplicate_keys(&body)
        .map_err(|error| format!("parse Python judged panel manifest `{display}`: {error}"))?;
    let manifest: PythonJudgedPanelManifest = serde_json::from_value(value)
        .map_err(|error| format!("parse Python judged panel manifest `{display}`: {error}"))?;
    let mut violations = validate_manifest(root, &manifest);
    violations.sort();
    violations.dedup();
    if violations.is_empty() {
        Ok(manifest)
    } else {
        Err(format!(
            "Python judged panel manifest `{display}` has {} semantic violation(s):\n- {}\nrerun: {RERUN_COMMAND}",
            violations.len(),
            violations.join("\n- ")
        ))
    }
}

fn validate_manifest(root: &Path, manifest: &PythonJudgedPanelManifest) -> Vec<String> {
    let mut violations = Vec::new();
    require_equal(
        &mut violations,
        "manifest.schema_version",
        &manifest.schema_version,
        "0.1",
    );
    require_equal(
        &mut violations,
        "manifest.kind",
        &manifest.kind,
        "python_judged_pr_panel_manifest",
    );
    require_equal(
        &mut violations,
        "manifest.spec",
        &manifest.spec,
        "RIPR-SPEC-0092",
    );
    require_equal(&mut violations, "manifest.tier", &manifest.tier, "B");
    require_non_empty(
        &mut violations,
        "manifest.description",
        &manifest.description,
    );
    if manifest.limits.is_empty() || manifest.limits.iter().any(|limit| limit.trim().is_empty()) {
        violations.push("manifest.limits: require non-empty seed non-claims".to_string());
    }
    if manifest.items.is_empty() {
        violations.push("manifest.items: selected denominator must not be empty".to_string());
    }

    let mut ids = BTreeSet::new();
    let mut selected_counts = BTreeMap::<&str, usize>::new();
    let mut false_actionable_total = 0_u64;
    let mut false_exposed_total = 0_u64;
    let mut judged_items_total = 0_u64;

    for (index, item) in manifest.items.iter().enumerate() {
        let subject = if item.id.trim().is_empty() {
            format!("items[{index}]")
        } else {
            format!("item `{}`", item.id)
        };
        if item.id.trim().is_empty() {
            violations.push(format!("{subject}.id: must not be blank"));
        } else if !ids.insert(item.id.as_str()) {
            violations.push(format!("{subject}.id: duplicate selected item identity"));
        }
        if !REQUIRED_DIRECTIONS.contains(&item.expected_direction.as_str()) {
            violations.push(format!(
                "{subject}.expected_direction: unknown direction `{}`",
                item.expected_direction
            ));
        } else {
            *selected_counts
                .entry(item.expected_direction.as_str())
                .or_default() += 1;
        }
        validate_item(root, item, &subject, &mut violations);

        if matches!(item.labels.false_actionable, Nullable::Value(true)) {
            false_actionable_total += 1;
        }
        if matches!(item.labels.false_exposed, Nullable::Value(true)) {
            false_exposed_total += 1;
        }
        if item.judgment_source.value().is_some() {
            judged_items_total += 1;
        }
    }

    for direction in REQUIRED_DIRECTIONS {
        if selected_counts.get(direction).copied().unwrap_or(0) == 0 {
            violations.push(format!(
                "manifest.items: selected denominator is missing `{direction}`"
            ));
        }
    }

    if let Some(summary) = manifest.measurement_summary.value() {
        validate_measurement_summary(
            summary,
            judged_items_total,
            false_exposed_total,
            false_actionable_total,
            &mut violations,
        );
    }

    violations
}

fn validate_measurement_summary(
    summary: &PythonJudgedPanelMeasurementSummary,
    _judged_total: u64,
    false_exposed_total: u64,
    false_actionable_total: u64,
    violations: &mut Vec<String>,
) {
    if summary.false_exposed_count != false_exposed_total {
        violations.push(format!(
            "manifest.measurement_summary.false_exposed_count: expected {false_exposed_total}, found {}",
            summary.false_exposed_count
        ));
    }
    if summary.false_actionable_count != false_actionable_total {
        violations.push(format!(
            "manifest.measurement_summary.false_actionable_count: expected {false_actionable_total}, found {}",
            summary.false_actionable_count
        ));
    }
    if summary.items_judged == 0 {
        violations
            .push("manifest.measurement_summary.items_judged: must be greater than 0".to_string());
    }
    if let Some(note) = summary.note.value()
        && note.trim().is_empty()
    {
        violations.push("manifest.measurement_summary.note: must not be blank".to_string());
    }
    if let Some(judged_against) = summary.judged_against.value()
        && judged_against.trim().is_empty()
    {
        violations
            .push("manifest.measurement_summary.judged_against: must not be blank".to_string());
    }
    if let Some(updated) = summary.updated.value()
        && updated.trim().is_empty()
    {
        violations.push("manifest.measurement_summary.updated: must not be blank".to_string());
    }
}

fn validate_item(
    root: &Path,
    item: &PythonJudgedPanelItem,
    subject: &str,
    violations: &mut Vec<String>,
) {
    require_non_empty(violations, &format!("{subject}.repo"), &item.repo);
    require_non_empty(violations, &format!("{subject}.reason"), &item.reason);
    require_equal(
        violations,
        &format!("{subject}.authority_boundary"),
        &item.authority_boundary,
        "review_advisory_only",
    );
    if item.repair_packet_ready {
        violations.push(format!(
            "{subject}.repair_packet_ready: must remain false as non-productization guard"
        ));
    }
    if item.shape.is_empty() || item.shape.iter().any(|shape| shape.trim().is_empty()) {
        violations.push(format!(
            "{subject}.shape: require non-empty shape classifications"
        ));
    }

    if let Some(must_not_claim) = item.must_not_claim.value()
        && (must_not_claim.is_empty() || must_not_claim.iter().any(|c| c.trim().is_empty()))
    {
        violations.push(format!(
            "{subject}.must_not_claim: entries must not be blank"
        ));
    }

    validate_direction_and_classification(item, subject, violations);
    validate_labels_and_judgment(item, subject, violations);
    validate_anchor_and_diff(root, item, subject, violations);
}

fn validate_direction_and_classification(
    item: &PythonJudgedPanelItem,
    subject: &str,
    violations: &mut Vec<String>,
) {
    match item.expected_direction.as_str() {
        "should_gap" => {
            match item.expected_classification.value().map(String::as_str) {
                Some("weakly_exposed") | Some("reachable_unrevealed") | Some("no_static_path") => {}
                Some(other) => violations.push(format!(
                    "{subject}.expected_classification: `should_gap` requires `weakly_exposed`, `reachable_unrevealed`, or `no_static_path`, found `{other}`"
                )),
                None => violations.push(format!(
                    "{subject}.expected_classification: `should_gap` requires a non-null classification"
                )),
            }
            if !item.expected_static_limit_kind.is_null()
                && !item.expected_static_limit_kind.is_missing()
            {
                violations.push(format!(
                    "{subject}.expected_static_limit_kind: `should_gap` requires null limit kind"
                ));
            }
        }
        "should_stay_quiet" => {
            match item.expected_classification.value().map(String::as_str) {
                Some("exposed") => {}
                Some(other) => violations.push(format!(
                    "{subject}.expected_classification: `should_stay_quiet` requires `exposed`, found `{other}`"
                )),
                None => violations.push(format!(
                    "{subject}.expected_classification: `should_stay_quiet` requires `exposed`"
                )),
            }
            if !item.expected_static_limit_kind.is_null()
                && !item.expected_static_limit_kind.is_missing()
            {
                violations.push(format!(
                    "{subject}.expected_static_limit_kind: `should_stay_quiet` requires null limit kind"
                ));
            }
        }
        "should_limit" => {
            // expected_classification can be static_unknown or null (e.g. timeout)
            match item.expected_classification.value().map(String::as_str) {
                Some("static_unknown") | None => {}
                Some(other) => violations.push(format!(
                    "{subject}.expected_classification: `should_limit` requires `static_unknown` or null, found `{other}`"
                )),
            }
            if let Some(limit_kind) = item.expected_static_limit_kind.value() {
                if !VALID_STATIC_LIMIT_KINDS.contains(&limit_kind.as_str()) {
                    violations.push(format!(
                        "{subject}.expected_static_limit_kind: unknown static limit kind `{limit_kind}`"
                    ));
                }
            } else if !item.expected_static_limit_kind.is_null() {
                violations.push(format!(
                    "{subject}.expected_static_limit_kind: `should_limit` requires a recognized limit kind or explicit null"
                ));
            }
        }
        _ => {}
    }
}

fn validate_labels_and_judgment(
    item: &PythonJudgedPanelItem,
    subject: &str,
    violations: &mut Vec<String>,
) {
    let fa = matches!(item.labels.false_actionable, Nullable::Value(true));
    let fe = matches!(item.labels.false_exposed, Nullable::Value(true));

    if fa && fe {
        violations.push(format!(
            "{subject}.labels: false_actionable and false_exposed cannot both be true"
        ));
    }

    if item.judgment_source.is_null() || item.judgment_source.is_missing() {
        // Unjudged item (seed manifest)
        if !item.labels.all_explicitly_null() {
            violations.push(format!(
                "{subject}.labels: unjudged seed labels must remain null; null means unjudged, not false/pass"
            ));
        }
        if !item.judged_at.is_null() && !item.judged_at.is_missing() {
            violations.push(format!(
                "{subject}.judged_at: unjudged seed requires null judged_at"
            ));
        }
        if !item.judged_by.is_null() && !item.judged_by.is_missing() {
            violations.push(format!(
                "{subject}.judged_by: unjudged seed requires null judged_by"
            ));
        }
        if let Some(base) = item.base.value()
            && base.trim().is_empty()
        {
            violations.push(format!("{subject}.base: must not be blank"));
        }
        if let Some(head) = item.head.value()
            && head.trim().is_empty()
        {
            violations.push(format!("{subject}.head: must not be blank"));
        }
        if !item.actual_classification.is_null() && !item.actual_classification.is_missing() {
            violations.push(format!(
                "{subject}.actual_classification: unjudged seed requires null"
            ));
        }
        if !item.actual_oracle_alignment.is_null() && !item.actual_oracle_alignment.is_missing() {
            violations.push(format!(
                "{subject}.actual_oracle_alignment: unjudged seed requires null"
            ));
        }
    } else if let Some(source) = item.judgment_source.value() {
        if source != "manual_review" {
            violations.push(format!(
                "{subject}.judgment_source: expected `manual_review`, found `{source}`"
            ));
        }
        if item.judged_at.value().is_none() {
            violations.push(format!(
                "{subject}.judged_at: requires non-empty date string"
            ));
        }
        if item.judged_by.value().is_none() {
            violations.push(format!(
                "{subject}.judged_by: requires non-empty author string"
            ));
        }
        if let Some(base) = item.base.value()
            && base.trim().is_empty()
        {
            violations.push(format!("{subject}.base: must not be blank"));
        }
        if let Some(head) = item.head.value()
            && head.trim().is_empty()
        {
            violations.push(format!("{subject}.head: must not be blank"));
        }
        if let Some(actual_class) = item.actual_classification.value()
            && actual_class.trim().is_empty()
        {
            violations.push(format!(
                "{subject}.actual_classification: must not be blank"
            ));
        }
        if let Some(actual_alignment) = item.actual_oracle_alignment.value()
            && actual_alignment.trim().is_empty()
        {
            violations.push(format!(
                "{subject}.actual_oracle_alignment: must not be blank"
            ));
        }
    }
}

fn validate_anchor_and_diff(
    root: &Path,
    item: &PythonJudgedPanelItem,
    subject: &str,
    violations: &mut Vec<String>,
) {
    require_non_empty(
        violations,
        &format!("{subject}.anchor.owner"),
        &item.anchor.owner,
    );
    require_non_empty(
        violations,
        &format!("{subject}.anchor.boundary"),
        &item.anchor.boundary,
    );

    let diff_path = Path::new(&item.diff_path);
    if normalize_path(diff_path) != item.diff_path || !is_confined_diff_path(diff_path) {
        violations.push(format!(
            "{subject}.diff_path: `{}` must be a relative file under `{DIFF_ROOT_SEEDS}` or `{DIFF_ROOT_SWEEP}` without parent traversal",
            item.diff_path
        ));
        return;
    }

    let canonical_root = match fs::canonicalize(root) {
        Ok(path) => path,
        Err(error) => {
            violations.push(format!(
                "{subject}.diff_path: failed to resolve repository root: {error}"
            ));
            return;
        }
    };
    let full_path = root.join(diff_path);
    if !full_path.is_file() {
        violations.push(format!(
            "{subject}.diff_path: `{}` is missing or is not a file",
            item.diff_path
        ));
        return;
    }

    let diff_root_prefix = if diff_path.starts_with(Path::new(DIFF_ROOT_SEEDS)) {
        DIFF_ROOT_SEEDS
    } else {
        DIFF_ROOT_SWEEP
    };
    let confined_root = match fs::canonicalize(root.join(diff_root_prefix)) {
        Ok(path) => path,
        Err(error) => {
            violations.push(format!(
                "{subject}.diff_path: failed to resolve governed diff root: {error}"
            ));
            return;
        }
    };
    if !confined_root.starts_with(&canonical_root) {
        violations.push(format!(
            "{subject}.diff_path: governed diff root resolves outside the repository root"
        ));
        return;
    }
    let resolved = match fs::canonicalize(&full_path) {
        Ok(path) => path,
        Err(error) => {
            violations.push(format!(
                "{subject}.diff_path: failed to resolve `{}`: {error}",
                item.diff_path
            ));
            return;
        }
    };
    if !resolved.starts_with(&confined_root) {
        violations.push(format!(
            "{subject}.diff_path: `{}` resolves outside `{diff_root_prefix}`",
            item.diff_path
        ));
        return;
    }

    // Anchor file/line verification
    if let (Some(anchor_file), Some(anchor_line)) =
        (item.anchor.file.value(), item.anchor.line.value())
    {
        if *anchor_line == 0 {
            violations.push(format!("{subject}.anchor.line: must be positive"));
        }
        let anchor_path = Path::new(anchor_file);
        if normalize_path(anchor_path) != *anchor_file || !is_confined_relative_path(anchor_path) {
            violations.push(format!(
                "{subject}.anchor.file: `{anchor_file}` must be a normalized repository-relative path"
            ));
            return;
        }

        let body = match fs::read_to_string(&full_path) {
            Ok(body) => body,
            Err(error) => {
                violations.push(format!(
                    "{subject}.diff_path: failed to read `{}`: {error}",
                    item.diff_path
                ));
                return;
            }
        };
        match hunk_covers_anchor_at(&body, anchor_file, *anchor_line) {
            Ok(true) => {}
            Ok(false) => violations.push(format!(
                "{subject}.anchor: `{anchor_file}` line {anchor_line} is not covered by a modifying diff hunk in `{}`",
                item.diff_path
            )),
            Err(error) => violations.push(format!("{subject}.diff_path: {error}")),
        }
    } else {
        // If file or line is null, only timeout limit kind is permissible
        if item.expected_static_limit_kind.value().map(String::as_str) != Some("timeout") {
            violations.push(format!(
                "{subject}.anchor: file and line may only be null for timeout limit kind"
            ));
        }
    }
}

fn is_confined_diff_path(path: &Path) -> bool {
    if !is_confined_relative_path(path) {
        return false;
    }
    (path.starts_with(Path::new(DIFF_ROOT_SEEDS)) || path.starts_with(Path::new(DIFF_ROOT_SWEEP)))
        && path.extension().and_then(|extension| extension.to_str()) == Some("diff")
}

fn is_confined_relative_path(path: &Path) -> bool {
    let raw = normalize_path(path);
    !raw.contains(':')
        && !raw.contains('\\')
        && !raw.contains("//")
        && !raw
            .split('/')
            .any(|segment| segment == "." || segment.is_empty())
        && !path.is_absolute()
        && !path.components().any(|component| {
            matches!(
                component,
                Component::CurDir | Component::ParentDir | Component::RootDir
            )
        })
}

fn hunk_covers_anchor_at(diff: &str, anchor_file: &str, anchor_line: u64) -> Result<bool, String> {
    let expected_target = format!("b/{anchor_file}");
    let mut target_matches = false;
    let mut target_headers = 0_usize;
    let mut anchor_covered = false;
    let mut hunk = None;
    let mut source_header_seen = false;
    let mut section_bound = false;
    for line in diff.lines() {
        if line.starts_with("diff --cc ")
            || line.starts_with("diff --combined ")
            || line.starts_with("GIT binary patch")
            || line.starts_with("Binary files ")
        {
            return Err("combined or binary diffs are unsupported".to_string());
        }
        if line.starts_with("diff --git ") {
            source_header_seen = false;
            section_bound = false;
            target_matches = false;
            finish_hunk(&mut hunk)?;
            continue;
        }
        if line.starts_with("--- ") {
            source_header_seen = true;
            section_bound = false;
            target_matches = false;
            finish_hunk(&mut hunk)?;
            continue;
        }
        if line.starts_with("rename from ")
            || line.starts_with("rename to ")
            || line.starts_with("copy from ")
            || line.starts_with("copy to ")
        {
            return Err("rename and copy diffs are unsupported".to_string());
        }
        if let Some(target) = line.strip_prefix("+++ ") {
            if !source_header_seen {
                return Err("`+++` target is not paired with a preceding `---` source".to_string());
            }
            if target.starts_with('"') || target.contains('\t') {
                return Err("quoted or metadata-bearing `+++` targets are unsupported".to_string());
            }
            target_matches = target.trim() == expected_target;
            if target_matches {
                target_headers += 1;
            }
            source_header_seen = false;
            section_bound = true;
            finish_hunk(&mut hunk)?;
            continue;
        }
        if line.starts_with("@@ ") {
            if !section_bound {
                return Err("hunk is not bound to a `---`/`+++` file section".to_string());
            }
            finish_hunk(&mut hunk)?;
            let parsed = parse_hunk_header(line)?;
            if target_matches
                && anchor_line >= parsed.start_line.saturating_sub(1)
                && anchor_line <= parsed.start_line.saturating_add(parsed.declared_count)
            {
                anchor_covered = true;
            }
            hunk = Some(parsed);
            continue;
        }
        let Some(state) = hunk.as_mut() else {
            if line.trim().is_empty() {
                continue;
            }
            continue;
        };
        if line.starts_with('\\') {
            continue;
        }
        if state.old_remaining == 0 && state.new_remaining == 0 && line.trim().is_empty() {
            finish_hunk(&mut hunk)?;
            continue;
        }
        if line.starts_with('+') {
            if state.new_remaining == 0 {
                return Err("hunk contains more new lines than declared".to_string());
            }
            state.has_added_lines = true;
            state.consume_new()?;
        } else if line.starts_with('-') {
            state.consume_old()?;
        } else if line.starts_with(' ') || line.is_empty() {
            state.consume_old()?;
            state.consume_new()?;
        } else {
            return Err(format!("unsupported line inside unified hunk `{line}`"));
        }
    }
    finish_hunk(&mut hunk)?;
    if target_headers > 1 {
        return Err(format!(
            "target `b/{anchor_file}` occurs in {target_headers} file sections"
        ));
    }
    Ok(anchor_covered)
}

#[derive(Debug)]
struct HunkState {
    start_line: u64,
    declared_count: u64,
    old_remaining: u64,
    new_remaining: u64,
    new_line: u64,
    has_added_lines: bool,
}

impl HunkState {
    fn consume_old(&mut self) -> Result<(), String> {
        self.old_remaining = self
            .old_remaining
            .checked_sub(1)
            .ok_or_else(|| "hunk contains more old lines than declared".to_string())?;
        Ok(())
    }

    fn consume_new(&mut self) -> Result<(), String> {
        self.new_remaining = self
            .new_remaining
            .checked_sub(1)
            .ok_or_else(|| "hunk contains more new lines than declared".to_string())?;
        self.new_line = self
            .new_line
            .checked_add(1)
            .ok_or_else(|| "hunk new-line counter overflowed".to_string())?;
        Ok(())
    }
}

fn finish_hunk(hunk: &mut Option<HunkState>) -> Result<(), String> {
    if let Some(state) = hunk.take()
        && (state.old_remaining != 0 || state.new_remaining != 0)
    {
        return Err(format!(
            "hunk ended before declared extents were consumed (old remaining {}, new remaining {})",
            state.old_remaining, state.new_remaining
        ));
    }
    Ok(())
}

fn parse_hunk_header(line: &str) -> Result<HunkState, String> {
    let mut parts = line.split_whitespace();
    if parts.next() != Some("@@") {
        return Err(format!("malformed unified hunk header `{line}`"));
    }
    let old = parts
        .next()
        .filter(|part| part.starts_with('-'))
        .ok_or_else(|| format!("malformed unified hunk header `{line}`"))?;
    let new = parts
        .next()
        .filter(|part| part.starts_with('+'))
        .ok_or_else(|| format!("malformed unified hunk header `{line}`"))?;
    if parts.next() != Some("@@") {
        return Err(format!("malformed unified hunk header `{line}`"));
    }
    let (_, old_count) = parse_hunk_range(old, '-')?;
    let (new_line, new_count) = parse_hunk_range(new, '+')?;
    Ok(HunkState {
        start_line: new_line,
        declared_count: new_count,
        old_remaining: old_count,
        new_remaining: new_count,
        new_line,
        has_added_lines: false,
    })
}

fn parse_hunk_range(value: &str, prefix: char) -> Result<(u64, u64), String> {
    let value = value
        .strip_prefix(prefix)
        .ok_or_else(|| format!("invalid hunk range `{value}`"))?;
    let mut pieces = value.split(',');
    let start = pieces
        .next()
        .and_then(|part| part.parse().ok())
        .ok_or_else(|| format!("invalid hunk range `{value}`"))?;
    let count = pieces
        .next()
        .map(str::parse)
        .transpose()
        .map_err(|error| format!("invalid hunk range `{value}`: {error}"))?
        .unwrap_or(1);
    if pieces.next().is_some() {
        return Err(format!("invalid hunk range `{value}`"));
    }
    Ok((start, count))
}

fn require_equal(violations: &mut Vec<String>, field: &str, actual: &str, expected: &str) {
    if actual != expected {
        violations.push(format!("{field}: expected `{expected}`, found `{actual}`"));
    }
}

fn require_non_empty(violations: &mut Vec<String>, field: &str, value: &str) {
    if value.trim().is_empty() {
        violations.push(format!("{field}: must not be blank"));
    }
}

fn normalize_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use serde_json::{Value, json};

    use super::{SEED_MANIFEST_PATH, check_all, load_and_validate_at};

    struct TempFixture {
        root: PathBuf,
    }

    impl TempFixture {
        fn new(name: &str) -> Result<Self, String> {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|error| error.to_string())?
                .as_nanos();
            let root = std::env::temp_dir().join(format!(
                "ripr-py-judged-panel-{name}-{}-{unique}",
                std::process::id()
            ));
            fs::create_dir_all(root.join("fixtures/python-judged-pr-panel/diffs"))
                .map_err(|error| error.to_string())?;
            fs::create_dir_all(root.join("fixtures/python-eval-sweep/diffs"))
                .map_err(|error| error.to_string())?;
            Ok(Self { root })
        }

        fn write_manifest(&self, path: &str, value: &Value) -> Result<(), String> {
            let full_path = self.root.join(path);
            if let Some(parent) = full_path.parent() {
                fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            let text = serde_json::to_string_pretty(value).map_err(|error| error.to_string())?;
            fs::write(full_path, text).map_err(|error| error.to_string())
        }

        fn write_diff(&self, name: &str, target_file: &str, code: &str) -> Result<String, String> {
            let relative = format!("fixtures/python-judged-pr-panel/diffs/{name}.diff");
            let body = format!(
                "--- a/{target_file}\n+++ b/{target_file}\n@@ -1,3 +1,3 @@\n def func():\n-    old()\n+    {code}\n     return\n"
            );
            fs::write(self.root.join(&relative), body).map_err(|error| error.to_string())?;
            Ok(relative)
        }
    }

    impl Drop for TempFixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn valid_seed_item(id: &str, direction: &str, diff_path: String, target_file: &str) -> Value {
        let (classification, limit_kind) = match direction {
            "should_gap" => (json!("weakly_exposed"), json!(null)),
            "should_stay_quiet" => (json!("exposed"), json!(null)),
            "should_limit" => (json!("static_unknown"), json!("decorator_indirection")),
            _ => (json!(null), json!(null)),
        };
        json!({
            "id": id,
            "repo": "test-repo",
            "base": null,
            "head": null,
            "diff_path": diff_path,
            "shape": ["pytest_library"],
            "expected_direction": direction,
            "anchor": {
                "file": target_file,
                "line": 3,
                "owner": "func",
                "boundary": "test boundary"
            },
            "expected_classification": classification,
            "expected_static_limit_kind": limit_kind,
            "labels": {
                "top_card_useful": null,
                "false_actionable": null,
                "false_exposed": null,
                "verify_command_valid": null,
                "suggested_location_valid": null,
                "packet_boundaries_safe": null,
                "limitation_quality": null
            },
            "judgment_source": null,
            "judged_at": null,
            "judged_by": null,
            "authority_boundary": "review_advisory_only",
            "repair_packet_ready": false,
            "must_not_claim": ["do not claim pass on null"],
            "reason": "testing valid item"
        })
    }

    fn valid_alternate_seed_manifest(fixture: &TempFixture) -> Result<Value, String> {
        let limit_path = fixture.write_diff("limit", "limit.py", "return 1")?;
        let gap_path = fixture.write_diff("gap", "gap.py", "return 2")?;
        let quiet_path = fixture.write_diff("quiet", "quiet.py", "return 3")?;

        Ok(json!({
            "schema_version": "0.1",
            "kind": "python_judged_pr_panel_manifest",
            "spec": "RIPR-SPEC-0092",
            "tier": "B",
            "description": "Alternate valid seed manifest.",
            "limits": ["seed only"],
            "items": [
                valid_seed_item("item-gap", "should_gap", gap_path, "gap.py"),
                valid_seed_item("item-quiet", "should_stay_quiet", quiet_path, "quiet.py"),
                valid_seed_item("item-limit", "should_limit", limit_path, "limit.py")
            ]
        }))
    }

    #[test]
    fn canonical_manifests_are_semantically_valid() -> Result<(), String> {
        let repository_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .ok_or_else(|| "xtask manifest has no repository parent".to_string())?;
        let (seed, starter, scaled) = check_all(repository_root)?;
        assert_eq!(seed.items.len(), 3);
        assert_eq!(starter.items.len(), 3);
        assert_eq!(scaled.items.len(), 5);
        Ok(())
    }

    #[test]
    fn alternate_valid_seed_passes() -> Result<(), String> {
        let fixture = TempFixture::new("alternate")?;
        let manifest = valid_alternate_seed_manifest(&fixture)?;
        fixture.write_manifest(SEED_MANIFEST_PATH, &manifest)?;
        load_and_validate_at(&fixture.root, Path::new(SEED_MANIFEST_PATH)).map(|_| ())
    }

    #[test]
    fn rejects_duplicate_id_and_missing_direction() -> Result<(), String> {
        let fixture = TempFixture::new("dup-id")?;
        let mut manifest = valid_alternate_seed_manifest(&fixture)?;
        manifest["items"][0]["id"] = manifest["items"][1]["id"].clone();
        if let Some(items) = manifest["items"].as_array_mut() {
            items.pop(); // remove limit item
        }
        fixture.write_manifest(SEED_MANIFEST_PATH, &manifest)?;
        let err = match load_and_validate_at(&fixture.root, Path::new(SEED_MANIFEST_PATH)) {
            Ok(_) => return Err("expected validation to fail for duplicate id".to_string()),
            Err(err) => err,
        };
        assert!(err.contains("duplicate selected item identity"));
        assert!(err.contains("missing `should_limit`"));
        Ok(())
    }

    #[test]
    fn rejects_both_false_actionable_and_false_exposed() -> Result<(), String> {
        let fixture = TempFixture::new("double-error")?;
        let mut manifest = valid_alternate_seed_manifest(&fixture)?;
        manifest["items"][0]["labels"]["false_actionable"] = json!(true);
        manifest["items"][0]["labels"]["false_exposed"] = json!(true);
        fixture.write_manifest(SEED_MANIFEST_PATH, &manifest)?;
        let err = match load_and_validate_at(&fixture.root, Path::new(SEED_MANIFEST_PATH)) {
            Ok(_) => return Err("expected validation to fail for double error".to_string()),
            Err(err) => err,
        };
        assert!(err.contains("false_actionable and false_exposed cannot both be true"));
        Ok(())
    }

    #[test]
    fn rejects_invalid_diff_path_traversal() -> Result<(), String> {
        let fixture = TempFixture::new("traversal")?;
        let mut manifest = valid_alternate_seed_manifest(&fixture)?;
        manifest["items"][0]["diff_path"] =
            json!("fixtures/python-judged-pr-panel/diffs/../secret.diff");
        fixture.write_manifest(SEED_MANIFEST_PATH, &manifest)?;
        let err = match load_and_validate_at(&fixture.root, Path::new(SEED_MANIFEST_PATH)) {
            Ok(_) => return Err("expected validation to fail for diff path traversal".to_string()),
            Err(err) => err,
        };
        assert!(err.contains("without parent traversal"));
        Ok(())
    }

    #[test]
    fn rejects_mismatched_summary_counts() -> Result<(), String> {
        let fixture = TempFixture::new("summary")?;
        let mut manifest = valid_alternate_seed_manifest(&fixture)?;
        manifest["measurement_summary"] = json!({
            "items_judged": 3,
            "false_exposed_count": 5, // actual 0
            "false_actionable_count": 0
        });
        fixture.write_manifest(SEED_MANIFEST_PATH, &manifest)?;
        let err = match load_and_validate_at(&fixture.root, Path::new(SEED_MANIFEST_PATH)) {
            Ok(_) => return Err("expected validation to fail for mismatched summary".to_string()),
            Err(err) => err,
        };
        assert!(
            err.contains("manifest.measurement_summary.false_exposed_count: expected 0, found 5")
        );
        Ok(())
    }

    #[test]
    fn rejects_unjudged_seed_with_non_null_label() -> Result<(), String> {
        let fixture = TempFixture::new("seed-label")?;
        let mut manifest = valid_alternate_seed_manifest(&fixture)?;
        manifest["items"][0]["labels"]["false_actionable"] = json!(false);
        fixture.write_manifest(SEED_MANIFEST_PATH, &manifest)?;
        let err = match load_and_validate_at(&fixture.root, Path::new(SEED_MANIFEST_PATH)) {
            Ok(_) => return Err("expected validation to fail for non-null seed label".to_string()),
            Err(err) => err,
        };
        assert!(err.contains("unjudged seed labels must remain null"));
        Ok(())
    }
}
